//! Mermaid flowchart export.
//!
//! The mapping follows Composer's plan editor:
//! - one node per plan component, shaped and coloured by task kind
//!   (action = rounded box, enquiry = diamond, decision = circle,
//!   plan = stadium / subgraph, generic task = trapezoid);
//! - the only edges are scheduling constraints: `schedule_constraint ::
//!   completed(A)` on component B draws `A --> B`, in file order;
//! - an edge leaving a decision is labelled with the candidate(s) that the
//!   target task's precondition tests (`result_of(D) = cand`,
//!   `result_set(D) includes cand`);
//! - `expanded` mode nests sub-plans as subgraphs, `drilldown` shows one
//!   level like Composer;
//! - direction and declaration order are derived from the `ltwh` layout.

use std::collections::HashSet;

use crate::ast::*;

#[derive(Debug, Clone)]
pub struct MermaidOptions {
    /// Plan to render (default: root plan).
    pub plan: Option<String>,
    /// Expand sub-plans as nested subgraphs.
    pub expanded: bool,
    /// `auto`, `TD`, `LR`, `BT` or `RL`.
    pub direction: String,
    /// `name`, `caption` or `both`.
    pub label: String,
    /// Draw decision candidates as nodes.
    pub candidates: bool,
    /// Draw sources as data nodes.
    pub data: bool,
    pub max_depth: usize,
}

impl Default for MermaidOptions {
    fn default() -> Self {
        MermaidOptions {
            plan: None,
            expanded: true,
            direction: "auto".into(),
            label: "both".into(),
            candidates: false,
            data: false,
            max_depth: 8,
        }
    }
}

const CLASSES: &[(&str, &str)] = &[
    (
        "action",
        "fill:#E4FAFA,stroke:#008585,stroke-width:2px,color:#000",
    ),
    (
        "enquiry",
        "fill:#E6FFE6,stroke:#009700,stroke-width:2px,color:#000",
    ),
    (
        "decision",
        "fill:#FFFFFF,stroke:#B40F78,stroke-width:2px,color:#000",
    ),
    (
        "plan",
        "fill:#FFDCD9,stroke:#E55344,stroke-width:2px,color:#000",
    ),
    (
        "keystone",
        "fill:#FEFBED,stroke:#D9AB0A,stroke-width:2px,color:#000",
    ),
    (
        "candidate",
        "fill:#F8A2D3,stroke:#B40F78,stroke-dasharray:3 3,color:#000",
    ),
    (
        "data",
        "fill:#F1F3F4,stroke:#5F6368,stroke-dasharray:2 2,color:#000",
    ),
    (
        "missing",
        "fill:#FCE8E6,stroke:#D93025,stroke-width:2px,color:#000",
    ),
    ("optional", "stroke-dasharray:6 4"),
    ("terminal", "stroke-width:4px"),
];

pub fn to_mermaid(doc: &Document, opts: &MermaidOptions) -> Result<String, String> {
    let plan = match &opts.plan {
        Some(p) => doc
            .find(p)
            .ok_or_else(|| format!("no component named '{}'", p))?,
        None => doc.root().ok_or("guideline has no root plan")?,
    };
    if plan.kind != ItemKind::Plan {
        return Err(format!(
            "'{}' is a {}, not a plan",
            plan.name,
            plan.kind.keyword()
        ));
    }
    let dir = match opts.direction.to_ascii_uppercase().as_str() {
        d @ ("TD" | "TB" | "LR" | "BT" | "RL") => d.to_string(),
        _ => auto_direction(plan),
    };

    let mut w = Writer {
        doc,
        opts,
        body: String::new(),
        used: HashSet::new(),
        data_nodes: HashSet::new(),
    };
    let title = doc
        .metadata
        .as_ref()
        .and_then(|m| m.fields.iter().find(|(f, _)| *f == MetaField::Title))
        .map(|(_, v)| v.clone())
        .or_else(|| plan.caption().map(str::to_string))
        .unwrap_or_else(|| plan.name.clone());

    let mut stack = vec![plan.name.clone()];
    w.plan_body(plan, &sanitize(&plan.name), 1, &mut stack);

    let mut out = String::new();
    out.push_str(&format!("---\ntitle: {}\n---\n", title.replace('\n', " ")));
    out.push_str(&format!("flowchart {}\n", dir));
    out.push_str(&w.body);
    for (c, style) in CLASSES {
        out.push_str(&format!("    classDef {} {}\n", c, style));
    }
    Ok(out)
}

