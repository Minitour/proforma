#!/usr/bin/env node
// Build a self-contained HTML report for a PROforma (.pf) guideline:
// per-plan Mermaid diagrams, the complete pathway, decision tables with
// arguments and evidence grades, tasks, data model, lint output and source.
//
// usage: node build_report.mjs <guideline.pf> [--out report.html]
//          [--source "citation of the source guideline"]
//          [--notes notes.md]          modelling notes (choices not taken from the source)
//          [--badge "text"]            extra validation badge, e.g. "Deontics engine: OK" (repeatable)
//          [--bin path/to/proforma]    CLI to use (default: $PROFORMA_BIN, `proforma` on PATH, else npx proforma-mcp)
//
// Only Node.js is required. Everything is derived from the .pf file via the CLI's
// JSON output, so the report never drifts from the guideline file.
import { execFileSync, execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const argv = process.argv.slice(2);
const flag = (name) => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : undefined;
};
const flags = (name) => argv.flatMap((a, i) => (a === name ? [argv[i + 1]] : []));
const pf = argv.find((a, i) => !a.startsWith("--") && (i === 0 || !argv[i - 1].startsWith("--")));
if (!pf) {
  console.error("usage: node build_report.mjs <guideline.pf> [--out report.html] [--source text] [--notes notes.md] [--badge text] [--bin proforma]");
  process.exit(2);
}
const out = flag("--out") || pf.replace(/\.pf$/i, "") + ".html";

// ── locate the CLI ───────────────────────────────────────────────────────────
function resolveCli() {
  const candidates = [flag("--bin"), process.env.PROFORMA_BIN, "proforma"].filter(Boolean);
  for (const c of candidates) {
    try {
      if (process.platform === "win32" && !/[\\/]/.test(c)) execSync(`${c} --version`, { stdio: "pipe" });
      else execFileSync(c, ["--version"], { stdio: "pipe" });
      return [c, []];
    } catch {
      /* try next */
    }
  }
  return ["npx", ["-y", "proforma-mcp"]];
}
const [bin, pre] = resolveCli();
const useShell = process.platform === "win32" && (bin === "npx" || bin === "proforma");
// On Windows, `proforma`/`npx` are .cmd shims that need a shell: build one quoted command string.
const quoteArg = (s) => `"${String(s).replace(/"/g, '\\"')}"`;
const cli = (...a) => {
  const opts = { encoding: "utf8", maxBuffer: 64 << 20, stdio: ["ignore", "pipe", "pipe"] };
  try {
    return useShell
      ? execSync([bin, ...pre, ...a].map((x, i) => (i === 0 ? x : quoteArg(x))).join(" "), opts)
      : execFileSync(bin, [...pre, ...a], opts);
  } catch (e) {
    if (e.stdout) return e.stdout; // lint exits non-zero when it finds errors
    throw e;
  }
};
const json = (...a) => JSON.parse(cli(...a, "--format", "json"));

// ── helpers ──────────────────────────────────────────────────────────────────
const esc = (s) =>
  String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const attr = (attrs, k) => ((attrs || []).find((a) => a.attr === k) || {}).value;
// highlight evidence grades written like [II, B] / [I, A] / [IV, C]
const grade = (s) =>
  esc(s).replace(/\[(I{1,3}|IV|V), ([A-E])\]/g, (m, lvl, g) => `<span class="grade g${g}">${lvl}, ${g}</span>`);
const strip = (m) => m.replace(/^---[\s\S]*?---\n/, "");

