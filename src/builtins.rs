//! Expression types and built-in function signatures, as the engine's verifier
//! types them.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ty {
    Untypeable,
    Any,
    Text,
    Integer,
    Real,
    Truth,
    SetText,
    SetInteger,
    SetReal,
    SetAny,
    Date,
    Time,
    DateTime,
    Object,
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Ty::Untypeable => "untypeable",
            Ty::Any => "any_type",
            Ty::Text => "text",
            Ty::Integer => "integer",
            Ty::Real => "real",
            Ty::Truth => "truth_value",
            Ty::SetText => "setof_text",
            Ty::SetInteger => "setof_integer",
            Ty::SetReal => "setof_real",
            Ty::SetAny => "setof_anything",
            Ty::Date => "date",
            Ty::Time => "time",
            Ty::DateTime => "datetime",
            Ty::Object => "object",
        })
    }
}

impl Ty {
    /// Type of a declared data item / function / parameter type name.
    /// Note: the engine types `boolean` data as `text`.
    pub fn from_declared(t: &str) -> Option<Ty> {
        Some(match t {
            "text" | "boolean" => Ty::Text,
            "integer" => Ty::Integer,
            "real" => Ty::Real,
            "date" => Ty::Date,
            "time" => Ty::Time,
            "datetime" => Ty::DateTime,
            "setof_text" => Ty::SetText,
            "setof_integer" => Ty::SetInteger,
            "setof_real" => Ty::SetReal,
            "object" => Ty::Object,
            _ => return None,
        })
    }

    /// Element type of a set type (used for range elements).
    pub fn element(self) -> Ty {
        match self {
            Ty::SetText => Ty::Text,
            Ty::SetInteger => Ty::Integer,
            Ty::SetReal => Ty::Real,
            Ty::SetAny => Ty::Any,
            other => other,
        }
    }
}

/// `canPromote(actual, expected)`.
pub fn can_promote(actual: Ty, expected: Ty) -> bool {
    actual == expected
        || expected == Ty::Any
        || (actual == Ty::Integer && expected == Ty::Real)
        || (actual == Ty::SetInteger && expected == Ty::SetReal)
        || (actual == Ty::SetAny && matches!(expected, Ty::SetInteger | Ty::SetReal | Ty::SetText))
}

use Ty::*;

