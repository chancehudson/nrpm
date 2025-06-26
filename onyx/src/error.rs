use axum::extract::multipart::MultipartError;
use axum::http::StatusCode;
use axum::response::IntoResponse;

#[derive(Clone, Default)]
pub struct OnyxError {
    message: Option<String>,
    status_code: StatusCode,
}

impl OnyxError {
    pub fn bad_request(message: &str) -> Self {
        Self {
            message: Some(message.to_string()),
            status_code: StatusCode::BAD_REQUEST,
        }
    }
}

macro_rules! impl_error_from {
    ($error_type:ty) => {
        impl From<$error_type> for OnyxError {
            fn from(value: $error_type) -> Self {
                Self {
                    message: Some(value.to_string()),
                    status_code: StatusCode::INTERNAL_SERVER_ERROR,
                }
            }
        }
    };
}

impl_error_from!(redb::StorageError);
impl_error_from!(redb::TransactionError);
impl_error_from!(redb::TableError);
impl_error_from!(redb::CommitError);
impl_error_from!(bcrypt::BcryptError);
impl_error_from!(blake3::HexError);
impl_error_from!(Box<bincode::ErrorKind>);

impl From<std::io::Error> for OnyxError {
    fn from(value: std::io::Error) -> Self {
        Self {
            message: Some(format!("Uncaught io error: {:?}", value.to_string())),
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
impl From<MultipartError> for OnyxError {
    fn from(value: MultipartError) -> Self {
        Self {
            message: Some(format!(
                "Error in multipart request: {:?}",
                value.to_string()
            )),
            status_code: StatusCode::BAD_REQUEST,
        }
    }
}

impl From<anyhow::Error> for OnyxError {
    fn from(value: anyhow::Error) -> Self {
        Self {
            message: Some(value.to_string()),
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<StatusCode> for OnyxError {
    fn from(value: StatusCode) -> Self {
        Self {
            message: None,
            status_code: value,
        }
    }
}

impl IntoResponse for OnyxError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status_code,
            self.message
                .unwrap_or("Unknown error ocurred in Onyx system".to_string()),
        )
            .into_response()
    }
}
