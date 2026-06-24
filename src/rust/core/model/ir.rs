//! Lightweight unified IR owned by RepoGrammar.
//!
//! Tree-sitter AST nodes are intentionally not exposed here.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IrNodeId(String);

impl IrNodeId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("IR node id must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrNode {
    pub id: IrNodeId,
    pub kind: String,
    pub children: Vec<IrNodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrEdge {
    pub from: IrNodeId,
    pub to: IrNodeId,
    pub label: String,
}
