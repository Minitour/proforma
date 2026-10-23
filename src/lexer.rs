//! Tokenizer for PROforma (`.pf`) text.
//!
//! Mirrors the JLex specification shipped with the reference engine (`PF1.lex`):
//! longest match wins, keywords are reserved (an identifier spelled like a keyword
//! is always the keyword, so such names must be single-quoted), comments are
//! written `/** ... **/`, and quoted atoms accept both `\'` and Arezzo-style `''`.

use serde::Serialize;

/// Byte span plus 1-based line/column of the start position.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub fn to(self, other: Span) -> Span {
        Span {
            start: self.start,
            end: other.end,
            line: self.line,
            col: self.col,
        }
    }
}

macro_rules! keywords {
    ($($variant:ident => $($text:literal)|+),* $(,)?) => {
        /// Reserved words of the PROforma language.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Kw { $($variant),* }

        impl Kw {
            pub fn from_ident(s: &str) -> Option<Kw> {
                match s {
                    $($($text)|+ => Some(Kw::$variant),)*
                    _ => None,
                }
            }

            /// Canonical spelling (first alternative).
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Kw::$variant => keywords!(@first $($text)|+),)*
                }
            }
        }

        /// Every spelling that the lexer treats as a reserved word.
        pub const RESERVED_WORDS: &[&str] = &[$($($text),+),*];
    };
    (@first $first:literal $(| $rest:literal)*) => { $first };
}

keywords! {
    Abort => "abort",
    Action => "action",
    Against => "against",
    And => "and" | "AND",
    Argument => "argument",
    ArgumentName => "argument_name",
    Attributes => "attributes",
    Autonomous => "autonomous",
    Candidate => "candidate",
    Caption => "caption",
    ChoiceMode => "choice_mode",
    Completed => "completed",
    Component => "component",
    Confirming => "confirming",
    Context => "context",
    CycleRepeat => "cycle_repeat",
    CycleUntil => "cycle_until",
    Data => "data",
    Days => "days",
    Decision => "decision",
    DefaultValue => "default_value",
    Derivation => "derivation",
    Dynamic => "dynamic",
    Description => "description",
    Directives => "directives",
    End => "end",
    Enquiry => "enquiry",
    Excluding => "excluding",
    FalseValue => "false_value",
    For => "for",
    Goal => "goal",
    Hours => "hours",
    Includes => "include" | "includes",
    IncludesAll => "includesAll",
    IncludesAny => "includesAny",
    Matches => "matches",
    MatchesAll => "matchesAll",
    MatchesAny => "matchesAny",
    MatchesRx => "matchesRx",
    MatchesRxAll => "matchesRxAll",
    MatchesRxAny => "matchesRxAny",
    MatchesIRx => "matchesIRx",
    MatchesIRxAll => "matchesIRxAll",
    MatchesIRxAny => "matchesIRxAny",
    TermIsa => "term_isa",
    TermIncludes => "term_includes",
    TermIncludesAny => "term_includesAny",
    TermIncludesAll => "term_includesAll",
    TermAssign => "term_assign",
    TermIntersectGeneric => "term_intersectGeneric",
    TermOccurences => "term_occurences",
    Ltwh => "ltwh",
    Mandatory => "mandatory",
    MandatoryValidation => "mandatory_validation",
    Metaprops => "metaprops",
    Minutes => "minutes",
    Multiple => "multiple",
    Numeric => "numeric",
    NumberOfCycles => "number_of_cycles",
    Oneof => "oneof",
    Optional => "optional",
    Or => "or" | "OR",
    ParamValue => "param_value",
    Parameters => "parameters",
    Plan => "plan",
    Postcondition => "postcondition",
    Precondition => "precondition",
    Priority => "priority",
    Group => "group",
    Procedure => "procedure",
    Range => "range",
    Recommendation => "recommendation",
    ResultOf => "result_of",
    ScheduleConstraint => "schedule_constraint",
    Seconds => "seconds",
    Single => "single",
    Source => "source",
    Symbolic => "symbolic",
    SupportMode => "support_mode",
    Task => "task",
    Terminal => "terminal",
    Terminate => "terminate",
    Trigger => "trigger",
    TrueValue => "true_value",
    Type => "type",
    Unit => "unit",
    WaitCondition => "wait_condition",
    WarningCondition => "warning_condition",
    Weeks => "weeks",
    TaskDefinition => "task_definition",
    DataDefinition => "data_definition",
    PreselectCandidates => "preselect_candidates",
    EngineVersion => "engine_version",
    Metadata => "metadata",
    Title => "title",
    Desc => "desc",
    Version => "version",
    Author => "author",
    Cdate => "cdate",
    Mdate => "mdate",
    Function => "function",
    FunctionBody => "function_body",
    FunctionTrigger => "function_trigger",
    Auto => "auto",
    Dynamic483 => "dynamic483",
    Derivation483 => "derivation483",
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Kw(Kw),
    /// Bare identifier or single-quoted atom (value is unescaped).
    Atom {
        value: String,
        quoted: bool,
    },
    /// Double-quoted string (value is unescaped).
    Str(String),
    Int(i64),
    /// Real literal; raw text preserved.
    Double(String),
    Date(String),
    DateTime(String),
    Time(String),
    Append,
    Colon,
    Comma,
    Divide,
    DoubleColon,
    Assign,
    Equals,
    NotEquals,
    FullStop,
    Gt,
    GtEq,
    LBracket,
    LParen,
    Lt,
    LtEq,
    Minus,
    Plus,
    RBracket,
    RParen,
    Semi,
    Times,
    AttrOper,
    Eof,
}

