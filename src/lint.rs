//! Guideline validation mirroring the reference engine's pipeline:
//! parse (PF-L*, PF-K*) -> construction (PF-C*) -> reference resolution (PF-R*)
//! -> post-processing (PF-PP*) -> guideline validation (PF-G*) -> verifier
//! (PF-V*, expression typing), plus our own checks (PF-X*) for engine crash
//! paths, Composer GUI invariants and things the engine never checks.
//!
//! Message texts of engine rules are the engine's own (including its typos),
//! so they can be matched against what Composer shows.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::ast::*;
use crate::builtins::{self, can_promote, Ty};
use crate::diagnostics::{Diagnostic, Severity};
use crate::lexer::{is_bare_atom, Span};

#[derive(Debug, Clone, Default)]
pub struct LintOptions {
    pub include_info: bool,
}

pub fn lint(doc: &Document, parse_diags: &[Diagnostic], opts: &LintOptions) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = parse_diags.to_vec();
    if !parse_diags.iter().any(|d| d.is_fatal()) {
        let env = Env::new(doc);
        let mut l = Linter {
            env: &env,
            out: Vec::new(),
            item_start: 0,
        };
        l.header();
        l.construction();
        let resolved = l.resolution();
        l.post_processing();
        l.guideline();
        if resolved {
            l.verify();
        }
        l.extras();
        out.extend(l.out);
    }
    if !opts.include_info {
        out.retain(|d| d.severity != Severity::Info);
    }
    out
}

// ── environment ─────────────────────────────────────────────────────────────

pub struct Env<'a> {
    pub doc: &'a Document,
    pub root: Option<&'a Item>,
    pub ignore_task_case: bool,
    pub ignore_data_case: bool,
}

fn eq(a: &str, b: &str, ci: bool) -> bool {
    if ci {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Parse `key=value` lines of a metaprops string (enough for the boolean flags).
fn metaprop(props: &str, key: &str) -> Option<String> {
    props.lines().find_map(|l| {
        let (k, v) = l.split_once(['=', ':'])?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

impl<'a> Env<'a> {
    pub fn new(doc: &'a Document) -> Self {
        let props = doc.metaprops.as_deref().unwrap_or("");
        // legacy (pre-1.7) guidelines are always case-insensitive
        let legacy = doc.engine_version.is_none();
        let flag = |k| legacy || metaprop(props, k).is_some_and(|v| v.eq_ignore_ascii_case("true"));
        Env {
            doc,
            root: doc.root(),
            ignore_task_case: flag("ignore.task.case"),
            ignore_data_case: flag("ignore.data.case"),
        }
    }

    fn items(&self) -> impl Iterator<Item = &'a Item> {
        self.doc.items.iter()
    }

    /// Top-level task definitions (the root plan excluded).
    fn tasks(&self) -> impl Iterator<Item = &'a Item> {
        self.doc.items.iter().skip(1).filter(|i| i.kind.is_task())
    }

    fn data(&self) -> impl Iterator<Item = &'a Item> {
        self.items().filter(|i| i.kind == ItemKind::Data)
    }

    fn functions(&self) -> impl Iterator<Item = &'a Item> {
        self.items().filter(|i| i.kind == ItemKind::Function)
    }

    /// Definition lookup used by components (root excluded; case per metaprops).
    pub fn task_def(&self, name: &str) -> Option<&'a Item> {
        let ci = self.ignore_task_case;
        self.tasks().find(|t| eq(&t.name, name, ci))
    }

    /// Case-insensitive task lookup including the root (verifier semantics).
    pub fn task_ci(&self, name: &str) -> Option<&'a Item> {
        self.items()
            .filter(|i| i.kind.is_task())
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }

    pub fn data_item(&self, name: &str) -> Option<&'a Item> {
        let ci = self.ignore_data_case;
        self.data().find(|d| eq(&d.name, name, ci))
    }

    /// Data item (preferred) or function with this name, and its declared type.
    pub fn typed(&self, name: &str) -> Option<(Ty, &'a Item)> {
        let ci = self.ignore_data_case;
        self.data()
            .find(|d| eq(&d.name, name, ci))
            .or_else(|| self.functions().find(|f| eq(&f.name, name, ci)))
            .map(|i| {
                (
                    i.data_type()
                        .and_then(Ty::from_declared)
                        .unwrap_or(Ty::Text),
                    i,
                )
            })
    }

    /// Component (task instance) anywhere in the guideline, case-insensitive.
    pub fn instance_ci(&self, name: &str) -> Option<(&'a Block, &'a Item)> {
        self.items()
            .filter(|i| i.kind == ItemKind::Plan)
            .find_map(|p| {
                p.components()
                    .find(|c| c.name.eq_ignore_ascii_case(name))
                    .map(|c| (c, p))
            })
    }

    fn is_dynamic(d: &Item) -> bool {
        d.attrs.iter().any(|a| matches!(a, Attr::Dynamic(true)))
    }
}

fn is_multiple(dec: &Item) -> bool {
    dec.attrs
        .iter()
        .any(|a| matches!(a, Attr::ChoiceMode(m) if m == "multiple"))
}

fn is_numeric(dec: &Item) -> bool {
    dec.attrs
        .iter()
        .any(|a| matches!(a, Attr::SupportMode(m) if m == "numeric"))
}

// ── canonical form (used in engine messages for sub-expressions) ─────────────

pub fn canon(n: &Node) -> String {
    match n {
        Node::Empty => String::new(),
        Node::Atom(a) => format!("'{}'", a),
        Node::Complex(a, b) => format!("'{}:{}'", a, b),
        Node::Int(i) => i.to_string(),
        Node::Real(r) => r.clone(),
        Node::Str(s) => format!("\"{}\"", s),
        Node::Date(d) | Node::DateTime(d) | Node::Time(d) => d.clone(),
        Node::ResultOf(d) => format!("result_of('{}')", d),
        Node::Call(f, args) => format!(
            "{}({})",
            f,
            args.iter().map(canon).collect::<Vec<_>>().join(",")
        ),
        Node::List(v) => format!("[{}]", v.iter().map(canon).collect::<Vec<_>>().join(",")),
        Node::Neg(x) => format!("-({})", canon(x)),
        Node::Paren(x) => format!("({})", canon(x)),
        Node::Bin(op, a, b) => format!("{} {} {}", canon(a), op.symbol(), canon(b)),
    }
}

fn functor(op: BinOp) -> &'static str {
    match op {
        BinOp::Or => "or",
        BinOp::And => "and",
        BinOp::Append => "#",
        BinOp::NotEquals => "!=",
        other => other.symbol(),
    }
}

// ── expression typing (Verifier.verifyParsedExp) ─────────────────────────────

