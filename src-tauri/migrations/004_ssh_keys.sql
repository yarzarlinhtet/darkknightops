CREATE TABLE ssh_keys (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, algorithm TEXT NOT NULL,
 fingerprint_sha256 TEXT NOT NULL UNIQUE, public_key TEXT NOT NULL,
 private_key_path TEXT NOT NULL, storage_type TEXT NOT NULL CHECK(storage_type IN ('MANAGED','EXTERNAL')),
 passphrase_keychain_ref TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
ALTER TABLE servers ADD COLUMN ssh_key_id TEXT REFERENCES ssh_keys(id) ON DELETE RESTRICT;
CREATE INDEX servers_ssh_key_id ON servers(ssh_key_id);
CREATE TABLE ssh_key_cleanup(id TEXT PRIMARY KEY, private_key_path TEXT, credential_ref TEXT);
