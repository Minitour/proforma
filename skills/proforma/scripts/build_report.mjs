#!/usr/bin/env node
// build_report.mjs: render a PROforma guideline (.pf) as a single self-contained HTML
// viewer in the style of a guideline editor: task tree, plan canvas (stored task
// positions, scheduling arrows, decision branch labels, pan / zoom / drill-down into
// sub-plans), a properties panel, and data, decision, lint and source views.
//
// Usage:
//   node <skill>/scripts/build_report.mjs <guideline.pf> [--out <file.html>] [--source "<text>"] [--notes <file.md>]
//                         [--badge "<text>"]... [--bin <proforma binary or bin/proforma.js>]
//                         [--attach-dir <folder> [--attach-name <file.zip>] [--attach-label "<text>"]
//                          [--attach-root <folder name> | --attach-root .]]
//
// --attach-dir embeds every file under <folder> (recursively, binary-safe) in the page, and the
// header gets a download button that zips them in the browser. The intended use is a Deontics
// Composer project folder (project.metadata, proforma/<Name>.pf, proforma/<Name>.deo), which
// the reader unzips and opens in Composer with File > Open Project. The zip holds the files
// under one top-level folder, named after the zip (--attach-name without .zip, by default
// <guideline>-composer-project) or --attach-root, so the extracted folder is the project;
// --attach-root . puts them at the zip root instead.
//
// The page is self-contained: the template (report/viewer.css, report/viewer-lib.js,
// report/viewer.js), the guideline data and the source are inlined, and nothing is
// fetched, so it can be opened locally or uploaded as is. The script refuses to write a page
// with an external reference.

import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { fileURLToPath } from "node:url";
import { prerender } from "./report/prerender.mjs";
import { exportGuideline, lintGuideline } from "./lib/proforma.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const opts = { file: null, out: null, source: null, notes: null, badges: [], attachDir: null, attachName: null, attachLabel: null, attachRoot: null };
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
  else if (a === "--attach-dir") opts.attachDir = next();
  else if (a === "--attach-name") opts.attachName = next();
  else if (a === "--attach-label") opts.attachLabel = next();
  else if (a === "--attach-root") opts.attachRoot = next();
  else if (a === "-h" || a === "--help") usage(null, 0);
  else if (a.startsWith("--")) usage(`unknown option ${a}`);
  else if (!opts.file) opts.file = a;
  else usage(`unexpected argument ${a}`);
}
if (!opts.file) usage("missing <guideline.pf>");
opts.out = opts.out || opts.file.replace(/\.pf$/i, "") + ".html";
if (!opts.attachDir && (opts.attachName || opts.attachLabel || opts.attachRoot != null)) usage("--attach-name, --attach-label and --attach-root need --attach-dir");

