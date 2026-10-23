//! Tool implementations shared by the MCP server and the CLI.
//!
//! Each tool is a parameter struct that derives `clap::Args` (CLI flags),
//! `Deserialize` + `JsonSchema` (MCP arguments) and exposes `run()`.
//! The file on disk is always the source of truth: every call re-reads it,
//! and every edit is written back atomically after verifying that the result
//! parses.

use std::path::Path;

use clap::Args;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ast::*;
use crate::diagnostics::{Diagnostic, Severity};
use crate::edit::{self, ChangeReport};
use crate::lint;
use crate::mermaid::{self, MermaidOptions};
use crate::model;
use crate::parser::{self, ParseOutput};
use crate::writer;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ToolError(pub String);

impl From<edit::EditError> for ToolError {
    fn from(e: edit::EditError) -> Self {
        ToolError(e.to_string())
    }
}

impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        ToolError(e.to_string())
    }
}

pub type ToolResult = Result<String, ToolError>;

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|e| format!("serialization error: {}", e))
}

// ── file helpers ─────────────────────────────────────────────────────────────

fn read(path: &str) -> Result<String, ToolError> {
    std::fs::read_to_string(path).map_err(|e| ToolError(format!("cannot read '{}': {}", path, e)))
}

/// Parse a file for read-only use (partial documents are fine).
fn load(path: &str) -> Result<ParseOutput, ToolError> {
    Ok(parser::parse(&read(path)?))
}

/// Parse a file for editing: refuses files with syntax errors, since a partial
/// tree would silently drop content when written back.
fn load_for_edit(path: &str) -> Result<Document, ToolError> {
    let out = load(path)?;
    let syntax: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.is_fatal())
        .map(|d| d.render(path))
        .collect();
    if !syntax.is_empty() {
        return Err(ToolError(format!(
            "refusing to edit a file with syntax errors (fix them first, e.g. with upsert_component):\n{}",
            syntax.join("\n")
        )));
    }
    Ok(out.doc)
}

fn write_atomic(path: &str, text: &str) -> Result<(), ToolError> {
    let p = Path::new(path);
    let dir = p.parent().filter(|d| !d.as_os_str().is_empty());
    let tmp = match dir {
        Some(d) => d.join(format!(
            ".{}.tmp",
            p.file_name().unwrap_or_default().to_string_lossy()
        )),
        None => Path::new(&format!(".{}.tmp", path)).to_path_buf(),
    };
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ToolError(format!("cannot write '{}': {}", path, e))
    })
}

/// Serialize, verify and save; returns the change report plus a lint summary.
fn save(path: &str, doc: &Document, report: ChangeReport) -> ToolResult {
    let text = writer::write(doc);
    let check = parser::parse(&text);
    if let Some(d) = check.diagnostics.iter().find(|d| d.is_fatal()) {
        return Err(ToolError(format!(
            "edit aborted: the result would not parse ({}). The file was not modified.",
            d.render(path)
        )));
    }
    write_atomic(path, &text)?;
    let diags = lint::lint(
        &check.doc,
        &check.diagnostics,
        &lint::LintOptions::default(),
    );
    let errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    let mut out = report.changes.join("\n");
    for w in &report.warnings {
        out.push_str(&format!("\nwarning: {}", w));
    }
    out.push_str(&format!(
        "\nsaved {} ({} lint error(s), {} warning(s){})",
        path,
        errors,
        warnings,
        if errors + warnings > 0 {
            "; run lint for details"
        } else {
            ""
        }
    ));
    Ok(out)
}

fn parse_kind(kind: &str) -> Result<ItemKind, ToolError> {
    ItemKind::parse(kind).ok_or_else(|| {
        ToolError(format!(
            "unknown component kind '{}' (expected plan, decision, action, enquiry, task, data or function)",
            kind
        ))
    })
}

/// Case-insensitive name collision check across all top-level components.
fn ensure_unique(doc: &Document, name: &str) -> Result<(), ToolError> {
    if let Some(existing) = doc.items.iter().find(|i| i.name.eq_ignore_ascii_case(name)) {
        return Err(ToolError(format!(
            "name '{}' clashes with existing {} '{}' (names are compared case-insensitively)",
            name,
            existing.kind.keyword(),
            existing.name
        )));
    }
    Ok(())
}

fn ensure_identifier(name: &str) -> Result<(), ToolError> {
    let ok = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok {
        return Err(ToolError(format!(
            "'{}' is not a valid name: use letters, digits and underscores, starting with a letter (Composer sanitizes names this way); put human-readable text in the caption",
            name
        )));
    }
    Ok(())
}

fn q(s: &str) -> String {
    edit::quote_text(s)
}

// ── read-only tools ──────────────────────────────────────────────────────────

/// Return the PROforma language reference (syntax, semantics, editing conventions) for agents.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct LanguageReference {
    /// Optional section: overview, syntax, tasks, decisions, data, expressions, functions, editing. Omit for everything.
    #[arg(long)]
    #[serde(default)]
    pub section: Option<String>,
}

impl LanguageReference {
    pub fn run(self) -> ToolResult {
        Ok(crate::reference::get(self.section.as_deref()))
    }
}

/// Show a guideline overview: metadata, the task tree from the root plan (with scheduling), data items and functions.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct Outline {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
}

impl Outline {
    pub fn run(self) -> ToolResult {
        let out = load(&self.file)?;
        let mut s = model::outline(&out.doc);
        let errors = out.diagnostics.iter().filter(|d| d.is_fatal()).count();
        if errors > 0 {
            s.push_str(&format!(
                "\nNOTE: {} syntax error(s); the outline may be incomplete. Run lint.\n",
                errors
            ));
        }
        Ok(s)
    }
}

