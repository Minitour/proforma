//! Recursive-descent parser for PROforma, following the reference LALR grammar
//! (`PF1.cup`): same operator precedence/associativity, same greedy attachment of
//! attributes to nested components/candidates/sources, and the same set of
//! accepted attribute spellings.
//!
//! The parser is slightly more permissive than the reference engine in a few
//! places (attribute ordering, strings where atoms are expected). Each such
//! leniency is reported as a `PF-K*` (Composer compatibility) error so files can be fixed
//! (usually just by re-formatting) before they are opened in Composer.

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::lexer::{tokenize, Kw, Span, Tok, Token};

pub const DATA_TYPES: &[&str] = &[
    "text",
    "integer",
    "real",
    "boolean",
    "datetime",
    "date",
    "time",
    "object",
    "setof_text",
    "setof_integer",
    "setof_real",
];

#[derive(Debug)]
pub struct ParseOutput {
    pub doc: Document,
    pub diagnostics: Vec<Diagnostic>,
}

impl ParseOutput {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == crate::diagnostics::Severity::Error)
    }
}

/// Parse a complete guideline.
pub fn parse(src: &str) -> ParseOutput {
    let tokens = match tokenize(src) {
        Ok(t) => t,
        Err(e) => {
            return ParseOutput {
                doc: Document::default(),
                diagnostics: vec![Diagnostic::error("PF-L001", e.message).at(Span {
                    start: e.offset,
                    end: e.offset,
                    line: e.line,
                    col: e.col,
                })],
            }
        }
    };
    let mut p = Parser::new(src, tokens);
    let doc = p.document();
    ParseOutput {
        doc,
        diagnostics: p.diags,
    }
}

/// Parse a standalone expression (e.g. a precondition body).
pub fn parse_expression(src: &str) -> Result<Expr, Diagnostic> {
    standalone(src, |p| p.expr())
}

/// Parse a standalone assertion (postcondition / param_value body), e.g. `x = 3 and y := z`.
pub fn parse_assertion(src: &str) -> Result<Expr, Diagnostic> {
    standalone(src, |p| p.assertion_expr())
}

/// Where an attribute snippet will be inserted; decides which keywords are legal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetContext {
    Item(ItemKind),
    Component,
    Candidate,
    Source,
    /// caption/description/metaprops only (arguments, warning conditions)
    Generic,
}

/// Parse a sequence of attributes such as `caption :: 'x'; precondition :: a > 1;`.
pub fn parse_attrs(ctx: SnippetContext, src: &str) -> Result<Vec<Attr>, Diagnostic> {
    let tokens = tokenize(src).map_err(|e| {
        Diagnostic::error("PF-L001", e.message).at(Span {
            start: e.offset,
            end: e.offset,
            line: e.line,
            col: e.col,
        })
    })?;
    let mut p = Parser::new(src, tokens);
    let c = match ctx {
        SnippetContext::Item(k) => match k {
            ItemKind::Plan => Ctx::Plan,
            ItemKind::Decision => Ctx::Decision,
            ItemKind::Action => Ctx::Action,
            ItemKind::Enquiry => Ctx::Enquiry,
            ItemKind::Task => Ctx::Task,
            ItemKind::Data => Ctx::Data,
            ItemKind::Function => Ctx::Function,
        },
        SnippetContext::Component => Ctx::Component,
        SnippetContext::Candidate => Ctx::Candidate,
        SnippetContext::Source => Ctx::Source,
        SnippetContext::Generic => Ctx::Generic,
    };
    let mut attrs = Vec::new();
    // data items accept `type` anywhere when edited as a snippet
    if c == Ctx::Data && p.at_kw(Kw::Type) {
        attrs.push(p.attr(Ctx::Data).map_err(|e| p.to_diag(e))?);
    }
    p.attr_list(c, &mut attrs).map_err(|e| p.to_diag(e))?;
    if !p.at(&Tok::Eof) {
        let t = p.peek().clone();
        let what = match &t.tok {
            Tok::Kw(k) => format!(
                "'{}' is not a valid attribute here (target accepts: {})",
                k.as_str(),
                allowed_keywords(c).join(", ")
            ),
            other => format!("unexpected {}", other.describe()),
        };
        return Err(Diagnostic::error("PF-L002", what).at(t.span));
    }
    if let Some(d) = p
        .diags
        .into_iter()
        .find(|d| d.severity == crate::diagnostics::Severity::Error)
    {
        return Err(d);
    }
    Ok(attrs)
}

/// Parse one or more complete top-level components (`plan :: x; ... end plan.` etc.).
pub fn parse_items(src: &str) -> Result<Vec<Item>, Vec<Diagnostic>> {
    let tokens = tokenize(src).map_err(|e| vec![Diagnostic::error("PF-L001", e.message)])?;
    let mut p = Parser::new(src, tokens);
    let mut items = Vec::new();
    while !p.at(&Tok::Eof) {
        match p.item() {
            Ok(i) => items.push(i),
            Err(e) => {
                let d = p.to_diag(e);
                return Err(vec![d]);
            }
        }
        p.current = None;
    }
    let errors: Vec<_> = p.diags.into_iter().filter(|d| d.is_fatal()).collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(items)
}

fn allowed_keywords(c: Ctx) -> Vec<&'static str> {
    crate::lexer::RESERVED_WORDS
        .iter()
        .filter(|w| matches!(Kw::from_ident(w), Some(k) if allowed(c, k) && k.as_str() == **w))
        .copied()
        .collect()
}

fn standalone(src: &str, f: impl FnOnce(&mut Parser) -> PResult<Expr>) -> Result<Expr, Diagnostic> {
    let trimmed = src.trim().trim_end_matches(';').trim_end();
    let tokens = tokenize(trimmed).map_err(|e| Diagnostic::error("PF-L001", e.message))?;
    let mut p = Parser::new(trimmed, tokens);
    let e = f(&mut p).map_err(|e| p.to_diag(e))?;
    if !p.at(&Tok::Eof) {
        let t = p.peek().clone();
        return Err(Diagnostic::error(
            "PF-L002",
            format!("unexpected {} after end of expression", t.tok.describe()),
        )
        .at(t.span));
    }
    if let Some(d) = p.diags.into_iter().next() {
        return Err(d);
    }
    Ok(e)
}

