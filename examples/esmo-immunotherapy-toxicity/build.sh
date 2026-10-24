#!/usr/bin/env bash
# Rebuilds esmo_irae.pf with the proforma CLI from:
#   Haanen J.B.A.G. et al. Management of toxicities from immunotherapy: ESMO Clinical Practice
#   Guidelines for diagnosis, treatment and follow-up. Ann Oncol 2017;28(Suppl 4):iv119-iv142.
# Recommendations follow Table 2 and the management algorithms (Figures 3, 5-10, 13, 14);
# evidence levels / grades of recommendation are quoted in brackets.
#
# usage: PROFORMA=/path/to/proforma ./build.sh      (default: `proforma` on PATH)
set -euo pipefail
cd "$(dirname "$0")"
P="${PROFORMA:-proforma}"
F=esmo_irae.pf
rm -f "$F"
q() { "$P" "$@" > /dev/null; }

YN='"yes","no"'
GRADES="0,1,2,3,4"

# ── guideline ───────────────────────────────────────────────────────────────
q create --file $F --name esmo_irae --title "Management of toxicities from immunotherapy: ESMO Clinical Practice Guidelines (2017)" \
  --author "Derived from Haanen et al., Ann Oncol 2017;28(Suppl 4):iv119-iv142"
q set-metadata --file $F --desc "Baseline assessment, surveillance and grade-based management of immune-related adverse events (irAEs) from immune checkpoint inhibitors (ICPi), following Table 2 and the management algorithms of the ESMO guideline. Evidence levels/grades of recommendation in brackets."

# ── data model ──────────────────────────────────────────────────────────────
q add-data --file $F --name icpi_regimen --type text --caption "Checkpoint inhibitor regimen" --range '"anti_ctla4","anti_pd1_pdl1","combination"'
q add-data --file $F --name autoimmune_history --type text --caption "History of (or active treatment for) autoimmune disease?" --range "$YN"
q add-data --file $F --name prior_irae --type text --caption "Previous irAEs on another checkpoint inhibitor?" --range "$YN"
q add-data --file $F --name follow_up_ended --type text --caption "irAE follow-up can end (therapy completed and follow-up period over)?" --range "$YN"
q add-data --file $F --name skin_grade --type integer --caption "Skin toxicity CTCAE grade (0 = none)" --range "$GRADES"
q add-data --file $F --name hepatitis_grade --type integer --caption "Hepatitis CTCAE grade (ALT/AST elevation; 0 = none)" --range "$GRADES"
q add-data --file $F --name diarrhoea_grade --type integer --caption "Diarrhoea/colitis CTCAE grade (0 = none)" --range "$GRADES"
q add-data --file $F --name pneumonitis_grade --type integer --caption "Pneumonitis CTCAE grade (0 = none)" --range "$GRADES"
q add-data --file $F --name nephritis_grade --type integer --caption "Nephritis CTCAE grade (creatinine rise; 0 = none)" --range "$GRADES"
q add-data --file $F --name arthralgia_grade --type integer --caption "Arthralgia grade (0 = none)" --range "0,1,2,3"
q add-data --file $F --name thyroid_status --type text --caption "Thyroid function (TSH/FT4)" --range '"normal","hypothyroid","hyperthyroid"'
q add-data --file $F --name thyroid_symptomatic --type text --caption "Symptoms of thyroid dysfunction?" --range "$YN"
q add-data --file $F --name hypophysitis_severity --type text --caption "Hypophysitis presentation" --range '"none","mild","moderate","severe"'
q add-data --file $F --name diabetes_grade --type integer --caption "New-onset (type 1) diabetes grade (3-4 = ketoacidosis; 0 = none)" --range "$GRADES"
q add-data --file $F --name neuro_severity --type text --caption "Neurological toxicity severity" --range '"none","mild","moderate","severe"'
q add-data --file $F --name gbs_or_myasthenia --type text --caption "Guillain-Barre or myasthenia-like syndrome?" --range "$YN"
q add-data --file $F --name myocarditis_suspected --type text --caption "Myocarditis suspected?" --range "$YN"