impl Tok {
    pub fn describe(&self) -> String {
        match self {
            Tok::Kw(k) => format!("keyword '{}'", k.as_str()),
            Tok::Atom { value, quoted } => {
                if *quoted {
                    format!("atom '{}'", value)
                } else {
                    format!("identifier '{}'", value)
                }
            }
            Tok::Str(s) => format!("string \"{}\"", s),
            Tok::Int(n) => format!("integer {}", n),
            Tok::Double(d) => format!("number {}", d),
            Tok::Date(d) | Tok::DateTime(d) | Tok::Time(d) => format!("date/time {}", d),
            Tok::Eof => "end of file".into(),
            other => format!("'{}'", other.punct()),
        }
    }

    pub fn punct(&self) -> &'static str {
        match self {
            Tok::Append => "++",
            Tok::Colon => ":",
            Tok::Comma => ",",
            Tok::Divide => "/",
            Tok::DoubleColon => "::",
            Tok::Assign => ":=",
            Tok::Equals => "=",
            Tok::NotEquals => "!=",
            Tok::FullStop => ".",
            Tok::Gt => ">",
            Tok::GtEq => ">=",
            Tok::LBracket => "[",
            Tok::LParen => "(",
            Tok::Lt => "<",
            Tok::LtEq => "<=",
            Tok::Minus => "-",
            Tok::Plus => "+",
            Tok::RBracket => "]",
            Tok::RParen => ")",
            Tok::Semi => ";",
            Tok::Times => "*",
            Tok::AttrOper => "->",
            _ => "?",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{line}:{col}: {message}")]
pub struct LexError {
    pub message: String,
    pub line: u32,
    pub col: u32,
    pub offset: usize,
}

struct Cursor<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Cursor<'a> {
    fn peek(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.pos + ahead).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.src[self.pos..].starts_with(s)
    }

