//! Built-in PROforma language reference served to agents (`proforma_reference` tool).

const OVERVIEW: &str = r#"# PROforma quick reference

PROforma is a task-based language for computer-interpretable clinical guidelines
(Fox, Sutton et al.). A guideline is a network of tasks:

| kind      | purpose                                                                 |
|-----------|-------------------------------------------------------------------------|
| plan      | container of components (task instances) scheduled by constraints      |
| decision  | chooses between candidates using arguments (for/against rules)          |
| enquiry   | requests data (sources) from the user or a system                       |
| action    | an instruction/procedure to be carried out                              |
| task      | generic "keystone" task (placeholder / annotation)                      |

plus `data` items (the patient/clinical variables) and user `function`s.

Two-level model: top-level blocks are *task definitions*; a plan refers to them
through *components* (`component :: 'x'; task_definition :: 'x';`). Composer
normally uses the same name for both (1:1). The first block is the root plan; it
is not a task definition and cannot be used as a component.

Task life-cycle: dormant -> in_progress -> completed, or discarded (precondition
false when the task becomes eligible). A component becomes eligible when every
`schedule_constraint :: completed(X)` antecedent has completed (or been discarded).
"#;

const SYNTAX: &str = r#"# File syntax (Composer / engine 1.9)

```
engine_version :: '1.9' ;            <- MUST be on its own line; otherwise the legacy 1.6 parser is used

metadata :: 'my_guideline' ;         <- required in practice (engine crashes without it)
     title :: 'My guideline' ;
     author :: 'Jane' ;
     version :: '1.0' ;
end metadata.

metaprops :: '' ;                    <- required guideline-level metaprops

plan :: 'my_guideline' ;             <- root plan (first block)
     caption :: 'My guideline' ;
     component :: 'assess' ;
          task_definition :: 'assess' ;
          ltwh :: 40,40,0,0;
     component :: 'treat' ;
          task_definition :: 'treat' ;
          schedule_constraint :: completed('assess') ;
          ltwh :: 40,140,0,0;
end plan.

enquiry :: 'assess' ;
     caption :: 'Assess patient' ;
     source :: 'age' ;
          data_definition :: 'age' ;
end enquiry.

action :: 'treat' ;
     caption :: 'Treat' ;
     precondition :: age >= 18 ;
     procedure :: 'Give drug X' ;
end action.

data :: 'age' ;
     type :: integer ;               <- type comes first in a data item
     caption :: 'Age' ;
     unit :: 'years' ;
end data.
```

Lexical rules:
* Every attribute is `keyword :: value ;`, blocks end with `end <kind>.`
* Atoms: bare identifiers `[A-Za-z_][A-Za-z0-9_]*` or single-quoted `'any text'`
  (escape `\'` or `''`). Captions/descriptions are always single-quoted atoms.
* Strings: double-quoted `"text"` (used in expressions and ranges).
* Keywords are reserved: a name such as `type`, `unit`, `data`, `source`, `range`,
  `title`, `version`, `group`, `goal`, `context`, `for`, `end` must be quoted.
* Comments: `/** ... **/` only.
* Generic attributes (caption, description, metaprops) must come first in a block.
* Duplicate single-valued attributes: the engine keeps the FIRST one.
"#;

const TASKS: &str = r#"# Task attributes

All task kinds (after caption/description/metaprops):
* `precondition :: <expr> ;`     task is discarded if false when it becomes eligible
* `wait_condition :: <expr> ;`   task waits (stays dormant) until true
* `postcondition :: <assertion> ;` e.g. `x = 3 and y := z`
* `goal :: <expr> ;`, `trigger :: atom ;`, `context :: atom ;`
* `parameters :: p1 attributes type :: integer ; caption :: 'P1'; end attributes, p2 ;`

plan only:
* `component :: 'name' ;` followed by component attributes (see below)
* `abort :: <expr> ;`  `terminate :: <expr> ;`