/// List top-level components (tasks, data items, functions) with kind, caption and type.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct ListComponents {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Only this kind: plan, decision, action, enquiry, task, data or function
    #[arg(long)]
    #[serde(default)]
    pub kind: Option<String>,
    /// Case-insensitive substring filter on name or caption
    #[arg(long)]
    #[serde(default)]
    pub filter: Option<String>,
}

impl ListComponents {
    pub fn run(self) -> ToolResult {
        let out = load(&self.file)?;
        let kind = self.kind.as_deref().map(parse_kind).transpose()?;
        let filter = self.filter.map(|f| f.to_lowercase());
        let list: Vec<_> = out
            .doc
            .items
            .iter()
            .filter(|i| kind.is_none_or(|k| i.kind == k))
            .filter(|i| {
                filter.as_ref().is_none_or(|f| {
                    i.name.to_lowercase().contains(f)
                        || i.caption().is_some_and(|c| c.to_lowercase().contains(f))
                })
            })
            .map(model::summarize)
            .collect();
        Ok(json(&list))
    }
}

/// Get one component or nested element as PROforma text (default) or JSON. Selector examples: `Choose`, `Main/component:Ask`, `Choose/candidate:a`, `Choose/candidate:a/argument:1`, `age/warning_condition:1`.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct GetComponent {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Component name or selector
    #[arg(long)]
    pub selector: String,
    /// Output format: pf (default) or json
    #[arg(long, default_value = "pf")]
    #[serde(default = "default_pf")]
    pub format: String,
}

fn default_pf() -> String {
    "pf".into()
}

impl GetComponent {
    pub fn run(self) -> ToolResult {
        let out = load(&self.file)?;
        let doc = &out.doc;
        let as_json = self.format.eq_ignore_ascii_case("json");
        if !self.selector.contains('/') {
            let item = doc
                .find(&self.selector)
                .ok_or_else(|| ToolError(format!("no component named '{}'", self.selector)))?;
            if as_json {
                return Ok(json(item));
            }
            let mut s = writer::write_item(item, doc);
            let refs = model::references(doc, &item.name);
            if !refs.is_empty() {
                s.push_str("\n/** referenced by: ");
                let locs: Vec<_> = refs
                    .iter()
                    .map(|r| format!("{} ({})", r.location, r.attribute))
                    .collect();
                s.push_str(&locs.join(", "));
                s.push_str(" **/\n");
            }
            return Ok(s);
        }
        let attr = edit::target_child_ref(doc, &self.selector)?;
        if as_json {
            Ok(json(attr))
        } else {
            Ok(writer::write_attr(attr, 0))
        }
    }
}

/// Find every reference to a name: plan components, scheduling constraints, sources and expressions.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct FindReferences {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Task, data item, function or component name
    #[arg(long)]
    pub name: String,
}

impl FindReferences {
    pub fn run(self) -> ToolResult {
        let out = load(&self.file)?;
        Ok(json(&model::references(&out.doc, &self.name)))
    }
}

/// Return the graph of a plan: its components (task instances) with kind, caption, layout and scheduling dependencies.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct PlanGraph {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Plan name (defaults to the root plan)
    #[arg(long)]
    #[serde(default)]
    pub plan: Option<String>,
}

impl PlanGraph {
    pub fn run(self) -> ToolResult {
        let out = load(&self.file)?;
        let plan = match &self.plan {
            Some(p) => out
                .doc
                .find(p)
                .ok_or_else(|| ToolError(format!("no component named '{}'", p)))?,
            None => out
                .doc
                .root()
                .ok_or_else(|| ToolError("guideline has no root plan".into()))?,
        };
        if plan.kind != ItemKind::Plan {
            return Err(ToolError(format!("'{}' is not a plan", plan.name)));
        }
        Ok(json(&model::plan_graph(&out.doc, plan)))
    }
}

/// Search names, captions, descriptions and expressions with a case-insensitive substring.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct Search {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Text to look for (case-insensitive)
    #[arg(long)]
    pub query: String,
    /// Maximum number of hits (default 100)
    #[arg(long, default_value_t = 100)]
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

#[derive(Serialize)]
struct Hit {
    location: String,
    field: String,
    text: String,
}

impl Search {
    pub fn run(self) -> ToolResult {
        let out = load(&self.file)?;
        let needle = self.query.to_lowercase();
        let mut hits = Vec::new();
        for item in &out.doc.items {
            if item.name.to_lowercase().contains(&needle) {
                hits.push(Hit {
                    location: item.name.clone(),
                    field: format!("{} name", item.kind.keyword()),
                    text: item.name.clone(),
                });
            }
        }
        model::walk_attrs(&out.doc, &mut |path, _item, attr| {
            let mut texts: Vec<String> = attr.exprs().iter().map(|e| e.text.clone()).collect();
            match attr {
                Attr::Caption(s)
                | Attr::Description(s)
                | Attr::Procedure(s)
                | Attr::Trigger(s)
                | Attr::Context(s) => texts.push(s.clone()),
                Attr::Component(b) | Attr::Candidate(b) | Attr::Source(b) => {
                    texts.push(b.name.clone())
                }
                _ => {}
            }
            for t in texts {
                if t.to_lowercase().contains(&needle) {
                    hits.push(Hit {
                        location: path.to_string(),
                        field: attr.keyword().to_string(),
                        text: t,
                    });
                }
            }
        });
        hits.truncate(self.limit);
        Ok(json(&hits))
    }
}

/// Validate a guideline: syntax, Composer compatibility, references, scheduling, decisions, data model and expression types.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct Lint {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Include informational findings (default false)
    #[arg(long)]
    #[serde(default)]
    pub include_info: bool,
    /// Output format: text (default) or json
    #[arg(long, default_value = "text")]
    #[serde(default = "default_text")]
    pub format: String,
}

