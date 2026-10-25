# Modelling guideline constructs in PROforma

How typical guideline content maps onto PROforma, with the CLI calls that build it. Each pattern explains
why it is modelled that way, so you can adapt it to constructs not listed here.

## Contents
1. Algorithm / flowchart → plans and scheduling
2. Threshold branching → decision + guarded tasks
3. Several options that can all apply → multiple-choice decision
4. Evidence levels and grades
5. "Not recommended", restricted populations, no consensus
6. Contributing factors, conditional steps
7. Repeated screening / monitoring
8. Data items
9. Expressions: what the engine can and cannot type
10. Names, captions, Composer compatibility
11. Native fields, ontology coding and source quotes

---

## 1. Algorithm / flowchart → plans and scheduling

Each stage box of the guideline's algorithm becomes a **sub-plan**. Steps inside a stage are its
**components**, and "then" arrows become `--after` (scheduling constraints). Steps without `--after`
run in parallel, which is right for independent assessments such as history, examination and labs.

```bash
proforma add-task --file $F --plan root --kind plan --name assessment --caption "Step II - Diagnostic assessment"
proforma add-task --file $F --plan assessment --kind enquiry --name history --caption "Clinical history"
proforma add-task --file $F --plan assessment --kind enquiry --name labs --caption "Laboratory tests"      # parallel to history
proforma add-task --file $F --plan root --kind plan --name management --caption "Step III - Management" --after assessment
```

Nest a whole stage in one plan with a precondition when the stage only applies on one branch: if
`assessment` is skipped, everything inside it is skipped too, and you don't have to repeat the condition on every task.

## 2. Threshold branching → decision + guarded tasks

A branch point ("if NRS ≥ 4 → assessment, else rescreen") is a **decision**. Give it one candidate per
branch and symmetric arguments. Each downstream task then gets a precondition testing the result:

```bash
proforma add-task --file $F --plan root --kind decision --name triage --caption "Moderate or severe?" --after screening
proforma add-candidate --file $F --decision triage --name assess --caption "Comprehensive assessment"
proforma add-argument  --file $F --decision triage --candidate assess --support for     --condition "fatigue_nrs >= 4" --caption "NRS >= 4 [II, B]"
proforma add-argument  --file $F --decision triage --candidate assess --support against --condition "fatigue_nrs < 4"
proforma add-candidate --file $F --decision triage --name rescreen --caption "Rescreen later"
proforma add-argument  --file $F --decision triage --candidate rescreen --support for --condition "fatigue_nrs < 4"
proforma add-task --file $F --plan root --kind plan --name assessment --after triage --precondition "result_of(triage) = assess"
```

Why a decision rather than just preconditions: the decision records *why* (arguments with grades), it
can be shown to a clinician, and the diagram labels the arrows with the candidate name. The default
recommendation rule `netsupport() > 0` recommends a candidate when its net support is positive.

## 3. Several options that can all apply → multiple-choice decision

Lists like "psychoeducation, CBT, MBSR and yoga can be offered" are one decision with
`choice_mode multiple`. Downstream tasks test `result_set`:

```bash
proforma set --file $F --selector psychosocial --attribute choice_mode --value multiple
proforma add-task ... --precondition 'result_set(psychosocial) includes "cbt"'
proforma add-task ... --precondition "count(result_set(psychosocial)) > 0"      # any option chosen
```

`result_of` is only for single-choice decisions, and lint flags the misuse (PF-V052/V054).

## 4. Evidence levels and grades

Put the grade at the end of the **argument caption**, written as in the source, e.g. `[I, A]`, `[II, B]`
(ESMO / IDSA style). The report highlights `[roman, letter]` grades in colour. For other systems such as
GRADE ("strong recommendation, moderate-quality evidence"), keep the source wording in the caption. The
argument condition is the logic; the caption is the provenance a reviewer needs.

## 5. "Not recommended", restricted populations, no consensus

- **Not recommended for anyone** (e.g. "modafinil is not recommended [II, D]"): list these in the
  decision's `--description`. If you want them visible as options, add a candidate whose only argument
  is `--support excluding --condition "fatigue_nrs >= 0"`, i.e. one that is always true. Don't leave
  them out silently: what *not* to do is part of the guideline.