action only: `procedure :: 'text' ;`
enquiry: `source :: 'data_item' ;` with optional `data_definition :: 'x'; mandatory :: yes|no;`
decision: see the decisions section.

Component (task instance inside a plan) attributes:
* `task_definition :: 'def' ;`   (defaults to the component name; always written)
* `schedule_constraint :: completed('sibling') ;`  (repeatable; sibling components only)
* `ltwh :: left,top,width,height;` diagram position (Composer has no auto-layout)
* `optional :: yes ;` `autonomous :: yes ;` `terminal :: yes ;`  (default no)
* `number_of_cycles :: <expr> ;` `cycle_until :: <expr> ;` `cycle_repeat :: <expr> days ;`
  (units: seconds minutes hours days weeks)
* `param_value :: p = <expr> ;` binds a parameter of the task definition
* caption/description/metaprops
"#;

const DECISIONS: &str = r#"# Decisions

```
decision :: 'triage' ;
     caption :: 'Triage' ;
     choice_mode :: single ;          (single | multiple)
     support_mode :: symbolic ;       (symbolic | numeric)
     candidate :: 'urgent' ;
          caption :: 'Urgent referral' ;
          argument :: for, pain_score > 7 attributes
               argument_name :: 'severe_pain' ;
               caption :: 'Severe pain' ;
          end attributes
          ;
          argument :: against, age < 16 ;
          recommendation :: netsupport(triage, urgent) >= 1 ;
          priority :: 1 ;
     candidate :: 'routine' ;
          argument :: confirming, symptoms_resolved = "yes" ;
          recommendation :: netsupport() > 0 ;
end decision.
```

* Symbolic support: `for` (+1), `against` (-1), `confirming` (+1000), `excluding` (-1000).
  Numeric mode: support is a weight expression, e.g. `argument :: 2, x > 3 ;`.
  Never mix: a numeric weight in a symbolic decision is written as unparsable text by Composer.
* `recommendation` decides whether the candidate is recommended; Composer's default is
  `netsupport() > 0` (net support of this candidate). `netsupport(D, C)` names them explicitly.
* Result: `result_of(triage)` (single choice) or `result_set(triage)` (multiple choice).
  Downstream tasks test it in preconditions, e.g. `precondition :: result_of(triage) = urgent ;`
  (diagram branch labels are inferred from such tests).
* Decisions may also have `source`s, and `preselect_candidates :: yes|no ;`.
"#;

const DATA: &str = r#"# Data items

```
data :: 'pain_score' ;
     type :: integer ;
     caption :: 'Pain score (0-10)' ;
     range :: 0,1,2,3,4,5,6,7,8,9,10;
     default_value :: 0 ;
     unit :: 'points' ;
     warning_condition :: 1, pain_score > 8 ;
          caption :: 'Severe pain' ;
end data.
```

Types: text, integer, real, boolean, date, time, datetime, object, setof_text,
setof_integer, setof_real.
* `range :: v1, v2, ... ;` allowed values. Composer uses `"value||Caption"` strings for
  captioned choices, e.g. `range :: "m||Male", "f||Female";`
* `derivation :: <expr> ;` computed value; with `dynamic :: yes ;` it is recomputed
  continuously (dynamic items cannot be enquiry sources).
* `mandatory_validation :: <expr> ;`, `true_value`/`false_value` (boolean captions).
* Literals: integers, reals, "strings", dates `2024-01-31`, times `10:30:00.000`,
  datetimes `2024-01-31T10:30:00.000`, lists `[1, 2, 3]`.
"#;

const EXPRESSIONS: &str = r#"# Expressions

Precedence (low -> high):
1. `or`
2. `and`
3. `=  !=  <>  <  <=  >  >=  :=`   (non-associative: `a < b < c` is an error)
4. `includes includesAll includesAny oneof`
5. `matches matchesAll matchesAny`, then `matchesRx...`, then `matchesIRx...`
6. `term_isa term_includes term_includesAll term_includesAny term_intersectGeneric term_assign term_occurences`
7. `++` / `#` (append)
8. `+  -`
9. `*  /`
10. unary `-`
11. `->` (attribute access)

