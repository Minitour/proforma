# proforma CLI ↔ MCP tool map

The CLI subcommand and the MCP tool take the same parameters: `--flag value` on the CLI, JSON
arguments over MCP. All take `--file` (`file`), the `.pf` path; the file on disk is always the source of truth.

| CLI | MCP tool | Use |
|---|---|---|
| `reference [--section s]` | `proforma_reference` | language reference (overview, syntax, tasks, decisions, data, expressions, functions, editing) |
| `outline` | `outline` | task tree, scheduling, candidates, sources, data items |
| `list [--kind k] [--filter t]` | `list_components` | components as JSON |
| `get --selector s [--format json]` | `get_component` | one component / nested element |
| `refs --name n` | `find_references` | where a name is used |
| `plan-graph [--plan p]` | `plan_graph` | a plan's components and dependencies as JSON |
| `search --query q` | `search` | text search |
| `lint [--include-info] [--format json]` | `lint` | validation (exit code 1 on errors) |
| `check-expression --expression e [--assertion]` | `check_expression` | parse and type an expression |
| `mermaid [--plan p] [--mode expanded\|drilldown] [--label both\|name\|caption] [--candidates] [--data] [--direction TD\|LR] [--output f]` | `to_mermaid` | diagram |
| `create --name n [--title t] [--author a]` | `create_guideline` | new 1.9 file |
| `add-task --plan p --kind k --name n [--caption] [--description] [--after x]... [--precondition e] [--ltwh l,t]` | `add_task` | definition + component |
| `add-component --plan p --task-definition d [--name n] [--after x] [--pf text]` | `add_component` | reuse a definition |
| `add-candidate --decision d --name n [--caption] [--recommendation e] [--priority n]` | `add_candidate` | option |
| `add-argument --decision d --candidate c --support for\|against\|confirming\|excluding\|<weight> --condition e [--name] [--caption]` | `add_argument` | rule |
| `add-source --task t --data d [--mandatory false] [--create-type type]` | `add_source` | enquiry/decision asks for data |
| `add-data --name n --type t [--caption] [--range r] [--default-value e] [--derivation e] [--unit u]` | `add_data_item` | data item |
| `set --selector s --attribute a --value v` | `set_attribute` | set any attribute (precondition, procedure, choice_mode, cycle_until, …) |
| `unset --selector s --attribute a` | `remove_attribute` | clear an attribute |
| `add-attributes --selector s --pf text` | `add_attributes` | insert raw PROforma attributes |
| `upsert --pf text` | `upsert_component` | add/replace whole blocks |
| `schedule --plan p --component c --after a [--remove]` | `schedule_constraint` | arrows (cycle-checked) |
| `rename --name old --new-name new` | `rename` | rename with reference refactoring |
| `remove --name n [--no-cascade]` | `remove_component` | delete (cascades) |
| `remove-element --selector s` | `remove_element` | delete nested element |
| `set-metadata [--title] [--desc] [--author] [--version] [--cdate] [--mdate]` | `set_metadata` | header |
| `format [--check] [--print]` | `format` | canonical Composer layout |
| `serve [--transport stdio\|http]` | — | run the MCP server |

**Selectors:** `name`, `plan/component:x`, `decision/candidate:c`,
`decision/candidate:c/argument:N`, where N is the argument_name or a 1-based index,
`enquiry/source:s`, `data/warning_condition:N`.

**MCP setup:** `claude mcp add proforma -- npx -y proforma-mcp serve`.

## Lint codes worth knowing

| Code | Meaning | Typical fix |
|---|---|---|
| PF-L00x | syntax / lexical error (file can't be loaded) | fix the text; the edit tools refuse to touch the file until then |
| PF-K00x | Composer compatibility (attribute order, header, engine_version line) | `proforma format` |
| PF-R001/2/3 | component → missing definition, constraint to non-sibling, source → missing data | add the definition/data item, or fix the name |
| PF-G004 | task definition not used in any plan (Composer drops it on save) | add a component or remove it |
| PF-V003/V004/V005 | decision without candidates / enquiry without sources / candidate without recommendation | complete the element |
| PF-V011 | expression has the wrong type (e.g. precondition not true/false) | fix the expression; `check-expression` |
| PF-V030 | unknown function | see `proforma reference --section expressions` |
| PF-V051/52/54 | result_of on a non-decision, or single/multiple choice mismatch | use result_of / result_set correctly |
| PF-X061 | compared a decision result with a non-candidate | typo in the candidate name |
| PF-X060 (info) | unknown word treated as text literal | usually a misspelt data item |
| PF-X101 | scheduling cycle | remove one `--after` |
