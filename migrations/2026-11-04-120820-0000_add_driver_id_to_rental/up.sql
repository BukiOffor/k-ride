ALTER TABLE rental_requests
    ADD COLUMN driver_id UUID REFERENCES users(id);
