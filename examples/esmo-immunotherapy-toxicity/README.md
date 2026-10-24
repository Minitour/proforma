# Management of toxicities from immunotherapy (ESMO 2017)

PROforma model of *Haanen J.B.A.G. et al., "Management of toxicities from immunotherapy: ESMO Clinical
Practice Guidelines for diagnosis, treatment and follow-up", Ann Oncol 2017;28(Suppl 4):iv119–iv142*.

| File | |
|---|---|
| [`esmo_irae.pf`](esmo_irae.pf) | the guideline (PROforma 1.9, Deontics Composer format) |
| [`esmo_irae.html`](esmo_irae.html) | report: per-stage diagrams, decision tables with evidence grades, data model, source |
| [`esmo_irae.mmd`](esmo_irae.mmd) | complete pathway as a Mermaid diagram |
| [`modelling-notes.md`](modelling-notes.md) | modelling choices not taken literally from the source |
| [`build.sh`](build.sh) | rebuilds the `.pf` with the proforma CLI |

**Structure:**
- **Baseline assessment and patient education,** then a surveillance cycle that grades toxicities at each visit.
- **One management stage per organ system:** skin, endocrine, liver, GI, lung, neurological, cardiac,
  rheumatological and renal.
- **In each stage, a grade-based decision.** Each rule follows Table 2 and the management algorithms,
  with ESMO's evidence levels and grades (e.g. `[IV-V, B]`) on the arguments.

**Validation:** `proforma lint` reports 0 errors and 0 warnings. Deontics' engine parses the file with
0 verifier issues, and re-saving it in Composer gives byte-identical output.

Research model only. It is not validated clinical decision support.
