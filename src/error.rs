// file_path: src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HiveDropError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Crypto error: {0}")]
    CryptoError(String),

    #[error("RSA error: {0}")]
    RsaError(#[from] rsa::Error),

    #[error("PKCS8 error: {0}")]
    Pkcs8Error(#[from] rsa::pkcs8::Error),

    #[error("SPKI error: {0}")]
    SpkiError(#[from] rsa::pkcs8::spki::Error),

    #[error("HTTP service error: {0}")]
    ServiceError(String),

    #[error("Certificate generation error: {0}")]
    RcgenError(#[from] rcgen::RcgenError),

    #[error("TLS error: {0}")]
    TlsError(#[from] rustls::Error),

    #[error("Transfer request error: {0}")]
    TransferRequestError(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Request not found: {0}")]
    RequestNotFound(String),

    #[error("Resource not found: {0}")]
    NotFoundError(String),

    #[error("Transfer status error: {0}")]
    TransferStatusError(String),

    #[error("Empty transfer list")]
    EmptyTransferList,

    #[error("Invalid transfer request: {0}")]
    ValidationError(String),

    #[error("HTTP body error: {0}")]
    HttpBodyError(String),

    #[error("{0}")]
    GenericError(String),
}

impl From<&str> for HiveDropError {
    fn from(error: &str) -> Self {
        Self::GenericError(error.to_string())
    }
}

// 添加reqwest错误的转换实现
impl From<reqwest::Error> for HiveDropError {
    fn from(error: reqwest::Error) -> Self {
        Self::NetworkError(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, HiveDropError>;
