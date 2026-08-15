//! Recursive-descent parser for the ADR-0045 declared Ada subset.
//!
//! The parser has two outcomes and no third: it consumes the whole compilation
//! unit, or it refuses the file. It never recovers, never resynchronises, and
//! never produces a partial tree, because an anchor recovered from a guess is
//! indistinguishable from an anchor that is there.
//!
//! That failure mode is what makes a hand-written subset acceptable. A gap in
//! the grammar costs recall -- the file abstains -- and can never invent a
//! registration that the source does not contain.
//!
//! ## The invariance argument
//!
//! RepoGrammar cannot see which Ada edition a GPR project selects, so the
//! declared subset is built so that it does not need to. For every text this
//! grammar admits, the token stream and the phrase structure are the same under
//! **every edition of {Ada 95, Ada 2005, Ada 2012, Ada 2022} under which that
//! text is legal**. Ada 83 is outside the set and costs nothing: `'Access` does
//! not exist in Ada 83, so no Ada 83 compilation unit can contain the admitted
//! anchor in the first place.
//!
//! The one construct class that could break this is a word whose classification
//! moves between editions, and [`super::lexer`] gives those five words their
//! own token kind. `overriding` is admitted only immediately before `procedure`
//! or `function`, where the Ada 95 reading is not legal at all, so the two
//! editions cannot disagree about a text that parses. Every other occurrence of
//! an edition-sensitive word is refused.

use super::lexer::{lex, Refusal, Token, TokenKind};

/// Recursion ceiling. Untrusted input may nest arbitrarily; a stack overflow
/// aborts the process, so the depth is bounded and a breach abstains.
const MAX_DEPTH: usize = 96;

/// Byte range of one admitted `Register_Routine` call, from the first character
/// of the callee name through the closing parenthesis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Registration {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct AdmittedUnit {
    /// A context clause in this file names a library unit whose first
    /// identifier is `AUnit`.
    pub aunit_context_clause: bool,
    pub registrations: Vec<Registration>,
}

/// Configuration pragmas that re-select the language, which is precisely what
/// the invariance argument assumes fixed.
///
/// `Ada_95`, `Ada_05`, `Ada_2005`, `Ada_12`, `Ada_2012`, and `Ada_2022` are
/// deliberately absent: they select a member of the declared set, and the
/// admitted parse is identical across the whole set, so they change nothing
/// this frontend claims.
const LANGUAGE_EDITION_PRAGMAS: &[&str] = &["ada_83", "ada_1983", "extensions_allowed"];

const REGISTER_ROUTINE: &str = "register_routine";

pub(crate) fn parse_compilation(source: &str) -> Result<AdmittedUnit, Refusal> {
    let tokens = lex(source)?;
    refuse_misplaced_edition_sensitive_words(source, &tokens)?;
    let mut parser = Parser::new(source, &tokens);
    parser.compilation()?;
    Ok(parser.admitted)
}

/// An edition-sensitive word is admitted in exactly one position: as the
/// `overriding_indicator` of a subprogram declaration or body, where Ada 95
/// has no legal reading of the same text.
fn refuse_misplaced_edition_sensitive_words(source: &str, tokens: &[Token]) -> Result<(), Refusal> {
    for (index, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::EditionSensitive {
            continue;
        }
        let is_overriding_indicator = token.text(source).eq_ignore_ascii_case("overriding")
            && tokens.get(index + 1).is_some_and(|next| {
                next.matches(source, TokenKind::Reserved, "procedure")
                    || next.matches(source, TokenKind::Reserved, "function")
            });
        if !is_overriding_indicator {
            return Err(Refusal::EditionSensitiveWord);
        }
    }
    Ok(())
}

/// What an actual parameter or expression turned out to be, to the precision
/// the admitted anchor needs and no further.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExprShape {
    /// A plain dotted name whose only other suffix is a final `'Access`.
    AccessName,
    /// Exactly one string literal.
    StringLiteral,
    Other,
}

struct CallSuffix {
    designator_start: usize,
    designator_end: usize,
    actuals: Vec<ExprShape>,
    end: usize,
}

struct NameInfo {
    start: usize,
    shape: ExprShape,
    /// Present when the last suffix of the name was an actual-parameter part,
    /// which is what makes the name a candidate procedure call.
    call: Option<CallSuffix>,
}

