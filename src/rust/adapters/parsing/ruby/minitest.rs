//! Bounded Ruby Minitest frontend for the ADR-0049 admitted shape.
//!
//! Only runner-scoped test paths reach this parser: a `test/` or `tests/`
//! component plus a `test_*.rb` or `*_test.rb` basename, the shapes Minitest
//! conventions execute. Every other `.rb` byte stays inventory and is never
//! read.
//!
//! The frontend is a real lexer (see [`super::lexer`]) plus a bounded
//! structure parser that answers exactly one class of question: which tokens
//! are statement boundaries, which keywords open `end`-regions, and where
//! `def` and `class` headers begin and end. Statements are otherwise opaque,
//! so nothing that text can fake inside a string, comment, heredoc, regex, or
//! `%`-literal can become an anchor.
//!
//! The anchor is the ADR-0049 D2 shape: a top-level class whose in-file
//! superclass chain ends at `Minitest::Test` or `ActiveSupport::TestCase`
//! (bounded dotted-name resolution, D3), and directly in that class body an
//! instance `def test_*` with no parameters. The framework identity is the
//! superclass itself — the honest analogue of the R lane's
//! DESCRIPTION-declares-testthat gate — and the top-level `require` line is
//! recorded as one bounded context token on the class anchor (D3a), never as
//! a retained literal.
//!
//! Outside the declared subset the file abstains whole-file with a typed
//! refusal shaped like the MATLAB lane's; a structural failure of the
//! end-matching grammar keeps only the declarations completed before the
//! failure point, the same guarantee ADR-0042 D4 and ADR-0046 give. The
//! parser never resynchronizes past either boundary, so no anchor after one
//! can be invented. Nothing here executes Ruby, Bundler, RubyGems, Rake,
//! Rails, or any test.

use super::super::{ir_edges_for_units, ir_nodes_for_units, sort_anchor_facts};
use super::lexer::{self, Token, TokenKind};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};

pub const RUBY_ANCHOR_ENGINE: &str = "repogrammar-ruby-minitest-parser";
pub const RUBY_ANCHOR_METHOD: &str = "bounded_ruby_minitest_v1";

/// Fixed family target for the one admitted exact anchor. This is the
/// `ruby.minitest.test_method` token ADR-0022 D1 reserved.
pub const RUBY_MINITEST_TARGET: &str = "ruby.minitest.test_method";

/// Context target for the admitted class anchor. The family claim rides the
/// method target; the class is identity evidence, not a second family.
pub const RUBY_MINITEST_CLASS_TARGET: &str = "ruby.minitest.test_class";

const MINITEST_BASE: &str = "Minitest::Test";
const ACTIVE_SUPPORT_BASE: &str = "ActiveSupport::TestCase";
/// Bound on superclass-chain hops inside one file (ADR-0049 D3).
const MAX_SUPERCLASS_HOPS: usize = 8;
const MAX_UNITS: usize = 4_096;
/// Bound on region nesting. Repository contents are untrusted, and a file of
/// ten thousand nested `begin`s must degrade, not overflow the stack.
const MAX_REGION_DEPTH: usize = 256;

/// True for the only paths this frontend may read.
pub fn is_minitest_path(path: &str) -> bool {
    let mut components = path.split('/').collect::<Vec<_>>();
    let Some(name) = components.pop() else {
        return false;
    };
    if !(name.ends_with(".rb")
        && (name.starts_with("test_") || (name.ends_with("_test.rb") && name != "_test.rb")))
    {
        return false;
    }
    components
        .iter()
        .any(|component| matches!(*component, "test" | "tests"))
}

/// The bounded `require` context of ADR-0049 D3a: one low-cardinality token,
/// never the literal argument text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequireContext {
    Autorun,
    TestUnit,
    LiteralOther,
    NonLiteral,
    Absent,
}

impl RequireContext {
    fn assumption(self) -> String {
        let token = match self {
            Self::Autorun => "autorun",
            Self::TestUnit => "test_unit",
            Self::LiteralOther => "literal_other",
            Self::NonLiteral => "non_literal",
            Self::Absent => "absent",
        };
        format!("minitest_require={token}")
    }
}

#[derive(Debug, Default)]
pub struct RubyMinitestParser;

impl SourceParser for RubyMinitestParser {
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
    if document.language != Language::Ruby || !is_minitest_path(document.path) {
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
            "unit:{}#ruby_file:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Ruby,
        kind: CodeUnitKind::Module,
        range: full_range.clone(),
        provenance: provenance.clone(),
    };
    let mut units = vec![module.clone()];
    let mut facts = Vec::new();
    let mut diagnostics = Vec::new();