/// Replicates Composer's line-based `ParserUtils.detectVersion`.
pub fn detect_version(src: &str) -> Option<String> {
    for raw in src.lines() {
        let line = raw.strip_suffix(char::from(13)).unwrap_or(raw);
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("plan") {
            if rest.trim_start().starts_with("::") && line.ends_with(';') {
                return None;
            }
        }
        let Some(body) = line.strip_suffix(';') else {
            continue;
        };
        let body = body.trim_end();
        let Some(body) = body.strip_suffix('\x27') else {
            continue;
        };
        let Some(rest) = body.trim_start().strip_prefix("engine_version") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix("::") else {
            continue;
        };
        if let Some(v) = rest.trim_start().strip_prefix('\x27') {
            return Some(v.to_string());
        }
    }
    None
}

#[derive(Debug)]
struct PErr {
    msg: String,
    span: Span,
    code: &'static str,
}

type PResult<T> = Result<T, PErr>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ctx {
    Plan,
    Decision,
    Action,
    Enquiry,
    Task,
    Data,
    Function,
    Component,
    Candidate,
    Source,
    /// generic attributes only (warning conditions, argument/parameter attributes)
    Generic,
}

const GENERIC: &[Kw] = &[Kw::Caption, Kw::Description, Kw::Metaprops];
const TASK_ATTRS: &[Kw] = &[
    Kw::Precondition,
    Kw::WaitCondition,
    Kw::Postcondition,
    Kw::Goal,
    Kw::Trigger,
    Kw::Context,
    Kw::Parameters,
];

fn allowed(ctx: Ctx, kw: Kw) -> bool {
    let generic = GENERIC.contains(&kw);
    let task = TASK_ATTRS.contains(&kw);
    match ctx {
        Ctx::Generic => generic,
        Ctx::Plan => generic || task || matches!(kw, Kw::Component | Kw::Abort | Kw::Terminate),
        Ctx::Decision => {
            generic
                || task
                || matches!(
                    kw,
                    Kw::Candidate
                        | Kw::Source
                        | Kw::ChoiceMode
                        | Kw::PreselectCandidates
                        | Kw::SupportMode
                )
        }
        Ctx::Action => generic || task || kw == Kw::Procedure,
        Ctx::Enquiry => generic || task || kw == Kw::Source,
        Ctx::Task => generic || task,
        Ctx::Data => {
            generic
                || matches!(
                    kw,
                    Kw::Range
                        | Kw::DefaultValue
                        | Kw::TrueValue
                        | Kw::FalseValue
                        | Kw::MandatoryValidation
                        | Kw::Derivation
                        | Kw::Dynamic
                        | Kw::WarningCondition
                        | Kw::Unit
                )
        }
        Ctx::Function => {
            generic
                || matches!(
                    kw,
                    Kw::Type | Kw::Auto | Kw::FunctionTrigger | Kw::FunctionBody
                )
        }
        Ctx::Component => matches!(
            kw,
            Kw::TaskDefinition
                | Kw::Caption
                | Kw::Description
                | Kw::Autonomous
                | Kw::Optional
                | Kw::Terminal
                | Kw::ParamValue
                | Kw::ScheduleConstraint
                | Kw::Ltwh
                | Kw::NumberOfCycles
                | Kw::CycleUntil
                | Kw::CycleRepeat
                | Kw::Metaprops
        ),
        Ctx::Candidate => {
            generic
                || matches!(
                    kw,
                    Kw::Argument | Kw::Recommendation | Kw::Priority | Kw::Group
                )
        }
        Ctx::Source => {
            generic
                || matches!(
                    kw,
                    Kw::DefaultValue
                        | Kw::DataDefinition
                        | Kw::Mandatory
                        | Kw::Dynamic483
                        | Kw::Derivation483
                )
        }
    }
}

