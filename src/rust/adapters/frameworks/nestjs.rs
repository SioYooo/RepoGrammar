use super::FrameworkAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NestJsAdapter;

impl FrameworkAdapter for NestJsAdapter {
    fn framework_name(&self) -> &'static str {
        "nestjs"
    }
}
