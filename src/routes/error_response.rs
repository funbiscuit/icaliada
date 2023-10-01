use askama_axum::IntoResponse;
use hyper::StatusCode;
use serde_json::json;

pub type ApiResult<T> = Result<T, ApiError>;

//todo custom debug to show cause chain
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),

    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> askama_axum::Response {
        tracing::error!("{:?}", self);

        let message = self.to_message();
        let body = json!({
            "status": "error",
            "error": message,
        });
        let mut res = axum::Json(body).into_response();
        *res.status_mut() = self.status_code();
        res
    }
}

impl ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn to_message(&self) -> String {
        match self {
            ApiError::NotFound(err) => err.clone(),
            ApiError::Unexpected(err) => err.to_string(),
        }
    }
}
