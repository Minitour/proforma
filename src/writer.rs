//! Serializer producing the exact layout written by Deontics Composer
//! (`PF1Parser.writeGuideline`, engine version 1.9): CRLF line endings,
//! 5-space indentation, fixed attribute order, Composer's spacing quirks and
//! its ordering of top-level components.
//!
//! Deliberate deviations from Composer (each avoids data loss or an output
//! Composer itself cannot re-read; `lint` reports the affected constructs):
//! - task definitions that no component uses are kept (Composer drops them);
//! - names containing `'` or `\` are escaped (Composer writes them raw);
//! - textual warning flags that are not identifiers are quoted;
//! - argument metaprops are kept even without a name/caption/description;
//! - 1.8-only source attributes (default_value, dynamic483, derivation483) are kept.

use crate::ast::*;
use crate::lexer::is_bare_atom;

const NL: &str = "\r\n";
const INDENT: &str = "     ";

struct Out {
    s: String,
}

impl Out {
    fn line(&mut self, level: usize, text: &str) {
        for _ in 0..level {
            self.s.push_str(INDENT);
        }
        self.s.push_str(text);
        self.s.push_str(NL);
    }

    fn blank(&mut self) {
        self.s.push_str(NL);
    }
}

/// Single-quoted atom with Composer's escaping (`\` -> `\\`, `'` -> `\'`).
pub fn atom(v: &str) -> String {
    let mut o = String::with_capacity(v.len() + 2);
    o.push('\'');
    for ch in v.chars() {
        match ch {
            '\\' => o.push_str("\\\\"),
            '\'' => o.push_str("\\'"),
            _ => o.push(ch),
        }
    }
    o.push('\'');
    o
}

fn atom_prop(out: &mut Out, level: usize, kw: &str, v: &str) {
    out.line(level, &format!("{} :: {} ;", kw, atom(v)));
}

fn expr_prop(out: &mut Out, level: usize, kw: &str, e: &Expr) {
    out.line(level, &format!("{} ::{};", kw, e.text.trim()));
}

fn yes(out: &mut Out, level: usize, kw: &str) {
    out.line(level, &format!("{} :: yes ;", kw));
}

fn header(out: &mut Out, kw: &str, name: &str) {
    out.line(0, &format!("{}  :: {} ;", kw, atom(name)));
}

fn generic(out: &mut Out, level: usize, attrs: &[Attr]) {
    for kw in ["caption", "description", "metaprops"] {
        if let Some(v) = attrs.iter().find_map(|a| match (a, kw) {
            (Attr::Caption(s), "caption")
            | (Attr::Description(s), "description")
            | (Attr::Metaprops(s), "metaprops") => Some(s),
            _ => None,
        }) {
            if kw == "metaprops" && v.is_empty() {
                continue;
            }
            atom_prop(out, level, kw, v);
        }
    }
}

/// First attribute with the given keyword (the engine keeps the first of duplicates).
fn first<'a>(attrs: &'a [Attr], kw: &str) -> Option<&'a Attr> {
    attrs.iter().find(|a| a.keyword() == kw)
}

fn first_expr<'a>(attrs: &'a [Attr], kw: &str) -> Option<&'a Expr> {
    first(attrs, kw).and_then(|a| a.exprs().into_iter().next())
}

fn first_str<'a>(attrs: &'a [Attr], kw: &str) -> Option<&'a str> {
    first(attrs, kw).and_then(|a| match a {
        Attr::Trigger(s)
        | Attr::Context(s)
        | Attr::Procedure(s)
        | Attr::Group(s)
        | Attr::Type(s)
        | Attr::TaskDefinition(s)
        | Attr::DataDefinition(s)
        | Attr::ChoiceMode(s)
        | Attr::SupportMode(s) => Some(s.as_str()),
        _ => None,
    })
}

fn flag(attrs: &[Attr], kw: &str) -> Option<bool> {
    first(attrs, kw).and_then(|a| match a {
        Attr::Autonomous(b)
        | Attr::Optional(b)
        | Attr::Terminal(b)
        | Attr::Dynamic(b)
        | Attr::Auto(b)
        | Attr::Mandatory(b)
        | Attr::Dynamic483(b)
        | Attr::PreselectCandidates(b) => Some(*b),
        _ => None,
    })
}

// ── document ─────────────────────────────────────────────────────────────────

