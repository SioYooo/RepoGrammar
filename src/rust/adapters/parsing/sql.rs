//! Bounded SQL DDL frontend for the ADR-0040 dialect-invariance set.
//!
//! The declared set is PostgreSQL 16 and SQLite 3. A construct is scanned only
//! when both members lex and nest it identically, so this frontend never selects
//! a dialect and never needs to: it claims only what holds under either member.
//! A construct the members disagree about moves token boundaries, so meeting one
//! degrades the whole file rather than one statement -- a diverged stream makes
//! every later statement boundary unproven.
//!
//! Nothing here connects to a database, runs a client, driver, or migration
//! tool, reads credentials, or executes, prepares, plans, or validates a
//! statement. No table, column, index, or other repository name reaches a code
//! unit id, fact target, note, or assumption: name identity folds differently
//! across the declared set, so it is exactly the dialect-dependent fact this
//! frontend has no authority to assert.

use super::{ir_edges_for_units, ir_nodes_for_units, sort_anchor_facts};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};

pub const SQL_ANCHOR_ENGINE: &str = "repogrammar-sql-ddl-scanner";
pub const SQL_ANCHOR_METHOD: &str = "bounded_pg16_sqlite3_invariant_ddl_v1";

/// Fixed support target for the one admitted exact anchor.
pub const SQL_CREATE_TABLE_TARGET: &str = "sql.ddl.create_table";

const MAX_STATEMENT_UNITS: usize = 4_096;
const MAX_FACTS: usize = 16_384;
const MAX_POST_SCAN_UNKNOWNS: usize = 3;

/// A construct the declared dialects lex differently.
///
/// Each moves token boundaries, so the statement split after it is unproven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexicalDivergence {
    /// Any `$`: `$tag$ ... $tag$` strings are PostgreSQL only, and the two
    /// members also spell `$` parameters differently.
    DollarSignConstruct,
    /// `E'...'` -- PostgreSQL only.
    EscapeString,
    /// Backtick-quoted token -- SQLite only.
    BacktickQuotedToken,
    /// Bracket-quoted token -- SQLite only; also PostgreSQL array-type syntax.
    BracketQuotedToken,
    /// `/*` inside an open block comment: PostgreSQL nests, SQLite does not.
    NestedBlockComment,
    /// A quoted token or block comment that never closes.
    UnterminatedToken,
}