# ── baseline, education, surveillance cycle ─────────────────────────────────
q add-task --file $F --plan esmo_irae --kind enquiry --name baseline_assessment --caption "Patient selection and baseline assessment" \
  --description "Before treatment: patient and family history, general physical condition, autoimmune diseases, baseline laboratory tests (incl. TSH, FT4, cortisol, glucose, liver and renal function) and radiology (CT chest/abdomen/pelvis, often brain MRI). Patients with autoimmune disease or prior irAEs are at higher risk."
for d in icpi_regimen autoimmune_history prior_irae; do q add-source --file $F --task baseline_assessment --data $d; done
q add-task --file $F --plan esmo_irae --kind action --name patient_education --caption "Inform patient about potential irAEs" --after baseline_assessment
q set --file $F --selector patient_education --attribute procedure --value "Inform the patient about potential adverse events before starting treatment; symptoms must be reported directly to the treating team so that work-up and action can start promptly."
q add-task --file $F --plan esmo_irae --kind plan --name irae_surveillance --caption "irAE surveillance and management cycle" --after patient_education \
  --description "irAEs mostly occur within the first 3 months but may appear up to 1 year after discontinuation. Severe irAEs usually require stopping the ICPi and immunosuppression (high-dose corticosteroids; sometimes TNF-alpha antagonists, mycophenolate or tacrolimus) with careful tapering; consider Pneumocystis prophylaxis for prolonged (>6 weeks) immunosuppression."
q set --file $F --selector esmo_irae/component:irae_surveillance --attribute cycle_until --value 'follow_up_ended = "yes"'

q add-task --file $F --plan irae_surveillance --kind enquiry --name toxicity_assessment --caption "Assess and grade toxicities (CTCAE)" \
  --description "At every visit / before every infusion: symptoms, examination and blood tests (LFTs, TFTs, glucose, creatinine, electrolytes); exclude other causes (infection, disease progression, other drugs)."
for d in skin_grade hepatitis_grade diarrhoea_grade pneumonitis_grade nephritis_grade arthralgia_grade thyroid_status thyroid_symptomatic hypophysitis_severity diabetes_grade neuro_severity gbs_or_myasthenia myocarditis_suspected follow_up_ended; do
  q add-source --file $F --task toxicity_assessment --data $d
done

# helpers: organ plan, decision candidate with one "for" argument, guarded action
organ() { q add-task --file $F --plan irae_surveillance --kind plan --name "$1" --caption "$2" --after toxicity_assessment --precondition "$3"; }
decision() { q add-task --file $F --plan "$1" --kind decision --name "$2" --caption "$3"; }
option() { # decision candidate caption condition rationale
  q add-candidate --file $F --decision "$1" --name "$2" --caption "$3" --recommendation "netsupport() > 0"
  q add-argument --file $F --decision "$1" --candidate "$2" --support for --condition "$4" --caption "$5"
}
step() { # plan name caption after precondition procedure
  q add-task --file $F --plan "$1" --kind action --name "$2" --caption "$3" --after "$4" --precondition "$5"
  q set --file $F --selector "$2" --attribute procedure --value "$6"
}