/// Serialize a whole guideline as engine version 1.9.
pub fn write(doc: &Document) -> String {
    let mut out = Out { s: String::new() };
    out.line(0, "/** PROforma version 1.9 (plain text) **/");
    out.blank();
    out.line(
        0,
        "/** IMPORTANT: Keep all your comments below the engine_version line **/",
    );
    out.line(0, "engine_version :: '1.9' ;");
    out.blank();

    let root_name = doc.root().map(|r| r.name.as_str()).unwrap_or("guideline");
    let (md_name, fields) = match &doc.metadata {
        Some(m) => (m.name.as_str(), m.fields.as_slice()),
        None => (root_name, &[][..]),
    };
    header(&mut out, "metadata", md_name);
    for f in [
        MetaField::Title,
        MetaField::Desc,
        MetaField::Version,
        MetaField::Author,
        MetaField::Cdate,
        MetaField::Mdate,
    ] {
        if let Some((_, v)) = fields.iter().find(|(k, _)| *k == f) {
            atom_prop(&mut out, 1, f.as_str(), v);
        }
    }
    out.line(0, "end metadata.");
    out.blank();
    out.line(
        0,
        &format!(
            "metaprops :: {} ;",
            atom(doc.metaprops.as_deref().unwrap_or(""))
        ),
    );
    out.blank();

    let Some(root) = doc.root() else {
        return out.s;
    };
    write_task(&mut out, root);

    for t in task_order(doc) {
        out.blank();
        write_task(&mut out, t);
    }

    let mut data: Vec<&Item> = doc
        .items
        .iter()
        .skip(1)
        .filter(|i| i.kind == ItemKind::Data)
        .collect();
    // non-dynamic items first, then dynamic ones (stable)
    data.sort_by_key(|d| flag(&d.attrs, "dynamic").unwrap_or(false));
    if !data.is_empty() {
        out.blank();
    }
    for d in data {
        out.blank();
        write_data(&mut out, d);
    }

    let funcs: Vec<&Item> = doc
        .items
        .iter()
        .skip(1)
        .filter(|i| i.kind == ItemKind::Function)
        .collect();
    if !funcs.is_empty() {
        out.blank();
    }
    for f in funcs {
        out.blank();
        write_function(&mut out, f);
    }
    out.s
}

/// Composer's save order for task definitions: depth-first post-order over the
/// plan tree starting at the root (components in plan order, first occurrence
/// wins), then definitions no plan uses, in file order.
pub fn task_order(doc: &Document) -> Vec<&Item> {
    let mut seen = std::collections::HashSet::new();
    let mut order = Vec::new();
    if let Some(root) = doc.root() {
        seen.insert(root.name.as_str());
        visit(doc, root, &mut seen, &mut order);
    }
    for i in doc.items.iter().skip(1) {
        if i.kind.is_task() && seen.insert(i.name.as_str()) {
            order.push(i);
        }
    }
    order
}

fn visit<'a>(
    doc: &'a Document,
    plan: &'a Item,
    seen: &mut std::collections::HashSet<&'a str>,
    order: &mut Vec<&'a Item>,
) {
    for c in plan.components() {
        let def = c.task_definition();
        if let Some(t) = doc
            .items
            .iter()
            .skip(1)
            .find(|i| i.name == def && i.kind.is_task())
        {
            if seen.insert(t.name.as_str()) {
                if t.kind == ItemKind::Plan {
                    visit(doc, t, seen, order);
                }
                order.push(t);
            }
        }
    }
}

/// Serialize a single top-level component (for display).
pub fn write_item(item: &Item, _doc: &Document) -> String {
    let mut out = Out { s: String::new() };
    match item.kind {
        ItemKind::Data => write_data(&mut out, item),
        ItemKind::Function => write_function(&mut out, item),
        _ => write_task(&mut out, item),
    }
    out.s
}

/// Serialize a nested attribute (component, candidate, source, ...) for display.
pub fn write_attr(attr: &Attr, level: usize) -> String {
    let mut out = Out { s: String::new() };
    match attr {
        Attr::Component(b) => write_component(&mut out, b),
        Attr::Candidate(b) => write_candidate(&mut out, b),
        Attr::Source(b) => write_source(&mut out, b),
        Attr::Argument(a) => write_argument(&mut out, a, false),
        Attr::WarningCondition(w) => write_warning(&mut out, w),
        other => {
            let mut tmp = Out { s: String::new() };
            write_simple(&mut tmp, level, other);
            return tmp.s;
        }
    }
    out.s
}

