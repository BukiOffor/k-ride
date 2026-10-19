CREATE TABLE admin_user_state (
 user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
 restricted boolean NOT NULL DEFAULT false,
 restriction_reason text,
 deactivation_reason text,
 membership_status text CHECK (membership_status IN ('UNVERIFIED','CHECKING','VERIFIED','ACTION_REQUIRED','REJECTED')),
 inspection jsonb,
 changes_requested jsonb NOT NULL DEFAULT '[]',
 updated_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC')
);
CREATE TABLE admin_document_reviews (
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 document_type text NOT NULL,
 status text NOT NULL CHECK (status IN ('VERIFIED','NEEDS_ATTENTION')),
 note text,
 reviewer_id uuid NOT NULL REFERENCES users(id),
 updated_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'),
 PRIMARY KEY(user_id,document_type)
);
CREATE TABLE admin_review_comments (
 id uuid PRIMARY KEY,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 author_id uuid NOT NULL REFERENCES users(id),
 body text NOT NULL,
 visibility text NOT NULL CHECK (visibility IN ('INTERNAL','APPLICANT')),
 document_type text,
 created_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC')
);
CREATE INDEX admin_review_comments_user ON admin_review_comments(user_id,created_at);
CREATE TABLE admin_incidents (
 id uuid PRIMARY KEY,
 safety_event_id uuid UNIQUE REFERENCES safety_events(id),
 ride_id uuid REFERENCES rides(id),
 category text NOT NULL CHECK(category IN ('SAFETY_CONCERN','SOS_ACTIVATION','DRIVER_CONDUCT','PASSENGER_CONDUCT','VEHICLE_ISSUE','PAYMENT_DISPUTE','OTHER')),
 severity text NOT NULL CHECK(severity IN ('LOW','MEDIUM','HIGH','CRITICAL')),
 status text NOT NULL DEFAULT 'OPEN' CHECK(status IN ('OPEN','UNDER_REVIEW','RESOLVED')),
 title text NOT NULL,
 description text NOT NULL,
 participant_ids uuid[] NOT NULL DEFAULT '{}',
 assignee_id uuid REFERENCES users(id),
 opened_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'),
 updated_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'),
 resolved_at timestamp
);
CREATE INDEX admin_incidents_status ON admin_incidents(status,opened_at DESC,id DESC);
CREATE INDEX admin_incidents_ride_open ON admin_incidents(ride_id) WHERE status <> 'RESOLVED';
CREATE TABLE admin_incident_notes (
 id uuid PRIMARY KEY,
 incident_id uuid NOT NULL REFERENCES admin_incidents(id) ON DELETE CASCADE,
 author_id uuid NOT NULL REFERENCES users(id),
 note text NOT NULL,
 created_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC')
);
CREATE TABLE admin_quality_flags (
 id uuid PRIMARY KEY,
 subject_id uuid NOT NULL REFERENCES users(id),
 reason text NOT NULL,
 source text NOT NULL DEFAULT 'manual',
 flagged_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'),
 cleared_at timestamp,
 clear_note text
);
CREATE UNIQUE INDEX admin_quality_one_open ON admin_quality_flags(subject_id) WHERE cleared_at IS NULL;
CREATE TABLE admin_events (
 id uuid PRIMARY KEY,
 actor_id uuid NOT NULL REFERENCES users(id),
 target_id uuid NOT NULL,
 event_type text NOT NULL,
 detail jsonb NOT NULL,
 created_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC')
);
CREATE INDEX admin_events_target ON admin_events(target_id,created_at,id);
CREATE INDEX admin_rides_created ON rides(created_at DESC,id DESC);
CREATE INDEX admin_rides_driver ON rides(driver_id,created_at DESC,id DESC);
CREATE INDEX admin_rides_passenger ON rides(passenger_id,created_at DESC,id DESC);
CREATE INDEX admin_ratings_ratee ON ride_ratings(ratee_id);

CREATE TABLE ride_notifications (
 id uuid PRIMARY KEY,recipient_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 event_type text NOT NULL,payload jsonb NOT NULL,created_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC')
);
CREATE INDEX ride_notifications_recipient ON ride_notifications(recipient_id,id DESC);

