-- Reservations remain separate from live rides until activation; all timestamps are UTC.
CREATE TABLE scheduled_rides (
 id uuid PRIMARY KEY, passenger_id uuid NOT NULL REFERENCES users(id), driver_id uuid REFERENCES users(id),
 estimate_id uuid NOT NULL REFERENCES fare_estimates(id), option_id text NOT NULL REFERENCES ride_options(id),
 option_snapshot jsonb NOT NULL, pickup jsonb NOT NULL, destination jsonb NOT NULL, route jsonb NOT NULL,
 fare jsonb NOT NULL, currency text NOT NULL, fare_total_minor bigint NOT NULL CHECK(fare_total_minor>=0),
 scheduled_at timestamp NOT NULL, timezone text NOT NULL, duration_seconds integer NOT NULL CHECK(duration_seconds>0),
 buffer_minutes integer NOT NULL DEFAULT 30 CHECK(buffer_minutes BETWEEN 0 AND 240),
 status text NOT NULL DEFAULT 'OPEN' CHECK(status IN ('OPEN','RESERVED','ACTIVATED','COMPLETED','CANCELLED','EXPIRED')),
 ride_id uuid UNIQUE REFERENCES rides(id) DEFERRABLE INITIALLY DEFERRED, version integer NOT NULL DEFAULT 1,
 cancellation_reason text, next_broadcast_at timestamp, reminder_sent boolean NOT NULL DEFAULT false,
 created_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'), updated_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'),
 UNIQUE(passenger_id,estimate_id),
 CHECK((status IN ('RESERVED','ACTIVATED','COMPLETED'))=(driver_id IS NOT NULL) OR status IN ('CANCELLED','EXPIRED')),
 CHECK(status NOT IN ('ACTIVATED','COMPLETED') OR ride_id IS NOT NULL)
);
CREATE INDEX scheduled_open_pickup ON scheduled_rides(scheduled_at,id) WHERE status='OPEN';
CREATE INDEX scheduled_driver_times ON scheduled_rides(driver_id,scheduled_at) WHERE status IN ('RESERVED','ACTIVATED');
CREATE INDEX scheduled_passenger_times ON scheduled_rides(passenger_id,scheduled_at);
CREATE INDEX scheduled_due_broadcast ON scheduled_rides(next_broadcast_at) WHERE status='OPEN';

-- Durable email jobs are committed alongside booking/lifecycle changes, never sent in a DB transaction.
CREATE TABLE email_outbox (
 sequence_id bigint GENERATED ALWAYS AS IDENTITY UNIQUE,
 id uuid PRIMARY KEY, aggregate_id uuid NOT NULL, event_key text NOT NULL, recipient_id uuid NOT NULL REFERENCES users(id),
 recipient text NOT NULL, subject text NOT NULL, body text NOT NULL,
 transport text NOT NULL DEFAULT 'resend' CHECK(transport IN ('resend','smtp')),
 attempts integer NOT NULL DEFAULT 0, next_attempt_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'),
 lease_token uuid, lease_until timestamp, sent_at timestamp, dead_at timestamp,
 provider_message_id text, last_error text, created_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'),
 UNIQUE(event_key,recipient_id)
);
CREATE INDEX email_outbox_due ON email_outbox(next_attempt_at,created_at) WHERE sent_at IS NULL AND dead_at IS NULL;
CREATE TABLE scheduled_events (
 sequence_id bigint GENERATED ALWAYS AS IDENTITY UNIQUE,
 id uuid PRIMARY KEY, scheduled_ride_id uuid NOT NULL REFERENCES scheduled_rides(id), kind text NOT NULL,
 payload jsonb NOT NULL, broadcast boolean NOT NULL DEFAULT false, driver_ids uuid[] NOT NULL DEFAULT '{}',
 created_at timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'), delivered_at timestamp,
 lease_token uuid, lease_until timestamp
);
CREATE INDEX scheduled_events_due ON scheduled_events(created_at) WHERE delivered_at IS NULL;