    /// Advance over `n` bytes, tracking line/column (columns count chars).
    fn bump(&mut self, n: usize) {
        let end = self.pos + n;
        for ch in self.src[self.pos..end].chars() {
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        self.pos = end;
    }

    fn here(&self) -> Span {
        Span {
            start: self.pos,
            end: self.pos,
            line: self.line,
            col: self.col,
        }
    }
}

fn is_digit(b: Option<u8>) -> bool {
    matches!(b, Some(b'0'..=b'9'))
}

/// Count consecutive digits starting `from` bytes ahead.
fn digits_at(c: &Cursor, from: usize) -> usize {
    let mut n = 0;
    while is_digit(c.peek(from + n)) {
        n += 1;
    }
    n
}

/// Check that the byte pattern (`d` = digit, anything else literal) matches at the cursor.
fn matches_shape(c: &Cursor, shape: &str) -> bool {
    shape.bytes().enumerate().all(|(i, s)| match s {
        b'd' => is_digit(c.peek(i)),
        lit => c.peek(i) == Some(lit),
    })
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let mut c = Cursor {
        src,
        bytes: src.as_bytes(),
        pos: 0,
        line: 1,
        col: 1,
    };
    let mut out = Vec::new();

    loop {
        // whitespace & comments
        loop {
            match c.peek(0) {
                Some(b' ' | b'\t' | b'\r' | b'\n' | 0x08 | 0x0c) => c.bump(1),
                Some(b'/') if c.starts_with("/**") => {
                    let start = c.here();
                    match src[c.pos + 3..].find("**/") {
                        Some(i) => c.bump(3 + i + 3),
                        None => {
                            return Err(LexError {
                                message: "unterminated comment (comments are /** ... **/)".into(),
                                line: start.line,
                                col: start.col,
                                offset: start.start,
                            })
                        }
                    }
                }
                Some(0xEF) if c.pos == 0 && c.starts_with("\u{feff}") => c.bump(3),
                _ => break,
            }
        }

        let start = c.here();
        let Some(b) = c.peek(0) else {
            out.push(Token {
                tok: Tok::Eof,
                span: start,
            });
            return Ok(out);
        };

        let (tok, len): (Tok, usize) = if b.is_ascii_alphabetic() || b == b'_' {
            let mut n = 1;
            while matches!(c.peek(n), Some(x) if x.is_ascii_alphanumeric() || x == b'_') {
                n += 1;
            }
            let word = &src[c.pos..c.pos + n];
            let tok = match Kw::from_ident(word) {
                Some(k) => Tok::Kw(k),
                None => Tok::Atom {
                    value: word.to_string(),
                    quoted: false,
                },
            };
            (tok, n)
        } else if b.is_ascii_digit() || (b == b'.' && is_digit(c.peek(1))) {
            lex_number(&c)
        } else if b == b'\'' || b == b'"' {
            let (tok, len) = lex_quoted(&c, b)?;
            (tok, len)
        } else {
            let two = c.src.get(c.pos..c.pos + 2).unwrap_or("");
            match two {
                "++" => (Tok::Append, 2),
                "::" => (Tok::DoubleColon, 2),
                ":=" => (Tok::Assign, 2),
                "!=" | "<>" => (Tok::NotEquals, 2),
                ">=" | "=>" => (Tok::GtEq, 2),
                "<=" | "=<" => (Tok::LtEq, 2),
                "->" => (Tok::AttrOper, 2),
                _ => match b {
                    b'#' => (Tok::Append, 1),
                    b':' => (Tok::Colon, 1),
                    b',' => (Tok::Comma, 1),
                    b'/' => (Tok::Divide, 1),
                    b'=' => (Tok::Equals, 1),
                    b'.' => (Tok::FullStop, 1),
                    b'>' => (Tok::Gt, 1),
                    b'[' => (Tok::LBracket, 1),
                    b'(' => (Tok::LParen, 1),
                    b'<' => (Tok::Lt, 1),
                    b'-' => (Tok::Minus, 1),
                    b'+' => (Tok::Plus, 1),
                    b']' => (Tok::RBracket, 1),
                    b')' => (Tok::RParen, 1),
                    b';' => (Tok::Semi, 1),
                    b'*' => (Tok::Times, 1),
                    _ => {
                        let ch = src[c.pos..].chars().next().unwrap();
                        return Err(LexError {
                            message: format!("unexpected character {:?}", ch),
                            line: start.line,
                            col: start.col,
                            offset: start.start,
                        });
                    }
                },
            }
        };
        c.bump(len);
        let mut span = start;
        span.end = c.pos;
        out.push(Token { tok, span });
    }
}

fn lex_number(c: &Cursor) -> (Tok, usize) {
    let text = |n: usize| c.src[c.pos..c.pos + n].to_string();
    // Longest-match among DATETIME / DATE / TIME / DOUBLE / INTEGER.
    if matches_shape(c, "dddd-dd-ddTdd:dd:dd.ddd") {
        return (Tok::DateTime(text(23)), 23);
    }
    if matches_shape(c, "dddd-dd-dd") {
        return (Tok::Date(text(10)), 10);
    }
    if matches_shape(c, "dd:dd:dd.ddd") {
        return (Tok::Time(text(12)), 12);
    }
    let int_len = digits_at(c, 0);
    let mut len = int_len;
    let mut is_double = false;
    if c.peek(len) == Some(b'.') {
        let frac = digits_at(c, len + 1);
        // `{DIGIT}+"."{DIGIT}*` or `"."{DIGIT}+`
        if int_len > 0 || frac > 0 {
            is_double = true;
            len += 1 + frac;
        }
    }
    if is_double && c.peek(len) == Some(b'e') {
        let mut k = len + 1;
        if matches!(c.peek(k), Some(b'+' | b'-')) {
            k += 1;
        }
        let exp = digits_at(c, k);
        if exp > 0 {
            len = k + exp;
        }
    }
    if is_double {
        (Tok::Double(text(len)), len)
    } else {
        let t = text(len);
        // Values beyond i64 are still accepted lexically; keep them as reals.
        match t.parse::<i64>() {
            Ok(n) => (Tok::Int(n), len),
            Err(_) => (Tok::Double(t), len),
        }
    }
}

fn lex_quoted(c: &Cursor, quote: u8) -> Result<(Tok, usize), LexError> {
    let src = &c.src[c.pos..];
    let mut value = String::new();
    let mut chars = src.char_indices().skip(1).peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '\\' {
            match chars.peek() {
                Some(&(_, n)) if n == '\'' || n == '"' || n == '\\' => {
                    value.push(n);
                    chars.next();
                }
                Some(&(_, n)) => {
                    value.push('\\');
                    value.push(n);
                    chars.next();
                }
                None => value.push('\\'),
            }
        } else if ch as u32 == quote as u32 {
            if quote == b'\'' && matches!(chars.peek(), Some(&(_, '\''))) {
                // Arezzo-style doubled quote inside a quoted atom
                value.push('\'');
                chars.next();
            } else {
                let len = i + 1;
                let tok = if quote == b'\'' {
                    Tok::Atom {
                        value,
                        quoted: true,
                    }
                } else {
                    Tok::Str(value)
                };
                return Ok((tok, len));
            }
        } else {
            value.push(ch);
        }
    }
    Err(LexError {
        message: format!(
            "unterminated {}",
            if quote == b'\'' {
                "quoted atom"
            } else {
                "string"
            }
        ),
        line: c.line,
        col: c.col,
        offset: c.pos,
    })
}