/// BuiltIn signatures in declaration order: (functor, argument types, return type).
pub const SIGNATURES: &[(&str, &[Ty], Ty)] = &[
    ("+", &[Integer, Integer], Integer),
    ("+", &[Real, Real], Real),
    ("-", &[Integer, Integer], Integer),
    ("-", &[Real, Real], Real),
    ("-", &[Integer], Integer),
    ("-", &[Real], Real),
    ("*", &[Integer, Integer], Integer),
    ("*", &[Real, Real], Real),
    ("/", &[Real, Real], Real),
    ("power_of", &[Integer, Integer], Integer),
    ("power_of", &[Real, Integer], Real),
    ("power_of", &[Integer, Real], Real),
    ("power_of", &[Real, Real], Real),
    (">", &[Real, Real], Truth),
    (">", &[Text, Text], Truth),
    (">", &[Truth, Truth], Truth),
    (">", &[SetReal, SetReal], Truth),
    (">", &[SetText, SetText], Truth),
    (">", &[DateTime, DateTime], Truth),
    (">=", &[Real, Real], Truth),
    (">=", &[Text, Text], Truth),
    (">=", &[Truth, Truth], Truth),
    (">=", &[SetReal, SetReal], Truth),
    (">=", &[SetText, SetText], Truth),
    (">=", &[DateTime, DateTime], Truth),
    ("<", &[Real, Real], Truth),
    ("<", &[Text, Text], Truth),
    ("<", &[Truth, Truth], Truth),
    ("<", &[SetReal, SetReal], Truth),
    ("<", &[SetText, SetText], Truth),
    ("<", &[DateTime, DateTime], Truth),
    ("<=", &[Real, Real], Truth),
    ("<=", &[Text, Text], Truth),
    ("<=", &[Truth, Truth], Truth),
    ("<=", &[SetReal, SetReal], Truth),
    ("<=", &[SetText, SetText], Truth),
    ("<=", &[DateTime, DateTime], Truth),
    ("=", &[Real, Real], Truth),
    ("=", &[Text, Text], Truth),
    ("=", &[Truth, Truth], Truth),
    ("=", &[SetReal, SetReal], Truth),
    ("=", &[SetText, SetText], Truth),
    ("=", &[DateTime, DateTime], Truth),
    ("!=", &[Real, Real], Truth),
    ("!=", &[Text, Text], Truth),
    ("!=", &[Truth, Truth], Truth),
    ("!=", &[SetReal, SetReal], Truth),
    ("!=", &[SetText, SetText], Truth),
    ("!=", &[DateTime, DateTime], Truth),
    ("compare", &[Real, Real, Real], Integer),
    ("or", &[Truth, Truth], Truth),
    ("and", &[Truth, Truth], Truth),
    ("not", &[Truth], Truth),
    ("if", &[Truth, Truth, Truth], Truth),
    ("if", &[Truth, Integer, Integer], Integer),
    ("if", &[Truth, Real, Real], Real),
    ("if", &[Truth, Text, Text], Text),
    ("if", &[Truth, SetInteger, SetInteger], SetInteger),
    ("if", &[Truth, SetReal, SetReal], SetReal),
    ("if", &[Truth, SetText, SetText], SetText),
    ("#", &[Text, Text], Text),
    ("#", &[Text, Integer], Text),
    ("#", &[Text, Real], Text),
    ("#", &[Integer, Text], Text),
    ("#", &[Integer, Integer], Text),
    ("#", &[Integer, Real], Text),
    ("#", &[Real, Text], Text),
    ("#", &[Real, Integer], Text),
    ("#", &[Real, Real], Text),
    ("matches", &[Text, Text], Truth),
    ("matchesAll", &[SetText, Text], Truth),
    ("matchesAny", &[SetText, Text], Truth),
    ("matchesRx", &[Text, Text], Truth),
    ("matchesRxAll", &[SetText, Text], Truth),
    ("matchesRxAny", &[SetText, Text], Truth),
    ("matchesIRx", &[Text, Text], Truth),
    ("matchesIRxAll", &[SetText, Text], Truth),
    ("matchesIRxAny", &[SetText, Text], Truth),
    ("term_isa", &[Text, Text], Truth),
    ("term_includes", &[SetText, Text], Truth),
    ("term_includesAll", &[SetText, SetText], Truth),
    ("term_includesAny", &[SetText, SetText], Truth),
    ("term_assign", &[Text, SetText], Truth),
    ("term_intersectGeneric", &[SetText, SetText], SetText),
    ("term_occurences", &[SetText, Text], Integer),
    ("includes", &[SetText, Text], Truth),
    ("includes", &[SetReal, Real], Truth),
    ("includes", &[SetInteger, Integer], Truth),
    ("includes", &[SetAny, Any], Truth),
    ("includesAll", &[SetText, SetText], Truth),
    ("includesAll", &[SetReal, SetReal], Truth),
    ("includesAll", &[SetInteger, SetInteger], Truth),
    ("includesAll", &[SetAny, SetAny], Truth),
    ("includesAny", &[SetText, SetText], Truth),
    ("includesAny", &[SetReal, SetReal], Truth),
    ("includesAny", &[SetInteger, SetInteger], Truth),
    ("includesAny", &[SetAny, SetAny], Truth),
    ("oneof", &[Text, SetText], Truth),
    ("oneof", &[Real, SetReal], Truth),
    ("set_add", &[SetAny, Any], Truth),
    ("set_remove", &[SetAny, Any], Truth),
    ("isknown", &[Integer], Truth),
    ("isknown", &[Real], Truth),
    ("isknown", &[Text], Truth),
    ("isknown", &[Truth], Truth),
    ("isknown", &[DateTime], Truth),
    ("isknown", &[SetReal], Truth),
    ("isknown", &[SetText], Truth),
    ("isunknown", &[Integer], Truth),
    ("isunknown", &[Real], Truth),
    ("isunknown", &[Text], Truth),
    ("isunknown", &[Truth], Truth),
    ("isunknown", &[DateTime], Truth),
    ("isunknown", &[SetReal], Truth),
    ("isunknown", &[SetText], Truth),
    ("isunknown", &[Any], Truth),
    ("isunknown", &[SetAny], Truth),
    ("count", &[SetReal], Integer),
    ("count", &[SetText], Integer),
    ("sum", &[SetInteger], Integer),
    ("sum", &[SetReal], Real),
    ("is_dormant", &[Text], Truth),
    ("is_in_progress", &[Text], Truth),
    ("is_discarded", &[Text], Truth),
    ("is_completed", &[Text], Truth),
    ("in_progress_time", &[Text], DateTime),
    ("discarded_time", &[Text], DateTime),
    ("completed_time", &[Text], DateTime),
    ("attr", &[Text, Text], Text),
    ("confirm", &[Text], Integer),
    ("->", &[Text, Text], Text),
    ("nth", &[Integer, SetInteger], Integer),
    ("nth", &[Integer, SetReal], Real),
    ("nth", &[Integer, SetText], Text),
    ("max", &[SetInteger], Integer),
    ("max", &[SetReal], Real),
    ("max", &[SetText], Text),
    ("min", &[SetInteger], Integer),
    ("min", &[SetReal], Real),
    ("min", &[SetText], Text),
    ("median", &[SetInteger], Integer),
    ("median", &[SetReal], Real),
    ("union", &[SetAny, SetAny], SetAny),
    ("union", &[SetInteger, SetInteger], SetInteger),
    ("union", &[SetReal, SetReal], SetReal),
    ("union", &[SetText, SetText], SetText),
    ("diff", &[SetAny, SetAny], SetAny),
    ("diff", &[SetInteger, SetInteger], SetInteger),
    ("diff", &[SetReal, SetReal], SetReal),
    ("diff", &[SetText, SetText], SetText),
    ("intersect", &[SetAny, SetAny], SetAny),
    ("intersect", &[SetInteger, SetInteger], SetInteger),
    ("intersect", &[SetReal, SetReal], SetReal),
    ("intersect", &[SetText, SetText], SetText),
    ("isempty", &[SetAny], Truth),
    ("prune", &[SetAny], SetAny),
    ("prune", &[SetInteger], SetInteger),
    ("prune", &[SetReal], SetReal),
    ("prune", &[SetText], SetText),
    ("first", &[SetAny], Any),
    ("first", &[SetInteger], Integer),
    ("first", &[SetReal], Real),
    ("first", &[SetText], Text),
    ("last", &[SetAny], Any),
    ("last", &[SetInteger], Integer),
    ("last", &[SetReal], Real),
    ("last", &[SetText], Text),
    ("exp", &[Real], Real),
    ("ln", &[Real], Real),
    ("abs", &[Integer], Integer),
    ("abs", &[Real], Real),
    ("sin", &[Real], Real),
    ("cos", &[Real], Real),
    ("tan", &[Real], Real),
    ("asin", &[Real], Real),
    ("acos", &[Real], Real),
    ("atan", &[Real], Real),
    ("random", &[], Real),
    ("now", &[], DateTime),
    ("startup_time", &[], DateTime),
    ("forever", &[], Truth),
    ("true", &[], Truth),
    ("false", &[], Truth),
    ("unknown", &[], Any),
    ("null", &[], Any),
    ("sprintf", &[Text, Any], Text),
    ("sprintf", &[Text, Any, Any], Text),
    ("formatnum", &[Any], Integer),
    ("join", &[Text, SetAny], Text),
    ("date_diff_years", &[DateTime, DateTime], Integer),
    ("date_diff_months", &[DateTime, DateTime], Integer),
    ("date_diff_weeks", &[DateTime, DateTime], Integer),
    ("date_diff_days", &[DateTime, DateTime], Integer),
    ("date_diff_hours", &[DateTime, DateTime], Integer),
    ("date_diff_minutes", &[DateTime, DateTime], Integer),
    ("date_diff_seconds", &[DateTime, DateTime], Integer),
    ("date_diff_milliseconds", &[DateTime, DateTime], Integer),
    ("date_part_year", &[DateTime], Integer),
    ("date_part_month", &[DateTime], Integer),
    ("date_part_day", &[DateTime], Integer),
    ("date_part_hour", &[DateTime], Integer),
    ("date_part_minute", &[DateTime], Integer),
    ("date_part_second", &[DateTime], Integer),
    ("date_part_millisecond", &[DateTime], Integer),
];

