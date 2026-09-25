-- Rename current_route_id to current_trip_id on buses
ALTER TABLE buses DROP CONSTRAINT IF EXISTS buses_current_route_id_fkey;
ALTER TABLE buses RENAME COLUMN current_route_id TO current_trip_id;

-- Add FK to trips instead of routes
ALTER TABLE buses ADD CONSTRAINT buses_current_trip_id_fkey
    FOREIGN KEY (current_trip_id) REFERENCES trips(id) ON DELETE SET NULL;

DROP INDEX IF EXISTS idx_buses_current_route_id;
CREATE INDEX IF NOT EXISTS idx_buses_current_trip_id ON buses (current_trip_id);