# ── skin (Figure 3) ─────────────────────────────────────────────────────────
organ skin_toxicity "Immune-related skin toxicity" "skin_grade >= 1"
decision skin_toxicity skin_management "Skin toxicity by grade"
option skin_management skin_continue "Continue ICPi with topical treatment" "skin_grade <= 2" "Grade 1-2: continue ICPi (check weekly); topical emollients, antihistamines and/or topical corticosteroid creams; reinitiate ICPi when <= grade 1"
option skin_management skin_interrupt "Interrupt ICPi, potent topical steroids" "skin_grade = 3" "Grade 3: interrupt ICPi and start topical emollients, antihistamines and high-strength corticosteroid creams [II, B]"
option skin_management skin_discontinue "Discontinue ICPi, i.v. steroids, dermatology" "skin_grade = 4" "Grade 4: discontinue ICPi (permanently), consider admission, consult dermatologist immediately, i.v. (methyl)prednisolone 1-2 mg/kg [II, B]"
step skin_toxicity skin_topical "Topical treatment, continue ICPi" skin_management "result_of(skin_management) = skin_continue" "Avoid skin irritants and sun exposure; topical emollients; mild (G1) or moderate/potent (G2) topical steroids; oral or topical antihistamines for itch; continue ICPi."
step skin_toxicity skin_grade3 "Withhold ICPi; potent topical steroids" skin_management "result_of(skin_management) = skin_interrupt" "Withhold ICPi; potent topical steroids; systemic prednisolone 0.5-1 mg/kg (i.v. if severe) weaned over 1-4 weeks; dermatology review, consider biopsy; recommence ICPi at G1 after discussion."
step skin_toxicity skin_grade4 "Admit; i.v. (methyl)prednisolone 1-2 mg/kg" skin_management "result_of(skin_management) = skin_discontinue" "Permanently discontinue ICPi; admit under dermatology supervision; i.v. (methyl)prednisolone 1-2 mg/kg tapered when toxicity resolves; urgent dermatology review, punch biopsy, clinical photography."

# ── endocrine (Figures 5-6) ─────────────────────────────────────────────────
organ endocrine_toxicity "Immune-related endocrinopathies" 'thyroid_status != "normal" or hypophysitis_severity != "none" or diabetes_grade >= 1'
decision endocrine_toxicity endocrine_management "Endocrinopathy management"
q set --file $F --selector endocrine_management --attribute choice_mode --value multiple
option endocrine_management hyperthyroid_treatment "Beta-blocker for symptomatic hyperthyroidism" 'thyroid_status = "hyperthyroid" and thyroid_symptomatic = "yes"' "Symptomatic hyperthyroidism (usually G1-2): interrupt ICPi, start propranolol or atenolol/metoprolol; restart ICPi when asymptomatic [IV-V, B]"
option endocrine_management hypothyroid_hrt "Thyroid hormone replacement" 'thyroid_status = "hypothyroid"' "Hypothyroidism: start levothyroxine 50-100 ug/day depending on severity, increase until TSH is normal [IV-V, B]"
option endocrine_management hypophysitis_steroids "High-dose steroids and HRT for hypophysitis" 'hypophysitis_severity = "moderate" or hypophysitis_severity = "severe"' "Hypophysitis with headache, diplopia or other neurological symptoms: (methyl)prednisolone 1 mg/kg, taper over 2-4 weeks; HRT for the affected axes [V, B]"
option endocrine_management hypophysitis_hrt "Hormone replacement, continue ICPi" 'hypophysitis_severity = "mild"' "Vague symptoms or asymptomatic: replace cortisol and/or thyroxine as needed and continue ICPi with appropriate HRT (Figure 6)"
option endocrine_management diabetes_insulin "Insulin for new-onset type 1 diabetes" "diabetes_grade >= 1" "New-onset type 1 DM: start insulin; grade 3-4 (ketoacidosis): admit immediately [I, A]. Corticosteroids are not recommended"
step endocrine_toxicity beta_blocker "Interrupt ICPi; beta-blocker" endocrine_management 'result_set(endocrine_management) includes "hyperthyroid_treatment"' "Interrupt ICPi; propranolol or atenolol/metoprolol; restart ICPi when asymptomatic. Rarely carbimazole or steroids are needed."
step endocrine_toxicity levothyroxine "Start levothyroxine" endocrine_management 'result_set(endocrine_management) includes "hypothyroid_hrt"' "Levothyroxine 50-100 ug/day (0.5-1.5 ug/kg; start low in elderly or cardiac history), titrate to normal TSH; continue ICPi. Prednisolone 1 mg/kg for painful thyroiditis."
step endocrine_toxicity hypophysitis_high_dose "Withhold ICPi; (methyl)prednisolone 1 mg/kg" endocrine_management 'result_set(endocrine_management) includes "hypophysitis_steroids"' "Withhold ICPi; send pituitary axis bloods, then (methyl)prednisolone 1 mg/kg (i.v. if severe mass effect or hypoadrenalism); MRI pituitary, visual fields; endocrinology; wean to replacement dose over 2-4 weeks; sick-day rules."
step endocrine_toxicity hypophysitis_replacement "Hormone replacement, continue ICPi" endocrine_management 'result_set(endocrine_management) includes "hypophysitis_hrt"' "Replace cortisol (hydrocortisone 20/10/10 mg if 9am cortisol low) and/or thyroxine; continue ICPi with HRT; MRI pituitary; refer to endocrinology; patient education (alert card)."
step endocrine_toxicity insulin_treatment "Start insulin" endocrine_management 'result_set(endocrine_management) includes "diabetes_insulin"' "Start insulin; admit if ketoacidotic (G3-4) and treat per local DKA guidelines; measure C-peptide, anti-GAD and islet cell antibodies; restart ICPi once regulated."

