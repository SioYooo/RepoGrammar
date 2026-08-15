//! Ada lexical analyser for the ADR-0045 declared subset.
//!
//! This is the first half of a frontend, not a scanner: it produces an Ada
//! token stream or it refuses the file. There is no third outcome, and no
//! recovery. Everything the parser in [`super::syntax`] decides rests on token
//! boundaries established here.
//!
//! Two properties are load-bearing and are the reason this layer exists at all.
//!
//! **The tick is resolved by the previous token, not by the previous byte.**
//! Ada's single quote opens a character literal and introduces an attribute. A
//! tick is an attribute tick exactly when the token before it can end a name:
//! an identifier, `)`, `all`, or an operator symbol. Otherwise it opens a
//! character literal, which is exactly one character wide -- which is what makes
//! `'''` need no special case.
//!
//! **A word's classification must not depend on the Ada edition.** `interface`,
//! `overriding`, and `synchronized` became reserved in Ada 2005, `some` in Ada
//! 2012, and `parallel` in Ada 2022. The same text therefore has two different
//! parses under two different editions, and RepoGrammar cannot see which
//! edition a GPR project selects. Those five words are lexed as their own kind
//! so the parser can refuse them everywhere except the one position where only
//! one reading is legal.

/// Why the frontend refused a file.
///
/// This is the frontend's complete abstention vocabulary. Every variant is a
/// fixed token: no source text, identifier, literal, or line number is ever
/// carried in a refusal, because a refusal reaches operator-visible surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A `gnatprep` conditional directive. The file is preprocessor input, not
    /// an Ada compilation unit.
    PreprocessorDirective,
    /// A `gnatprep` `$symbol` substitution. The text is a template whose
    /// expansion RepoGrammar cannot compute.
    PreprocessorSubstitution,
    /// A configuration pragma that re-selects the language edition or enables
    /// non-standard syntax, which is exactly what the invariance argument
    /// assumes fixed.
    LanguageEditionPragma,
    /// A word that is an identifier in one edition of the declared set and a
    /// reserved word in another.
    EditionSensitiveWord,
    /// An obsolescent replacement character from Ada RM J.2, which moves string
    /// and based-literal boundaries.
    ReplacementCharacter,
    /// A string or character literal left open.
    UnterminatedLiteral,
    /// A character that is not part of the declared subset's lexical elements.
    UnsupportedCharacter,
    /// A construct the declared grammar does not admit.
    OutsideDeclaredSubset,
    /// The bounded nesting ceiling was reached.
    NestingLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// A name that is a reserved word in no edition of the declared set.
    Identifier,
    /// A reserved word in every edition of the declared set.
    Reserved,
    /// A word that is reserved in some editions of the declared set and an
    /// identifier in others.
    EditionSensitive,
    Numeric,
    Character,
    String,
    /// Punctuation, including the compound delimiters.
    Delimiter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub(crate) fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }

    /// Ada is case-insensitive, so every word comparison is.
    pub(crate) fn matches(&self, source: &str, kind: TokenKind, word: &str) -> bool {
        self.kind == kind && self.text(source).eq_ignore_ascii_case(word)
    }
}

/// Reserved in Ada 95 and in every later edition of the declared set.
const RESERVED_WORDS: &[&str] = &[
    "abort",
    "abs",
    "abstract",
    "accept",
    "access",
    "aliased",
    "all",
    "and",
    "array",
    "at",
    "begin",
    "body",
    "case",
    "constant",
    "declare",
    "delay",
    "delta",
    "digits",
    "do",
    "else",
    "elsif",
    "end",
    "entry",
    "exception",
    "exit",
    "for",
    "function",
    "generic",
    "goto",
    "if",
    "in",
    "is",
    "limited",
    "loop",
    "mod",
    "new",
    "not",
    "null",
    "of",
    "or",
    "others",
    "out",
    "package",
    "pragma",
    "private",
    "procedure",
    "protected",
    "raise",
    "range",
    "record",
    "rem",
    "renames",
    "requeue",
    "return",
    "reverse",
    "select",
    "separate",
    "subtype",
    "tagged",
    "task",
    "terminate",
    "then",
    "type",
    "until",
    "use",
    "when",
    "while",
    "with",
    "xor",
];

