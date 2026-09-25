ALTER TABLE buses DROP CONSTRAINT IF EXISTS buses_current_trip_id_fkey;
ALTER TABLE buses RENAME COLUMN current_trip_id TO current_route_id;

ALTER TABLE buses ADD CONSTRAINT buses_current_route_id_fkey
    FOREIGN KEY (current_route_id) REFERENCES routes(id) ON DELETE SET NULL;

DROP INDEX IF EXISTS idx_buses_current_trip_id;
CREATE INDEX IF NOT EXISTS idx_buses_current_route_id ON buses (current_route_id);
