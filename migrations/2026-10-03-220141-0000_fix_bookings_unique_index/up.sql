DROP INDEX IF EXISTS idx_bookings_active_bus_seat;
CREATE UNIQUE INDEX IF NOT EXISTS idx_bookings_active_bus_seat ON bookings (bus_id, trip_id, seat_number) 
    WHERE status NOT IN ('Cancelled', 'Revoked');