/// date_diff_* / date_part_* accept any of date, time and datetime per argument.
pub fn is_date_family(f: &str) -> bool {
    f.starts_with("date_diff_") || f.starts_with("date_part_")
}

pub fn is_builtin(f: &str) -> bool {
    SIGNATURES.iter().any(|(n, _, _)| *n == f)
}

/// Candidate support functions (special-cased; arity 0, 1 or 2).
pub const CANDIDATE_REAL_FNS: &[&str] = &[
    "netsupport",
    "positive_support",
    "negative_support",
    "candsupport_count_for",
    "candsupport_count_against",
    "candsupport_count_neutral",
    "candsupport_count_summary",
];
pub const CANDIDATE_TRUTH_FNS: &[&str] = &[
    "is_recommended",
    "is_discouraged",
    "is_relevant",
    "is_selected",
    "is_controversial",
    "is_committed",
];

/// Functions whose first argument names a task.
pub const TASK_ARG_FNS: &[&str] = &[
    "is_dormant",
    "is_in_progress",
    "is_discarded",
    "is_completed",
    "in_progress_time",
    "discarded_time",
    "completed_time",
    "confirm",
];

/// Documented by Composer's autocompletion but unknown to the verifier.
pub const DOCUMENTED_BUT_UNKNOWN: &[(&str, &str)] = &[
    ("selected_time", "not supported by the engine verifier"),
    ("committed_time", "not supported by the engine verifier"),
    ("assigned_time", "not supported by the engine verifier"),
    ("modified_time", "not supported by the engine verifier"),
    (
        "date_add_days",
        "not supported; use date_diff_* / date_part_*",
    ),
    ("date_add", "not supported; use date_diff_* / date_part_*"),
    ("cap", "not supported by the engine verifier"),
    ("get_caption", "not supported by the engine verifier"),
    (
        "format_num",
        "the verifier only knows formatnum(x) (one argument)",
    ),
];