fn default_text() -> String {
    "text".into()
}

impl Lint {
    pub fn run(self) -> ToolResult {
        let out = load(&self.file)?;
        let opts = lint::LintOptions {
            include_info: self.include_info,
        };
        let diags = lint::lint(&out.doc, &out.diagnostics, &opts);
        if self.format.eq_ignore_ascii_case("json") {
            return Ok(json(&diags));
        }
        Ok(render_diags(&self.file, &diags))
    }
}

pub fn render_diags(file: &str, diags: &[Diagnostic]) -> String {
    let errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    let mut s: String = diags.iter().map(|d| d.render(file) + "\n").collect();
    s.push_str(&format!("{} error(s), {} warning(s)\n", errors, warnings));
    s
}

/// Parse and analyse an expression: syntax, referenced names and what they resolve to, and inferred type.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct CheckExpression {
    /// The expression, e.g. `age >= 18 and result_of(triage) = urgent`
    #[arg(long)]
    pub expression: String,
    /// Optional .pf file used to resolve names and types
    #[arg(long)]
    #[serde(default)]
    pub file: Option<String>,
    /// Parse as an assertion (postcondition / param_value syntax: `x = e and y := f`)
    #[arg(long)]
    #[serde(default)]
    pub assertion: bool,
}

impl CheckExpression {
    pub fn run(self) -> ToolResult {
        let parsed = if self.assertion {
            parser::parse_assertion(&self.expression)
        } else {
            parser::parse_expression(&self.expression)
        };
        let expr = parsed.map_err(|d| ToolError(format!("invalid expression: {}", d.message)))?;
        let doc = match &self.file {
            Some(f) => load(f)?.doc,
            None => Document::default(),
        };
        Ok(json(&lint::analyze_expression(&doc, &expr)))
    }
}

/// Export a plan as a Mermaid flowchart (shapes per task type, scheduling arrows, nested sub-plans, decision branches).
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct ToMermaid {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Plan to render (defaults to the root plan)
    #[arg(long)]
    #[serde(default)]
    pub plan: Option<String>,
    /// expanded (default; sub-plans become nested subgraphs) or drilldown (one level, like Composer's plan editor)
    #[arg(long, default_value = "expanded")]
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Flowchart direction: auto (default, from ltwh layout), TD or LR
    #[arg(long, default_value = "auto")]
    #[serde(default = "default_auto")]
    pub direction: String,
    /// Node labels: both (default), name or caption
    #[arg(long, default_value = "both")]
    #[serde(default = "default_both")]
    pub label: String,
    /// Draw decision candidates as nodes
    #[arg(long)]
    #[serde(default)]
    pub candidates: bool,
    /// Draw enquiry/decision data sources as data nodes
    #[arg(long)]
    #[serde(default)]
    pub data: bool,
    /// Maximum sub-plan nesting depth in expanded mode (default 8)
    #[arg(long, default_value_t = 8)]
    #[serde(default = "default_depth")]
    pub depth: usize,
    /// Write the diagram to this file instead of returning it (.mmd or .md; .md wraps it in a ```mermaid fence)
    #[arg(long)]
    #[serde(default)]
    pub output: Option<String>,
}

fn default_mode() -> String {
    "expanded".into()
}
fn default_auto() -> String {
    "auto".into()
}
fn default_both() -> String {
    "both".into()
}
fn default_depth() -> usize {
    8
}

impl ToMermaid {
    pub fn run(self) -> ToolResult {
        let out = load(&self.file)?;
        let opts = MermaidOptions {
            plan: self.plan,
            expanded: !self.mode.eq_ignore_ascii_case("drilldown"),
            direction: self.direction,
            label: self.label,
            candidates: self.candidates,
            data: self.data,
            max_depth: self.depth,
        };
        let m = mermaid::to_mermaid(&out.doc, &opts).map_err(ToolError)?;
        match self.output {
            Some(path) => {
                let text = if path.to_lowercase().ends_with(".md") {
                    format!("```mermaid\n{}```\n", m)
                } else {
                    m
                };
                write_atomic(&path, &text)?;
                Ok(format!("wrote {}", path))
            }
            None => Ok(m),
        }
    }
}

// ── editing tools ────────────────────────────────────────────────────────────

/// Create a new, valid PROforma 1.9 guideline file with metadata and an empty root plan.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct CreateGuideline {
    /// Path of the .pf file to create
    #[arg(long)]
    pub file: String,
    /// Root plan name (identifier, e.g. chest_pain)
    #[arg(long)]
    pub name: String,
    /// Human-readable title (used for metadata title and root caption)
    #[arg(long)]
    #[serde(default)]
    pub title: Option<String>,
    /// Author for the metadata block
    #[arg(long)]
    #[serde(default)]
    pub author: Option<String>,
    /// Overwrite an existing file
    #[arg(long)]
    #[serde(default)]
    pub overwrite: bool,
}

impl CreateGuideline {
    pub fn run(self) -> ToolResult {
        ensure_identifier(&self.name)?;
        if Path::new(&self.file).exists() && !self.overwrite {
            return Err(ToolError(format!(
                "'{}' already exists (pass overwrite to replace it)",
                self.file
            )));
        }
        let title = self.title.clone().unwrap_or_else(|| self.name.clone());
        let mut fields = vec![(MetaField::Title, title.clone())];
        if let Some(a) = &self.author {
            fields.push((MetaField::Author, a.clone()));
        }
        fields.push((MetaField::Version, "1.0".into()));
        let doc = Document {
            engine_version: Some("1.9".into()),
            directives: None,
            metadata: Some(Metadata {
                name: self.name.clone(),
                fields,
            }),
            metaprops: Some(String::new()),
            items: vec![Item {
                kind: ItemKind::Plan,
                name: self.name.clone(),
                attrs: vec![Attr::Caption(title)],
                span: Default::default(),
            }],
        };
        let mut r = ChangeReport::default();
        r.changes.push(format!("created guideline '{}'", self.name));
        save(&self.file, &doc, r)
    }
}