-- Keep the existing SOS/report write paths atomic with incident creation.
CREATE FUNCTION admin_open_safety_incident() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO admin_incidents(id,safety_event_id,ride_id,category,severity,title,description,participant_ids,opened_at)
 VALUES(NEW.id,NEW.id,NEW.ride_id,
 CASE WHEN NEW.kind='sos' THEN 'SOS_ACTIVATION' WHEN NEW.event_type IN ('SAFETY_CONCERN','DRIVER_CONDUCT','PASSENGER_CONDUCT','VEHICLE_ISSUE','PAYMENT_DISPUTE','OTHER') THEN NEW.event_type ELSE 'SAFETY_CONCERN' END,
 CASE WHEN NEW.kind='sos' THEN 'CRITICAL' ELSE 'MEDIUM' END,
 CASE WHEN NEW.kind='sos' THEN 'Emergency SOS: ' ELSE 'Safety report: ' END || NEW.event_type,
 COALESCE(NEW.details,''),ARRAY[NEW.reporter_id],NEW.created_at);
 INSERT INTO admin_events(id,actor_id,target_id,event_type,detail,created_at)
 VALUES(gen_random_uuid(),NEW.reporter_id,NEW.id,CASE WHEN NEW.kind='sos' THEN 'sos.raised' ELSE 'incident.opened' END,jsonb_build_object('rideId',NEW.ride_id,'type',NEW.event_type),NEW.created_at);
 IF NEW.kind='sos' THEN
  INSERT INTO ride_notifications(id,recipient_id,event_type,payload)
  SELECT gen_random_uuid(),id,'sos.raised',jsonb_build_object('alertId',NEW.id,'rideId',NEW.ride_id) FROM users WHERE role IN ('Admin','Root') AND is_active;
  UPDATE rides SET sos_status='sent' WHERE id=NEW.ride_id;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER admin_safety_incident AFTER INSERT ON safety_events FOR EACH ROW EXECUTE FUNCTION admin_open_safety_incident();
INSERT INTO admin_incidents(id,safety_event_id,ride_id,category,severity,title,description,participant_ids,opened_at)
SELECT id,id,ride_id,CASE WHEN kind='sos' THEN 'SOS_ACTIVATION' ELSE 'SAFETY_CONCERN' END,
 CASE WHEN kind='sos' THEN 'CRITICAL' ELSE 'MEDIUM' END,event_type,COALESCE(details,''),ARRAY[reporter_id],created_at FROM safety_events;

CREATE TABLE trusted_contacts (
 id uuid PRIMARY KEY,user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 name text NOT NULL,relationship text NOT NULL,phone text NOT NULL,
 created_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC')
);
CREATE INDEX trusted_contacts_user ON trusted_contacts(user_id);

CREATE VIEW admin_vehicle_view AS SELECT id, jsonb_build_object('id',id,'make',brand,'model',model,'colour',color,'plate',license_plate,'seats',capacity,'capacity',capacity,'color',color,'vehicleClass',lower(car_type),'verified',is_verified) AS data FROM cars;
CREATE VIEW admin_people_view AS
WITH stats AS (
 SELECT subject, role, count(*) FILTER (WHERE status='COMPLETED') AS completed FROM (
 SELECT passenger_id AS subject,'User'::text AS role,status FROM rides UNION ALL SELECT driver_id,'Driver',status FROM rides WHERE driver_id IS NOT NULL
 ) r GROUP BY subject,role
), ratings AS (SELECT ratee_id, avg(stars)::float8 AS rating, count(*) FILTER (WHERE stars<=2) AS low FROM ride_ratings GROUP BY ratee_id)
SELECT u.id, u.role, u.is_active, dp.track, dp.status AS verification_status,
 jsonb_build_object('userId',u.id,'displayName',u.first_name || ' ' || u.last_name,'avatarUrl',NULL,
 'email',u.email,'homeArea',u.city,'joinedAt',u.created_at AT TIME ZONE 'UTC','track',dp.track,
 'verificationStatus',upper(COALESCE(dp.status,'DRAFT')),'ninVerified',u.is_identity_verified,
 'membershipStatus',COALESCE(s.membership_status,CASE WHEN u.is_identity_verified THEN 'VERIFIED' ELSE 'UNVERIFIED' END),
 'restricted',COALESCE(s.restricted,false),'trustedContactCount',(SELECT count(*) FROM trusted_contacts tc WHERE tc.user_id=u.id),
 'completedRides',COALESCE(st.completed,0),'rating',rt.rating,'lowRatings',COALESCE(rt.low,0),
 'vehicle',v.data,'inspection',s.inspection,'deactivationReason',s.deactivation_reason,
 'flagged',f.id IS NOT NULL,'accountStatus',CASE WHEN NOT u.is_active THEN 'DEACTIVATED' WHEN f.id IS NOT NULL THEN 'FLAGGED' ELSE 'ACTIVE' END,
 'availability',COALESCE(da.status,'OFFLINE')) AS data
