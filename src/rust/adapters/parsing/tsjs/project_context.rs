use crate::ports::parser::ParserProjectContext;

pub(super) fn has_package(context: &ParserProjectContext, package: &str) -> bool {
    context
        .tsjs_package_dependencies
        .iter()
        .any(|dependency| dependency == package)
}
