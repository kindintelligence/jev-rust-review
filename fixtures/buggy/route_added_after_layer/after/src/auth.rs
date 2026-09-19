use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

pub async fn require_admin(request: Request, next: Next) -> Result<Response, StatusCode> {
    let is_admin = request
        .headers()
        .get("x-role")
        .is_some_and(|role| role.as_bytes() == b"admin");
    if is_admin {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}
