-- Reverse: restore route columns and drop trip_id from bookings

ALTER TABLE bookings DROP COLUMN IF EXISTS trip_id;
DROP INDEX IF EXISTS idx_bookings_trip_id;

ALTER TABLE routes ADD COLUMN ride_type VARCHAR(50) NOT NULL DEFAULT 'Pickup';
ALTER TABLE routes ADD COLUMN departure_time TIME WITHOUT TIME ZONE NOT NULL DEFAULT '00:00:00';
ALTER TABLE routes ADD COLUMN departure_date DATE NOT NULL DEFAULT CURRENT_DATE;
ALTER TABLE routes ADD COLUMN is_completed BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE routes ADD COLUMN time_completed TIMESTAMP WITHOUT TIME ZONE;

CREATE INDEX IF NOT EXISTS idx_routes_departure_date ON routes (departure_date);
CREATE INDEX IF NOT EXISTS idx_routes_departure_time ON routes (departure_time);
CREATE INDEX IF NOT EXISTS idx_routes_ride_type ON routes (ride_type);
CREATE INDEX IF NOT EXISTS idx_routes_is_completed ON routes (is_completed);