struct Parser<'a> {
    src: &'a str,
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
    current: Option<String>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, toks: Vec<Token>) -> Self {
        Parser {
            src,
            toks,
            pos: 0,
            diags: Vec::new(),
            current: None,
        }
    }

    // ── token helpers ────────────────────────────────────────────────────────

    fn peek(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn peek_at(&self, n: usize) -> &Token {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)]
    }

    fn at(&self, t: &Tok) -> bool {
        &self.peek().tok == t
    }

    fn at_kw(&self, k: Kw) -> bool {
        self.peek().tok == Tok::Kw(k)
    }

    fn advance(&mut self) -> Token {
        let t = self.peek().clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn prev_span(&self) -> Span {
        if self.pos == 0 {
            self.peek().span
        } else {
            self.toks[self.pos - 1].span
        }
    }

    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(PErr {
            code: "PF-L002",
            msg: msg.into(),
            span: self.peek().span,
        })
    }

    fn expected<T>(&self, what: &str) -> PResult<T> {
        let found = self.peek().tok.describe();
        self.err(format!("expected {}, found {}", what, found))
    }

    fn expect(&mut self, t: Tok) -> PResult<Token> {
        if self.at(&t) {
            Ok(self.advance())
        } else {
            let what = format!("'{}'", t.punct());
            self.expected(&what)
        }
    }

    fn expect_kw(&mut self, k: Kw) -> PResult<Token> {
        if self.at_kw(k) {
            Ok(self.advance())
        } else {
            self.expected(&format!("'{}'", k.as_str()))
        }
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn to_diag(&self, e: PErr) -> Diagnostic {
        let mut d = Diagnostic::error(e.code, e.msg).at(e.span);
        if let Some(c) = &self.current {
            d = d.in_component(c.clone());
        }
        d
    }

    fn compat(&mut self, code: &str, span: Span, msg: impl Into<String>) {
        let mut d = Diagnostic::error(code, msg).at(span);
        if let Some(c) = &self.current {
            d = d.in_component(c.clone());
        }
        self.diags.push(d);
    }

    /// An atom (bare or quoted). Strings are accepted with a compatibility error.
    fn atom(&mut self, what: &str) -> PResult<String> {
        let t = self.peek().clone();
        match t.tok {
            Tok::Atom { value, .. } => {
                self.advance();
                Ok(value)
            }
            Tok::Str(value) => {
                self.advance();
                self.compat(
                    "PF-K002",
                    t.span,
                    format!(
                        "{} must be a single-quoted atom ('...'), not a double-quoted string",
                        what
                    ),
                );
                Ok(value)
            }
            Tok::Kw(k) => self.err(format!(
                "expected {}, found reserved word '{}' (quote it as '{}')",
                what,
                k.as_str(),
                k.as_str()
            )),
            _ => self.expected(what),
        }
    }

    /// A name, possibly a complex atom `a:b`.
    fn data_name(&mut self) -> PResult<String> {
        let a = self.atom("a name")?;
        if self.at(&Tok::Colon) {
            self.advance();
            let b = self.atom("a name after ':'")?;
            Ok(format!("{}:{}", a, b))
        } else {
            Ok(a)
        }
    }

    // ── document ─────────────────────────────────────────────────────────────

    fn document(&mut self) -> Document {
        let mut doc = Document::default();
        if let Err(e) = self.header(&mut doc) {
            let d = self.to_diag(e);
            self.diags.push(d);
            self.recover();
        }

        while !self.at(&Tok::Eof) {
            let start = self.pos;
            match self.item() {
                Ok(item) => doc.items.push(item),
                Err(e) => {
                    let d = self.to_diag(e);
                    self.diags.push(d);
                    self.recover();
                    if self.pos == start {
                        self.advance();
                    }
                }
            }
            self.current = None;
        }

        self.check_version_detection(&doc);

        match doc.items.first() {
            None => self.diags.push(Diagnostic::error(
                "PF-K004",
                "guideline has no components; the first component must be the root plan",
            )),
            Some(first) if first.kind != ItemKind::Plan => self.diags.push(
                Diagnostic::error(
                    "PF-K004",
                    format!(
                        "the first component must be the root plan, found {} '{}'",
                        first.kind.keyword(),
                        first.name
                    ),
                )
                .at(first.span),
            ),
            _ => {}
        }
        doc
    }

    /// Composer picks the parser from a line that must read exactly
    /// `engine_version :: '<v>';` (before the root `plan ::` line); otherwise
    /// it falls back to the legacy 1.6 parser.
    fn check_version_detection(&mut self, doc: &Document) {
        let detected = detect_version(self.src);
        match (&doc.engine_version, detected) {
            (Some(v), _) if !matches!(v.as_str(), "legacy" | "1.7" | "1.8" | "1.9") => {
                self.diags.push(Diagnostic::error(
                    "PF-L006",
                    format!("No Such verision enum: {} (engine_version must be 1.7, 1.8 or 1.9; Composer crashes on other values)", v),
                ));
            }
            (Some(_), None) => self.diags.push(Diagnostic::error(
                "PF-K005",
                "the engine_version declaration is not on a line of its own (exactly `engine_version :: '1.9' ;` with nothing after the ';'), so Composer will read this file with the legacy 1.6 parser and fail; run `format` to fix",
            )),
            (None, _) if doc.metadata.is_some() || doc.metaprops.is_some() => {
                self.diags.push(Diagnostic::error(
                    "PF-K005",
                    "file has a metadata/metaprops header but no engine_version line, so Composer will use the legacy 1.6 parser and fail; run `format` to add `engine_version :: '1.9' ;`",
                ))
            }
            (None, _) => self.diags.push(Diagnostic::warning(
                "PF-K006",
                "legacy (pre-1.7) guideline: no engine_version line. Composer converts it to 1.9 on import; run `format` to upgrade it",
            )),
            _ => {}
        }
    }

    fn header(&mut self, doc: &mut Document) -> PResult<()> {
        if self.at_kw(Kw::EngineVersion) {
            self.advance();
            self.expect(Tok::DoubleColon)?;
            doc.engine_version = Some(self.atom("an engine version")?);
            self.expect(Tok::Semi)?;
        }
        if self.at_kw(Kw::Directives) {
            self.advance();
            self.expect(Tok::DoubleColon)?;
            let mut dirs = Vec::new();
            while !self.at_kw(Kw::End) && !self.at(&Tok::Eof) {
                dirs.push(self.atom("a directive")?);
                self.expect(Tok::Semi)?;
            }
            self.expect_kw(Kw::End)?;
            self.expect_kw(Kw::Directives)?;
            self.expect(Tok::FullStop)?;
            doc.directives = Some(dirs);
        }
        if self.at_kw(Kw::Metadata) {
            self.advance();
            self.expect(Tok::DoubleColon)?;
            let name = self.atom("a metadata name")?;
            self.expect(Tok::Semi)?;
            let mut fields = Vec::new();
            loop {
                let field = match self.peek().tok {
                    Tok::Kw(Kw::Title) => MetaField::Title,
                    Tok::Kw(Kw::Desc) => MetaField::Desc,
                    Tok::Kw(Kw::Author) => MetaField::Author,
                    Tok::Kw(Kw::Version) => MetaField::Version,
                    Tok::Kw(Kw::Cdate) => MetaField::Cdate,
                    Tok::Kw(Kw::Mdate) => MetaField::Mdate,
                    _ => break,
                };
                self.advance();
                self.expect(Tok::DoubleColon)?;
                let v = self.atom("a metadata value")?;
                self.expect(Tok::Semi)?;
                fields.push((field, v));
            }
            self.expect_kw(Kw::End)?;
            self.expect_kw(Kw::Metadata)?;
            self.expect(Tok::FullStop)?;
            doc.metadata = Some(Metadata { name, fields });
        }
        if self.at_kw(Kw::Metaprops) {
            self.advance();
            self.expect(Tok::DoubleColon)?;
            doc.metaprops = Some(self.atom("a metaprops value")?);
            self.expect(Tok::Semi)?;
        } else {
            let span = self.peek().span;
            self.compat(
                "PF-L009",
                span,
                "missing guideline-level `metaprops :: '...';` before the root plan (required by Composer)",
            );
        }
        Ok(())
    }

    /// Skip to just after the next `end <block> .` or to the next block start.
    fn recover(&mut self) {
        loop {
            match &self.peek().tok {
                Tok::Eof => return,
                Tok::Kw(Kw::End) => {
                    let is_block_end = matches!(
                        self.peek_at(1).tok,
                        Tok::Kw(
                            Kw::Plan
                                | Kw::Decision
                                | Kw::Action
                                | Kw::Enquiry
                                | Kw::Task
                                | Kw::Data
                                | Kw::Function
                        )
                    ) && self.peek_at(2).tok == Tok::FullStop;
                    if is_block_end {
                        self.advance();
                        self.advance();
                        self.advance();
                        return;
                    }
                    self.advance();
                }
                Tok::Kw(
                    Kw::Plan
                    | Kw::Decision
                    | Kw::Action
                    | Kw::Enquiry
                    | Kw::Task
                    | Kw::Data
                    | Kw::Function,
                ) if self.peek_at(1).tok == Tok::DoubleColon
                    && self.pos > 0
                    && self.toks[self.pos - 1].tok != Tok::Kw(Kw::End) =>
                {
                    return
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn item(&mut self) -> PResult<Item> {
        let start = self.peek().span;
        let (kind, ctx, kw) = match self.peek().tok {
            Tok::Kw(Kw::Plan) => (ItemKind::Plan, Ctx::Plan, Kw::Plan),
            Tok::Kw(Kw::Decision) => (ItemKind::Decision, Ctx::Decision, Kw::Decision),
            Tok::Kw(Kw::Action) => (ItemKind::Action, Ctx::Action, Kw::Action),
            Tok::Kw(Kw::Enquiry) => (ItemKind::Enquiry, Ctx::Enquiry, Kw::Enquiry),
            Tok::Kw(Kw::Task) => (ItemKind::Task, Ctx::Task, Kw::Task),
            Tok::Kw(Kw::Data) => (ItemKind::Data, Ctx::Data, Kw::Data),
            Tok::Kw(Kw::Function) => (ItemKind::Function, Ctx::Function, Kw::Function),
            _ => {
                return self.expected(
                    "a component (plan, decision, action, enquiry, task, data or function)",
                )
            }
        };
        self.advance();
        self.expect(Tok::DoubleColon)?;
        let name = if matches!(kind, ItemKind::Data | ItemKind::Function) {
            self.data_name()?
        } else {
            self.atom(&format!("a {} name", kind.keyword()))?
        };
        self.current = Some(name.clone());
        self.expect(Tok::Semi)?;

        let mut attrs = Vec::new();
        if kind == ItemKind::Data {
            // `type :: t;` must come first in a data item.
            if self.at_kw(Kw::Type) {
                attrs.push(self.attr(Ctx::Data)?);
            } else {
                let span = self.peek().span;
                self.compat(
                    "PF-K003",
                    span,
                    format!("data item '{}' must start with `type :: <type>;`", name),
                );
            }
        }
        self.attr_list(ctx, &mut attrs)?;
        if kind == ItemKind::Data && !attrs.iter().any(|a| matches!(a, Attr::Type(_))) {
            self.diags.push(
                Diagnostic::error("PF-K003", format!("data item '{}' has no type", name))
                    .at(start)
                    .in_component(name.clone()),
            );
        }
        self.expect_kw(Kw::End).map_err(|e| PErr {
            code: "PF-L002",
            msg: format!(
                "{} (unexpected attribute or missing `end {} .`)",
                e.msg,
                kw.as_str()
            ),
            span: e.span,
        })?;
        if !self.at_kw(kw) {
            return self.expected(&format!("'{}' after 'end'", kw.as_str()));
        }
        self.advance();
        self.expect(Tok::FullStop)?;
        let span = start.to(self.prev_span());
        let item = Item {
            kind,
            name,
            attrs,
            span,
        };
        self.check_order(&item);
        Ok(item)
    }

    /// Composer's grammar fixes the relative order of some attribute groups.
    fn check_order(&mut self, item: &Item) {
        let generic_first = item.kind.is_task() || item.kind == ItemKind::Data;
        if generic_first {
            let mut seen_specific = false;
            for a in &item.attrs {
                if a.is_generic() {
                    if seen_specific {
                        self.diags.push(
                            Diagnostic::error(
                                "PF-K001",
                                format!(
                                    "'{}' must come before the other attributes of {} '{}' (run `format` to fix)",
                                    a.keyword(),
                                    item.kind.keyword(),
                                    item.name
                                ),
                            )
                            .at(item.span)
                            .in_component(item.name.clone()),
                        );
                        break;
                    }
                } else if !matches!(a, Attr::Type(_)) {
                    seen_specific = true;
                }
            }
        }
        if item.kind == ItemKind::Function {
            let rank = |a: &Attr| match a {
                Attr::Caption(_) | Attr::Description(_) | Attr::Metaprops(_) => 0,
                Attr::Type(_) => 1,
                Attr::Auto(_) => 2,
                Attr::FunctionTrigger(_) => 3,
                Attr::FunctionBody(_) => 4,
                _ => 5,
            };
            let ranks: Vec<_> = item.attrs.iter().map(rank).collect();
            if ranks.windows(2).any(|w| w[0] > w[1]) {
                self.diags.push(
                    Diagnostic::error(
                        "PF-K001",
                        format!(
                            "function '{}' attributes must be ordered: caption/description, type, auto, function_trigger, function_body (run `format` to fix)",
                            item.name
                        ),
                    )
                    .at(item.span)
                    .in_component(item.name.clone()),
                );
            }
            for (kw, ok) in [
                (
                    "type",
                    item.attrs.iter().any(|a| matches!(a, Attr::Type(_))),
                ),
                (
                    "function_body",
                    item.attrs
                        .iter()
                        .any(|a| matches!(a, Attr::FunctionBody(_))),
                ),
            ] {
                if !ok {
                    self.diags.push(
                        Diagnostic::error(
                            "PF-K003",
                            format!("function '{}' requires `{}`", item.name, kw),
                        )
                        .at(item.span)
                        .in_component(item.name.clone()),
                    );
                }
            }
        }
    }

    fn attr_list(&mut self, ctx: Ctx, out: &mut Vec<Attr>) -> PResult<()> {
        while let Tok::Kw(k) = self.peek().tok {
            if !allowed(ctx, k) {
                break;
            }
            out.push(self.attr(ctx)?);
        }
        Ok(())
    }

    fn kw_attr_head(&mut self) -> PResult<Token> {
        let t = self.advance();
        self.expect(Tok::DoubleColon)?;
        Ok(t)
    }

    fn end_attr(&mut self) -> PResult<()> {
        self.expect(Tok::Semi).map(|_| ())
    }

    fn yes_no(&mut self) -> PResult<bool> {
        let t = self.peek().clone();
        let v = match &t.tok {
            Tok::Atom { value, .. } => value.to_ascii_lowercase(),
            _ => return self.expected("yes or no"),
        };
        self.advance();
        match v.as_str() {
            "yes" | "true" => Ok(true),
            "no" | "false" => Ok(false),
            _ => Err(PErr {
                code: "PF-L004",
                msg: format!("expected \"yes\" or \"no\", found '{}'", v),
                span: t.span,
            }),
        }
    }

    fn attr(&mut self, ctx: Ctx) -> PResult<Attr> {
        let Tok::Kw(kw) = self.peek().tok else {
            return self.expected("an attribute");
        };
        let head = self.kw_attr_head()?;
        let attr = match kw {
            Kw::Caption => Attr::Caption(self.atom("a caption")?),
            Kw::Description => Attr::Description(self.atom("a description")?),
            Kw::Metaprops => Attr::Metaprops(self.atom("a metaprops value")?),
            Kw::Precondition => Attr::Precondition(self.expr()?),
            Kw::WaitCondition => Attr::WaitCondition(self.expr()?),
            Kw::Postcondition => Attr::Postcondition(self.assertion_expr()?),
            Kw::Goal => Attr::Goal(self.expr()?),
            Kw::Trigger => Attr::Trigger(self.atom("a trigger")?),
            Kw::Context => Attr::Context(self.atom("a context")?),
            Kw::Parameters => Attr::Parameters(self.parameters()?),
            Kw::Abort => Attr::Abort(self.expr()?),
            Kw::Terminate => Attr::Terminate(self.expr()?),
            Kw::Procedure => Attr::Procedure(self.atom("a procedure")?),
            Kw::ChoiceMode => {
                let v = match self.peek().tok {
                    Tok::Kw(Kw::Single) => "single",
                    Tok::Kw(Kw::Multiple) => "multiple",
                    _ => return self.expected("'single' or 'multiple'"),
                };
                self.advance();
                Attr::ChoiceMode(v.into())
            }
            Kw::SupportMode => {
                let v = match self.peek().tok {
                    Tok::Kw(Kw::Symbolic) => "symbolic",
                    Tok::Kw(Kw::Numeric) => "numeric",
                    _ => return self.expected("'symbolic' or 'numeric'"),
                };
                self.advance();
                Attr::SupportMode(v.into())
            }
            Kw::PreselectCandidates => Attr::PreselectCandidates(self.yes_no()?),
            Kw::Type => {
                let t = self.peek().clone();
                let ty = self.atom("a data type")?;
                let valid = DATA_TYPES.contains(&ty.as_str());
                if !valid {
                    return Err(PErr {
                        code: "PF-L003",
                        msg: format!(
                            "'{}' is not a valid data type (expected one of: {})",
                            ty,
                            DATA_TYPES.join(", ")
                        ),
                        span: t.span,
                    });
                }
                Attr::Type(ty)
            }
            Kw::Range => {
                // Composer records each element up to its following comma, so
                // whitespace before a comma is part of the element text.
                let mut v = Vec::new();
                loop {
                    let mut e = self.expr()?;
                    if self.at(&Tok::Comma) {
                        let comma = self.peek().span.start;
                        e.text = self.src[e.span.start..comma].trim_start().to_string();
                        v.push(e);
                        self.advance();
                    } else {
                        v.push(e);
                        break;
                    }
                }
                Attr::Range(v)
            }
            Kw::DefaultValue => Attr::DefaultValue(self.expr()?),
            Kw::TrueValue => Attr::TrueValue(self.textual_constant()?),
            Kw::FalseValue => Attr::FalseValue(self.textual_constant()?),
            Kw::Unit => Attr::Unit(self.textual_constant()?),
            Kw::MandatoryValidation => Attr::MandatoryValidation(self.expr()?),
            Kw::Derivation => Attr::Derivation(self.expr()?),
            Kw::Dynamic => Attr::Dynamic(self.yes_no()?),
            Kw::Auto => Attr::Auto(self.yes_no()?),
            Kw::FunctionTrigger => Attr::FunctionTrigger(self.expr()?),
            Kw::FunctionBody => Attr::FunctionBody(self.expr()?),
            Kw::TaskDefinition => Attr::TaskDefinition(self.atom("a task definition name")?),
            Kw::Autonomous => Attr::Autonomous(self.yes_no()?),
            Kw::Optional => Attr::Optional(self.yes_no()?),
            Kw::Terminal => Attr::Terminal(self.yes_no()?),
            Kw::ParamValue => Attr::ParamValue(self.assignment_expr()?),
            Kw::ScheduleConstraint => {
                self.expect_kw(Kw::Completed)?;
                self.expect(Tok::LParen)?;
                let n = self.atom("a component name")?;
                self.expect(Tok::RParen)?;
                Attr::ScheduleConstraint(n)
            }
            Kw::Ltwh => {
                let mut v = [0i64; 4];
                for (i, slot) in v.iter_mut().enumerate() {
                    if i > 0 {
                        self.expect(Tok::Comma)?;
                    }
                    match self.peek().tok {
                        Tok::Int(n) => {
                            self.advance();
                            *slot = n;
                        }
                        _ => return self.expected("an integer"),
                    }
                }
                Attr::Ltwh(v)
            }
            Kw::NumberOfCycles => Attr::NumberOfCycles(self.expr()?),
            Kw::CycleUntil => Attr::CycleUntil(self.expr()?),
            Kw::CycleRepeat => {
                let e = self.expr()?;
                let unit = match self.peek().tok {
                    Tok::Kw(Kw::Seconds) => TimeUnit::Seconds,
                    Tok::Kw(Kw::Minutes) => TimeUnit::Minutes,
                    Tok::Kw(Kw::Hours) => TimeUnit::Hours,
                    Tok::Kw(Kw::Days) => TimeUnit::Days,
                    Tok::Kw(Kw::Weeks) => TimeUnit::Weeks,
                    _ => return self.expected("a time unit (seconds/minutes/hours/days/weeks)"),
                };
                self.advance();
                Attr::CycleRepeat(e, unit)
            }
            Kw::Recommendation => Attr::Recommendation(self.expr()?),
            Kw::Priority => match self.peek().tok {
                Tok::Int(n) => {
                    self.advance();
                    Attr::Priority(n)
                }
                _ => return self.expected("an integer priority"),
            },
            Kw::Group => Attr::Group(self.atom("a group name")?),
            Kw::DataDefinition => Attr::DataDefinition(self.data_name()?),
            Kw::Mandatory => Attr::Mandatory(self.yes_no()?),
            Kw::Dynamic483 => Attr::Dynamic483(self.yes_no()?),
            Kw::Derivation483 => Attr::Derivation483(self.expr()?),

            // nested blocks: header ends with ';' then greedy attribute list
            Kw::Component | Kw::Candidate | Kw::Source => {
                let name = if kw == Kw::Source {
                    self.data_name()?
                } else {
                    self.atom(&format!("a {} name", kw.as_str()))?
                };
                self.end_attr()?;
                let sub = match kw {
                    Kw::Component => Ctx::Component,
                    Kw::Candidate => Ctx::Candidate,
                    _ => Ctx::Source,
                };
                let mut attrs = Vec::new();
                self.attr_list(sub, &mut attrs)?;
                let block = Block {
                    name,
                    attrs,
                    span: head.span.to(self.prev_span()),
                };
                if kw == Kw::Candidate {
                    self.check_candidate_order(&block);
                }
                return Ok(match kw {
                    Kw::Component => Attr::Component(block),
                    Kw::Candidate => Attr::Candidate(block),
                    _ => Attr::Source(block),
                });
            }
            Kw::WarningCondition => {
                let flag = self.constant()?;
                self.expect(Tok::Comma)?;
                let condition = self.expr()?;
                self.end_attr()?;
                let mut attrs = Vec::new();
                self.attr_list(Ctx::Generic, &mut attrs)?;
                return Ok(Attr::WarningCondition(Warning {
                    flag,
                    condition,
                    attrs,
                    span: head.span.to(self.prev_span()),
                }));
            }
            Kw::Argument => {
                let support = match self.peek().tok {
                    Tok::Kw(Kw::For) => Some(Support::For),
                    Tok::Kw(Kw::Against) => Some(Support::Against),
                    Tok::Kw(Kw::Confirming) => Some(Support::Confirming),
                    Tok::Kw(Kw::Excluding) => Some(Support::Excluding),
                    _ => None,
                };
                let support = match support {
                    Some(s) => {
                        self.advance();
                        s
                    }
                    None => Support::Weight(self.expr()?),
                };
                self.expect(Tok::Comma)?;
                let condition = self.expr()?;
                let mut name = None;
                let mut attrs = Vec::new();
                let mut has_attributes_section = false;
                if self.at_kw(Kw::Attributes) {
                    has_attributes_section = true;
                    self.advance();
                    if self.at_kw(Kw::ArgumentName) {
                        self.advance();
                        self.expect(Tok::DoubleColon)?;
                        name = Some(self.atom("an argument name")?);
                        self.expect(Tok::Semi)?;
                    }
                    self.attr_list(Ctx::Generic, &mut attrs)?;
                    self.expect_kw(Kw::End)?;
                    self.expect_kw(Kw::Attributes)?;
                }
                self.end_attr()?;
                return Ok(Attr::Argument(Argument {
                    support,
                    condition,
                    name,
                    attrs,
                    has_attributes_section,
                    span: head.span.to(self.prev_span()),
                }));
            }
            other => {
                return Err(PErr {
                    code: "PF-L002",
                    msg: format!(
                        "'{}' is not allowed here ({:?} context)",
                        other.as_str(),
                        ctx
                    ),
                    span: head.span,
                })
            }
        };
        self.end_attr()?;
        Ok(attr)
    }

    fn check_candidate_order(&mut self, b: &Block) {
        let mut seen_specific = false;
        for a in &b.attrs {
            if a.is_generic() && seen_specific {
                self.compat(
                    "PF-K001",
                    b.span,
                    format!(
                        "caption/description/metaprops of candidate '{}' must come before its arguments/recommendation/priority (run `format` to fix)",
                        b.name
                    ),
                );
                return;
            }
            if !a.is_generic() {
                seen_specific = true;
            }
        }
    }

    fn parameters(&mut self) -> PResult<Vec<Parameter>> {
        let mut out = Vec::new();
        loop {
            let name = self.atom("a parameter name")?;
            let mut p = Parameter {
                name,
                data_type: None,
                attrs: Vec::new(),
                has_attributes_section: false,
            };
            if self.at_kw(Kw::Attributes) {
                self.advance();
                p.has_attributes_section = true;
                if self.at_kw(Kw::Type) {
                    self.advance();
                    self.expect(Tok::DoubleColon)?;
                    let t = self.peek().span;
                    let ty = self.atom("a data type")?;
                    if !DATA_TYPES.contains(&ty.as_str()) {
                        return Err(PErr {
                            code: "PF-L003",
                            msg: format!("'{}' is not a valid data type", ty),
                            span: t,
                        });
                    }
                    p.data_type = Some(ty);
                    self.expect(Tok::Semi)?;
                }
                self.attr_list(Ctx::Generic, &mut p.attrs)?;
                self.expect_kw(Kw::End)?;
                self.expect_kw(Kw::Attributes)?;
            }
            out.push(p);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        Ok(out)
    }

    fn textual_constant(&mut self) -> PResult<Constant> {
        let t = self.peek().clone();
        match t.tok {
            Tok::Str(s) => {
                self.advance();
                Ok(Constant::String(s))
            }
            Tok::Atom { value, .. } => {
                self.advance();
                Ok(Constant::Atom(value))
            }
            _ => self.expected("a string or atom"),
        }
    }

    fn constant(&mut self) -> PResult<Constant> {
        let neg = self.eat(&Tok::Minus);
        let t = self.peek().clone();
        let sign = if neg { "-" } else { "" };
        match t.tok {
            Tok::Int(n) => {
                self.advance();
                Ok(Constant::Number(format!("{}{}", sign, n)))
            }
            Tok::Double(d) => {
                self.advance();
                Ok(Constant::Number(format!("{}{}", sign, d)))
            }
            _ if neg => self.expected("a number"),
            _ => self.textual_constant(),
        }
    }

    // ── expressions ──────────────────────────────────────────────────────────

    fn wrap(&self, start_tok: usize, node: Node) -> Expr {
        let start = self.toks[start_tok].span;
        let end = self.prev_span();
        let text = self.src[start.start..end.end.max(start.start)]
            .trim()
            .to_string();
        Expr {
            text,
            node,
            span: start.to(end),
        }
    }

    fn expr(&mut self) -> PResult<Expr> {
        let start = self.pos;
        let node = self.expr_prec(1)?;
        Ok(self.wrap(start, node))
    }

    fn binop(&self) -> Option<BinOp> {
        Some(match &self.peek().tok {
            Tok::Kw(Kw::Or) => BinOp::Or,
            Tok::Kw(Kw::And) => BinOp::And,
            Tok::Assign => BinOp::Assign,
            Tok::Lt => BinOp::Lt,
            Tok::LtEq => BinOp::LtEq,
            Tok::Gt => BinOp::Gt,
            Tok::GtEq => BinOp::GtEq,
            Tok::NotEquals => BinOp::NotEquals,
            Tok::Equals => BinOp::Equals,
            Tok::Kw(Kw::Includes) => BinOp::Includes,
            Tok::Kw(Kw::IncludesAll) => BinOp::IncludesAll,
            Tok::Kw(Kw::IncludesAny) => BinOp::IncludesAny,
            Tok::Kw(Kw::Oneof) => BinOp::Oneof,
            Tok::Kw(Kw::Matches) => BinOp::Matches,
            Tok::Kw(Kw::MatchesAll) => BinOp::MatchesAll,
            Tok::Kw(Kw::MatchesAny) => BinOp::MatchesAny,
            Tok::Kw(Kw::MatchesRx) => BinOp::MatchesRx,
            Tok::Kw(Kw::MatchesRxAll) => BinOp::MatchesRxAll,
            Tok::Kw(Kw::MatchesRxAny) => BinOp::MatchesRxAny,
            Tok::Kw(Kw::MatchesIRx) => BinOp::MatchesIRx,
            Tok::Kw(Kw::MatchesIRxAll) => BinOp::MatchesIRxAll,
            Tok::Kw(Kw::MatchesIRxAny) => BinOp::MatchesIRxAny,
            Tok::Kw(Kw::TermIsa) => BinOp::TermIsa,
            Tok::Kw(Kw::TermIncludes) => BinOp::TermIncludes,
            Tok::Kw(Kw::TermIncludesAll) => BinOp::TermIncludesAll,
            Tok::Kw(Kw::TermIncludesAny) => BinOp::TermIncludesAny,
            Tok::Kw(Kw::TermIntersectGeneric) => BinOp::TermIntersectGeneric,
            Tok::Kw(Kw::TermAssign) => BinOp::TermAssign,
            Tok::Kw(Kw::TermOccurences) => BinOp::TermOccurences,
            Tok::Append => BinOp::Append,
            Tok::Plus => BinOp::Plus,
            Tok::Minus => BinOp::Minus,
            Tok::Times => BinOp::Times,
            Tok::Divide => BinOp::Divide,
            Tok::AttrOper => BinOp::AttrOper,
            _ => return None,
        })
    }

    fn expr_prec(&mut self, min: u8) -> PResult<Node> {
        let mut lhs = self.unary()?;
        while let Some(op) = self.binop() {
            let lvl = op.level();
            if lvl < min {
                break;
            }
            self.advance();
            let rhs = self.expr_prec(lvl + 1)?;
            lhs = Node::Bin(op, Box::new(lhs), Box::new(rhs));
            if !op.is_left_assoc() {
                if let Some(next) = self.binop() {
                    if next.level() == lvl {
                        return self.err(format!(
                            "operators '{}' and '{}' cannot be chained; add parentheses",
                            op.symbol(),
                            next.symbol()
                        ));
                    }
                }
            }
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Node> {
        if self.at(&Tok::Minus) {
            self.advance();
            // unary minus binds tighter than * / but looser than `->`
            let operand = self.expr_prec(13)?;
            return Ok(Node::Neg(Box::new(operand)));
        }
        self.primary()
    }

    fn primary(&mut self) -> PResult<Node> {
        let t = self.peek().clone();
        match t.tok {
            Tok::Atom { value, .. } => {
                self.advance();
                if self.at(&Tok::LParen) {
                    self.advance();
                    let args = self.expr_list(&Tok::RParen)?;
                    self.expect(Tok::RParen)?;
                    Ok(Node::Call(value, args))
                } else if self.at(&Tok::Colon) {
                    self.advance();
                    let b = self.atom("a name after ':'")?;
                    Ok(Node::Complex(value, b))
                } else {
                    Ok(Node::Atom(value))
                }
            }
            Tok::Int(n) => {
                if n > i32::MAX as i64 {
                    return Err(PErr {
                        code: "PF-L005",
                        msg: format!("For input string: \"{}\" (integer literals must fit in 32 bits)", n),
                        span: t.span,
                    });
                }
                self.advance();
                Ok(Node::Int(n))
            }
            Tok::Double(d) => {
                self.advance();
                Ok(Node::Real(d))
            }
            Tok::Str(s) => {
                self.advance();
                Ok(Node::Str(s))
            }
            Tok::Date(d) => {
                self.advance();
                Ok(Node::Date(d))
            }
            Tok::DateTime(d) => {
                self.advance();
                Ok(Node::DateTime(d))
            }
            Tok::Time(d) => {
                self.advance();
                Ok(Node::Time(d))
            }
            Tok::LParen => {
                self.advance();
                let inner = self.expr_prec(1)?;
                self.expect(Tok::RParen)?;
                Ok(Node::Paren(Box::new(inner)))
            }
            Tok::LBracket => {
                self.advance();
                let items = self.expr_list(&Tok::RBracket)?;
                self.expect(Tok::RBracket)?;
                Ok(Node::List(items))
            }
            Tok::Kw(Kw::ResultOf) => {
                self.advance();
                self.expect(Tok::LParen)?;
                let n = self.atom("a decision name")?;
                self.expect(Tok::RParen)?;
                Ok(Node::ResultOf(n))
            }
            Tok::Kw(k) => self.err(format!(
                "expected an expression, found reserved word '{}' (quote names that clash with keywords, e.g. '{}')",
                k.as_str(),
                k.as_str()
            )),
            _ => self.expected("an expression"),
        }
    }

    fn expr_list(&mut self, close: &Tok) -> PResult<Vec<Node>> {
        let mut v = Vec::new();
        if self.at(close) {
            return Ok(v);
        }
        v.push(self.expr_prec(1)?);
        while self.eat(&Tok::Comma) {
            v.push(self.expr_prec(1)?);
        }
        Ok(v)
    }

    // ── assertions (postcondition / param_value) ─────────────────────────────

    fn assertion_expr(&mut self) -> PResult<Expr> {
        let start = self.pos;
        let node = self.assertion()?;
        Ok(self.wrap(start, node))
    }

    fn assignment_expr(&mut self) -> PResult<Expr> {
        let start = self.pos;
        let node = self.assignment(true)?;
        Ok(self.wrap(start, node))
    }

    fn assertion(&mut self) -> PResult<Node> {
        let mut lhs = self.assertion_term()?;
        while self.at_kw(Kw::And) {
            self.advance();
            let rhs = self.assertion_term()?;
            lhs = Node::Bin(BinOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn assertion_term(&mut self) -> PResult<Node> {
        if self.at(&Tok::LParen) {
            self.advance();
            let inner = self.assertion()?;
            self.expect(Tok::RParen)?;
            return Ok(Node::Paren(Box::new(inner)));
        }
        self.assignment(false)
    }

    /// `ATOM (= | := | term_assign) expr`; when `required` is false a bare ATOM is allowed.
    fn assignment(&mut self, required: bool) -> PResult<Node> {
        let name = self.atom("a data item name")?;
        let op = match self.peek().tok {
            Tok::Equals => BinOp::Equals,
            Tok::Assign => BinOp::Assign,
            Tok::Kw(Kw::TermAssign) => BinOp::TermAssign,
            _ if !required => return Ok(Node::Atom(name)),
            _ => return self.expected("'=' or ':='"),
        };
        self.advance();
        let rhs = self.expr_prec(op.level() + 1)?;
        Ok(Node::Bin(op, Box::new(Node::Atom(name)), Box::new(rhs)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"engine_version :: '1.9';
metadata :: m; title :: 'S'; end metadata.
metaprops :: '';
plan :: 'Main' ;
    caption :: 'Main plan';
    component :: 'Ask' ;
        ltwh :: 10, 20, 100, 50 ;
    component :: 'Choose' ;
        schedule_constraint :: completed('Ask') ;
end plan.

enquiry :: 'Ask' ;
    caption :: 'Ask questions';
    source :: age ;
        mandatory :: yes ;
end enquiry.

decision :: 'Choose' ;
    candidate :: a ;
        caption :: 'Option A';
        argument :: for, age > 18 attributes argument_name :: adult; caption :: 'Adult'; end attributes ;
        recommendation :: netsupport(Choose, a) >= 1 ;
    candidate :: b ;
        argument :: against, age >= 18 and age < 65 ;
end decision.

data :: age ;
    type :: integer ;
    caption :: 'Age';
    range :: 0, 120;
    warning_condition :: 1, age > 100 ; caption :: 'Old';
end data.

function :: double_it ;
    type :: integer ;
    function_body :: age * 2 ;
end function.
"#;

    #[test]
    fn parses_sample() {
        let out = parse(SAMPLE);
        assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
        let d = &out.doc;
        assert_eq!(d.items.len(), 5);
        assert_eq!(d.items[0].components().count(), 2);
        let dec = d.find("Choose").unwrap();
        let cands: Vec<_> = dec.candidates().collect();
        assert_eq!(cands.len(), 2);
        assert_eq!(cands[0].caption(), Some("Option A"));
        let arg = cands[0].arguments().next().unwrap();
        assert_eq!(arg.name.as_deref(), Some("adult"));
        assert_eq!(arg.condition.text, "age > 18");
    }

    #[test]
    fn precedence() {
        let e = parse_expression("a + b * c > 3 and not_x or y").unwrap();
        match e.node {
            Node::Bin(BinOp::Or, l, _) => match *l {
                Node::Bin(BinOp::And, l2, _) => match *l2 {
                    Node::Bin(BinOp::Gt, l3, _) => {
                        assert!(matches!(*l3, Node::Bin(BinOp::Plus, _, _)))
                    }
                    other => panic!("{:?}", other),
                },
                other => panic!("{:?}", other),
            },
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn nonassoc_comparison_rejected() {
        assert!(parse_expression("a < b < c").is_err());
    }

    #[test]
    fn assertion_splits_on_and() {
        let e = parse_assertion("x = a + 1 and y := 'z'").unwrap();
        assert!(matches!(e.node, Node::Bin(BinOp::And, _, _)));
    }

    #[test]
    fn recovers_after_error() {
        let out =
            parse("metaprops :: ''; plan :: p; precondition :: ; end plan. task :: t; end task.");
        assert!(out.has_errors());
        assert_eq!(out.doc.items.len(), 1);
        assert_eq!(out.doc.items[0].name, "t");
    }
}
