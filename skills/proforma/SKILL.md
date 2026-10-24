---
name: proforma
description: Turn clinical guidelines, care pathways and protocols (PDFs, text, flowcharts) into executable PROforma (.pf) computer-interpretable guidelines, and read, edit, validate, diagram (Mermaid) and report on existing .pf files with the `proforma` CLI / MCP server (npm package proforma-mcp). Produces the .pf file plus a shareable HTML report with per-stage diagrams and decision tables. Use this whenever the user mentions PROforma, .pf files, Deontics Composer, Arezzo/Tallis, computer-interpretable guidelines (CIG), clinical decision support logic, or asks to encode, model, digitise or "make executable" a clinical guideline, recommendation set or care pathway — or wants a decision-flow diagram or report of one — even if they never say "PROforma".
---

# PROforma guidelines with the proforma CLI / MCP

PROforma is a task-network language for clinical guidelines: **plans** contain **components** scheduled
after one another, **decisions** weigh **candidates** with **for/against arguments**, **enquiries** collect
**data items**, and **actions** carry out procedures. Deontics Composer reads and writes `.pf` files;
the `proforma` tool reads and writes the same format byte-for-byte, so files you produce open in Composer.

The usual job is: *guideline document in → `.pf` file + HTML report out*. Editing or explaining an existing
`.pf` file uses the same tools.

## 1. Get the tool

Prefer the MCP tools if they are connected (names like `outline`, `add_task`, `lint`, `to_mermaid`).
Otherwise use the CLI — every MCP tool is a subcommand with the same parameters as `--flags`:

```bash
proforma --version            # installed globally (npm i -g proforma-mcp)
npx -y proforma-mcp --version  # or run it without installing
```

`proforma reference` (MCP: `proforma_reference`) prints the language reference; read the relevant
section before writing PROforma text by hand. The command map is in `references/cli.md`.

## 2. Read the guideline and extract its logic

Before touching the tool, pull out of the source:

- **The algorithm**: the figure or flowchart (screening → assessment → management …). Its boxes become
  plans and tasks; its arrows become scheduling; its branch points become decisions.
- **Every recommendation**: population, condition, intervention, and evidence level / grade
  (e.g. `[II, B]`, "strong, moderate quality"). Grades go into argument captions so they survive into the report.
- **Thresholds and definitions** (e.g. "NRS ≥ 4 = moderate"), which become expressions over data items.
- **Negative and split recommendations**: "not recommended", "no consensus", restricted populations
  (elderly, pregnancy, renal impairment). Guidelines are full of these and they matter clinically.

Do not invent clinical content. Where the guideline is silent (an interval, a threshold, which option wins
when two apply), make the smallest reasonable modelling choice and record it as a modelling note; the user
needs to know which logic came from the guideline and which from you.

## 3. Sketch the model, then build it with one script

Sketch the plan hierarchy first (a few lines in your head or a scratch file): root plan → stages as sub-plans
→ tasks. Then build the file with a single shell script of CLI calls, so the whole construction is
reviewable and re-runnable. Each command validates what it adds, keeps the file in Composer's canonical
layout, and refuses edits that would break it, which is much safer than hand-writing `.pf` text.

Order matters: **data items first** (sources and expressions refer to them), then tasks top-down, then
sources, candidates and arguments.

```bash
F=guideline.pf
proforma create   --file $F --name crf --title "Cancer-related fatigue (ESMO 2020)" --author "Derived from ..."
proforma add-data --file $F --name fatigue_nrs --type integer --caption "Fatigue (NRS 0-10)" --range "0,1,2,3,4,5,6,7,8,9,10"
proforma add-data --file $F --name metastatic --type text --caption "Metastatic disease?" --range '"yes","no"'
proforma add-task --file $F --plan crf --kind enquiry  --name screening --caption "Screen fatigue"
proforma add-source --file $F --task screening --data fatigue_nrs
proforma add-task --file $F --plan crf --kind decision --name triage --caption "Moderate or severe?" --after screening
proforma add-candidate --file $F --decision triage --name assess --caption "Full assessment" --recommendation "netsupport() > 0"
proforma add-argument  --file $F --decision triage --candidate assess --support for --condition "fatigue_nrs >= 4" \
                       --caption "NRS >= 4 needs a focused assessment [II, B]"
proforma add-task --file $F --plan crf --kind plan --name assessment --caption "Diagnostic assessment" \
                  --after triage --precondition "result_of(triage) = assess"
```

