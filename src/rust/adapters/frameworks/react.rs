use super::FrameworkAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReactAdapter;

impl FrameworkAdapter for ReactAdapter {
    fn framework_name(&self) -> &'static str {
        "react"
    }
}