    if document.text.len() > usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX) {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            "ruby_parse",
            "source_byte_limit",
            full_range,
            "Ruby source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts, diagnostics);
    }

    let lexed = lexer::lex(document.text);
    if let Some(abstention) = lexed.abstention {
        // A boundary the lexer cannot find exactly unproves every later
        // `def`/`class`/`end` extent, so the whole file abstains.
        facts.push(unknown_fact(
            &module,
            abstention.reason(),
            abstention.claim(),
            abstention.unknown_kind(),
            module.range.clone(),
            abstention.note(),
        )?);
        diagnostics.push(degraded(document.path, abstention.note()));
        return finish(units, facts, diagnostics);
    }

    let parsed = parse_tokens(document.text, &lexed.tokens);
    match parsed.halt {
        Some(ParseHalt::Outside) => {
            // A construct outside the declared subset: every anchor the file
            // might have carried is dropped, not partially kept.
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::InsufficientSupport,
                "ruby_parse",
                "unadmitted_ruby_construct",
                module.range.clone(),
                "a Ruby construct outside the declared subset ends the parse, so the declarations after it are unread and earlier anchors are dropped",
            )?);
            diagnostics.push(degraded(
                document.path,
                "a Ruby construct outside the declared subset ends the parse, so the declarations after it are unread",
            ));
            return finish(units, facts, diagnostics);
        }
        Some(ParseHalt::Structural) => {
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::InsufficientSupport,
                "ruby_parse",
                "ruby_structural_failure",
                module.range.clone(),
                "Ruby block structure does not close, so the declarations after the failure point were never parsed",
            )?);
            diagnostics.push(degraded(
                document.path,
                "Ruby block structure does not close, so the declarations after the failure point were never parsed",
            ));
        }
        Some(ParseHalt::Depth) => {
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::InsufficientSupport,
                "ruby_parse",
                "parser_depth_limit",
                module.range.clone(),
                "Ruby nesting exceeded the bounded parse depth, so the declarations after it are unread",
            )?);
            diagnostics.push(degraded(
                document.path,
                "Ruby nesting exceeded the bounded parse depth, so the declarations after it are unread",
            ));
        }
        None => {}
    }

    // The bounded superclass resolution of ADR-0049 D3, over the class
    // headers whose regions actually closed.
    let mut unbound_test_methods = false;
    let mut qualifying = Vec::new();
    for (index, class) in parsed.classes.iter().enumerate() {
        if !class.completed || !class.top_level {
            continue;
        }
        if superclass_reaches_minitest(&class.name, &parsed.classes) {
            qualifying.push(index);
        } else if class.has_test_shape_method {
            unbound_test_methods = true;
        }
    }

    let require_context = parsed
        .require_context
        .unwrap_or(RequireContext::Absent)
        .assumption();

    let mut limit_hit = false;
    for index in qualifying {
        let class = &parsed.classes[index];
        if units.len() >= MAX_UNITS {
            limit_hit = true;
            break;
        }
        let class_unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#ruby_minitest_test_class:{}-{}",
                document.path, class.start, class.end
            ))
            .map_err(ParseError::Internal)?,
            language: Language::Ruby,
            kind: CodeUnitKind::RubyMinitestTestClass,
            range: SourceRange::new(class.start, class.end).map_err(ParseError::Internal)?,
            provenance: provenance.clone(),
        };
        facts.push(anchor_fact(
            &class_unit,
            RUBY_MINITEST_CLASS_TARGET,
            "ruby_anchor_kind=minitest_test_class",
            "bounded Ruby Minitest test class anchor",
            &require_context,
        )?);
        units.push(class_unit);
    }
    for method in &parsed.methods {
        let class_qualifies = parsed.classes.get(method.class_index).is_some_and(|class| {
            class.completed
                && class.top_level
                && superclass_reaches_minitest(&class.name, &parsed.classes)
        });
        if !class_qualifies || !method.is_test_shape() {
            continue;
        }
        if units.len() >= MAX_UNITS {
            limit_hit = true;
            break;
        }
        let method_unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#ruby_minitest_test_method:{}-{}",
                document.path, method.start, method.end
            ))
            .map_err(ParseError::Internal)?,
            language: Language::Ruby,
            kind: CodeUnitKind::RubyMinitestTestMethod,
            range: SourceRange::new(method.start, method.end).map_err(ParseError::Internal)?,
            provenance: provenance.clone(),
        };
        facts.push(anchor_fact(
            &method_unit,
            RUBY_MINITEST_TARGET,
            "ruby_anchor_kind=minitest_test_method",
            "bounded Ruby Minitest test method anchor",
            "provider_resolved=false",
        )?);
        units.push(method_unit);
    }

    if unbound_test_methods {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::UnresolvedImport,
            "ruby_minitest_superclass",
            "test_methods_without_minitest_base",
            module.range.clone(),
            "test-shaped instance methods appear in a class whose in-file superclass chain does not reach Minitest::Test or ActiveSupport::TestCase, so the framework identity is unproven",
        )?);
    }

    if limit_hit {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            "ruby_parse",
            "parser_resource_limit",
            module.range.clone(),
            "Ruby parser exceeded the bounded unit limit",
        )?);
    }

    finish(units, facts, diagnostics)
}