/// Reserved in some editions of the declared set and an identifier in others.
///
/// `interface`, `overriding`, and `synchronized` are Ada 2005 additions; `some`
/// is Ada 2012; `parallel` is Ada 2022.
const EDITION_SENSITIVE_WORDS: &[&str] = &[
    "interface",
    "overriding",
    "parallel",
    "some",
    "synchronized",
];

/// Compound delimiters, longest first so that `<>` never lexes as `<` `>`.
const COMPOUND_DELIMITERS: &[&str] = &["=>", "..", "**", ":=", "/=", ">=", "<=", "<<", ">>", "<>"];

const SINGLE_DELIMITERS: &[u8] = b"&'()*+,-./:;<=>|";

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, Refusal> {
    let bytes = source.as_bytes();
    let mut tokens: Vec<Token> = Vec::new();
    let mut index = 0usize;
    // True while no token has been emitted yet on the current line. `gnatprep`
    // directives are line-leading, which is what separates them from a `#`
    // inside a based literal.
    let mut line_leading = true;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' {
            line_leading = true;
            index += 1;
            continue;
        }
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'-' && bytes.get(index + 1) == Some(&b'-') {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }

        let starts_line = line_leading;
        line_leading = false;
        let token = lex_token(source, bytes, &mut index, starts_line, tokens.last())?;
        tokens.push(token);
    }
    Ok(tokens)
}

fn lex_token(
    source: &str,
    bytes: &[u8],
    index: &mut usize,
    starts_line: bool,
    previous: Option<&Token>,
) -> Result<Token, Refusal> {
    let start = *index;
    let byte = bytes[start];

    if byte.is_ascii_alphabetic() {
        let end = word_end(bytes, start);
        *index = end;
        return Ok(Token {
            kind: classify_word(&source[start..end]),
            start,
            end,
        });
    }
    if byte.is_ascii_digit() {
        let end = numeric_literal_end(bytes, start)?;
        *index = end;
        return Ok(Token {
            kind: TokenKind::Numeric,
            start,
            end,
        });
    }
    if byte == b'"' {
        let end = string_literal_end(bytes, start)?;
        *index = end;
        return Ok(Token {
            kind: TokenKind::String,
            start,
            end,
        });
    }
    if byte == b'\'' {
        if tick_is_attribute(source, previous) {
            *index = start + 1;
            return Ok(Token {
                kind: TokenKind::Delimiter,
                start,
                end: start + 1,
            });
        }
        let end = character_literal_end(source, start)?;
        *index = end;
        return Ok(Token {
            kind: TokenKind::Character,
            start,
            end,
        });
    }
    if byte == b'#' {
        // A `#` reachable here is not part of a based literal, because
        // `numeric_literal_end` consumes those.
        return Err(if starts_line {
            Refusal::PreprocessorDirective
        } else {
            Refusal::UnsupportedCharacter
        });
    }
    if byte == b'$' {
        return Err(Refusal::PreprocessorSubstitution);
    }
    if byte == b'%' || byte == b'!' {
        // Ada RM J.2 lets `%` replace `"` and `!` replace `|`. Both move token
        // boundaries, so the file leaves the declared subset rather than being
        // read under one of the two spellings.
        return Err(Refusal::ReplacementCharacter);
    }
    for compound in COMPOUND_DELIMITERS {
        if source[start..].starts_with(compound) {
            *index = start + compound.len();
            return Ok(Token {
                kind: TokenKind::Delimiter,
                start,
                end: *index,
            });
        }
    }
    if SINGLE_DELIMITERS.contains(&byte) {
        *index = start + 1;
        return Ok(Token {
            kind: TokenKind::Delimiter,
            start,
            end: start + 1,
        });
    }
    Err(Refusal::UnsupportedCharacter)
}

