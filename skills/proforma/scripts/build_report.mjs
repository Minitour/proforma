#!/usr/bin/env node
// build_report.mjs: render a PROforma guideline (.pf) as a single self-contained HTML
// viewer in the style of a guideline editor: task tree, plan canvas (stored task
// positions, scheduling arrows, decision branch labels, pan / zoom / drill-down into
// sub-plans), a properties panel, and data, decision, lint and source views.
//
// Usage:
//   node <skill>/scripts/build_report.mjs <guideline.pf> [--out <file.html>] [--source "<text>"] [--notes <file.md>]
//                         [--badge "<text>"]... [--bin <proforma binary or bin/proforma.js>]
//
// The page is self-contained: the template (report/viewer.css, report/viewer.js), the guideline
// data and the source are inlined, and nothing is fetched, so it can be opened locally or
// uploaded as is. The script refuses to write a page with an external reference.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { exportGuideline, lintGuideline } from "./lib/proforma.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const opts = { file: null, out: null, source: null, notes: null, badges: [] };
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  const next = () => {
    if (i + 1 >= argv.length) usage(`missing value for ${a}`);
    return argv[++i];
  };
  if (a === "--out") opts.out = next();
  else if (a === "--source") opts.source = next();
  else if (a === "--notes") opts.notes = next();
  else if (a === "--badge") opts.badges.push(next());
  else if (a === "--bin") process.env.PROFORMA_BIN = next();
  else if (a === "-h" || a === "--help") usage(null, 0);
  else if (a.startsWith("--")) usage(`unknown option ${a}`);
  else if (!opts.file) opts.file = a;
  else usage(`unexpected argument ${a}`);
}
if (!opts.file) usage("missing <guideline.pf>");
opts.out = opts.out || opts.file.replace(/\.pf$/i, "") + ".html";

function usage(msg, code = 2) {
  if (msg) console.error(`build_report: ${msg}`);
  console.error('usage: node build_report.mjs <guideline.pf> [--out file.html] [--source "text"] [--notes notes.md] [--badge "text"]... [--bin path]');
  process.exit(code);
}

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

