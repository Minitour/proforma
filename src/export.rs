//! Semantic JSON export of a guideline.
//!
//! Unlike the raw syntax tree (`get --format json`), this is a flat,
//! query-friendly model: every task, data item and function with its
//! attributes as named fields, metaprops parsed into key/value maps,
//! expressions as source text, and the plan structure resolved (which plans
//! use a task, its paths from the root plan). Every field is always present
//! (`null` / `[]` / `false` when unset) so scripts can rely on the shape.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::ast::*;
use crate::props;

pub const FORMAT: &str = "proforma-export/1";

#[derive(Debug, Serialize)]
pub struct Export {
    pub format: &'static str,
    pub engine_version: Option<String>,
    pub metadata: BTreeMap<String, String>,
    /// Guideline-level metaprops.
    pub metaprops: BTreeMap<String, String>,
    pub root: Option<String>,
    pub tasks: Vec<Task>,
    pub data: Vec<Data>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Serialize)]
pub struct Task {
    pub name: String,
    pub kind: ItemKind,
    pub caption: Option<String>,
    pub description: Option<String>,
    pub metaprops: BTreeMap<String, String>,
    pub precondition: Option<String>,
    pub wait_condition: Option<String>,
    pub postcondition: Option<String>,
    pub goal: Option<String>,
    pub trigger: Option<String>,
    pub context: Option<String>,
    /// Action only.
    pub procedure: Option<String>,
    /// Plan only.
    pub abort: Option<String>,
    pub terminate: Option<String>,
    pub components: Vec<Component>,
    /// Decision only.
    pub choice_mode: Option<String>,
    pub support_mode: Option<String>,
    pub candidates: Vec<Candidate>,
    /// Enquiry / decision.
    pub sources: Vec<Source>,
    /// Plans that have a component running this task.
    pub used_by: Vec<String>,
    /// Component paths from the root plan, e.g. `root/Monitoring/Monitor_weight`.
    pub paths: Vec<String>,
    pub reachable: bool,
}

#[derive(Debug, Serialize)]
pub struct Component {
    pub name: String,
    pub task: String,
    pub caption: Option<String>,
    pub description: Option<String>,
    pub metaprops: BTreeMap<String, String>,
    /// Sibling components this one waits for (`schedule_constraint :: completed(x)`).
    pub after: Vec<String>,
    pub optional: bool,
    pub autonomous: bool,
    pub terminal: bool,
    pub number_of_cycles: Option<String>,
    pub cycle_until: Option<String>,
    /// e.g. `3 days`.
    pub cycle_repeat: Option<String>,
    pub param_values: Vec<String>,
    pub ltwh: Option<[i64; 4]>,
}

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub name: String,
    pub caption: Option<String>,
    pub description: Option<String>,
    pub metaprops: BTreeMap<String, String>,
    pub recommendation: Option<String>,
    pub priority: Option<i64>,
    pub arguments: Vec<ArgumentOut>,
}

