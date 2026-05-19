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

CREATE TABLE IF NOT EXISTS localsend_peers (
    fingerprint   TEXT PRIMARY KEY NOT NULL,
    alias         TEXT NOT NULL DEFAULT '',
    label         TEXT,
    status        TEXT NOT NULL CHECK (status IN ('pending', 'trusted', 'blocked'))
                              DEFAULT 'pending',
    first_seen    INTEGER NOT NULL,
    last_seen     INTEGER NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    source_ip     TEXT
);

CREATE INDEX IF NOT EXISTS idx_localsend_peers_status_seen
    ON localsend_peers(status, last_seen);

CREATE TABLE IF NOT EXISTS localsend_lan_peers (
    fingerprint   TEXT PRIMARY KEY NOT NULL,
    alias         TEXT NOT NULL DEFAULT '',
    source_ip     TEXT NOT NULL,
    port          INTEGER NOT NULL,
    protocol      TEXT NOT NULL DEFAULT 'https',
    device_model  TEXT,
    device_type   TEXT,
    download      INTEGER NOT NULL DEFAULT 1,
    first_seen    INTEGER NOT NULL,
    last_seen     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_localsend_lan_peers_seen
    ON localsend_lan_peers(last_seen);