impl LexicalDivergence {
    fn as_kind(self) -> &'static str {
        match self {
            Self::DollarSignConstruct => "dollar_sign_construct",
            Self::EscapeString => "escape_string_constant",
            Self::BacktickQuotedToken => "backtick_quoted_token",
            Self::BracketQuotedToken => "bracket_quoted_token",
            Self::NestedBlockComment => "nested_block_comment",
            Self::UnterminatedToken => "unterminated_token",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::DollarSignConstruct => {
                "dollar-quoted strings exist in PostgreSQL only and the members spell dollar parameters differently, so statement boundaries after one are unproven"
            }
            Self::EscapeString => {
                "C-style escape string constants exist in PostgreSQL only, so statement boundaries after one are unproven"
            }
            Self::BacktickQuotedToken => {
                "backtick-quoted tokens exist in SQLite only, so statement boundaries after one are unproven"
            }
            Self::BracketQuotedToken => {
                "bracket-quoted tokens are SQLite identifiers and PostgreSQL array syntax, so statement boundaries after one are unproven"
            }
            Self::NestedBlockComment => {
                "block comments nest in PostgreSQL but not in SQLite, so the two dialects resume the statement at different offsets"
            }
            Self::UnterminatedToken => {
                "a quoted token or block comment never closed, so no statement boundary in this file is proven"
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct SqlDdlParser;

impl SourceParser for SqlDdlParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        parse_output(document).map(|output| output.report)
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        self.parse(document)
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        parse_output(document)
    }
}

pub(crate) fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::Sql || !document.path.ends_with(".sql") {
        return Err(ParseError::UnsupportedLanguage);
    }
    let full_range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let module = CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#sql_module:0-{}:0",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Sql,
        kind: CodeUnitKind::Module,
        range: full_range.clone(),
        provenance: provenance.clone(),
    };

    // Every file keeps this: the admitted shapes do not depend on the dialect,
    // but catalog state, execution semantics, migration order, and extension
    // identity all do, and none of them is established here.
    let mut facts = vec![unknown_fact(
        &module,
        UnknownReasonCode::MissingProjectConfig,
        CLAIM_DIALECT_PROFILE,
        "unproven_dialect_profile",
        full_range.clone(),
        "a .sql path does not select a dialect; only constructs invariant across PostgreSQL 16 and SQLite 3 are scanned",
    )?];
    let mut units = vec![module.clone()];

    if document.text.len() > usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX) {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            CLAIM_STATEMENT_BOUNDARY,
            "source_byte_limit",
            full_range,
            "SQL source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts);
    }

    let statements = match split_top_level_statements(document.text) {
        Ok(statements) => statements,
        Err(divergence) => {
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::ConflictingFacts,
                CLAIM_STATEMENT_BOUNDARY,
                divergence.as_kind(),
                full_range,
                divergence.note(),
            )?);
            return finish(units, facts);
        }
    };

    let mut statement_limit_hit = false;
    for (index, statement) in statements.iter().enumerate() {
        if index >= MAX_STATEMENT_UNITS || facts.len() >= MAX_FACTS - MAX_POST_SCAN_UNKNOWNS {
            statement_limit_hit = true;
            break;
        }
        let range =
            SourceRange::new(statement.start, statement.end).map_err(ParseError::Internal)?;
        let is_create_table = statement_is_admitted_create_table(&statement.tokens);
        let kind = if is_create_table {
            CodeUnitKind::SqlTableDefinition
        } else {
            CodeUnitKind::SqlStatement
        };
        let unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#{}:{}-{}:{}",
                document.path,
                kind.as_str(),
                statement.start,
                statement.end,
                index
            ))
            .map_err(ParseError::Internal)?,
            language: Language::Sql,
            kind,
            range: range.clone(),
            provenance: provenance.clone(),
        };
        if is_create_table {
            facts.push(anchor_fact(
                &unit,
                SQL_CREATE_TABLE_TARGET,
                "sql_anchor_kind=create_table",
                "exact CREATE TABLE definition list, invariant across the declared dialect set",
            )?);
        } else {
            facts.push(unknown_fact(
                &unit,
                UnknownReasonCode::InsufficientSupport,
                CLAIM_STATEMENT_SHAPE,
                "unadmitted_statement_shape",
                range,
                "statement is bounded inventory only; its shape is outside the admitted exact-anchor subset",
            )?);
        }
        units.push(unit);
    }

    if statement_limit_hit {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            CLAIM_STATEMENT_BOUNDARY,
            "scanner_resource_limit",
            module.range.clone(),
            "SQL scanner exceeded a bounded statement or fact limit",
        )?);
    }

    finish(units, facts)
}

fn finish(
    mut units: Vec<CodeUnit>,
    mut facts: Vec<SemanticFact>,
) -> Result<SourceParseOutput, ParseError> {
    units.sort_by(|left, right| {
        (left.range.start_byte, left.range.end_byte, left.id.as_str()).cmp(&(
            right.range.start_byte,
            right.range.end_byte,
            right.id.as_str(),
        ))
    });
    sort_anchor_facts(&mut facts);
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    Ok(SourceParseOutput {
        report: ParseReport {
            units,
            ir_nodes,
            ir_edges,
            semantic_facts: facts,
            diagnostics: Vec::new(),
        },
        python_interface_hash: None,
        dependencies: Vec::new(),
    })
}

/// One top-level statement and the significant tokens it opens with.
struct Statement {
    start: usize,
    end: usize,
    tokens: Vec<Token>,
}

/// A significant token, reduced to what statement dispatch needs.
///
/// Quoted tokens and string constants collapse to [`Token::Opaque`]: both
/// members lex them as one token, and their contents are a repository name or
/// literal that must never leave this module.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Opaque,
    Punctuation(char),
}

/// Enough for the longest admitted opening: `CREATE TABLE IF NOT EXISTS`, a
/// qualified name, `(`, and the first token inside the definition list.
const MAX_DISPATCH_TOKENS: usize = 12;