// minimal Markdown for notes: headings, lists, paragraphs, **bold**, `code`
function markdown(md) {
  const inline = (s) => esc(s).replace(/\*\*(.+?)\*\*/g, "<b>$1</b>").replace(/`([^`]+)`/g, "<code>$1</code>");
  const out = [];
  let list = null;
  let para = [];
  const flush = () => {
    if (para.length) out.push(`<p>${inline(para.join(" "))}</p>`);
    para = [];
  };
  for (const line of md.replace(/\r\n/g, "\n").replace(/^---[\s\S]*?---\n/, "").split("\n")) {
    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    const li = /^\s*[-*]\s+(.*)$/.exec(line) || /^\s*\d+\.\s+(.*)$/.exec(line);
    if (h) {
      flush();
      if (list) out.push("</ul>"), (list = null);
      out.push(`<h${h[1].length + 1}>${inline(h[2])}</h${h[1].length + 1}>`);
    } else if (li) {
      flush();
      if (!list) out.push("<ul>"), (list = true);
      out.push(`<li>${inline(li[1])}</li>`);
    } else if (!line.trim()) {
      flush();
      if (list) out.push("</ul>"), (list = null);
    } else para.push(line.trim());
  }
  flush();
  if (list) out.push("</ul>");
  return out.join("\n");
}

let model;
let lint;
try {
  model = exportGuideline(opts.file);
  lint = lintGuideline(opts.file).concat([]);
} catch (e) {
  console.error(`build_report: ${e.message}`);
  process.exit(2);
}
// informational findings are useful in the lint view
try {
  const { proforma } = await import("./lib/proforma.mjs");
  const r = proforma(["lint", "--file", opts.file, "--include-info", "--format", "json"]);
  const all = JSON.parse(r.stdout);
  if (Array.isArray(all)) lint = all;
} catch {
  /* keep errors and warnings only */
}

const source = fs.readFileSync(opts.file, "latin1").replace(/\r\n/g, "\n");
const notesHtml = opts.notes && fs.existsSync(opts.notes) ? markdown(fs.readFileSync(opts.notes, "utf8")) : "";
const css = fs.readFileSync(path.join(HERE, "report", "viewer.css"), "utf8");
const js = fs.readFileSync(path.join(HERE, "report", "viewer.js"), "utf8");

const m = model.metadata || {};
const title = m.title || model.tasks.find((t) => t.name === model.root)?.caption || model.root || path.basename(opts.file);
const count = (k) => model.tasks.filter((t) => t.kind === k).length;
const errors = lint.filter((d) => d.severity === "error").length;
const warnings = lint.filter((d) => d.severity === "warning").length;
const sub = [m.version && `version ${m.version}`, m.author, opts.source].filter(Boolean).join(" · ");
const payload = JSON.stringify({ model, lint, source, file: path.basename(opts.file) }).replace(/</g, "\\u003c");

const brand = `<svg class="brand" viewBox="0 0 128 128" aria-hidden="true"><rect width="128" height="128" rx="28" fill="#8a1538"/><g stroke-linejoin="round" stroke-linecap="round"><g stroke="#FFDCD9" stroke-width="5" fill="none"><path d="M46 46 L54 54"/><path d="M74 74 L82 82"/></g><path d="M32 16 L48 32 L32 48 L16 32 Z" fill="#E6FFE6" stroke="#009700" stroke-width="4"/><circle cx="64" cy="64" r="15" fill="#FFFFFF" stroke="#B40F78" stroke-width="4"/><circle cx="64" cy="64" r="5" fill="#B40F78"/><rect x="80" y="80" width="32" height="32" rx="8" fill="#E4FAFA" stroke="#008585" stroke-width="4"/></g></svg>`;

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)}</title>
<style>${css}</style>
</head>
<body>
<header class="top">
  <button class="icon" id="treeToggle" title="Task tree">☰</button>
  ${brand}
  <div class="titles"><h1 title="${esc(title)}">${esc(title)}</h1><div class="sub">${esc(sub)}${m.desc ? ` — ${esc(m.desc)}` : ""}</div></div>
  <div class="chips">
    ${opts.badges.map((b) => `<span class="chip">${esc(b)}</span>`).join("")}
    <span class="chip">${count("plan")} plans</span>
    <span class="chip">${count("action")} actions</span>
    <span class="chip">${count("enquiry")} enquiries</span>
    <span class="chip">${count("decision")} decisions</span>
    <span class="chip">${model.data.length} data items</span>
    <span class="chip ${errors ? "err" : warnings ? "warn" : "ok"}">lint: ${errors} errors, ${warnings} warnings</span>
    <button class="icon" id="themeToggle" title="Light / dark">◐</button>
  </div>
</header>
<nav class="tabs">
  <button data-tab="workspace" class="on">Plan editor</button>
  <button data-tab="dataview">Data definitions</button>
  <button data-tab="decisionview">Decisions</button>
  ${notesHtml ? '<button data-tab="notesview">Modelling notes</button>' : ""}
  <button data-tab="lintview">Lint</button>
  <button data-tab="sourceview">Source</button>
</nav>
<main>
  <section class="view on" id="workspace">
    <aside class="left">
      <div class="pane-title">Tasks</div>
      <ul class="tree tree-section" id="tree"></ul>
      <div class="pane-title">Data</div>
      <ul class="tree tree-section" id="datatree"></ul>
    </aside>
    <div class="canvas-wrap">
      <div class="crumbs" id="crumbs"></div>
      <svg id="canvas" role="img" aria-label="Plan diagram"></svg>
      <div class="toolbar">
        <button id="zin" title="Zoom in (+)">+</button>
        <button id="zout" title="Zoom out (−)">−</button>
        <button id="zfit" title="Fit to view (0)">⤢</button>
        <button id="z100" title="Actual size">1:1</button>
        <div class="sep"></div>
        <button id="zup" title="Up to the parent plan (Backspace)">↑</button>
        <button id="zlayout" title="Layout: stored positions (click for automatic)">⊞</button>
        <button id="zlabel" title="Show captions instead of names">Aa</button>
      </div>
      <div class="hint">Drag to pan · wheel or pinch to zoom · double-click a plan to open it</div>
      <div class="zoomlabel" id="zoomlabel">100%</div>
    </div>
    <aside class="right">
      <div class="pane-title">Properties</div>
      <div class="props" id="props"></div>
    </aside>
  </section>
  <section class="view doc" id="dataview"></section>
  <section class="view doc" id="decisionview"></section>
  ${notesHtml ? `<section class="view doc" id="notesview"><div class="inner notes card">${notesHtml}</div></section>` : ""}
  <section class="view doc" id="lintview"></section>
  <section class="view doc" id="sourceview"></section>
</main>
<script type="application/json" id="pf-data">${payload}</script>
<script>${js}</script>
</body>
</html>
`;

// the page must stay self-contained: everything is inlined, nothing is fetched
// markup and CSS are what a browser would fetch from; the guideline text in the data payload is not
const markup = html.replace(payload, "");
const external = /<link\b|<(?:script|img|iframe)[^>]+\bsrc=|(?:src|href)="https?:/i.exec(markup) || /@import|url\(\s*['"]?https?:/i.exec(css);
if (external) {
  console.error(`build_report: refusing to write a page with an external reference: ${external[0]}`);
  process.exit(2);
}
fs.mkdirSync(path.dirname(path.resolve(opts.out)), { recursive: true });
fs.writeFileSync(opts.out, html);
console.log(`wrote ${opts.out} (${Math.round(html.length / 1024)} KB)`);