function usage(msg, code = 2) {
  if (msg) console.error(`build_report: ${msg}`);
  console.error('usage: node build_report.mjs <guideline.pf> [--out file.html] [--source "text"] [--notes notes.md] [--badge "text"]... [--bin path]');
  console.error('                         [--attach-dir folder [--attach-name file.zip] [--attach-label "text"] [--attach-root name|.]]');
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
// the page script: the pure helpers (viewer-lib.js, also used here) and then the viewer
const lib = fs.readFileSync(path.join(HERE, "report", "viewer-lib.js"), "utf8");
const PFLib = vm.runInNewContext(lib + "\n;PFLib", {}, { filename: "viewer-lib.js" });
const js = lib + "\n" + fs.readFileSync(path.join(HERE, "report", "viewer.js"), "utf8");

const m = model.metadata || {};
// a caption may hold Composer Markdown: the page title is its first line, plain
const title = m.title || PFLib.plain(model.tasks.find((t) => t.name === model.root)?.caption) || model.root || path.basename(opts.file);
const count = (k) => model.tasks.filter((t) => t.kind === k).length;
const errors = lint.filter((d) => d.severity === "error").length;
const warnings = lint.filter((d) => d.severity === "warning").length;
const sub = [m.version && `version ${m.version}`, m.author, opts.source].filter(Boolean).join(" · ");
const payload = JSON.stringify({ model, lint, source, file: path.basename(opts.file) }).replace(/</g, "\\u003c");

// --attach-dir: every file under the folder, relative paths with forward slashes, base64
function readAttachments(dir) {
  const files = [];
  const walk = (abs, rel) => {
    const names = fs.readdirSync(abs).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
    for (const name of names) {
      const p = path.join(abs, name);
      const r = rel ? rel + "/" + name : name;
      const st = fs.statSync(p);
      if (st.isDirectory()) walk(p, r);
      else if (st.isFile()) {
        const data = fs.readFileSync(p);
        files.push({ path: r, size: data.length, mtime: Math.round(st.mtimeMs), base64: data.toString("base64") });
      }
    }
  };
  walk(dir, "");
  return files;
}
let attach = null;
if (opts.attachDir) {
  if (!fs.existsSync(opts.attachDir) || !fs.statSync(opts.attachDir).isDirectory()) usage(`--attach-dir ${opts.attachDir} is not a folder`);
  const files = readAttachments(opts.attachDir);
  if (!files.length) usage(`--attach-dir ${opts.attachDir} has no files`);
  let name = opts.attachName || path.basename(opts.file).replace(/\.pf$/i, "") + "-composer-project.zip";
  if (/[\\/]/.test(name)) usage("--attach-name is a file name, not a path");
  if (!/\.zip$/i.test(name)) name += ".zip";
  const root = opts.attachRoot == null ? name.replace(/\.zip$/i, "") : opts.attachRoot === "." ? "" : opts.attachRoot;
  if (/[\\/]|^\.\.?$/.test(root)) usage("--attach-root is one folder name, or . for the zip root");
  const label = opts.attachLabel || "Download Composer project";
  const bytes = files.reduce((n, f) => n + f.size, 0);
  if (bytes > 50 << 20) console.error(`build_report: warning: ${Math.round(bytes / (1 << 20))} MB attached, the page will be large`);
  attach = { name, label, root, files };
  attach.json = JSON.stringify({ name, label, root, files }).replace(/</g, "\\u003c");
  attach.summary = `${files.length} file${files.length === 1 ? "" : "s"}, ${bytes < 1024 ? bytes + " bytes" : Math.max(1, Math.round(bytes / 1024)) + " KB"}`;
}

// the first screen as static markup, so the page reads without JavaScript (thumbnails, previews)
let pre = { ids: {}, canvasAttrs: "" };
try {
  pre = prerender(js, payload);
} catch (e) {
  console.error(`build_report: static preview skipped (${e.message})`);
}
const P = (id) => pre.ids[id] || "";

const brand = `<svg class="brand" viewBox="0 0 128 128" aria-hidden="true"><rect width="128" height="128" rx="28" fill="#8a1538"/><g stroke-linejoin="round" stroke-linecap="round"><g stroke="#FFDCD9" stroke-width="5" fill="none"><path d="M46 46 L54 54"/><path d="M74 74 L82 82"/></g><path d="M32 16 L48 32 L32 48 L16 32 Z" fill="#E6FFE6" stroke="#009700" stroke-width="4"/><circle cx="64" cy="64" r="15" fill="#FFFFFF" stroke="#B40F78" stroke-width="4"/><circle cx="64" cy="64" r="5" fill="#B40F78"/><rect x="80" y="80" width="32" height="32" rx="8" fill="#E4FAFA" stroke="#008585" stroke-width="4"/></g></svg>`;

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>${esc(title)}</title>
<style>${css}</style>
</head>
<body>
<header class="top">
  <button class="icon" id="treeToggle" title="Task tree">☰</button>
  ${brand}
  <div class="titles"><h1 title="${esc(title)}">${esc(title)}</h1><div class="sub">${esc(sub)}${m.desc ? ` — ${esc(m.desc)}` : ""}</div></div>
  ${attach ? `<button class="icon dl" id="downloadBtn" type="button" hidden title="${esc(`${attach.label}: ${attach.name} (${attach.summary})`)}" aria-label="${esc(attach.label)}"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v8M4.5 6.5 8 10l3.5-3.5M3 13h10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg><span class="dl-label">${esc(attach.label)}</span></button>` : ""}
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
      <ul class="tree tree-section" id="tree">${P("tree")}</ul>
      <div class="pane-title">Data</div>
      <ul class="tree tree-section" id="datatree">${P("datatree")}</ul>
    </aside>
    <div class="resizer" id="leftResizer" title="Drag to resize · double-click to reset"></div>
    <div class="canvas-wrap">
      <div class="crumbs" id="crumbs">${P("crumbs")}</div>
      <svg id="canvas" role="img" aria-label="Plan diagram"${pre.canvasAttrs}>${P("canvas")}</svg>
      <div class="toolbar">
        <button id="zin" title="Zoom in (+)">+</button>
        <button id="zout" title="Zoom out (−)">−</button>
        <button id="zfit" title="Fit to view (0)">⤢</button>
        <button id="z100" title="Actual size">1:1</button>
        <div class="sep"></div>
        <button id="zup" title="Up to the parent plan (Backspace)">↑</button>
        <button id="zexpand" title="Show all levels: sub-plans drawn inline (E)">⧉</button>
        <button id="zlayout" title="Layout: stored positions (click for automatic)">⊞</button>
        <button id="zlabel" title="Show captions instead of names">Aa</button>
      </div>
      <div class="hint">Drag to pan · wheel or pinch to zoom · double-click a plan to open it · ⧉ shows all levels</div>
      <div class="zoomlabel" id="zoomlabel">${P("zoomlabel") || "100%"}</div>
    </div>
    <div class="resizer right" id="rightResizer" title="Drag to resize · double-click to reset"></div>
    <aside class="right">
      <div class="pane-title">Properties</div>
      <div class="props" id="props">${P("props")}</div>
    </aside>
  </section>
  <section class="view doc" id="dataview"></section>
  <section class="view doc" id="decisionview"></section>
  ${notesHtml ? `<section class="view doc" id="notesview"><div class="inner notes card">${notesHtml}</div></section>` : ""}
  <section class="view doc" id="lintview"></section>
  <section class="view doc" id="sourceview"></section>
</main>
${attach ? `<script type="application/json" id="pf-attachments">${attach.json}</script>
` : ""}<script type="application/json" id="pf-data">${payload}</script>
<script>${js}</script>
</body>
</html>
`;

// the page must stay self-contained: everything is inlined, nothing is fetched
// markup and CSS are what a browser would fetch from; the guideline text in the data payload is not
const markup = html.replace(payload, "").replace(attach ? attach.json : "", "");
const external = /<link\b|<(?:script|img|iframe)[^>]+\bsrc=|(?:src|href)="https?:/i.exec(markup) || /@import|url\(\s*['"]?https?:/i.exec(css);
if (external) {
  console.error(`build_report: refusing to write a page with an external reference: ${external[0]}`);
  process.exit(2);
}
fs.mkdirSync(path.dirname(path.resolve(opts.out)), { recursive: true });
fs.writeFileSync(opts.out, html);
console.log(`wrote ${opts.out} (${Math.round(html.length / 1024)} KB)`);
