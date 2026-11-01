CREATE TABLE password_reset_otps (
 user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
 challenge_id uuid NOT NULL,
 otp_hash text NOT NULL,
 expires_at timestamptz NOT NULL,
 requested_at timestamptz NOT NULL DEFAULT now(),
 attempts integer NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
 consumed_at timestamptz
);