fn classify_word(word: &str) -> TokenKind {
    let folded = word.to_ascii_lowercase();
    if RESERVED_WORDS.contains(&folded.as_str()) {
        return TokenKind::Reserved;
    }
    if EDITION_SENSITIVE_WORDS.contains(&folded.as_str()) {
        return TokenKind::EditionSensitive;
    }
    TokenKind::Identifier
}

fn word_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
        index += 1;
    }
    index
}

/// A tick is an attribute tick exactly when the previous token can end a name.
fn tick_is_attribute(source: &str, previous: Option<&Token>) -> bool {
    let Some(token) = previous else {
        return false;
    };
    match token.kind {
        // `X'Access`, `T'Class'Input`.
        TokenKind::Identifier | TokenKind::EditionSensitive => true,
        // `Ptr.all'Address`.
        TokenKind::Reserved => token.text(source).eq_ignore_ascii_case("all"),
        // `Items (1)'Length`.
        TokenKind::Delimiter => token.text(source) == ")",
        // `Standard."+"'Access` -- an operator symbol is a name.
        TokenKind::String => true,
        TokenKind::Numeric | TokenKind::Character => false,
    }
}

/// A character literal holds exactly one character, so its width is fixed and
/// `'''` needs no special case.
fn character_literal_end(source: &str, start: usize) -> Result<usize, Refusal> {
    let rest = &source[start + 1..];
    let character = rest.chars().next().ok_or(Refusal::UnterminatedLiteral)?;
    let close = start + 1 + character.len_utf8();
    if source.as_bytes().get(close) != Some(&b'\'') {
        return Err(Refusal::UnterminatedLiteral);
    }
    Ok(close + 1)
}

fn string_literal_end(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            if bytes.get(index + 1) == Some(&b'"') {
                index += 2;
                continue;
            }
            return Ok(index + 1);
        }
        // A string literal may not span a line, so a newline inside one is the
        // decidable well-formedness violation ADR-0045 D4 reports.
        if bytes[index] == b'\n' {
            return Err(Refusal::UnterminatedLiteral);
        }
        index += 1;
    }
    Err(Refusal::UnterminatedLiteral)
}

/// `decimal_literal` and `based_literal` per Ada RM 2.4.
fn numeric_literal_end(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let mut index = numeral_end(bytes, start);
    if bytes.get(index) == Some(&b':') {
        // RM J.2 lets `:` replace `#` in a based literal.
        return Err(Refusal::ReplacementCharacter);
    }
    if bytes.get(index) == Some(&b'#') {
        index = based_numeral_end(bytes, index + 1);
        if bytes.get(index) == Some(&b'.') {
            index = based_numeral_end(bytes, index + 1);
        }
        if bytes.get(index) != Some(&b'#') {
            return Err(Refusal::UnterminatedLiteral);
        }
        index += 1;
    } else if bytes.get(index) == Some(&b'.')
        && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
    {
        index = numeral_end(bytes, index + 1);
    }
    if bytes
        .get(index)
        .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'e'))
    {
        let mut exponent = index + 1;
        if matches!(bytes.get(exponent), Some(b'+') | Some(b'-')) {
            exponent += 1;
        }
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            index = numeral_end(bytes, exponent);
        }
    }
    Ok(index)
}

fn numeral_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
        index += 1;
    }
    index
}

