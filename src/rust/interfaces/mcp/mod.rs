//! Transport-neutral MCP contract placeholders.
//!
//! The bootstrap records tool names and boundary intent without implementing an
//! MCP server or transport schema.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpToolName {
    FindAnalogues,
    ShowFamily,
    ExplainDeviation,
    CheckConformance,
}

impl McpToolName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FindAnalogues => "find_analogues",
            Self::ShowFamily => "show_family",
            Self::ExplainDeviation => "explain_deviation",
            Self::CheckConformance => "check_conformance",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_match_bootstrap_contract() {
        assert_eq!(McpToolName::FindAnalogues.as_str(), "find_analogues");
        assert_eq!(McpToolName::ShowFamily.as_str(), "show_family");
        assert_eq!(McpToolName::ExplainDeviation.as_str(), "explain_deviation");
        assert_eq!(McpToolName::CheckConformance.as_str(), "check_conformance");
    }
}