/// Add or replace whole top-level components from PROforma text, e.g. `decision :: 'triage'; ... end decision.` Several blocks may be given. Existing components with the same name are replaced.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct UpsertComponent {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// One or more complete blocks (plan/decision/action/enquiry/task/data/function ... end X.)
    #[arg(long)]
    pub pf: String,
}

impl UpsertComponent {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let items = parser::parse_items(&self.pf).map_err(|ds| {
            ToolError(format!(
                "invalid PROforma text:\n{}",
                ds.iter()
                    .map(|d| d.render("<pf>"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        })?;
        if items.is_empty() {
            return Err(ToolError("no components found in the given text".into()));
        }
        let mut report = ChangeReport::default();
        for item in items {
            let r = if doc.find(&item.name).is_some() {
                let name = item.name.clone();
                edit::replace_item(&mut doc, &name, item)?
            } else {
                ensure_unique(&doc, &item.name)?;
                edit::add_item(&mut doc, item)?
            };
            report.changes.extend(r.changes);
        }
        save(&self.file, &doc, report)
    }
}

/// Create a new task (definition + component instance) inside a plan, optionally scheduled after other components. Mirrors Composer's "add task".
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct AddTask {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Plan that will contain the task
    #[arg(long)]
    pub plan: String,
    /// Task kind: plan, decision, action, enquiry or task
    #[arg(long)]
    pub kind: String,
    /// Task name (identifier); also used as the component name
    #[arg(long)]
    pub name: String,
    /// Caption (human-readable label)
    #[arg(long)]
    #[serde(default)]
    pub caption: Option<String>,
    /// Description text
    #[arg(long)]
    #[serde(default)]
    pub description: Option<String>,
    /// Components in the same plan that must complete first (schedule constraints)
    #[arg(long = "after")]
    #[serde(default)]
    pub after: Vec<String>,
    /// Precondition expression
    #[arg(long)]
    #[serde(default)]
    pub precondition: Option<String>,
    /// Layout as "left,top" or "left,top,width,height" (default: below the last sibling)
    #[arg(long)]
    #[serde(default)]
    pub ltwh: Option<String>,
}

impl AddTask {
    pub fn run(self) -> ToolResult {
        let kind = parse_kind(&self.kind)?;
        if !kind.is_task() {
            return Err(ToolError(
                "add_task creates tasks; use add_data_item or upsert_component for data items and functions".into(),
            ));
        }
        ensure_identifier(&self.name)?;
        let mut doc = load_for_edit(&self.file)?;
        ensure_unique(&doc, &self.name)?;
        let plan = doc
            .find(&self.plan)
            .ok_or_else(|| ToolError(format!("no plan named '{}'", self.plan)))?;
        if plan.kind != ItemKind::Plan {
            return Err(ToolError(format!("'{}' is not a plan", self.plan)));
        }
        if plan
            .components()
            .any(|c| c.name.eq_ignore_ascii_case(&self.name))
        {
            return Err(ToolError(format!(
                "plan '{}' already has a component named '{}'",
                self.plan, self.name
            )));
        }
        for a in &self.after {
            if !plan.components().any(|c| c.name.eq_ignore_ascii_case(a)) {
                return Err(ToolError(format!(
                    "plan '{}' has no component '{}' to schedule after",
                    self.plan, a
                )));
            }
        }
        let ltwh = match &self.ltwh {
            Some(s) => parse_ltwh(s)?,
            None => next_ltwh(plan),
        };

        let mut def_attrs = Vec::new();
        if let Some(c) = &self.caption {
            def_attrs.push(Attr::Caption(c.clone()));
        }
        if let Some(d) = &self.description {
            def_attrs.push(Attr::Description(d.clone()));
        }
        if let Some(p) = &self.precondition {
            let e = parser::parse_expression(p)
                .map_err(|d| ToolError(format!("invalid precondition: {}", d.message)))?;
            def_attrs.push(Attr::Precondition(e));
        }
        if kind == ItemKind::Decision {
            def_attrs.push(Attr::ChoiceMode("single".into()));
            def_attrs.push(Attr::SupportMode("symbolic".into()));
        }
        let mut comp_attrs = vec![Attr::TaskDefinition(self.name.clone()), Attr::Ltwh(ltwh)];
        for a in &self.after {
            comp_attrs.push(Attr::ScheduleConstraint(a.clone()));
        }
        let plan = doc.find_mut(&self.plan).unwrap();
        plan.attrs.push(Attr::Component(Block {
            name: self.name.clone(),
            attrs: comp_attrs,
            span: Default::default(),
        }));
        let mut report = edit::add_item(
            &mut doc,
            Item {
                kind,
                name: self.name.clone(),
                attrs: def_attrs,
                span: Default::default(),
            },
        )?;
        report.changes.push(format!(
            "added component '{}' to plan '{}'{}",
            self.name,
            self.plan,
            if self.after.is_empty() {
                String::new()
            } else {
                format!(" after {}", self.after.join(", "))
            }
        ));
        save(&self.file, &doc, report)
    }
}

fn parse_ltwh(s: &str) -> Result<[i64; 4], ToolError> {
    let nums: Result<Vec<i64>, _> = s.split(',').map(|x| x.trim().parse::<i64>()).collect();
    match nums {
        Ok(v) if v.len() == 2 => Ok([v[0], v[1], 0, 0]),
        Ok(v) if v.len() == 4 => Ok([v[0], v[1], v[2], v[3]]),
        _ => Err(ToolError(format!(
            "invalid ltwh '{}': expected 'left,top' or 'left,top,width,height'",
            s
        ))),
    }
}

/// Place a new node below the lowest sibling (Composer has no auto-layout).
fn next_ltwh(plan: &Item) -> [i64; 4] {
    let mut max_top = None;
    let mut left = 40;
    for c in plan.components() {
        for a in &c.attrs {
            if let Attr::Ltwh(v) = a {
                if max_top.is_none_or(|m| v[1] > m) {
                    max_top = Some(v[1]);
                    left = v[0];
                }
            }
        }
    }
    [left, max_top.map(|t| t + 100).unwrap_or(40), 0, 0]
}

/// Add a component to a plan that reuses an existing task definition (several components may share one definition).
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct AddComponent {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Plan that will contain the component
    #[arg(long)]
    pub plan: String,
    /// Existing task definition to instantiate
    #[arg(long)]
    pub task_definition: String,
    /// Component name (defaults to the task definition name)
    #[arg(long)]
    #[serde(default)]
    pub name: Option<String>,
    /// Components in the same plan that must complete first
    #[arg(long = "after")]
    #[serde(default)]
    pub after: Vec<String>,
    /// Extra component attributes as PROforma text, e.g. `optional :: yes; cycle_repeat :: 1 days;`
    #[arg(long)]
    #[serde(default)]
    pub pf: Option<String>,
}

impl AddComponent {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let def = doc
            .find(&self.task_definition)
            .ok_or_else(|| ToolError(format!("no task definition '{}'", self.task_definition)))?;
        if !def.kind.is_task() {
            return Err(ToolError(format!(
                "'{}' is not a task",
                self.task_definition
            )));
        }
        if doc.index_of(&self.task_definition) == Some(0) {
            return Err(ToolError(
                "the root plan cannot be used as a component".into(),
            ));
        }
        let name = self.name.clone().unwrap_or(self.task_definition.clone());
        ensure_identifier(&name)?;
        let plan = doc
            .find(&self.plan)
            .filter(|p| p.kind == ItemKind::Plan)
            .ok_or_else(|| ToolError(format!("no plan named '{}'", self.plan)))?;
        if plan
            .components()
            .any(|c| c.name.eq_ignore_ascii_case(&name))
        {
            return Err(ToolError(format!(
                "plan '{}' already has a component named '{}'",
                self.plan, name
            )));
        }
        let ltwh = next_ltwh(plan);
        let mut snippet = format!(
            "component :: {} ; task_definition :: {} ; ltwh :: {},{},{},{} ;",
            q(&name),
            q(&self.task_definition),
            ltwh[0],
            ltwh[1],
            ltwh[2],
            ltwh[3]
        );
        for a in &self.after {
            snippet.push_str(&format!(" schedule_constraint :: completed({}) ;", q(a)));
        }
        if let Some(extra) = &self.pf {
            snippet.push(' ');
            snippet.push_str(extra);
        }
        let r = edit::add_attrs(&mut doc, &self.plan, &snippet)?;
        save(&self.file, &doc, r)
    }
}

/// Remove a top-level component (task, data item or function). By default also removes plan components using it, their scheduling constraints, and sources reading a removed data item; remaining expression references are reported.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct RemoveComponent {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Component name
    #[arg(long)]
    pub name: String,
    /// Keep plan components / sources that reference it (leaves dangling references)
    #[arg(long)]
    #[serde(default)]
    pub no_cascade: bool,
}

impl RemoveComponent {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let r = edit::remove_item(&mut doc, &self.name, !self.no_cascade)?;
        save(&self.file, &doc, r)
    }
}

