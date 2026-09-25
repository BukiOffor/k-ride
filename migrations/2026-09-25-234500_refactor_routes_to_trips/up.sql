-- ==============================================================================
-- Migration: refactor_routes_to_trips
-- Moves scheduling/ride fields from routes into trips, adds trip_id to bookings
-- ==============================================================================

-- 1. Remove trip-specific columns from routes
ALTER TABLE routes DROP COLUMN IF EXISTS ride_type;
ALTER TABLE routes DROP COLUMN IF EXISTS departure_time;
ALTER TABLE routes DROP COLUMN IF EXISTS departure_date;
ALTER TABLE routes DROP COLUMN IF EXISTS is_completed;
ALTER TABLE routes DROP COLUMN IF EXISTS time_completed;

-- Drop now-unused indexes on routes
DROP INDEX IF EXISTS idx_routes_departure_date;
DROP INDEX IF EXISTS idx_routes_departure_time;
DROP INDEX IF EXISTS idx_routes_ride_type;
DROP INDEX IF EXISTS idx_routes_is_completed;

-- 2. Add trip_id to bookings (references the trip, not the route directly for scheduling)
ALTER TABLE bookings ADD COLUMN trip_id UUID REFERENCES trips(id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_bookings_trip_id ON bookings (trip_id);