/// Split on `;` outside strings, quoted tokens, comments, and parentheses.
///
/// Returns the first divergence instead of a split whenever one is met: after a
/// construct the declared dialects lex differently, no later boundary in the
/// file is proven.
fn split_top_level_statements(text: &str) -> Result<Vec<Statement>, LexicalDivergence> {
    let bytes = text.as_bytes();
    let mut statements = Vec::new();
    let mut tokens: Vec<Token> = Vec::new();
    let mut statement_start: Option<usize> = None;
    let mut statement_end = 0usize;
    let mut depth = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index = line_comment_end(bytes, index);
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = block_comment_end(bytes, index)?;
                continue;
            }
            b'\'' => {
                let end = single_quoted_end(bytes, index)?;
                mark(&mut statement_start, &mut statement_end, index, end);
                push_token(&mut tokens, Token::Opaque);
                index = end;
                continue;
            }
            b'"' => {
                let end = delimited_end(bytes, index, b'"')?;
                mark(&mut statement_start, &mut statement_end, index, end);
                push_token(&mut tokens, Token::Opaque);
                index = end;
                continue;
            }
            b'`' => return Err(LexicalDivergence::BacktickQuotedToken),
            b'[' => return Err(LexicalDivergence::BracketQuotedToken),
            b'$' => return Err(LexicalDivergence::DollarSignConstruct),
            b';' if depth == 0 => {
                if let Some(start) = statement_start.take() {
                    statements.push(Statement {
                        start,
                        end: index + 1,
                        tokens: std::mem::take(&mut tokens),
                    });
                } else {
                    tokens.clear();
                }
                index += 1;
                continue;
            }
            _ if byte.is_ascii_whitespace() => {
                index += 1;
                continue;
            }
            _ => {}
        }

        if byte.is_ascii_digit() {
            let end = numeric_end(bytes, index);
            mark(&mut statement_start, &mut statement_end, index, end);
            push_token(&mut tokens, Token::Opaque);
            index = end;
            continue;
        }

        if is_word_start(byte) {
            let end = word_end(bytes, index);
            let word = text[index..end].to_ascii_uppercase();
            if word == "E" && bytes.get(end) == Some(&b'\'') {
                return Err(LexicalDivergence::EscapeString);
            }
            mark(&mut statement_start, &mut statement_end, index, end);
            push_token(&mut tokens, Token::Word(word));
            index = end;
            continue;
        }

        if byte == b'(' {
            depth += 1;
        } else if byte == b')' {
            depth = depth.saturating_sub(1);
        }
        mark(&mut statement_start, &mut statement_end, index, index + 1);
        push_token(&mut tokens, Token::Punctuation(byte as char));
        index += 1;
    }

    if let Some(start) = statement_start {
        statements.push(Statement {
            start,
            end: statement_end,
            tokens,
        });
    }
    Ok(statements)
}

/// Record that significant text at `[start, end)` belongs to the open statement.
fn mark(statement_start: &mut Option<usize>, statement_end: &mut usize, start: usize, end: usize) {
    if statement_start.is_none() {
        *statement_start = Some(start);
    }
    *statement_end = end;
}

/// Keep only the leading tokens statement dispatch reads.
fn push_token(tokens: &mut Vec<Token>, token: Token) {
    if tokens.len() < MAX_DISPATCH_TOKENS {
        tokens.push(token);
    }
}

/// `CREATE TABLE [IF NOT EXISTS] <name> (` with a non-empty definition list.
///
/// Every other spelling, including `CREATE TEMP TABLE` and `CREATE TABLE … AS
/// SELECT`, is outside the admitted subset and yields no anchor.
fn statement_is_admitted_create_table(tokens: &[Token]) -> bool {
    let mut rest = match tokens {
        [Token::Word(create), Token::Word(table), rest @ ..]
            if create == "CREATE" && table == "TABLE" =>
        {
            rest
        }
        _ => return false,
    };
    if let [Token::Word(word_if), Token::Word(not), Token::Word(exists), tail @ ..] = rest {
        if word_if == "IF" && not == "NOT" && exists == "EXISTS" {
            rest = tail;
        }
    }
    let rest = match rest {
        [Token::Word(_) | Token::Opaque, tail @ ..] => tail,
        _ => return false,
    };
    // A qualified name is one more `.`-joined part; both members spell it the
    // same way, and neither part is read.
    let rest = match rest {
        [Token::Punctuation('.'), Token::Word(_) | Token::Opaque, tail @ ..] => tail,
        other => other,
    };
    matches!(
        rest,
        [Token::Punctuation('('), next, ..] if *next != Token::Punctuation(')')
    )
}

fn line_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

/// End of a non-nested `/* … */`. A `/*` inside it is a divergence.
fn block_comment_end(bytes: &[u8], start: usize) -> Result<usize, LexicalDivergence> {
    let mut index = start + 2;
    while index < bytes.len() {
        if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
            return Ok(index + 2);
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            return Err(LexicalDivergence::NestedBlockComment);
        }
        index += 1;
    }
    Err(LexicalDivergence::UnterminatedToken)
}

/// End of a `'…'` constant, where `''` embeds one quote in both members.
fn single_quoted_end(bytes: &[u8], start: usize) -> Result<usize, LexicalDivergence> {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\'' {
            if bytes.get(index + 1) == Some(&b'\'') {
                index += 2;
                continue;
            }
            return Ok(index + 1);
        }
        index += 1;
    }
    Err(LexicalDivergence::UnterminatedToken)
}

