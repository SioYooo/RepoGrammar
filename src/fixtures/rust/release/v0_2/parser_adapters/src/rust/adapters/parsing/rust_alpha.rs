use crate::ports::parser::{ParseReport, SourceDocument, SourceParser};

pub struct AlphaParser;

impl SourceParser for AlphaParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, String> {
        parse_document(document)?;
        parse_report(document)
    }
}

pub fn parse_alpha(document: SourceDocument<'_>) -> Result<ParseReport, String> {
    parse_document(document)?;
    parse_report(document)
}

pub fn parse_beta(document: SourceDocument<'_>) -> Result<ParseReport, String> {
    parse_document(document)?;
    parse_report(document)
}

pub fn parse_gamma(document: SourceDocument<'_>) -> Result<ParseReport, String> {
    parse_document(document)?;
    parse_report(document)
}

fn parse_document(_document: SourceDocument<'_>) -> Result<(), String> {
    Ok(())
}

fn parse_report(_document: SourceDocument<'_>) -> Result<ParseReport, String> {
    Err("fixture report omitted".to_string())
}
