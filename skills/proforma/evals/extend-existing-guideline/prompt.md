---
tags: [skill-creator]
max_turns: 60
timeout_seconds: 1800
allowed_tools: [Read, Glob, Grep, Skill, Write, Edit, Bash]
---

In sore_throat.pf (a PROforma guideline), add handling for penicillin allergy: patients who chose antibiotics but have penicillin_allergy = yes should get clarithromycin 500 mg twice daily for 5 days instead of penicillin. Also add a follow-up enquiry 48 hours after treatment asking whether symptoms improved. Then regenerate an HTML report of the updated guideline.

Input files: sore_throat.pf
