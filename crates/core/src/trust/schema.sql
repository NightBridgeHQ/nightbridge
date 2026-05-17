CREATE TABLE IF NOT EXISTS peers (
    fingerprint  TEXT PRIMARY KEY NOT NULL,
    pubkey       BLOB NOT NULL,
    label        TEXT NOT NULL DEFAULT '',
    trusted_at   INTEGER NOT NULL,
    last_seen    INTEGER,
    native_certificate_fingerprint TEXT,
    policy       TEXT NOT NULL CHECK (policy IN ('auto_accept', 'prompt', 'block'))
                              DEFAULT 'prompt'
);

CREATE INDEX IF NOT EXISTS idx_peers_last_seen ON peers(last_seen);