/// Markdown table of built-ins for the language reference.
pub fn reference_table() -> String {
    let mut names: Vec<&str> = SIGNATURES
        .iter()
        .map(|(n, _, _)| *n)
        .filter(|n| n.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
        .collect();
    names.dedup();
    let mut s = String::from(
        "\n# Built-in functions (as accepted by the engine verifier)\n\n| function | signatures |\n|---|---|\n",
    );
    for n in names {
        if n.starts_with("matches")
            || n.starts_with("term_")
            || n.starts_with("includes")
            || n == "oneof"
            || n == "and"
            || n == "or"
        {
            continue; // operators, documented in the expressions section
        }
        let sigs: Vec<String> = SIGNATURES
            .iter()
            .filter(|(f, _, _)| *f == n)
            .map(|(_, args, ret)| {
                let a: Vec<String> = args
                    .iter()
                    .map(|t| {
                        if is_date_family(n) {
                            "date|time|datetime".to_string()
                        } else {
                            t.to_string()
                        }
                    })
                    .collect();
                format!("({}) -> {}", a.join(", "), ret)
            })
            .collect();
        s.push_str(&format!("| `{}` | {} |\n", n, sigs.join("; ")));
    }
    s.push_str("| `result_of(D)` | decision -> text (single choice decisions) |\n");
    s.push_str("| `result_set(D)` | decision -> setof_text (multiple choice decisions) |\n");
    s.push_str("| `activate_trigger(T)` | task -> integer |\n");
    s.push_str(&format!(
        "| `{}` | `()` in a candidate recommendation, `(cand)` in a decision, `(decision_component, cand)` anywhere -> real |\n",
        CANDIDATE_REAL_FNS.join("`, `")
    ));
    s.push_str(&format!(
        "| `{}` | same arities -> truth_value |\n",
        CANDIDATE_TRUTH_FNS.join("`, `")
    ));
    s.push_str(
        "| `calculate<Int|Real|Text|Boolean>[s](\"java.Class\", ...)` | Java user functions |\n",
    );
    s.push_str("\nNotes: `boolean` data items are typed as text (compare with `= \"true\"`/atoms, not `true()`); there is no date arithmetic (use date_diff_*/date_part_*; `date_diff_<unit>(from, to)` returns to - from, e.g. `date_diff_hours(completed_time(T), now()) >= 48`); integers promote to reals but not vice versa; user `function`s are referenced as bare names (no parentheses).\n");
    s.push_str("Not supported by the verifier although documented: ");
    s.push_str(
        &DOCUMENTED_BUT_UNKNOWN
            .iter()
            .map(|(n, _)| format!("`{}`", n))
            .collect::<Vec<_>>()
            .join(", "),
    );
    s.push('\n');
    s
}
