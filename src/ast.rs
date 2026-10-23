//! Syntax tree for PROforma guidelines.
//!
//! Every block (task, data item, function, and nested component / candidate /
//! source / argument / warning) keeps its attributes as an ordered list so the
//! document can be inspected, edited and re-serialized without losing anything
//! the grammar can express.

use serde::Serialize;

use crate::lexer::Span;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Document {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directives: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Guideline-level `metaprops` string (required by the reference grammar).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metaprops: Option<String>,
    /// Top-level components in file order. The first one is the root plan.
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metadata {
    pub name: String,
    /// (field, value) pairs in file order: title, desc, author, version, cdate, mdate.
    pub fields: Vec<(MetaField, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetaField {
    Title,
    Desc,
    Author,
    Version,
    Cdate,
    Mdate,
}

impl MetaField {
    pub fn as_str(self) -> &'static str {
        match self {
            MetaField::Title => "title",
            MetaField::Desc => "desc",
            MetaField::Author => "author",
            MetaField::Version => "version",
            MetaField::Cdate => "cdate",
            MetaField::Mdate => "mdate",
        }
    }

    pub fn parse(s: &str) -> Option<MetaField> {
        Some(match s {
            "title" => MetaField::Title,
            "desc" => MetaField::Desc,
            "author" => MetaField::Author,
            "version" => MetaField::Version,
            "cdate" => MetaField::Cdate,
            "mdate" => MetaField::Mdate,
            _ => return None,
        })
    }
}

/// Kind of a top-level component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Plan,
    Decision,
    Action,
    Enquiry,
    Task,
    Data,
    Function,
}

impl ItemKind {
    pub fn keyword(self) -> &'static str {
        match self {
            ItemKind::Plan => "plan",
            ItemKind::Decision => "decision",
            ItemKind::Action => "action",
            ItemKind::Enquiry => "enquiry",
            ItemKind::Task => "task",
            ItemKind::Data => "data",
            ItemKind::Function => "function",
        }
    }

    pub fn parse(s: &str) -> Option<ItemKind> {
        Some(match s.to_ascii_lowercase().as_str() {
            "plan" => ItemKind::Plan,
            "decision" => ItemKind::Decision,
            "action" => ItemKind::Action,
            "enquiry" => ItemKind::Enquiry,
            "task" | "generic_task" | "keystone" => ItemKind::Task,
            "data" | "data_item" => ItemKind::Data,
            "function" => ItemKind::Function,
            _ => return None,
        })
    }

    pub fn is_task(self) -> bool {
        !matches!(self, ItemKind::Data | ItemKind::Function)
    }

    pub const ALL: [ItemKind; 7] = [
        ItemKind::Plan,
        ItemKind::Decision,
        ItemKind::Action,
        ItemKind::Enquiry,
        ItemKind::Task,
        ItemKind::Data,
        ItemKind::Function,
    ];
}

/// A top-level component: task (plan/decision/action/enquiry/task), data item or function.
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub kind: ItemKind,
    pub name: String,
    pub attrs: Vec<Attr>,
    #[serde(skip)]
    pub span: Span,
}

/// Nested block that has a name followed by its own attribute list
/// (component, candidate, source).
#[derive(Debug, Clone, Serialize)]
pub struct Block {
    pub name: String,
    pub attrs: Vec<Attr>,
    #[serde(skip)]
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Argument {
    pub support: Support,
    pub condition: Expr,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Generic attributes (caption/description/metaprops) inside `attributes ... end attributes`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attr>,
    /// Whether an `attributes ... end attributes` section was present.
    #[serde(skip)]
    pub has_attributes_section: bool,
    #[serde(skip)]
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "weight", rename_all = "lowercase")]
pub enum Support {
    For,
    Against,
    Confirming,
    Excluding,
    /// Numeric weight / arbitrary expression (numeric support mode).
    Weight(Expr),
}

