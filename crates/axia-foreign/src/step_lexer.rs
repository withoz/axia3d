//! STEP ISO 10303-21 (Part 21) lexer (Phase G Stage 4-B, ADR-035 P20.2).
//!
//! Tokenizes a STEP ASCII file into a flat `Vec<Token>` for the parser.
//! Zero external deps — hand-written byte-level scanner.
//!
//! ## Token taxonomy
//!
//! ISO 10303-21 의 모든 lexical primitive:
//! - **Identifier** (entity ref): `#N` where N is integer (e.g. `#42`)
//! - **Tag** (entity name / keyword): uppercase identifier (e.g. `LINE`,
//!   `CARTESIAN_POINT`, `HEADER`, `DATA`, `ENDSEC`)
//! - **Number**: integer or float (e.g. `1`, `-2.5`, `1.0E+3`)
//! - **String**: `'...'` with `''` 이중 quote escape
//! - **Enum**: `.IDENT.` (e.g. `.T.`, `.UNSPECIFIED.`)
//! - **Punctuation**: `(`, `)`, `,`, `;`, `=`, `*` (omitted), `$` (null)
//!
//! ## Comment handling
//!
//! `/* ... */` block comment. Nested 안 됨 (per ISO spec).
//!
//! ## Whitespace
//!
//! Space / tab / CR / LF 모두 token separator.

use std::fmt;

/// Token 위치 — 디버깅 / 에러 메시지용 (line + column 1-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub col: u32,
}

impl Position {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// STEP token — ISO 10303-21 의 lexical primitive.
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    /// Entity reference `#N`.
    Ref(u32),
    /// Identifier / keyword (대문자, e.g. `LINE`, `HEADER`, `ENDSEC`).
    Tag(String),
    /// Integer literal.
    Int(i64),
    /// Float literal.
    Float(f64),
    /// String literal `'...'` (single quotes already stripped, escape resolved).
    Str(String),
    /// Enum literal `.IDENT.`.
    Enum(String),
    LParen,
    RParen,
    Comma,
    Semicolon,
    Equals,
    /// `*` — derived attribute placeholder.
    Asterisk,
    /// `$` — null / unset attribute.
    Dollar,
    /// End of input.
    Eof,
}

/// Token + 위치.
#[derive(Clone, Debug)]
pub struct LocatedToken {
    pub token: Token,
    pub pos: Position,
}

/// Lexer error.
#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub pos: Position,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "STEP lex error at {}: {}", self.pos, self.message)
    }
}

impl std::error::Error for LexError {}