/// TD if components spread more vertically than horizontally, else LR.
fn auto_direction(plan: &Item) -> String {
    let pts: Vec<[i64; 4]> = plan
        .components()
        .filter_map(|c| {
            c.attrs.iter().find_map(|a| match a {
                Attr::Ltwh(v) => Some(*v),
                _ => None,
            })
        })
        .collect();
    if pts.len() < 2 {
        return "TD".into();
    }
    let spread = |i: usize| {
        let min = pts.iter().map(|p| p[i]).min().unwrap();
        let max = pts.iter().map(|p| p[i]).max().unwrap();
        max - min
    };
    if spread(0) > spread(1) {
        "LR".into()
    } else {
        "TD".into()
    }
}

struct Writer<'a> {
    doc: &'a Document,
    opts: &'a MermaidOptions,
    body: String,
    used: HashSet<String>,
    data_nodes: HashSet<String>,
}

impl<'a> Writer<'a> {
    fn line(&mut self, depth: usize, s: &str) {
        self.body.push_str(&"    ".repeat(depth));
        self.body.push_str(s);
        self.body.push('\n');
    }

    fn unique_id(&mut self, raw: &str) -> String {
        let mut base = sanitize(raw);
        if base.is_empty()
            || base.starts_with(|c: char| c.is_ascii_digit())
            || is_mermaid_keyword(&base)
        {
            base = format!("n_{}", base);
        }
        let mut id = base.clone();
        let mut n = 2;
        while !self.used.insert(id.clone()) {
            id = format!("{}_{}", base, n);
            n += 1;
        }
        id
    }

    fn label_for(&self, c: &Block, def: Option<&Item>) -> String {
        let caption = c
            .caption()
            .or_else(|| def.and_then(|d| d.caption()))
            .filter(|s| !s.trim().is_empty());
        let base = match (self.opts.label.as_str(), caption) {
            ("caption", Some(cap)) => esc(cap),
            ("both", Some(cap)) if cap != c.name => {
                format!("{}<br/><i>{}</i>", esc(&c.name), esc(cap))
            }
            _ => esc(&c.name),
        };
        let markers = markers(c, def);
        if markers.is_empty() {
            base
        } else {
            format!("{}<br/>{}", base, esc(&markers.join(" ")))
        }
    }

