CREATE TABLE account_owner (wallet BLOB NOT NULL PRIMARY KEY CHECK(length(wallet) = 32));
CREATE TABLE product_storage (
    product_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value BLOB NOT NULL,
    PRIMARY KEY (product_id, key)
);
CREATE TABLE core_state (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL);
CREATE TABLE paired_hosts (
    wallet BLOB NOT NULL,
    peer_statement BLOB NOT NULL,
    peer_encryption BLOB NOT NULL,
    status TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    outgoing_update_at INTEGER,
    last_sync_offer_id TEXT,
    PRIMARY KEY (wallet, peer_statement, peer_encryption)
);
CREATE TABLE paired_host_metadata (
    wallet BLOB NOT NULL,
    peer_statement BLOB NOT NULL,
    peer_encryption BLOB NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (wallet, peer_statement, peer_encryption, key),
    FOREIGN KEY (wallet, peer_statement, peer_encryption)
        REFERENCES paired_hosts (wallet, peer_statement, peer_encryption) ON DELETE CASCADE
);
CREATE TABLE allowance_records (
    wallet BLOB NOT NULL,
    chain BLOB NOT NULL,
    resource TEXT NOT NULL,
    account BLOB NOT NULL,
    allocated_at INTEGER NOT NULL,
    priority INTEGER,
    last_renewed_period INTEGER,
    PRIMARY KEY (wallet, chain, resource, account)
);
CREATE TABLE statement_slots (
    wallet BLOB NOT NULL,
    chain BLOB NOT NULL,
    collection TEXT NOT NULL,
    period INTEGER NOT NULL,
    slot INTEGER NOT NULL,
    account BLOB NOT NULL,
    priority INTEGER NOT NULL,
    last_allocated_or_renewed_at INTEGER NOT NULL,
    PRIMARY KEY (wallet, chain, collection, period, slot)
);
CREATE TABLE products (
    product_id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    icon_cid TEXT,
    icon_format TEXT,
    worker_url_override TEXT
);
CREATE TABLE scheduled_notifications (
    product_id TEXT NOT NULL REFERENCES products(product_id) ON DELETE RESTRICT,
    notification_id INTEGER NOT NULL,
    text TEXT NOT NULL,
    deeplink TEXT,
    scheduled_at INTEGER,
    state TEXT NOT NULL CHECK(state IN ('register', 'registered', 'cancel')),
    revision INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (product_id, notification_id),
    UNIQUE (notification_id)
);
CREATE TABLE product_workers (
    product_id TEXT PRIMARY KEY REFERENCES products(product_id) ON DELETE CASCADE,
    content_hash BLOB NOT NULL,
    pending_hash BLOB,
    manifest BLOB NOT NULL,
    checked_at INTEGER NOT NULL,
    failure_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);
CREATE TABLE product_worker_reasons (
    product_id TEXT NOT NULL REFERENCES product_workers(product_id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('chat', 'pocket')),
    card_id TEXT NOT NULL DEFAULT '',
    added_at INTEGER NOT NULL,
    PRIMARY KEY (product_id, kind, card_id)
);
CREATE TABLE product_worker_operations (
    product_id TEXT NOT NULL REFERENCES product_workers(product_id) ON DELETE CASCADE,
    operation_id INTEGER NOT NULL,
    label TEXT,
    started_at INTEGER NOT NULL,
    PRIMARY KEY (product_id, operation_id)
);
CREATE TABLE runtime_sequences (
    name TEXT NOT NULL PRIMARY KEY,
    value INTEGER NOT NULL CHECK(value BETWEEN 1 AND 4294967295)
);