# ── hepatitis (Figure 7) ────────────────────────────────────────────────────
organ hepatotoxicity "Immune-related hepatotoxicity" "hepatitis_grade >= 1"
decision hepatotoxicity hepatitis_management "Hepatitis by grade"
option hepatitis_management hep_continue "Continue ICPi, repeat LFTs" "hepatitis_grade = 1" "Grade 1 (ALT/AST > ULN-3x ULN): continue treatment; repeat LFTs in 1 week"
option hepatitis_management hep_withhold "Withhold ICPi, close LFT monitoring" "hepatitis_grade = 2" "Grade 2: withhold ICPi and monitor AST/ALT 1-2 times per week; if no improvement after 1 week start (methyl)prednisolone 0.5-1 mg/kg [IV-V, B]"
option hepatitis_management hep_discontinue "Discontinue ICPi, steroids 1-2 mg/kg" "hepatitis_grade = 3" "Grade 3: discontinue ICPi and start (methyl)prednisolone 1-2 mg/kg; add MMF 1000 mg 3x daily if no improvement in 2-3 days [IV-V, B]"
option hepatitis_management hep_permanent "Permanently discontinue, admit, i.v. steroids" "hepatitis_grade = 4" "Grade 4: permanently discontinue ICPi, admit, i.v. (methyl)prednisolone 2 mg/kg; add MMF if no improvement in 2-3 days; consult hepatologist [IV-V, B]"
step hepatotoxicity hep_grade1 "Continue ICPi; repeat LFTs in 1 week" hepatitis_management "result_of(hepatitis_management) = hep_continue" "Continue ICPi; if ALT/AST > ULN-3x ULN repeat in 1 week."
step hepatotoxicity hep_grade2 "Withhold ICPi; LFTs 1-2x weekly" hepatitis_management "result_of(hepatitis_management) = hep_withhold" "Withhold ICPi; recheck LFTs/INR/albumin every 3 days; review medication and alcohol; liver screen (hepatitis A/B/C, E, autoantibodies); if rising or no improvement after 1 week start oral prednisolone 0.5-1 mg/kg; resume ICPi after taper when prednisolone <= 10 mg."
step hepatotoxicity hep_grade3 "Discontinue ICPi; (methyl)prednisolone 1-2 mg/kg" hepatitis_management "result_of(hepatitis_management) = hep_discontinue" "Discontinue ICPi; (methyl)prednisolone 1-2 mg/kg; daily LFTs/INR/albumin, ultrasound with Doppler; add MMF 1000 mg 3x daily if no improvement in 2-3 days; taper over 4-6 weeks."
step hepatotoxicity hep_grade4 "Admit; i.v. (methyl)prednisolone 2 mg/kg" hepatitis_management "result_of(hepatitis_management) = hep_permanent" "Permanently discontinue ICPi; admit; i.v. (methyl)prednisolone 2 mg/kg; add MMF if no improvement in 2-3 days; hepatology consult, consider liver biopsy; ATG or tacrolimus if refractory; taper over 6 weeks."