fn based_numeral_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<(TokenKind, String)> {
        lex(source)
            .expect("lex")
            .into_iter()
            .map(|token| (token.kind, token.text(source).to_string()))
            .collect()
    }

    #[test]
    fn a_quote_character_literal_is_one_character_wide() {
        // `'''` is the shape that breaks a lexer searching for a closing tick.
        assert_eq!(
            kinds("X := ''';"),
            vec![
                (TokenKind::Identifier, "X".to_string()),
                (TokenKind::Delimiter, ":=".to_string()),
                (TokenKind::Character, "'''".to_string()),
                (TokenKind::Delimiter, ";".to_string()),
            ]
        );
    }

    #[test]
    fn a_tick_after_a_name_is_an_attribute_and_not_a_literal_opener() {
        for source in [
            "Y := Integer'First;",
            "Y := Items (1)'Length;",
            "Y := Ptr.all'Address;",
            "Y := T'Class'Input;",
        ] {
            let lexed = lex(source).expect(source);
            assert!(
                lexed.iter().all(|token| token.kind != TokenKind::Character),
                "{source}"
            );
        }
    }

    #[test]
    fn edition_sensitive_words_get_their_own_kind() {
        for word in EDITION_SENSITIVE_WORDS {
            assert_eq!(classify_word(word), TokenKind::EditionSensitive, "{word}");
            assert_eq!(
                classify_word(&word.to_ascii_uppercase()),
                TokenKind::EditionSensitive,
                "{word}"
            );
        }
        assert_eq!(classify_word("Overriding_Count"), TokenKind::Identifier);
        assert_eq!(classify_word("PROCEDURE"), TokenKind::Reserved);
    }

    #[test]
    fn based_and_decimal_literals_lex_so_the_directive_refusal_cannot_misfire() {
        for source in [
            "X := 16#FF#;",
            "X := 2#1010#E4;",
            "X := 1_000;",
            "X := 3.5E-2;",
        ] {
            let lexed = lex(source).expect(source);
            assert!(
                lexed.iter().any(|token| token.kind == TokenKind::Numeric),
                "{source}"
            );
        }
        assert_eq!(kinds("X := 16#FF#;")[2].1, "16#FF#");
    }

    #[test]
    fn a_line_leading_hash_is_a_preprocessor_directive() {
        assert_eq!(
            lex("#if DEBUG\nX := 1;\n#end if;\n"),
            Err(Refusal::PreprocessorDirective)
        );
        assert_eq!(lex("X := 1 # 2;"), Err(Refusal::UnsupportedCharacter));
    }

    #[test]
    fn preprocessor_substitution_and_replacement_characters_are_refused() {
        assert_eq!(lex("X := $Value;"), Err(Refusal::PreprocessorSubstitution));
        assert_eq!(lex("X := %text%;"), Err(Refusal::ReplacementCharacter));
        assert_eq!(
            lex("case X is when 1 ! 2 => null;"),
            Err(Refusal::ReplacementCharacter)
        );
        assert_eq!(lex("X := 16:FF:;"), Err(Refusal::ReplacementCharacter));
    }

    #[test]
    fn characters_outside_the_declared_subset_are_refused() {
        for source in [
            "X := @ + 1;",
            "X := [1, 2];",
            "X := {1};",
            "X := 1 ^ 2;",
            "Wide := Ü;",
        ] {
            assert_eq!(lex(source), Err(Refusal::UnsupportedCharacter), "{source}");
        }
    }

    #[test]
    fn an_unterminated_literal_is_refused_rather_than_recovered() {
        assert_eq!(
            lex("X := \"never closed\n"),
            Err(Refusal::UnterminatedLiteral)
        );
        assert_eq!(
            lex("X := \"never closed"),
            Err(Refusal::UnterminatedLiteral)
        );
        assert_eq!(lex("X := 'a;"), Err(Refusal::UnterminatedLiteral));
    }

    #[test]
    fn comments_and_strings_may_hold_anything_including_refused_characters() {
        let source = "--  #if $x %y% [z] Ü\nNote : constant String := \"#if $x ''\";\n";
        let lexed = lex(source).expect("lex");
        assert_eq!(
            lexed
                .iter()
                .filter(|token| token.kind == TokenKind::String)
                .count(),
            1
        );
        assert!(lexed.iter().all(|token| token.kind != TokenKind::Character));
    }

    #[test]
    fn compound_delimiters_never_split() {
        assert_eq!(
            kinds("A <> B .. C ** D")
                .into_iter()
                .filter(|(kind, _)| *kind == TokenKind::Delimiter)
                .map(|(_, text)| text)
                .collect::<Vec<_>>(),
            vec!["<>".to_string(), "..".to_string(), "**".to_string()]
        );
    }
}