struct Parser<'a> {
    source: &'a str,
    tokens: &'a [Token],
    position: usize,
    depth: usize,
    admitted: AdmittedUnit,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: &'a [Token]) -> Self {
        Self {
            source,
            tokens,
            position: 0,
            depth: 0,
            admitted: AdmittedUnit::default(),
        }
    }

    // ---------------------------------------------------------------- cursor

    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.position)
    }

    fn peek_at(&self, offset: usize) -> Option<&'a Token> {
        self.tokens.get(self.position + offset)
    }

    fn at(&self, kind: TokenKind, word: &str) -> bool {
        self.peek()
            .is_some_and(|token| token.matches(self.source, kind, word))
    }

    fn at_word(&self, word: &str) -> bool {
        self.at(TokenKind::Reserved, word)
    }

    fn at_symbol(&self, symbol: &str) -> bool {
        self.at(TokenKind::Delimiter, symbol)
    }

    fn at_kind(&self, kind: TokenKind) -> bool {
        self.peek().is_some_and(|token| token.kind == kind)
    }

    fn advance(&mut self) -> Result<&'a Token, Refusal> {
        let token = self.peek().ok_or(Refusal::OutsideDeclaredSubset)?;
        self.position += 1;
        Ok(token)
    }

    fn eat_word(&mut self, word: &str) -> bool {
        let found = self.at_word(word);
        if found {
            self.position += 1;
        }
        found
    }

    fn eat_symbol(&mut self, symbol: &str) -> bool {
        let found = self.at_symbol(symbol);
        if found {
            self.position += 1;
        }
        found
    }

    fn expect_word(&mut self, word: &str) -> Result<(), Refusal> {
        if self.eat_word(word) {
            Ok(())
        } else {
            Err(Refusal::OutsideDeclaredSubset)
        }
    }

    fn expect_symbol(&mut self, symbol: &str) -> Result<(), Refusal> {
        if self.eat_symbol(symbol) {
            Ok(())
        } else {
            Err(Refusal::OutsideDeclaredSubset)
        }
    }

    fn expect_identifier(&mut self) -> Result<&'a Token, Refusal> {
        if self.at_kind(TokenKind::Identifier) {
            self.advance()
        } else {
            Err(Refusal::OutsideDeclaredSubset)
        }
    }

    fn enter(&mut self) -> Result<(), Refusal> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(Refusal::NestingLimit);
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    // ----------------------------------------------------------- compilation

    /// `compilation ::= { pragma | with_clause | use_clause } library_item`
    ///
    /// `library_item` is a package body or a library-level subprogram body,
    /// which are the two forms a `.adb` file holds. A subunit (`separate (P)`)
    /// is refused: its context clause is inherited from the parent, and
    /// ADR-0045 D2 does not admit an import it cannot see.
    fn compilation(&mut self) -> Result<(), Refusal> {
        loop {
            if self.at_word("pragma") {
                self.pragma()?;
            } else if self.at_word("with") || self.at_word("limited") || self.at_word("private") {
                self.with_clause()?;
            } else if self.at_word("use") {
                self.use_clause()?;
            } else {
                break;
            }
        }
        if self.at_word("package") {
            self.package_body()?;
        } else if self.at_word("procedure") || self.at_word("function") || self.at_word("not") {
            match self.subprogram_declaration_or_body()? {
                SubprogramForm::Body => {}
                SubprogramForm::Declaration => return Err(Refusal::OutsideDeclaredSubset),
            }
        } else {
            return Err(Refusal::OutsideDeclaredSubset);
        }
        if self.position != self.tokens.len() {
            return Err(Refusal::OutsideDeclaredSubset);
        }
        Ok(())
    }

    /// `with_clause ::= [limited] [private] "with" name {"," name} ";"`
    fn with_clause(&mut self) -> Result<(), Refusal> {
        let _ = self.eat_word("limited");
        let _ = self.eat_word("private");
        self.expect_word("with")?;
        loop {
            let first = self.expect_identifier()?;
            if first.text(self.source).eq_ignore_ascii_case("aunit") {
                self.admitted.aunit_context_clause = true;
            }
            while self.eat_symbol(".") {
                self.expect_identifier()?;
            }
            if !self.eat_symbol(",") {
                break;
            }
        }
        self.expect_symbol(";")
    }

    /// `use_clause ::= "use" ["all"] ["type"] name {"," name} ";"`
    fn use_clause(&mut self) -> Result<(), Refusal> {
        self.expect_word("use")?;
        let _ = self.eat_word("all");
        let _ = self.eat_word("type");
        loop {
            self.name()?;
            if !self.eat_symbol(",") {
                break;
            }
        }
        self.expect_symbol(";")
    }

    /// `pragma ::= "pragma" identifier [ "(" association_list ")" ] ";"`
    fn pragma(&mut self) -> Result<(), Refusal> {
        self.expect_word("pragma")?;
        // The identifier position also accepts a few reserved words, because
        // `pragma Interface` and `pragma Pure` spell differently across
        // editions; only the edition-selecting names matter here.
        let name = self.advance()?;
        if !matches!(name.kind, TokenKind::Identifier | TokenKind::Reserved) {
            return Err(Refusal::OutsideDeclaredSubset);
        }
        if LANGUAGE_EDITION_PRAGMAS
            .iter()
            .any(|pragma| name.text(self.source).eq_ignore_ascii_case(pragma))
        {
            return Err(Refusal::LanguageEditionPragma);
        }
        if self.at_symbol("(") {
            self.parenthesised_associations()?;
        }
        self.expect_symbol(";")
    }

    // -------------------------------------------------------------- packages

    /// `package_body ::= "package" "body" name [aspect] "is" declarative_part`
    /// `[ "begin" handled_sequence ] "end" [name] ";"`
    fn package_body(&mut self) -> Result<(), Refusal> {
        self.expect_word("package")?;
        self.expect_word("body")?;
        self.defining_name()?;
        self.optional_aspect_specification()?;
        self.expect_word("is")?;
        self.declarative_part()?;
        if self.eat_word("begin") {
            self.handled_sequence_of_statements()?;
        }
        self.expect_word("end")?;
        if self.at_kind(TokenKind::Identifier) {
            self.defining_name()?;
        }
        self.expect_symbol(";")
    }

    /// The declarative-level `package` forms: a body, a nested declaration, a
    /// generic instantiation, and a renaming.
    fn package_declarative_item(&mut self) -> Result<(), Refusal> {
        if self
            .peek_at(1)
            .is_some_and(|token| token.matches(self.source, TokenKind::Reserved, "body"))
        {
            return self.package_body();
        }
        self.expect_word("package")?;
        self.defining_name()?;
        if self.eat_word("renames") {
            self.name()?;
            return self.expect_symbol(";");
        }
        self.optional_aspect_specification()?;
        self.expect_word("is")?;
        if self.eat_word("new") {
            self.name()?;
            self.optional_aspect_specification()?;
            return self.expect_symbol(";");
        }
        self.declarative_part()?;
        if self.eat_word("private") {
            self.declarative_part()?;
        }
        self.expect_word("end")?;
        if self.at_kind(TokenKind::Identifier) {
            self.defining_name()?;
        }
        self.expect_symbol(";")
    }

    /// A defining name may be dotted, because a child unit body names its
    /// parent (`package body Parent.Child is`).
    fn defining_name(&mut self) -> Result<(), Refusal> {
        self.expect_identifier()?;
        while self.at_symbol(".")
            && self
                .peek_at(1)
                .is_some_and(|token| token.kind == TokenKind::Identifier)
        {
            self.position += 1;
            self.position += 1;
        }
        Ok(())
    }

    // ---------------------------------------------------------- declarations

    fn declarative_part(&mut self) -> Result<(), Refusal> {
        self.enter()?;
        while !(self.peek().is_none()
            || self.at_word("begin")
            || self.at_word("end")
            || self.at_word("private")
            || self.at_word("exception"))
        {
            self.declarative_item()?;
        }
        self.leave();
        Ok(())
    }

    fn declarative_item(&mut self) -> Result<(), Refusal> {
        if self.at_word("pragma") {
            return self.pragma();
        }
        if self.at_word("use") {
            return self.use_clause();
        }
        if self.at_word("type") {
            return self.type_declaration();
        }
        if self.at_word("subtype") {
            return self.subtype_declaration();
        }
        if self.at_word("package") {
            return self.package_declarative_item();
        }
        if self.at_word("procedure") || self.at_word("function") || self.at_word("not") {
            self.subprogram_declaration_or_body()?;
            return Ok(());
        }
        if self.at_kind(TokenKind::EditionSensitive) {
            // Only `overriding procedure` / `overriding function` reach here,
            // because the pre-pass refused every other placement.
            self.subprogram_declaration_or_body()?;
            return Ok(());
        }
        if self.at_kind(TokenKind::Identifier) {
            return self.object_like_declaration();
        }
        // `generic`, `task`, `protected`, `entry`, `for` representation
        // clauses, and `separate` subunits are outside the declared subset.
        Err(Refusal::OutsideDeclaredSubset)
    }

    /// Object, number, exception, and object-renaming declarations, which all
    /// begin with a defining identifier list followed by `:`.
    fn object_like_declaration(&mut self) -> Result<(), Refusal> {
        self.identifier_list()?;
        self.expect_symbol(":")?;
        if self.eat_word("exception") {
            if self.eat_word("renames") {
                self.name()?;
            }
            return self.expect_symbol(";");
        }
        let _ = self.eat_word("aliased");
        let constant = self.eat_word("constant");
        if constant && self.at_symbol(":=") {
            // `X : constant := 5;` -- a number declaration has no subtype.
            self.position += 1;
            self.expression()?;
            self.optional_aspect_specification()?;
            return self.expect_symbol(";");
        }
        self.object_subtype()?;
        if self.eat_word("renames") {
            self.name()?;
            self.optional_aspect_specification()?;
            return self.expect_symbol(";");
        }
        if self.eat_symbol(":=") {
            self.expression()?;
        }
        self.optional_aspect_specification()?;
        self.expect_symbol(";")
    }

    fn identifier_list(&mut self) -> Result<(), Refusal> {
        loop {
            self.expect_identifier()?;
            if !self.eat_symbol(",") {
                return Ok(());
            }
        }
    }

    fn object_subtype(&mut self) -> Result<(), Refusal> {
        if self.at_word("array") {
            return self.array_type_definition();
        }
        self.subtype_indication()
    }

    /// `subtype_indication ::= [not null] subtype_mark [constraint]`
    ///
    /// An index or discriminant constraint is a parenthesised suffix of the
    /// name, so it is already consumed by [`Self::name`].
    fn subtype_indication(&mut self) -> Result<(), Refusal> {
        if self.at_word("not") {
            self.position += 1;
            self.expect_word("null")?;
        }
        if self.at_word("access") {
            return self.access_definition();
        }
        self.name()?;
        self.optional_scalar_constraint()
    }

    fn optional_scalar_constraint(&mut self) -> Result<(), Refusal> {
        if self.eat_word("range") {
            self.range_or_box()?;
        } else if self.eat_word("digits") {
            self.simple_expression()?;
            if self.eat_word("range") {
                self.range_or_box()?;
            }
        } else if self.eat_word("delta") {
            self.simple_expression()?;
            if self.eat_word("digits") {
                self.simple_expression()?;
            }
            if self.eat_word("range") {
                self.range_or_box()?;
            }
        }
        Ok(())
    }

    fn range_or_box(&mut self) -> Result<(), Refusal> {
        if self.eat_symbol("<>") {
            return Ok(());
        }
        self.simple_expression()?;
        if self.eat_symbol("..") {
            self.simple_expression()?;
        }
        Ok(())
    }

    /// `access_definition ::= "access" ["all"|"constant"] subtype_indication`
    /// `| "access" ["protected"] ("procedure"|"function") profile`
    fn access_definition(&mut self) -> Result<(), Refusal> {
        self.expect_word("access")?;
        let _ = self.eat_word("protected");
        if self.eat_word("procedure") {
            if self.at_symbol("(") {
                self.parameter_profile()?;
            }
            return Ok(());
        }
        if self.eat_word("function") {
            if self.at_symbol("(") {
                self.parameter_profile()?;
            }
            self.expect_word("return")?;
            return self.return_subtype();
        }
        let _ = self.eat_word("all") || self.eat_word("constant");
        self.subtype_indication()
    }

    fn subtype_declaration(&mut self) -> Result<(), Refusal> {
        self.expect_word("subtype")?;
        self.expect_identifier()?;
        self.expect_word("is")?;
        self.subtype_indication()?;
        self.optional_aspect_specification()?;
        self.expect_symbol(";")
    }

    // ----------------------------------------------------------------- types

    fn type_declaration(&mut self) -> Result<(), Refusal> {
        self.expect_word("type")?;
        self.expect_identifier()?;
        if self.at_symbol("(") {
            self.discriminant_part()?;
        }
        if self.eat_symbol(";") {
            // An incomplete type declaration.
            return Ok(());
        }
        self.expect_word("is")?;
        self.type_definition()?;
        self.optional_aspect_specification()?;
        self.expect_symbol(";")
    }

    fn discriminant_part(&mut self) -> Result<(), Refusal> {
        self.expect_symbol("(")?;
        if self.eat_symbol("<>") {
            return self.expect_symbol(")");
        }
        loop {
            self.identifier_list()?;
            self.expect_symbol(":")?;
            if self.at_word("access") {
                self.access_definition()?;
            } else {
                self.subtype_indication()?;
            }
            if self.eat_symbol(":=") {
                self.expression()?;
            }
            if !self.eat_symbol(";") {
                break;
            }
        }
        self.expect_symbol(")")
    }

    fn type_definition(&mut self) -> Result<(), Refusal> {
        // `abstract`, `tagged`, and `limited` are prefixes of several
        // definitions and never change where the definition ends.
        while self.eat_word("abstract") || self.eat_word("tagged") || self.eat_word("limited") {}
        if self.at_symbol("(") {
            return self.enumeration_type_definition();
        }
        if self.at_word("array") {
            return self.array_type_definition();
        }
        if self.at_word("access") || self.at_word("not") {
            return self.subtype_indication();
        }
        if self.eat_word("private") {
            return Ok(());
        }
        if self.eat_word("range") {
            return self.range_or_box();
        }
        if self.eat_word("mod") {
            self.simple_expression()?;
            return Ok(());
        }
        if self.at_word("digits") || self.at_word("delta") {
            return self.optional_scalar_constraint();
        }
        if self.at_word("null") || self.at_word("record") {
            return self.record_definition();
        }
        if self.eat_word("new") {
            self.subtype_indication()?;
            if self.eat_word("with") {
                if self.eat_word("private") {
                    return Ok(());
                }
                return self.record_definition();
            }
            return Ok(());
        }
        // `task`, `protected`, `interface`, and `synchronized` type
        // definitions are outside the declared subset.
        Err(Refusal::OutsideDeclaredSubset)
    }

    fn enumeration_type_definition(&mut self) -> Result<(), Refusal> {
        self.expect_symbol("(")?;
        loop {
            if self.at_kind(TokenKind::Identifier) || self.at_kind(TokenKind::Character) {
                self.position += 1;
            } else {
                return Err(Refusal::OutsideDeclaredSubset);
            }
            if !self.eat_symbol(",") {
                break;
            }
        }
        self.expect_symbol(")")
    }

    fn array_type_definition(&mut self) -> Result<(), Refusal> {
        self.expect_word("array")?;
        self.expect_symbol("(")?;
        loop {
            if self.at_word("others") {
                return Err(Refusal::OutsideDeclaredSubset);
            }
            self.discrete_range()?;
            if !self.eat_symbol(",") {
                break;
            }
        }
        self.expect_symbol(")")?;
        self.expect_word("of")?;
        self.component_definition()
    }

    /// `discrete_range ::= subtype_indication | range`
    ///
    /// Both readings consume the same tokens, and this frontend derives no
    /// meaning from either, so one production covers them.
    fn discrete_range(&mut self) -> Result<(), Refusal> {
        if self.at_word("range") {
            self.position += 1;
            return self.range_or_box();
        }
        self.simple_expression()?;
        if self.eat_symbol("..") {
            self.simple_expression()?;
        } else if self.eat_word("range") {
            self.range_or_box()?;
        }
        Ok(())
    }

    fn component_definition(&mut self) -> Result<(), Refusal> {
        let _ = self.eat_word("aliased");
        self.subtype_indication()
    }

    /// `record_definition ::= "null" "record" | "record" component_list "end" "record"`
    fn record_definition(&mut self) -> Result<(), Refusal> {
        if self.eat_word("null") {
            return self.expect_word("record");
        }
        self.expect_word("record")?;
        self.component_list()?;
        self.expect_word("end")?;
        self.expect_word("record")
    }

    fn component_list(&mut self) -> Result<(), Refusal> {
        self.enter()?;
        loop {
            if self.at_word("end") || self.at_word("when") || self.peek().is_none() {
                break;
            }
            if self.eat_word("null") {
                self.expect_symbol(";")?;
                continue;
            }
            if self.at_word("case") {
                self.variant_part()?;
                continue;
            }
            self.identifier_list()?;
            self.expect_symbol(":")?;
            self.component_definition()?;
            if self.eat_symbol(":=") {
                self.expression()?;
            }
            self.optional_aspect_specification()?;
            self.expect_symbol(";")?;
        }
        self.leave();
        Ok(())
    }

    fn variant_part(&mut self) -> Result<(), Refusal> {
        self.expect_word("case")?;
        self.name()?;
        self.expect_word("is")?;
        while self.eat_word("when") {
            self.discrete_choice_list()?;
            self.expect_symbol("=>")?;
            self.component_list()?;
        }
        self.expect_word("end")?;
        self.expect_word("case")?;
        self.expect_symbol(";")
    }

    // ------------------------------------------------------------ subprogram

    fn subprogram_declaration_or_body(&mut self) -> Result<SubprogramForm, Refusal> {
        if self.at_word("not") {
            self.position += 1;
        }
        if self.at_kind(TokenKind::EditionSensitive) {
            // The pre-pass proved this is an `overriding` indicator.
            self.position += 1;
        }
        self.subprogram_specification()?;
        if self.eat_word("renames") {
            self.name()?;
            self.optional_aspect_specification()?;
            self.expect_symbol(";")?;
            return Ok(SubprogramForm::Declaration);
        }
        self.optional_aspect_specification()?;
        if self.eat_symbol(";") {
            return Ok(SubprogramForm::Declaration);
        }
        self.expect_word("is")?;
        if self.eat_word("abstract") || self.eat_word("separate") {
            self.optional_aspect_specification()?;
            self.expect_symbol(";")?;
            return Ok(SubprogramForm::Declaration);
        }
        if self.at_word("null")
            && self
                .peek_at(1)
                .is_some_and(|token| token.matches(self.source, TokenKind::Delimiter, ";"))
        {
            self.position += 2;
            return Ok(SubprogramForm::Declaration);
        }
        if self.eat_word("new") {
            self.name()?;
            self.optional_aspect_specification()?;
            self.expect_symbol(";")?;
            return Ok(SubprogramForm::Declaration);
        }
        if self.at_symbol("(") {
            // An Ada 2012 expression function.
            self.parenthesised_associations()?;
            self.optional_aspect_specification()?;
            self.expect_symbol(";")?;
            return Ok(SubprogramForm::Declaration);
        }
        self.declarative_part()?;
        self.expect_word("begin")?;
        self.handled_sequence_of_statements()?;
        self.expect_word("end")?;
        self.optional_designator()?;
        self.expect_symbol(";")?;
        Ok(SubprogramForm::Body)
    }

    fn subprogram_specification(&mut self) -> Result<(), Refusal> {
        if self.eat_word("procedure") {
            self.defining_name()?;
            if self.at_symbol("(") {
                self.parameter_profile()?;
            }
            return Ok(());
        }
        self.expect_word("function")?;
        if self.at_kind(TokenKind::String) {
            // An operator symbol, as in `function "+" (...)`.
            self.position += 1;
        } else {
            self.defining_name()?;
        }
        if self.at_symbol("(") {
            self.parameter_profile()?;
        }
        self.expect_word("return")?;
        self.return_subtype()
    }

    fn return_subtype(&mut self) -> Result<(), Refusal> {
        if self.at_word("not") || self.at_word("access") {
            return self.subtype_indication();
        }
        self.name()?;
        Ok(())
    }

    fn optional_designator(&mut self) -> Result<(), Refusal> {
        if self.at_kind(TokenKind::Identifier) {
            self.defining_name()?;
        } else if self.at_kind(TokenKind::String) {
            self.position += 1;
        }
        Ok(())
    }

    fn parameter_profile(&mut self) -> Result<(), Refusal> {
        self.expect_symbol("(")?;
        loop {
            self.identifier_list()?;
            self.expect_symbol(":")?;
            let _ = self.eat_word("aliased");
            let _ = self.eat_word("in");
            let _ = self.eat_word("out");
            if self.at_word("access") {
                self.access_definition()?;
            } else {
                self.subtype_indication()?;
            }
            if self.eat_symbol(":=") {
                self.expression()?;
            }
            if !self.eat_symbol(";") {
                break;
            }
        }
        self.expect_symbol(")")
    }

    /// `aspect_specification ::= "with" aspect_mark ["=>" expression] {"," ...}`
    ///
    /// The caller must have already consumed any `with` that belongs to a
    /// derived type definition, because that `with` is followed by `private`,
    /// `null`, or `record` -- all reserved words, so the two are distinct.
    fn optional_aspect_specification(&mut self) -> Result<(), Refusal> {
        if !self.at_word("with") {
            return Ok(());
        }
        self.position += 1;
        loop {
            self.expect_identifier()?;
            if self.eat_symbol("'") {
                self.expect_identifier()?;
            }
            if self.eat_symbol("=>") {
                self.expression()?;
            }
            if !self.eat_symbol(",") {
                return Ok(());
            }
        }
    }

    // ------------------------------------------------------------ statements

    fn handled_sequence_of_statements(&mut self) -> Result<(), Refusal> {
        self.sequence_of_statements()?;
        if self.eat_word("exception") {
            while self.eat_word("when") {
                if self.at_kind(TokenKind::Identifier)
                    && self
                        .peek_at(1)
                        .is_some_and(|token| token.matches(self.source, TokenKind::Delimiter, ":"))
                {
                    self.position += 2;
                }
                self.discrete_choice_list()?;
                self.expect_symbol("=>")?;
                self.sequence_of_statements()?;
            }
        }
        Ok(())
    }

    fn sequence_of_statements(&mut self) -> Result<(), Refusal> {
        self.enter()?;
        let mut statements = 0usize;
        while !(self.peek().is_none()
            || self.at_word("end")
            || self.at_word("elsif")
            || self.at_word("else")
            || self.at_word("when")
            || self.at_word("exception"))
        {
            self.statement()?;
            statements += 1;
        }
        self.leave();
        if statements == 0 {
            // Ada requires at least one statement, so an empty sequence means
            // the surrounding shape was misread.
            return Err(Refusal::OutsideDeclaredSubset);
        }
        Ok(())
    }

    fn statement(&mut self) -> Result<(), Refusal> {
        while self.eat_symbol("<<") {
            self.expect_identifier()?;
            self.expect_symbol(">>")?;
        }
        if self.at_kind(TokenKind::Identifier)
            && self
                .peek_at(1)
                .is_some_and(|token| token.matches(self.source, TokenKind::Delimiter, ":"))
        {
            // A statement label on a loop or a block.
            self.position += 2;
            return self.compound_statement();
        }
        if self.at_word("pragma") {
            return self.pragma();
        }
        if self.eat_word("null") {
            return self.expect_symbol(";");
        }
        if self.at_word("if")
            || self.at_word("case")
            || self.at_word("loop")
            || self.at_word("while")
            || self.at_word("for")
            || self.at_word("declare")
            || self.at_word("begin")
        {
            return self.compound_statement();
        }
        if self.eat_word("exit") {
            if self.at_kind(TokenKind::Identifier) {
                self.name()?;
            }
            if self.eat_word("when") {
                self.expression()?;
            }
            return self.expect_symbol(";");
        }
        if self.eat_word("goto") {
            self.name()?;
            return self.expect_symbol(";");
        }
        if self.at_word("return") {
            return self.return_statement();
        }
        if self.eat_word("raise") {
            if !self.at_symbol(";") {
                self.name()?;
                if self.eat_word("with") {
                    self.expression()?;
                }
            }
            return self.expect_symbol(";");
        }
        if self.at_kind(TokenKind::Identifier) {
            return self.call_or_assignment();
        }
        // `accept`, `select`, `delay`, `abort`, `requeue`, and `terminate` are
        // outside the declared subset.
        Err(Refusal::OutsideDeclaredSubset)
    }

    fn compound_statement(&mut self) -> Result<(), Refusal> {
        if self.at_word("if") {
            return self.if_statement();
        }
        if self.at_word("case") {
            return self.case_statement();
        }
        if self.at_word("loop") || self.at_word("while") || self.at_word("for") {
            return self.loop_statement();
        }
        if self.at_word("declare") || self.at_word("begin") {
            return self.block_statement();
        }
        Err(Refusal::OutsideDeclaredSubset)
    }

    fn if_statement(&mut self) -> Result<(), Refusal> {
        self.expect_word("if")?;
        self.expression()?;
        self.expect_word("then")?;
        self.sequence_of_statements()?;
        while self.eat_word("elsif") {
            self.expression()?;
            self.expect_word("then")?;
            self.sequence_of_statements()?;
        }
        if self.eat_word("else") {
            self.sequence_of_statements()?;
        }
        self.expect_word("end")?;
        self.expect_word("if")?;
        self.expect_symbol(";")
    }

    fn case_statement(&mut self) -> Result<(), Refusal> {
        self.expect_word("case")?;
        self.expression()?;
        self.expect_word("is")?;
        while self.eat_word("when") {
            self.discrete_choice_list()?;
            self.expect_symbol("=>")?;
            self.sequence_of_statements()?;
        }
        self.expect_word("end")?;
        self.expect_word("case")?;
        self.expect_symbol(";")
    }

    fn loop_statement(&mut self) -> Result<(), Refusal> {
        if self.eat_word("while") {
            self.expression()?;
        } else if self.eat_word("for") {
            self.expect_identifier()?;
            if self.eat_symbol(":") {
                self.subtype_indication()?;
            }
            if !self.eat_word("in") && !self.eat_word("of") {
                return Err(Refusal::OutsideDeclaredSubset);
            }
            let _ = self.eat_word("reverse");
            self.discrete_range()?;
        }
        self.expect_word("loop")?;
        self.sequence_of_statements()?;
        self.expect_word("end")?;
        self.expect_word("loop")?;
        if self.at_kind(TokenKind::Identifier) {
            self.position += 1;
        }
        self.expect_symbol(";")
    }

    fn block_statement(&mut self) -> Result<(), Refusal> {
        if self.eat_word("declare") {
            self.declarative_part()?;
        }
        self.expect_word("begin")?;
        self.handled_sequence_of_statements()?;
        self.expect_word("end")?;
        if self.at_kind(TokenKind::Identifier) {
            self.position += 1;
        }
        self.expect_symbol(";")
    }

    fn return_statement(&mut self) -> Result<(), Refusal> {
        self.expect_word("return")?;
        if self.eat_symbol(";") {
            return Ok(());
        }
        if self.at_kind(TokenKind::Identifier)
            && self
                .peek_at(1)
                .is_some_and(|token| token.matches(self.source, TokenKind::Delimiter, ":"))
        {
            // An Ada 2005 extended return statement.
            self.position += 2;
            let _ = self.eat_word("aliased");
            let _ = self.eat_word("constant");
            self.subtype_indication()?;
            if self.eat_symbol(":=") {
                self.expression()?;
            }
            if self.eat_word("do") {
                self.handled_sequence_of_statements()?;
                self.expect_word("end")?;
                self.expect_word("return")?;
            }
            return self.expect_symbol(";");
        }
        self.expression()?;
        self.expect_symbol(";")
    }

    /// A procedure call statement and an assignment both start with a name.
    fn call_or_assignment(&mut self) -> Result<(), Refusal> {
        let name = self.name()?;
        if self.eat_symbol(":=") {
            self.expression()?;
            return self.expect_symbol(";");
        }
        self.expect_symbol(";")?;
        if let Some(call) = name.call.as_ref() {
            self.record_registration(name.start, call);
        }
        Ok(())
    }

    /// ADR-0045 D2's exact anchor, read off the parse rather than off the text.
    fn record_registration(&mut self, start: usize, call: &CallSuffix) {
        let designator = &self.source[call.designator_start..call.designator_end];
        if !designator.eq_ignore_ascii_case(REGISTER_ROUTINE) {
            return;
        }
        if call.actuals.len() != 3 {
            return;
        }
        if call.actuals[1] != ExprShape::AccessName || call.actuals[2] != ExprShape::StringLiteral {
            return;
        }
        self.admitted.registrations.push(Registration {
            start,
            end: call.end,
        });
    }

    // ----------------------------------------------------------- expressions

    fn expression(&mut self) -> Result<ExprShape, Refusal> {
        self.enter()?;
        let mut shape = self.relation()?;
        loop {
            if self.eat_word("and") {
                let _ = self.eat_word("then");
            } else if self.eat_word("or") {
                let _ = self.eat_word("else");
            } else if !self.eat_word("xor") {
                break;
            }
            self.relation()?;
            shape = ExprShape::Other;
        }
        self.leave();
        Ok(shape)
    }

    fn relation(&mut self) -> Result<ExprShape, Refusal> {
        let mut shape = self.simple_expression()?;
        if self.at_symbol("=")
            || self.at_symbol("/=")
            || self.at_symbol("<")
            || self.at_symbol("<=")
            || self.at_symbol(">")
            || self.at_symbol(">=")
        {
            self.position += 1;
            self.simple_expression()?;
            return Ok(ExprShape::Other);
        }
        let negated = self.at_word("not")
            && self
                .peek_at(1)
                .is_some_and(|token| token.matches(self.source, TokenKind::Reserved, "in"));
        if negated {
            self.position += 1;
        }
        if self.eat_word("in") {
            self.membership_choice_list()?;
            shape = ExprShape::Other;
        }
        Ok(shape)
    }

    fn membership_choice_list(&mut self) -> Result<(), Refusal> {
        loop {
            self.simple_expression()?;
            if self.eat_symbol("..") {
                self.simple_expression()?;
            }
            if !self.eat_symbol("|") {
                return Ok(());
            }
        }
    }

    fn simple_expression(&mut self) -> Result<ExprShape, Refusal> {
        let signed = self.at_symbol("+") || self.at_symbol("-");
        if signed {
            self.position += 1;
        }
        let mut shape = self.term()?;
        if signed {
            shape = ExprShape::Other;
        }
        while self.at_symbol("+") || self.at_symbol("-") || self.at_symbol("&") {
            self.position += 1;
            self.term()?;
            shape = ExprShape::Other;
        }
        Ok(shape)
    }

    fn term(&mut self) -> Result<ExprShape, Refusal> {
        let mut shape = self.factor()?;
        while self.at_symbol("*")
            || self.at_symbol("/")
            || self.at_word("mod")
            || self.at_word("rem")
        {
            self.position += 1;
            self.factor()?;
            shape = ExprShape::Other;
        }
        Ok(shape)
    }

    fn factor(&mut self) -> Result<ExprShape, Refusal> {
        if self.eat_word("abs") || self.eat_word("not") {
            self.primary()?;
            return Ok(ExprShape::Other);
        }
        let shape = self.primary()?;
        if self.eat_symbol("**") {
            self.primary()?;
            return Ok(ExprShape::Other);
        }
        Ok(shape)
    }

    fn primary(&mut self) -> Result<ExprShape, Refusal> {
        self.enter()?;
        let shape = self.primary_inner()?;
        self.leave();
        Ok(shape)
    }

    fn primary_inner(&mut self) -> Result<ExprShape, Refusal> {
        if self.at_kind(TokenKind::Numeric) || self.at_kind(TokenKind::Character) {
            self.position += 1;
            return Ok(ExprShape::Other);
        }
        if self.at_kind(TokenKind::String) {
            // A string literal is also how an operator symbol is spelled, so it
            // is a name when a name suffix follows it.
            let is_name = self.peek_at(1).is_some_and(|token| {
                token.matches(self.source, TokenKind::Delimiter, "(")
                    || token.matches(self.source, TokenKind::Delimiter, ".")
                    || token.matches(self.source, TokenKind::Delimiter, "'")
            });
            if !is_name {
                self.position += 1;
                return Ok(ExprShape::StringLiteral);
            }
            return self.name().map(|name| name.shape);
        }
        if self.eat_word("null") {
            return Ok(ExprShape::Other);
        }
        if self.eat_word("new") {
            self.subtype_indication()?;
            return Ok(ExprShape::Other);
        }
        if self.at_symbol("(") {
            self.parenthesised_associations()?;
            return Ok(ExprShape::Other);
        }
        if self.at_kind(TokenKind::Identifier) {
            return self.name().map(|name| name.shape);
        }
        Err(Refusal::OutsideDeclaredSubset)
    }

    /// One production covers a parenthesised expression, an aggregate, an
    /// actual-parameter part, and an index. All four consume the same tokens,
    /// and this frontend derives no meaning from the distinction.
    fn parenthesised_associations(&mut self) -> Result<Vec<ExprShape>, Refusal> {
        self.enter()?;
        self.expect_symbol("(")?;
        let mut shapes = Vec::new();
        if self.at_word("if") {
            self.conditional_expression()?;
            shapes.push(ExprShape::Other);
        } else if self.at_word("case") {
            self.case_expression()?;
            shapes.push(ExprShape::Other);
        } else if self.at_word("for") {
            self.quantified_expression()?;
            shapes.push(ExprShape::Other);
        } else if self.at_word("null")
            && self
                .peek_at(1)
                .is_some_and(|token| token.matches(self.source, TokenKind::Reserved, "record"))
        {
            self.position += 2;
            shapes.push(ExprShape::Other);
        } else {
            loop {
                shapes.push(self.association()?);
                if self.eat_word("with") {
                    // An extension aggregate; its associations continue here.
                    if self.at_word("null")
                        && self.peek_at(1).is_some_and(|token| {
                            token.matches(self.source, TokenKind::Reserved, "record")
                        })
                    {
                        self.position += 2;
                        break;
                    }
                    shapes.push(self.association()?);
                }
                if !self.eat_symbol(",") {
                    break;
                }
            }
        }
        self.expect_symbol(")")?;
        self.leave();
        Ok(shapes)
    }

    /// `association ::= [choice_list "=>"] ("<>" | expression [".." expression])`
    fn association(&mut self) -> Result<ExprShape, Refusal> {
        if self.at_word("others") {
            self.position += 1;
            self.expect_symbol("=>")?;
            if !self.eat_symbol("<>") {
                self.expression()?;
            }
            return Ok(ExprShape::Other);
        }
        if self.eat_symbol("<>") {
            return Ok(ExprShape::Other);
        }
        let shape = self.expression()?;
        if self.eat_symbol("..") {
            self.simple_expression()?;
            return Ok(ExprShape::Other);
        }
        if self.at_symbol("|") {
            while self.eat_symbol("|") {
                self.discrete_choice()?;
            }
            self.expect_symbol("=>")?;
            if !self.eat_symbol("<>") {
                self.expression()?;
            }
            return Ok(ExprShape::Other);
        }
        if self.eat_symbol("=>") {
            if !self.eat_symbol("<>") {
                self.expression()?;
            }
            return Ok(ExprShape::Other);
        }
        Ok(shape)
    }

    fn discrete_choice_list(&mut self) -> Result<(), Refusal> {
        loop {
            self.discrete_choice()?;
            if !self.eat_symbol("|") {
                return Ok(());
            }
        }
    }

    fn discrete_choice(&mut self) -> Result<(), Refusal> {
        if self.eat_word("others") {
            return Ok(());
        }
        self.discrete_range()
    }

    fn conditional_expression(&mut self) -> Result<(), Refusal> {
        self.expect_word("if")?;
        self.expression()?;
        self.expect_word("then")?;
        self.expression()?;
        while self.eat_word("elsif") {
            self.expression()?;
            self.expect_word("then")?;
            self.expression()?;
        }
        if self.eat_word("else") {
            self.expression()?;
        }
        Ok(())
    }

    fn case_expression(&mut self) -> Result<(), Refusal> {
        self.expect_word("case")?;
        self.expression()?;
        self.expect_word("is")?;
        loop {
            self.expect_word("when")?;
            self.discrete_choice_list()?;
            self.expect_symbol("=>")?;
            self.expression()?;
            if !self.eat_symbol(",") {
                return Ok(());
            }
        }
    }

    fn quantified_expression(&mut self) -> Result<(), Refusal> {
        self.expect_word("for")?;
        // `for some` is refused by the edition pre-pass, so only `for all`
        // reaches here.
        self.expect_word("all")?;
        self.expect_identifier()?;
        if !self.eat_word("in") && !self.eat_word("of") {
            return Err(Refusal::OutsideDeclaredSubset);
        }
        let _ = self.eat_word("reverse");
        self.discrete_range()?;
        self.expect_symbol("=>")?;
        self.expression()?;
        Ok(())
    }

    // ----------------------------------------------------------------- names

    fn name(&mut self) -> Result<NameInfo, Refusal> {
        self.enter()?;
        let start = self.peek().ok_or(Refusal::OutsideDeclaredSubset)?.start;
        if self.at_kind(TokenKind::Identifier)
            || self.at_kind(TokenKind::String)
            || self.at_kind(TokenKind::Character)
        {
            self.position += 1;
        } else {
            return Err(Refusal::OutsideDeclaredSubset);
        }
        // A name is "plain" while every suffix so far is a selected component
        // naming an identifier. Only a plain prefix may carry the `'Access`
        // that ADR-0045 D2 admits as the second actual parameter.
        let mut plain = true;
        let mut shape = ExprShape::Other;
        let mut call: Option<CallSuffix> = None;
        // The identifier that would name a called subprogram: the last one
        // before an actual-parameter part.
        let mut designator = (
            self.tokens[self.position - 1].start,
            self.tokens[self.position - 1].end,
        );

        loop {
            if self.at_symbol(".") {
                self.position += 1;
                shape = ExprShape::Other;
                call = None;
                if self.eat_word("all") {
                    plain = false;
                    continue;
                }
                let selected = self.advance()?;
                if !matches!(
                    selected.kind,
                    TokenKind::Identifier | TokenKind::String | TokenKind::Character
                ) {
                    return Err(Refusal::OutsideDeclaredSubset);
                }
                if selected.kind == TokenKind::Identifier {
                    designator = (selected.start, selected.end);
                } else {
                    plain = false;
                }
                continue;
            }
            if self.at_symbol("(") {
                let actuals = self.parenthesised_associations()?;
                let end = self.tokens[self.position - 1].end;
                call = Some(CallSuffix {
                    designator_start: designator.0,
                    designator_end: designator.1,
                    actuals,
                    end,
                });
                plain = false;
                shape = ExprShape::Other;
                continue;
            }
            if self.at_symbol("'") {
                self.position += 1;
                call = None;
                if self.at_symbol("(") {
                    // A qualified expression, as in `Integer'(1)`.
                    self.parenthesised_associations()?;
                    plain = false;
                    shape = ExprShape::Other;
                    continue;
                }
                let designator_token = self.advance()?;
                let is_attribute = designator_token.kind == TokenKind::Identifier
                    || matches!(designator_token.kind, TokenKind::Reserved)
                        && ["access", "delta", "digits", "mod", "range"]
                            .iter()
                            .any(|word| {
                                designator_token
                                    .text(self.source)
                                    .eq_ignore_ascii_case(word)
                            });
                if !is_attribute {
                    return Err(Refusal::OutsideDeclaredSubset);
                }
                let is_access = designator_token
                    .text(self.source)
                    .eq_ignore_ascii_case("access");
                shape = if plain && is_access {
                    ExprShape::AccessName
                } else {
                    ExprShape::Other
                };
                plain = false;
                if self.at_symbol("(") {
                    self.parenthesised_associations()?;
                    shape = ExprShape::Other;
                }
                continue;
            }
            break;
        }
        self.leave();
        Ok(NameInfo { start, shape, call })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubprogramForm {
    Declaration,
    Body,
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
                        procedure Register_Tests (T : in out Test_Case) is\nbegin\n";
    const TAIL: &str = "end Register_Tests;\nend Catalog_Tests;\n";

    fn body(statements: &str) -> String {
        format!("{HEAD}{statements}{TAIL}")
    }

    fn parse(source: &str) -> Result<AdmittedUnit, Refusal> {
        parse_compilation(source)
    }

    fn registrations(source: &str) -> usize {
        parse(source).expect("parse").registrations.len()
    }

    #[test]
    fn the_admitted_call_anchors_bare_prefixed_and_multi_line() {
        let parsed = parse(&body(
            "Register_Routine (T, Loads'Access, \"loads\");\n\
             Registration.Register_Routine\n  (T, Filters'Access, \"filters\");\n\
             AUnit.Test_Cases.Registration.Register_Routine (T, Sorts'Access, \"sorts\");\n",
        ))
        .expect("parse");
        assert_eq!(parsed.registrations.len(), 3);
        assert!(parsed.aunit_context_clause);
    }

    #[test]
    fn the_registration_span_covers_the_whole_dotted_callee() {
        let source = body("Registration.Register_Routine (T, Loads'Access, \"loads\");\n");
        let parsed = parse(&source).expect("parse");
        let span = parsed.registrations[0];
        assert_eq!(
            &source[span.start..span.end],
            "Registration.Register_Routine (T, Loads'Access, \"loads\")"
        );
    }

    #[test]
    fn only_a_real_context_clause_proves_the_import() {
        // A `with` inside a type extension is not a context clause, and the old
        // text scan could not tell the two apart.
        let parsed = parse(
            "with Ada.Text_IO;\npackage body Catalog_Tests is\n\
               type AUnit_Like is new Base with null record;\n\
               procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
               Register_Routine (T, Loads'Access, \"loads\");\n\
               end Register_Tests;\nend Catalog_Tests;\n",
        )
        .expect("parse");
        assert!(!parsed.aunit_context_clause);
        assert_eq!(parsed.registrations.len(), 1);
    }

    #[test]
    fn a_registration_only_anchors_as_a_procedure_call_statement() {
        // A function result named `Register_Routine` is not AUnit's procedure,
        // and the text scan this parser replaces could not tell them apart.
        assert_eq!(
            registrations(&body(
                "Handle := Register_Routine (T, Loads'Access, \"loads\");\n"
            )),
            0
        );
        assert_eq!(
            registrations(&body(
                "Report (Register_Routine (T, Loads'Access, \"loads\"));\n"
            )),
            0
        );
    }

    #[test]
    fn a_body_at_the_input_ceiling_parses_within_the_bounded_budget() {
        let mut source = String::from("with AUnit.Test_Cases;\npackage body Big is\n");
        while source.len() < 1_000_000 {
            source.push_str(
                "procedure P (A : in out Natural) is\n   L : constant Natural := 16#FF#;\n\
                 begin\n   for I in 1 .. 10 loop\n      A := A + Table (I)'Length * L;\n\
                 end loop;\n   Register_Routine (T, Loads'Access, \"loads\");\nend P;\n",
            );
        }
        source.push_str("end Big;\n");
        let started = std::time::Instant::now();
        let parsed = parse(&source).expect("parse");
        assert!(parsed.registrations.len() > 1_000);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "a bounded parse of the input ceiling took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn the_argument_shape_is_required() {
        for call in [
            "Register_Routine (T, Loads'Access);",
            "Register_Routine (T, Routine_Ptr, \"loads\");",
            "Register_Routine (T, Loads'Access, Description);",
            "Register_Routine (T, Loads'Address, \"loads\");",
            "Register_Routine (T, Loads'Access, \"loads\", Extra);",
            // A concatenation is not a string literal.
            "Register_Routine (T, Loads'Access, \"a\" & \"b\");",
            // A call result is not a plain name, so its `'Access` is not the
            // admitted second actual.
            "Register_Routine (T, Table (1)'Access, \"loads\");",
            "My_Register_Routine (T, Loads'Access, \"loads\");",
            // Named notation for the routine and the description is not the
            // admitted positional shape.
            "Register_Routine (T, Routine => Loads'Access, Name => \"loads\");",
        ] {
            assert_eq!(registrations(&body(&format!("{call}\n"))), 0, "{call}");
        }
    }

    #[test]
    fn a_nested_call_in_the_first_argument_does_not_split_the_arguments() {
        assert_eq!(
            registrations(&body(
                "Register_Routine (Fixture (T, 1), Loads'Access, \"loads\");\n"
            )),
            1
        );
    }

    #[test]
    fn a_registration_in_a_comment_or_a_string_never_anchors() {
        assert_eq!(
            registrations(&body(
                "--  Register_Routine (T, In_A_Comment'Access, \"no\");\n\
                 Note := \"Register_Routine (T, In_A_String'Access, \"\"no\"\")\";\n"
            )),
            0
        );
    }

    #[test]
    fn case_insensitivity_follows_the_language() {
        let parsed = parse(
            "WITH Aunit.Test_Cases;\nPACKAGE BODY Catalog_Tests IS\n\
               PROCEDURE Register_Tests (T : IN OUT Test_Case) IS\nBEGIN\n\
               REGISTER_ROUTINE (T, Loads'ACCESS, \"loads\");\n\
               END Register_Tests;\nEND Catalog_Tests;\n",
        )
        .expect("parse");
        assert!(parsed.aunit_context_clause);
        assert_eq!(parsed.registrations.len(), 1);
    }

    #[test]
    fn an_edition_sensitive_word_outside_the_overriding_indicator_is_refused() {
        for declaration in [
            "Interface : Boolean := False;",
            "Some : Natural := 1;",
            "Parallel : Boolean := False;",
            "Synchronized : Boolean := False;",
        ] {
            let source = format!(
                "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n{declaration}\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\n\
                 end Register_Tests;\nend Catalog_Tests;\n"
            );
            assert_eq!(
                parse(&source),
                Err(Refusal::EditionSensitiveWord),
                "{declaration}"
            );
        }
    }

    #[test]
    fn an_overriding_indicator_is_the_one_admitted_placement() {
        let parsed = parse(
            "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
               overriding function Name (T : Test_Case) return Message_String is\n\
               begin\n   return Format (\"catalog\");\nend Name;\n\
               not overriding procedure Set_Up (T : in out Test_Case) is\nbegin\n null;\nend Set_Up;\n\
               procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
               Register_Routine (T, Loads'Access, \"loads\");\n\
               end Register_Tests;\nend Catalog_Tests;\n",
        )
        .expect("parse");
        assert_eq!(parsed.registrations.len(), 1);
    }

    #[test]
    fn an_edition_selecting_pragma_is_refused_and_a_member_of_the_set_is_not() {
        assert_eq!(
            parse(&format!("pragma Ada_83;\n{}", body(""))),
            Err(Refusal::LanguageEditionPragma)
        );
        assert_eq!(
            parse(&format!("pragma Extensions_Allowed (On);\n{}", body(""))),
            Err(Refusal::LanguageEditionPragma)
        );
        for pragma in ["pragma Ada_95;", "pragma Ada_2012;", "pragma Ada_2022;"] {
            let source = format!(
                "{pragma}\n{}",
                body("Register_Routine (T, Loads'Access, \"loads\");\n")
            );
            assert_eq!(registrations(&source), 1, "{pragma}");
        }
    }

    #[test]
    fn constructs_outside_the_declared_subset_abstain_rather_than_recover() {
        for declaration in [
            "task body Worker is begin null; end Worker;",
            "protected body Guard is end Guard;",
            "generic\n type Element is private;\n procedure Swap (A : in out Element);",
            "for Flags'Address use System.Null_Address;",
        ] {
            let source = format!(
                "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n{declaration}\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\n\
                 end Register_Tests;\nend Catalog_Tests;\n"
            );
            assert_eq!(
                parse(&source),
                Err(Refusal::OutsideDeclaredSubset),
                "{declaration}"
            );
        }
        for statement in [
            "accept Ready;",
            "select\n   accept Ready;\nelse\n   null;\nend select;",
            "delay 1.0;",
            "abort Worker;",
        ] {
            assert_eq!(
                parse(&body(&format!("{statement}\n"))),
                Err(Refusal::OutsideDeclaredSubset),
                "{statement}"
            );
        }
    }

    #[test]
    fn a_subunit_and_a_spec_are_not_admitted_compilation_units() {
        assert_eq!(
            parse(
                "with AUnit.Test_Cases;\nseparate (Catalog_Tests)\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\nend Register_Tests;\n"
            ),
            Err(Refusal::OutsideDeclaredSubset)
        );
        assert_eq!(
            parse("with AUnit.Test_Cases;\npackage Catalog_Tests is\nend Catalog_Tests;\n"),
            Err(Refusal::OutsideDeclaredSubset)
        );
    }

    #[test]
    fn a_library_level_subprogram_body_is_admitted() {
        assert_eq!(
            registrations(
                "with AUnit.Test_Cases;\nprocedure Register_Tests (T : in out Test_Case) is\n\
                 begin\n   Register_Routine (T, Loads'Access, \"loads\");\nend Register_Tests;\n"
            ),
            1
        );
    }

    #[test]
    fn deep_nesting_hits_the_ceiling_instead_of_the_stack() {
        let source = body(&format!(
            "X := {}1{};\n",
            "(".repeat(MAX_DEPTH * 4),
            ")".repeat(MAX_DEPTH * 4)
        ));
        assert_eq!(parse(&source), Err(Refusal::NestingLimit));
    }

    #[test]
    fn ordinary_ada_around_the_anchor_still_parses() {
        let parsed = parse(
            "with AUnit.Assertions;\nwith AUnit.Test_Cases;\nuse AUnit.Assertions;\n\
             package body Catalog_Tests is\n\
             type Test_Case is new AUnit.Test_Cases.Test_Case with null record;\n\
             subtype Small is Natural range 0 .. 10;\n\
             Table : constant array (1 .. 3) of Natural := (1, 2, 3);\n\
             Mask  : constant Natural := 16#FF#;\n\
             Tick  : constant Character := ''';\n\
             Empty : exception;\n\
             function Score (Value : Natural := 0) return Natural is\n\
             begin\n\
                if Value > 10 then\n   return 10;\n   elsif Value in 1 .. 9 then\n\
                   return Value;\n   else\n      return 0;\n   end if;\n\
             end Score;\n\
             procedure Loads (T : in out AUnit.Test_Cases.Test_Case'Class) is\n\
                Total : Natural := 0;\n\
             begin\n\
                for Index in Table'Range loop\n      Total := Total + Table (Index);\n   end loop;\n\
                case Total is\n      when 0 => null;\n      when others => Assert (True, \"loads\");\n\
                end case;\n\
                declare\n      Local : constant Natural := Total;\n   begin\n\
                   Assert (Local > 0, \"positive\");\n   end;\n\
             exception\n   when Empty => null;\n   when others => raise;\n\
             end Loads;\n\
             procedure Register_Tests (T : in out Test_Case) is\n\
             begin\n   Register_Routine (T, Loads'Access, \"loads the catalog\");\n\
             end Register_Tests;\n\
             end Catalog_Tests;\n",
        )
        .expect("parse ordinary Ada");
        assert_eq!(parsed.registrations.len(), 1);
        assert!(parsed.aunit_context_clause);
    }
}