-- Choose the next unexpired approach threshold after an initial/reopened broadcast.
CREATE FUNCTION scheduled_next_broadcast(at_time timestamp) RETURNS timestamp LANGUAGE sql STABLE AS $$
 SELECT min(t) FROM (VALUES(at_time-interval '30 minutes'),(at_time-interval '15 minutes'),(at_time-interval '5 minutes'),(at_time)) times(t) WHERE t>(now() AT TIME ZONE 'UTC')
$$;

-- Central eligibility policy is reused by list, broadcast and locked acceptance.
CREATE FUNCTION scheduled_driver_eligible(who uuid, opt jsonb) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM users u JOIN driver_profiles p ON p.user_id=u.id JOIN cars c ON c.id=p.car_id
 WHERE u.id=who AND u.role='Driver' AND u.is_active AND p.status='APPROVED' AND lower(p.track) IN ('professional','both') AND c.is_verified
 AND c.capacity>=COALESCE((opt->>'capacity')::int,1)
 AND COALESCE(opt->'eligible_vehicle_classes','["sedan","suv","van"]'::jsonb) ? CASE lower(c.car_type) WHEN 'saloon' THEN 'sedan' ELSE lower(c.car_type) END)
$$;

CREATE VIEW scheduled_ride_view AS SELECT s.*,
 jsonb_build_object('id',s.id,'passengerId',s.passenger_id,'driverId',s.driver_id,'status',s.status,'version',s.version,
 'pickup',s.pickup,'destination',s.destination,'scheduledAt',s.scheduled_at AT TIME ZONE 'UTC','timezone',s.timezone,
 'durationSeconds',s.duration_seconds,'bufferMinutes',s.buffer_minutes,'rideId',s.ride_id,
 'fareLocked',true,'fare',s.fare,'total',jsonb_build_object('amountMinor',s.fare_total_minor,'currency',s.currency),
 'option',jsonb_build_object('id',s.option_id,'name',s.option_snapshot->>'name','vehicleClass',s.option_snapshot->>'vehicle_class'),
 'createdAt',s.created_at AT TIME ZONE 'UTC','updatedAt',s.updated_at AT TIME ZONE 'UTC','cancellationReason',s.cancellation_reason) AS data
 FROM scheduled_rides s;

-- A cancelled reservation can be reopened without changing its locked fare or journey.
CREATE FUNCTION scheduled_emit(booking uuid, event_name text, announce boolean DEFAULT false, previous_driver uuid DEFAULT NULL) RETURNS void LANGUAGE plpgsql AS $$
DECLARE s scheduled_rides; recipient_row record; event_id uuid:=gen_random_uuid(); v_payload jsonb; key text; human_update text;
BEGIN
 SELECT * INTO STRICT s FROM scheduled_rides WHERE id=booking;
 SELECT data INTO v_payload FROM scheduled_ride_view WHERE id=booking;
 INSERT INTO scheduled_events(id,scheduled_ride_id,kind,payload,broadcast,driver_ids)
 VALUES(event_id,s.id,event_name,v_payload,announce,ARRAY_REMOVE(ARRAY[s.driver_id,previous_driver],NULL));
 key:=s.id::text || '/' || s.version::text || '/' || event_name;
 human_update:=CASE event_name
 WHEN 'scheduled.opened' THEN 'Your request has been saved. We are looking for a driver.'
 WHEN 'scheduled.reserved' THEN 'A driver has accepted your scheduled ride.'
 WHEN 'scheduled.reopened' THEN 'Your booking is open again. We are looking for a driver.'
 WHEN 'scheduled.activated' THEN 'Your driver is preparing for your pickup.'
 WHEN 'scheduled.completed' THEN 'Your journey is complete.'
 WHEN 'scheduled.cancelled' THEN 'Your scheduled ride has been cancelled.'
 WHEN 'scheduled.expired' THEN 'We could not start your scheduled ride. Please check the app for details.'
 WHEN 'scheduled.matching_reminder' THEN 'Your pickup time is approaching. We are still looking for a driver.'
 WHEN 'scheduled.reminder' THEN 'Your scheduled pickup is approaching.'
 WHEN 'scheduled.ride_driver_arrived' THEN 'Your driver has arrived.'
 WHEN 'scheduled.ride_driver_en_route' THEN 'Your driver is on the way.'
 WHEN 'scheduled.ride_in_progress' THEN 'Your journey has started.'
 WHEN 'scheduled.ride_paused' THEN 'Your journey has been paused.'
 ELSE 'There is an update to your scheduled ride.' END;

 FOR recipient_row IN SELECT id,email,first_name FROM users WHERE id=s.passenger_id OR id=s.driver_id OR id=previous_driver LOOP
  INSERT INTO email_outbox(id,aggregate_id,event_key,recipient_id,recipient,subject,body)
  VALUES(gen_random_uuid(),s.id,key,recipient_row.id,recipient_row.email,'K-Ride scheduled ride update',
   format(E'Hello %s,\n\n%s\nPickup: %s\nDestination: %s\nPickup time: %s (%s)\nLocked fare: %s %s.\nBooking: %s\n\nCheck the app for the current status.',
    recipient_row.first_name,human_update,s.pickup->>'name',s.destination->>'name',to_char(s.scheduled_at AT TIME ZONE 'UTC' AT TIME ZONE s.timezone,'DD Mon YYYY, HH24:MI'),s.timezone,s.currency,to_char(s.fare_total_minor::numeric/100,'FM999999999999990.00'),s.id))
  ON CONFLICT(event_key,recipient_id) DO NOTHING;
  INSERT INTO ride_notifications(id,recipient_id,event_type,payload) VALUES(gen_random_uuid(),recipient_row.id,event_name,jsonb_build_object('scheduledRideId',s.id,'status',s.status));
 END LOOP;
