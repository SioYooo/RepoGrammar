use thiserror::Error;

#[derive(Error, Debug)]
pub enum CatalogError {
    #[error("catalog item missing")]
    Missing,
    #[error("catalog item invalid: {0}")]
    Invalid(String),
}

#[derive(Error, Debug)]
pub enum PageError {
    #[error("page out of range")]
    OutOfRange,
    #[error("page decode failed: {0}")]
    Decode(String),
}

#[derive(Error, Debug)]
pub enum FilterError {
    #[error("filter unsupported")]
    Unsupported,
    #[error("filter parse failed: {0}")]
    Parse(String),
}
