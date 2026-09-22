-- ==============================================================================
-- Migration: init_database
-- Tables: users, points, routes, buses, bookings, activity_logs
-- Includes: Primary Keys, Foreign Keys, Uniques, Checks, Indexes, Triggers
-- ==============================================================================

-- 1. USERS TABLE
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    first_name VARCHAR(100) NOT NULL,
    last_name VARCHAR(100) NOT NULL,
    email VARCHAR(255) NOT NULL,
    username VARCHAR(100) NOT NULL,
    phone VARCHAR(50) NOT NULL,
    address TEXT NOT NULL,
    country VARCHAR(100) NOT NULL,
    identity_type VARCHAR(100),
    identity_number VARCHAR(100),
    password_hash TEXT NOT NULL,
    is_identity_verified BOOLEAN NOT NULL DEFAULT FALSE,
    role VARCHAR(50) NOT NULL DEFAULT 'User',
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_users_email UNIQUE (email),
    CONSTRAINT uq_users_username UNIQUE (username),
    CONSTRAINT uq_users_phone UNIQUE (phone),
    CONSTRAINT chk_users_email_non_empty CHECK (trim(email) <> ''),
    CONSTRAINT chk_users_phone_non_empty CHECK (trim(phone) <> '')
);

CREATE INDEX IF NOT EXISTS idx_users_role ON users (role);
CREATE INDEX IF NOT EXISTS idx_users_is_active ON users (is_active);
CREATE INDEX IF NOT EXISTS idx_users_created_at ON users (created_at DESC);

SELECT diesel_manage_updated_at('users');


-- 2. POINTS TABLE (Bus stops, stations, pickup points)
CREATE TABLE IF NOT EXISTS points (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    geo_location JSONB NOT NULL,
    landmark TEXT NOT NULL,
    description TEXT NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_points_name ON points (name);
CREATE INDEX IF NOT EXISTS idx_points_geo_location ON points USING GIN (geo_location);
CREATE INDEX IF NOT EXISTS idx_points_created_at ON points (created_at DESC);

SELECT diesel_manage_updated_at('points');


-- 3. ROUTES TABLE
CREATE TABLE IF NOT EXISTS routes (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT NOT NULL,
    start_point UUID NOT NULL REFERENCES points (id) ON DELETE RESTRICT,
    end_point UUID NOT NULL REFERENCES points (id) ON DELETE RESTRICT,
    fare REAL NOT NULL DEFAULT 0.0,
    distance REAL NOT NULL DEFAULT 0.0,
    ride_type VARCHAR(50) NOT NULL DEFAULT 'Pickup',
    stops UUID[] NOT NULL DEFAULT '{}',
    departure_time TIME WITHOUT TIME ZONE NOT NULL,
    departure_date DATE NOT NULL,
    is_completed BOOLEAN NOT NULL DEFAULT FALSE,
    time_completed TIMESTAMP WITHOUT TIME ZONE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_routes_fare_positive CHECK (fare >= 0),
    CONSTRAINT chk_routes_distance_positive CHECK (distance >= 0),
    CONSTRAINT chk_routes_start_end_distinct CHECK (start_point <> end_point)
);

CREATE INDEX IF NOT EXISTS idx_routes_start_point ON routes (start_point);
CREATE INDEX IF NOT EXISTS idx_routes_end_point ON routes (end_point);
CREATE INDEX IF NOT EXISTS idx_routes_departure_date ON routes (departure_date);
CREATE INDEX IF NOT EXISTS idx_routes_departure_time ON routes (departure_time);
CREATE INDEX IF NOT EXISTS idx_routes_ride_type ON routes (ride_type);
CREATE INDEX IF NOT EXISTS idx_routes_is_completed ON routes (is_completed);
CREATE INDEX IF NOT EXISTS idx_routes_stops ON routes USING GIN (stops);
CREATE INDEX IF NOT EXISTS idx_routes_created_at ON routes (created_at DESC);

SELECT diesel_manage_updated_at('routes');


-- 4. BUSES TABLE
CREATE TABLE IF NOT EXISTS buses (
    id UUID PRIMARY KEY,
    license_plate VARCHAR(50) NOT NULL,
    capacity INTEGER NOT NULL,
    current_passenger_count INTEGER NOT NULL DEFAULT 0,
    current_route_id UUID REFERENCES routes (id) ON DELETE SET NULL,
    state VARCHAR(50) NOT NULL DEFAULT 'Stationary',
    status VARCHAR(50) NOT NULL DEFAULT 'Available',
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_buses_license_plate UNIQUE (license_plate),
    CONSTRAINT chk_buses_capacity_positive CHECK (capacity > 0),
    CONSTRAINT chk_buses_passenger_count_bounds CHECK (current_passenger_count >= 0 AND current_passenger_count <= capacity)
);

CREATE INDEX IF NOT EXISTS idx_buses_current_route_id ON buses (current_route_id);
CREATE INDEX IF NOT EXISTS idx_buses_status ON buses (status);
CREATE INDEX IF NOT EXISTS idx_buses_state ON buses (state);
CREATE INDEX IF NOT EXISTS idx_buses_created_at ON buses (created_at DESC);

SELECT diesel_manage_updated_at('buses');


-- 5. BOOKINGS TABLE
CREATE TABLE IF NOT EXISTS bookings (
    id UUID PRIMARY KEY,
    booking_ref VARCHAR(100) NOT NULL,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    bus_id UUID NOT NULL REFERENCES buses (id) ON DELETE CASCADE,
    route_id UUID NOT NULL REFERENCES routes (id) ON DELETE CASCADE,
    seat_number INTEGER NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'Confirmed',
    payment_status VARCHAR(50) NOT NULL DEFAULT 'Pending',
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_bookings_ref UNIQUE (booking_ref),
    CONSTRAINT chk_bookings_seat_positive CHECK (seat_number > 0)
);

-- Partial unique index: guarantees no duplicate active bookings for the same seat on a bus
CREATE UNIQUE INDEX IF NOT EXISTS idx_bookings_active_bus_seat ON bookings (bus_id, seat_number) 
    WHERE status NOT IN ('Cancelled', 'Revoked');

CREATE INDEX IF NOT EXISTS idx_bookings_user_id ON bookings (user_id);
CREATE INDEX IF NOT EXISTS idx_bookings_bus_id ON bookings (bus_id);
CREATE INDEX IF NOT EXISTS idx_bookings_route_id ON bookings (route_id);
CREATE INDEX IF NOT EXISTS idx_bookings_status ON bookings (status);
CREATE INDEX IF NOT EXISTS idx_bookings_payment_status ON bookings (payment_status);
CREATE INDEX IF NOT EXISTS idx_bookings_created_at ON bookings (created_at DESC);

SELECT diesel_manage_updated_at('bookings');


-- 6. ACTIVITY LOGS TABLE
CREATE TABLE IF NOT EXISTS activity_logs (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    activity_type VARCHAR(100) NOT NULL,
    target_id UUID,
    target_type VARCHAR(100),
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    ip_address VARCHAR(100),
    user_agent TEXT,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_activity_logs_user_id ON activity_logs (user_id);
CREATE INDEX IF NOT EXISTS idx_activity_logs_activity_type ON activity_logs (activity_type);
CREATE INDEX IF NOT EXISTS idx_activity_logs_target ON activity_logs (target_id, target_type);
CREATE INDEX IF NOT EXISTS idx_activity_logs_details ON activity_logs USING GIN (details);
CREATE INDEX IF NOT EXISTS idx_activity_logs_created_at ON activity_logs (created_at DESC);