FROM users u LEFT JOIN driver_profiles dp ON dp.user_id=u.id
LEFT JOIN admin_user_state s ON s.user_id=u.id LEFT JOIN admin_vehicle_view v ON v.id=COALESCE(dp.car_id,u.car)
LEFT JOIN stats st ON st.subject=u.id AND st.role=u.role LEFT JOIN ratings rt ON rt.ratee_id=u.id
LEFT JOIN driver_availability da ON da.driver_id=u.id
LEFT JOIN admin_quality_flags f ON f.subject_id=u.id AND f.cleared_at IS NULL;

CREATE VIEW admin_sos_view AS
SELECT se.id,se.ride_id,se.status,se.created_at,
 jsonb_build_object('id',se.id,'rideId',se.ride_id,'rideReference','KR-' || upper(left(replace(se.ride_id::text,'-',''),12)),
 'status',se.status,'type',se.event_type,'raisedAt',se.occurred_at AT TIME ZONE 'UTC','location',se.location,
 'raisedBy',jsonb_build_object('userId',u.id,'role',se.reporter_role,'displayName',u.first_name || ' ' || u.last_name),
 'acknowledgedAt',(SELECT min(created_at) AT TIME ZONE 'UTC' FROM admin_events WHERE target_id=se.id AND event_type='sos.acknowledged'),
 'resolvedAt',(SELECT min(created_at) AT TIME ZONE 'UTC' FROM admin_events WHERE target_id=se.id AND event_type='sos.resolved')) AS data
FROM safety_events se JOIN users u ON u.id=se.reporter_id WHERE se.kind='sos';

CREATE VIEW admin_rides_view AS
SELECT r.id,r.created_at,r.status,r.driver_id,r.passenger_id,
 jsonb_build_object('id',r.id,'reference','KR-' || upper(left(replace(r.id::text,'-',''),12)),
 'status',r.status,'rideType',CASE WHEN COALESCE((r.option_snapshot->>'shared')::boolean,false) THEN 'SHARED' ELSE 'PRIVATE' END,
 'createdAt',r.created_at AT TIME ZONE 'UTC','updatedAt',r.updated_at AT TIME ZONE 'UTC','version',r.version,'startedAt',r.started_at AT TIME ZONE 'UTC','fare',r.fare,'payment',jsonb_build_object('status',r.payment_status,'methodLabel',initcap(r.payment_method)),'option',r.option_snapshot,'rating',r.rating,'cancellation',r.cancellation,'progress',r.tracking->'progress','approach',r.tracking->'approach','timestamps',jsonb_build_object('assignedAt',r.assigned_at AT TIME ZONE 'UTC','arrivedAt',r.arrived_at AT TIME ZONE 'UTC','startedAt',r.started_at AT TIME ZONE 'UTC','completedAt',r.completed_at AT TIME ZONE 'UTC','cancelledAt',r.cancelled_at AT TIME ZONE 'UTC'),
 'pickup',r.pickup,'destination',r.destination,'route',r.route,'track','Professional',
 'etaSeconds',COALESCE((r.tracking->'approach'->>'etaSeconds')::numeric,(r.route->>'durationSeconds')::numeric),
 'driver',CASE WHEN d.id IS NULL THEN NULL ELSE jsonb_build_object('userId',d.id,'displayName',d.data->'displayName','avatarUrl',d.data->'avatarUrl','rating',d.data->'rating','completedRides',d.data->'completedRides','vehicle',d.data->'vehicle') END,
 'passengers',jsonb_build_array(jsonb_build_object('userId',p.id,'displayName',p.data->'displayName','avatarUrl',p.data->'avatarUrl','rating',p.data->'rating','dropoff',r.destination,
 'state',CASE WHEN r.status='COMPLETED' THEN 'DROPPED_OFF' WHEN r.status IN ('IN_PROGRESS','PAUSED') THEN 'PICKED_UP' ELSE 'AWAITING_PICKUP' END)),
 'sos',jsonb_build_object('status',r.sos_status,'alertId',(SELECT id FROM safety_events WHERE ride_id=r.id AND kind='sos' ORDER BY created_at DESC,id DESC LIMIT 1)),
 'driverLocation',CASE WHEN da.lat IS NULL THEN NULL ELSE jsonb_build_object('point',jsonb_build_object('lat',da.lat,'lng',da.lng),'heading',da.heading,'recordedAt',da.recorded_at AT TIME ZONE 'UTC') END,
 'total',jsonb_build_object('amountMinor',COALESCE(r.final_fare_minor,r.fare_total_minor+r.waiting_charge_minor),'currency',r.currency)) AS data
