use super::FrameworkAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VitestAdapter;

impl FrameworkAdapter for VitestAdapter {
    fn framework_name(&self) -> &'static str {
        "vitest"
    }
}
