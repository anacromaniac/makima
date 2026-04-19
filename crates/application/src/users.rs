//! User profile use cases.

use std::sync::Arc;

use domain::{RepositoryError, User, UserRepository};
use uuid::Uuid;

/// Errors that can occur in user profile operations.
#[derive(Debug, thiserror::Error)]
pub enum UserError {
    /// The authenticated user's record was not found.
    #[error("user not found")]
    NotFound,
    /// A storage-layer error occurred.
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
}

/// Application service for user-profile workflows.
#[derive(Clone)]
pub struct UserService {
    user_repo: Arc<dyn UserRepository>,
}

impl UserService {
    /// Create a new user service.
    pub fn new(user_repo: Arc<dyn UserRepository>) -> Self {
        Self { user_repo }
    }

    /// Return the current user's profile.
    pub async fn get_me(&self, user_id: Uuid) -> Result<User, UserError> {
        self.user_repo
            .find_by_id(user_id)
            .await?
            .ok_or(UserError::NotFound)
    }
}
