# Sore throat assessment (tutorial example)

A small guideline built step by step with the `proforma` CLI, to demonstrate the tool. It has an
enquiry, a symptomatic decision on antibiotics versus self-care using the Centor score, and guarded
actions. The clinical content is illustrative only.

Open [`sore_throat.html`](sore_throat.html) for the interactive viewer (task tree, plan canvas, properties). Rebuild or inspect it with:

```bash
proforma outline --file sore_throat.pf
proforma mermaid --file sore_throat.pf --label caption
node ../../skills/proforma/scripts/build_report.mjs sore_throat.pf --out sore_throat.html
```
