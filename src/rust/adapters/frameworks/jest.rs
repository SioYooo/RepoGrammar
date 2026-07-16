use super::FrameworkAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JestAdapter;

impl FrameworkAdapter for JestAdapter {
    fn framework_name(&self) -> &'static str {
        "jest"
    }
}
