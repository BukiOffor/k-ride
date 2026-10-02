ALTER TABLE trips
DROP COLUMN return_time;

ALTER TABLE bookings
DROP COLUMN num_of_kids,
DROP COLUMN dropoff_point,
DROP COLUMN pickup_point;
