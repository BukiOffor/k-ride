CREATE TABLE IF NOT EXISTS trips (
    id UUID PRIMARY KEY,
    route_id UUID NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    bus_id UUID REFERENCES buses(id) ON DELETE SET NULL,
    driver_id UUID REFERENCES users(id) ON DELETE SET NULL,
    departure_time TIMESTAMP WITHOUT TIME ZONE NOT NULL,
    arrival_time TIMESTAMP WITHOUT TIME ZONE NOT NULL,
    actual_departure_time TIMESTAMP WITHOUT TIME ZONE,
    actual_arrival_time TIMESTAMP WITHOUT TIME ZONE,
    ride_type VARCHAR(50) NOT NULL DEFAULT 'Pickup',
    status VARCHAR(50) NOT NULL DEFAULT 'NotStarted',
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_trips_route_id ON trips (route_id);
CREATE INDEX IF NOT EXISTS idx_trips_bus_id ON trips (bus_id);
CREATE INDEX IF NOT EXISTS idx_trips_driver_id ON trips (driver_id);
CREATE INDEX IF NOT EXISTS idx_trips_status ON trips (status);
CREATE INDEX IF NOT EXISTS idx_trips_departure_time ON trips (departure_time);

SELECT diesel_manage_updated_at('trips');
