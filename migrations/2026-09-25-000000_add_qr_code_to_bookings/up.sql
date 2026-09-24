-- Add qr_code column to store the generated QR code base64 PNG at booking creation.
-- Nullable so existing rows are unaffected; they'll be lazily backfilled on first access.
ALTER TABLE bookings ADD COLUMN qr_code TEXT;