fn degraded(path: &str, message: &str) -> ParseDiagnostic {
    ParseDiagnostic {
        path: path.to_string(),
        range: None,
        severity: ParseDiagnosticSeverity::Error,
        message: message.to_string(),
    }
}

fn finish(
    mut units: Vec<CodeUnit>,
    mut facts: Vec<SemanticFact>,
    diagnostics: Vec<ParseDiagnostic>,
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
            diagnostics,
        },
        python_interface_hash: None,
        dependencies: Vec::new(),
    })
}

fn anchor_fact(
    unit: &CodeUnit,
    target: &str,
    assumption: &str,
    note: &str,
    extra_assumption: &str,
) -> Result<SemanticFact, ParseError> {
    let mut assumptions = vec![
        "provider_resolved=false".to_string(),
        assumption.to_string(),
    ];
    if extra_assumption != "provider_resolved=false" {
        assumptions.push(extra_assumption.to_string());
    }
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
        assumptions,
    })
}

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
            format!("ruby_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: RUBY_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: RUBY_ANCHOR_METHOD.to_string(),
    }
}

/// Whether a top-level class's in-file superclass chain terminates at a
/// Minitest base within the bounded hop count (ADR-0049 D3).
fn superclass_reaches_minitest(name: &str, classes: &[ClassDecl]) -> bool {
    let mut current = name;
    for _ in 0..=MAX_SUPERCLASS_HOPS {
        let Some(decl) = classes
            .iter()
            .find(|class| class.completed && class.top_level && class.name == current)
        else {
            return false;
        };
        let Some(super_path) = &decl.super_path else {
            return false;
        };
        if super_path == MINITEST_BASE || super_path == ACTIVE_SUPPORT_BASE {
            return true;
        }
        if super_path.contains("::") {
            return false;
        }
        current = super_path;
    }
    false
}

// ---------------------------------------------------------------------------
// Structure parser
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ClassDecl {
    /// The full dotted constant path of the header.
    name: String,
    super_path: Option<String>,
    start: usize,
    end: usize,
    /// Whether the `class` header was a direct statement of the program body.
    top_level: bool,
    /// Whether the body region closed; only completed declarations anchor.
    completed: bool,
    /// Whether the body directly declares a `def test_*` instance method with
    /// no parameters — used only to type the unbound-superclass observation.
    has_test_shape_method: bool,
}

#[derive(Debug)]
struct MethodDecl {
    class_index: usize,
    name: String,
    is_self: bool,
    zero_params: bool,
    start: usize,
    end: usize,
}

impl MethodDecl {
    fn is_test_shape(&self) -> bool {
        !self.is_self && self.zero_params && self.name.starts_with("test_") && self.name.len() > 5
    }
}

/// Why the descent stopped. `Outside` drops every anchor; the other two keep
/// the declarations completed before them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseHalt {
    Outside,
    Structural,
    Depth,
}

#[derive(Debug, Default)]
struct FileParse {
    classes: Vec<ClassDecl>,
    methods: Vec<MethodDecl>,
    require_context: Option<RequireContext>,
    halt: Option<ParseHalt>,
}

/// The recording context for a `def` or `class` header.
enum Frame {
    Program,
    Class(usize),
    Other,
}

struct Parser<'a> {
    text: &'a str,
    tokens: &'a [Token],
    index: usize,
    depth: usize,
    out: FileParse,
}

