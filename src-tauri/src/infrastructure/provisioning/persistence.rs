use crate::{
    application::provisioning::ProvisioningRepository,
    domain::{provisioning::*, AppError, ErrorCode},
    infrastructure::persistence::SqliteServerRepository,
};
use rusqlite::{params, OptionalExtension};
fn error(_: impl std::fmt::Debug) -> AppError {
    AppError::new(
        ErrorCode::DatabaseError,
        "Provisioning history could not be saved or loaded.",
    )
}
impl ProvisioningRepository for SqliteServerRepository {
    fn save_run(&self, run: &ProvisioningRun) -> Result<(), AppError> {
        let mut db = self.db.lock().map_err(error)?;
        let tx = db.transaction().map_err(error)?;
        let snapshot = serde_json::to_string(run).map_err(error)?;
        let status = serde_json::to_value(&run.status).map_err(error)?;
        tx.execute("INSERT INTO provisioning_runs(id,server_id,status,plan_hash,started_at,finished_at,error_code,error_message,snapshot) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET status=excluded.status,finished_at=excluded.finished_at,error_code=excluded.error_code,error_message=excluded.error_message,snapshot=excluded.snapshot",params![run.id,run.server_id,status.as_str(),run.plan_hash,run.started_at,run.finished_at,run.error_code,run.error_message,snapshot]).map_err(error)?;
        for (order, step) in run.steps.iter().enumerate() {
            tx.execute("INSERT INTO provisioning_steps(id,run_id,step_key,step_order,status,started_at,finished_at,error_code,sanitized_output) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(run_id,step_key) DO UPDATE SET status=excluded.status,started_at=excluded.started_at,finished_at=excluded.finished_at,error_code=excluded.error_code,sanitized_output=excluded.sanitized_output",params![format!("{}:{}",run.id,step.key),run.id,step.key,order,step.status,step.started_at,step.finished_at,step.error_code,step.sanitized_output]).map_err(error)?;
        }
        tx.commit().map_err(error)
    }
    fn get_run(&self, server: &str, id: &str) -> Result<ProvisioningRun, AppError> {
        let db = self.db.lock().map_err(error)?;
        let value: Option<String> = db
            .query_row(
                "SELECT snapshot FROM provisioning_runs WHERE server_id=?1 AND id=?2",
                params![server, id],
                |r| r.get(0),
            )
            .optional()
            .map_err(error)?;
        serde_json::from_str(&value.ok_or_else(AppError::not_found)?).map_err(error)
    }
    fn list_runs(&self, server: &str) -> Result<Vec<ProvisioningRun>, AppError> {
        let db = self.db.lock().map_err(error)?;
        let mut stmt = db.prepare("SELECT snapshot FROM provisioning_runs WHERE server_id=?1 ORDER BY started_at DESC,id DESC LIMIT 50").map_err(error)?;
        let values = stmt
            .query_map([server], |r| r.get::<_, String>(0))
            .map_err(error)?;
        values
            .map(|v| serde_json::from_str(&v.map_err(error)?).map_err(error))
            .collect()
    }
    fn recover(&self) -> Result<(), AppError> {
        let runs = {
            let db = self.db.lock().map_err(error)?;
            let mut stmt = db
                .prepare(
                    "SELECT snapshot FROM provisioning_runs WHERE status IN ('READY','RUNNING')",
                )
                .map_err(error)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(error)?
        };
        for value in runs {
            let mut run: ProvisioningRun = serde_json::from_str(&value).map_err(error)?;
            run.status = RunStatus::Interrupted;
            run.error_code = Some("INTERRUPTED".into());
            run.error_message = Some("Desktop exited during provisioning. Remote state is unknown; run fresh preflight before retrying.".into());
            run.finished_at = Some(crate::infrastructure::discovery::now());
            for step in &mut run.steps {
                if step.status == "RUNNING" {
                    step.status = "UNKNOWN".into();
                }
            }
            self.save_run(&run)?;
        }
        Ok(())
    }
}
