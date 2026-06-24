use super::FrameworkAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpressAdapter;

impl FrameworkAdapter for ExpressAdapter {
    fn framework_name(&self) -> &'static str {
        "express"
    }
}
