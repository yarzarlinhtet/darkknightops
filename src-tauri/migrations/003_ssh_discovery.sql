CREATE TABLE server_connections(
    server_id TEXT PRIMARY KEY REFERENCES servers(id) ON DELETE CASCADE,
    record_json TEXT NOT NULL
);
