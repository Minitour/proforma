//! Read-only semantic queries over a parsed guideline: traversal, reference
//! lookup, plan graphs and human-readable outlines.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::ast::*;

/// Visit every attribute in the document (including attributes nested in
/// components, candidates, sources, arguments and warning conditions).
/// `path` is a selector such as `Choose/candidate:a/argument:1`.
pub fn walk_attrs<'a>(doc: &'a Document, f: &mut dyn FnMut(&str, &'a Item, &'a Attr)) {
    for item in &doc.items {
        walk_list(&item.name, item, &item.attrs, f);
    }
}

fn walk_list<'a>(
    path: &str,
    item: &'a Item,
    attrs: &'a [Attr],
    f: &mut dyn FnMut(&str, &'a Item, &'a Attr),
) {
    let mut arg_idx = 0;
    let mut warn_idx = 0;
    for a in attrs {
        f(path, item, a);
        let child_path = match a {
            Attr::Component(b) => format!("{}/component:{}", path, b.name),
            Attr::Candidate(b) => format!("{}/candidate:{}", path, b.name),
            Attr::Source(b) => format!("{}/source:{}", path, b.name),
            Attr::Argument(x) => {
                arg_idx += 1;
                format!(
                    "{}/argument:{}",
                    path,
                    x.name.clone().unwrap_or_else(|| arg_idx.to_string())
                )
            }
            Attr::WarningCondition(_) => {
                warn_idx += 1;
                format!("{}/warning_condition:{}", path, warn_idx)
            }
            _ => continue,
        };
        if let Some(children) = a.children() {
            walk_list(&child_path, item, children, f);
        }
    }
}

