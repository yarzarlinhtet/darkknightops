use crate::{
    application::keys::{Cleanup, KeyRepository, NewKey},
    domain::{keys::SshKey, validate_id, AppError, ErrorCode},
    infrastructure::persistence::SqliteServerRepository,
};
use rusqlite::params;
use std::path::PathBuf;
fn fail() -> AppError {
    super::persistence::key_database_error()
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SshKey> {
    Ok(SshKey {
        id: r.get(0)?,
        name: r.get(1)?,
        algorithm: r.get(2)?,
        fingerprint_sha256: r.get(3)?,
        public_key: r.get(4)?,
        storage_type: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
        server_count: r.get(8)?,
        passphrase_saved: r.get(9)?,
    })
}
const SELECT:&str="SELECT k.id,k.name,k.algorithm,k.fingerprint_sha256,k.public_key,k.storage_type,k.created_at,k.updated_at,(SELECT count(*) FROM servers WHERE ssh_key_id=k.id),k.passphrase_keychain_ref IS NOT NULL FROM ssh_keys k";
impl KeyRepository for SqliteServerRepository {
    fn list_keys(&self) -> Result<Vec<SshKey>, AppError> {
        let db = self.db.lock().map_err(|_| fail())?;
        let mut q = db
            .prepare(&format!("{SELECT} ORDER BY k.name COLLATE NOCASE,k.id"))
            .map_err(|_| fail())?;
        let rows = q.query_map([], row).map_err(|_| fail())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|_| fail())
    }
    fn get_key(&self, id: &str) -> Result<SshKey, AppError> {
        validate_id(id)?;
        KeyRepository::list_keys(self)?
            .into_iter()
            .find(|k| k.id == id)
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "SSH key no longer exists."))
    }
    fn resolve(&self, id: &str) -> Result<(PathBuf, Option<String>, String), AppError> {
        let db = self.db.lock().map_err(|_| fail())?;
        db.query_row("SELECT private_key_path,passphrase_keychain_ref,fingerprint_sha256 FROM ssh_keys WHERE id=?",[id],|r|Ok((PathBuf::from(r.get::<_,String>(0)?),r.get(1)?,r.get(2)?))).map_err(|_|fail())
    }
    fn assigned(&self, server: &str) -> Result<Option<String>, AppError> {
        let db = self.db.lock().map_err(|_| fail())?;
        db.query_row("SELECT ssh_key_id FROM servers WHERE id=?", [server], |r| {
            r.get(0)
        })
        .map_err(|_| fail())
    }
    fn servers(&self, id: &str) -> Result<Vec<String>, AppError> {
        KeyRepository::get_key(self, id)?;
        let db = self.db.lock().map_err(|_| fail())?;
        let mut q = db
            .prepare("SELECT id FROM servers WHERE ssh_key_id=? ORDER BY name")
            .map_err(|_| fail())?;
        let rows = q.query_map([id], |r| r.get(0)).map_err(|_| fail())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|_| fail())
    }

    fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        self.db
            .lock()
            .map_err(|_| fail())?
            .execute(
                "UPDATE ssh_keys SET name=?,updated_at=? WHERE id=?",
                params![name, chrono::Utc::now().to_rfc3339(), id],
            )
            .map_err(|_| fail())?;
        Ok(())
    }
    fn contains_fingerprint(&self, fingerprint: &str) -> Result<bool, AppError> {
        self.db
            .lock()
            .map_err(|_| fail())?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM ssh_keys WHERE fingerprint_sha256=?)",
                [fingerprint],
                |r| r.get(0),
            )
            .map_err(|_| fail())
    }
    fn queue_cleanup(&self, entry: &Cleanup) -> Result<(), AppError> {
        self.db
            .lock()
            .map_err(|_| fail())?
            .execute(
                "INSERT INTO ssh_key_cleanup VALUES(?,?,?)",
                params![entry.id, entry.path, entry.reference],
            )
            .map_err(|_| fail())?;
        Ok(())
    }
    fn insert(&self, k: &NewKey) -> Result<(), AppError> {
        let mut db = self.db.lock().map_err(|_| fail())?;
        let tx = db.transaction().map_err(|_| fail())?;
        tx.execute(
            "INSERT INTO ssh_keys VALUES(?,?,?,?,?,?,?,?,?,?)",
            params![
                k.id,
                k.name,
                k.algorithm,
                k.fingerprint,
                k.public_key,
                k.path,
                k.storage_type,
                k.reference,
                k.now,
                k.now
            ],
        )
        .map_err(|_| fail())?;
        tx.execute("DELETE FROM ssh_key_cleanup WHERE id=?", [&k.id])
            .map_err(|_| fail())?;
        tx.commit().map_err(|_| fail())
    }
    fn delete_queued(&self, entry: &Cleanup) -> Result<(), AppError> {
        let mut db = self.db.lock().map_err(|_| fail())?;
        let tx = db.transaction().map_err(|_| fail())?;
        tx.execute(
            "INSERT INTO ssh_key_cleanup VALUES(?,?,?)",
            params![entry.id, entry.path, entry.reference],
        )
        .map_err(|_| fail())?;
        tx.execute("DELETE FROM ssh_keys WHERE id=?", [&entry.id])
            .map_err(|_| fail())?;
        tx.commit().map_err(|_| fail())
    }
    fn cleanup_entries(&self) -> Result<Vec<Cleanup>, AppError> {
        let db = self.db.lock().map_err(|_| fail())?;
        let mut q = db
            .prepare("SELECT id,private_key_path,credential_ref FROM ssh_key_cleanup")
            .map_err(|_| fail())?;
        let rows = q
            .query_map([], |r| {
                Ok(Cleanup {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    reference: r.get(2)?,
                })
            })
            .map_err(|_| fail())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|_| fail())
    }
    fn finish_cleanup(&self, id: &str) -> Result<(), AppError> {
        self.db
            .lock()
            .map_err(|_| fail())?
            .execute("DELETE FROM ssh_key_cleanup WHERE id=?", [id])
            .map_err(|_| fail())?;
        Ok(())
    }
    fn save_server(
        &self,
        id: Option<&str>,
        input: &crate::domain::ServerInput,
        key: &str,
    ) -> Result<crate::domain::Server, AppError> {
        let server_id = id
            .map(str::to_string)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let now = chrono::Utc::now().to_rfc3339();
        {
            let mut db = self.db.lock().map_err(|_| fail())?;
            let tx = db.transaction().map_err(|_| fail())?;
            if id.is_some() {
                let changed=tx.execute("UPDATE servers SET name=?,host=?,port=?,username=?,notes=?,updated_at=?,ssh_key_id=? WHERE id=?",params![input.name,input.host,input.port,input.username,input.notes,now,key,server_id]).map_err(|_|fail())?;
                if changed == 0 {
                    return Err(AppError::not_found());
                }
            } else {
                tx.execute("INSERT INTO servers(id,name,host,port,username,notes,created_at,updated_at,ssh_key_id) VALUES(?,?,?,?,?,?,?,?,?)",params![server_id,input.name,input.host,input.port,input.username,input.notes,now,now,key]).map_err(|_|fail())?;
            }
            tx.commit().map_err(|_| fail())?;
        }
        crate::application::ServerRepository::get(self, &server_id)
    }
    fn assign(&self, server: &str, key: Option<&str>) -> Result<(), AppError> {
        self.db
            .lock()
            .map_err(|_| fail())?
            .execute(
                "UPDATE servers SET ssh_key_id=?,updated_at=? WHERE id=?",
                params![key, chrono::Utc::now().to_rfc3339(), server],
            )
            .map_err(|_| fail())?;
        Ok(())
    }
}
