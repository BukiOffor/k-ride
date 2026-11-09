-- Create cars table if it does not exist
CREATE TABLE IF NOT EXISTS cars (
    id UUID PRIMARY KEY,
    license_plate VARCHAR(50) NOT NULL,
    capacity INTEGER NOT NULL,
    car_type VARCHAR(50) NOT NULL,
    car_tier VARCHAR(50) NOT NULL,
    car_size VARCHAR(50) NOT NULL,
    color VARCHAR(50) NOT NULL,
    color_hex VARCHAR(20),
    brand VARCHAR(100) NOT NULL,
    model VARCHAR(100) NOT NULL,
    year INTEGER NOT NULL,
    vehicle_photo TEXT,
    vehicle_registration TEXT,
    insurance_certificate TEXT,
    roadworthiness_certificate TEXT,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_cars_license_plate ON cars (license_plate);

-- Add car column to users table if not already added
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' AND column_name = 'car'
    ) THEN
        ALTER TABLE users ADD COLUMN car UUID REFERENCES cars(id) ON DELETE SET NULL;
    END IF;
END $$;

-- Create driver_profiles table
CREATE TABLE IF NOT EXISTS driver_profiles (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    car_id UUID NOT NULL REFERENCES cars(id) ON DELETE RESTRICT,
    nin_slip TEXT NOT NULL,
    profile_photo TEXT NOT NULL,
    is_koinonia_worker BOOLEAN NOT NULL DEFAULT FALSE,
    koinonia_department VARCHAR(255),
    guarantor_name VARCHAR(255),
    guarantor_phone VARCHAR(50),
    guarantor_relationship VARCHAR(100),
    guarantor_photo TEXT,
    licence_number VARCHAR(100) NOT NULL,
    licence_class VARCHAR(10) NOT NULL,
    licence_expiry_date DATE NOT NULL,
    licence_photo TEXT NOT NULL,
    track VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'Pending',
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_driver_profiles_user_id UNIQUE (user_id)
);

CREATE INDEX IF NOT EXISTS idx_driver_profiles_user_id ON driver_profiles (user_id);
CREATE INDEX IF NOT EXISTS idx_driver_profiles_car_id ON driver_profiles (car_id);
CREATE INDEX IF NOT EXISTS idx_driver_profiles_status ON driver_profiles (status);