END $$;

-- Emit state-change notifications only for business changes, not worker bookkeeping.
CREATE FUNCTION scheduled_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE event_name text;
BEGIN
 IF TG_OP='INSERT' THEN
  PERFORM scheduled_emit(NEW.id,'scheduled.opened',true);
 ELSIF NEW.status IS DISTINCT FROM OLD.status OR NEW.driver_id IS DISTINCT FROM OLD.driver_id OR NEW.scheduled_at IS DISTINCT FROM OLD.scheduled_at THEN
  event_name:=CASE WHEN NEW.status='OPEN' THEN 'scheduled.reopened' WHEN NEW.status='RESERVED' THEN 'scheduled.reserved' WHEN NEW.status='ACTIVATED' THEN 'scheduled.activated' WHEN NEW.status='COMPLETED' THEN 'scheduled.completed' WHEN NEW.status='EXPIRED' THEN 'scheduled.expired' ELSE 'scheduled.cancelled' END;
  PERFORM scheduled_emit(NEW.id,event_name,NEW.status='OPEN',OLD.driver_id);
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER scheduled_changes AFTER INSERT OR UPDATE ON scheduled_rides FOR EACH ROW EXECUTE FUNCTION scheduled_changed();

-- Serialize all commitments for the same users; estimate-based end times protect future bookings.
CREATE FUNCTION scheduled_guard_live_ride() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE linked scheduled_rides; actor uuid; estimated_end timestamp;
BEGIN
 IF NEW.status IN ('COMPLETED','CANCELLED') THEN RETURN NEW; END IF;
 -- A driver cancelling a scheduled live ride must reopen the reservation, not dispatch it as an immediate trip.
 IF TG_OP='UPDATE' AND NEW.status='SEARCHING' AND OLD.driver_id IS NOT NULL AND NEW.driver_id IS NULL THEN
  SELECT * INTO linked FROM scheduled_rides WHERE ride_id=NEW.id AND status='ACTIVATED';
  IF FOUND THEN
   IF OLD.started_at IS NOT NULL THEN RAISE EXCEPTION 'Cannot reopen a journey after it has started' USING ERRCODE='23514'; END IF;
   NEW.status:='CANCELLED'; NEW.cancelled_at:=(now() AT TIME ZONE 'UTC');
   RETURN NEW;
  END IF;
 END IF;
 IF TG_OP='UPDATE' AND NEW.driver_id IS NOT DISTINCT FROM OLD.driver_id AND NEW.passenger_id=OLD.passenger_id THEN RETURN NEW; END IF;
 FOR actor IN SELECT id FROM users WHERE id=NEW.passenger_id OR id=NEW.driver_id ORDER BY id FOR UPDATE LOOP
  estimated_end:=(now() AT TIME ZONE 'UTC')+make_interval(secs=>GREATEST(COALESCE((NEW.route->>'durationSeconds')::int,3600),1));
  IF EXISTS(SELECT 1 FROM scheduled_rides s WHERE (s.passenger_id=actor OR s.driver_id=actor) AND s.status IN ('OPEN','RESERVED','ACTIVATED')
   AND s.ride_id IS DISTINCT FROM NEW.id AND s.scheduled_at<estimated_end+make_interval(mins=>s.buffer_minutes)
   AND s.scheduled_at+make_interval(secs=>s.duration_seconds,mins=>s.buffer_minutes)>(now() AT TIME ZONE 'UTC')) THEN
   RAISE EXCEPTION 'Ride conflicts with a scheduled reservation' USING ERRCODE='23514';
  END IF;
 END LOOP;
 RETURN NEW;
