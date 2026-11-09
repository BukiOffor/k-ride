-- =====================================================================
-- Paid Rides (v1): ride products, estimates, rides, offers, driver
-- availability, ratings, safety events and idempotency keys.
-- Money is stored in integer minor units (kobo for NGN).
-- =====================================================================

-- Ride products ("Go", "Comfort", "XL"...). Data-driven pricing: every
-- number the fare estimator uses lives here, so it can be tuned without
-- a deploy.
CREATE TABLE IF NOT EXISTS ride_options (
    id VARCHAR(50) PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    vehicle_class VARCHAR(20) NOT NULL,
    eligible_vehicle_classes TEXT[] NOT NULL DEFAULT '{}',
    capacity INTEGER NOT NULL,
    shared BOOLEAN NOT NULL DEFAULT FALSE,
    currency VARCHAR(3) NOT NULL DEFAULT 'NGN',
    base_fare_minor BIGINT NOT NULL,
    per_km_minor BIGINT NOT NULL,
    per_minute_minor BIGINT NOT NULL,
    minimum_fare_minor BIGINT NOT NULL,
    booking_fee_minor BIGINT NOT NULL DEFAULT 0,
    free_waiting_minutes INTEGER NOT NULL DEFAULT 5,
    waiting_per_minute_minor BIGINT NOT NULL DEFAULT 0,
    rounding_minor BIGINT NOT NULL DEFAULT 5000,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO ride_options (id, name, description, vehicle_class, eligible_vehicle_classes, capacity,
                          base_fare_minor, per_km_minor, per_minute_minor, minimum_fare_minor,
                          booking_fee_minor, free_waiting_minutes, waiting_per_minute_minor, sort_order)
VALUES
    ('go', 'Go', 'Affordable everyday rides', 'sedan', '{compact,sedan,suv,premium,van}', 4,
     50000, 15000, 2000, 100000, 0, 5, 3000, 1),
    ('comfort', 'Comfort', 'Newer cars with extra legroom', 'suv', '{suv,premium}', 4,
     80000, 22000, 3000, 150000, 0, 5, 5000, 2),
    ('xl', 'XL', 'Rides for groups of up to 6', 'van', '{van}', 6,
     100000, 25000, 3500, 200000, 0, 5, 5000, 3)
ON CONFLICT (id) DO NOTHING;

-- Priced quotes handed to a rider. A ride request must reference a live one.
CREATE TABLE IF NOT EXISTS fare_estimates (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pickup JSONB NOT NULL,
    destination JSONB NOT NULL,
    route JSONB NOT NULL,
    options JSONB NOT NULL,
    expires_at TIMESTAMP WITHOUT TIME ZONE NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_fare_estimates_user ON fare_estimates (user_id);

CREATE TABLE IF NOT EXISTS rides (
    id UUID PRIMARY KEY,
    passenger_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    driver_id UUID REFERENCES users(id) ON DELETE RESTRICT,
    status VARCHAR(30) NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    option_id VARCHAR(50) NOT NULL REFERENCES ride_options(id),
    option_snapshot JSONB NOT NULL,
    estimate_id UUID NOT NULL,
    pickup JSONB NOT NULL,
    destination JSONB NOT NULL,
    route JSONB NOT NULL,
    fare JSONB NOT NULL,
    currency VARCHAR(3) NOT NULL,
    fare_total_minor BIGINT NOT NULL,
    waiting_charge_minor BIGINT NOT NULL DEFAULT 0,
    final_fare_minor BIGINT,
    payment_method VARCHAR(30) NOT NULL DEFAULT 'cash',
    payment_status VARCHAR(30) NOT NULL DEFAULT 'pending',
    trip_pin VARCHAR(8) NOT NULL,
    dispatch_round INTEGER NOT NULL DEFAULT 1,
    cancellation JSONB,
    tracking JSONB,
    rating JSONB,
    sos_status VARCHAR(20) NOT NULL DEFAULT 'none',
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    assigned_at TIMESTAMP WITHOUT TIME ZONE,
    arrived_at TIMESTAMP WITHOUT TIME ZONE,
    started_at TIMESTAMP WITHOUT TIME ZONE,
    completed_at TIMESTAMP WITHOUT TIME ZONE,
    cancelled_at TIMESTAMP WITHOUT TIME ZONE
);
CREATE INDEX IF NOT EXISTS idx_rides_passenger ON rides (passenger_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_rides_driver ON rides (driver_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_rides_status ON rides (status);
-- At most one live ride per passenger and per driver (atomic guard against races).
CREATE UNIQUE INDEX IF NOT EXISTS uq_rides_live_passenger ON rides (passenger_id)
    WHERE status NOT IN ('COMPLETED', 'CANCELLED');
CREATE UNIQUE INDEX IF NOT EXISTS uq_rides_live_driver ON rides (driver_id)
    WHERE driver_id IS NOT NULL AND status NOT IN ('COMPLETED', 'CANCELLED');

CREATE TABLE IF NOT EXISTS ride_offers (
    id UUID PRIMARY KEY,
    ride_id UUID NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
    driver_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    dispatch_round INTEGER NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'offered',
    pickup_distance_m INTEGER,
    pickup_eta_seconds INTEGER,
    expires_at TIMESTAMP WITHOUT TIME ZONE NOT NULL,
    responded_at TIMESTAMP WITHOUT TIME ZONE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_ride_offers_ride ON ride_offers (ride_id, dispatch_round);
CREATE INDEX IF NOT EXISTS idx_ride_offers_driver ON ride_offers (driver_id, status);

CREATE TABLE IF NOT EXISTS driver_availability (
    driver_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    status VARCHAR(20) NOT NULL DEFAULT 'OFFLINE',
    lat DOUBLE PRECISION,
    lng DOUBLE PRECISION,
    heading DOUBLE PRECISION,
    speed_mps DOUBLE PRECISION,
    accuracy_m DOUBLE PRECISION,
    recorded_at TIMESTAMP WITHOUT TIME ZONE,
    received_at TIMESTAMP WITHOUT TIME ZONE,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_driver_availability_status ON driver_availability (status);

CREATE TABLE IF NOT EXISTS ride_ratings (
    id UUID PRIMARY KEY,
    ride_id UUID NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
    rater_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    ratee_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rater_role VARCHAR(20) NOT NULL,
    stars SMALLINT NOT NULL CHECK (stars BETWEEN 1 AND 5),
    tags TEXT[] NOT NULL DEFAULT '{}',
    comment TEXT,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_ride_ratings_ride_role UNIQUE (ride_id, rater_role)
);
CREATE INDEX IF NOT EXISTS idx_ride_ratings_ratee ON ride_ratings (ratee_id);

-- Append-only safety log (SOS + reports). Never updated by application code
-- except for desk handling status.
CREATE TABLE IF NOT EXISTS safety_events (
    id UUID PRIMARY KEY,
    ride_id UUID NOT NULL REFERENCES rides(id) ON DELETE RESTRICT,
    reporter_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    reporter_role VARCHAR(20) NOT NULL,
    kind VARCHAR(20) NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    details TEXT,
    location JSONB,
    driver_location JSONB,
    status VARCHAR(20) NOT NULL DEFAULT 'sent',
    occurred_at TIMESTAMP WITHOUT TIME ZONE NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_safety_events_ride ON safety_events (ride_id);

CREATE TABLE IF NOT EXISTS idempotency_keys (
    user_id UUID NOT NULL,
    endpoint VARCHAR(100) NOT NULL,
    idempotency_key VARCHAR(200) NOT NULL,
    status_code INTEGER NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, endpoint, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_idempotency_keys_created ON idempotency_keys (created_at);
