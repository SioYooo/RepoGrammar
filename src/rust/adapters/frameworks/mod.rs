//! Framework adapters own framework-specific recognition rules.

pub mod express;
pub mod jest;
pub mod nestjs;
pub mod react;
pub mod vitest;

pub trait FrameworkAdapter {
    fn framework_name(&self) -> &'static str;
}