/// Rename a top-level component and update every reference (components, task_definition, schedule constraints, sources, expressions). A selector `Plan/component:X` renames only that component instance.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct Rename {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Current name, or `Plan/component:Name` for a component instance
    #[arg(long)]
    pub name: String,
    /// New name (identifier)
    #[arg(long)]
    pub new_name: String,
}

impl Rename {
    pub fn run(self) -> ToolResult {
        ensure_identifier(&self.new_name)?;
        let mut doc = load_for_edit(&self.file)?;
        let r =
            match self.name.split_once("/component:") {
                Some((plan, comp)) => edit::rename_component(&mut doc, plan, comp, &self.new_name)?,
                None => {
                    if let Some(existing) = doc.items.iter().find(|i| {
                        i.name.eq_ignore_ascii_case(&self.new_name) && i.name != self.name
                    }) {
                        return Err(ToolError(format!(
                            "name '{}' clashes with existing {} '{}'",
                            self.new_name,
                            existing.kind.keyword(),
                            existing.name
                        )));
                    }
                    edit::rename_item(&mut doc, &self.name, &self.new_name)?
                }
            };
        save(&self.file, &doc, r)
    }
}

/// Set (add or replace) one attribute on a component or nested element. `value` is the PROforma text after `attribute ::` (captions/descriptions may be plain text). Examples: attribute=precondition value="age > 18"; attribute=caption value="Assess patient"; attribute=ltwh value="40,140,0,0"; attribute=choice_mode value=multiple.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct SetAttribute {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Component name or selector (e.g. `triage`, `main/component:triage`, `triage/candidate:urgent`)
    #[arg(long)]
    pub selector: String,
    /// Attribute keyword, e.g. caption, description, precondition, wait_condition, recommendation, type, range, default_value, derivation, unit, mandatory, optional, cycle_repeat, ...
    #[arg(long)]
    pub attribute: String,
    /// Value as PROforma text (without the trailing ';')
    #[arg(long)]
    pub value: String,
}

