-- Add city column to users table
ALTER TABLE users ADD COLUMN city VARCHAR(100) NOT NULL DEFAULT '';