impl Support {
    pub fn label(&self) -> String {
        match self {
            Support::For => "for".into(),
            Support::Against => "against".into(),
            Support::Confirming => "confirming".into(),
            Support::Excluding => "excluding".into(),
            Support::Weight(e) => e.text.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Warning {
    pub flag: Constant,
    pub condition: Expr,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attr>,
    #[serde(skip)]
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Parameter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attr>,
    #[serde(skip)]
    pub has_attributes_section: bool,
}

/// `true_value`, `false_value`, `unit`, warning flag: a number, string or atom.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum Constant {
    Atom(String),
    String(String),
    Number(String),
}

impl Constant {
    pub fn value(&self) -> &str {
        match self {
            Constant::Atom(s) | Constant::String(s) | Constant::Number(s) => s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Weeks,
}

impl TimeUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            TimeUnit::Seconds => "seconds",
            TimeUnit::Minutes => "minutes",
            TimeUnit::Hours => "hours",
            TimeUnit::Days => "days",
            TimeUnit::Weeks => "weeks",
        }
    }
}

/// An attribute inside a block. Variants are grouped by the block(s) where the
/// reference grammar permits them.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "attr", content = "value", rename_all = "snake_case")]
pub enum Attr {
    // generic (any task / data / function / candidate / source / argument / warning)
    Caption(String),
    Description(String),
    Metaprops(String),

    // task attributes (all task kinds)
    Precondition(Expr),
    WaitCondition(Expr),
    Postcondition(Expr),
    Goal(Expr),
    Trigger(String),
    Context(String),
    Parameters(Vec<Parameter>),

    // plan
    Component(Block),
    Abort(Expr),
    Terminate(Expr),

    // action
    Procedure(String),

    // decision
    Candidate(Block),
    ChoiceMode(String),
    PreselectCandidates(bool),
    SupportMode(String),

    // decision / enquiry
    Source(Block),

    // data item
    Type(String),
    Range(Vec<Expr>),
    DefaultValue(Expr),
    TrueValue(Constant),
    FalseValue(Constant),
    MandatoryValidation(Expr),
    Derivation(Expr),
    Dynamic(bool),
    WarningCondition(Warning),
    Unit(Constant),

    // function
    Auto(bool),
    FunctionTrigger(Expr),
    FunctionBody(Expr),

    // component (task instance inside a plan)
    TaskDefinition(String),
    Autonomous(bool),
    Optional(bool),
    Terminal(bool),
    ParamValue(Expr),
    ScheduleConstraint(String),
    Ltwh([i64; 4]),
    NumberOfCycles(Expr),
    CycleUntil(Expr),
    CycleRepeat(Expr, TimeUnit),

    // candidate
    Argument(Argument),
    Recommendation(Expr),
    Priority(i64),
    Group(String),

    // source
    DataDefinition(String),
    Mandatory(bool),
    Dynamic483(bool),
    Derivation483(Expr),
}

impl Attr {
    /// The keyword that introduces this attribute in `.pf` text.
    pub fn keyword(&self) -> &'static str {
        match self {
            Attr::Caption(_) => "caption",
            Attr::Description(_) => "description",
            Attr::Metaprops(_) => "metaprops",
            Attr::Precondition(_) => "precondition",
            Attr::WaitCondition(_) => "wait_condition",
            Attr::Postcondition(_) => "postcondition",
            Attr::Goal(_) => "goal",
            Attr::Trigger(_) => "trigger",
            Attr::Context(_) => "context",
            Attr::Parameters(_) => "parameters",
            Attr::Component(_) => "component",
            Attr::Abort(_) => "abort",
            Attr::Terminate(_) => "terminate",
            Attr::Procedure(_) => "procedure",
            Attr::Candidate(_) => "candidate",
            Attr::ChoiceMode(_) => "choice_mode",
            Attr::PreselectCandidates(_) => "preselect_candidates",
            Attr::SupportMode(_) => "support_mode",
            Attr::Source(_) => "source",
            Attr::Type(_) => "type",
            Attr::Range(_) => "range",
            Attr::DefaultValue(_) => "default_value",
            Attr::TrueValue(_) => "true_value",
            Attr::FalseValue(_) => "false_value",
            Attr::MandatoryValidation(_) => "mandatory_validation",
            Attr::Derivation(_) => "derivation",
            Attr::Dynamic(_) => "dynamic",
            Attr::WarningCondition(_) => "warning_condition",
            Attr::Unit(_) => "unit",
            Attr::Auto(_) => "auto",
            Attr::FunctionTrigger(_) => "function_trigger",
            Attr::FunctionBody(_) => "function_body",
            Attr::TaskDefinition(_) => "task_definition",
            Attr::Autonomous(_) => "autonomous",
            Attr::Optional(_) => "optional",
            Attr::Terminal(_) => "terminal",
            Attr::ParamValue(_) => "param_value",
            Attr::ScheduleConstraint(_) => "schedule_constraint",
            Attr::Ltwh(_) => "ltwh",
            Attr::NumberOfCycles(_) => "number_of_cycles",
            Attr::CycleUntil(_) => "cycle_until",
            Attr::CycleRepeat(..) => "cycle_repeat",
            Attr::Argument(_) => "argument",
            Attr::Recommendation(_) => "recommendation",
            Attr::Priority(_) => "priority",
            Attr::Group(_) => "group",
            Attr::DataDefinition(_) => "data_definition",
            Attr::Mandatory(_) => "mandatory",
            Attr::Dynamic483(_) => "dynamic483",
            Attr::Derivation483(_) => "derivation483",
        }
    }

