use super::unknown::{self, RustUnknownSpec};
use crate::core::model::{CodeUnit, SemanticFact};
use crate::ports::parser::{ParseError, SourceDocument};

pub(super) fn macro_unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
    kind: &'static str,
) -> Result<SemanticFact, ParseError> {
    unknown::fact(
        document,
        unit,
        start_byte,
        end_byte,
        RustUnknownSpec {
            reason: "MacroOrPreprocessor",
            affected_claim: "rust_macro_expansion",
            kind,
            note: "Rust macro syntax is not expanded",
        },
    )
}

pub(super) fn unit_unknowns(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    slice: &str,
) -> Result<Vec<SemanticFact>, ParseError> {
    let mut facts = Vec::new();
    if slice.contains("#[cfg(") || slice.contains("#[cfg_attr(") {
        facts.push(unknown::fact(
            document,
            unit,
            unit.range.start_byte,
            unit.range.end_byte,
            RustUnknownSpec {
                reason: "BuildVariantAmbiguity",
                affected_claim: "rust_build_variant",
                kind: "cfg_attribute",
                note: "Rust cfg/cfg_attr build variant is not evaluated",
            },
        )?);
    }
    if slice.contains("#[proc_macro")
        || slice.contains("#[proc_macro_attribute")
        || slice.contains("#[proc_macro_derive")
    {
        facts.push(unknown::fact(
            document,
            unit,
            unit.range.start_byte,
            unit.range.end_byte,
            RustUnknownSpec {
                reason: "MacroOrPreprocessor",
                affected_claim: "rust_macro_expansion",
                kind: "proc_macro_attribute",
                note: "Rust procedural macro attribute is not expanded",
            },
        )?);
    }
    if slice.contains("dyn ") || slice.contains("Box<dyn") || slice.contains("Arc<dyn") {
        facts.push(unknown::fact(
            document,
            unit,
            unit.range.start_byte,
            unit.range.end_byte,
            RustUnknownSpec {
                reason: "FrameworkMagic",
                affected_claim: "rust_trait_dispatch",
                kind: "trait_dispatch",
                note: "Rust trait object dispatch is not resolved",
            },
        )?);
    }
    Ok(facts)
}
