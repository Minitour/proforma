//! In-memory editing of a parsed guideline.
//!
//! Targets are addressed with selectors:
//!
//! ```text
//! Item                                   top-level task / data item / function
//! Item/component:Name                    component (task instance) inside a plan
//! Item/candidate:Name                    candidate inside a decision
//! Item/source:Name                       source inside an enquiry or decision
//! Item/candidate:Name/argument:N         argument by argument_name or 1-based index
//! Item/warning_condition:N               warning condition of a data item (1-based)
//! ```

use serde::Serialize;

use crate::ast::*;
use crate::lexer::{is_bare_atom, tokenize, Tok};
use crate::parser::{parse_assertion, parse_attrs, parse_expression, SnippetContext};

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

pub type EditResult<T> = Result<T, EditError>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Seg {
    Component(String),
    Candidate(String),
    Source(String),
    Argument(String),
    Warning(String),
}

fn parse_selector(sel: &str) -> EditResult<(String, Vec<Seg>)> {
    let mut parts = sel.split('/');
    let item = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| EditError::Invalid("empty selector".into()))?
        .to_string();
    let mut segs = Vec::new();
    for p in parts {
        let (kind, name) = p.split_once(':').ok_or_else(|| {
            EditError::Invalid(format!(
                "selector segment '{}' must look like component:<name>, candidate:<name>, source:<name>, argument:<name|index> or warning_condition:<index>",
                p
            ))
        })?;
        let name = name.to_string();
        segs.push(match kind {
            "component" => Seg::Component(name),
            "candidate" => Seg::Candidate(name),
            "source" => Seg::Source(name),
            "argument" => Seg::Argument(name),
            "warning_condition" | "warning" => Seg::Warning(name),
            other => {
                return Err(EditError::Invalid(format!(
                    "unknown selector segment kind '{}'",
                    other
                )))
            }
        });
    }
    Ok((item, segs))
}

/// Locate the index of a nested attribute within `attrs` for a selector segment.
fn find_child(attrs: &[Attr], seg: &Seg) -> Option<usize> {
    let mut n_arg = 0;
    let mut n_warn = 0;
    for (i, a) in attrs.iter().enumerate() {
        let hit = match (a, seg) {
            (Attr::Component(b), Seg::Component(n))
            | (Attr::Candidate(b), Seg::Candidate(n))
            | (Attr::Source(b), Seg::Source(n)) => &b.name == n,
            (Attr::Argument(x), Seg::Argument(n)) => {
                n_arg += 1;
                x.name.as_deref() == Some(n.as_str()) || n_arg.to_string() == *n
            }
            (Attr::WarningCondition(_), Seg::Warning(n)) => {
                n_warn += 1;
                n_warn.to_string() == *n
            }
            _ => false,
        };
        if hit {
            return Some(i);
        }
    }
    None
}

fn seg_label(s: &Seg) -> String {
    match s {
        Seg::Component(n) => format!("component '{}'", n),
        Seg::Candidate(n) => format!("candidate '{}'", n),
        Seg::Source(n) => format!("source '{}'", n),
        Seg::Argument(n) => format!("argument '{}'", n),
        Seg::Warning(n) => format!("warning_condition #{}", n),
    }
}

/// The attribute list addressed by a selector, plus the snippet context to use for it.
pub fn target_attrs<'a>(
    doc: &'a mut Document,
    selector: &str,
) -> EditResult<(&'a mut Vec<Attr>, SnippetContext)> {
    let (item_name, segs) = parse_selector(selector)?;
    let item = doc
        .find_mut(&item_name)
        .ok_or_else(|| EditError::NotFound(format!("no component named '{}'", item_name)))?;
    let mut ctx = SnippetContext::Item(item.kind);
    let mut attrs = &mut item.attrs;
    for seg in &segs {
        let idx = find_child(attrs, seg).ok_or_else(|| {
            EditError::NotFound(format!("{} not found in '{}'", seg_label(seg), selector))
        })?;
        ctx = match seg {
            Seg::Component(_) => SnippetContext::Component,
            Seg::Candidate(_) => SnippetContext::Candidate,
            Seg::Source(_) => SnippetContext::Source,
            Seg::Argument(_) | Seg::Warning(_) => SnippetContext::Generic,
        };
        attrs = attrs[idx]
            .children_mut()
            .expect("nested selector always points to a block");
    }
    Ok((attrs, ctx))
}