Operands: data item names, parameters, numbers, "strings", 'quoted atoms', dates/times,
lists `[a, b]`, function calls `f(x, y)`, `result_of(decision)`, complex atoms `a:b`.
An unknown bare atom is treated as a text literal (e.g. `result_of(triage) = urgent`).
Negation is the function `not(x)`. Truth values: `true()`/`false()`.

Common built-ins (full table below; lint flags unknown functions): `isknown(d)`, `isunknown(d)`,
`is_completed(T)`, `is_in_progress(T)`, `is_discarded(T)`, `is_dormant(T)`, `netsupport(D, C)`,
`result_set(D)`, `now()`, `count(set)`, `max(set)`, `min(set)`, `sum(set)`, `abs(x)`,
`date_diff_days(from, to)` (= to - from), `date_part_year(d)`, `completed_time(T)`, `if(cond, a, b)`, `not(x)`.
"#;

const FUNCTIONS: &str = r#"# User functions

```
function :: 'bmi' ;
     caption :: 'Body mass index' ;
     type :: real ;                   <- after caption/description, before body
     auto :: no ;                     (optional)
     function_trigger :: weight > 0 ; (optional)
     function_body :: weight / (height * height) ;
end function.
```
Functions share the namespace of data items; call them like built-ins: `bmi() > 30`
or by bare name `bmi > 30`.
"#;

const EDITING: &str = r#"# Editing with proforma

* Addressing: a top-level name (`triage`) or a selector:
  `main/component:assess`, `triage/candidate:urgent`, `triage/candidate:urgent/argument:2`
  (argument by argument_name or 1-based index), `assess/source:age`,
  `pain_score/warning_condition:1`.
* Prefer the structured tools (add_task, add_candidate, add_argument, add_source,
  add_data_item, schedule_constraint, set_attribute) - they enforce Composer's rules
  (identifier names, case-insensitive uniqueness, sibling-only constraints, no cycles,
  support mode consistency).
* For anything else use add_attributes (PROforma attribute text into a block) or
  upsert_component (whole blocks; replaces a block with the same name).
* Every edit re-parses the result before saving and reports a lint summary; run `lint`
  for details and `format` to normalise a hand-written file to Composer's layout.
* `rename` updates task_definition references, scheduling constraints, sources and
  expression references; `remove_component` cascades to plan components and sources.
* Names: identifiers only (letters, digits, `_`). Human-readable text goes in `caption`.
* Comments (`/** ... **/`) are not preserved: every edit rewrites the file in Composer's layout (Composer
  drops comments on save as well). Keep explanations in captions, descriptions and metadata `desc`.
"#;

pub fn get(section: Option<&str>) -> String {
    let all = [
        ("overview", OVERVIEW),
        ("syntax", SYNTAX),
        ("tasks", TASKS),
        ("decisions", DECISIONS),
        ("data", DATA),
        ("expressions", EXPRESSIONS),
        ("functions", FUNCTIONS),
        ("editing", EDITING),
    ];
    match section.map(|s| s.trim().to_lowercase()) {
        Some(s) if !s.is_empty() && s != "all" => {
            match all.iter().find(|(k, _)| k.starts_with(s.as_str())) {
                Some((_, text)) => {
                    let mut t = text.to_string();
                    if s.starts_with("expr") || s.starts_with("func") {
                        t.push_str(&crate::builtins::reference_table());
                    }
                    t
                }
                None => format!(
                    "unknown section '{}'; available: {}",
                    s,
                    all.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
                ),
            }
        }
        _ => {
            let mut t = all.iter().map(|(_, t)| *t).collect::<Vec<_>>().join("\n");
            t.push_str(&crate::builtins::reference_table());
            t
        }
    }
}