impl SetAttribute {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let r = edit::set_attr(&mut doc, &self.selector, &self.attribute, &self.value)?;
        save(&self.file, &doc, r)
    }
}

/// Insert attributes or nested elements written as PROforma text into a component, e.g. add a candidate with arguments to a decision, a source to an enquiry, or a component to a plan.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct AddAttributes {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Target component name or selector
    #[arg(long)]
    pub selector: String,
    /// PROforma attribute text, e.g. `candidate :: 'refer'; caption :: 'Refer'; argument :: for, risk = high; recommendation :: netsupport(triage, refer) > 0;`
    #[arg(long)]
    pub pf: String,
}

impl AddAttributes {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let r = edit::add_attrs(&mut doc, &self.selector, &self.pf)?;
        save(&self.file, &doc, r)
    }
}

/// Remove all attributes with a given keyword from a component or nested element (e.g. remove the precondition of a task).
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct RemoveAttribute {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Component name or selector
    #[arg(long)]
    pub selector: String,
    /// Attribute keyword to remove
    #[arg(long)]
    pub attribute: String,
}

impl RemoveAttribute {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let r = edit::remove_attr(&mut doc, &self.selector, &self.attribute)?;
        save(&self.file, &doc, r)
    }
}

/// Remove a nested element addressed by selector: a plan component (`plan/component:X`, also drops sibling constraints on it), candidate, source, argument or warning condition.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct RemoveElement {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Selector of the nested element, e.g. `triage/candidate:refer/argument:2`
    #[arg(long)]
    pub selector: String,
}

impl RemoveElement {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let r = edit::remove_child(&mut doc, &self.selector)?;
        save(&self.file, &doc, r)
    }
}

/// Add or remove a scheduling constraint (diagram arrow): `component` starts only after `after` completes. Both must be components of the same plan; cycles are rejected.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct ScheduleConstraint {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Plan containing both components
    #[arg(long)]
    pub plan: String,
    /// Dependent component
    #[arg(long)]
    pub component: String,
    /// Component that must complete first
    #[arg(long)]
    pub after: String,
    /// Remove the constraint instead of adding it
    #[arg(long)]
    #[serde(default)]
    pub remove: bool,
}

impl ScheduleConstraint {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let plan = doc
            .find(&self.plan)
            .filter(|p| p.kind == ItemKind::Plan)
            .ok_or_else(|| ToolError(format!("no plan named '{}'", self.plan)))?;
        let comps: Vec<&Block> = plan.components().collect();
        let find = |n: &str| {
            comps
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(n))
                .copied()
        };
        let target = find(&self.component).ok_or_else(|| {
            ToolError(format!(
                "plan '{}' has no component '{}'",
                self.plan, self.component
            ))
        })?;
        let before = find(&self.after).ok_or_else(|| {
            ToolError(format!(
                "plan '{}' has no component '{}'",
                self.plan, self.after
            ))
        })?;
        let (target_name, before_name) = (target.name.clone(), before.name.clone());
        let exists = target
            .schedule_constraints()
            .any(|s| s.eq_ignore_ascii_case(&before_name));
        let mut r = ChangeReport::default();
        if self.remove {
            if !exists {
                return Err(ToolError(format!(
                    "'{}' has no constraint completed({})",
                    target_name, before_name
                )));
            }
        } else {
            if exists {
                return Err(ToolError("constraint already exists".into()));
            }
            if target_name == before_name {
                return Err(ToolError("a component cannot wait for itself".into()));
            }
            // reject cycles: does `before` (transitively) wait for `target`?
            let mut stack = vec![before_name.clone()];
            let mut seen = std::collections::HashSet::new();
            while let Some(n) = stack.pop() {
                if n.eq_ignore_ascii_case(&target_name) {
                    return Err(ToolError(format!(
                        "adding completed({}) to '{}' would create a scheduling cycle",
                        before_name, target_name
                    )));
                }
                if !seen.insert(n.to_lowercase()) {
                    continue;
                }
                if let Some(c) = find(&n) {
                    stack.extend(c.schedule_constraints().map(str::to_string));
                }
            }
        }
        let plan = doc.find_mut(&self.plan).unwrap();
        for a in plan.attrs.iter_mut() {
            if let Attr::Component(c) = a {
                if c.name == target_name {
                    if self.remove {
                        c.attrs.retain(|x| {
                            !matches!(x, Attr::ScheduleConstraint(s) if s.eq_ignore_ascii_case(&before_name))
                        });
                        r.changes.push(format!(
                            "removed constraint: '{}' no longer waits for '{}'",
                            target_name, before_name
                        ));
                    } else {
                        c.attrs.push(Attr::ScheduleConstraint(before_name.clone()));
                        r.changes.push(format!(
                            "'{}' now starts after '{}' completes",
                            target_name, before_name
                        ));
                    }
                }
            }
        }
        save(&self.file, &doc, r)
    }
}

/// Add a candidate (option) to a decision. Default recommendation rule is `netsupport() > 0` (as in Composer).
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct AddCandidate {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Decision name
    #[arg(long)]
    pub decision: String,
    /// Candidate name (identifier)
    #[arg(long)]
    pub name: String,
    /// Caption (human-readable option text)
    #[arg(long)]
    #[serde(default)]
    pub caption: Option<String>,
    /// Recommendation expression (default `netsupport() > 0`)
    #[arg(long)]
    #[serde(default)]
    pub recommendation: Option<String>,
    /// Priority (integer)
    #[arg(long)]
    #[serde(default)]
    pub priority: Option<i64>,
}

