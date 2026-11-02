ALTER TABLE servers ADD COLUMN notes TEXT;
ALTER TABLE servers ADD COLUMN created_at TEXT NOT NULL DEFAULT '';
ALTER TABLE servers ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
UPDATE servers SET created_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now');
CREATE INDEX servers_name_index ON servers(name COLLATE NOCASE, id);