- **Excluded population** (e.g. "pharmacological interventions are not recommended in the elderly
  [II, D]"): add an `excluding` argument to each affected candidate (`--condition "age >= 65"`).
  `excluding` vetoes the candidate whatever its other support.
- **Only for a sub-population** (e.g. "corticosteroids only in metastatic disease"): a `for` argument on
  the qualifying condition, plus an `excluding` argument on its negation.
- **No panel consensus**: keep the option as a candidate. Require more than one supporting argument
  (`--recommendation "netsupport() >= 2"`) and route it to a "discuss with patient / specialist" action
  rather than straight to treatment. Record the split in the caption, e.g. "3 of 9 panel members: [II, C]".

## 6. Contributing factors, conditional steps

"Treat anaemia, pain, depression … before treating fatigue itself" → one action with a precondition
that ORs the factors, scheduled between assessment and management. If none applies, the action is
discarded and the plan continues.

```bash
proforma add-task --file $F --plan care --kind action --name manage_factors --after assessment \
  --precondition 'anaemia = "yes" or pain_score >= 4 or depression_positive = "yes"'
```

Conditions restricted to a sub-group (e.g. "review medication in patients ≥ 65") are actions with a precondition.

## 7. Repeated screening / monitoring

"Screen at regular intervals during therapy and aftercare" → put the cycle in a plan and make its
**component** cyclic:

```bash
proforma set --file $F --selector root/component:care_cycle --attribute cycle_until --value 'care_ended = "yes"'
proforma set --file $F --selector root/component:care_cycle --attribute cycle_repeat --value "4 weeks"   # only if the guideline gives an interval
```

If the guideline gives no interval, use only `cycle_until`, and record that the interval is set locally.

## 8. Data items

| Guideline content | Data item |
|---|---|
| score / scale (NRS 0-10, ECOG 0-4) | `--type integer --range "0,1,...,10"` |
| lab value, dose | `--type real --unit mg/dL` |
| yes/no finding | `--type text --range '"yes","no"'` (not `boolean`: the engine types it as text and then warns on comparisons) |
| categorical (phase of care, tumour type) | `--type text --range '"active_treatment","post_treatment","palliative"'` |
| several findings at once | `--type setof_text --range '"pain","nausea",...'` |
| computed (BMI, score totals) | `--derivation "weight / (height * height)"` |

Every data item that a decision or precondition reads should be requested by some enquiry
(`add-source`). Otherwise nothing ever collects it.

## 9. Expressions: what the engine can and cannot type

- Comparisons work on numbers, text, truth values and datetimes. Integers promote to reals, but nothing else promotes.
- Text constants need quotes: `stage = "IV"`. A bare unknown word is treated as text silently, and lint info PF-X060 warns about it.
- There is no date arithmetic. Use `date_diff_<unit>(from, to)`, which returns **to − from** in that unit
  (`years`, `months`, `weeks`, `days`, `hours`, `minutes`, `seconds`, `milliseconds`), and `date_part_<part>(d)`.
  "48 hours after task T completed" is `date_diff_hours(completed_time(T), now()) >= 48`, typically as a
  `wait_condition`. There's no need for `abs()`.
- Negation is `not(x)`, and knowing whether a value has been entered is `isknown(x)`.
- User `function`s are referenced by bare name, without parentheses.
- Candidate support functions take three forms: `netsupport()` inside a candidate's recommendation,
  `netsupport(cand)` inside the same decision, and `netsupport(decision, cand)` anywhere else.
- When in doubt: `proforma check-expression --file $F --expression "..."`.

## 10. Names, captions, Composer compatibility

- Names are identifiers (`[A-Za-z_][A-Za-z0-9_]*`) and unique across tasks, data items and functions,
  compared case-insensitively. Reserved words (`type`, `unit`, `range`, `source`, `version`, `title`,
  `group`, `goal`, `context`, `data`, `task`, `days`, …) cannot be names.
- Captions are the human-readable labels shown in Composer and in the diagram (`--label caption`).
  Descriptions hold the longer guideline text.
- Lint codes: `PF-L` = syntax, `PF-K` = Composer compatibility (`proforma format` fixes these),
  `PF-C/R/PP/G` = Composer load checks, `PF-V` = Composer verifier (type checking), `PF-X` = extra checks.
  A clean lint means Composer will load and verify the file without complaint.

## 11. Native fields, ontology coding and source quotes

Composer's properties panel has native fields for most of what a guideline needs. Fill those
before reaching for free `key=value` metaprops: Composer shows them as fields, and the report gives
them their own rows.

| Content | Where it goes |
|---|---|
| Short label | `caption` (tasks, data items, candidates, arguments) |
| Longer guideline text, rationale | `description` |
| What an action does (the instruction to carry out) | `procedure` (actions only) |
| Entry, waiting and exit conditions | `precondition`, `wait_condition`, `postcondition` |
| Fixed repetition of a component | `number_of_cycles`, `cycle_repeat`, `cycle_until` on the component |
| Clinical concept the element stands for | ontology coding, `ontology.coding=SNOMED:<code>` |
| Verbatim passage the element was modelled from | `source.N` + `quote.N` metaprops |

**Ontology coding.** Composer stores its "Ontology coding" property as the metaprop
`ontology.coding`, written `SCHEME:code` (Composer's own files use `SNOMED:372733002`). Its
metaprops editor hides the key and shows the term as a field. It applies to tasks, data items,
candidates and arguments. Add `ontology.label` with the term's preferred name, so readers don't
have to look the code up. The report shows the coding as its own row and links SNOMED CT codes to
the SNOMED browser.

```bash
proforma set --file $F --selector Monitor_Pancreatitis --attribute metaprops --value "'goal.verb=order
ontology.coding=SNOMED:197456007
ontology.label=Acute pancreatitis (disorder)
...'"
proforma set --file $F --selector "treatment/candidate:ace_inhibitor" --attribute metaprops --value "'ontology.coding=SNOMED:372733002'"
```

Code what the element is about: the disorder or finding a monitor watches, the procedure or
substance an action delivers, the observable a data item records. Leave structural elements
(plans, an eligibility candidate such as `Eligible`) uncoded rather than forcing a poor match.

**Source quotes.** Record why each element exists as a citation plus the verbatim passage it came
from. Pair them by number: `source.1` is the citation (document and section), and `quote.1` is the
excerpt, prefixed with its page or section, e.g. `quote.1=p.21: "follow-up ... at least monthly
during the first 3 months"`. Use the bare `source` / `quote` when there is only one. Keep each quote
short, at most about 25 words, copied exactly from the source, with `...` marking any omission.
That is enough to find the passage again without reproducing the document. The report shows each
quote under its citation, on tasks, data items, candidates and arguments alike.

Composer can also link elements to highlighted regions of a source PDF. Those citations live in
Composer's separate project evidence model, not in the `.pf` file, so a `.pf` built with the CLI
carries its provenance in these metaprops instead.

`set --attribute metaprops` replaces the whole map. To change one key, read the current map
(`get` or `export`), change it, and write the whole map back. Keys are sorted, and `=` and `:` in
values are escaped on save; the tools read the escapes back, so write plain values.