/// Where an expression is evaluated, for candidate support functions.
#[derive(Clone, Copy)]
enum VCtx<'a> {
    Other,
    Recommendation(&'a Item),
    ArgCondition(&'a Item),
}

struct Typer<'e, 'a> {
    env: &'e Env<'a>,
    comp: String,
    vctx: VCtx<'a>,
    span: Span,
    escalate: bool,
    diags: Vec<Diagnostic>,
    /// names seen while typing: (name, resolution)
    refs: Vec<(String, String)>,
}

impl<'e, 'a> Typer<'e, 'a> {
    fn new(env: &'e Env<'a>, comp: &str, span: Span) -> Self {
        Typer {
            env,
            comp: comp.to_string(),
            vctx: VCtx::Other,
            span,
            escalate: false,
            diags: Vec::new(),
            refs: Vec::new(),
        }
    }

    fn report(&mut self, sev: Severity, code: &str, msg: String) {
        let sev = if self.escalate && sev == Severity::Warning {
            Severity::Error
        } else {
            sev
        };
        self.diags.push(
            Diagnostic::new(sev, code, msg)
                .at(self.span)
                .in_component(self.comp.clone()),
        );
    }

    fn warn(&mut self, code: &str, msg: String) {
        self.report(Severity::Warning, code, msg)
    }

    fn error(&mut self, code: &str, msg: String) {
        self.report(Severity::Error, code, msg)
    }

    fn atom_type(&mut self, name: &str) -> Ty {
        match self.env.typed(name) {
            Some((t, item)) => {
                self.refs
                    .push((name.to_string(), format!("{} ({})", item.kind.keyword(), t)));
                t
            }
            None => {
                self.refs.push((
                    name.to_string(),
                    "text literal (not a data item or function)".into(),
                ));
                Ty::Text
            }
        }
    }

    fn ty(&mut self, n: &Node, top: Option<&str>) -> Ty {
        match n {
            Node::Empty => Ty::Untypeable,
            Node::Atom(a) => self.atom_type(a),
            Node::Complex(a, b) => self.atom_type(&format!("{}:{}", a, b)),
            Node::Str(_) => Ty::Text,
            Node::Int(_) => Ty::Integer,
            Node::Real(_) => Ty::Real,
            Node::Date(_) | Node::DateTime(_) | Node::Time(_) => Ty::DateTime,
            Node::Paren(x) => self.ty(x, top),
            Node::List(v) => self.set_type(v),
            Node::ResultOf(d) => self.result_fn("result_of", d),
            Node::Neg(x) => self.builtin("-", std::slice::from_ref(x.as_ref()), n, top),
            Node::Bin(BinOp::Assign | BinOp::TermAssign, a, b) => self.assign_expr(a, b),
            Node::Bin(op, a, b) => {
                let args = [a.as_ref().clone(), b.as_ref().clone()];
                self.builtin(functor(*op), &args, n, top)
            }
            Node::Call(f, args) => self.call(f, args, n, top),
        }
    }

    fn set_type(&mut self, v: &[Node]) -> Ty {
        if v.is_empty() {
            return Ty::SetAny;
        }
        let mut base = None;
        for e in v {
            let t = self.ty(e, None);
            if t == Ty::Untypeable {
                return Ty::Untypeable;
            }
            base = match base {
                None => Some(t),
                Some(b) if b == t => Some(b),
                Some(Ty::Integer) if t == Ty::Real => Some(Ty::Real),
                Some(Ty::Real) if t == Ty::Integer => Some(Ty::Real),
                Some(_) => {
                    self.warn(
                        "PF-V070",
                        " set contains members of different types ".into(),
                    );
                    return Ty::Untypeable;
                }
            };
        }
        match base.unwrap() {
            Ty::Integer => Ty::SetInteger,
            Ty::Real => Ty::SetReal,
            Ty::Text => Ty::SetText,
            _ => {
                self.warn(
                    "PF-V071",
                    "members of a set must be either text strings, integers, or reals".into(),
                );
                Ty::Untypeable
            }
        }
    }

    fn text_of(n: &Node, top: Option<&str>) -> String {
        top.map(str::to_string).unwrap_or_else(|| canon(n))
    }

    fn builtin(&mut self, f: &str, args: &[Node], whole: &Node, top: Option<&str>) -> Ty {
        let mut tys = Vec::with_capacity(args.len());
        for a in args {
            let t = self.ty(a, None);
            if t == Ty::Untypeable {
                if !builtins::is_builtin(f) {
                    self.unknown_function(f);
                }
                return Ty::Untypeable;
            }
            tys.push(t);
        }
        if !builtins::is_builtin(f) {
            self.unknown_function(f);
            return Ty::Untypeable;
        }
        let date_family = builtins::is_date_family(f);
        for (name, sig, ret) in builtins::SIGNATURES {
            if *name != f || sig.len() != tys.len() {
                continue;
            }
            let ok = tys.iter().zip(sig.iter()).all(|(a, e)| {
                if date_family {
                    matches!(a, Ty::Date | Ty::Time | Ty::DateTime)
                } else {
                    can_promote(*a, *e)
                }
            });
            if ok {
                return *ret;
            }
        }
        self.warn(
            "PF-V034",
            format!("Unable to type expression: {}", Self::text_of(whole, top)),
        );
        Ty::Untypeable
    }

    fn unknown_function(&mut self, f: &str) {
        let mut msg = format!("Unrecognised function: {}", f);
        if self.env.functions().any(|x| x.name.eq_ignore_ascii_case(f)) {
            msg.push_str(
                " (user-defined functions are referenced by bare name, without parentheses)",
            );
        } else if let Some((_, hint)) = builtins::DOCUMENTED_BUT_UNKNOWN
            .iter()
            .find(|(n, _)| *n == f)
        {
            msg.push_str(&format!(" ({})", hint));
        }
        self.warn("PF-V030", msg);
    }

    fn call(&mut self, f: &str, args: &[Node], whole: &Node, top: Option<&str>) -> Ty {
        if f == "result_of" || f == "result_set" || f == "activate_trigger" {
            return match args {
                [Node::Atom(x)] => self.result_fn(f, x),
                _ => {
                    self.error(
                        "PF-V002",
                        format!(
                            "Unexplained error - contact development team! ({} expects exactly one task name; the engine fails here)",
                            f
                        ),
                    );
                    Ty::Untypeable
                }
            };
        }
        let real = builtins::CANDIDATE_REAL_FNS.contains(&f);
        let truth = builtins::CANDIDATE_TRUTH_FNS.contains(&f);
        if real || truth {
            self.candidate_fn(args);
            return if real { Ty::Real } else { Ty::Truth };
        }
        if f.len() >= 9 && f[..9].eq_ignore_ascii_case("calculate") {
            return self.calculate(f, args);
        }
        if builtins::TASK_ARG_FNS.contains(&f) {
            if let Some(Node::Atom(t)) = args.first() {
                let known = self.env.task_ci(t).is_some() || self.env.instance_ci(t).is_some();
                self.refs.push((
                    t.clone(),
                    if known {
                        "task".into()
                    } else {
                        "unknown task".into()
                    },
                ));
                if !known {
                    self.warn(
                        "PF-X062",
                        format!(
                            "{}({}) refers to '{}', which is not a task or component",
                            f, t, t
                        ),
                    );
                }
                // typed as text by the engine; do not resolve as data
                let rest: Vec<Node> = args.to_vec();
                let mut tys = vec![Ty::Text];
                for a in rest.iter().skip(1) {
                    tys.push(self.ty(a, None));
                }
                return self.builtin_with_types(f, &tys, whole, top);
            }
        }
        self.builtin(f, args, whole, top)
    }

    fn builtin_with_types(&mut self, f: &str, tys: &[Ty], whole: &Node, top: Option<&str>) -> Ty {
        for (name, sig, ret) in builtins::SIGNATURES {
            if *name == f
                && sig.len() == tys.len()
                && tys.iter().zip(sig.iter()).all(|(a, e)| can_promote(*a, *e))
            {
                return *ret;
            }
        }
        self.warn(
            "PF-V034",
            format!("Unable to type expression: {}", Self::text_of(whole, top)),
        );
        Ty::Untypeable
    }

    fn result_fn(&mut self, f: &str, x: &str) -> Ty {
        let task = self.env.task_ci(x);
        self.refs.push((
            x.to_string(),
            task.map(|t| t.kind.keyword().to_string())
                .unwrap_or_else(|| "unknown task".into()),
        ));
        match f {
            "activate_trigger" => {
                if task.is_none() {
                    self.warn(
                        "PF-V050",
                        format!(
                            "{} is used an argument activate_trigger but is not a task",
                            x
                        ),
                    );
                }
                Ty::Integer
            }
            _ => {
                match task {
                    Some(d) if d.kind == ItemKind::Decision => {
                        if f == "result_of" && is_multiple(d) {
                            self.warn("PF-V052", " result_of should not be used with multiple choice decisions - use result_set instead".into());
                        }
                        if f == "result_set" && !is_multiple(d) {
                            self.warn("PF-V054", "result_set should not be used with single choice decisions - use result_of instead".into());
                        }
                    }
                    _ => self.warn(
                        if f == "result_of" {
                            "PF-V051"
                        } else {
                            "PF-V053"
                        },
                        format!(
                            "{} is used an argument to result_of but is not the name of a Decision",
                            x
                        ),
                    ),
                }
                if f == "result_of" {
                    Ty::Text
                } else {
                    Ty::SetText
                }
            }
        }
    }

    fn candidate_fn(&mut self, args: &[Node]) {
        let name_of = |n: &Node| match n {
            Node::Atom(a) | Node::Str(a) => Some(a.clone()),
            _ => None,
        };
        let check_cand = |t: &mut Self, dec: &Item, cand: &str| {
            if !dec.candidates().any(|c| c.name.eq_ignore_ascii_case(cand)) {
                t.error(
                    "PF-V064",
                    format!(
                        "Task '{}'does not contain the requested candidate",
                        dec.name
                    ),
                );
            } else {
                t.refs
                    .push((cand.to_string(), format!("candidate of {}", dec.name)));
            }
        };
        match args.len() {
            2 => {
                let Some(x) = name_of(&args[0]) else { return };
                match self.env.instance_ci(&x) {
                    None => self.error("PF-V060", format!("{} is not a decision instance", x)),
                    Some((inst, _)) => {
                        match self.env.task_ci(inst.task_definition()) {
                            Some(d) if d.kind == ItemKind::Decision => {
                                self.refs.push((x.clone(), "decision component".into()));
                                if let Some(c) = name_of(&args[1]) {
                                    check_cand(self, d, &c);
                                }
                            }
                            _ => self.error("PF-V061", format!("{} is not a decision ", x)),
                        }
                    }
                }
            }
            1 => match self.vctx {
                VCtx::Other => self.error(
                    "PF-V062",
                    format!(
                        "Candidate support function used with implicit decision arguments outside a decison context: {}",
                        self.comp
                    ),
                ),
                VCtx::Recommendation(d) | VCtx::ArgCondition(d) => {
                    if let Some(c) = name_of(&args[0]) {
                        check_cand(self, d, &c);
                    }
                }
            },
            _ => {
                if !matches!(self.vctx, VCtx::Recommendation(..)) {
                    self.error(
                        "PF-V063",
                        format!(
                            "Candidate support function used with implicit arguments outside a candidate context:{}",
                            self.comp
                        ),
                    );
                }
            }
        }
    }

    fn calculate(&mut self, f: &str, args: &[Node]) -> Ty {
        let suffix = f[9..].to_lowercase();
        let base = suffix.trim_end_matches('s');
        let plural = suffix.ends_with('s') && suffix != "boolean";
        let ret = if base.starts_with("int") {
            if plural {
                Ty::SetInteger
            } else {
                Ty::Integer
            }
        } else if base.starts_with("real") {
            if plural {
                Ty::SetReal
            } else {
                Ty::Real
            }
        } else if base.starts_with("text") {
            if plural {
                Ty::SetText
            } else {
                Ty::Text
            }
        } else if suffix == "boolean" {
            Ty::Truth
        } else {
            self.warn("PF-V032", format!("Unrecognised function: {}", f));
            return Ty::Untypeable;
        };
        let first_ok = args.first().map(|a| self.ty(a, None)) == Some(Ty::Text);
        if !first_ok {
            self.warn(
                "PF-V031",
                "Incorrectly specified user function - first argument must be a java class name"
                    .into(),
            );
            return Ty::Untypeable;
        }
        ret
    }

    /// `x := e` / `x term_assign e` used as an expression.
    fn assign_expr(&mut self, lhs: &Node, rhs: &Node) -> Ty {
        let name = match lhs {
            Node::Atom(a) => a.clone(),
            other => canon(other),
        };
        let lower = name.to_lowercase();
        let target = self
            .env
            .data()
            .filter(|d| !Env::is_dynamic(d))
            .find(|d| eq(&d.name, &lower, self.env.ignore_data_case));
        let rt = self.ty(rhs, None);
        match target {
            None => {
                self.warn(
                    "PF-V040",
                    format!(
                        "Left hand side of the assignment ({})is not an assignable data item",
                        lower
                    ),
                );
                Ty::Untypeable
            }
            Some(d) => {
                let dt = d
                    .data_type()
                    .and_then(Ty::from_declared)
                    .unwrap_or(Ty::Text);
                if rt != dt {
                    self.warn(
                        "PF-V041",
                        format!(
                            "Data item {} cannot be assigned a value of type {}",
                            lower, rt
                        ),
                    );
                    Ty::Untypeable
                } else {
                    Ty::Truth
                }
            }
        }
    }

    /// Check a whole property expression against an expected type (PF-V011).
    fn check(&mut self, e: &Expr, expected: Ty, prop: &str) -> Ty {
        let t = self.ty(&e.node, Some(e.text.as_str()));
        if !can_promote(t, expected) {
            self.warn(
                "PF-V011",
                format!(
                    "{} should be of type {} but is actually of type {}\n    {} is {}",
                    prop, expected, t, prop, e.text
                ),
            );
        }
        t
    }
}

enum AssertMode<'a> {
    Postcondition,
    ParamValue(Option<&'a Vec<Parameter>>),
}

// ── the linter ───────────────────────────────────────────────────────────────

struct Linter<'e, 'a> {
    env: &'e Env<'a>,
    out: Vec<Diagnostic>,
    /// index into `out` where the component currently being verified started
    item_start: usize,
}

impl<'e, 'a> Linter<'e, 'a> {
    fn push(&mut self, d: Diagnostic) {
        self.out.push(d);
    }

    fn typer(&self, comp: &str, span: Span) -> Typer<'e, 'a> {
        Typer::new(self.env, comp, span)
    }

    fn absorb(&mut self, t: Typer) {
        self.out.extend(t.diags);
    }

    // ── header ───────────────────────────────────────────────────────────────

    fn header(&mut self) {
        let doc = self.env.doc;
        if doc.metadata.is_none() && doc.engine_version.is_some() {
            self.push(Diagnostic::error(
                "PF-L008",
                "no `metadata :: '...'; ... end metadata.` block: Composer's engine crashes (NullPointerException) while loading this file; run `format` (or set_metadata) to add one",
            ));
        }
        if doc.directives.is_some() {
            self.push(Diagnostic::error(
                "PF-L010",
                "a `directives` block can never be parsed by Composer (its grammar has no terminating rule); remove it",
            ));
        }
        let check_props = |owner: &str, s: &str, out: &mut Vec<Diagnostic>| {
            if !valid_properties_escapes(s) {
                out.push(
                    Diagnostic::error("PF-L007", "Malformed \\uxxxx encoding.")
                        .in_component(owner.to_string()),
                );
            }
        };
        let mut found = Vec::new();
        if let Some(p) = &doc.metaprops {
            check_props("guideline metaprops", p, &mut found);
        }
        crate::model::walk_attrs(doc, &mut |path, _, a| {
            if let Attr::Metaprops(p) = a {
                check_props(path, p, &mut found);
            }
        });
        self.out.extend(found);
    }

    // ── construction (PF-C*) ─────────────────────────────────────────────────

    fn construction(&mut self) {
        let env = self.env;
        // duplicate task definitions (root excluded) / data items / functions
        let mut seen: HashMap<String, &Item> = HashMap::new();
        for t in env.tasks() {
            let key = if env.ignore_task_case {
                t.name.to_lowercase()
            } else {
                t.name.clone()
            };
            if seen.insert(key, t).is_some() {
                self.push(
                    Diagnostic::error(
                        "PF-C001",
                        format!("Duplicate definition of task {}", t.name),
                    )
                    .at(t.span)
                    .in_component(t.name.clone()),
                );
            }
        }
        let mut seen: HashSet<String> = HashSet::new();
        for d in env.data() {
            let key = if env.ignore_data_case {
                d.name.to_lowercase()
            } else {
                d.name.clone()
            };
            if !seen.insert(key) {
                self.push(
                    Diagnostic::error("PF-C003", format!("Duplicate data definition: {}", d.name))
                        .at(d.span)
                        .in_component(d.name.clone()),
                );
            }
        }
        let mut seen: HashSet<&str> = HashSet::new();
        for f in env.functions() {
            if !seen.insert(f.name.as_str()) {
                self.push(
                    Diagnostic::warning(
                        "PF-C007",
                        format!("duplicate function '{}': the engine silently keeps the last definition", f.name),
                    )
                    .at(f.span)
                    .in_component(f.name.clone()),
                );
            }
        }
        // duplicate component names in plans
        for p in env.items().filter(|i| i.kind == ItemKind::Plan) {
            let mut names: HashSet<String> = HashSet::new();
            for c in p.components() {
                if !names.insert(c.name.to_lowercase()) {
                    let is_root = env.root.is_some_and(|r| std::ptr::eq(r, p));
                    if is_root {
                        self.push(
                            Diagnostic::error(
                                "PF-C002",
                                format!(
                                    "Duplicate task instance path {}#{}",
                                    p.name.to_lowercase(),
                                    c.name.to_lowercase()
                                ),
                            )
                            .at(c.span)
                            .in_component(p.name.clone()),
                        );
                    } else {
                        self.push(
                            Diagnostic::warning(
                                "PF-X034",
                                format!(
                                    "plan '{}' has two components named '{}' (names are compared case-insensitively by scheduling and the verifier)",
                                    p.name, c.name
                                ),
                            )
                            .at(c.span)
                            .in_component(p.name.clone()),
                        );
                    }
                }
            }
        }
        // duplicate single-valued attributes (first wins)
        let mut dups = Vec::new();
        for item in env.items() {
            dup_attrs(&item.name, &item.attrs, &mut dups);
            for a in &item.attrs {
                if let Some(ch) = a.children() {
                    dup_attrs(&item.name, ch, &mut dups);
                }
            }
        }
        self.out.extend(dups);

        // data item defaults and range captions
        for d in env.data() {
            let ty = d.data_type().unwrap_or("text");
            if let Some(Attr::DefaultValue(e)) =
                d.attrs.iter().find(|a| matches!(a, Attr::DefaultValue(_)))
            {
                if let Some(diag) = date_default(ty, &e.node, &d.name) {
                    self.push(diag.at(e.span).in_component(d.name.clone()));
                }
            }
            if let Some(Attr::Range(v)) = d.attrs.iter().find(|a| matches!(a, Attr::Range(_))) {
                for e in v {
                    if let Node::Str(s) = &e.node {
                        let value = s.split("||").next().unwrap_or("");
                        let bad = match ty {
                            "integer" | "setof_integer" => value.trim().parse::<i32>().is_err(),
                            "real" | "setof_real" => value.trim().parse::<f64>().is_err(),
                            "date" | "time" | "datetime" => true,
                            _ => false,
                        };
                        if bad {
                            self.push(
                                Diagnostic::error(
                                    "PF-C006",
                                    format!("Could not process range caption for {}", d.name),
                                )
                                .at(e.span)
                                .in_component(d.name.clone()),
                            );
                            break;
                        }
                    }
                }
            }
        }
    }

    // ── reference resolution (PF-R*) ─────────────────────────────────────────

    fn resolution(&mut self) -> bool {
        let env = self.env;
        let before = self.out.len();
        for p in env.items().filter(|i| i.kind == ItemKind::Plan) {
            for c in p.components() {
                if env.task_def(c.task_definition()).is_none() {
                    let mut msg = format!(
                        "ERROR: component {} is defined by task definition {} which cannot be found",
                        c.name,
                        c.task_definition()
                    );
                    if env.root.is_some_and(|r| r.name == c.task_definition()) {
                        msg.push_str(" (the root plan cannot be used as a component)");
                    } else if let Some(t) = env
                        .tasks()
                        .find(|t| t.name.eq_ignore_ascii_case(c.task_definition()))
                    {
                        msg.push_str(&format!(
                            " (did you mean '{}'? task names are case-sensitive)",
                            t.name
                        ));
                    }
                    self.push(
                        Diagnostic::error("PF-R001", msg)
                            .at(c.span)
                            .in_component(p.name.clone()),
                    );
                }
                for s in c.schedule_constraints() {
                    if !p.components().any(|x| x.name.eq_ignore_ascii_case(s)) {
                        self.push(
                            Diagnostic::error(
                                "PF-R002",
                                format!(
                                    "ERROR: component {} has a schedule constraint on a task {} which cannot be found in the appropriate scope ",
                                    c.name, s
                                ),
                            )
                            .at(c.span)
                            .in_component(p.name.clone()),
                        );
                    }
                }
            }
        }
        for t in env
            .items()
            .filter(|i| matches!(i.kind, ItemKind::Enquiry | ItemKind::Decision))
        {
            for s in t.sources() {
                match env.data_item(s.data_definition()) {
                    None => self.push(
                        Diagnostic::error(
                            "PF-R003",
                            format!(
                                "ERROR: cannot find data definition: {} for source {}",
                                s.data_definition(),
                                s.name
                            ),
                        )
                        .at(s.span)
                        .in_component(t.name.clone()),
                    ),
                    Some(d) => {
                        let ty = d.data_type().unwrap_or("text");
                        if let Some(Attr::DefaultValue(e)) =
                            s.attrs.iter().find(|a| matches!(a, Attr::DefaultValue(_)))
                        {
                            if let Some(diag) = date_default(ty, &e.node, &s.name) {
                                let sev = if diag.code == "PF-C005" {
                                    Severity::Error
                                } else {
                                    Severity::Warning
                                };
                                let mut diag = diag;
                                diag.severity = sev;
                                diag.code = "PF-R004".into();
                                self.push(diag.at(e.span).in_component(t.name.clone()));
                            }
                        }
                    }
                }
            }
        }
        !self.out[before..]
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    // ── post-processing (PF-PP*) ─────────────────────────────────────────────

    fn post_processing(&mut self) {
        for d in self.env.data() {
            if Env::is_dynamic(d) && !d.attrs.iter().any(|a| matches!(a, Attr::Derivation(_))) {
                self.push(
                    Diagnostic::error(
                        "PF-PP001",
                        format!(
                            "Dynamic data cannot have empty derivation. Data: {}",
                            d.name
                        ),
                    )
                    .at(d.span)
                    .in_component(d.name.clone()),
                );
            }
        }
    }

    // ── guideline validation (PF-G*, engine 1.9) ─────────────────────────────

    fn guideline(&mut self) {
        let env = self.env;
        for t in env.items() {
            for s in t.sources() {
                for (kw, code, label) in [
                    ("default_value", "PF-G001", "defaultValue"),
                    ("derivation483", "PF-G002", "derivation483"),
                    ("dynamic483", "PF-G003", "dynamic483"),
                ] {
                    let present = s.attrs.iter().any(|a| match a {
                        Attr::Dynamic483(b) => *b && kw == "dynamic483",
                        other => other.keyword() == kw && kw != "dynamic483",
                    });
                    if present {
                        self.push(
                            Diagnostic::warning(
                                code,
                                format!(
                                    "Source {} has a {} which is not allowed (engine 1.9; Composer drops it when saving)",
                                    s.name, label
                                ),
                            )
                            .at(s.span)
                            .in_component(t.name.clone()),
                        );
                    }
                }
            }
        }

        // recursive plans (engine stack overflow) and reachability
        let mut reachable: HashSet<&str> = HashSet::new();
        let mut reported: HashSet<String> = HashSet::new();
        if let Some(root) = env.root {
            let mut stack: Vec<&str> = vec![root.name.as_str()];
            self.walk_plans(root, &mut stack, &mut reachable, &mut reported);
        }
        for t in env.tasks() {
            if !reachable.contains(t.name.as_str()) {
                self.push(
                    Diagnostic::warning(
                        "PF-G004",
                        format!(
                            "Task definition {} does not have any instances (Composer silently drops it when saving)",
                            t.name
                        ),
                    )
                    .at(t.span)
                    .in_component(t.name.clone()),
                );
            }
        }
    }

    fn walk_plans(
        &mut self,
        plan: &'a Item,
        stack: &mut Vec<&'a str>,
        reachable: &mut HashSet<&'a str>,
        reported: &mut HashSet<String>,
    ) {
        for c in plan.components() {
            let Some(def) = self.env.task_def(c.task_definition()) else {
                continue;
            };
            if stack.iter().any(|s| *s == def.name) {
                let cycle = format!("{} -> {}", stack.join(" -> "), def.name);
                if reported.insert(def.name.clone()) {
                    self.push(
                        Diagnostic::error(
                            "PF-G005",
                            format!(
                                "plan '{}' contains itself ({}); Composer's engine crashes with a stack overflow on recursive plans",
                                def.name, cycle
                            ),
                        )
                        .at(c.span)
                        .in_component(plan.name.clone()),
                    );
                }
                continue;
            }
            let first = reachable.insert(def.name.as_str());
            if def.kind == ItemKind::Plan && first {
                stack.push(def.name.as_str());
                self.walk_plans(def, stack, reachable, reported);
                stack.pop();
            }
        }
    }

    // ── verifier (PF-V*) ─────────────────────────────────────────────────────

    fn verify(&mut self) {
        let env = self.env;
        // duplicates under lower-casing
        let mut seen: HashMap<String, &str> = HashMap::new();
        for t in env.items().filter(|i| i.kind.is_task()) {
            if let Some(prev) = seen.insert(t.name.to_lowercase(), &t.name) {
                if prev != t.name {
                    self.push(
                        Diagnostic::error(
                            "PF-V001",
                            format!("Duplicate definition of task {}", t.name),
                        )
                        .at(t.span)
                        .in_component(env.root.map(|r| r.name.clone()).unwrap_or_default()),
                    );
                }
            }
        }
        let mut seen: HashMap<String, &str> = HashMap::new();
        for d in env.data() {
            if let Some(prev) = seen.insert(d.name.to_lowercase(), &d.name) {
                if prev != d.name {
                    self.push(
                        Diagnostic::error(
                            "PF-V001",
                            format!("Duplicate definition of data item {}", d.name),
                        )
                        .at(d.span)
                        .in_component(env.root.map(|r| r.name.clone()).unwrap_or_default()),
                    );
                }
            }
        }

        for item in env.items() {
            match item.kind {
                ItemKind::Data => self.verify_data(item),
                ItemKind::Function => self.verify_function(item),
                _ => self.verify_task(item),
            }
        }
    }

    fn verify_task(&mut self, t: &'a Item) {
        let name = t.name.clone();
        self.item_start = self.out.len();
        for a in &t.attrs {
            // the engine aborts the rest of a component after an internal error (PF-V002)
            if self.aborted() {
                return;
            }
            match a {
                Attr::Goal(e) => {
                    let mut ty = self.typer(&name, e.span);
                    ty.check(e, Ty::Truth, "goal");
                    self.absorb(ty);
                }
                Attr::Precondition(e) => {
                    let mut ty = self.typer(&name, e.span);
                    ty.check(e, Ty::Truth, "precondition");
                    self.absorb(ty);
                }
                Attr::Postcondition(e) => {
                    let mut ty = self.typer(&name, e.span);
                    assertion(&mut ty, &e.node, &AssertMode::Postcondition);
                    self.absorb(ty);
                }
                Attr::WaitCondition(e) => {
                    let mut ty = self.typer(&name, e.span);
                    ty.check(e, Ty::Truth, "wait_condition");
                    self.absorb_unchecked(ty, "wait_condition");
                }
                Attr::Abort(e) => {
                    let mut ty = self.typer(&name, e.span);
                    ty.check(e, Ty::Truth, "Abort condition");
                    self.absorb(ty);
                }
                Attr::Terminate(e) => {
                    let mut ty = self.typer(&name, e.span);
                    ty.check(e, Ty::Truth, "Termination condition");
                    self.absorb(ty);
                }
                Attr::Component(c) => self.verify_component(t, c),
                _ => {}
            }
        }
        match t.kind {
            ItemKind::Decision => {
                if t.candidates().next().is_none() {
                    self.push(
                        Diagnostic::warning(
                            "PF-V003",
                            format!("Decision {} has no candidates", t.name),
                        )
                        .at(t.span)
                        .in_component(name.clone()),
                    );
                }
                for c in t.candidates() {
                    self.verify_candidate(t, c);
                }
            }
            ItemKind::Enquiry if t.sources().next().is_none() => {
                self.push(
                    Diagnostic::warning("PF-V004", "The Enquiry has no sources")
                        .at(t.span)
                        .in_component(name),
                );
            }
            _ => {}
        }
    }

    fn aborted(&self) -> bool {
        self.out[self.item_start..]
            .iter()
            .any(|d| d.code == "PF-V002")
    }

    fn verify_component(&mut self, plan: &'a Item, c: &'a Block) {
        let pname = plan.name.clone();
        let def = self.env.task_def(c.task_definition());
        for a in &c.attrs {
            if self.aborted() {
                return;
            }
            match a {
                Attr::CycleRepeat(e, _) => {
                    let mut ty = self.typer(&pname, e.span);
                    ty.check(
                        e,
                        Ty::Integer,
                        &format!("Cycle repeat expression of {}", c.name),
                    );
                    self.absorb(ty);
                }
                Attr::CycleUntil(e) => {
                    let mut ty = self.typer(&pname, e.span);
                    ty.check(
                        e,
                        Ty::Truth,
                        &format!("Cycle until expression of {}", c.name),
                    );
                    self.absorb(ty);
                }
                Attr::NumberOfCycles(e) => {
                    let mut ty = self.typer(&pname, e.span);
                    ty.check(e, Ty::Integer, &format!("Number of cycles of {}", c.name));
                    self.absorb(ty);
                }
                Attr::ParamValue(e) => {
                    let params = def.and_then(|d| {
                        d.attrs.iter().find_map(|x| match x {
                            Attr::Parameters(p) => Some(p),
                            _ => None,
                        })
                    });
                    let mut ty = self.typer(&pname, e.span);
                    assertion(&mut ty, &e.node, &AssertMode::ParamValue(params));
                    self.absorb(ty);
                }
                _ => {}
            }
        }
    }

    fn verify_candidate(&mut self, dec: &'a Item, c: &'a Block) {
        let dname = dec.name.clone();
        let numeric = is_numeric(dec);
        let mut has_rec = false;
        for a in &c.attrs {
            match a {
                Attr::Recommendation(e) => {
                    has_rec = true;
                    let mut ty = self.typer(&dname, e.span);
                    ty.vctx = VCtx::Recommendation(dec);
                    ty.check(
                        e,
                        Ty::Truth,
                        &format!("recommendation rule of candidate {}", c.name),
                    );
                    self.absorb(ty);
                }
                Attr::Argument(arg) => {
                    let mut ty = self.typer(&dname, arg.condition.span);
                    ty.vctx = VCtx::ArgCondition(dec);
                    // Composer's rule validator turns every warning here into an error
                    ty.escalate = true;
                    ty.check(
                        &arg.condition,
                        Ty::Truth,
                        &format!("condition for argument: {}", arg.condition.text),
                    );
                    self.absorb(ty);
                    match (&arg.support, numeric) {
                        (Support::Weight(w), false) => self.push(
                            Diagnostic::warning(
                                "PF-V006",
                                format!(
                                    "not a valid symbolic support value: argument weight '{}' in symbolic decision {} (Composer writes this argument as unparsable text; use for/against/confirming/excluding or set support_mode :: numeric)",
                                    w.text, dname
                                ),
                            )
                            .at(arg.span)
                            .in_component(dname.clone()),
                        ),
                        (Support::Weight(w), true) => {
                            let mut ty = self.typer(&dname, w.span);
                            ty.vctx = VCtx::ArgCondition(dec);
                            ty.escalate = true;
                            ty.check(w, Ty::Real, &format!("weight for argument: {}", arg.condition.text));
                            self.absorb_unchecked(ty, "numeric argument weight");
                        }
                        (s, true) => self.push(
                            Diagnostic::error(
                                "PF-X031",
                                format!(
                                    "Argument uses symbolic support '{}' in a numeric-support decision (Composer's writer crashes on this); use a numeric weight such as 1 or -1",
                                    s.label()
                                ),
                            )
                            .at(arg.span)
                            .in_component(dname.clone()),
                        ),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if !has_rec {
            self.push(
                Diagnostic::warning(
                    "PF-V005",
                    format!(
                        "The Candidate, {}  has no recommendation rule - all arguments will be ignored",
                        c.name
                    ),
                )
                .at(c.span)
                .in_component(dname),
            );
        }
    }

    fn verify_data(&mut self, d: &'a Item) {
        let name = d.name.clone();
        let declared = d.data_type().unwrap_or("text");
        let dt = Ty::from_declared(declared).unwrap_or(Ty::Text);
        for a in &d.attrs {
            match a {
                Attr::DefaultValue(e) => {
                    let mut ty = self.typer(&name, e.span);
                    ty.check(e, dt, "Default Value");
                    self.absorb(ty);
                }
                Attr::Derivation(e) => {
                    let mut ty = self.typer(&name, e.span);
                    ty.check(e, dt, "Default Value");
                    self.absorb(ty);
                }
                Attr::MandatoryValidation(e) => {
                    let mut ty = self.typer(&name, e.span);
                    ty.check(e, Ty::Truth, "Default Value");
                    self.absorb(ty);
                }
                Attr::Range(v) => {
                    for e in v {
                        let captioned = matches!(&e.node, Node::Str(s) if s.contains("||"));
                        if captioned
                            && matches!(dt, Ty::Integer | Ty::Real | Ty::SetInteger | Ty::SetReal)
                        {
                            self.push(
                                Diagnostic::info(
                                    "PF-V011",
                                    format!(
                                        "Range Expression {} should be of type {} but is actually of type text (the engine reports this for every captioned numeric range value; harmless)",
                                        e.text,
                                        dt.element()
                                    ),
                                )
                                .at(e.span)
                                .in_component(name.clone()),
                            );
                            continue;
                        }
                        let mut ty = self.typer(&name, e.span);
                        ty.check(e, dt.element(), &format!("Range Expression {}", e.text));
                        self.absorb(ty);
                    }
                }
                Attr::WarningCondition(w) => {
                    let mut ty = self.typer(&name, w.condition.span);
                    ty.check(
                        &w.condition,
                        Ty::Truth,
                        &format!("Warning condition: {}", w.condition.text),
                    );
                    self.absorb(ty);
                }
                _ => {}
            }
        }
    }

    /// Functions are never verified by the engine; we check them as an extension.
    fn verify_function(&mut self, f: &'a Item) {
        let name = f.name.clone();
        let rt = f
            .data_type()
            .and_then(Ty::from_declared)
            .unwrap_or(Ty::Text);
        for a in &f.attrs {
            match a {
                Attr::FunctionBody(e) => {
                    let mut ty = self.typer(&name, e.span);
                    if is_assertion_shape(&e.node) {
                        assertion(&mut ty, &e.node, &AssertMode::Postcondition);
                    } else {
                        ty.check(e, rt, "function_body");
                    }
                    self.absorb_unchecked(ty, "function_body");
                }
                Attr::FunctionTrigger(e) => {
                    let mut ty = self.typer(&name, e.span);
                    ty.check(e, Ty::Truth, "function_trigger");
                    self.absorb_unchecked(ty, "function_trigger");
                }
                _ => {}
            }
        }
    }

    /// Findings in expressions the engine never verifies are reported as PF-X050.
    fn absorb_unchecked(&mut self, t: Typer, what: &str) {
        for mut d in t.diags {
            d.message = format!("{} [{} is not checked by Composer]", d.message, what);
            d.code = "PF-X050".into();
            if d.severity == Severity::Error {
                d.severity = Severity::Warning;
            }
            self.out.push(d);
        }
    }

    // ── our extra checks (PF-X*) ─────────────────────────────────────────────

    fn extras(&mut self) {
        let env = self.env;

        // scheduling cycles and self-constraints
        for p in env.items().filter(|i| i.kind == ItemKind::Plan) {
            let comps: Vec<&Block> = p.components().collect();
            for c in &comps {
                if c.schedule_constraints()
                    .any(|s| s.eq_ignore_ascii_case(&c.name))
                {
                    self.push(
                        Diagnostic::warning(
                            "PF-X102",
                            format!(
                                "component {} waits for its own completion (it can never start)",
                                c.name
                            ),
                        )
                        .at(c.span)
                        .in_component(p.name.clone()),
                    );
                }
            }
            if let Some(cycle) = find_schedule_cycle(&comps) {
                self.push(
                    Diagnostic::warning(
                        "PF-X101",
                        format!("Schedule constraints form a cycle: {} (none of these components can start)", cycle.join(" -> ")),
                    )
                    .in_component(p.name.clone()),
                );
            }
        }

        for t in env.items() {
            // dynamic data used as a source; duplicate sources / candidates
            let mut srcs = HashSet::new();
            for s in t.sources() {
                if let Some(d) = env.data_item(s.data_definition()) {
                    if Env::is_dynamic(d) {
                        self.push(
                            Diagnostic::warning("PF-X030", format!("Source {} uses dynamic data item {} (Composer removes such sources)", s.name, d.name))
                                .at(s.span)
                                .in_component(t.name.clone()),
                        );
                    }
                }
                if !srcs.insert(s.data_definition().to_lowercase()) {
                    self.push(
                        Diagnostic::warning(
                            "PF-X032",
                            format!(
                                "{} has more than one source for {}",
                                t.name,
                                s.data_definition()
                            ),
                        )
                        .at(s.span)
                        .in_component(t.name.clone()),
                    );
                }
            }
            let mut cands = HashSet::new();
            for c in t.candidates() {
                if !cands.insert(c.name.to_lowercase()) {
                    self.push(
                        Diagnostic::warning(
                            "PF-X033",
                            format!(
                                "decision {} has more than one candidate named {}",
                                t.name, c.name
                            ),
                        )
                        .at(c.span)
                        .in_component(t.name.clone()),
                    );
                }
            }
            // names Composer cannot write back / expressions cannot reference
            if t.name.contains('\'') || t.name.contains('\\') {
                self.push(
                    Diagnostic::warning("PF-X036", format!("name '{}' contains a quote or backslash: Composer writes names unescaped and would corrupt the file", t.name))
                        .at(t.span)
                        .in_component(t.name.clone()),
                );
            } else if !is_bare_atom(&t.name) && !t.name.contains(':') {
                self.push(
                    Diagnostic::info("PF-X020", format!("name '{}' is not a plain identifier (or is a reserved word), so expressions must quote it", t.name))
                        .at(t.span)
                        .in_component(t.name.clone()),
                );
            }
            // writer defects
            for a in &t.attrs {
                match a {
                    Attr::WarningCondition(w) => {
                        if let Constant::Atom(s) | Constant::String(s) = &w.flag {
                            if !is_bare_atom(s) {
                                self.push(
                                    Diagnostic::warning("PF-X037", format!("warning flag '{}' is not an identifier or number: Composer writes flags unquoted and would produce an unparsable file", s))
                                        .at(w.span)
                                        .in_component(t.name.clone()),
                                );
                            }
                        }
                    }
                    Attr::Parameters(ps) => {
                        for p in ps {
                            if p.data_type.is_none() && p.attrs.iter().any(|x| x.is_generic()) {
                                self.push(
                                    Diagnostic::warning("PF-X038", format!("parameter {} has a caption/description but no type: Composer writes it as unparsable text and param_value assignments to it crash the verifier", p.name))
                                        .at(t.span)
                                        .in_component(t.name.clone()),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // decision results compared with non-candidates
        let mut found = Vec::new();
        crate::model::walk_attrs(env.doc, &mut |_path, item, attr| {
            for e in attr.exprs() {
                result_comparisons(env, &e.node, &mut |dec, value| {
                    found.push(
                        Diagnostic::warning(
                            "PF-X061",
                            format!(
                                "'{}' is compared with the result of decision {}, but {} has no candidate named '{}' (candidates: {})",
                                value,
                                dec.name,
                                dec.name,
                                value,
                                dec.candidates().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
                            ),
                        )
                        .at(e.span)
                        .in_component(item.name.clone()),
                    )
                });
            }
        });
        self.out.extend(found);

        // unresolved atoms (informational: they are text literals to the engine)
        let literals = known_literals(env);
        let mut infos = Vec::new();
        crate::model::walk_attrs(env.doc, &mut |_path, item, attr| {
            if matches!(attr, Attr::Range(_)) {
                return;
            }
            let params: Vec<&str> = item
                .attrs
                .iter()
                .filter_map(|a| match a {
                    Attr::Parameters(p) => {
                        Some(p.iter().map(|x| x.name.as_str()).collect::<Vec<_>>())
                    }
                    _ => None,
                })
                .flatten()
                .collect();
            for e in attr.exprs() {
                for name in data_position_atoms(&e.node) {
                    if env.typed(&name).is_none()
                        && !literals.contains(&name.to_lowercase())
                        && !params.contains(&name.as_str())
                    {
                        infos.push(
                            Diagnostic::info(
                                "PF-X060",
                                format!("'{}' is not a data item or function; the engine treats it as the text literal \"{}\"", name, name),
                            )
                            .at(e.span)
                            .in_component(item.name.clone()),
                        );
                    }
                }
            }
        });
        self.out.extend(infos);
    }
}

fn is_assertion_shape(n: &Node) -> bool {
    match n {
        Node::Bin(BinOp::Equals | BinOp::Assign | BinOp::TermAssign, l, _) => {
            matches!(l.as_ref(), Node::Atom(_))
        }
        Node::Bin(BinOp::And, a, b) => is_assertion_shape(a) && is_assertion_shape(b),
        Node::Paren(x) => is_assertion_shape(x),
        _ => false,
    }
}

/// Verify an assertion (postcondition / param_value / function body assertion).
fn assertion(ty: &mut Typer, n: &Node, mode: &AssertMode) {
    match n {
        Node::Paren(x) => assertion(ty, x, mode),
        Node::Bin(BinOp::And, a, b) => {
            assertion(ty, a, mode);
            assertion(ty, b, mode);
        }
        Node::Atom(f) => {
            if !ty.env.functions().any(|x| x.name.eq_ignore_ascii_case(f)) {
                ty.warn(
                    "PF-V020",
                    format!("{} is not a function. Only functions or assignments are allowed in assertiongs", f),
                );
            }
        }
        Node::Bin(BinOp::Equals | BinOp::Assign | BinOp::TermAssign, l, r) => {
            let Node::Atom(lhs) = l.as_ref() else { return };
            let lhs_ty = match mode {
                AssertMode::Postcondition => {
                    let d = ty
                        .env
                        .data()
                        .filter(|d| !Env::is_dynamic(d))
                        .find(|d| eq(&d.name, lhs, ty.env.ignore_data_case));
                    match d {
                        Some(d) => d
                            .data_type()
                            .and_then(Ty::from_declared)
                            .unwrap_or(Ty::Text),
                        None => {
                            ty.warn(
                                "PF-V021",
                                format!("Can't find Data Item on left hand side of {}", canon(n)),
                            );
                            return;
                        }
                    }
                }
                AssertMode::ParamValue(params) => {
                    let p =
                        params.and_then(|ps| ps.iter().find(|p| p.name.eq_ignore_ascii_case(lhs)));
                    match p {
                        None => {
                            ty.warn(
                                "PF-V021",
                                format!("Can't find Parameter on left hand side of {}", canon(n)),
                            );
                            return;
                        }
                        Some(p) => match p.data_type.as_deref().and_then(Ty::from_declared) {
                            Some(t) => t,
                            None => {
                                ty.error(
                                    "PF-V002",
                                    format!(
                                        "Unexplained error - contact development team! (parameter {} has no declared type; give it `attributes type :: <type>; end attributes`)",
                                        p.name
                                    ),
                                );
                                return;
                            }
                        },
                    }
                }
            };
            let rt = ty.ty(r, None);
            if rt != lhs_ty {
                ty.warn(
                    "PF-V022",
                    format!(
                        "Data item or parameter {} cannot be assigned a value of type {}",
                        // parameter names are looked up (and reported) lower-cased
                        if matches!(mode, AssertMode::ParamValue(_)) {
                            lhs.to_lowercase()
                        } else {
                            lhs.clone()
                        },
                        rt
                    ),
                );
            }
        }
        other => {
            ty.warn(
                "PF-V020",
                format!("{} is not a function. Only functions or assignments are allowed in assertiongs", canon(other)),
            );
        }
    }
}

fn dup_attrs(owner: &str, attrs: &[Attr], out: &mut Vec<Diagnostic>) {
    let mut seen = HashSet::new();
    for a in attrs {
        if a.is_repeatable() {
            continue;
        }
        if !seen.insert(a.keyword()) {
            out.push(
                Diagnostic::warning(
                    "PF-C008",
                    format!("'{}' is given more than once; the engine keeps the first value and ignores the rest", a.keyword()),
                )
                .in_component(owner.to_string()),
            );
        }
    }
}

/// Date-typed default checks shared by data items (PF-C004/C005) and sources (PF-R004).
fn date_default(ty: &str, n: &Node, name: &str) -> Option<Diagnostic> {
    let n = match n {
        Node::Paren(x) => x.as_ref(),
        other => other,
    };
    let want = match ty {
        "datetime" => "datetime",
        "date" => "date",
        "time" => "time",
        _ => return None,
    };
    let have = match n {
        Node::DateTime(_) => "datetime",
        Node::Date(_) => "date",
        Node::Time(_) => "time",
        _ => {
            return Some(Diagnostic::error(
                "PF-C005",
                format!(
                    "default value of {} must be a {} literal (e.g. {}); Composer's engine crashes with a ClassCastException otherwise",
                    name,
                    want,
                    match want {
                        "datetime" => "2024-01-31T09:00:00.000",
                        "date" => "2024-01-31",
                        _ => "09:00:00.000",
                    }
                ),
            ))
        }
    };
    (have != want).then(|| {
        Diagnostic::error(
            "PF-C004",
            format!("Non {} default value given form data item {}", want, name),
        )
    })
}

/// `java.util.Properties` rejects `\u` not followed by four hex digits.
fn valid_properties_escapes(s: &str) -> bool {
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '\\' && i + 1 < b.len() {
            if b[i + 1] == 'u' {
                let hex = &b[i + 2..(i + 6).min(b.len())];
                if hex.len() < 4 || !hex.iter().all(|c| c.is_ascii_hexdigit()) {
                    return false;
                }
                i += 6;
                continue;
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    true
}

fn find_schedule_cycle(comps: &[&Block]) -> Option<Vec<String>> {
    let idx = |n: &str| comps.iter().position(|c| c.name.eq_ignore_ascii_case(n));
    // edges: component -> antecedent
    let mut state = vec![0u8; comps.len()];
    fn dfs(
        i: usize,
        comps: &[&Block],
        idx: &dyn Fn(&str) -> Option<usize>,
        state: &mut Vec<u8>,
        path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        state[i] = 1;
        path.push(i);
        for s in comps[i].schedule_constraints() {
            let Some(j) = idx(s) else { continue };
            if j == i {
                continue; // self-loops are reported separately
            }
            if state[j] == 1 {
                let start = path.iter().position(|&x| x == j).unwrap();
                let mut cyc = path[start..].to_vec();
                cyc.push(j);
                return Some(cyc);
            }
            if state[j] == 0 {
                if let Some(c) = dfs(j, comps, idx, state, path) {
                    return Some(c);
                }
            }
        }
        path.pop();
        state[i] = 2;
        None
    }
    for i in 0..comps.len() {
        if state[i] == 0 {
            let mut path = Vec::new();
            if let Some(c) = dfs(i, comps, &idx, &mut state, &mut path) {
                // edges point to antecedents: reverse to read in execution order
                let mut names: Vec<String> = c.iter().map(|&k| comps[k].name.clone()).collect();
                names.reverse();
                return Some(names);
            }
        }
    }
    None
}

/// Find `result_of(D) = v` / `result_set(D) includes v` where v is not a candidate of D.
fn result_comparisons<'a>(env: &Env<'a>, n: &Node, f: &mut dyn FnMut(&'a Item, String)) {
    let decision_of = |x: &Node| -> Option<&'a Item> {
        let name = match x {
            Node::ResultOf(d) => d.as_str(),
            Node::Call(fname, args) if fname == "result_set" => match args.first() {
                Some(Node::Atom(d)) => d.as_str(),
                _ => return None,
            },
            Node::Paren(inner) => {
                return match inner.as_ref() {
                    Node::ResultOf(d) => env.task_ci(d),
                    _ => None,
                }
            }
            _ => return None,
        };
        env.task_ci(name).filter(|t| t.kind == ItemKind::Decision)
    };
    let values = |x: &Node| -> Vec<String> {
        match x {
            Node::Atom(s) | Node::Str(s) => vec![s.clone()],
            Node::List(v) => v
                .iter()
                .filter_map(|e| match e {
                    Node::Atom(s) | Node::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        }
    };
    n.walk(&mut |node| {
        if let Node::Bin(
            BinOp::Equals
            | BinOp::NotEquals
            | BinOp::Includes
            | BinOp::IncludesAny
            | BinOp::IncludesAll
            | BinOp::Oneof,
            a,
            b,
        ) = node
        {
            let (dec, other) = match (decision_of(a), decision_of(b)) {
                (Some(d), _) => (d, b.as_ref()),
                (_, Some(d)) => (d, a.as_ref()),
                _ => return,
            };
            for v in values(other) {
                if !dec.candidates().any(|c| c.name.eq_ignore_ascii_case(&v)) {
                    f(dec, v);
                }
            }
        }
    });
}

/// Names that are legitimately used as text literals: candidate names, task and
/// component names, range values.
fn known_literals(env: &Env) -> HashSet<String> {
    let mut s = HashSet::new();
    for i in env.items() {
        s.insert(i.name.to_lowercase());
        for c in i.candidates() {
            s.insert(c.name.to_lowercase());
        }
        for c in i.components() {
            s.insert(c.name.to_lowercase());
        }
        for a in &i.attrs {
            if let Attr::Range(v) = a {
                for e in v {
                    if let Node::Atom(x) = &e.node {
                        s.insert(x.to_lowercase());
                    }
                }
            }
            if let Attr::Trigger(t) = a {
                s.insert(t.to_lowercase());
            }
        }
    }
    for w in ["true", "false", "yes", "no", "unknown", "now"] {
        s.insert(w.to_string());
    }
    s
}

/// Atoms that the engine resolves as data/function references (not task,
/// candidate or decision arguments of special functions).
fn data_position_atoms(n: &Node) -> Vec<String> {
    let mut out = Vec::new();
    fn go(n: &Node, out: &mut Vec<String>) {
        match n {
            Node::Atom(a) => out.push(a.clone()),
            Node::Complex(a, b) => out.push(format!("{}:{}", a, b)),
            Node::Call(f, args) => {
                let skip_all = builtins::CANDIDATE_REAL_FNS.contains(&f.as_str())
                    || builtins::CANDIDATE_TRUTH_FNS.contains(&f.as_str())
                    || f == "result_set"
                    || f == "activate_trigger";
                let skip_first = builtins::TASK_ARG_FNS.contains(&f.as_str());
                for (i, a) in args.iter().enumerate() {
                    if skip_all || (skip_first && i == 0) {
                        continue;
                    }
                    go(a, out);
                }
            }
            Node::List(v) => v.iter().for_each(|x| go(x, out)),
            Node::Neg(x) | Node::Paren(x) => go(x, out),
            Node::Bin(_, a, b) => {
                go(a, out);
                go(b, out);
            }
            _ => {}
        }
    }
    go(n, &mut out);
    out.sort();
    out.dedup();
    out
}

// ── standalone expression analysis (check_expression tool) ───────────────────

#[derive(Debug, Serialize)]
pub struct ExprAnalysis {
    pub text: String,
    pub canonical: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub references: Vec<ExprRef>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize)]
pub struct ExprRef {
    pub name: String,
    pub resolves_to: String,
}

pub fn analyze_expression(doc: &Document, e: &Expr) -> ExprAnalysis {
    let env = Env::new(doc);
    let mut t = Typer::new(&env, "expression", Span::default());
    let ty = if is_assertion_shape(&e.node)
        && matches!(e.node, Node::Bin(BinOp::Assign | BinOp::TermAssign, ..))
    {
        assertion(&mut t, &e.node, &AssertMode::Postcondition);
        Ty::Truth
    } else {
        t.ty(&e.node, Some(&e.text))
    };
    let mut refs: Vec<ExprRef> = Vec::new();
    for (name, res) in t.refs {
        if !refs.iter().any(|r| r.name == name) {
            refs.push(ExprRef {
                name,
                resolves_to: res,
            });
        }
    }
    let mut diagnostics = t.diags;
    for d in diagnostics.iter_mut() {
        d.line = None;
        d.col = None;
        d.component = None;
    }
    ExprAnalysis {
        text: e.text.clone(),
        canonical: canon(&e.node),
        ty: ty.to_string(),
        references: refs,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn codes(src: &str) -> Vec<String> {
        let out = parse(src);
        lint(
            &out.doc,
            &out.diagnostics,
            &LintOptions {
                include_info: false,
            },
        )
        .into_iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
    }

    const HEAD: &str = "engine_version :: '1.9';\nmetadata :: 'm'; title :: 'T'; end metadata.\nmetaprops :: '';\n";

    #[test]
    fn clean_guideline_has_no_findings() {
        let src = format!("{}plan :: main; component :: ask; component :: d; schedule_constraint :: completed(ask); end plan.\n\
            enquiry :: ask; source :: age; end enquiry.\n\
            decision :: d; candidate :: a; argument :: for, age > 18; recommendation :: netsupport(d, a) >= 1; end decision.\n\
            data :: age; type :: integer; end data.", HEAD);
        let c = codes(&src);
        assert!(c.is_empty(), "{:#?}", c);
    }

    #[test]
    fn engine_rules() {
        let src = format!("{}plan :: main; component :: x; component :: a; schedule_constraint :: completed(zz); component :: d; end plan.\n\
            action :: a; precondition :: age + 1; end action.\n\
            decision :: d; candidate :: c1; argument :: 2, age > 1; candidate :: c2; recommendation :: netsupport(d, nope) > 0; end decision.\n\
            action :: orphan; end action.\n\
            data :: age; type :: integer; default_value :: \"x\"; end data.\n\
            data :: dy; type :: integer; dynamic :: yes; end data.", HEAD);
        let c = codes(&src).join("\n");
        for code in ["PF-R001", "PF-R002", "PF-PP001", "PF-G004"] {
            assert!(c.contains(code), "missing {}:\n{}", code, c);
        }
        // verifier is skipped when resolution fails
        assert!(!c.contains("PF-V011"), "{}", c);
    }

    #[test]
    fn verifier_rules() {
        let src = format!("{}plan :: main; component :: a; component :: d; component :: e; end plan.\n\
            action :: a; precondition :: age + 1; postcondition :: zz = 1; end action.\n\
            decision :: d; choice_mode :: multiple; candidate :: c1; argument :: 2, age > 1; candidate :: c2; recommendation :: netsupport(d, nope) > 0; end decision.\n\
            enquiry :: e; precondition :: result_of(d) = c1 and foo(1) and result_set(d) includes c9; end enquiry.\n\
            data :: age; type :: integer; default_value :: \"x\"; end data.", HEAD);
        let c = codes(&src).join("\n");
        for code in [
            "PF-V011 precondition should be of type truth_value but is actually of type integer",
            "PF-V021 Can't find Data Item on left hand side of 'zz' = 1",
            "PF-V006",
            "PF-V005 The Candidate, c1  has no recommendation rule",
            "PF-V064 Task 'd'does not contain the requested candidate",
            "PF-V052",
            "PF-V030 Unrecognised function: foo",
            "PF-V004 The Enquiry has no sources",
            "PF-V011 Default Value should be of type integer but is actually of type text",
            "PF-X061 'c9' is compared with the result of decision d",
        ] {
            assert!(c.contains(code), "missing {}:\n{}", code, c);
        }
    }

    #[test]
    fn extras_detect_cycles_and_recursion() {
        let src = format!("{}plan :: main; component :: a; schedule_constraint :: completed(b); component :: b; schedule_constraint :: completed(a); component :: p; end plan.\n\
            action :: a; end action. action :: b; end action.\n\
            plan :: p; component :: p; end plan.", HEAD);
        let c = codes(&src).join("\n");
        assert!(c.contains("PF-X101"), "{}", c);
        assert!(c.contains("PF-G005"), "{}", c);
    }

    #[test]
    fn typing() {
        let doc = parse(&format!("{}plan :: main; end plan.\ndata :: d; type :: date; end data.\ndata :: r; type :: real; end data.", HEAD)).doc;
        let ty =
            |s: &str| analyze_expression(&doc, &crate::parser::parse_expression(s).unwrap()).ty;
        assert_eq!(ty("1 + 2"), "integer");
        assert_eq!(ty("1 + 2.5"), "real");
        assert_eq!(ty("r / 2"), "real");
        assert_eq!(ty("date_diff_days(d, now())"), "integer");
        assert_eq!(ty("d < 2020-01-01"), "untypeable");
        assert_eq!(ty("[1, 2.5]"), "setof_real");
        assert_eq!(ty("x # 1"), "text");
    }
}