/// Collect names referenced by an expression tree: plain atoms, function
/// names, `result_of` targets and both halves of complex atoms.
pub fn expr_names(node: &Node) -> Vec<String> {
    let mut out = Vec::new();
    node.walk(&mut |n| match n {
        Node::Atom(a) => out.push(a.clone()),
        Node::Call(f, _) => out.push(f.clone()),
        Node::ResultOf(d) => out.push(d.clone()),
        Node::Complex(a, b) => {
            out.push(format!("{}:{}", a, b));
            out.push(a.clone());
        }
        _ => {}
    });
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct Reference {
    /// Selector of the block containing the reference.
    pub location: String,
    /// Attribute keyword holding the reference.
    pub attribute: String,
    /// How the name is used: task_definition, component, schedule_constraint,
    /// data_definition, source, expression, result_of, function_call.
    pub usage: String,
    /// The expression text or value where the reference occurs.
    pub context: String,
}

/// Every place where `name` is referenced (definitions themselves excluded).
pub fn references(doc: &Document, name: &str) -> Vec<Reference> {
    let mut refs = Vec::new();
    walk_attrs(doc, &mut |path, _item, attr| {
        let mut push = |usage: &str, context: String| {
            refs.push(Reference {
                location: path.to_string(),
                attribute: attr.keyword().to_string(),
                usage: usage.to_string(),
                context,
            })
        };
        match attr {
            Attr::Component(b) => {
                if b.task_definition() == name {
                    push("task_definition", format!("component {}", b.name));
                } else if b.name == name {
                    push("component", format!("component {}", b.name));
                }
            }
            Attr::ScheduleConstraint(s) if s == name => {
                push("schedule_constraint", format!("completed({})", s))
            }
            Attr::Source(b) => {
                if b.data_definition() == name {
                    push("data_definition", format!("source {}", b.name));
                } else if b.name == name {
                    push("source", format!("source {}", b.name));
                }
            }
            _ => {
                for e in attr.exprs() {
                    let mut hit = None;
                    e.node.walk(&mut |n| match n {
                        Node::Atom(a) if a == name => {
                            hit.get_or_insert("expression");
                        }
                        Node::Complex(a, b) if a == name || format!("{}:{}", a, b) == name => {
                            hit.get_or_insert("expression");
                        }
                        Node::ResultOf(d) if d == name => {
                            hit = Some("result_of");
                        }
                        Node::Call(f, _) if f == name => {
                            hit.get_or_insert("function_call");
                        }
                        _ => {}
                    });
                    if let Some(u) = hit {
                        push(u, e.text.clone());
                    }
                }
            }
        }
    });
    refs
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentSummary {
    pub name: String,
    pub kind: ItemKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
    /// Plans: number of components; decisions: number of candidates; enquiries: number of sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<usize>,
    pub line: u32,
}

pub fn summarize(item: &Item) -> ComponentSummary {
    let children = match item.kind {
        ItemKind::Plan => Some(item.components().count()),
        ItemKind::Decision => Some(item.candidates().count()),
        ItemKind::Enquiry => Some(item.sources().count()),
        _ => None,
    };
    ComponentSummary {
        name: item.name.clone(),
        kind: item.kind,
        caption: item.caption().map(str::to_string),
        data_type: item.data_type().map(str::to_string),
        children,
        line: item.span.line,
    }
}

/// A node in a plan graph: one component (task instance) of a plan.
#[derive(Debug, Clone, Serialize)]
pub struct PlanNode {
    pub component: String,
    pub task_definition: String,
    /// Kind of the referenced task definition, if it exists.
    pub kind: Option<ItemKind>,
    pub caption: Option<String>,
    pub ltwh: Option<[i64; 4]>,
    /// Components that must complete before this one starts (`schedule_constraint :: completed(X)`).
    pub after: Vec<String>,
    pub optional: bool,
    pub autonomous: bool,
    pub terminal: bool,
    pub cyclic: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanGraph {
    pub plan: String,
    pub caption: Option<String>,
    pub nodes: Vec<PlanNode>,
}

pub fn plan_graph(doc: &Document, plan: &Item) -> PlanGraph {
    let nodes = plan
        .components()
        .map(|c| {
            let def = c.task_definition().to_string();
            let target = doc.find(&def);
            let flag = |kw: &str| {
                c.attrs.iter().any(|a| match a {
                    Attr::Optional(b) | Attr::Autonomous(b) | Attr::Terminal(b) => {
                        a.keyword() == kw && *b
                    }
                    _ => false,
                })
            };
            PlanNode {
                component: c.name.clone(),
                kind: target.map(|t| t.kind),
                caption: c
                    .caption()
                    .or_else(|| target.and_then(|t| t.caption()))
                    .map(str::to_string),
                ltwh: c.attrs.iter().find_map(|a| match a {
                    Attr::Ltwh(v) => Some(*v),
                    _ => None,
                }),
                after: c.schedule_constraints().map(str::to_string).collect(),
                optional: flag("optional"),
                autonomous: flag("autonomous"),
                terminal: flag("terminal"),
                cyclic: c.attrs.iter().any(|a| {
                    matches!(
                        a,
                        Attr::NumberOfCycles(_) | Attr::CycleUntil(_) | Attr::CycleRepeat(..)
                    )
                }),
                task_definition: def,
            }
        })
        .collect();
    PlanGraph {
        plan: plan.name.clone(),
        caption: plan.caption().map(str::to_string),
        nodes,
    }
}

/// Indented tree of the guideline starting at the root plan, followed by the
/// data model and any task definitions not reachable from the root.
pub fn outline(doc: &Document) -> String {
    let mut out = String::new();
    if let Some(md) = &doc.metadata {
        for (f, v) in &md.fields {
            if matches!(f, MetaField::Title | MetaField::Version | MetaField::Author) {
                out.push_str(&format!("{}: {}\n", f.as_str(), v));
            }
        }
    }
    if let Some(v) = &doc.engine_version {
        out.push_str(&format!("engine_version: {}\n", v));
    }
    let mut seen = HashSet::new();
    if let Some(root) = doc.root() {
        out.push_str("\nTask tree:\n");
        tree(doc, root, &root.name, 1, &mut seen, &mut out);
    }

    let orphans: Vec<_> = doc
        .items
        .iter()
        .filter(|i| i.kind.is_task() && !seen.contains(i.name.as_str()))
        .collect();
    if !orphans.is_empty() {
        out.push_str("\nTask definitions not used by any plan reachable from the root:\n");
        for o in orphans {
            out.push_str(&format!(
                "  - {} {}\n",
                o.kind.keyword(),
                label(&o.name, o.caption())
            ));
        }
    }

    let data: Vec<_> = doc
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Data)
        .collect();
    if !data.is_empty() {
        out.push_str("\nData items:\n");
        for d in data {
            let range = d.attrs.iter().find_map(|a| match a {
                Attr::Range(v) => Some(
                    v.iter()
                        .map(|e| e.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                _ => None,
            });
            let derived = d.attrs.iter().any(|a| matches!(a, Attr::Derivation(_)));
            out.push_str(&format!(
                "  - {} : {}{}{}\n",
                label(&d.name, d.caption()),
                d.data_type().unwrap_or("?"),
                range.map(|r| format!(" in [{}]", r)).unwrap_or_default(),
                if derived { " (derived)" } else { "" }
            ));
        }
    }

    let funcs: Vec<_> = doc
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Function)
        .collect();
    if !funcs.is_empty() {
        out.push_str("\nFunctions:\n");
        for f in funcs {
            out.push_str(&format!(
                "  - {} : {}\n",
                label(&f.name, f.caption()),
                f.data_type().unwrap_or("?")
            ));
        }
    }
    out
}

fn label(name: &str, caption: Option<&str>) -> String {
    match caption {
        Some(c) if !c.is_empty() && c != name => format!("{} \"{}\"", name, c),
        _ => name.to_string(),
    }
}

fn tree<'a>(
    doc: &'a Document,
    item: &'a Item,
    shown_as: &str,
    depth: usize,
    seen: &mut HashSet<&'a str>,
    out: &mut String,
) {
    let indent = "  ".repeat(depth);
    let first_visit = seen.insert(item.name.as_str());
    let extra = match item.kind {
        ItemKind::Decision => {
            let c: Vec<_> = item.candidates().map(|c| c.name.as_str()).collect();
            format!(" candidates: [{}]", c.join(", "))
        }
        ItemKind::Enquiry => {
            let s: Vec<_> = item.sources().map(|s| s.name.as_str()).collect();
            format!(" sources: [{}]", s.join(", "))
        }
        _ => String::new(),
    };
    let def = if shown_as != item.name {
        format!(" (definition {})", item.name)
    } else {
        String::new()
    };
    out.push_str(&format!(
        "{}- {} {}{}{}\n",
        indent,
        item.kind.keyword(),
        label(shown_as, item.caption()),
        def,
        extra
    ));
    if item.kind != ItemKind::Plan {
        return;
    }
    if !first_visit {
        out.push_str(&format!("{}  (already expanded above)\n", indent));
        return;
    }
    for c in item.components() {
        let after: Vec<_> = c.schedule_constraints().collect();
        let after = if after.is_empty() {
            String::new()
        } else {
            format!("  [after {}]", after.join(", "))
        };
        match doc.find(c.task_definition()) {
            Some(t) => {
                let before = out.len();
                tree(doc, t, &c.name, depth + 1, seen, out);
                if !after.is_empty() {
                    // attach scheduling info to the component's own line
                    let line_end = out[before..].find('\n').map(|i| before + i);
                    if let Some(i) = line_end {
                        out.insert_str(i, &after);
                    }
                }
            }
            None => out.push_str(&format!(
                "{}  - component {} -> MISSING task definition '{}'{}\n",
                indent,
                c.name,
                c.task_definition(),
                after
            )),
        }
    }
}

/// Count of each component kind.
pub fn stats(doc: &Document) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for i in &doc.items {
        *m.entry(i.kind.keyword().to_string()).or_insert(0) += 1;
    }
    m
}
