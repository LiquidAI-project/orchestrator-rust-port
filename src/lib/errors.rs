use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde_json::json;




#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub msg: String,
}
impl ApiError {
    pub fn bad_request(e: impl std::fmt::Display) -> Self {
        Self { status: StatusCode::BAD_REQUEST, msg: format!("bad request: {e}") }
    }
    pub fn not_found(e: impl std::fmt::Display) -> Self {
        Self { status: StatusCode::NOT_FOUND, msg: format!("not found: {e}") }
    }
    pub fn internal_error(e: impl std::fmt::Display) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, msg: format!("internal server error: {e}") }
    }
    pub fn db(e: impl std::fmt::Display) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, msg: format!("db error: {e}") }
    }
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.status, self.msg)
    }
}
impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode { self.status }
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status).json(json!({ "error": self.msg }))
    }
}