/// End of a `"…"` token, where a doubled delimiter embeds one in both members.
fn delimited_end(bytes: &[u8], start: usize, delimiter: u8) -> Result<usize, LexicalDivergence> {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == delimiter {
            if bytes.get(index + 1) == Some(&delimiter) {
                index += 2;
                continue;
            }
            return Ok(index + 1);
        }
        index += 1;
    }
    Err(LexicalDivergence::UnterminatedToken)
}

fn is_word_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic() || !byte.is_ascii()
}

fn numeric_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && (bytes[index] == b'.' || bytes[index].is_ascii_alphanumeric()) {
        index += 1;
    }
    index
}

fn word_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len()
        && (bytes[index] == b'_'
            || bytes[index].is_ascii_alphanumeric()
            || !bytes[index].is_ascii())
    {
        index += 1;
    }
    index
}

fn anchor_fact(
    unit: &CodeUnit,
    target: &str,
    assumption: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![assumption.to_string()],
    })
}

/// The claim one typed `UNKNOWN` scopes itself to.
///
/// The split is the ADR-0040 argument in the type system. A diverged token
/// stream leaves later statement boundaries unproven, so it blocks any claim
/// built on those boundaries. An unproven dialect does not: the admitted parse
/// is invariant across the declared set, so the dialect is recorded as a
/// standing subclaim rather than as a veto over the anchor it cannot change.
const CLAIM_STATEMENT_BOUNDARY: &str = "sql_statement_boundary";
const CLAIM_DIALECT_PROFILE: &str = "sql_dialect_profile";
const CLAIM_STATEMENT_SHAPE: &str = "sql_statement_shape";

