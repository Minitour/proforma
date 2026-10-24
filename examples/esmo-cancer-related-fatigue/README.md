# Cancer-related fatigue (ESMO 2020)

PROforma model of *Fabi A. et al., "Cancer-related fatigue: ESMO Clinical Practice Guidelines for
diagnosis and treatment", Ann Oncol 2020;31(6):713–723*.

| File | |
|---|---|
| [`esmo_crf.pf`](esmo_crf.pf) | the guideline (PROforma 1.9, Deontics Composer format) |
| [`esmo_crf.html`](esmo_crf.html) | report: per-stage diagrams, decision tables with evidence grades, data model, source |
| [`esmo_crf.mmd`](esmo_crf.mmd) | complete pathway as a Mermaid diagram |

**Structure (Figure 1 of the guideline):**
- **Step I, screening:** a 0–10 numerical rating scale (NRS); a score of 4 or more triggers assessment.
- **Step II, diagnostic assessment:** history, fatigue, associated elements and comorbidities.
- **Treatable contributing factors** are managed first.
- **Step III, management:** decisions on physical activity, psychosocial and pharmacological
  interventions, with ESMO evidence grades on the arguments.
- **Re-evaluation,** then the cycle repeats.

**Validation:** `proforma lint` reports 0 errors and 0 warnings. Deontics' engine parses the file with
0 verifier issues, and re-saving it in Composer gives byte-identical output.

Research model only. It is not validated clinical decision support.