    fn plan_body(&mut self, plan: &'a Item, scope: &str, depth: usize, stack: &mut Vec<String>) {
        let mut comps: Vec<&'a Block> = plan.components().collect();
        // declaration order approximates Composer's layout: by (top, left)
        let pos = |c: &Block| {
            c.attrs
                .iter()
                .find_map(|a| match a {
                    Attr::Ltwh(v) => Some((v[1], v[0])),
                    _ => None,
                })
                .unwrap_or((i64::MAX, i64::MAX))
        };
        let mut ordered = comps.clone();
        ordered.sort_by_key(|c| pos(c));

        let ids: Vec<(String, String)> = comps
            .iter()
            .map(|c| (c.name.clone(), format!("{}__{}", scope, sanitize(&c.name))))
            .map(|(n, raw)| (n, self.unique_id(&raw)))
            .collect();
        let id_of = |name: &str| {
            ids.iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, i)| i.clone())
        };

        // candidate node ids per decision component, when drawn
        let mut cand_ids: Vec<(String, String, String)> = Vec::new(); // (component, candidate, id)

        for c in &ordered {
            let id = id_of(&c.name).unwrap();
            let def = self.doc.find(c.task_definition());
            let label = self.label_for(c, def);
            let optional = c.attrs.iter().any(|a| matches!(a, Attr::Optional(true)));
            let terminal = c.attrs.iter().any(|a| matches!(a, Attr::Terminal(true)));
            let mut classes = Vec::new();
            match def {
                Some(d)
                    if d.kind == ItemKind::Plan
                        && self.opts.expanded
                        && stack.len() < self.opts.max_depth
                        && !stack.contains(&d.name) =>
                {
                    self.line(depth, &format!("subgraph {}[\"{}\"]", id, label));
                    stack.push(d.name.clone());
                    self.plan_body(d, &id, depth + 1, stack);
                    stack.pop();
                    self.line(depth, "end");
                    let mut style = "fill:#FFDCD9,stroke:#E55344,stroke-width:2px".to_string();
                    if optional {
                        style.push_str(",stroke-dasharray:6 4");
                    }
                    self.line(depth, &format!("style {} {}", id, style));
                    continue;
                }
                Some(d) => {
                    let (open, close) = shape(d.kind);
                    self.line(depth, &format!("{}{}\"{}\"{}", id, open, label, close));
                    classes.push(class_of(d.kind));
                    if d.kind == ItemKind::Decision && self.opts.candidates {
                        for cand in d.candidates() {
                            let cid = self.unique_id(&format!("{}__c_{}", id, cand.name));
                            let n_args = cand.arguments().count();
                            let text = match cand.caption() {
                                Some(cap) if self.opts.label != "name" && cap != cand.name => {
                                    format!("{}<br/><i>{}</i>", esc(&cand.name), esc(cap))
                                }
                                _ => esc(&cand.name),
                            };
                            self.line(
                                depth,
                                &format!(
                                    "{}>\"{}<br/>({} arg{})\"]:::candidate",
                                    cid,
                                    text,
                                    n_args,
                                    if n_args == 1 { "" } else { "s" }
                                ),
                            );
                            self.line(depth, &format!("{} -.-> {}", id, cid));
                            cand_ids.push((c.name.clone(), cand.name.clone(), cid));
                        }
                    }
                    if self.opts.data {
                        for s in d.sources() {
                            let dname = s.data_definition().to_string();
                            let did = format!("data__{}", sanitize(&dname));
                            if self.data_nodes.insert(did.clone()) {
                                self.used.insert(did.clone());
                                let item = self.doc.find(&dname);
                                let text = match (
                                    item.and_then(|i| i.caption()),
                                    item.and_then(|i| i.data_type()),
                                ) {
                                    (Some(cap), Some(t)) => {
                                        format!("{}<br/><i>{}: {}</i>", esc(&dname), esc(cap), t)
                                    }
                                    (None, Some(t)) => format!("{}<br/><i>{}</i>", esc(&dname), t),
                                    _ => esc(&dname),
                                };
                                self.line(depth, &format!("{}[(\"{}\")]:::data", did, text));
                            }
                            self.line(depth, &format!("{} -.- {}", did, id));
                        }
                    }
                }
                None => {
                    self.line(
                        depth,
                        &format!(
                            "{}[\"{}<br/><b>missing definition: {}</b>\"]",
                            id,
                            label,
                            esc(c.task_definition())
                        ),
                    );
                    classes.push("missing");
                }
            }
            if optional {
                classes.push("optional");
            }
            if terminal {
                classes.push("terminal");
            }
            if !classes.is_empty() {
                self.line(depth, &format!("class {} {}", id, classes.join(",")));
            }
        }

        comps.dedup_by(|a, b| a.name == b.name);
        for c in &comps {
            let target = id_of(&c.name).unwrap();
            let target_def = self.doc.find(c.task_definition());
            for dep in c.schedule_constraints() {
                let Some(src) = id_of(dep) else {
                    self.line(
                        depth,
                        &format!(
                            "%% unresolved: schedule_constraint completed({}) on {}",
                            dep, c.name
                        ),
                    );
                    continue;
                };
                // branch label when the antecedent is a decision tested by the target's precondition
                let src_comp = comps.iter().find(|x| x.name.eq_ignore_ascii_case(dep));
                let src_def = src_comp.and_then(|x| self.doc.find(x.task_definition()));
                let label = match (src_def, target_def) {
                    (Some(sd), Some(td)) if sd.kind == ItemKind::Decision => {
                        branch_label(td, &sd.name)
                    }
                    _ => None,
                };
                match label {
                    Some(l) => {
                        let routed = cand_ids
                            .iter()
                            .find(|(comp, cand, _)| comp == dep && *cand == l)
                            .map(|(_, _, id)| id.clone());
                        match routed {
                            Some(cid) => {
                                self.line(
                                    depth,
                                    &format!("%% schedule_constraint: completed({})", dep),
                                );
                                self.line(depth, &format!("{} --> {}", cid, target));
                            }
                            None => self
                                .line(depth, &format!("{} -->|\"{}\"| {}", src, esc(&l), target)),
                        }
                    }
                    None => self.line(depth, &format!("{} --> {}", src, target)),
                }
            }
        }
    }
}