/// Fallback for single attributes in display contexts.
fn write_simple(out: &mut Out, level: usize, a: &Attr) {
    match a {
        Attr::Caption(s)
        | Attr::Description(s)
        | Attr::Metaprops(s)
        | Attr::Trigger(s)
        | Attr::Context(s)
        | Attr::Procedure(s)
        | Attr::Group(s)
        | Attr::TaskDefinition(s)
        | Attr::DataDefinition(s) => atom_prop(out, level, a.keyword(), s),
        Attr::Type(t) => out.line(level, &format!("type :: {} ;", t)),
        Attr::ChoiceMode(s) | Attr::SupportMode(s) => {
            out.line(level, &format!("{} :: {} ;", a.keyword(), s))
        }
        Attr::Priority(n) => out.line(level, &format!("priority :: {} ;", n)),
        Attr::Ltwh(v) => out.line(
            level,
            &format!("ltwh :: {},{},{},{};", v[0], v[1], v[2], v[3]),
        ),
        Attr::ScheduleConstraint(s) => out.line(
            level,
            &format!("schedule_constraint :: completed({}) ;", atom(s)),
        ),
        Attr::CycleRepeat(e, u) => out.line(
            level,
            &format!("cycle_repeat :: {} {} ;", e.text.trim(), u.as_str()),
        ),
        Attr::Range(v) => out.line(level, &format!("range ::{};", join_exprs(v))),
        Attr::TrueValue(c) | Attr::FalseValue(c) | Attr::Unit(c) => {
            atom_prop(out, level, a.keyword(), c.value())
        }
        Attr::Autonomous(b)
        | Attr::Optional(b)
        | Attr::Terminal(b)
        | Attr::Dynamic(b)
        | Attr::Auto(b)
        | Attr::Mandatory(b)
        | Attr::Dynamic483(b)
        | Attr::PreselectCandidates(b) => out.line(
            level,
            &format!("{} :: {} ;", a.keyword(), if *b { "yes" } else { "no" }),
        ),
        Attr::Parameters(ps) => write_parameters(out, ps),
        _ => {
            if let Some(e) = a.exprs().into_iter().next() {
                expr_prop(out, level, a.keyword(), e);
            }
        }
    }
}

fn join_exprs(v: &[Expr]) -> String {
    v.iter()
        .map(|e| e.text.trim_start().to_string())
        .collect::<Vec<_>>()
        .join(",")
}

// ── tasks ────────────────────────────────────────────────────────────────────

fn write_task(out: &mut Out, t: &Item) {
    header(out, t.kind.keyword(), &t.name);
    let a = &t.attrs;
    generic(out, 1, a);
    if let Some(Attr::Parameters(ps)) = first(a, "parameters") {
        write_parameters(out, ps);
    }
    if let Some(v) = first_str(a, "context") {
        atom_prop(out, 1, "context", v);
    }
    if let Some(e) = first_expr(a, "precondition") {
        expr_prop(out, 1, "precondition", e);
    }
    if let Some(e) = first_expr(a, "goal") {
        expr_prop(out, 1, "goal", e);
    }
    if let Some(v) = first_str(a, "trigger") {
        atom_prop(out, 1, "trigger", v);
    }
    if let Some(e) = first_expr(a, "postcondition") {
        expr_prop(out, 1, "postcondition", e);
    }
    if let Some(e) = first_expr(a, "wait_condition") {
        expr_prop(out, 1, "wait_condition", e);
    }

    match t.kind {
        ItemKind::Plan => {
            if let Some(e) = first_expr(a, "abort") {
                expr_prop(out, 1, "abort", e);
            }
            if let Some(e) = first_expr(a, "terminate") {
                expr_prop(out, 1, "terminate", e);
            }
            for c in t.components() {
                write_component(out, c);
            }
        }
        ItemKind::Action => {
            if let Some(v) = first_str(a, "procedure") {
                atom_prop(out, 1, "procedure", v);
            }
        }
        ItemKind::Enquiry => write_sources(out, t),
        ItemKind::Decision => {
            write_sources(out, t);
            if first_str(a, "choice_mode") == Some("multiple") {
                out.line(1, "choice_mode :: multiple ;");
            }
            if first_str(a, "support_mode") == Some("numeric") {
                out.line(1, "support_mode :: numeric ;");
            }
            if flag(a, "preselect_candidates") == Some(true) {
                // Composer writes this decision attribute at level 2
                yes(out, 2, "preselect_candidates");
            }
            for c in t.candidates() {
                write_candidate(out, c);
            }
        }
        _ => {}
    }
    out.line(0, &format!("end {}.", t.kind.keyword()));
}

