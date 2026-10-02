ALTER TABLE bookings
ADD COLUMN pickup_point UUID REFERENCES points(id),
ADD COLUMN dropoff_point UUID REFERENCES points(id),
ADD COLUMN num_of_kids INTEGER NOT NULL DEFAULT 0;

ALTER TABLE trips
ADD COLUMN return_time TIMESTAMP;