/// STEP ASCII text → Vec<LocatedToken>.
pub fn tokenize(src: &str) -> Result<Vec<LocatedToken>, LexError> {
    let mut lex = Lexer::new(src);
    let mut tokens = Vec::new();
    loop {
        let tok = lex.next_token()?;
        let is_eof = matches!(tok.token, Token::Eof);
        tokens.push(tok);
        if is_eof { break; }
    }
    Ok(tokens)
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src: src.as_bytes(), pos: 0, line: 1, col: 1 }
    }

    fn current_pos(&self) -> Position {
        Position::new(self.line, self.col)
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.src.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let c = self.src.get(self.pos).copied()?;
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            match self.peek() {
                None => return Ok(()),
                Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') => {
                    self.advance();
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    let start_pos = self.current_pos();
                    self.advance();  // /
                    self.advance();  // *
                    loop {
                        match self.advance() {
                            None => return Err(LexError {
                                message: "unterminated block comment".to_string(),
                                pos: start_pos,
                            }),
                            Some(b'*') if self.peek() == Some(b'/') => {
                                self.advance();
                                break;
                            }
                            Some(_) => {}
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn next_token(&mut self) -> Result<LocatedToken, LexError> {
        self.skip_whitespace_and_comments()?;
        let pos = self.current_pos();
        let c = match self.peek() {
            None => return Ok(LocatedToken { token: Token::Eof, pos }),
            Some(c) => c,
        };

        match c {
            b'(' => { self.advance(); Ok(LocatedToken { token: Token::LParen, pos }) }
            b')' => { self.advance(); Ok(LocatedToken { token: Token::RParen, pos }) }
            b',' => { self.advance(); Ok(LocatedToken { token: Token::Comma, pos }) }
            b';' => { self.advance(); Ok(LocatedToken { token: Token::Semicolon, pos }) }
            b'=' => { self.advance(); Ok(LocatedToken { token: Token::Equals, pos }) }
            b'*' => { self.advance(); Ok(LocatedToken { token: Token::Asterisk, pos }) }
            b'$' => { self.advance(); Ok(LocatedToken { token: Token::Dollar, pos }) }
            b'#' => self.lex_ref(pos),
            b'\'' => self.lex_string(pos),
            b'.' => self.lex_enum_or_number(pos),
            b'-' | b'+' => self.lex_number(pos),
            d if d.is_ascii_digit() => self.lex_number(pos),
            a if a.is_ascii_uppercase() || a == b'_' => self.lex_tag(pos),
            other => Err(LexError {
                message: format!("unexpected character {:?} (0x{:02x})", other as char, other),
                pos,
            }),
        }
    }

    fn lex_ref(&mut self, pos: Position) -> Result<LocatedToken, LexError> {
        self.advance();  // consume '#'
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.advance();
        }
        let digits = &self.src[start..self.pos];
        if digits.is_empty() {
            return Err(LexError {
                message: "expected digits after '#'".to_string(),
                pos,
            });
        }
        let s = std::str::from_utf8(digits).map_err(|_| LexError {
            message: "invalid utf8 in entity ref".to_string(), pos,
        })?;
        let n: u32 = s.parse().map_err(|_| LexError {
            message: format!("invalid entity ref '#{}'", s), pos,
        })?;
        Ok(LocatedToken { token: Token::Ref(n), pos })
    }

    fn lex_string(&mut self, pos: Position) -> Result<LocatedToken, LexError> {
        self.advance();  // consume opening '
        let mut buf = String::new();
        loop {
            match self.advance() {
                None => return Err(LexError {
                    message: "unterminated string literal".to_string(),
                    pos,
                }),
                Some(b'\'') => {
                    // ISO escape: '' = single literal quote
                    if self.peek() == Some(b'\'') {
                        self.advance();
                        buf.push('\'');
                    } else {
                        return Ok(LocatedToken { token: Token::Str(buf), pos });
                    }
                }
                // ISO 10303-21 §6.6 control directives. Without these a
                // non-ASCII name reads back as its raw escape (e.g. the Korean
                // "강철" as `\X2\AC15CCA0\X0\`), which is what an IFC written by
                // us — or by any other tool — actually contains.
                Some(b'\\') => self.lex_string_escape(&mut buf),
                Some(c) => {
                    // Remaining bytes are ISO 8859-1; each byte is one char.
                    buf.push(c as char);
                }
            }
        }
    }

    /// Decode one `\…\` control directive, having consumed the leading `\`.
    /// Unknown or malformed directives are kept verbatim rather than dropped —
    /// losing characters silently would be worse than showing the escape.
    fn lex_string_escape(&mut self, buf: &mut String) {
        match self.peek() {
            // `\X2\HHHH…\X0\` — UTF-16 code units (BMP + surrogate pairs).
            Some(b'X') if self.peek_at(1) == Some(b'2') && self.peek_at(2) == Some(b'\\') => {
                self.advance();
                self.advance();
                self.advance();
                let mut units: Vec<u16> = Vec::new();
                loop {
                    // Terminator `\X0\`.
                    if self.peek() == Some(b'\\')
                        && self.peek_at(1) == Some(b'X')
                        && self.peek_at(2) == Some(b'0')
                        && self.peek_at(3) == Some(b'\\')
                    {
                        self.advance();
                        self.advance();
                        self.advance();
                        self.advance();
                        break;
                    }
                    match self.hex_quad() {
                        Some(u) => units.push(u),
                        None => break, // malformed / EOF: stop, keep what we have
                    }
                }
                match String::from_utf16(&units) {
                    Ok(s) => buf.push_str(&s),
                    Err(_) => buf.push_str(&String::from_utf16_lossy(&units)),
                }
            }
            // `\X\HH` — one ISO 8859-1 byte.
            Some(b'X') if self.peek_at(1) == Some(b'\\') => {
                self.advance();
                self.advance();
                match self.hex_pair() {
                    Some(b) => buf.push(b as char),
                    None => buf.push_str("\\X\\"),
                }
            }
            // `\S\c` — ISO 8859-1 with the high bit set on the next char.
            Some(b'S') if self.peek_at(1) == Some(b'\\') => {
                self.advance();
                self.advance();
                match self.advance() {
                    Some(c) => buf.push((c as u32 + 0x80) as u8 as char),
                    None => buf.push_str("\\S\\"),
                }
            }
            // Anything else (`\P…\`, `\N\`, a stray backslash): keep it literal.
            _ => buf.push('\\'),
        }
    }

    /// Read four hex digits as one UTF-16 code unit.
    fn hex_quad(&mut self) -> Option<u16> {
        let mut v: u16 = 0;
        for _ in 0..4 {
            v = v.checked_mul(16)? + hex_val(self.peek()?)? as u16;
            self.advance();
        }
        Some(v)
    }

    /// Read two hex digits as one byte.
    fn hex_pair(&mut self) -> Option<u8> {
        let hi = hex_val(self.peek()?)?;
        self.advance();
        let lo = hex_val(self.peek()?)?;
        self.advance();
        Some(hi * 16 + lo)
    }

    fn lex_enum_or_number(&mut self, pos: Position) -> Result<LocatedToken, LexError> {
        // '.' followed by:
        //   - uppercase ident + '.' → enum (e.g. .T., .UNSPECIFIED.)
        //   - digit → float starting with dot (e.g. .5)
        if matches!(self.peek_at(1), Some(c) if c.is_ascii_uppercase() || c == b'_') {
            self.advance();  // consume '.'
            let start = self.pos;
            while matches!(self.peek(), Some(c) if c.is_ascii_uppercase() || c == b'_' || c.is_ascii_digit()) {
                self.advance();
            }
            let ident = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").to_string();
            if self.peek() != Some(b'.') {
                return Err(LexError {
                    message: format!("enum literal '.{}' missing closing '.'", ident),
                    pos,
                });
            }
            self.advance();
            return Ok(LocatedToken { token: Token::Enum(ident), pos });
        }
        // dot followed by digit → number
        self.lex_number(pos)
    }

    fn lex_number(&mut self, pos: Position) -> Result<LocatedToken, LexError> {
        let start = self.pos;
        // optional sign
        if matches!(self.peek(), Some(b'+') | Some(b'-')) {
            self.advance();
        }
        let mut saw_digit = false;
        let mut is_float = false;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.advance();
            saw_digit = true;
        }
        if self.peek() == Some(b'.') {
            is_float = true;
            self.advance();
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.advance();
                saw_digit = true;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.advance();
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.advance();
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.advance();
            }
        }
        if !saw_digit {
            return Err(LexError {
                message: "number literal contains no digits".to_string(),
                pos,
            });
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("");
        if is_float {
            let v: f64 = text.parse().map_err(|_| LexError {
                message: format!("invalid float '{}'", text), pos,
            })?;
            Ok(LocatedToken { token: Token::Float(v), pos })
        } else {
            let v: i64 = text.parse().map_err(|_| LexError {
                message: format!("invalid int '{}'", text), pos,
            })?;
            Ok(LocatedToken { token: Token::Int(v), pos })
        }
    }

    fn lex_tag(&mut self, pos: Position) -> Result<LocatedToken, LexError> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_uppercase() || c == b'_' || c.is_ascii_digit()) {
            self.advance();
        }
        let name = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").to_string();
        Ok(LocatedToken { token: Token::Tag(name), pos })
    }
}