fn write_parameters(out: &mut Out, ps: &[Parameter]) {
    out.line(1, "parameters :: ");
    for (i, p) in ps.iter().enumerate() {
        let last = i + 1 == ps.len();
        let sep = if last { ";" } else { "," };
        let has_attrs = p.data_type.is_some() || p.attrs.iter().any(|a| a.is_generic());
        if has_attrs {
            out.line(2, &atom(&p.name));
            out.line(2, "attributes");
            if let Some(t) = &p.data_type {
                out.line(3, &format!("type :: {} ;", t));
            }
            generic(out, 3, &p.attrs);
            out.line(2, &format!("end attributes{}", sep));
        } else {
            out.line(2, &format!("{}{}", atom(&p.name), sep));
        }
    }
}

fn write_component(out: &mut Out, c: &Block) {
    out.line(1, &format!("component :: {} ;", atom(&c.name)));
    let a = &c.attrs;
    generic(out, 2, a);
    atom_prop(out, 2, "task_definition", c.task_definition());
    for kw in ["autonomous", "optional", "terminal"] {
        if flag(a, kw) == Some(true) {
            yes(out, 2, kw);
        }
    }
    for x in a {
        if let Attr::ParamValue(e) = x {
            expr_prop(out, 2, "param_value", e);
        }
    }
    for s in c.schedule_constraints() {
        out.line(
            2,
            &format!("schedule_constraint :: completed({}) ;", atom(s)),
        );
    }
    if let Some(e) = first_expr(a, "number_of_cycles") {
        expr_prop(out, 2, "number_of_cycles", e);
    }
    if let Some(e) = first_expr(a, "cycle_until") {
        expr_prop(out, 2, "cycle_until", e);
    }
    if let Some(Attr::CycleRepeat(e, u)) = first(a, "cycle_repeat") {
        out.line(
            2,
            &format!("cycle_repeat :: {} {} ;", e.text.trim(), u.as_str()),
        );
    }
    if let Some(Attr::Ltwh(v)) = first(a, "ltwh") {
        out.line(2, &format!("ltwh :: {},{},{},{};", v[0], v[1], v[2], v[3]));
    }
}

fn write_sources(out: &mut Out, t: &Item) {
    let mut sources: Vec<&Block> = t.sources().collect();
    sources.sort_by_key(|s| s.data_definition().to_lowercase());
    for s in sources {
        write_source(out, s);
    }
}

fn write_source(out: &mut Out, s: &Block) {
    out.line(1, &format!("source :: {} ;", atom(&s.name)));
    let a = &s.attrs;
    generic(out, 2, a);
    atom_prop(out, 2, "data_definition", s.data_definition());
    if let Some(e) = first_expr(a, "default_value") {
        expr_prop(out, 2, "default_value", e);
    }
    if flag(a, "mandatory") == Some(false) {
        out.line(2, "mandatory :: no ;");
    }
    if flag(a, "dynamic483") == Some(true) {
        yes(out, 2, "dynamic483");
    }
    if let Some(e) = first_expr(a, "derivation483") {
        expr_prop(out, 2, "derivation483", e);
    }
}

fn write_candidate(out: &mut Out, c: &Block) {
    out.line(1, &format!("candidate :: {} ;", atom(&c.name)));
    let a = &c.attrs;
    generic(out, 2, a);
    if let Some(Attr::Priority(p)) = first(a, "priority") {
        if *p != 0 {
            out.line(2, &format!("priority :: {} ;", p));
        }
    }
    if let Some(g) = first_str(a, "group") {
        atom_prop(out, 2, "group", g);
    }
    for arg in c.arguments() {
        write_argument(out, arg, true);
    }
    if let Some(e) = first_expr(a, "recommendation") {
        expr_prop(out, 2, "recommendation", e);
    }
}

fn write_argument(out: &mut Out, arg: &Argument, _in_candidate: bool) {
    let support = arg.support.label();
    let head = format!(
        "argument :: {},{}",
        support.trim(),
        arg.condition.text.trim()
    );
    let has_block = arg.name.is_some() || !arg.attrs.is_empty();
    if !has_block {
        out.line(2, &format!("{};", head));
        return;
    }
    out.line(2, &format!("{} attributes", head));
    if let Some(n) = &arg.name {
        atom_prop(out, 3, "argument_name", n);
    }
    generic(out, 3, &arg.attrs);
    out.line(2, "end attributes");
    out.line(2, ";");
}

// ── data items and functions ─────────────────────────────────────────────────

