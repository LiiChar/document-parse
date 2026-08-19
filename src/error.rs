#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("unsupported format")]
    UnsupportedFormat,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid document: {0}")]
    InvalidDocument(String),

    #[error("parser error: {0}")]
    Parser(String),
}