#[derive(Debug, Serialize)]
pub struct ArgumentOut {
    pub name: Option<String>,
    /// `for`, `against`, `confirming`, `excluding` or a numeric weight.
    pub support: String,
    pub condition: String,
    pub caption: Option<String>,
    pub description: Option<String>,
    pub metaprops: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct Source {
    pub data: String,
    pub mandatory: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct Data {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: Option<String>,
    pub caption: Option<String>,
    pub description: Option<String>,
    pub metaprops: BTreeMap<String, String>,
    pub unit: Option<String>,
    pub range: Vec<String>,
    pub default_value: Option<String>,
    pub derivation: Option<String>,
    pub dynamic: bool,
    pub mandatory_validation: Option<String>,
    pub warnings: Vec<WarningOut>,
    /// Enquiries / decisions that have this data item as a source.
    pub asked_by: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct WarningOut {
    pub flag: String,
    pub condition: String,
    pub caption: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Function {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: Option<String>,
    pub caption: Option<String>,
    pub description: Option<String>,
    pub metaprops: BTreeMap<String, String>,
    pub body: Option<String>,
}

fn text(e: &Expr) -> String {
    e.text.trim().to_string()
}

fn metaprops(attrs: &[Attr]) -> BTreeMap<String, String> {
    attrs
        .iter()
        .filter_map(|a| match a {
            Attr::Metaprops(s) => Some(props::parse(s)),
            _ => None,
        })
        .flatten()
        .collect()
}

fn first<T>(attrs: &[Attr], f: impl Fn(&Attr) -> Option<T>) -> Option<T> {
    attrs.iter().find_map(f)
}

fn caption(attrs: &[Attr]) -> Option<String> {
    first(attrs, |a| match a {
        Attr::Caption(s) => Some(s.clone()),
        _ => None,
    })
}

fn description(attrs: &[Attr]) -> Option<String> {
    first(attrs, |a| match a {
        Attr::Description(s) => Some(s.clone()),
        _ => None,
    })
}

fn expr_attr(attrs: &[Attr], keyword: &str) -> Option<String> {
    first(attrs, |a| {
        if a.keyword() != keyword {
            return None;
        }
        match a {
            Attr::Precondition(e)
            | Attr::WaitCondition(e)
            | Attr::Postcondition(e)
            | Attr::Goal(e)
            | Attr::Abort(e)
            | Attr::Terminate(e)
            | Attr::Recommendation(e)
            | Attr::DefaultValue(e)
            | Attr::Derivation(e)
            | Attr::Derivation483(e)
            | Attr::MandatoryValidation(e)
            | Attr::NumberOfCycles(e)
            | Attr::CycleUntil(e)
            | Attr::FunctionBody(e) => Some(text(e)),
            _ => None,
        }
    })
}

fn string_attr(attrs: &[Attr], keyword: &str) -> Option<String> {
    first(attrs, |a| {
        if a.keyword() != keyword {
            return None;
        }
        match a {
            Attr::Trigger(s)
            | Attr::Context(s)
            | Attr::Procedure(s)
            | Attr::ChoiceMode(s)
            | Attr::SupportMode(s)
            | Attr::Type(s) => Some(s.clone()),
            _ => None,
        }
    })
}

fn flag(attrs: &[Attr], f: impl Fn(&Attr) -> Option<bool>) -> bool {
    first(attrs, f).unwrap_or(false)
}

fn component(b: &Block) -> Component {
    let a = &b.attrs;
    Component {
        name: b.name.clone(),
        task: b.task_definition().to_string(),
        caption: caption(a),
        description: description(a),
        metaprops: metaprops(a),
        after: b.schedule_constraints().map(str::to_string).collect(),
        optional: flag(a, |x| match x {
            Attr::Optional(v) => Some(*v),
            _ => None,
        }),
        autonomous: flag(a, |x| match x {
            Attr::Autonomous(v) => Some(*v),
            _ => None,
        }),
        terminal: flag(a, |x| match x {
            Attr::Terminal(v) => Some(*v),
            _ => None,
        }),
        number_of_cycles: expr_attr(a, "number_of_cycles"),
        cycle_until: expr_attr(a, "cycle_until"),
        cycle_repeat: first(a, |x| match x {
            Attr::CycleRepeat(e, u) => Some(format!("{} {}", text(e), u.as_str())),
            _ => None,
        }),
        param_values: a
            .iter()
            .filter_map(|x| match x {
                Attr::ParamValue(e) => Some(text(e)),
                _ => None,
            })
            .collect(),
        ltwh: first(a, |x| match x {
            Attr::Ltwh(v) => Some(*v),
            _ => None,
        }),
    }
}

fn candidate(b: &Block) -> Candidate {
    let a = &b.attrs;
    Candidate {
        name: b.name.clone(),
        caption: caption(a),
        description: description(a),
        metaprops: metaprops(a),
        recommendation: expr_attr(a, "recommendation"),
        priority: first(a, |x| match x {
            Attr::Priority(p) => Some(*p),
            _ => None,
        }),
        arguments: b
            .arguments()
            .map(|arg| ArgumentOut {
                name: arg.name.clone(),
                support: arg.support.label(),
                condition: text(&arg.condition),
                caption: caption(&arg.attrs),
                description: description(&arg.attrs),
                metaprops: metaprops(&arg.attrs),
            })
            .collect(),
    }
}

fn source(b: &Block) -> Source {
    Source {
        data: b.data_definition().to_string(),
        mandatory: first(&b.attrs, |x| match x {
            Attr::Mandatory(v) => Some(*v),
            _ => None,
        }),
    }
}

/// Build the export model of a parsed document.
pub fn export(doc: &Document) -> Export {
    let root = doc.root().filter(|r| r.kind == ItemKind::Plan);

    // plan structure: who uses each task, and its paths from the root
    let mut used_by: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for plan in doc.items.iter().filter(|i| i.kind == ItemKind::Plan) {
        for c in plan.components() {
            let users = used_by.entry(c.task_definition()).or_default();
            if !users.contains(&plan.name) {
                users.push(plan.name.clone());
            }
        }
    }
    let mut paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(r) = root {
        fn walk(
            doc: &Document,
            item: &Item,
            path: String,
            stack: &mut Vec<String>,
            out: &mut BTreeMap<String, Vec<String>>,
        ) {
            out.entry(item.name.clone()).or_default().push(path.clone());
            if item.kind != ItemKind::Plan || stack.contains(&item.name) {
                return;
            }
            stack.push(item.name.clone());
            for c in item.components() {
                if let Some(t) = doc.find(c.task_definition()) {
                    walk(doc, t, format!("{}/{}", path, c.name), stack, out);
                }
            }
            stack.pop();
        }
        walk(doc, r, r.name.clone(), &mut Vec::new(), &mut paths);
    }

    let mut asked_by: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for t in doc.items.iter().filter(|i| i.kind.is_task()) {
        for s in t.sources() {
            asked_by
                .entry(s.data_definition())
                .or_default()
                .push(t.name.clone());
        }
    }

    let mut tasks = Vec::new();
    let mut data = Vec::new();
    let mut functions = Vec::new();
    for item in &doc.items {
        let a = &item.attrs;
        match item.kind {
            ItemKind::Data => data.push(Data {
                name: item.name.clone(),
                data_type: item.data_type().map(str::to_string),
                caption: caption(a),
                description: description(a),
                metaprops: metaprops(a),
                unit: first(a, |x| match x {
                    Attr::Unit(c) => Some(c.value().to_string()),
                    _ => None,
                }),
                range: first(a, |x| match x {
                    Attr::Range(v) => Some(v.iter().map(text).collect()),
                    _ => None,
                })
                .unwrap_or_default(),
                default_value: expr_attr(a, "default_value"),
                derivation: expr_attr(a, "derivation"),
                dynamic: flag(a, |x| match x {
                    Attr::Dynamic(v) => Some(*v),
                    _ => None,
                }),
                mandatory_validation: expr_attr(a, "mandatory_validation"),
                warnings: a
                    .iter()
                    .filter_map(|x| match x {
                        Attr::WarningCondition(w) => Some(WarningOut {
                            flag: w.flag.value().to_string(),
                            condition: text(&w.condition),
                            caption: caption(&w.attrs),
                        }),
                        _ => None,
                    })
                    .collect(),
                asked_by: asked_by
                    .get(item.name.as_str())
                    .cloned()
                    .unwrap_or_default(),
            }),
            ItemKind::Function => functions.push(Function {
                name: item.name.clone(),
                data_type: item.data_type().map(str::to_string),
                caption: caption(a),
                description: description(a),
                metaprops: metaprops(a),
                body: expr_attr(a, "function_body"),
            }),
            _ => {
                let p = paths.get(&item.name).cloned().unwrap_or_default();
                tasks.push(Task {
                    name: item.name.clone(),
                    kind: item.kind,
                    caption: caption(a),
                    description: description(a),
                    metaprops: metaprops(a),
                    precondition: expr_attr(a, "precondition"),
                    wait_condition: expr_attr(a, "wait_condition"),
                    postcondition: expr_attr(a, "postcondition"),
                    goal: expr_attr(a, "goal"),
                    trigger: string_attr(a, "trigger"),
                    context: string_attr(a, "context"),
                    procedure: string_attr(a, "procedure"),
                    abort: expr_attr(a, "abort"),
                    terminate: expr_attr(a, "terminate"),
                    components: item.components().map(component).collect(),
                    choice_mode: string_attr(a, "choice_mode"),
                    support_mode: string_attr(a, "support_mode"),
                    candidates: item.candidates().map(candidate).collect(),
                    sources: item.sources().map(source).collect(),
                    used_by: used_by.get(item.name.as_str()).cloned().unwrap_or_default(),
                    reachable: !p.is_empty(),
                    paths: p,
                });
            }
        }
    }

    Export {
        format: FORMAT,
        engine_version: doc.engine_version.clone(),
        metadata: doc
            .metadata
            .as_ref()
            .map(|m| {
                let mut out: BTreeMap<String, String> = BTreeMap::new();
                out.insert("name".into(), m.name.clone());
                for (k, v) in &m.fields {
                    out.entry(k.as_str().to_string())
                        .or_insert_with(|| v.clone());
                }
                out
            })
            .unwrap_or_default(),
        metaprops: doc
            .metaprops
            .as_deref()
            .map(|s| props::parse(s).into_iter().collect())
            .unwrap_or_default(),
        root: root.map(|r| r.name.clone()),
        tasks,
        data,
        functions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    const SRC: &str = "engine_version :: '1.9';\nmetadata :: m; title :: 'T'; end metadata.\nmetaprops :: '';\n\
        plan :: main; component :: ask; ltwh :: 0,0,0,0; component :: choose; schedule_constraint :: completed(ask);\n\
        component :: treat; schedule_constraint :: completed(choose); cycle_repeat :: 3 days; end plan.\n\
        enquiry :: ask; source :: age; end enquiry.\n\
        decision :: choose; candidate :: a; caption :: 'A'; argument :: for, age >= 18 attributes argument_name :: adult; end attributes ;\n\
        recommendation :: netsupport() > 0; end decision.\n\
        action :: treat; metaprops :: 'goal.verb=order\nsource.1=X'; precondition :: result_of(choose) = a; procedure :: 'Give X'; end action.\n\
        action :: orphan; end action.\n\
        data :: age; type :: integer; unit :: years; end data.";

    #[test]
    fn exports_structure_and_metaprops() {
        let out = parse(SRC);
        assert!(
            out.diagnostics.iter().all(|d| !d.is_fatal()),
            "{:?}",
            out.diagnostics
        );
        let e = export(&out.doc);
        assert_eq!(e.root.as_deref(), Some("main"));
        assert_eq!(e.metadata.get("title").map(String::as_str), Some("T"));
        let treat = e.tasks.iter().find(|t| t.name == "treat").unwrap();
        assert_eq!(treat.precondition.as_deref(), Some("result_of(choose) = a"));
        assert_eq!(treat.procedure.as_deref(), Some("Give X"));
        assert_eq!(
            treat.metaprops.get("goal.verb").map(String::as_str),
            Some("order")
        );
        assert_eq!(treat.paths, vec!["main/treat".to_string()]);
        assert_eq!(treat.used_by, vec!["main".to_string()]);
        let main = e.tasks.iter().find(|t| t.name == "main").unwrap();
        assert_eq!(main.components[2].after, vec!["choose".to_string()]);
        assert_eq!(main.components[2].cycle_repeat.as_deref(), Some("3 days"));
        let choose = e.tasks.iter().find(|t| t.name == "choose").unwrap();
        let arg = &choose.candidates[0].arguments[0];
        assert_eq!(
            (arg.support.as_str(), arg.condition.as_str()),
            ("for", "age >= 18")
        );
        assert_eq!(arg.name.as_deref(), Some("adult"));
        assert!(
            !e.tasks
                .iter()
                .find(|t| t.name == "orphan")
                .unwrap()
                .reachable
        );
        let age = &e.data[0];
        assert_eq!(age.data_type.as_deref(), Some("integer"));
        assert_eq!(age.asked_by, vec!["ask".to_string()]);
    }
}