fn write_data(out: &mut Out, d: &Item) {
    header(out, "data", &d.name);
    let a = &d.attrs;
    out.line(1, &format!("type :: {} ;", d.data_type().unwrap_or("text")));
    generic(out, 1, a);
    for kw in ["true_value", "false_value"] {
        if let Some(Attr::TrueValue(c) | Attr::FalseValue(c)) = first(a, kw) {
            atom_prop(out, 1, kw, c.value());
        }
    }
    if let Some(Attr::Range(v)) = first(a, "range") {
        if !v.is_empty() {
            out.line(1, &format!("range ::{};", join_exprs(v)));
        }
    }
    for kw in ["default_value", "mandatory_validation", "derivation"] {
        if let Some(e) = first_expr(a, kw) {
            expr_prop(out, 1, kw, e);
        }
    }
    if flag(a, "dynamic") == Some(true) {
        yes(out, 1, "dynamic");
    }
    for x in a {
        if let Attr::WarningCondition(w) = x {
            write_warning(out, w);
        }
    }
    if let Some(Attr::Unit(c)) = first(a, "unit") {
        atom_prop(out, 1, "unit", c.value());
    }
    out.line(0, "end data.");
}

fn write_warning(out: &mut Out, w: &Warning) {
    let flag = match &w.flag {
        Constant::Number(n) => n.clone(),
        Constant::Atom(s) | Constant::String(s) => {
            if is_bare_atom(s) {
                s.clone()
            } else {
                atom(s)
            }
        }
    };
    out.line(
        2,
        &format!(
            "warning_condition :: {},{} ;",
            flag,
            w.condition.text.trim()
        ),
    );
    generic(out, 3, &w.attrs);
}

fn write_function(out: &mut Out, f: &Item) {
    header(out, "function", &f.name);
    let a = &f.attrs;
    generic(out, 1, a);
    out.line(1, &format!("type :: {} ;", f.data_type().unwrap_or("text")));
    if flag(a, "auto") == Some(true) {
        yes(out, 1, "auto");
    }
    if let Some(e) = first_expr(a, "function_trigger") {
        expr_prop(out, 1, "function_trigger", e);
    }
    if let Some(e) = first_expr(a, "function_body") {
        expr_prop(out, 1, "function_body", e);
    }
    out.line(0, "end function.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn empty_guideline_matches_composer() {
        let doc = Document {
            engine_version: Some("1.9".into()),
            metadata: Some(Metadata {
                name: "MyGuideline".into(),
                fields: vec![],
            }),
            metaprops: Some(String::new()),
            items: vec![Item {
                kind: ItemKind::Plan,
                name: "MyGuideline".into(),
                attrs: vec![],
                span: Default::default(),
            }],
            directives: None,
        };
        let expected = "/** PROforma version 1.9 (plain text) **/\r\n\r\n/** IMPORTANT: Keep all your comments below the engine_version line **/\r\nengine_version :: '1.9' ;\r\n\r\nmetadata  :: 'MyGuideline' ;\r\nend metadata.\r\n\r\nmetaprops :: '' ;\r\n\r\nplan  :: 'MyGuideline' ;\r\nend plan.\r\n";
        assert_eq!(write(&doc), expected);
    }

    #[test]
    fn round_trip_is_fixed_point() {
        let src = "engine_version :: '1.9';\nmetadata :: m; title :: 'T'; end metadata.\nmetaprops :: '';\n\
            plan :: p; caption :: 'It''s'; component :: a; ltwh :: 1,2,3,4; component :: b; schedule_constraint :: completed(a); end plan.\n\
            decision :: a; candidate :: x; argument :: for, v > 1 attributes argument_name :: n; end attributes; recommendation :: netsupport() > 0; end decision.\n\
            enquiry :: b; source :: zed; source :: Alpha; mandatory :: no; end enquiry.\n\
            data :: v; type :: integer; range :: 1 , 2; warning_condition :: 1, v > 5; caption :: 'W'; end data.\n\
            data :: zed; type :: text; end data. data :: 'Alpha'; type :: text; end data.";
        let out = parse(src);
        assert!(!out.has_errors(), "{:?}", out.diagnostics);
        let w1 = write(&out.doc);
        let again = parse(&w1);
        assert!(!again.has_errors(), "{:?}\n{}", again.diagnostics, w1);
        let w2 = write(&again.doc);
        assert_eq!(w1, w2);
        assert!(w1.contains("caption :: 'It\\'s' ;"));
        assert!(w1.find("source :: 'Alpha'").unwrap() < w1.find("source :: 'zed'").unwrap());
        assert!(w1.contains("range ::1 ,2;"), "{}", w1);
    }
}