/// Read-only view of the attribute list addressed by a selector.
pub fn target_attrs_ref<'a>(doc: &'a Document, selector: &str) -> EditResult<&'a Vec<Attr>> {
    let (item_name, segs) = parse_selector(selector)?;
    let item = doc
        .find(&item_name)
        .ok_or_else(|| EditError::NotFound(format!("no component named '{}'", item_name)))?;
    let mut attrs = &item.attrs;
    for seg in &segs {
        let idx = find_child(attrs, seg).ok_or_else(|| {
            EditError::NotFound(format!("{} not found in '{}'", seg_label(seg), selector))
        })?;
        attrs = attrs[idx].children().unwrap();
    }
    Ok(attrs)
}

/// Nested attribute (component, candidate, ...) addressed by a selector with at least one segment.
pub fn target_child_ref<'a>(doc: &'a Document, selector: &str) -> EditResult<&'a Attr> {
    let (parent, last) = selector
        .rsplit_once('/')
        .ok_or_else(|| EditError::Invalid("selector does not address a nested element".into()))?;
    let attrs = target_attrs_ref(doc, parent)?;
    let (_, segs) = parse_selector(&format!("x/{}", last))?;
    let idx = find_child(attrs, &segs[0]).ok_or_else(|| {
        EditError::NotFound(format!("{} not found in '{}'", seg_label(&segs[0]), parent))
    })?;
    Ok(&attrs[idx])
}