FROM rides r JOIN admin_people_view p ON p.id=r.passenger_id LEFT JOIN admin_people_view d ON d.id=r.driver_id
LEFT JOIN driver_availability da ON da.driver_id=r.driver_id;

CREATE VIEW admin_incidents_view AS
SELECT i.id,i.opened_at,i.status,i.severity,i.category,i.ride_id,
 jsonb_build_object('id',i.id,'reference','INC-' || upper(left(replace(i.id::text,'-',''),12)),
 'rideId',i.ride_id,'rideReference',CASE WHEN i.ride_id IS NOT NULL THEN 'KR-' || upper(left(replace(i.ride_id::text,'-',''),12)) END,
 'category',i.category,'severity',i.severity,'status',i.status,'title',i.title,'description',i.description,
 'openedAt',i.opened_at AT TIME ZONE 'UTC','updatedAt',i.updated_at AT TIME ZONE 'UTC','resolvedAt',i.resolved_at AT TIME ZONE 'UTC',
 'assignedTo',CASE WHEN a.id IS NOT NULL THEN jsonb_build_object('userId',a.id,'displayName',a.first_name || ' ' || a.last_name) END) AS data
FROM admin_incidents i LEFT JOIN users a ON a.id=i.assignee_id;

CREATE FUNCTION admin_notify_incident_resolution() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.status='RESOLVED' AND OLD.status<>'RESOLVED' THEN
  INSERT INTO ride_notifications(id,recipient_id,event_type,payload)
  SELECT gen_random_uuid(),u.id,'incident.resolved',jsonb_build_object('incidentId',NEW.id,'rideId',NEW.ride_id)
  FROM users u WHERE u.id=ANY(NEW.participant_ids) OR u.id IN (SELECT passenger_id FROM rides WHERE id=NEW.ride_id UNION SELECT driver_id FROM rides WHERE id=NEW.ride_id);
  UPDATE safety_events SET status='resolved' WHERE id=NEW.safety_event_id;
  UPDATE rides SET sos_status=CASE WHEN EXISTS(SELECT 1 FROM safety_events WHERE ride_id=NEW.ride_id AND kind='sos' AND status<>'resolved') THEN 'sent' ELSE 'resolved' END WHERE id=NEW.ride_id AND NEW.safety_event_id IS NOT NULL;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER admin_incident_resolution AFTER UPDATE ON admin_incidents FOR EACH ROW EXECUTE FUNCTION admin_notify_incident_resolution();

