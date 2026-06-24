//! Clustering groups aligned candidates into pattern families.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClusterId(String);

impl ClusterId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("cluster id must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }
}
