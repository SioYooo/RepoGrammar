//! Structural fingerprints provide cheap candidate grouping before alignment.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructuralFingerprint(String);

impl StructuralFingerprint {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("structural fingerprint must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }
}