# ── diarrhoea / colitis (Figure 8) ──────────────────────────────────────────
organ gi_toxicity "Diarrhoea and colitis" "diarrhoea_grade >= 1"
decision gi_toxicity colitis_management "Diarrhoea/colitis by grade"
option colitis_management gi_symptomatic "Continue ICPi, antidiarrhoeals" "diarrhoea_grade = 1" "Grade 1 (non-severe): ICPi can be continued; antidiarrhoeal treatment (e.g. loperamide), fluid and electrolyte supplementation [IV-V, B]"
option colitis_management gi_steroids "Withhold ICPi, oral steroids" "diarrhoea_grade = 2" "Grade 2: withhold ICPi; budesonide or oral corticosteroids 1 mg/kg; if no improvement in 3-5 days colonoscopy and, for colitis, infliximab 5 mg/kg [IV-V, B]"
option colitis_management gi_admit "Discontinue ICPi, admit, i.v. steroids" "diarrhoea_grade >= 3" "Grade 3-4: permanently discontinue anti-CTLA4, admit, i.v. (methyl)prednisolone 1-2 mg/kg; switch to infliximab if no response within 3-5 days [IV-V, B]"
step gi_toxicity gi_grade1 "Symptomatic management, continue ICPi" colitis_management "result_of(colitis_management) = gi_symptomatic" "Oral fluids, loperamide, avoid high-fibre/lactose diet; baseline bloods and stool tests (C. difficile, culture, viral PCR); escalate if G1 persists > 14 days."
step gi_toxicity gi_grade2 "Withhold ICPi; prednisolone 0.5-1 mg/kg" colitis_management "result_of(colitis_management) = gi_steroids" "Withhold ICPi; prednisolone 0.5-1 mg/kg (or budesonide 9 mg od if no bloody diarrhoea); do not wait for endoscopy to start; contact patient every 72h; if no improvement in 72h or worsening manage as severe."
step gi_toxicity gi_grade34 "Admit; i.v. (methyl)prednisolone 1-2 mg/kg" colitis_management "result_of(colitis_management) = gi_admit" "Admit and isolate until infection excluded; i.v. (methyl)prednisolone 1-2 mg/kg; gastroenterology review and sigmoidoscopy/colonoscopy; infliximab 5 mg/kg if no improvement in 72h (no perforation, sepsis, TB, hepatitis, NYHA III/IV CHF); vedolizumab, MMF or tacrolimus as alternatives; early surgical review if bleeding, pain or distension."

# ── pneumonitis (Figure 9) ──────────────────────────────────────────────────
organ pneumonitis "Immune-related pneumonitis" "pneumonitis_grade >= 1"
decision pneumonitis pneumonitis_management "Pneumonitis by grade"
option pneumonitis_management pn_grade1 "Delay ICPi, monitor" "pneumonitis_grade = 1" "Grade 1 (radiographic changes only): consider delaying ICPi; monitor symptoms every 2-3 days; if worsening treat as grade 2 or 3-4"
option pneumonitis_management pn_grade2 "Interrupt ICPi, oral prednisone" "pneumonitis_grade = 2" "Grade 1-2: interrupt ICPi, try to rule out infection and start prednisone 1-2 mg/kg orally; taper over 4-6 weeks [IV-V, B]"
option pneumonitis_management pn_grade34 "Discontinue ICPi, admit, high-dose i.v. steroids" "pneumonitis_grade >= 3" "Grade 3-4: discontinue ICPi permanently, admit (ICU if necessary), high-dose (methyl)prednisolone 2-4 mg/kg i.v.; add infliximab, MMF or cyclophosphamide if deterioration; taper over 4-6 weeks [IV-V, B]"
step pneumonitis pn_monitor "Delay ICPi; monitor every 2-3 days" pneumonitis_management "result_of(pneumonitis_management) = pn_grade1" "Consider delay of treatment; monitor symptoms every 2-3 days; chest X-ray, bloods, sputum for infection as indicated."
step pneumonitis pn_oral_steroids "Withhold ICPi; prednisolone 1 mg/kg" pneumonitis_management "result_of(pneumonitis_management) = pn_grade2" "Withhold ICPi; antibiotics if infection suspected; if no evidence of infection or no improvement after 48h add prednisolone 1 mg/kg/day orally; consider Pneumocystis prophylaxis; HRCT +/- bronchoscopy and BAL; daily symptom monitoring; if no improvement after 48h manage as grade 3."
step pneumonitis pn_admit "Admit; i.v. (methyl)prednisolone 2-4 mg/kg" pneumonitis_management "result_of(pneumonitis_management) = pn_grade34" "Discontinue ICPi; admit; i.v. (methyl)prednisolone 2-4 mg/kg/day; empirical antibiotics; HRCT and respiratory review; if not improving after 48h add infliximab 5 mg/kg or MMF; discuss escalation and ventilation."

