//! Anti-unification will derive canonical templates and variation slots.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariationSlot {
    pub name: String,
    pub allowed_shapes: Vec<String>,
}
