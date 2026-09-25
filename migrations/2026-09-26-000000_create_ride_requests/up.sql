CREATE TABLE IF NOT EXISTS ride_requests (
    id UUID PRIMARY KEY,
    route_id UUID NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    count INT NOT NULL DEFAULT 1,
    departure_date DATE NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_ride_requests_route_date UNIQUE (route_id, departure_date)
);

CREATE INDEX IF NOT EXISTS idx_ride_requests_route_id ON ride_requests(route_id);
CREATE INDEX IF NOT EXISTS idx_ride_requests_departure_date ON ride_requests(departure_date);