# ── neurological (Figures 10-12) ────────────────────────────────────────────
organ neurological_toxicity "Neurological toxicity" 'neuro_severity != "none"'
decision neurological_toxicity neuro_management "Neurological toxicity by severity"
option neuro_management neuro_workup "Withhold ICPi, work-up" 'neuro_severity = "mild"' "Mild neurological AEs: withhold ICPi and perform work-up (MRI, lumbar puncture) to define the nature of neurotoxicity [V, B]"
option neuro_management neuro_moderate "Withhold ICPi, prednisone 0.5-1 mg/kg" 'neuro_severity = "moderate"' "Moderate symptoms: withhold ICPi; prednisone 0.5-1 mg/kg should be considered [V, B]"
option neuro_management neuro_severe "Admit, high-dose steroids" 'neuro_severity = "severe"' "Deterioration or severe symptoms: admit and start (methyl)prednisolone 1-2 mg/kg orally or i.v. [V, B]"
step neurological_toxicity neuro_mild "Withhold ICPi; neurological work-up" neuro_management "result_of(neuro_management) = neuro_workup" "Low threshold to withhold ICPi; comprehensive neurological examination; diabetic screen, B12/folate, HIV, TSH; MRI brain/spine; monitor closely for progression."
step neurological_toxicity neuro_prednisone "Withhold ICPi; prednisone 0.5-1 mg/kg" neuro_management "result_of(neuro_management) = neuro_moderate" "Withhold ICPi; initial observation or prednisone 0.5-1 mg/kg; pregabalin or duloxetine for pain; neurology consult; resume ICPi once back to grade 1."
step neurological_toxicity neuro_admit "Admit; (methyl)prednisolone 1-2 mg/kg" neuro_management "result_of(neuro_management) = neuro_severe" "Withhold ICPi and admit; (methyl)prednisolone 2 mg/kg i.v.; neurologist in care; daily neurological review +/- vital capacity; NCS/EMG, lumbar puncture."
q add-task --file $F --plan neurological_toxicity --kind action --name plasmapheresis_ivig --caption "Plasmapheresis or i.v. immunoglobulin" --after neuro_management --precondition 'gbs_or_myasthenia = "yes"'
q set --file $F --selector plasmapheresis_ivig --attribute procedure --value "For Guillain-Barre or myasthenia-like symptoms consider plasmapheresis or i.v. immunoglobulin [V, B]; pyridostigmine for myasthenia; monitor respiratory function."

# ── cardiac ─────────────────────────────────────────────────────────────────
q add-task --file $F --plan irae_surveillance --kind action --name myocarditis_management --caption "Suspected myocarditis: admit, high-dose steroids" \
  --after toxicity_assessment --precondition 'myocarditis_suspected = "yes"'
q set --file $F --selector myocarditis_management --attribute procedure --value "Admit immediately and start high-dose (methyl)prednisolone 1-2 mg/kg; early cardiology consultation; in case of deterioration add another immunosuppressive drug (MMF or tacrolimus) [V, B]."