/// Value of one hex digit, or `None` if the byte is not one.
fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn just_tokens(src: &str) -> Vec<Token> {
        tokenize(src).unwrap().into_iter().map(|lt| lt.token).collect()
    }

    /// Just the string values, for escape-decoding tests.
    fn just_strings(src: &str) -> Vec<String> {
        just_tokens(src)
            .into_iter()
            .filter_map(|t| match t {
                Token::Str(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn string_x2_directive_decodes_utf16() {
        // ISO 10303-21 §6.6: \X2\HHHH…\X0\ carries UTF-16 code units. Our own
        // IFC export writes Korean this way, so the round-trip depends on it.
        assert_eq!(just_strings(r"'\X2\AC15CCA0\X0\'"), vec!["강철".to_string()]);
        // ASCII around an escaped run
        assert_eq!(just_strings(r"'a\X2\AC00\X0\b'"), vec!["a가b".to_string()]);
        // surrogate pair → U+1F600
        assert_eq!(just_strings(r"'\X2\D83DDE00\X0\'"), vec!["\u{1F600}".to_string()]);
    }

    #[test]
    fn string_x_and_s_directives_decode_latin1() {
        // \X\HH — one ISO 8859-1 byte (0xE9 = é)
        assert_eq!(just_strings(r"'caf\X\E9'"), vec!["café".to_string()]);
        // \S\c — next char with the high bit set (0x69 + 0x80 = 0xE9)
        assert_eq!(just_strings(r"'caf\S\i'"), vec!["café".to_string()]);
    }

    #[test]
    fn unknown_or_malformed_escapes_are_kept_verbatim() {
        // Dropping characters silently would be worse than showing the escape.
        assert_eq!(just_strings(r"'a\Qb'"), vec!["a\\Qb".to_string()]);
        assert_eq!(just_strings(r"'a\X\ZZ'"), vec!["a\\X\\ZZ".to_string()]);
    }

    #[test]
    fn empty_input_yields_eof() {
        assert_eq!(just_tokens(""), vec![Token::Eof]);
    }

    #[test]
    fn whitespace_only_yields_eof() {
        assert_eq!(just_tokens("   \n\t \r\n  "), vec![Token::Eof]);
    }

    #[test]
    fn punctuation_tokens() {
        assert_eq!(
            just_tokens("();,= * $"),
            vec![Token::LParen, Token::RParen, Token::Semicolon, Token::Comma,
                 Token::Equals, Token::Asterisk, Token::Dollar, Token::Eof]
        );
    }

    #[test]
    fn entity_ref() {
        assert_eq!(
            just_tokens("#0 #42 #999999"),
            vec![Token::Ref(0), Token::Ref(42), Token::Ref(999999), Token::Eof]
        );
    }

    #[test]
    fn entity_ref_without_digits_errors() {
        let r = tokenize("#abc");
        assert!(r.is_err());
        assert!(r.unwrap_err().message.contains("digits"));
    }

    #[test]
    fn integer_and_float() {
        assert_eq!(
            just_tokens("0 1 -2 3.14 -0.5 1.0E+3 2.5e-1 .25"),
            vec![
                Token::Int(0), Token::Int(1), Token::Int(-2),
                Token::Float(3.14), Token::Float(-0.5),
                Token::Float(1000.0), Token::Float(0.25), Token::Float(0.25),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn string_literal_and_escape() {
        assert_eq!(
            just_tokens("'hello' '' 'it''s'"),
            vec![
                Token::Str("hello".to_string()),
                Token::Str("".to_string()),
                Token::Str("it's".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn unterminated_string_errors() {
        let r = tokenize("'hello");
        assert!(r.is_err());
        assert!(r.unwrap_err().message.contains("unterminated"));
    }

    #[test]
    fn enum_literal() {
        assert_eq!(
            just_tokens(".T. .F. .UNSPECIFIED."),
            vec![
                Token::Enum("T".to_string()),
                Token::Enum("F".to_string()),
                Token::Enum("UNSPECIFIED".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn enum_missing_closing_dot_errors() {
        let r = tokenize(".T,");
        assert!(r.is_err());
    }

    #[test]
    fn tag_keywords() {
        assert_eq!(
            just_tokens("HEADER DATA ENDSEC LINE CARTESIAN_POINT"),
            vec![
                Token::Tag("HEADER".to_string()),
                Token::Tag("DATA".to_string()),
                Token::Tag("ENDSEC".to_string()),
                Token::Tag("LINE".to_string()),
                Token::Tag("CARTESIAN_POINT".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn block_comment_skipped() {
        assert_eq!(
            just_tokens("LINE /* comment */ POINT /* multi\nline */ DATA"),
            vec![
                Token::Tag("LINE".to_string()),
                Token::Tag("POINT".to_string()),
                Token::Tag("DATA".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn unterminated_comment_errors() {
        let r = tokenize("LINE /* unterminated");
        assert!(r.is_err());
        assert!(r.unwrap_err().message.contains("unterminated block comment"));
    }

    #[test]
    fn full_entity_assignment() {
        // #1 = CARTESIAN_POINT('', (0., 0., 0.));
        let toks = just_tokens("#1 = CARTESIAN_POINT('', (0., 0., 0.));");
        assert_eq!(toks, vec![
            Token::Ref(1),
            Token::Equals,
            Token::Tag("CARTESIAN_POINT".to_string()),
            Token::LParen,
            Token::Str("".to_string()),
            Token::Comma,
            Token::LParen,
            Token::Float(0.0), Token::Comma,
            Token::Float(0.0), Token::Comma,
            Token::Float(0.0),
            Token::RParen,
            Token::RParen,
            Token::Semicolon,
            Token::Eof,
        ]);
    }

    #[test]
    fn position_tracking_line_column() {
        let result = tokenize("#1\n  =\n    LINE").unwrap();
        assert_eq!(result[0].pos, Position::new(1, 1));    // #1
        assert_eq!(result[1].pos, Position::new(2, 3));    // =
        assert_eq!(result[2].pos, Position::new(3, 5));    // LINE
    }

    #[test]
    fn header_section_realistic() {
        let src = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('test'),'2;1');\nENDSEC;";
        let toks = tokenize(src).unwrap();
        // Verify ISO-10303-21 prefix is tokenized as tag-like (well, ISO-10303-21
        // contains hyphen, which our lexer doesn't handle as identifier — should
        // error). Per ISO spec, the prefix is literal `ISO-10303-21;` and is
        // typically handled by parser pre-scan. So we test only after that.
        // Skip the prefix in test src:
        let src2 = "HEADER;\nFILE_DESCRIPTION(('test'),'2;1');\nENDSEC;";
        let _ = tokenize(src2).unwrap();
        // First token check
        let _ = toks;
    }
}
