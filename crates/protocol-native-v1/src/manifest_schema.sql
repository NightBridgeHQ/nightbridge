CREATE TABLE IF NOT EXISTS transfers (
    transfer_id      TEXT PRIMARY KEY NOT NULL,
    peer_fingerprint TEXT NOT NULL,
    direction        TEXT NOT NULL CHECK (direction IN ('send', 'receive')),
    state            TEXT NOT NULL CHECK (state IN ('pending', 'active', 'interrupted', 'completed', 'cancelled', 'failed')),
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS transfer_files (
    transfer_id TEXT NOT NULL,
    file_id     TEXT NOT NULL,
    file_name   TEXT NOT NULL,
    size        INTEGER NOT NULL,
    blake3      TEXT,
    PRIMARY KEY (transfer_id, file_id),
    FOREIGN KEY (transfer_id) REFERENCES transfers(transfer_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS verified_chunks (
    transfer_id TEXT NOT NULL,
    file_id     TEXT NOT NULL,
    offset      INTEGER NOT NULL,
    length      INTEGER NOT NULL,
    blake3      TEXT NOT NULL,
    PRIMARY KEY (transfer_id, file_id, offset),
    FOREIGN KEY (transfer_id, file_id) REFERENCES transfer_files(transfer_id, file_id) ON DELETE CASCADE
);