    pub fn is_generic(&self) -> bool {
        matches!(
            self,
            Attr::Caption(_) | Attr::Description(_) | Attr::Metaprops(_)
        )
    }

    /// Attributes that may legitimately repeat inside one block.
    pub fn is_repeatable(&self) -> bool {
        matches!(
            self,
            Attr::Component(_)
                | Attr::Candidate(_)
                | Attr::Source(_)
                | Attr::Argument(_)
                | Attr::WarningCondition(_)
                | Attr::ScheduleConstraint(_)
                | Attr::ParamValue(_)
        )
    }

    /// Every expression directly held by this attribute (not nested blocks).
    pub fn exprs(&self) -> Vec<&Expr> {
        match self {
            Attr::Precondition(e)
            | Attr::WaitCondition(e)
            | Attr::Postcondition(e)
            | Attr::Goal(e)
            | Attr::Abort(e)
            | Attr::Terminate(e)
            | Attr::DefaultValue(e)
            | Attr::MandatoryValidation(e)
            | Attr::Derivation(e)
            | Attr::FunctionTrigger(e)
            | Attr::FunctionBody(e)
            | Attr::ParamValue(e)
            | Attr::NumberOfCycles(e)
            | Attr::CycleUntil(e)
            | Attr::CycleRepeat(e, _)
            | Attr::Recommendation(e)
            | Attr::Derivation483(e) => vec![e],
            Attr::Range(v) => v.iter().collect(),
            Attr::WarningCondition(w) => vec![&w.condition],
            Attr::Argument(a) => {
                let mut v = vec![&a.condition];
                if let Support::Weight(e) = &a.support {
                    v.push(e);
                }
                v
            }
            _ => vec![],
        }
    }