/// Candidate(s) of `decision` tested positively by `task`'s precondition.
fn branch_label(task: &Item, decision: &str) -> Option<String> {
    let pre = task.attrs.iter().find_map(|a| match a {
        Attr::Precondition(e) => Some(e),
        _ => None,
    })?;
    let mut found: Vec<String> = Vec::new();
    let mut uncertain = false;
    collect_tests(&pre.node, decision, false, &mut found, &mut uncertain);
    if found.is_empty() {
        return None;
    }
    if uncertain {
        return Some("?".into());
    }
    found.dedup();
    Some(found.join("/"))
}

fn collect_tests(
    n: &Node,
    decision: &str,
    under_or: bool,
    found: &mut Vec<String>,
    uncertain: &mut bool,
) {
    let is_decision = |x: &Node| match x {
        Node::ResultOf(d) => d.eq_ignore_ascii_case(decision),
        Node::Call(f, args) if f == "result_set" || f == "result_of" => {
            matches!(args.first(), Some(Node::Atom(d)) if d.eq_ignore_ascii_case(decision))
        }
        _ => false,
    };
    let value = |x: &Node| -> Vec<String> {
        match x {
            Node::Atom(s) | Node::Str(s) => vec![s.clone()],
            Node::List(v) => v
                .iter()
                .filter_map(|e| match e {
                    Node::Atom(s) | Node::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            Node::Paren(inner) => match inner.as_ref() {
                Node::Atom(s) | Node::Str(s) => vec![s.clone()],
                _ => vec![],
            },
            _ => vec![],
        }
    };
    match n {
        Node::Paren(inner) => collect_tests(inner, decision, under_or, found, uncertain),
        Node::Bin(BinOp::And, a, b) => {
            collect_tests(a, decision, under_or, found, uncertain);
            collect_tests(b, decision, under_or, found, uncertain);
        }
        Node::Bin(BinOp::Or, a, b) => {
            collect_tests(a, decision, true, found, uncertain);
            collect_tests(b, decision, true, found, uncertain);
        }
        Node::Bin(op, a, b) => {
            let (side, other) = if is_decision(a) {
                (Some(b), a)
            } else if is_decision(b) {
                (Some(a), b)
            } else {
                (None, a)
            };
            let _ = other;
            if let Some(v) = side {
                let vals = value(v);
                match op {
                    BinOp::Equals
                    | BinOp::Includes
                    | BinOp::IncludesAny
                    | BinOp::IncludesAll
                    | BinOp::Oneof
                        if !vals.is_empty() =>
                    {
                        if under_or {
                            *uncertain = true;
                        }
                        found.extend(vals);
                    }
                    _ => {
                        *uncertain = true;
                        found.push("?".into());
                    }
                }
            }
        }
        Node::Call(f, args) if f == "not" => {
            let mut tmp = Vec::new();
            for a in args {
                collect_tests(a, decision, under_or, &mut tmp, uncertain);
            }
            if !tmp.is_empty() {
                *uncertain = true;
                found.push("?".into());
            }
        }
        _ => {}
    }
}

fn markers(c: &Block, def: Option<&Item>) -> Vec<String> {
    let mut v = Vec::new();
    let mut cycle_repeat = None;
    let mut cycle_until = None;
    let mut cycles = None;
    for a in &c.attrs {
        match a {
            Attr::Optional(true) => v.push("[optional]".to_string()),
            Attr::Terminal(true) => v.push("[terminal]".to_string()),
            Attr::Autonomous(true) => v.push("[auto]".to_string()),
            Attr::CycleRepeat(e, u) => cycle_repeat = Some(format!("{} {}", e.text, u.as_str())),
            Attr::CycleUntil(e) => cycle_until = Some(e.text.clone()),
            Attr::NumberOfCycles(e) => cycles = Some(e.text.clone()),
            _ => {}
        }
    }
    if cycle_repeat.is_some() || cycle_until.is_some() || cycles.is_some() {
        let mut parts = Vec::new();
        if let Some(r) = cycle_repeat {
            parts.push(r);
        }
        if let Some(u) = cycle_until {
            parts.push(format!("until {}", truncate(&u, 40)));
        }
        if let Some(n) = cycles {
            parts.push(format!("x{}", n));
        }
        v.push(format!("[cycle: {}]", parts.join(", ")));
    }
    if let Some(t) = def.and_then(|d| {
        d.attrs.iter().find_map(|a| match a {
            Attr::Trigger(t) => Some(t.clone()),
            _ => None,
        })
    }) {
        v.push(format!("[trigger: {}]", t));
    }
    v
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(n).collect::<String>())
    }
}