// ── read the guideline ───────────────────────────────────────────────────────
const file = pf;
const items = JSON.parse(cli("list", "--file", file));
if (!items.length) {
  console.error(`no components found in ${pf} (does it parse? run: proforma lint --file ${pf})`);
  process.exit(1);
}
const full = Object.fromEntries(items.map((i) => [i.name, json("get", "--file", file, "--selector", i.name)]));
const outline = cli("outline", "--file", file);
const meta = Object.fromEntries(
  outline
    .split("\n")
    .map((l) => l.match(/^(title|version|author|engine_version): (.*)$/))
    .filter(Boolean)
    .map((m) => [m[1], m[2]])
);
const lintText = cli("lint", "--file", file, "--include-info").trim();
const lintSummary = (lintText.match(/(\d+) error\(s\), (\d+) warning\(s\)/) || [0, "?", "?"]).slice(1).map(Number);
const source = fs.readFileSync(pf, "latin1").replace(/\r\n/g, "\n");
const root = items[0];
const title = meta.title || full[root.name].attrs.find((a) => a.attr === "caption")?.value || root.name;
const sourceText = flag("--source") || meta.author || "";
const notesFile = flag("--notes");
const notes = notesFile && fs.existsSync(notesFile) ? fs.readFileSync(notesFile, "utf8") : "";

const counts = {};
for (const i of items) counts[i.kind] = (counts[i.kind] || 0) + 1;

// ── diagrams: one per plan (drilldown), plus the complete nested pathway ─────
const comps = (plan) => (full[plan].attrs || []).filter((a) => a.attr === "component").map((a) => a.value);
const kindOf = (def) => (items.find((i) => i.name === def) || {}).kind;
const stages = [];
for (const p of items.filter((i) => i.kind === "plan")) {
  const cs = comps(p.name);
  if (!cs.length) continue;
  // skip a root plan that only wraps a single sub-plan (the sub-plan gets its own view)
  if (p.name === root.name && cs.length === 1 && kindOf(cs[0].attrs?.find((a) => a.attr === "task_definition")?.value || cs[0].name) === "plan") continue;
  const hasDecision = cs.some((c) => kindOf(attr(c.attrs, "task_definition") || c.name) === "decision");
  const parallel = cs.filter((c) => !(c.attrs || []).some((a) => a.attr === "schedule_constraint")).length;
  const extra = [];
  if (hasDecision) extra.push("--candidates");
  if (hasDecision || parallel >= 4) extra.push("--direction", "LR");
  const caption = attr(full[p.name].attrs, "caption") || p.name;
  const desc = attr(full[p.name].attrs, "description") || "";
  stages.push([caption, desc, strip(cli("mermaid", "--file", file, "--label", "caption", "--mode", "drilldown", "--plan", p.name, ...extra))]);
}
const fullDiagram = strip(cli("mermaid", "--file", file, "--label", "caption"));

// ── decisions ────────────────────────────────────────────────────────────────
const supportLabel = (s) => (s.kind === "weight" ? `weight ${esc(s.weight.text)}` : s.kind);
let decisions = "";
for (const i of items.filter((x) => x.kind === "decision")) {
  const d = full[i.name];
  const mode = attr(d.attrs, "choice_mode") === "multiple" ? "multiple choice" : "single choice";
  const desc = attr(d.attrs, "description");
  let rows = "";
  for (const c of d.attrs.filter((a) => a.attr === "candidate").map((a) => a.value)) {
    const args = c.attrs.filter((a) => a.attr === "argument").map((a) => a.value);
    const rec = (attr(c.attrs, "recommendation") || {}).text;
    const argHtml = args.length
      ? args
          .map(
            (a) =>
              `<li><span class="sup ${a.support.kind}">${supportLabel(a.support)}</span> <code>${esc(a.condition.text)}</code>${
                attr(a.attrs, "caption") ? `<div class="why">${grade(attr(a.attrs, "caption"))}</div>` : ""
              }</li>`
          )
          .join("")
      : `<li class="muted">no arguments</li>`;
    rows += `<tr><td><b>${esc(c.name)}</b><div class="cap">${grade(attr(c.attrs, "caption"))}</div></td><td><ul class="args">${argHtml}</ul></td><td><code>${esc(rec || "—")}</code></td></tr>`;
  }
  decisions += `<section class="card"><h3>${esc(attr(d.attrs, "caption") || i.name)} <span class="tag">${esc(i.name)} · ${mode}</span></h3>${
    desc ? `<p class="desc">${grade(desc)}</p>` : ""
  }<table><thead><tr><th>Candidate</th><th>Arguments (support · condition · rationale)</th><th>Recommended when</th></tr></thead><tbody>${rows}</tbody></table></section>`;
}