    pub fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            Attr::Precondition(e)
            | Attr::WaitCondition(e)
            | Attr::Postcondition(e)
            | Attr::Goal(e)
            | Attr::Abort(e)
            | Attr::Terminate(e)
            | Attr::DefaultValue(e)
            | Attr::MandatoryValidation(e)
            | Attr::Derivation(e)
            | Attr::FunctionTrigger(e)
            | Attr::FunctionBody(e)
            | Attr::ParamValue(e)
            | Attr::NumberOfCycles(e)
            | Attr::CycleUntil(e)
            | Attr::CycleRepeat(e, _)
            | Attr::Recommendation(e)
            | Attr::Derivation483(e) => vec![e],
            Attr::Range(v) => v.iter_mut().collect(),
            Attr::WarningCondition(w) => vec![&mut w.condition],
            Attr::Argument(a) => {
                let mut v = vec![&mut a.condition];
                if let Support::Weight(e) = &mut a.support {
                    v.push(e);
                }
                v
            }
            _ => vec![],
        }
    }

    /// Nested attribute lists (component/candidate/source/argument/warning/parameter attrs).
    pub fn children(&self) -> Option<&Vec<Attr>> {
        match self {
            Attr::Component(b) | Attr::Candidate(b) | Attr::Source(b) => Some(&b.attrs),
            Attr::Argument(a) => Some(&a.attrs),
            Attr::WarningCondition(w) => Some(&w.attrs),
            _ => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<Attr>> {
        match self {
            Attr::Component(b) | Attr::Candidate(b) | Attr::Source(b) => Some(&mut b.attrs),
            Attr::Argument(a) => Some(&mut a.attrs),
            Attr::WarningCondition(w) => Some(&mut w.attrs),
            _ => None,
        }
    }
}

/// An expression: original source text plus its parsed tree.
#[derive(Debug, Clone, Serialize)]
pub struct Expr {
    pub text: String,
    #[serde(skip)]
    pub node: Node,
    #[serde(skip)]
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BinOp {
    Or,
    And,
    Assign,
    Lt,
    LtEq,
    Gt,
    GtEq,
    NotEquals,
    Equals,
    Includes,
    IncludesAll,
    IncludesAny,
    Oneof,
    Matches,
    MatchesAll,
    MatchesAny,
    MatchesRx,
    MatchesRxAll,
    MatchesRxAny,
    MatchesIRx,
    MatchesIRxAll,
    MatchesIRxAny,
    TermIsa,
    TermIncludes,
    TermIncludesAll,
    TermIncludesAny,
    TermIntersectGeneric,
    TermAssign,
    TermOccurences,
    Append,
    Plus,
    Minus,
    Times,
    Divide,
    AttrOper,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Or => "or",
            BinOp::And => "and",
            BinOp::Assign => ":=",
            BinOp::Lt => "<",
            BinOp::LtEq => "<=",
            BinOp::Gt => ">",
            BinOp::GtEq => ">=",
            BinOp::NotEquals => "!=",
            BinOp::Equals => "=",
            BinOp::Includes => "includes",
            BinOp::IncludesAll => "includesAll",
            BinOp::IncludesAny => "includesAny",
            BinOp::Oneof => "oneof",
            BinOp::Matches => "matches",
            BinOp::MatchesAll => "matchesAll",
            BinOp::MatchesAny => "matchesAny",
            BinOp::MatchesRx => "matchesRx",
            BinOp::MatchesRxAll => "matchesRxAll",
            BinOp::MatchesRxAny => "matchesRxAny",
            BinOp::MatchesIRx => "matchesIRx",
            BinOp::MatchesIRxAll => "matchesIRxAll",
            BinOp::MatchesIRxAny => "matchesIRxAny",
            BinOp::TermIsa => "term_isa",
            BinOp::TermIncludes => "term_includes",
            BinOp::TermIncludesAll => "term_includesAll",
            BinOp::TermIncludesAny => "term_includesAny",
            BinOp::TermIntersectGeneric => "term_intersectGeneric",
            BinOp::TermAssign => "term_assign",
            BinOp::TermOccurences => "term_occurences",
            BinOp::Append => "++",
            BinOp::Plus => "+",
            BinOp::Minus => "-",
            BinOp::Times => "*",
            BinOp::Divide => "/",
            BinOp::AttrOper => "->",
        }
    }

    /// Precedence level following the reference grammar (higher binds tighter).
    pub fn level(self) -> u8 {
        use BinOp::*;
        match self {
            Or => 1,
            And => 2,
            Assign | Lt | LtEq | Gt | GtEq | NotEquals | Equals => 3,
            Includes | IncludesAll | IncludesAny | Oneof => 4,
            Matches | MatchesAll | MatchesAny => 5,
            MatchesRx | MatchesRxAll | MatchesRxAny => 6,
            MatchesIRx | MatchesIRxAll | MatchesIRxAny => 7,
            TermIsa | TermIncludes | TermIncludesAll | TermIncludesAny | TermIntersectGeneric
            | TermAssign | TermOccurences => 8,
            Append => 9,
            Plus | Minus => 10,
            Times | Divide => 11,
            AttrOper => 13,
        }
    }

    pub fn is_left_assoc(self) -> bool {
        matches!(
            self,
            BinOp::Or
                | BinOp::And
                | BinOp::Append
                | BinOp::Plus
                | BinOp::Minus
                | BinOp::Times
                | BinOp::Divide
                | BinOp::AttrOper
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Node {
    #[default]
    Empty,
    /// Identifier or quoted atom (data item, task, parameter, enumerated value...).
    Atom(String),
    /// `a:b` complex atom.
    Complex(String, String),
    Int(i64),
    Real(String),
    Str(String),
    Date(String),
    DateTime(String),
    Time(String),
    ResultOf(String),
    Call(String, Vec<Node>),
    List(Vec<Node>),
    Neg(Box<Node>),
    Paren(Box<Node>),
    Bin(BinOp, Box<Node>, Box<Node>),
}

impl Node {
    /// Visit every node depth-first.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Node)) {
        f(self);
        match self {
            Node::Call(_, args) | Node::List(args) => args.iter().for_each(|a| a.walk(f)),
            Node::Neg(n) | Node::Paren(n) => n.walk(f),
            Node::Bin(_, a, b) => {
                a.walk(f);
                b.walk(f);
            }
            _ => {}
        }
    }
}

impl Document {
    pub fn root(&self) -> Option<&Item> {
        self.items.first()
    }

    pub fn find(&self, name: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.name == name)
    }

    pub fn find_mut(&mut self, name: &str) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| i.name == name)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.items.iter().position(|i| i.name == name)
    }
}