fn unknown_fact(
    unit: &CodeUnit,
    reason: UnknownReasonCode,
    affected_claim: &str,
    kind: &str,
    range: SourceRange,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.as_protocol_str()).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(unit.id.clone(), range, unit.provenance.clone(), note)
            .map_err(ParseError::Internal)?,
        assumptions: vec![
            format!("affected_claim={affected_claim}"),
            format!("sql_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: SQL_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: SQL_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        SqlDdlParser
            .parse_with_context_output(
                SourceDocument {
                    path: "db/schema.sql",
                    language: Language::Sql,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse SQL")
    }

    fn anchors(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(SQL_CREATE_TABLE_TARGET)
            })
            .count()
    }

    fn unknown_kinds(parsed: &SourceParseOutput) -> Vec<String> {
        parsed
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| {
                assumption
                    .strip_prefix("sql_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    #[test]
    fn admitted_create_tables_anchor_and_other_statements_stay_inventory() {
        let parsed = output(concat!(
            "-- bootstrap\n",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL);\n",
            "CREATE TABLE IF NOT EXISTS audit.events (id INTEGER, payload TEXT);\n",
            "CREATE TABLE \"Orders\" (id INTEGER);\n",
            "CREATE INDEX users_email ON users (email);\n",
            "INSERT INTO users (id, email) VALUES (1, 'a@example.com');\n",
        ));

        assert_eq!(anchors(&parsed), 3);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::SqlTableDefinition)
                .count(),
            3
        );
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::SqlStatement)
                .count(),
            2
        );
        assert_eq!(
            unknown_kinds(&parsed)
                .iter()
                .filter(|kind| *kind == "unadmitted_statement_shape")
                .count(),
            2
        );
        assert!(unknown_kinds(&parsed).contains(&"unproven_dialect_profile".to_string()));
    }

    #[test]
    fn no_repository_name_or_literal_reaches_any_output() {
        let parsed =
            output("CREATE TABLE customers (secret_column TEXT DEFAULT 'super-secret-token');\n");

        let rendered = format!("{parsed:?}");
        for leaked in ["customers", "secret_column", "super-secret-token"] {
            assert!(!rendered.contains(leaked), "{leaked} leaked into output");
        }
        assert_eq!(anchors(&parsed), 1);
    }

    #[test]
    fn create_table_text_inside_a_comment_or_string_never_anchors() {
        let commented = output(concat!(
            "-- CREATE TABLE ghost (id INTEGER);\n",
            "/* CREATE TABLE phantom (id INTEGER); */\n",
        ));
        assert_eq!(anchors(&commented), 0);
        assert!(commented
            .report
            .units
            .iter()
            .all(|unit| unit.kind == CodeUnitKind::Module));

        let quoted =
            output("INSERT INTO log (body) VALUES ('CREATE TABLE ghost (id INTEGER);');\n");
        assert_eq!(anchors(&quoted), 0);
    }

    #[test]
    fn each_divergent_construct_degrades_the_whole_file_without_anchors() {
        for (text, expected) in [
            (
                "CREATE TABLE a (id INTEGER);\nCREATE FUNCTION f() RETURNS INTEGER AS $$ SELECT 1 $$;\n",
                "dollar_sign_construct",
            ),
            (
                "CREATE TABLE a (id INTEGER);\nINSERT INTO a VALUES (E'\\n');\n",
                "escape_string_constant",
            ),
            (
                "CREATE TABLE `a` (id INTEGER);\n",
                "backtick_quoted_token",
            ),
            (
                "CREATE TABLE a (tags TEXT[]);\n",
                "bracket_quoted_token",
            ),
            (
                "/* outer /* inner */ */\nCREATE TABLE a (id INTEGER);\n",
                "nested_block_comment",
            ),
            (
                "CREATE TABLE a (id INTEGER);\n/* never closed\n",
                "unterminated_token",
            ),
            (
                "CREATE TABLE a (id INTEGER);\nINSERT INTO a VALUES ('never closed\n",
                "unterminated_token",
            ),
        ] {
            let parsed = output(text);
            assert_eq!(anchors(&parsed), 0, "{expected}: anchors survived divergence");
            assert_eq!(
                parsed.report.units.len(),
                1,
                "{expected}: only the module unit may survive divergence"
            );
            assert!(
                unknown_kinds(&parsed).contains(&expected.to_string()),
                "{expected}: got {:?}",
                unknown_kinds(&parsed)
            );
            assert!(parsed.report.semantic_facts.iter().any(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some("ConflictingFacts")
            }));
        }
    }

    #[test]
    fn unadmitted_create_table_spellings_do_not_anchor() {
        for text in [
            "CREATE TEMP TABLE a (id INTEGER);\n",
            "CREATE TEMPORARY TABLE a (id INTEGER);\n",
            "CREATE TABLE a AS SELECT 1;\n",
            "CREATE TABLE a ();\n",
            "CREATE TABLE;\n",
            "CREATE VIEW a AS SELECT 1;\n",
            "ALTER TABLE a ADD COLUMN b TEXT;\n",
        ] {
            let parsed = output(text);
            assert_eq!(anchors(&parsed), 0, "{text:?} must not anchor");
        }
    }

    #[test]
    fn semicolons_inside_strings_comments_and_parens_do_not_split_statements() {
        let parsed = output(concat!(
            "CREATE TABLE a (\n",
            "  id INTEGER, -- ; not a boundary\n",
            "  label TEXT DEFAULT 'a;b',\n",
            "  note TEXT /* ; not a boundary */\n",
            ");\n",
        ));

        assert_eq!(anchors(&parsed), 1);
        assert_eq!(parsed.report.units.len(), 2);
    }

    #[test]
    fn every_fact_stays_below_family_supporting_certainty_at_the_frontend() {
        let parsed = output("CREATE TABLE a (id INTEGER);\n");

        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn statement_and_fact_limits_are_bounded() {
        let many = "CREATE TABLE a (id INTEGER);\n".repeat(MAX_STATEMENT_UNITS + 1);
        let parsed = output(&many);

        assert_eq!(parsed.report.units.len(), MAX_STATEMENT_UNITS + 1);
        assert!(unknown_kinds(&parsed).contains(&"scanner_resource_limit".to_string()));
        assert!(parsed.report.semantic_facts.len() <= MAX_FACTS);
    }

    #[test]
    fn ir_contains_edges_link_statements_to_their_module() {
        let parsed = output("CREATE TABLE a (id INTEGER);\nSELECT 1;\n");

        assert_eq!(parsed.report.ir_nodes.len(), 3);
        assert_eq!(parsed.report.ir_edges.len(), 2);
    }

    #[test]
    fn parser_rejects_other_languages_and_suffixes() {
        let parser = SqlDdlParser;
        let document = SourceDocument {
            path: "db/schema.sql",
            language: Language::R,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text: "CREATE TABLE a (id INTEGER);\n",
        };
        assert_eq!(
            parser.parse(document.clone()),
            Err(ParseError::UnsupportedLanguage)
        );
        assert_eq!(
            parser.parse(SourceDocument {
                path: "db/schema.SQL",
                language: Language::Sql,
                ..document
            }),
            Err(ParseError::UnsupportedLanguage)
        );
    }
}