fn shape(kind: ItemKind) -> (&'static str, &'static str) {
    match kind {
        ItemKind::Action => ("(", ")"),
        ItemKind::Enquiry => ("{", "}"),
        ItemKind::Decision => ("((", "))"),
        ItemKind::Plan => ("([", "])"),
        ItemKind::Task => ("[\\", "/]"),
        ItemKind::Data => ("[(", ")]"),
        ItemKind::Function => ("[[", "]]"),
    }
}

fn class_of(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Action => "action",
        ItemKind::Enquiry => "enquiry",
        ItemKind::Decision => "decision",
        ItemKind::Plan => "plan",
        ItemKind::Task => "keystone",
        ItemKind::Data | ItemKind::Function => "data",
    }
}

fn is_mermaid_keyword(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "end"
            | "graph"
            | "subgraph"
            | "flowchart"
            | "style"
            | "class"
            | "classdef"
            | "click"
            | "default"
            | "direction"
            | "linkstyle"
    )
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Escape text for a double-quoted Mermaid label.
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("#quot;"),
            '<' => o.push_str("#lt;"),
            '>' => o.push_str("#gt;"),
            '\n' => o.push_str("<br/>"),
            '\r' => {}
            _ => o.push(ch),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    const SRC: &str = "engine_version :: '1.9';\nmetaprops :: '';\n\
        plan :: main; component :: ask; ltwh :: 0,0,0,0; component :: choose; ltwh :: 0,100,0,0; schedule_constraint :: completed(ask);\n\
        component :: treat; ltwh :: 0,200,0,0; schedule_constraint :: completed(choose);\n\
        component :: sub; ltwh :: 100,200,0,0; optional :: yes; schedule_constraint :: completed(choose); end plan.\n\
        enquiry :: ask; caption :: 'Ask \"Q\"'; end enquiry.\n\
        decision :: choose; candidate :: a; candidate :: b; end decision.\n\
        action :: treat; precondition :: result_of(choose) = a; end action.\n\
        plan :: sub; precondition :: result_of(choose) = b or x; component :: act; end plan.\n action :: act; end action.";

    #[test]
    fn renders_shapes_edges_and_branch_labels() {
        let out = parse(SRC);
        assert!(!out.has_errors(), "{:?}", out.diagnostics);
        let m = to_mermaid(&out.doc, &MermaidOptions::default()).unwrap();
        assert!(m.contains("flowchart TD"), "{}", m);
        assert!(
            m.contains("main__ask{\"ask<br/><i>Ask #quot;Q#quot;</i>\"}"),
            "{}",
            m
        );
        assert!(m.contains("main__choose((\"choose\"))"), "{}", m);
        assert!(m.contains("main__ask --> main__choose"), "{}", m);
        assert!(m.contains("main__choose -->|\"a\"| main__treat"), "{}", m);
        assert!(m.contains("main__choose -->|\"?\"| main__sub"), "{}", m);
        assert!(m.contains("subgraph main__sub"), "{}", m);
        assert!(m.contains("main__sub__act(\"act\")"), "{}", m);
    }

    #[test]
    fn drilldown_and_candidates() {
        let out = parse(SRC);
        let opts = MermaidOptions {
            expanded: false,
            candidates: true,
            ..Default::default()
        };
        let m = to_mermaid(&out.doc, &opts).unwrap();
        assert!(m.contains("main__sub([\"sub<br/>[optional]\"])"), "{}", m);
        assert!(m.contains("main__choose__c_a>"), "{}", m);
        assert!(m.contains("main__choose__c_a --> main__treat"), "{}", m);
    }
}
