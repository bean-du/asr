use axum::{
    http::Request,
    middleware::Next,
    response::Response,
    http::StatusCode,
};
use crate::utils::http::HttpResponse;
use super::{Auth, Permission};

pub async fn auth_middleware<B>(
    req: Request<B>,
    next: Next<B>,
    auth: Auth,
    required_permission: Permission,
) -> Result<Response, (StatusCode, axum::Json<HttpResponse<String>>)> {
    let api_key = req
        .headers()
        .get("X-API-Key")
        .and_then(|value| value.to_str().ok());

    if let Err(e) = auth.verify_api_key(api_key, required_permission).await {
        let (status, message) = match e {
            super::AuthError::MissingApiKey => 
                (StatusCode::UNAUTHORIZED, "Missing API key"),
            super::AuthError::InvalidApiKey => 
                (StatusCode::UNAUTHORIZED, "Invalid API key"),
            super::AuthError::KeyExpired => 
                (StatusCode::FORBIDDEN, "API key has expired"),
            super::AuthError::KeySuspended => 
                (StatusCode::FORBIDDEN, "API key is suspended"),
            super::AuthError::InsufficientPermissions => 
                (StatusCode::FORBIDDEN, "Insufficient permissions"),
            super::AuthError::RateLimitExceeded => 
                (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded"),
            super::AuthError::StorageError(_) => 
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
        };

        return Err((
            status,
            axum::Json(HttpResponse::<String>::new(
                status.as_u16() as i32,
                message.to_string(),
                "".to_string(),
            )),
        ));
    }

    Ok(next.run(req).await)
} 