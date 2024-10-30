use std::fmt::Display;

#[derive(Debug)]
pub enum AuthError {
    InvalidApiKey,
    MissingApiKey,
    KeyExpired,
    KeySuspended,
    InsufficientPermissions,
    RateLimitExceeded,
    StorageError(String),
}

impl Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<String> for AuthError {
    fn from(error: String) -> Self {
        AuthError::StorageError(error)
    }
} 