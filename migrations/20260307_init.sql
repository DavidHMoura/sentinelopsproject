CREATE TABLE IF NOT EXISTS events (
    id UUID PRIMARY KEY,
    ts TIMESTAMPTZ NOT NULL,
    event_type VARCHAR(64) NOT NULL,
    source_ip VARCHAR(64) NOT NULL,
    user_id UUID,
    actor VARCHAR(320),
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_events_ts ON events(ts);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_source_ip ON events(source_ip);

CREATE TABLE IF NOT EXISTS alerts (
    id UUID PRIMARY KEY,
    severity VARCHAR(16) NOT NULL,
    status VARCHAR(24) NOT NULL DEFAULT 'new',
    title VARCHAR(160) NOT NULL,
    description TEXT NOT NULL,
    fingerprint VARCHAR(128) UNIQUE NOT NULL,
    evidence JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_alerts_severity ON alerts(severity);
CREATE INDEX IF NOT EXISTS idx_alerts_status ON alerts(status);
CREATE INDEX IF NOT EXISTS idx_alerts_fingerprint ON alerts(fingerprint);
