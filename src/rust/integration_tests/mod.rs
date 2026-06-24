use crate::{
    interfaces::{cli, mcp::McpToolName},
    test_support::TempWorkspace,
};

#[test]
fn bootstrap_interfaces_are_reachable() {
    assert_eq!(cli::run(["--version"]).status, 0);
    assert_eq!(McpToolName::FindAnalogues.as_str(), "find_analogues");
}

#[test]
fn test_support_workspace_creates_real_temp_directory() {
    let workspace = TempWorkspace::new("integration");

    assert!(workspace.path().is_dir());
}