# ── rheumatological (Figure 13) ─────────────────────────────────────────────
organ rheumatological_toxicity "Rheumatological toxicity" "arthralgia_grade >= 1"
decision rheumatological_toxicity arthralgia_management "Arthralgia by grade"
option arthralgia_management rheum_nsaids "Analgesia, continue ICPi" "arthralgia_grade = 1" "Mild arthralgia: NSAIDs/analgesia (paracetamol, ibuprofen); continue ICPi [V, B]"
option arthralgia_management rheum_low_dose_steroids "Low-dose prednisone" "arthralgia_grade = 2" "No improvement / moderate: low-dose steroids (prednisone 10-20 mg) [V, B]"
option arthralgia_management rheum_referral "Rheumatology referral, prednisone 1 mg/kg" "arthralgia_grade = 3" "Severe polyarthritis: refer to or consult a rheumatologist and start prednisone 1 mg/kg; infliximab or another anti-TNF-alpha drug if needed [V, B]"
step rheumatological_toxicity arthralgia_g1 "Analgesia; continue ICPi" arthralgia_management "result_of(arthralgia_management) = rheum_nsaids" "Paracetamol and/or ibuprofen; continue ICPi; rheumatological history and examination."
step rheumatological_toxicity arthralgia_g2 "Escalate analgesia; prednisolone 10-20 mg" arthralgia_management "result_of(arthralgia_management) = rheum_low_dose_steroids" "Escalate analgesia (diclofenac, naproxen or etoricoxib); if inadequately controlled prednisolone 10-20 mg or intra-articular steroids; consider withholding ICPi; early rheumatology referral."
step rheumatological_toxicity arthralgia_g3 "Withhold ICPi; prednisolone 0.5-1 mg/kg" arthralgia_management "result_of(arthralgia_management) = rheum_referral" "Withhold ICPi; prednisolone 0.5-1 mg/kg (1 mg/kg per Table 2); rheumatology advice; consider anti-TNF-alpha therapy if no improvement after 4 weeks."

# ── renal (Figure 14) ───────────────────────────────────────────────────────
organ renal_toxicity "Renal toxicity (nephritis)" "nephritis_grade >= 1"
decision renal_toxicity nephritis_management "Nephritis by grade"
option nephritis_management renal_continue "Continue ICPi, weekly creatinine" "nephritis_grade = 1" "Grade 1: continue ICPi, repeat creatinine weekly; review hydration, medication and urinary infection"
option nephritis_management renal_withhold "Withhold ICPi, steroids if irAE" "nephritis_grade = 2" "Rule out other causes, stop nephrotoxic drugs; interrupt ICPi depending on severity; (methyl)prednisolone; consider renal biopsy [V, B]"
option nephritis_management renal_discontinue "Discontinue ICPi, admit, i.v. steroids" "nephritis_grade >= 3" "Severe renal insufficiency: permanently discontinue ICPi, admit, (methyl)prednisolone 1-2 mg/kg; nephrology consultation, renal biopsy [V, B]"
step renal_toxicity renal_g1 "Continue ICPi; weekly creatinine" nephritis_management "result_of(nephritis_management) = renal_continue" "Continue ICPi; repeat creatinine weekly; urine dipstick and protein assessment; ultrasound if obstruction suspected."
step renal_toxicity renal_g2 "Withhold ICPi; prednisolone 0.5-1 mg/kg if irAE" nephritis_management "result_of(nephritis_management) = renal_withhold" "Withhold ICPi; hydration and review creatinine in 48-72h; if attributed to irAE start oral prednisolone 0.5-1 mg/kg; discuss with nephrologist and need for biopsy; recommence ICPi when back to G1/baseline."
step renal_toxicity renal_g34 "Admit; i.v. (methyl)prednisolone 1-2 mg/kg" nephritis_management "result_of(nephritis_management) = renal_discontinue" "Withhold/discontinue ICPi; admit for monitoring and fluid balance; daily creatinine/K+; nephrology and biopsy; i.v. (methyl)prednisolone 1-2 mg/kg; grade 4: manage where renal replacement therapy is available."

"$P" lint --file $F --include-info
