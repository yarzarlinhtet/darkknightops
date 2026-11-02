use crate::domain::{validate_id, AppError, Server, ServerInput};
use std::sync::Arc;

pub trait ServerRepository: Send + Sync {
    fn list(&self) -> Result<Vec<Server>, AppError>;
    fn get(&self, id: &str) -> Result<Server, AppError>;
    fn create(&self, input: &ServerInput) -> Result<Server, AppError>;
    fn update(&self, id: &str, input: &ServerInput) -> Result<Server, AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
}

pub struct ServerService {
    repository: Arc<dyn ServerRepository>,
}
impl ServerService {
    pub fn new(repository: Arc<dyn ServerRepository>) -> Self {
        Self { repository }
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
        self.repository.update(id, input)
    }
    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        validate_id(id)?;
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