The patterns for translating guideline constructs (thresholds, multiple options, not-recommended drugs,
excluded populations, no-consensus options, repeated monitoring, evidence grades) are in
`references/modelling.md`. Read it before building anything beyond a toy example.

Things that commonly go wrong:
- Names are identifiers (`fatigue_nrs`); human text goes in `--caption` / `--description`.
- Yes/no data: use `--type text --range '"yes","no"'` and compare with `= "yes"`. (The engine types
  `boolean` data as text, so boolean items cause spurious type warnings.)
- Downstream branches test the decision with `result_of(decision) = candidate` (or
  `result_set(decision) includes "x"` for multiple-choice decisions); this is also what labels the diagram arrows.
- Shell quoting: wrap expressions containing `"` in single quotes in bash. In PowerShell, use a here-string
  or a `.ps1` file for long scripts.
- Comments (`/** ... **/`) don't survive: every CLI edit rewrites the whole file in Composer's layout,
  and Composer drops comments on save too. Put explanations where they persist: `--caption` and
  `--description` on tasks, data items and candidates; argument captions for rationale and grades;
  `set-metadata --desc` for the guideline as a whole; and `modelling-notes.md` for assumptions.
- Time intervals: `date_diff_hours(from, to)` returns *to − from*, so "48 h after treatment" is
  `date_diff_hours(completed_time(treat), now()) >= 48`.

## 4. Validate until clean

```bash
proforma lint --file $F --include-info
```

Fix every error and warning, and read the info findings: `PF-X060` often reveals a misspelt data item
(an unknown word in an expression is silently treated as a text literal). `proforma check-expression
--file $F --expression "..."` shows the inferred type and what each name resolves to. The codes are
explained in `references/cli.md`. A file is done when lint reports `0 error(s), 0 warning(s)`.

## 5. Diagram and report

```bash
proforma mermaid --file $F --label caption --output guideline.mmd    # full nested pathway
node <this-skill>/scripts/build_report.mjs $F --out guideline.html \
     --source "Derived from <citation of the source guideline>" \
     --notes modelling-notes.md
```

`build_report.mjs` (Node only, no dependencies; the page template is `scripts/report/`) writes a
self-contained HTML viewer laid out like a guideline editor:
- a resizable task tree and properties pane, and a plan canvas that draws each plan at its stored task positions with the editor's
  shapes and colours, scheduling arrows, decision branch labels and precondition/cycle badges. Drag to
  pan, wheel or pinch to zoom, double-click a plan to open it (breadcrumbs lead back). When stored
  positions stack parallel tasks in one column, it switches to an automatic grid layout;
  the ⧉ button (or E) shows all levels at once, with every sub-plan drawn inline as a nested frame;
- a properties panel for the selected task: caption, description, linked expressions, scheduling,
  cycles, candidates and arguments with evidence grades, and metaprops grouped into goal,
  ServiceRequest, sources and provenance;
- data definitions, decision tables, modelling notes, lint findings and the `.pf` source as tabs.

It reads the guideline through `proforma export` and `proforma lint`, so it needs proforma-mcp 0.2.0 or
later. It finds the CLI on its own: `--bin` / `$PROFORMA_BIN`, a global `npm install -g proforma-mcp`,
`proforma` on PATH, or a private install of the pinned version in `~/.cache/proforma-mcp`.

Write `modelling-notes.md` as a short bullet list of every modelling choice that is not literally in the
guideline (chosen thresholds, tie-breaking rules, intervals, simplifications). The report shows it
prominently, so a clinician reviewing the model can find the assumptions.

## 6. Hand over

Give the user:
- the `.pf` path and the `.html` report path, plus the `.mmd` file if one was asked for;
- the lint result;
- a short description of the structure (stages, decisions, how recommendations became rules);
- the modelling choices that need clinical review.

Show the Mermaid diagram inline when the user asks to see it, and point to the `.html` viewer for exploring the plans. Note that the model is a research
representation, not validated clinical decision support.

## Editing or explaining an existing .pf file

Start with `proforma outline --file X.pf`, then `get --selector <name>`
(`plan/component:x`, `decision/candidate:c`, `decision/candidate:c/argument:2`) to see details.

Make changes with the structured commands (`add-task`, `set`, `add-argument`, `schedule`, `rename`,
`remove`, `remove-element`). They keep references consistent: `rename` updates every reference,
`remove` cascades, and `schedule` rejects cycles. `add-attributes` and `upsert` take raw PROforma text
for anything the structured commands don't cover.

Re-run `lint` after editing, and regenerate the report if one exists. If a file has syntax errors, the
tool refuses to edit it; fix them first, usually with `upsert`.