fn parse_tokens(text: &str, tokens: &[Token]) -> FileParse {
    let mut parser = Parser {
        text,
        tokens,
        index: 0,
        depth: 0,
        out: FileParse::default(),
    };
    if let Err(halt) = parser.parse_region(&Frame::Program) {
        if parser.out.halt.is_none() {
            parser.out.halt = Some(halt);
        }
    }
    parser.out
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.index).copied()
    }

    fn word(&self, token: Token) -> &'a str {
        &self.text[token.start..token.end]
    }

    fn peek_word(&self) -> Option<&'a str> {
        self.peek().map(|token| self.word(token))
    }

    fn is_op(&self, token: Token, spelling: &str) -> bool {
        token.kind == TokenKind::Op && self.word(token) == spelling
    }

    fn skip_separators(&mut self) {
        while matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Newline | TokenKind::Semicolon,
                ..
            })
        ) {
            self.index += 1;
        }
    }

    /// Whether the upcoming token is a keyword that opens an `end`-region.
    fn block_opener_ahead(&self) -> Option<&'a str> {
        let token = self.peek()?;
        if token.kind != TokenKind::Keyword {
            return None;
        }
        let word = self.word(token);
        matches!(
            word,
            "def"
                | "class"
                | "module"
                | "begin"
                | "if"
                | "unless"
                | "while"
                | "until"
                | "case"
                | "for"
        )
        .then_some(word)
    }

    fn enter(&mut self) -> Result<(), ParseHalt> {
        self.depth += 1;
        if self.depth > MAX_REGION_DEPTH {
            return Err(ParseHalt::Depth);
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// One `end`-terminated (or program-level) statement region.
    fn parse_region(&mut self, frame: &Frame) -> Result<(), ParseHalt> {
        self.enter()?;
        let result = self.region_body(frame);
        self.leave();
        result
    }

    fn region_body(&mut self, frame: &Frame) -> Result<(), ParseHalt> {
        loop {
            self.skip_separators();
            let Some(token) = self.peek() else {
                return match frame {
                    Frame::Program => Ok(()),
                    _ => Err(ParseHalt::Structural),
                };
            };
            match token.kind {
                TokenKind::Keyword => match self.word(token) {
                    "end" => {
                        if matches!(frame, Frame::Program) {
                            // An `end` with nothing to close.
                            return Err(ParseHalt::Structural);
                        }
                        self.index += 1;
                        return Ok(());
                    }
                    // Clause markers of the enclosing construct: skip and keep
                    // reading the region.
                    "else" | "elsif" | "rescue" | "ensure" | "when" | "then" | "in" => {
                        if matches!(frame, Frame::Program) {
                            return Err(ParseHalt::Structural);
                        }
                        self.index += 1;
                    }
                    _ => {
                        if let Some(opener) = self.block_opener_ahead() {
                            self.parse_opener(opener, frame)?;
                        } else {
                            self.consume_statement(frame)?;
                        }
                    }
                },
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    // A closer with nothing open at region level.
                    return Err(ParseHalt::Structural);
                }
                _ => self.consume_statement(frame)?,
            }
        }
    }

    /// Dispatch an `end`-region opener. Only a `def`/`class` that is itself a
    /// statement of `frame` records against that frame; one reached
    /// mid-statement or inside a nested region never does.
    fn parse_opener(&mut self, opener: &str, frame: &Frame) -> Result<(), ParseHalt> {
        match opener {
            "def" => self.parse_def(frame),
            "class" => self.parse_class(frame),
            "module" => self.parse_module(),
            _ => self.parse_control_block(),
        }
    }

    /// `begin`/`if`/`unless`/`while`/`until`/`case`/`for`: the condition runs
    /// to a newline, `then`, `do`, or (`case` only) `when`, then the body is a
    /// nested region closed by `end`.
    fn parse_control_block(&mut self) -> Result<(), ParseHalt> {
        self.index += 1; // the opener keyword
        let mut bracket_depth = 0i64;
        loop {
            let Some(token) = self.peek() else {
                return Err(ParseHalt::Structural);
            };
            match token.kind {
                TokenKind::Newline | TokenKind::Semicolon if bracket_depth == 0 => break,
                TokenKind::Keyword if bracket_depth == 0 => match self.word(token) {
                    "then" | "do" | "when" => {
                        self.index += 1;
                        break;
                    }
                    _ => {
                        self.index += 1;
                    }
                },
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    bracket_depth += 1;
                    self.index += 1;
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    bracket_depth -= 1;
                    self.index += 1;
                }
                _ => self.index += 1,
            }
        }
        self.parse_region(&Frame::Other)
    }

    fn parse_def(&mut self, frame: &Frame) -> Result<(), ParseHalt> {
        let def_token = self.peek().expect("caller checked the keyword");
        self.index += 1;
        let mut is_self = false;
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Keyword && self.word(token) == "self")
        {
            self.index += 1;
            if !self.peek().is_some_and(|token| self.is_op(token, ".")) {
                // `def self` without the dot is outside the admitted shape.
                return Err(ParseHalt::Outside);
            }
            self.index += 1;
            is_self = true;
        }
        let Some(name_token) = self.peek() else {
            return Err(ParseHalt::Structural);
        };
        if name_token.kind != TokenKind::Ident {
            // Operator method definitions and keyword names are outside the
            // admitted subset.
            return Err(ParseHalt::Outside);
        }
        let name = self.word(name_token).to_string();
        self.index += 1;

        let mut zero_params = true;
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::LParen)
        {
            zero_params = false;
            if self
                .peek_at(self.index + 1)
                .is_some_and(|token| token.kind == TokenKind::RParen)
            {
                self.index += 2;
                zero_params = true;
            } else {
                self.index += 1;
                let mut paren_depth = 1i64;
                while let Some(token) = self.peek() {
                    match token.kind {
                        TokenKind::LParen => paren_depth += 1,
                        TokenKind::RParen => {
                            paren_depth -= 1;
                            if paren_depth == 0 {
                                self.index += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    self.index += 1;
                }
                if paren_depth != 0 {
                    return Err(ParseHalt::Structural);
                }
            }
        }

        // The header line must end here. Anything else — an endless `= expr`
        // body foremost — is outside the admitted subset.
        if !matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Newline | TokenKind::Semicolon,
                ..
            })
        ) {
            return Err(ParseHalt::Outside);
        }

        self.parse_region(&Frame::Other)?;
        let end = self
            .tokens
            .get(self.index.wrapping_sub(1))
            .map(|token| token.end)
            .unwrap_or(def_token.end);

        if let Frame::Class(class_index) = frame {
            self.out.methods.push(MethodDecl {
                class_index: *class_index,
                name,
                is_self,
                zero_params,
                start: def_token.start,
                end,
            });
            if let Some(decl) = self.out.classes.get_mut(*class_index) {
                decl.has_test_shape_method = decl.has_test_shape_method
                    || self
                        .out
                        .methods
                        .last()
                        .is_some_and(MethodDecl::is_test_shape);
            }
        }
        Ok(())
    }

    fn peek_at(&self, index: usize) -> Option<Token> {
        self.tokens.get(index).copied()
    }

    fn parse_class(&mut self, frame: &Frame) -> Result<(), ParseHalt> {
        let class_token = self.peek().expect("caller checked the keyword");
        self.index += 1;
        if self.peek().is_some_and(|token| self.is_op(token, "<<")) {
            // A singleton class body: structure only, never an anchor.
            self.consume_to_newline();
            return self.parse_region(&Frame::Other);
        }
        let Some(name) = self.const_path() else {
            return Err(ParseHalt::Outside);
        };
        let mut super_path = None;
        if self.peek().is_some_and(|token| self.is_op(token, "<")) {
            self.index += 1;
            match self.const_path() {
                Some(path) => super_path = Some(path),
                None => return Err(ParseHalt::Outside),
            }
        }
        // ADR-0049 D2: the header is a single logical line.
        if !matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Newline | TokenKind::Semicolon,
                ..
            })
        ) {
            return Err(ParseHalt::Outside);
        }

        let top_level = matches!(frame, Frame::Program);
        let class_index = self.out.classes.len();
        self.out.classes.push(ClassDecl {
            name,
            super_path,
            start: class_token.start,
            end: class_token.end,
            top_level,
            completed: false,
            has_test_shape_method: false,
        });
        let inner = if top_level {
            Frame::Class(class_index)
        } else {
            Frame::Other
        };
        let result = self.parse_region(&inner);
        if let Some(decl) = self.out.classes.get_mut(class_index) {
            decl.completed = result.is_ok();
            if result.is_ok() {
                decl.end = self
                    .tokens
                    .get(self.index.wrapping_sub(1))
                    .map(|token| token.end)
                    .unwrap_or(class_token.end);
            }
        }
        result
    }

    fn parse_module(&mut self) -> Result<(), ParseHalt> {
        self.index += 1; // the `module` keyword
        if self.const_path().is_none() {
            return Err(ParseHalt::Outside);
        }
        if !matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Newline | TokenKind::Semicolon,
                ..
            })
        ) {
            return Err(ParseHalt::Outside);
        }
        self.parse_region(&Frame::Other)
    }

    /// A dotted constant path `A::B::C`.
    fn const_path(&mut self) -> Option<String> {
        let first = self.peek()?;
        if first.kind != TokenKind::Const {
            return None;
        }
        let mut path = self.word(first).to_string();
        self.index += 1;
        while self.peek().is_some_and(|token| self.is_op(token, "::")) {
            self.index += 1;
            let next = self.peek()?;
            if next.kind != TokenKind::Const {
                return None;
            }
            path.push_str("::");
            path.push_str(self.word(next));
            self.index += 1;
        }
        Some(path)
    }

    fn consume_to_newline(&mut self) {
        while let Some(token) = self.peek() {
            if token.kind == TokenKind::Newline {
                return;
            }
            self.index += 1;
        }
    }

    /// Consume one statement: opaque tokens up to a structural boundary at
    /// bracket depth zero. Bracket groups balance internally, `do` opens a
    /// nested `end`-region, and `def`/`class`-shaped keywords mid-statement
    /// parse without recording.
    fn consume_statement(&mut self, frame: &Frame) -> Result<(), ParseHalt> {
        // A top-level `require`/`require_relative` statement is the bounded
        // context fact of ADR-0049 D3a.
        if matches!(frame, Frame::Program) {
            if let Some(word) = self.peek_word() {
                if matches!(word, "require" | "require_relative") {
                    if let Some(argument) = self.tokens.get(self.index + 1) {
                        let next_context = if argument.kind == TokenKind::Str {
                            match self.word(*argument) {
                                "\"minitest/autorun\"" | "'minitest/autorun'" => {
                                    RequireContext::Autorun
                                }
                                "\"test-unit\"" | "'test-unit'" => RequireContext::TestUnit,
                                _ => RequireContext::LiteralOther,
                            }
                        } else {
                            RequireContext::NonLiteral
                        };
                        self.out.require_context = Some(next_context);
                    }
                }
            }
        }

        let mut depth = 0i64;
        loop {
            let Some(token) = self.peek() else {
                return Ok(());
            };
            match token.kind {
                TokenKind::Newline | TokenKind::Semicolon if depth == 0 => return Ok(()),
                TokenKind::Keyword if depth == 0 => match self.word(token) {
                    "end" | "else" | "elsif" | "rescue" | "ensure" | "when" | "then" => {
                        return Ok(());
                    }
                    "do" => {
                        self.index += 1;
                        self.parse_region(&Frame::Other)?;
                    }
                    _ => {
                        if let Some(opener) = self.block_opener_ahead() {
                            self.parse_opener(opener, &Frame::Other)?;
                        } else {
                            self.index += 1;
                        }
                    }
                },
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                    self.index += 1;
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    if depth == 0 {
                        return Ok(());
                    }
                    depth -= 1;
                    self.index += 1;
                }
                _ => self.index += 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(path: &str, text: &str) -> SourceParseOutput {
        RubyMinitestParser
            .parse_with_context_output(
                SourceDocument {
                    path,
                    language: Language::Ruby,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .unwrap_or_else(|error| panic!("parse {path}: {error:?}"))
    }

    fn anchors(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.target.as_ref().map(SymbolId::as_str) == Some(RUBY_MINITEST_TARGET))
            .count()
    }

    fn class_anchors(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(RUBY_MINITEST_CLASS_TARGET)
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
                    .strip_prefix("ruby_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    const CATALOG: &str = r#"
require "minitest/autorun"

class CatalogTest < Minitest::Test
  def setup
    @catalog = %w[apple banana cherry]
  end

  def test_loads_catalog
    text = <<~DESCRIPTION
      class Fake < Minitest::Test
      fake def test_hidden
    DESCRIPTION
    refute_includes text, "cherry"
    assert_equal 3, @catalog.length
  end

  def test_filters_catalog
    assert_match(/b+a+n/, "banana")
    assert_equal (2 + 2) / 2, 2
  end

  def helper_method
    :not_a_test
  end
end
"#;

    #[test]
    fn admitted_classes_and_test_methods_anchor() {
        let parsed = output("test/test_catalog.rb", CATALOG);
        assert!(parsed.report.diagnostics.is_empty());
        assert_eq!(anchors(&parsed), 2);
        assert_eq!(class_anchors(&parsed), 1);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::RubyMinitestTestMethod)
                .count(),
            2
        );
        // The require context rides the class anchor as one bounded token.
        let class_fact = parsed
            .report
            .semantic_facts
            .iter()
            .find(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(RUBY_MINITEST_CLASS_TARGET)
            })
            .expect("class anchor");
        assert!(class_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "minitest_require=autorun"));
    }

    #[test]
    fn superclass_chains_resolve_within_the_file() {
        let source = r#"
require "test-unit"

class CatalogBaseTest < Minitest::Test
end

class OrdersTest < CatalogBaseTest
  def test_places_order
    assert_equal 1, 1
  end

  def test_totals_order
    assert_equal 2, 2
  end
end

class LoopA < LoopB
end

class LoopB < LoopA
end
"#;
        let parsed = output("test/test_orders.rb", source);
        assert_eq!(anchors(&parsed), 2, "the chained subclass anchors");
        assert_eq!(class_anchors(&parsed), 2, "base and subclass both qualify");
        // The cyclic pair qualifies for nothing and reports nothing.
        assert!(!unknown_kinds(&parsed).contains(&"test_methods_without_minitest_base".to_string()));
        let class_fact = parsed
            .report
            .semantic_facts
            .iter()
            .find(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(RUBY_MINITEST_CLASS_TARGET)
            })
            .expect("class anchor");
        assert!(class_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "minitest_require=test_unit"));
    }

    #[test]
    fn activesupport_is_an_admitted_base_and_absent_require_is_recorded() {
        let source = "\nclass WidgetTest < ActiveSupport::TestCase\n  def test_renders\n    assert true\n  end\nend\n";
        let parsed = output("test/widget_test.rb", source);
        assert_eq!(anchors(&parsed), 1);
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "minitest_require=absent")
        }));
    }

    #[test]
    fn lookalikes_do_not_anchor_and_unbound_test_methods_are_typed() {
        let source = r#"
class HelperService
  def test_loads
    :no_base
  end
end

class WidgetTest < BaseTest
  def test_renders
    :unknown_base
  end
end

class ShadowTest < Minitest::Test
  def self.test_class_method
    :class_method
  end

  def test_with_params(count)
    :parameterized
  end

  def testing_prefix
    :wrong_prefix
  end

  private def test_visibility_prefixed
    :not_direct
  end

  def test_real
    assert true
  end
end
"#;
        let parsed = output("test/test_lookalikes.rb", source);
        // Exactly one anchor: the direct zero-parameter instance method.
        assert_eq!(anchors(&parsed), 1);
        assert!(unknown_kinds(&parsed).contains(&"test_methods_without_minitest_base".to_string()));
    }

    #[test]
    fn nested_and_dynamic_defs_do_not_anchor() {
        let source = r#"
class NestedTest < Minitest::Test
  def outer_helper
    def test_defined_dynamically_inside_a_method
      :dynamic
    end
  end

  def test_conditional
    :plain
  end
end

NestedTest.class_eval do
  def test_class_eval
    :dynamic
  end
end

describe "spec dsl" do
  it "has tests" do
    assert true
  end
end
"#;
        let parsed = output("test/test_nested.rb", source);
        assert_eq!(
            anchors(&parsed),
            1,
            "only the direct class-body def anchors"
        );
    }

    #[test]
    fn conditional_and_block_wrapped_defs_do_not_anchor() {
        let source = r#"
class ConditionalTest < Minitest::Test
  if true
    def test_under_if
      :conditional
    end
  end

  [:a].each do |name|
    define_method("test_#{name}") do
      :generated
    end
  end

  begin
    def test_under_begin
      :guarded
    end
  end

  def test_direct
    assert true
  end
end
"#;
        let parsed = output("test/test_conditional.rb", source);
        // `def` under `if`, `begin`, or a block is not a direct class-body
        // statement; only the unconditional one anchors.
        assert_eq!(anchors(&parsed), 1);
    }

    #[test]
    fn lexer_refusals_abstain_whole_file() {
        for (path, source, kind) in [
            (
                "test/test_slash.rb",
                "\nclass SlashTest < Minitest::Test\n  def test_ratio\n    ratio = expected / 2\n  end\nend\n",
                "ruby_slash_disambiguation",
            ),
            (
                "test/test_heredoc_shift.rb",
                "\nx = items <<value\n",
                "ruby_heredoc_disambiguation",
            ),
            (
                "test/test_unterminated.rb",
                "\nclass OpenTest < Minitest::Test\n  def test_open\n    text = <<~NEVER\n  end\nend\n",
                "unterminated_heredoc",
            ),
            (
                "test/test_percent.rb",
                "\nwords = %w[a b\n",
                "unterminated_percent_literal",
            ),
            (
                "test/test_string.rb",
                "\nx = 'open\n",
                "unterminated_string",
            ),
        ] {
            let parsed = output(path, source);
            assert_eq!(anchors(&parsed), 0, "{path}");
            assert_eq!(class_anchors(&parsed), 0, "{path}");
            assert!(
                unknown_kinds(&parsed).contains(&kind.to_string()),
                "{path}: {:?}",
                unknown_kinds(&parsed)
            );
            assert!(!parsed.report.diagnostics.is_empty(), "{path}");
        }
    }

    #[test]
    fn unadmitted_constructs_abstain_whole_file() {
        for (path, source) in [
            // Endless method definition (Ruby 3.0): no `end` to match.
            (
                "test/test_endless.rb",
                "\nclass EndlessTest < Minitest::Test\n  def test_value = assert_equal(2, 1 + 1)\nend\n",
            ),
            // Operator method definition.
            (
                "test/test_operator_def.rb",
                "\nclass OpTest < Minitest::Test\n  def ==(other)\n    true\n  end\nend\n",
            ),
            // Non-ASCII in code position.
            (
                "test/test_unicode.rb",
                "\nclass UniTest < Minitest::Test\n  def test_value\n    \u{e4} = 1\n  end\nend\n",
            ),
        ] {
            let parsed = output(path, source);
            assert_eq!(anchors(&parsed), 0, "{path}");
            assert!(
                unknown_kinds(&parsed).contains(&"unadmitted_ruby_construct".to_string()),
                "{path}: {:?}",
                unknown_kinds(&parsed)
            );
        }
    }

    #[test]
    fn structural_failures_keep_the_declarations_completed_before_them() {
        let source = "\nclass FirstTest < Minitest::Test\n  def test_ok\n    assert true\n  end\nend\nclass BrokenTest < Minitest::Test\n  def test_dangling\n    if true\n  end\nend\n";
        let parsed = output("test/test_broken.rb", source);
        assert_eq!(anchors(&parsed), 1, "the completed class keeps its anchor");
        assert!(unknown_kinds(&parsed).contains(&"ruby_structural_failure".to_string()));
        assert!(!parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn only_runner_scoped_paths_are_admitted() {
        for path in [
            "main.rb",
            "lib/catalog.rb",
            "test/catalog.rb",
            "test/helper.rb",
            "spec/test_catalog.rb",
            "test_catalog.rb",
            "test/test_catalog.RB",
        ] {
            assert!(!is_minitest_path(path), "{path}");
        }
        for path in [
            "test/test_catalog.rb",
            "test/models/order_test.rb",
            "tests/test_helper.rb",
            "app/test/test_deep_test.rb",
        ] {
            assert!(is_minitest_path(path), "{path}");
        }
    }

    #[test]
    fn ir_links_module_to_class_and_method_units() {
        let parsed = output(
            "test/test_catalog.rb",
            "\nclass CatalogTest < Minitest::Test\n  def test_loads\n    assert true\n  end\nend\n",
        );
        // Module, class, method nodes; module→class and module→method edges.
        assert_eq!(parsed.report.units.len(), 3);
        assert_eq!(parsed.report.ir_nodes.len(), 3);
        assert!(parsed.report.ir_edges.len() >= 2);
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output("test/test_catalog.rb", CATALOG);
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn non_literal_require_arguments_record_without_retaining_text() {
        let source = "\nrequire helper\n\nclass StaticTest < Minitest::Test\n  def test_static\n    assert true\n  end\nend\n";
        let parsed = output("test/test_static.rb", source);
        assert_eq!(anchors(&parsed), 1);
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "minitest_require=non_literal")
        }));
        assert!(!format!("{:?}", parsed.report.semantic_facts).contains("helper"));
    }

    #[test]
    fn committed_release_fixtures_parse_through_the_product_parser() {
        use crate::adapters::parsing::RepoGrammarSourceParser;
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/ruby/release/v0_2");
        let parser = RepoGrammarSourceParser::default();
        let mut total_method_anchors = 0usize;
        for fixture in [
            "minitest_exact_tests",
            "minitest_lookalikes",
            "minitest_low_support",
        ] {
            let mut dir = std::fs::read_dir(root.join(fixture).join("test"))
                .unwrap_or_else(|error| panic!("fixture {fixture}: {error}"));
            while let Some(entry) = dir.next().and_then(Result::ok) {
                let path = entry.path();
                let text = std::fs::read_to_string(&path).expect("fixture text");
                let relative = path
                    .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                    .expect("relative")
                    .to_string_lossy()
                    .replace('\\', "/");
                let report = parser
                    .parse_with_context(
                        SourceDocument {
                            path: &relative,
                            language: Language::Ruby,
                            content_hash: ContentHash::new(
                                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                            )
                            .expect("hash"),
                            repository_revision: RepositoryRevision::new("UNKNOWN")
                                .expect("revision"),
                            text: &text,
                        },
                        &ParserProjectContext::default(),
                    )
                    .unwrap_or_else(|error| panic!("fixture {relative}: {error:?}"));
                total_method_anchors += report
                    .semantic_facts
                    .iter()
                    .filter(|fact| {
                        fact.target.as_ref().map(SymbolId::as_str) == Some(RUBY_MINITEST_TARGET)
                    })
                    .count();
            }
        }
        assert!(
            total_method_anchors >= 3,
            "the support corpus must reach the family minimum: {total_method_anchors}"
        );
    }

    #[test]
    fn ruby_readiness_output_stays_source_free_and_low_cardinality() {
        // The product parser over the committed fixtures must expose no
        // fixture identifier, sentinel, or source text, mirroring the r
        // lane's readiness assertions at the parser boundary this lane owns.
        use crate::adapters::parsing::RepoGrammarSourceParser;
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/ruby/release/v0_2");
        let parser = RepoGrammarSourceParser::default();
        let markers = [
            "CatalogTest",
            "PricingTest",
            "OrdersTest",
            "SoloTest",
            "test_loads_catalog",
            "test_applies_discount",
            "test_places_order",
            "test_runs_alone",
            "test_runs_again",
            "expected",
            "SENTINEL",
        ];
        let mut checked = 0usize;
        for fixture in [
            "minitest_exact_tests",
            "minitest_lookalikes",
            "minitest_low_support",
            "minitest_parse_degraded",
        ] {
            let mut dir = std::fs::read_dir(root.join(fixture).join("test"))
                .unwrap_or_else(|error| panic!("fixture {fixture}: {error}"));
            while let Some(entry) = dir.next().and_then(Result::ok) {
                let path = entry.path();
                let text = std::fs::read_to_string(&path).expect("fixture text");
                let relative = path
                    .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                    .expect("relative")
                    .to_string_lossy()
                    .replace('\\', "/");
                let output = parser
                    .parse_with_context_output(
                        SourceDocument {
                            path: &relative,
                            language: Language::Ruby,
                            content_hash: ContentHash::new(
                                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                            )
                            .expect("hash"),
                            repository_revision: RepositoryRevision::new("UNKNOWN")
                                .expect("revision"),
                            text: &text,
                        },
                        &ParserProjectContext::default(),
                    )
                    .expect("parse fixture");
                let debug = format!("{output:?}");
                for marker in markers {
                    assert!(
                        !debug.contains(marker),
                        "{relative} leaked {marker} into readiness output"
                    );
                }
                checked += 1;
            }
        }
        assert!(
            checked >= 6,
            "expected the full fixture matrix, saw {checked}"
        );
    }
}
