pub mod connection;
pub mod keys;
pub mod provisioning;
use crate::domain::{validate_id, AppError, Server, ServerInput};
use std::sync::Arc;

pub trait ServerRepository: Send + Sync {
    fn connection(
        &self,
        _id: &str,
    ) -> Result<crate::domain::connection::ConnectionRecord, AppError> {
        Err(AppError::new(
            crate::domain::ErrorCode::Internal,
            "Connection repository unavailable.",
        ))
    }
    fn save_connection(
        &self,
        _id: &str,
        _record: &crate::domain::connection::ConnectionRecord,
    ) -> Result<(), AppError> {
        Err(AppError::new(
            crate::domain::ErrorCode::Internal,
            "Connection repository unavailable.",
        ))
    }
    fn list(&self) -> Result<Vec<Server>, AppError>;
    fn get(&self, id: &str) -> Result<Server, AppError>;
    fn create(&self, input: &ServerInput) -> Result<Server, AppError>;
    fn update(&self, id: &str, input: &ServerInput) -> Result<Server, AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
}

pub struct ServerService {
    pub provisioning: Option<Arc<provisioning::ProvisioningService>>,
    pub keys: Option<Arc<keys::KeyService>>,
    repository: Arc<dyn ServerRepository>,
    pub connections: Option<Arc<connection::ConnectionService>>,
}
impl ServerService {
    pub fn new(repository: Arc<dyn ServerRepository>) -> Self {
        Self {
            keys: None,
            provisioning: None,
            repository,
            connections: None,
        }
    }
    pub fn with_connections(mut self, connections: Arc<connection::ConnectionService>) -> Self {
        self.connections = Some(connections);
        self
    }
    pub fn list(&self) -> Result<Vec<Server>, AppError> {
        self.repository.list()
    }
    pub fn get(&self, id: &str) -> Result<Server, AppError> {
        validate_id(id)?;
        self.repository.get(id)
    }
    pub fn create(&self, input: &ServerInput) -> Result<Server, AppError> {
        input.validate()?;
        self.repository.create(input)
    }
    pub fn update(&self, id: &str, input: &ServerInput) -> Result<Server, AppError> {
        validate_id(id)?;
        input.validate()?;
        let _guard = self.connections.as_ref().map(|c| c.begin(id)).transpose()?;
        let previous = self.repository.get(id)?;
        if let Some(c) = &self.connections {
            c.prepare_edit(
                id,
                previous.host != input.host || previous.port != input.port,
            )?;
        }
        self.repository.update(id, input)
    }
    pub fn save_with_key(
        &self,
        id: Option<&str>,
        input: &ServerInput,
        key: &str,
    ) -> Result<Server, AppError> {
        input.validate()?;
        let keys = self.keys.as_ref().ok_or_else(|| {
            AppError::new(
                crate::domain::ErrorCode::Internal,
                "Key service unavailable.",
            )
        })?;
        keys.get(key)?;
        let _guard = if let Some(id) = id {
            validate_id(id)?;
            self.connections.as_ref().map(|c| c.begin(id)).transpose()?
        } else {
            None
        };
        if let Some(id) = id {
            let previous = self.repository.get(id)?;
            if let Some(c) = &self.connections {
                c.prepare_edit(
                    id,
                    previous.host != input.host || previous.port != input.port,
                )?;
            }
        }
        keys.repository.save_server(id, input, key)
    }
    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        validate_id(id)?;
        let _guard = self.connections.as_ref().map(|c| c.begin(id)).transpose()?;
        if let Some(c) = &self.connections {
            c.prepare_delete(id)?;
        }
        self.repository.delete(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct MustNotBeCalled;
    impl ServerRepository for MustNotBeCalled {
        fn list(&self) -> Result<Vec<Server>, AppError> {
            panic!("unexpected persistence call")
        }
        fn get(&self, _: &str) -> Result<Server, AppError> {
            panic!("invalid ID reached persistence")
        }
        fn create(&self, _: &ServerInput) -> Result<Server, AppError> {
            panic!("invalid input reached persistence")
        }
        fn update(&self, _: &str, _: &ServerInput) -> Result<Server, AppError> {
            panic!("invalid input reached persistence")
        }
        fn delete(&self, _: &str) -> Result<(), AppError> {
            panic!("invalid ID reached persistence")
        }
    }
    #[test]
    fn application_rejects_invalid_requests_before_persistence() {
        let service = ServerService::new(Arc::new(MustNotBeCalled));
        let input = ServerInput {
            name: String::new(),
            host: "example.com".into(),
            port: 22,
            username: "ubuntu".into(),
            notes: None,
        };
        assert!(service.create(&input).is_err());
        assert!(service
            .update(&uuid::Uuid::new_v4().to_string(), &input)
            .is_err());
        assert!(service.get("invalid").is_err());
        assert!(service.delete("invalid").is_err());
    }
}