// ── tasks and data ───────────────────────────────────────────────────────────
let tasks = "";
for (const i of items.filter((x) => ["plan", "action", "enquiry", "task"].includes(x.kind))) {
  const t = full[i.name];
  const pre = (attr(t.attrs, "precondition") || {}).text;
  const proc = attr(t.attrs, "procedure");
  const desc = attr(t.attrs, "description");
  const srcs = t.attrs.filter((a) => a.attr === "source").map((a) => a.value.name);
  tasks += `<tr><td><span class="kind ${i.kind}">${i.kind}</span></td><td><b>${esc(i.caption || i.name)}</b><div class="tag">${esc(i.name)}</div></td><td>${
    desc ? `<div>${grade(desc)}</div>` : ""
  }${proc ? `<div class="proc">${grade(proc)}</div>` : ""}${srcs.length ? `<div class="muted">asks for: ${srcs.map(esc).join(", ")}</div>` : ""}</td><td>${
    pre ? `<code>${esc(pre)}</code>` : '<span class="muted">—</span>'
  }</td></tr>`;
}
let data = "";
for (const i of items.filter((x) => x.kind === "data")) {
  const d = full[i.name];
  const range = attr(d.attrs, "range");
  const unit = attr(d.attrs, "unit");
  const deriv = (attr(d.attrs, "derivation") || {}).text;
  data += `<tr><td><code>${esc(i.name)}</code></td><td>${esc(i.caption)}</td><td>${esc(i.data_type)}${unit ? ` (${esc(unit.value)})` : ""}</td><td>${
    range ? range.map((r) => `<code>${esc(r.text)}</code>`).join(" ") : deriv ? `derived: <code>${esc(deriv)}</code>` : '<span class="muted">—</span>'
  }</td></tr>`;
}

const notesHtml = notes
  ? notes
      .split(/\n{2,}/)
      .map((block) =>
        /^\s*[-*] /m.test(block)
          ? `<ul>${block
              .split("\n")
              .filter((l) => /^\s*[-*] /.test(l))
              .map((l) => `<li>${grade(l.replace(/^\s*[-*] /, ""))}</li>`)
              .join("")}</ul>`
          : `<p>${grade(block.replace(/^#+\s*/, ""))}</p>`
      )
      .join("")
  : "";

const badges = [
  `<span class="badge ${lintSummary[0] === 0 ? "ok" : "bad"}">proforma lint: ${lintSummary[0]} errors · ${lintSummary[1]} warnings</span>`,
  ...flags("--badge").map((b) => `<span class="badge ok">${esc(b)}</span>`),
  meta.engine_version ? `<span class="badge">PROforma engine_version ${esc(meta.engine_version)}</span>` : "",
].join("");

