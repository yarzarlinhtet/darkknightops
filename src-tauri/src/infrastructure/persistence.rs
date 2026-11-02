use crate::{
    application::ServerRepository,
    domain::{AppError, ErrorCode, Server, ServerInput},
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../migrations/001_servers.sql")),
    (2, include_str!("../../migrations/002_server_details.sql")),
];
fn database_error(_: rusqlite::Error) -> AppError {
    AppError::new(
        ErrorCode::DatabaseError,
        "Local database operation failed. Retry or restart RemoteDock.",
    )
}
fn migrate(db: &mut Connection, migrations: &[(i64, &str)]) -> Result<(), AppError> {
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(database_error)?;
    if version > migrations.last().map_or(0, |m| m.0) {
        return Err(AppError::new(
            ErrorCode::MigrationFailed,
            "This database belongs to a newer RemoteDock version.",
        ));
    }
    let tx = db.transaction().map_err(database_error)?;
    for (number, sql) in migrations.iter().filter(|m| m.0 > version) {
        tx.execute_batch(sql).map_err(|_| {
            AppError::new(
                ErrorCode::MigrationFailed,
                "Database migration failed. No changes were applied.",
            )
        })?;
        tx.pragma_update(None, "user_version", number)
            .map_err(database_error)?;
    }
    tx.commit().map_err(database_error)
}
pub struct SqliteServerRepository {
    db: Mutex<Connection>,
}
impl SqliteServerRepository {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let mut db = Connection::open(path).map_err(|_| {
            AppError::new(
                ErrorCode::DatabaseUnavailable,
                "Cannot open the local database.",
            )
        })?;
        db.busy_timeout(Duration::from_secs(5))
            .map_err(database_error)?;
        migrate(&mut db, MIGRATIONS)?;
        Ok(Self { db: Mutex::new(db) })
    }
    fn connection(&self) -> Result<MutexGuard<'_, Connection>, AppError> {
        self.db.lock().map_err(|_| {
            AppError::new(
                ErrorCode::DatabaseUnavailable,
                "Database is unavailable. Restart RemoteDock.",
            )
        })
    }
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Server> {
    Ok(Server {
        id: r.get(0)?,
        name: r.get(1)?,
        host: r.get(2)?,
        port: r.get(3)?,
        username: r.get(4)?,
        notes: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}
fn get(db: &Connection, id: &str) -> Result<Server, AppError> {
    db.query_row(
        "SELECT id,name,host,port,username,notes,created_at,updated_at FROM servers WHERE id=?1",
        [id],
        row,
    )
    .optional()
    .map_err(database_error)?
    .ok_or_else(AppError::not_found)
}
impl ServerRepository for SqliteServerRepository {
    fn list(&self) -> Result<Vec<Server>, AppError> {
        let db = self.connection()?;
        let mut stmt = db.prepare("SELECT id,name,host,port,username,notes,created_at,updated_at FROM servers ORDER BY name COLLATE NOCASE,id").map_err(database_error)?;
        let rows = stmt.query_map([], row).map_err(database_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
    }
    fn get(&self, id: &str) -> Result<Server, AppError> {
        let db = self.connection()?;
        get(&db, id)
    }
    fn create(&self, input: &ServerInput) -> Result<Server, AppError> {
        input.validate()?;
        let db = self.connection()?;
        let id = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO servers(id,name,host,port,username,notes,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))", params![id,input.name,input.host,input.port,input.username,input.notes]).map_err(database_error)?;
        get(&db, &id)
    }
    fn update(&self, id: &str, input: &ServerInput) -> Result<Server, AppError> {
        input.validate()?;
        let db = self.connection()?;
        let changed = db.execute("UPDATE servers SET name=?2,host=?3,port=?4,username=?5,notes=?6,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1", params![id,input.name,input.host,input.port,input.username,input.notes]).map_err(database_error)?;
        if changed == 0 {
            return Err(AppError::not_found());
        }
        get(&db, id)
    }
    fn delete(&self, id: &str) -> Result<(), AppError> {
        let changed = self
            .connection()?
            .execute("DELETE FROM servers WHERE id=?1", [id])
            .map_err(database_error)?;
        if changed == 0 {
            return Err(AppError::not_found());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(name: &str) -> ServerInput {
        ServerInput {
            name: name.into(),
            host: "example.com".into(),
            port: 22,
            username: "ubuntu".into(),
            notes: Some("a note".into()),
        }
    }
    #[test]
    fn crud_survives_reopening_and_orders_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        let repo = SqliteServerRepository::open(&path).unwrap();
        let a = repo.create(&input("Zulu")).unwrap();
        let b = repo
            .create(&input("Alpha '; DROP TABLE servers; --"))
            .unwrap();
        assert_eq!(repo.list().unwrap()[0].id, b.id);
        let changed = repo.update(&a.id, &input("Edited")).unwrap();
        assert_eq!(changed.created_at, a.created_at);
        assert!(!changed.updated_at.is_empty());
        drop(repo);
        let repo = SqliteServerRepository::open(&path).unwrap();
        assert_eq!(repo.get(&a.id).unwrap().name, "Edited");
        assert_eq!(repo.get(&b.id).unwrap().name, b.name);
        repo.delete(&a.id).unwrap();
        assert_eq!(repo.get(&a.id).unwrap_err().code, ErrorCode::NotFound);
        assert_eq!(
            repo.update(&a.id, &input("missing")).unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(repo.delete(&a.id).unwrap_err().code, ErrorCode::NotFound);
    }
    #[test]
    fn migrates_existing_v1_data_and_repeats_safely() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(MIGRATIONS[0].1).unwrap();
        db.execute(
            "INSERT INTO servers VALUES('legacy','Old','example.com',22,'ubuntu')",
            [],
        )
        .unwrap();
        migrate(&mut db, MIGRATIONS).unwrap();
        migrate(&mut db, MIGRATIONS).unwrap();
        let server = get(&db, "legacy").unwrap();
        assert!(!server.created_at.is_empty());
        assert_eq!(server.notes, None);
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn migration_failure_rolls_back_and_newer_versions_are_rejected() {
        let mut db = Connection::open_in_memory().unwrap();
        assert!(migrate(
            &mut db,
            &[(1, "CREATE TABLE example(id INTEGER);"), (2, "INVALID SQL")]
        )
        .is_err());
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(db.prepare("SELECT * FROM example").is_err());
        db.pragma_update(None, "user_version", 99).unwrap();
        assert_eq!(
            migrate(&mut db, MIGRATIONS).unwrap_err().code,
            ErrorCode::MigrationFailed
        );
    }
    #[test]
    fn invalid_data_is_not_persisted() {
        let repo = SqliteServerRepository::open(Path::new(":memory:")).unwrap();
        let mut data = input("bad");
        data.host = "a;rm".into();
        assert!(repo.create(&data).is_err());
        assert!(repo.list().unwrap().is_empty());
    }
}