END $$;
CREATE TRIGGER scheduled_live_guard BEFORE INSERT OR UPDATE ON rides FOR EACH ROW EXECUTE FUNCTION scheduled_guard_live_ride();

-- Synchronize linked journeys and generate emails for each material ride status change.
CREATE FUNCTION scheduled_sync_live_ride() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE s scheduled_rides;
BEGIN
 IF TG_OP='INSERT' THEN RETURN NEW; END IF;
 SELECT * INTO s FROM scheduled_rides WHERE ride_id=NEW.id AND status='ACTIVATED';
 IF NOT FOUND THEN RETURN NEW; END IF;
 IF NEW.status='COMPLETED' THEN
  UPDATE scheduled_rides SET status='COMPLETED',version=version+1,updated_at=(now() AT TIME ZONE 'UTC') WHERE id=s.id;
 ELSIF NEW.status='CANCELLED' THEN
  IF NEW.cancellation->>'by'='driver' AND NEW.started_at IS NULL AND s.scheduled_at>(now() AT TIME ZONE 'UTC') THEN
   UPDATE scheduled_rides SET status='OPEN',driver_id=NULL,ride_id=NULL,reminder_sent=false,next_broadcast_at=scheduled_next_broadcast(scheduled_at),version=version+1,updated_at=(now() AT TIME ZONE 'UTC'),cancellation_reason=NEW.cancellation->>'reason' WHERE id=s.id;
  ELSE
   UPDATE scheduled_rides SET status='CANCELLED',version=version+1,updated_at=(now() AT TIME ZONE 'UTC'),cancellation_reason=NEW.cancellation->>'reason' WHERE id=s.id;
  END IF;
 ELSIF NEW.status IS DISTINCT FROM OLD.status THEN
  -- Keep fare locked even when the normal ride lifecycle computes waiting charges.
  UPDATE scheduled_rides SET version=version+1,updated_at=(now() AT TIME ZONE 'UTC') WHERE id=s.id;
  PERFORM scheduled_emit(s.id,'scheduled.ride_' || lower(NEW.status));
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER scheduled_live_sync AFTER UPDATE ON rides FOR EACH ROW EXECUTE FUNCTION scheduled_sync_live_ride();

-- The locked scheduled fare overrides waiting/final amounts on the linked ride.
CREATE FUNCTION scheduled_locked_fare() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE locked bigint;
BEGIN
 SELECT fare_total_minor INTO locked FROM scheduled_rides WHERE ride_id=NEW.id;
 IF FOUND THEN
  NEW.fare_total_minor:=locked; NEW.waiting_charge_minor:=0;
  IF NEW.status='COMPLETED' THEN NEW.final_fare_minor:=locked; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER scheduled_price_lock BEFORE UPDATE ON rides FOR EACH ROW EXECUTE FUNCTION scheduled_locked_fare();