impl AddCandidate {
    pub fn run(self) -> ToolResult {
        ensure_identifier(&self.name)?;
        let mut doc = load_for_edit(&self.file)?;
        let dec = doc
            .find(&self.decision)
            .filter(|d| d.kind == ItemKind::Decision)
            .ok_or_else(|| ToolError(format!("no decision named '{}'", self.decision)))?;
        if dec
            .candidates()
            .any(|c| c.name.eq_ignore_ascii_case(&self.name))
        {
            return Err(ToolError(format!(
                "decision '{}' already has a candidate '{}'",
                self.decision, self.name
            )));
        }
        let mut s = format!("candidate :: {} ;", q(&self.name));
        if let Some(c) = &self.caption {
            s.push_str(&format!(" caption :: {} ;", q(c)));
        }
        s.push_str(&format!(
            " recommendation :: {} ;",
            self.recommendation.as_deref().unwrap_or("netsupport() > 0")
        ));
        if let Some(p) = self.priority {
            s.push_str(&format!(" priority :: {} ;", p));
        }
        let r = edit::add_attrs(&mut doc, &self.decision, &s)?;
        save(&self.file, &doc, r)
    }
}

/// Add an argument (rule) to a decision candidate. In symbolic decisions support is for, against, confirming or excluding; in numeric decisions it is a weight expression.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct AddArgument {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Decision name
    #[arg(long)]
    pub decision: String,
    /// Candidate name
    #[arg(long)]
    pub candidate: String,
    /// Support: for, against, confirming, excluding (symbolic) or a number/expression (numeric)
    #[arg(long)]
    pub support: String,
    /// Condition expression under which the argument applies
    #[arg(long)]
    pub condition: String,
    /// Optional argument name
    #[arg(long)]
    #[serde(default)]
    pub name: Option<String>,
    /// Optional caption (justification text)
    #[arg(long)]
    #[serde(default)]
    pub caption: Option<String>,
}

impl AddArgument {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let dec = doc
            .find(&self.decision)
            .filter(|d| d.kind == ItemKind::Decision)
            .ok_or_else(|| ToolError(format!("no decision named '{}'", self.decision)))?;
        let numeric = dec
            .attrs
            .iter()
            .any(|a| matches!(a, Attr::SupportMode(m) if m == "numeric"));
        let support = self.support.trim();
        let symbolic = matches!(support, "for" | "against" | "confirming" | "excluding");
        if numeric && symbolic {
            return Err(ToolError(format!(
                "decision '{}' uses numeric support: give a weight (e.g. 1, -1, 1000) instead of '{}'",
                self.decision, support
            )));
        }
        if !numeric && !symbolic {
            return Err(ToolError(format!(
                "decision '{}' uses symbolic support: use for, against, confirming or excluding (or set support_mode to numeric first)",
                self.decision
            )));
        }
        let mut s = format!("argument :: {}, {}", support, self.condition.trim());
        if self.name.is_some() || self.caption.is_some() {
            s.push_str(" attributes");
            if let Some(n) = &self.name {
                s.push_str(&format!(" argument_name :: {} ;", q(n)));
            }
            if let Some(c) = &self.caption {
                s.push_str(&format!(" caption :: {} ;", q(c)));
            }
            s.push_str(" end attributes");
        }
        s.push_str(" ;");
        let sel = format!("{}/candidate:{}", self.decision, self.candidate);
        let r = edit::add_attrs(&mut doc, &sel, &s)?;
        save(&self.file, &doc, r)
    }
}

/// Add a data source (a data item to be requested) to an enquiry or decision. Optionally creates the data item.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct AddSource {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Enquiry (or decision) name
    #[arg(long)]
    pub task: String,
    /// Data item to request
    #[arg(long)]
    pub data: String,
    /// Whether a value is mandatory (default yes)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    #[serde(default = "default_true")]
    pub mandatory: bool,
    /// Create the data item with this type if it does not exist (text, integer, real, boolean, date, ...)
    #[arg(long)]
    #[serde(default)]
    pub create_type: Option<String>,
}

fn default_true() -> bool {
    true
}

impl AddSource {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let task = doc
            .find(&self.task)
            .ok_or_else(|| ToolError(format!("no component named '{}'", self.task)))?;
        if !matches!(task.kind, ItemKind::Enquiry | ItemKind::Decision) {
            return Err(ToolError(
                "sources can only be added to enquiries and decisions".into(),
            ));
        }
        if task.sources().any(|s| s.data_definition() == self.data) {
            return Err(ToolError(format!(
                "'{}' already has a source for '{}'",
                self.task, self.data
            )));
        }
        let mut report = ChangeReport::default();
        match doc.find(&self.data) {
            Some(d) if d.kind != ItemKind::Data => {
                return Err(ToolError(format!("'{}' is not a data item", self.data)))
            }
            Some(d) => {
                if d.attrs.iter().any(|a| matches!(a, Attr::Dynamic(true))) {
                    return Err(ToolError(format!(
                        "'{}' is a dynamic (derived) data item and cannot be a source",
                        self.data
                    )));
                }
            }
            None => match &self.create_type {
                Some(t) => {
                    ensure_identifier(&self.data)?;
                    ensure_unique(&doc, &self.data)?;
                    let attrs = parser::parse_attrs(
                        parser::SnippetContext::Item(ItemKind::Data),
                        &format!("type :: {} ; caption :: {} ;", t, q(&self.data)),
                    )
                    .map_err(|d| ToolError(d.message))?;
                    report.changes.extend(
                        edit::add_item(
                            &mut doc,
                            Item {
                                kind: ItemKind::Data,
                                name: self.data.clone(),
                                attrs,
                                span: Default::default(),
                            },
                        )?
                        .changes,
                    );
                }
                None => {
                    return Err(ToolError(format!(
                        "no data item '{}' (pass create_type to create it)",
                        self.data
                    )))
                }
            },
        }
        let s = format!(
            "source :: {} ; data_definition :: {} ; mandatory :: {} ;",
            q(&self.data),
            q(&self.data),
            if self.mandatory { "yes" } else { "no" }
        );
        report
            .changes
            .extend(edit::add_attrs(&mut doc, &self.task, &s)?.changes);
        save(&self.file, &doc, report)
    }
}

