CREATE TABLE IF NOT EXISTS ml_features_log (
    event_id UUID PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,
    hour_of_day SMALLINT NOT NULL,
    is_weekend BOOLEAN NOT NULL,
    event_type_id INTEGER NOT NULL,
    has_actor BOOLEAN NOT NULL,
    payload_size_bytes INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ml_features_created_at ON ml_features_log(created_at);