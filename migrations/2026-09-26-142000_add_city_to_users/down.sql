-- Rollback: remove city column from users table
ALTER TABLE users DROP COLUMN IF EXISTS city;
