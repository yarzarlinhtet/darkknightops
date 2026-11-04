CREATE TABLE provisioning_runs (
 id TEXT PRIMARY KEY,
 server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
 status TEXT NOT NULL,
 plan_hash TEXT NOT NULL,
 started_at TEXT NOT NULL,
 finished_at TEXT,
 error_code TEXT,
 error_message TEXT,
 snapshot TEXT NOT NULL
);
CREATE INDEX provisioning_runs_server ON provisioning_runs(server_id, started_at DESC);
CREATE UNIQUE INDEX provisioning_one_active ON provisioning_runs(server_id) WHERE status IN ('READY','RUNNING');
CREATE TABLE provisioning_steps (
 id TEXT PRIMARY KEY,
 run_id TEXT NOT NULL REFERENCES provisioning_runs(id) ON DELETE CASCADE,
 step_key TEXT NOT NULL,
 step_order INTEGER NOT NULL,
 status TEXT NOT NULL,
 started_at TEXT,
 finished_at TEXT,
 error_code TEXT,
 sanitized_output TEXT NOT NULL,
 UNIQUE(run_id, step_key)
);
