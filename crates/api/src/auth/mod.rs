//! Authentication module: JWT, refresh tokens, registration, login, and password change.

pub mod dto;
pub mod handlers;
pub mod jwt;
pub mod middleware;

pub use middleware::AuthenticatedUser;