-- Normalize legacy free-string statuses at the database boundary, including older clients.
UPDATE driver_profiles SET status=CASE
 WHEN upper(replace(status,'_','')) IN ('APPROVED','VERIFIED','ACTIVE') THEN 'APPROVED'
 WHEN upper(replace(status,'_','')) IN ('PENDING','SUBMITTED') THEN 'SUBMITTED'
 WHEN upper(replace(status,'_','')) IN ('UNDERREVIEW','INREVIEW','DOCUMENTREVIEW') THEN 'DOCUMENT_REVIEW'
 WHEN upper(status) IN ('DRAFT','INSPECTION_REQUIRED','INSPECTION_SCHEDULED','FINAL_REVIEW','CHANGES_REQUIRED','REJECTED') THEN upper(status)
 ELSE 'SUBMITTED' END;
ALTER TABLE driver_profiles ADD CONSTRAINT admin_application_status CHECK(status IN ('DRAFT','SUBMITTED','DOCUMENT_REVIEW','INSPECTION_REQUIRED','INSPECTION_SCHEDULED','FINAL_REVIEW','APPROVED','CHANGES_REQUIRED','REJECTED'));
CREATE FUNCTION admin_normalize_application() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 NEW.status=CASE WHEN upper(NEW.status) IN ('PENDING','SUBMITTED') THEN 'SUBMITTED' WHEN upper(NEW.status) IN ('APPROVED','VERIFIED','ACTIVE') THEN 'APPROVED' ELSE upper(NEW.status) END;
 IF TG_OP='UPDATE' THEN
  DELETE FROM admin_document_reviews r WHERE r.user_id=NEW.user_id AND
   ((r.document_type='GOVERNMENT_ID' AND NEW.nin_slip IS DISTINCT FROM OLD.nin_slip) OR
    (r.document_type='PROFILE_PHOTO' AND NEW.profile_photo IS DISTINCT FROM OLD.profile_photo) OR
    (r.document_type='DRIVERS_LICENCE' AND (NEW.licence_photo IS DISTINCT FROM OLD.licence_photo OR NEW.licence_expiry_date IS DISTINCT FROM OLD.licence_expiry_date)) OR
    (r.document_type='GUARANTOR_PHOTO' AND NEW.guarantor_photo IS DISTINCT FROM OLD.guarantor_photo) OR NEW.car_id IS DISTINCT FROM OLD.car_id);
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER admin_application_normalize BEFORE INSERT OR UPDATE ON driver_profiles FOR EACH ROW EXECUTE FUNCTION admin_normalize_application();
CREATE FUNCTION admin_invalidate_car_documents() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 DELETE FROM admin_document_reviews r USING driver_profiles p WHERE r.user_id=p.user_id AND p.car_id=NEW.id AND
 ((r.document_type='VEHICLE_REGISTRATION' AND NEW.vehicle_registration IS DISTINCT FROM OLD.vehicle_registration) OR
 (r.document_type='INSURANCE' AND NEW.insurance_certificate IS DISTINCT FROM OLD.insurance_certificate) OR
 (r.document_type='ROADWORTHINESS_CERTIFICATE' AND NEW.roadworthiness_certificate IS DISTINCT FROM OLD.roadworthiness_certificate) OR
 (r.document_type='VEHICLE_PHOTO' AND NEW.vehicle_photo IS DISTINCT FROM OLD.vehicle_photo));
 RETURN NEW;
END $$;
CREATE TRIGGER admin_car_documents BEFORE UPDATE ON cars FOR EACH ROW EXECUTE FUNCTION admin_invalidate_car_documents();

CREATE FUNCTION admin_record_ride_event() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' OR NEW.status IS DISTINCT FROM OLD.status THEN
  INSERT INTO admin_events(id,actor_id,target_id,event_type,detail,created_at)
  VALUES(gen_random_uuid(),NEW.passenger_id,NEW.id,'ride.status_changed',jsonb_build_object('status',NEW.status,'version',NEW.version),NEW.updated_at);
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER admin_ride_event AFTER INSERT OR UPDATE ON rides FOR EACH ROW EXECUTE FUNCTION admin_record_ride_event();

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS account_number VARCHAR(10),
    ADD COLUMN IF NOT EXISTS bank_name VARCHAR(100);