const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>${esc(title)} — PROforma model</title>
<style>
:root{--bg:#f7f7f5;--card:#fff;--ink:#1d2329;--muted:#66707a;--line:#e3e5e8;--accent:#8a1538}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.55 -apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif}
header{background:linear-gradient(120deg,#5b0e2d,#8a1538);color:#fff;padding:36px 24px 28px}
.wrap{max-width:1180px;margin:0 auto;padding:0 20px}
header h1{margin:0 0 6px;font-size:28px;letter-spacing:-.01em}header p{margin:4px 0;opacity:.9}
.badges{display:flex;flex-wrap:wrap;gap:8px;margin-top:16px}.badge{background:rgba(255,255,255,.14);border:1px solid rgba(255,255,255,.25);border-radius:999px;padding:4px 12px;font-size:13px}
.badge.ok{background:#e6f6ea;color:#11602a;border-color:#bfe3c9}.badge.bad{background:#fdecea;color:#9b1c1c;border-color:#f3b3ad}
main{padding:26px 0 60px}h2{font-size:21px;margin:34px 0 12px;color:var(--accent)}h3{margin:0 0 8px;font-size:17px}
.card{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:18px 20px;margin:14px 0;box-shadow:0 1px 2px rgba(0,0,0,.03)}
.note{border-left:4px solid #e3a008;background:#fffbea}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px}.stat{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:14px 16px}.stat b{display:block;font-size:24px}.stat span{color:var(--muted);font-size:13px}
.diagram.stage{max-height:none;text-align:center}.diagram{overflow:auto;max-height:80vh;background:#fff;border:1px solid var(--line);border-radius:12px;padding:16px}.diagram.fit svg{max-width:100%!important;height:auto}
.toolbar{display:flex;gap:12px;align-items:center;margin:0 0 8px}.toolbar button{font:inherit;font-size:13px;border:1px solid var(--line);background:#fff;border-radius:8px;padding:5px 12px;cursor:pointer}
.legend{display:flex;flex-wrap:wrap;gap:14px;font-size:13px;color:var(--muted);margin:10px 2px}.legend i{display:inline-block;width:14px;height:14px;border-radius:4px;vertical-align:-2px;margin-right:6px;border:2px solid}
table{width:100%;border-collapse:collapse;font-size:14px}th,td{text-align:left;vertical-align:top;padding:9px 10px;border-top:1px solid var(--line)}th{font-size:12px;text-transform:uppercase;letter-spacing:.04em;color:var(--muted);border-top:0}
code{font:12.5px/1.4 ui-monospace,SFMono-Regular,Consolas,monospace;background:#f1f2f4;border-radius:5px;padding:1px 5px;word-break:break-word}
.tag{font-size:12px;color:var(--muted);font-weight:400;margin-left:6px}.cap{color:var(--muted);font-size:13px}.desc{color:#3d4650}.proc{margin-top:4px}
.why{font-size:13px;color:#3d4650;margin:3px 0 0}.muted{color:var(--muted)}ul.args{margin:0;padding-left:16px}ul.args li{margin:2px 0 6px}
.sup{display:inline-block;font-size:11.5px;font-weight:600;border-radius:5px;padding:1px 6px;text-transform:uppercase;letter-spacing:.03em}
.sup.for{background:#e6f6ea;color:#11602a}.sup.against{background:#fdecea;color:#9b1c1c}.sup.excluding{background:#2b2f36;color:#fff}.sup.confirming{background:#d9f0ff;color:#0b4f84}.sup.weight{background:#eef0ff;color:#2d3a8c}
.grade{display:inline-block;font-size:11.5px;font-weight:700;border-radius:999px;padding:0 7px;margin:0 1px;border:1px solid}
.gA,.gB{color:#11602a;border-color:#9fd6ad;background:#effaf2}.gC{color:#8a5a00;border-color:#f0cf84;background:#fff8e6}.gD,.gE{color:#9b1c1c;border-color:#f3b3ad;background:#fdf1f0}
.kind{font-size:11.5px;font-weight:700;border-radius:6px;padding:2px 7px;text-transform:uppercase}.kind.plan{background:#FFDCD9;color:#a12a1e}.kind.action{background:#E4FAFA;color:#006262}.kind.enquiry{background:#E6FFE6;color:#006d00}.kind.task{background:#FEFBED;color:#8a6d00}
details{background:var(--card);border:1px solid var(--line);border-radius:12px;margin:14px 0}summary{cursor:pointer;padding:14px 18px;font-weight:600}
pre.src{margin:0;max-height:620px;overflow:auto;background:#0f1720;color:#dbe4ee;padding:16px 18px;border-radius:0 0 12px 12px;font:12.5px/1.5 ui-monospace,Consolas,monospace}
footer{color:var(--muted);font-size:13px;border-top:1px solid var(--line);padding-top:18px;margin-top:40px}
@media print{header{background:#5b0e2d!important;-webkit-print-color-adjust:exact;print-color-adjust:exact}.toolbar{display:none}.diagram svg{max-width:100%!important;height:auto}.diagram,pre.src{max-height:none;overflow:visible}details{display:block}details>summary{display:none}.card,tr{break-inside:avoid}}
</style></head>
<body>
<header><div class="wrap">
<h1>${esc(title)}</h1>
${sourceText ? `<p>${esc(sourceText)}</p>` : ""}
<div class="badges">${badges}</div></div></header>
<main class="wrap">

<section class="card note"><b>Research model.</b> An executable PROforma representation of the source guideline for research and tooling. It is not a clinical decision support system and does not replace the published guideline.</section>
${notesHtml ? `<section class="card"><h3>Modelling notes</h3>${notesHtml}</section>` : ""}

<div class="grid">
<div class="stat"><b>${counts.plan || 0}</b><span>plans (incl. root)</span></div>
<div class="stat"><b>${counts.decision || 0}</b><span>decisions</span></div>
<div class="stat"><b>${counts.enquiry || 0}</b><span>enquiries</span></div>
<div class="stat"><b>${counts.action || 0}</b><span>actions</span></div>
<div class="stat"><b>${counts.data || 0}</b><span>data items</span></div>
</div>

<h2>Care pathway</h2>
<div class="legend">
<span><i style="background:#FFDCD9;border-color:#E55344"></i>plan</span>
<span><i style="background:#fff;border-color:#B40F78;border-radius:50%"></i>decision</span>
<span><i style="background:#E6FFE6;border-color:#009700;transform:rotate(45deg)"></i>enquiry (data collection)</span>
<span><i style="background:#E4FAFA;border-color:#008585"></i>action</span>
<span><i style="background:#F8A2D3;border-color:#B40F78"></i>decision option</span>
<span>arrows = scheduling (B starts after A); arrow labels = decision result tested by the next task's precondition</span>
</div>
${stages
  .map(([t, d, m]) => `<section class="card"><h3>${esc(t)}</h3>${d ? `<p class="desc">${grade(d)}</p>` : ""}<div class="diagram stage"><pre class="mermaid">${esc(m)}</pre></div></section>`)
  .join("")}
<details><summary>Complete pathway in one diagram (all plans nested)</summary><div class="toolbar" style="padding:0 18px"><button id="fit" type="button">Fit to width</button><span class="muted">scroll to pan</span></div><div class="diagram" id="diagram" style="margin:0 18px 18px"><pre class="mermaid">${esc(fullDiagram)}</pre></div></details>

${decisions ? `<h2>Decisions</h2><p class="muted">Symbolic argumentation: <b>for</b> +1, <b>against</b> −1, <b>confirming</b> +1000, <b>excluding</b> rules a candidate out; <code>netsupport()</code> is the candidate's net support.</p>${decisions}` : ""}

<h2>Tasks</h2>
<section class="card"><table><thead><tr><th>Kind</th><th>Task</th><th>What it does</th><th>Precondition</th></tr></thead><tbody>${tasks}</tbody></table></section>

${data ? `<h2>Data model</h2><section class="card"><table><thead><tr><th>Name</th><th>Question / label</th><th>Type</th><th>Allowed values</th></tr></thead><tbody>${data}</tbody></table></section>` : ""}

<h2>Source</h2>
<details><summary>Task tree</summary><pre class="src">${esc(outline)}</pre></details>
<details><summary>${esc(path.basename(pf))} — ${source.split("\n").length} lines</summary><pre class="src">${esc(source)}</pre></details>
<details><summary>Validation output</summary><pre class="src">$ proforma lint --file ${esc(path.basename(pf))} --include-info
${esc(lintText)}</pre></details>

<footer>Generated with the <code>proforma</code> CLI (npm: proforma-mcp) · research use only.</footer>
</main>
<script src="https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.min.js"></script>
<script>mermaid.initialize({startOnLoad:true,securityLevel:"loose",flowchart:{htmlLabels:true,curve:"basis",useMaxWidth:false,nodeSpacing:40,rankSpacing:55},themeVariables:{fontSize:"15px"},theme:"default"});
const fitBtn=document.getElementById("fit"),dg=document.getElementById("diagram");fitBtn.onclick=()=>{dg.classList.toggle("fit");fitBtn.textContent=dg.classList.contains("fit")?"Actual size":"Fit to width";};</script>
</body></html>`;

fs.writeFileSync(out, html);
console.log(`wrote ${out} (${(html.length / 1024).toFixed(1)} KB; lint: ${lintSummary[0]} errors, ${lintSummary[1]} warnings; ${stages.length} plan diagrams)`);