#[derive(Debug, Default, Serialize)]
pub struct ChangeReport {
    pub changes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl ChangeReport {
    fn note(&mut self, s: impl Into<String>) {
        self.changes.push(s.into());
    }
}

// ── top-level components ─────────────────────────────────────────────────────

pub fn add_item(doc: &mut Document, item: Item) -> EditResult<ChangeReport> {
    if doc.find(&item.name).is_some() {
        return Err(EditError::Invalid(format!(
            "a component named '{}' already exists (names are shared by tasks, data items and functions)",
            item.name
        )));
    }
    let mut r = ChangeReport::default();
    r.note(format!("added {} '{}'", item.kind.keyword(), item.name));
    doc.items.push(item);
    Ok(r)
}

/// Replace an existing top-level component with a new definition (same name or renamed).
pub fn replace_item(doc: &mut Document, name: &str, item: Item) -> EditResult<ChangeReport> {
    let idx = doc
        .index_of(name)
        .ok_or_else(|| EditError::NotFound(format!("no component named '{}'", name)))?;
    if item.name != name && doc.find(&item.name).is_some() {
        return Err(EditError::Invalid(format!(
            "a component named '{}' already exists",
            item.name
        )));
    }
    if idx == 0 && item.kind != ItemKind::Plan {
        return Err(EditError::Invalid(
            "the root plan can only be replaced by a plan".into(),
        ));
    }
    let mut r = ChangeReport::default();
    r.note(format!("replaced {} '{}'", item.kind.keyword(), name));
    doc.items[idx] = item;
    Ok(r)
}

/// Remove a top-level component. With `cascade`, plan components that use it,
/// schedule constraints on those components and sources reading it are removed too.
pub fn remove_item(doc: &mut Document, name: &str, cascade: bool) -> EditResult<ChangeReport> {
    let idx = doc
        .index_of(name)
        .ok_or_else(|| EditError::NotFound(format!("no component named '{}'", name)))?;
    if idx == 0 {
        return Err(EditError::Invalid(
            "cannot remove the root plan; replace it instead".into(),
        ));
    }
    let removed = doc.items.remove(idx);
    let mut r = ChangeReport::default();
    r.note(format!("removed {} '{}'", removed.kind.keyword(), name));

    if cascade {
        for item in doc.items.iter_mut() {
            if item.kind == ItemKind::Plan && removed.kind.is_task() {
                let dropped: Vec<String> = item
                    .components()
                    .filter(|c| c.task_definition() == name)
                    .map(|c| c.name.clone())
                    .collect();
                if !dropped.is_empty() {
                    item.attrs
                        .retain(|a| !matches!(a, Attr::Component(c) if dropped.contains(&c.name)));
                    for a in item.attrs.iter_mut() {
                        if let Attr::Component(c) = a {
                            let before = c.attrs.len();
                            c.attrs.retain(|x| {
                                !matches!(x, Attr::ScheduleConstraint(s) if dropped.contains(s))
                            });
                            if c.attrs.len() != before {
                                r.note(format!(
                                    "removed schedule constraint(s) on '{}' in plan '{}'",
                                    c.name, item.name
                                ));
                            }
                        }
                    }
                    r.note(format!(
                        "removed component(s) {} from plan '{}'",
                        dropped.join(", "),
                        item.name
                    ));
                }
            }
            if removed.kind == ItemKind::Data {
                let before = item.attrs.len();
                item.attrs
                    .retain(|a| !matches!(a, Attr::Source(s) if s.data_definition() == name));
                if item.attrs.len() != before {
                    r.note(format!("removed source '{}' from '{}'", name, item.name));
                }
            }
        }
    }

    let remaining = crate::model::references(doc, name);
    for rf in remaining {
        r.warnings.push(format!(
            "'{}' is still referenced at {} ({}: {})",
            name, rf.location, rf.attribute, rf.context
        ));
    }
    Ok(r)
}

/// Rename a top-level component and update every reference to it.
pub fn rename_item(doc: &mut Document, old: &str, new: &str) -> EditResult<ChangeReport> {
    if old == new {
        return Ok(ChangeReport::default());
    }
    validate_name(new)?;
    if doc.find(new).is_some() {
        return Err(EditError::Invalid(format!(
            "a component named '{}' already exists",
            new
        )));
    }
    let item = doc
        .find_mut(old)
        .ok_or_else(|| EditError::NotFound(format!("no component named '{}'", old)))?;
    item.name = new.to_string();
    let kind = item.kind;
    let mut r = ChangeReport::default();
    r.note(format!("renamed {} '{}' to '{}'", kind.keyword(), old, new));

    for item in doc.items.iter_mut() {
        let owner = item.name.clone();
        rename_in_attrs(&mut item.attrs, old, new, kind, &owner, &mut r);
    }
    Ok(r)
}

fn rename_in_attrs(
    attrs: &mut [Attr],
    old: &str,
    new: &str,
    kind: ItemKind,
    path: &str,
    r: &mut ChangeReport,
) {
    for a in attrs.iter_mut() {
        match a {
            Attr::Component(c) if kind.is_task() => {
                let explicit = c.attrs.iter_mut().find_map(|x| match x {
                    Attr::TaskDefinition(t) => Some(t),
                    _ => None,
                });
                match explicit {
                    Some(t) if t == old => {
                        *t = new.to_string();
                        r.note(format!(
                            "{}: component '{}' now uses '{}'",
                            path, c.name, new
                        ));
                    }
                    None if c.name == old => {
                        // implicit task_definition == component name: keep the
                        // instance name stable and make the definition explicit
                        c.attrs.insert(0, Attr::TaskDefinition(new.to_string()));
                        r.note(format!(
                            "{}: component '{}' now uses '{}'",
                            path, c.name, new
                        ));
                    }
                    _ => {}
                }
            }
            Attr::Source(s) if kind == ItemKind::Data => {
                let explicit = s.attrs.iter_mut().find_map(|x| match x {
                    Attr::DataDefinition(t) => Some(t),
                    _ => None,
                });
                match explicit {
                    Some(t) if t == old => {
                        *t = new.to_string();
                        if s.name == old {
                            s.name = new.to_string();
                        }
                        r.note(format!("{}: source now reads '{}'", path, new));
                    }
                    None if s.name == old => {
                        s.name = new.to_string();
                        r.note(format!("{}: source now reads '{}'", path, new));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        for e in a.exprs_mut() {
            if let Some(text) = rename_in_text(&e.text, old, new) {
                let reparsed =
                    if matches!(e.node, Node::Bin(BinOp::And, ..)) || a_is_assertion(&e.text) {
                        parse_assertion(&text).or_else(|_| parse_expression(&text))
                    } else {
                        parse_expression(&text).or_else(|_| parse_assertion(&text))
                    };
                if let Ok(ne) = reparsed {
                    r.note(format!("{}: '{}' -> '{}'", path, e.text, text));
                    e.node = ne.node;
                    e.text = text;
                }
            }
        }
        if let Some(children) = a.children_mut() {
            rename_in_attrs(children, old, new, kind, path, r);
        }
    }
}

fn a_is_assertion(text: &str) -> bool {
    text.contains(":=")
}

/// Rename a component instance within a plan, updating sibling schedule constraints.
pub fn rename_component(
    doc: &mut Document,
    plan: &str,
    old: &str,
    new: &str,
) -> EditResult<ChangeReport> {
    validate_name(new)?;
    let item = doc
        .find_mut(plan)
        .ok_or_else(|| EditError::NotFound(format!("no plan named '{}'", plan)))?;
    if item.components().any(|c| c.name == new) {
        return Err(EditError::Invalid(format!(
            "plan '{}' already has a component named '{}'",
            plan, new
        )));
    }
    let mut found = false;
    let mut r = ChangeReport::default();
    for a in item.attrs.iter_mut() {
        if let Attr::Component(c) = a {
            if c.name == old {
                if !c.attrs.iter().any(|x| matches!(x, Attr::TaskDefinition(_))) {
                    c.attrs.insert(0, Attr::TaskDefinition(old.to_string()));
                }
                c.name = new.to_string();
                found = true;
            }
            for x in c.attrs.iter_mut() {
                if let Attr::ScheduleConstraint(s) = x {
                    if s == old {
                        *s = new.to_string();
                        r.note(format!("updated schedule constraint on '{}'", c.name));
                    }
                }
            }
        }
    }
    if !found {
        return Err(EditError::NotFound(format!(
            "plan '{}' has no component '{}'",
            plan, old
        )));
    }
    r.changes.insert(
        0,
        format!(
            "renamed component '{}' to '{}' in plan '{}'",
            old, new, plan
        ),
    );
    Ok(r)
}

/// Replace atom tokens equal to `old` in expression text, preserving formatting.
pub fn rename_in_text(text: &str, old: &str, new: &str) -> Option<String> {
    let toks = tokenize(text).ok()?;
    let mut out = String::new();
    let mut last = 0;
    let mut changed = false;
    for t in &toks {
        if let Tok::Atom { value, .. } = &t.tok {
            if value == old {
                out.push_str(&text[last..t.span.start]);
                out.push_str(&quote_atom(new));
                last = t.span.end;
                changed = true;
            }
        }
    }
    if !changed {
        return None;
    }
    out.push_str(&text[last..]);
    Some(out)
}

/// Write an atom bare when possible, otherwise single-quoted with escapes.
pub fn quote_atom(s: &str) -> String {
    if is_bare_atom(s) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

pub fn validate_name(name: &str) -> EditResult<()> {
    if name.trim().is_empty() {
        return Err(EditError::Invalid("name must not be empty".into()));
    }
    if name.contains(['\n', '\r']) {
        return Err(EditError::Invalid(
            "name must not contain line breaks".into(),
        ));
    }
    Ok(())
}

// ── attributes and nested elements ───────────────────────────────────────────

/// Parse `snippet` (one or more attributes) in the context of `selector` and
/// append it. Non-repeatable attributes that already exist are replaced.
pub fn add_attrs(doc: &mut Document, selector: &str, snippet: &str) -> EditResult<ChangeReport> {
    let (attrs, ctx) = target_attrs(doc, selector)?;
    let new = parse_attrs(ctx, snippet)
        .map_err(|d| EditError::Invalid(format!("invalid attribute snippet: {}", d.message)))?;
    let mut r = ChangeReport::default();
    for a in new {
        let kw = a.keyword();
        if let Some(name) = child_name(&a) {
            if attrs
                .iter()
                .any(|x| x.keyword() == kw && child_name(x).as_deref() == Some(name.as_str()))
            {
                return Err(EditError::Invalid(format!(
                    "{} '{}' already exists in '{}'",
                    kw, name, selector
                )));
            }
        }
        if a.is_repeatable() {
            r.note(format!("added {} to '{}'", kw, selector));
            attrs.push(a);
        } else if let Some(pos) = attrs.iter().position(|x| x.keyword() == kw) {
            r.note(format!("replaced {} of '{}'", kw, selector));
            attrs[pos] = a;
        } else {
            r.note(format!("set {} of '{}'", kw, selector));
            attrs.push(a);
        }
    }
    Ok(r)
}

fn child_name(a: &Attr) -> Option<String> {
    match a {
        Attr::Component(b) | Attr::Candidate(b) | Attr::Source(b) => Some(b.name.clone()),
        Attr::Argument(x) => x.name.clone(),
        _ => None,
    }
}

/// Set a single attribute: `value` is the PROforma text that follows `keyword ::`.
pub fn set_attr(
    doc: &mut Document,
    selector: &str,
    keyword: &str,
    value: &str,
) -> EditResult<ChangeReport> {
    let keyword = keyword.trim();
    let value = value.trim().trim_end_matches(';').trim_end();
    let value = match keyword {
        // free-text attributes: accept raw text and quote it
        "caption" | "description" | "metaprops" | "procedure" | "trigger" | "context" => {
            if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
                value.to_string()
            } else {
                quote_text(value)
            }
        }
        _ => value.to_string(),
    };
    let snippet = format!("{} :: {} ;", keyword, value);
    add_attrs(doc, selector, &snippet)
}

/// Remove attributes with the given keyword from the selected block.
pub fn remove_attr(doc: &mut Document, selector: &str, keyword: &str) -> EditResult<ChangeReport> {
    let (attrs, _) = target_attrs(doc, selector)?;
    let before = attrs.len();
    attrs.retain(|a| a.keyword() != keyword);
    let n = before - attrs.len();
    if n == 0 {
        return Err(EditError::NotFound(format!(
            "'{}' has no '{}' attribute",
            selector, keyword
        )));
    }
    let mut r = ChangeReport::default();
    r.note(format!(
        "removed {} '{}' attribute(s) from '{}'",
        n, keyword, selector
    ));
    Ok(r)
}

/// Remove a nested element (component, candidate, source, argument, warning) addressed by selector.
pub fn remove_child(doc: &mut Document, selector: &str) -> EditResult<ChangeReport> {
    let (parent, last) = selector.rsplit_once('/').ok_or_else(|| {
        EditError::Invalid(
            "selector must address a nested element, e.g. Plan/component:X (use remove_component for top-level components)".into(),
        )
    })?;
    let (_, segs) = parse_selector(&format!("x/{}", last))?;
    let seg = &segs[0];
    let (attrs, _) = target_attrs(doc, parent)?;
    let idx = find_child(attrs, seg).ok_or_else(|| {
        EditError::NotFound(format!("{} not found in '{}'", seg_label(seg), parent))
    })?;
    attrs.remove(idx);
    let mut r = ChangeReport::default();
    r.note(format!("removed {} from '{}'", seg_label(seg), parent));
    if let Seg::Component(name) = seg {
        for a in attrs.iter_mut() {
            if let Attr::Component(c) = a {
                let before = c.attrs.len();
                c.attrs
                    .retain(|x| !matches!(x, Attr::ScheduleConstraint(s) if s == name));
                if before != c.attrs.len() {
                    r.note(format!(
                        "removed schedule constraint completed({}) from '{}'",
                        name, c.name
                    ));
                }
            }
        }
    }
    Ok(r)
}

/// Quote free text as a PROforma atom.
pub fn quote_text(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn doc() -> Document {
        let src = "engine_version :: '1.9';\nmetaprops :: '';\nplan :: main; component :: ask; component :: choose; schedule_constraint :: completed(ask); end plan.\n\
                   enquiry :: ask; source :: age; end enquiry.\n\
                   decision :: choose; candidate :: a; argument :: for, age > 18; recommendation :: netsupport(choose, a) >= 1; end decision.\n\
                   data :: age; type :: integer; end data.";
        let out = parse(src);
        assert!(!out.has_errors(), "{:?}", out.diagnostics);
        out.doc
    }

    #[test]
    fn rename_updates_references() {
        let mut d = doc();
        let r = rename_item(&mut d, "age", "patient_age").unwrap();
        assert!(r.changes.len() >= 3, "{:?}", r);
        let refs = crate::model::references(&d, "patient_age");
        assert_eq!(refs.len(), 2, "{:?}", refs);
        assert!(crate::model::references(&d, "age").is_empty());
    }

    #[test]
    fn remove_cascades() {
        let mut d = doc();
        let r = remove_item(&mut d, "ask", true).unwrap();
        assert!(d.root().unwrap().components().all(|c| c.name != "ask"));
        assert!(d
            .root()
            .unwrap()
            .components()
            .all(|c| c.schedule_constraints().count() == 0));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn selectors_and_snippets() {
        let mut d = doc();
        add_attrs(&mut d, "choose/candidate:a", "caption :: 'Option A';").unwrap();
        set_attr(
            &mut d,
            "choose/candidate:a/argument:1",
            "caption",
            "It's adult",
        )
        .unwrap();
        let arg = target_child_ref(&d, "choose/candidate:a/argument:1").unwrap();
        match arg {
            Attr::Argument(a) => {
                assert!(matches!(&a.attrs[0], Attr::Caption(c) if c == "It's adult"))
            }
            _ => panic!(),
        }
        assert!(add_attrs(&mut d, "choose", "procedure :: x;").is_err());
        remove_child(&mut d, "main/component:ask").unwrap();
        assert_eq!(d.root().unwrap().components().count(), 1);
    }
}
