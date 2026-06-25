//! Framework-role detection port.

use crate::core::model::{CodeUnit, SemanticFact};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameworkRoleError {
    InvalidFact(String),
}

pub trait FrameworkRoleDetector {
    fn detect_roles(&self, units: &[CodeUnit]) -> Result<Vec<SemanticFact>, FrameworkRoleError>;
}
