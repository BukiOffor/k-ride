CREATE TABLE rental_requests (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    pickup_location TEXT NOT NULL CHECK (length(trim(pickup_location)) BETWEEN 1 AND 1000),
    notes TEXT NOT NULL DEFAULT '' CHECK (length(notes) <= 5000),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'assigned')),
    assigned_by UUID REFERENCES users(id),
    assigned_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (ends_at >= starts_at + interval '24 hours'),
    CHECK ((status = 'pending' AND assigned_by IS NULL AND assigned_at IS NULL)
        OR (status = 'assigned' AND assigned_by IS NOT NULL AND assigned_at IS NOT NULL))
);
CREATE INDEX rental_requests_user_created ON rental_requests(user_id, created_at DESC, id DESC);
CREATE INDEX rental_requests_status_created ON rental_requests(status, created_at DESC, id DESC);