/// Helpers shared by items and nested blocks for reading attributes.
pub trait Attrs {
    fn attrs(&self) -> &Vec<Attr>;

    fn caption(&self) -> Option<&str> {
        self.attrs().iter().find_map(|a| match a {
            Attr::Caption(s) => Some(s.as_str()),
            _ => None,
        })
    }

    fn description(&self) -> Option<&str> {
        self.attrs().iter().find_map(|a| match a {
            Attr::Description(s) => Some(s.as_str()),
            _ => None,
        })
    }

    fn find_attr(&self, keyword: &str) -> Option<&Attr> {
        self.attrs().iter().find(|a| a.keyword() == keyword)
    }
}

impl Attrs for Item {
    fn attrs(&self) -> &Vec<Attr> {
        &self.attrs
    }
}

impl Attrs for Block {
    fn attrs(&self) -> &Vec<Attr> {
        &self.attrs
    }
}

impl Item {
    pub fn data_type(&self) -> Option<&str> {
        self.attrs.iter().find_map(|a| match a {
            Attr::Type(t) => Some(t.as_str()),
            _ => None,
        })
    }

    pub fn components(&self) -> impl Iterator<Item = &Block> {
        self.attrs.iter().filter_map(|a| match a {
            Attr::Component(b) => Some(b),
            _ => None,
        })
    }

    pub fn candidates(&self) -> impl Iterator<Item = &Block> {
        self.attrs.iter().filter_map(|a| match a {
            Attr::Candidate(b) => Some(b),
            _ => None,
        })
    }

    pub fn sources(&self) -> impl Iterator<Item = &Block> {
        self.attrs.iter().filter_map(|a| match a {
            Attr::Source(b) => Some(b),
            _ => None,
        })
    }
}

impl Block {
    /// For a plan component: the referenced task definition (defaults to the component name).
    pub fn task_definition(&self) -> &str {
        self.attrs
            .iter()
            .find_map(|a| match a {
                Attr::TaskDefinition(t) => Some(t.as_str()),
                _ => None,
            })
            .unwrap_or(&self.name)
    }

    pub fn schedule_constraints(&self) -> impl Iterator<Item = &str> {
        self.attrs.iter().filter_map(|a| match a {
            Attr::ScheduleConstraint(s) => Some(s.as_str()),
            _ => None,
        })
    }

    pub fn arguments(&self) -> impl Iterator<Item = &Argument> {
        self.attrs.iter().filter_map(|a| match a {
            Attr::Argument(x) => Some(x),
            _ => None,
        })
    }

    /// For a source: the referenced data definition (defaults to the source name).
    pub fn data_definition(&self) -> &str {
        self.attrs
            .iter()
            .find_map(|a| match a {
                Attr::DataDefinition(t) => Some(t.as_str()),
                _ => None,
            })
            .unwrap_or(&self.name)
    }
}