/// True if `s` can be written as a bare (unquoted) atom.
pub fn is_bare_atom(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {}
        _ => return false,
    }
    it.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') && Kw::from_ident(s).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        tokenize(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn keywords_and_atoms() {
        assert_eq!(
            toks("plan :: 'my plan' ;"),
            vec![
                Tok::Kw(Kw::Plan),
                Tok::DoubleColon,
                Tok::Atom {
                    value: "my plan".into(),
                    quoted: true
                },
                Tok::Semi,
                Tok::Eof
            ]
        );
    }

    #[test]
    fn numbers_and_dates() {
        assert_eq!(
            toks("2020-01-02 12 3.5 .5 4. 10:11:12.123"),
            vec![
                Tok::Date("2020-01-02".into()),
                Tok::Int(12),
                Tok::Double("3.5".into()),
                Tok::Double(".5".into()),
                Tok::Double("4.".into()),
                Tok::Time("10:11:12.123".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn escapes_and_comments() {
        assert_eq!(
            toks("/** c **/ 'it''s \\'x\\'' \"a\\\"b\""),
            vec![
                Tok::Atom {
                    value: "it's 'x'".into(),
                    quoted: true
                },
                Tok::Str("a\"b".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn operators() {
        assert_eq!(
            toks("a := b <> c => d =< e -> f # g"),
            vec![
                Tok::Atom {
                    value: "a".into(),
                    quoted: false
                },
                Tok::Assign,
                Tok::Atom {
                    value: "b".into(),
                    quoted: false
                },
                Tok::NotEquals,
                Tok::Atom {
                    value: "c".into(),
                    quoted: false
                },
                Tok::GtEq,
                Tok::Atom {
                    value: "d".into(),
                    quoted: false
                },
                Tok::LtEq,
                Tok::Atom {
                    value: "e".into(),
                    quoted: false
                },
                Tok::AttrOper,
                Tok::Atom {
                    value: "f".into(),
                    quoted: false
                },
                Tok::Append,
                Tok::Atom {
                    value: "g".into(),
                    quoted: false
                },
                Tok::Eof
            ]
        );
    }
}
