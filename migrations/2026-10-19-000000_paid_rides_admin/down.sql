DROP TRIGGER admin_ride_event ON rides;
DROP FUNCTION admin_record_ride_event();
DROP TRIGGER admin_car_documents ON cars;
DROP FUNCTION admin_invalidate_car_documents();
DROP TRIGGER admin_application_normalize ON driver_profiles;
DROP FUNCTION admin_normalize_application();
ALTER TABLE driver_profiles DROP CONSTRAINT admin_application_status;
DROP TRIGGER admin_incident_resolution ON admin_incidents;
DROP FUNCTION admin_notify_incident_resolution();
DROP TABLE ride_notifications;
DROP VIEW admin_incidents_view, admin_rides_view, admin_sos_view, admin_people_view, admin_vehicle_view;
DROP TRIGGER admin_safety_incident ON safety_events;
DROP FUNCTION admin_open_safety_incident();
DROP TABLE admin_events, admin_quality_flags, admin_incident_notes, admin_incidents, admin_review_comments, admin_document_reviews, admin_user_state;
DROP INDEX admin_rides_created, admin_rides_driver, admin_rides_passenger, admin_ratings_ratee;
DROP TABLE trusted_contacts;
ALTER TABLE users
    DROP COLUMN IF EXISTS account_number,
    DROP COLUMN IF EXISTS bank_name;