/// Add a data item (patient/clinical variable) with type, caption and optional range, default, derivation or unit.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct AddDataItem {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Data item name (identifier)
    #[arg(long)]
    pub name: String,
    /// Type: text, integer, real, boolean, date, time, datetime, setof_text, setof_integer, setof_real
    #[arg(long = "type")]
    #[serde(rename = "type")]
    pub data_type: String,
    /// Caption (question / label shown to users)
    #[arg(long)]
    #[serde(default)]
    pub caption: Option<String>,
    /// Allowed values as PROforma text, e.g. `"low", "high"` or `0, 120`
    #[arg(long)]
    #[serde(default)]
    pub range: Option<String>,
    /// Default value expression
    #[arg(long)]
    #[serde(default)]
    pub default_value: Option<String>,
    /// Derivation expression (makes the item derived/computed)
    #[arg(long)]
    #[serde(default)]
    pub derivation: Option<String>,
    /// Unit, e.g. kg
    #[arg(long)]
    #[serde(default)]
    pub unit: Option<String>,
}

impl AddDataItem {
    pub fn run(self) -> ToolResult {
        ensure_identifier(&self.name)?;
        let mut doc = load_for_edit(&self.file)?;
        ensure_unique(&doc, &self.name)?;
        let mut s = format!("type :: {} ;", self.data_type);
        s.push_str(&format!(
            " caption :: {} ;",
            q(self.caption.as_deref().unwrap_or(&self.name))
        ));
        if let Some(r) = &self.range {
            s.push_str(&format!(" range :: {} ;", r));
        }
        if let Some(d) = &self.default_value {
            s.push_str(&format!(" default_value :: {} ;", d));
        }
        if let Some(d) = &self.derivation {
            s.push_str(&format!(" derivation :: {} ;", d));
        }
        if let Some(u) = &self.unit {
            s.push_str(&format!(" unit :: {} ;", edit::quote_atom(u)));
        }
        let attrs = parser::parse_attrs(parser::SnippetContext::Item(ItemKind::Data), &s)
            .map_err(|d| ToolError(format!("invalid data item: {}", d.message)))?;
        let r = edit::add_item(
            &mut doc,
            Item {
                kind: ItemKind::Data,
                name: self.name.clone(),
                attrs,
                span: Default::default(),
            },
        )?;
        save(&self.file, &doc, r)
    }
}

/// Set guideline metadata fields (title, desc, author, version, cdate, mdate) and/or the engine version.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct SetMetadata {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    #[arg(long)]
    #[serde(default)]
    pub title: Option<String>,
    #[arg(long)]
    #[serde(default)]
    pub desc: Option<String>,
    #[arg(long)]
    #[serde(default)]
    pub author: Option<String>,
    #[arg(long)]
    #[serde(default)]
    pub version: Option<String>,
    #[arg(long)]
    #[serde(default)]
    pub cdate: Option<String>,
    #[arg(long)]
    #[serde(default)]
    pub mdate: Option<String>,
}

impl SetMetadata {
    pub fn run(self) -> ToolResult {
        let mut doc = load_for_edit(&self.file)?;
        let root = doc.root().map(|r| r.name.clone()).unwrap_or_default();
        let md = doc.metadata.get_or_insert_with(|| Metadata {
            name: root,
            fields: vec![],
        });
        let mut r = ChangeReport::default();
        for (f, v) in [
            (MetaField::Title, &self.title),
            (MetaField::Desc, &self.desc),
            (MetaField::Author, &self.author),
            (MetaField::Version, &self.version),
            (MetaField::Cdate, &self.cdate),
            (MetaField::Mdate, &self.mdate),
        ] {
            if let Some(v) = v {
                match md.fields.iter_mut().find(|(k, _)| *k == f) {
                    Some(slot) => slot.1 = v.clone(),
                    None => md.fields.push((f, v.clone())),
                }
                r.changes
                    .push(format!("set metadata {} = '{}'", f.as_str(), v));
            }
        }
        if r.changes.is_empty() {
            return Err(ToolError("no metadata fields given".into()));
        }
        save(&self.file, &doc, r)
    }
}

/// Rewrite a file in Composer's canonical layout (and upgrade legacy files to engine_version 1.9). With `check`, only report whether the file is already formatted.
#[derive(Debug, Clone, Args, Deserialize, Serialize, JsonSchema)]
pub struct Format {
    /// Path to the .pf file
    #[arg(long)]
    pub file: String,
    /// Only check; do not modify the file
    #[arg(long)]
    #[serde(default)]
    pub check: bool,
    /// Return the formatted text instead of writing the file
    #[arg(long)]
    #[serde(default)]
    pub print: bool,
}

impl Format {
    pub fn run(self) -> ToolResult {
        let src = read(&self.file)?;
        let doc = load_for_edit(&self.file)?;
        let text = writer::write(&doc);
        if self.print {
            return Ok(text);
        }
        if self.check {
            return Ok(if text == src {
                "already formatted".into()
            } else {
                "not formatted (run format to rewrite)".into()
            });
        }
        if text == src {
            return Ok("already formatted; no changes".into());
        }
        let mut r = ChangeReport::default();
        r.changes.push("reformatted".into());
        save(&self.file, &doc, r)
    }
}
