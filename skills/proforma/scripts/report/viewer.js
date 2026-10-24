/* PROforma guideline viewer (client side). Reads the embedded `proforma export` model
   (#pf-data) and renders a Composer-style workspace: task tree, plan canvas with
   pan / zoom / drill-down, and a properties panel; plus data, decision, lint and
   source views. No external dependencies. */
(function () {
  "use strict";
  const D = JSON.parse(document.getElementById("pf-data").textContent);
  const G = D.model;
  const tasks = new Map(G.tasks.map((t) => [t.name, t]));
  const dataItems = new Map(G.data.map((d) => [d.name, d]));
  const $ = (s, el) => (el || document).querySelector(s);
  const esc = (s) =>
    String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  const NS = "http://www.w3.org/2000/svg";
  const svgEl = (name, attrs, parent) => {
    const e = document.createElementNS(NS, name);
    for (const k in attrs || {}) e.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(e);
    return e;
  };

  // evidence grades such as [I, A] or [IV-V, B]
  const LEVEL = "(?:I{1,3}|IV|V)";
  const GRADE_RE = new RegExp("\\[(" + LEVEL + "(?:\\s*[-–]\\s*" + LEVEL + ")?),\\s*([A-E])\\]", "g");
  const rich = (s) => esc(s).replace(GRADE_RE, (m, l, g) => '<span class="grade g' + g + '">' + l + ", " + g + "</span>");

  // names in an expression become links to the task / data item they refer to
  const linkExpr = (expr) =>
    esc(expr).replace(/\b[A-Za-z_][A-Za-z0-9_]*\b/g, (w) =>
      tasks.has(w) ? '<a class="ref" data-task="' + w + '">' + w + "</a>" : dataItems.has(w) ? '<a class="ref" data-data="' + w + '">' + w + "</a>" : w
    );

  // ---------------------------------------------------------------- icons
  const KIND_STYLE = {
    plan: ["var(--plan-fill)", "var(--plan-stroke)"],
    action: ["var(--action-fill)", "var(--action-stroke)"],
    enquiry: ["var(--enquiry-fill)", "var(--enquiry-stroke)"],
    decision: ["var(--decision-fill)", "var(--decision-stroke)"],
    task: ["var(--task-fill)", "var(--task-stroke)"],
    data: ["var(--panel-2)", "var(--muted)"],
  };
  function iconSvg(kind) {
    const [f, s] = KIND_STYLE[kind] || KIND_STYLE.task;
    const body = {
      plan: '<rect x="1.5" y="3.5" width="13" height="9" rx="3" fill="' + f + '" stroke="' + s + '" stroke-width="1.5"/>',
      action: '<rect x="2.5" y="2.5" width="11" height="11" rx="2.5" fill="' + f + '" stroke="' + s + '" stroke-width="1.5"/>',
      enquiry: '<path d="M8 1.5 14.5 8 8 14.5 1.5 8Z" fill="' + f + '" stroke="' + s + '" stroke-width="1.5"/>',
      decision: '<circle cx="8" cy="8" r="6" fill="' + f + '" stroke="' + s + '" stroke-width="1.8"/><circle cx="8" cy="8" r="2" fill="' + s + '"/>',
      task: '<path d="M3 13 5 3h6l2 10Z" fill="' + f + '" stroke="' + s + '" stroke-width="1.5"/>',
      data: '<ellipse cx="8" cy="4.5" rx="5.5" ry="2.5" fill="' + f + '" stroke="' + s + '"/><path d="M2.5 4.5v7c0 1.4 2.5 2.5 5.5 2.5s5.5-1.1 5.5-2.5v-7" fill="' + f + '" stroke="' + s + '"/>',
    }[kind] || "";
    return '<svg class="ico" viewBox="0 0 16 16" aria-hidden="true">' + body + "</svg>";
  }

  // ---------------------------------------------------------------- state
  const state = { plan: G.root, trail: [G.root], sel: null, layout: "stored", label: "name" };
  const view = { x: 0, y: 0, k: 1 };

  // ---------------------------------------------------------------- tabs
  for (const b of document.querySelectorAll("nav.tabs button")) {
    b.onclick = () => showTab(b.dataset.tab);
  }
  function showTab(id) {
    for (const b of document.querySelectorAll("nav.tabs button")) b.classList.toggle("on", b.dataset.tab === id);
    for (const v of document.querySelectorAll(".view")) v.classList.toggle("on", v.id === id);
    if (id === "workspace") requestAnimationFrame(() => fit(false));
  }

  // ---------------------------------------------------------------- tree
  function buildTree() {
    const ul = $("#tree");
    ul.innerHTML = "";
    const add = (parentUl, taskName, compName, path, depth) => {
      const t = tasks.get(taskName);
      const li = document.createElement("li");
      const kids = t && t.kind === "plan" && !path.includes(taskName) ? t.components : [];
      const row = document.createElement("div");
      row.className = "row";
      row.dataset.task = taskName;
      row.title = t && t.caption ? t.caption : taskName;
      row.innerHTML =
        '<span class="twist">' + (kids.length ? (depth < 2 ? "▾" : "▸") : "") + "</span>" + iconSvg(t ? t.kind : "task") +
        '<span class="nm">' + esc(compName || taskName) + "</span>";
      li.appendChild(row);
      parentUl.appendChild(li);
      if (kids.length) {
        const sub = document.createElement("ul");
        if (depth >= 2) sub.hidden = true;
        li.appendChild(sub);
        for (const c of kids) add(sub, c.task, c.name, path.concat(taskName), depth + 1);
        row.querySelector(".twist").onclick = (e) => {
          e.stopPropagation();
          sub.hidden = !sub.hidden;
          e.target.textContent = sub.hidden ? "▸" : "▾";
        };
      }
      row.onclick = () => selectTask(taskName, { open: true });
      row.ondblclick = () => t && t.kind === "plan" && openPlan(taskName);
    };
    if (G.root) add(ul, G.root, null, [], 0);
    const orphans = G.tasks.filter((t) => !t.reachable);
    if (orphans.length) {
      const li = document.createElement("li");
      li.innerHTML = '<div class="row muted"><span class="twist"></span>Not in any plan</div>';
      ul.appendChild(li);
      const sub = document.createElement("ul");
      li.appendChild(sub);
      for (const t of orphans) add(sub, t.name, null, [], 3);
    }
    const dl = $("#datatree");
    dl.innerHTML = "";
    for (const d of G.data) {
      const li = document.createElement("li");
      li.innerHTML =
        '<div class="row" data-data="' + esc(d.name) + '"><span class="twist"></span>' + iconSvg("data") + '<span class="nm">' + esc(d.name) + "</span></div>";
      li.firstChild.onclick = () => selectData(d.name);
      dl.appendChild(li);
    }
  }
  function markTree() {
    for (const r of document.querySelectorAll("ul.tree .row")) {
      const on = state.sel && ((state.sel.type === "task" && r.dataset.task === state.sel.name) || (state.sel.type === "data" && r.dataset.data === state.sel.name));
      r.classList.toggle("sel", !!on);
    }
  }

  // ---------------------------------------------------------------- canvas
  const svg = $("#canvas");
  const vp = svgEl("g", {}, svg);
  const NODE = 64;

  function planLayout(plan) {
    const comps = plan.components;
    const stored = comps.map((c) => (c.ltwh ? { x: c.ltwh[0], y: c.ltwh[1] } : null));
    const seen = new Set();
    let usable = stored.every(Boolean);
    for (const p of stored) {
      if (!p) break;
      const k = p.x + "," + p.y;
      if (seen.has(k)) usable = false;
      seen.add(k);
    }
    // the CLI's default placement stacks every new component in one column; when that
    // hides parallel siblings (same scheduling depth), the stored layout misleads
    const depthOf = (c, seenNames) => (c.after.length ? 1 + Math.max(...c.after.map((a) => {
      const x = comps.find((y) => y.name === a);
      return x && !seenNames.includes(a) ? depthOf(x, seenNames.concat(a)) : 0;
    })) : 0);
    if (usable && comps.length > 2 && new Set(stored.map((p) => p.x)).size === 1) {
      const depths = comps.map((c) => depthOf(c, [c.name]));
      if (new Set(depths).size < depths.length) usable = false;
    }
    state.autoUsed = !(state.layout === "stored" && usable);
    if (state.layout === "stored" && usable) {
      // Composer stores icon positions; spread them a little so labels fit
      return comps.map((c, i) => ({ x: stored[i].x * 1.6, y: stored[i].y * 1.25 }));
    }
    // auto: layers by scheduling depth, one row per layer
    const byName = new Map(comps.map((c) => [c.name, c]));
    const level = new Map();
    const lv = (c, stack) => {
      if (level.has(c.name)) return level.get(c.name);
      if (stack.includes(c.name)) return 0;
      const ante = c.after.map((a) => byName.get(a)).filter(Boolean);
      const v = ante.length ? Math.max(...ante.map((a) => lv(a, stack.concat(c.name)))) + 1 : 0;
      level.set(c.name, v);
      return v;
    };
    comps.forEach((c) => lv(c, []));
    const rows = new Map();
    comps.forEach((c, i) => {
      const l = level.get(c.name);
      if (!rows.has(l)) rows.set(l, []);
      rows.get(l).push(i);
    });
    // wide layers wrap into a grid so many parallel tasks stay readable
    const PER_ROW = 5;
    const pos = [];
    const width = Math.min(PER_ROW, Math.max(...[...rows.values()].map((r) => r.length)));
    let y = 40;
    for (const l of [...rows.keys()].sort((a, b) => a - b)) {
      const idx = rows.get(l);
      for (let start = 0; start < idx.length; start += PER_ROW) {
        const chunk = idx.slice(start, start + PER_ROW);
        chunk.forEach((i, j) => {
          pos[i] = { x: 40 + (j + (width - chunk.length) / 2) * 170, y };
        });
        y += 150;
      }
    }
    return pos;
  }

  function branchLabel(target, anteTask) {
    // a decision antecedent whose result the target's precondition tests
    if (!target.precondition || !anteTask || anteTask.kind !== "decision") return "";
    const re = new RegExp("result_(?:of|set)\\(\\s*'?" + anteTask.name + "'?\\s*\\)\\s*(?:=|includes)\\s*'?([A-Za-z_][A-Za-z0-9_]*)", "g");
    const out = [];
    let m;
    while ((m = re.exec(target.precondition))) out.push(m[1]);
    return out.join(" / ");
  }

  function drawShape(g, kind, cx, cy) {
    const [f, s] = KIND_STYLE[kind] || KIND_STYLE.task;
    const r = NODE / 2;
    const common = { class: "shape", fill: f, stroke: s };
    if (kind === "plan") {
      svgEl("rect", Object.assign({ x: cx - 34, y: cy - 24, width: 68, height: 48, rx: 14 }, common), g);
      for (let i = -1; i <= 1; i++) svgEl("rect", { x: cx + i * 14 - 5, y: cy - 5, width: 10, height: 10, rx: 2, fill: s, opacity: 0.55 }, g);
      return 26;
    }
    if (kind === "action") {
      svgEl("rect", Object.assign({ x: cx - 25, y: cy - 25, width: 50, height: 50, rx: 10 }, common), g);
      svgEl("path", { d: "M" + (cx - 10) + " " + cy + " l7 7 l13 -15", fill: "none", stroke: s, "stroke-width": 3, "stroke-linecap": "round", "stroke-linejoin": "round" }, g);
      return 27;
    }
    if (kind === "enquiry") {
      svgEl("path", Object.assign({ d: "M" + cx + " " + (cy - r) + " L" + (cx + r) + " " + cy + " L" + cx + " " + (cy + r) + " L" + (cx - r) + " " + cy + "Z" }, common), g);
      const q = svgEl("text", { x: cx, y: cy + 1, "text-anchor": "middle", "dominant-baseline": "central", fill: s, "font-size": 20, "font-weight": 700 }, g);
      q.textContent = "?";
      return r - 2;
    }
    if (kind === "decision") {
      svgEl("circle", Object.assign({ cx, cy, r: 27, "stroke-width": 3 }, common), g);
      svgEl("circle", { cx, cy, r: 7, fill: s }, g);
      return 28;
    }
    svgEl("path", Object.assign({ d: "M" + (cx - 26) + " " + (cy + 22) + " L" + (cx - 16) + " " + (cy - 22) + " H" + (cx + 16) + " L" + (cx + 26) + " " + (cy + 22) + "Z" }, common), g);
    return 26;
  }

  function wrapLabel(text, max) {
    const words = String(text).split(/\s+/);
    const lines = [];
    let cur = "";
    for (const w of words) {
      if ((cur + " " + w).trim().length > max && cur) {
        lines.push(cur);
        cur = w;
      } else cur = (cur + " " + w).trim();
    }
    if (cur) lines.push(cur);
    if (lines.length > 3) {
      lines.length = 3;
      lines[2] = lines[2].replace(/.{0,2}$/, "") + "…";
    }
    return lines;
  }

  function render() {
    vp.innerHTML = "";
    const plan = tasks.get(state.plan);
    renderCrumbs();
    if (!plan) return;
    const defs = svgEl("defs", {}, vp);
    const mk = svgEl("marker", { id: "arrow", viewBox: "0 0 10 10", refX: 9, refY: 5, markerWidth: 9, markerHeight: 9, orient: "auto-start-reverse" }, defs);
    svgEl("path", { d: "M0 0 L10 5 L0 10 z", fill: "var(--edge)" }, mk);

    const pos = planLayout(plan);
    const centers = new Map();
    const radius = new Map();
    const boxes = new Map();
    // where the ray from (cx, cy) towards (tx, ty) leaves a node's box
    const clip = (b, cx, cy, tx, ty) => {
      const dx = tx - cx;
      const dy = ty - cy;
      let t = 1;
      if (dx > 0) t = Math.min(t, (b.x2 - cx) / dx);
      if (dx < 0) t = Math.min(t, (b.x1 - cx) / dx);
      if (dy > 0) t = Math.min(t, (b.y2 - cy) / dy);
      if (dy < 0) t = Math.min(t, (b.y1 - cy) / dy);
      return { x: cx + dx * t, y: cy + dy * t };
    };
    plan.components.forEach((c, i) => centers.set(c.name, { x: pos[i].x + NODE / 2, y: pos[i].y + NODE / 2 }));
    const edgeLayer = svgEl("g", {}, vp);
    const nodeLayer = svgEl("g", {}, vp);

    plan.components.forEach((c) => {
      const t = tasks.get(c.task) || { kind: "task", name: c.task };
      const { x: cx, y: cy } = centers.get(c.name);
      const g = svgEl("g", { class: "node" + (c.optional ? " optional" : "") + (c.terminal ? " terminal" : ""), "data-task": c.task }, nodeLayer);
      svgEl("rect", { class: "halo", x: cx - 40, y: cy - 38, width: 80, height: 76, rx: 12 }, g);
      radius.set(c.name, drawShape(g, t.kind, cx, cy));
      const nLines = wrapLabel(state.label === "caption" && t.caption ? t.caption : c.name, state.label === "caption" ? 26 : 22).length;
      boxes.set(c.name, { x1: cx - 36, x2: cx + 36, y1: cy - 32, y2: cy + 36 + nLines * 14 + (t.kind === "plan" ? 12 : 0) });
      // badges: precondition (if) and cycles
      if (t.precondition) {
        svgEl("circle", { cx: cx - 30, cy: cy - 26, r: 9, fill: "var(--decision-stroke)" }, g);
        svgEl("text", { class: "badge", x: cx - 30, y: cy - 26 }, g).textContent = "if";
      }
      if (c.cycle_repeat || c.number_of_cycles || c.cycle_until) {
        svgEl("circle", { cx: cx + 30, cy: cy - 26, r: 9, fill: "var(--action-stroke)" }, g);
        svgEl("text", { class: "badge", x: cx + 30, y: cy - 26 }, g).textContent = "↻";
      }
      const label = state.label === "caption" && t.caption ? t.caption : c.name;
      const lines = wrapLabel(label, state.label === "caption" ? 26 : 22);
      lines.forEach((ln, i) => {
        svgEl("text", { class: "lbl", x: cx, y: cy + 46 + i * 14 }, g).textContent = ln;
      });
      if (t.kind === "plan") {
        svgEl("text", { class: "plan-drill", x: cx, y: cy + 46 + lines.length * 14 }, g).textContent =
          "▸ " + (t.components ? t.components.length : 0) + " tasks · double-click to open";
      }
      const title = svgEl("title", {}, g);
      title.textContent = (t.caption ? t.caption + "\n" : "") + t.kind + " " + c.task + (t.precondition ? "\nprecondition: " + t.precondition : "");
      g.addEventListener("click", (e) => {
        e.stopPropagation();
        selectTask(c.task, { open: false });
      });
      g.addEventListener("dblclick", (e) => {
        e.stopPropagation();
        if (t.kind === "plan") openPlan(c.task);
      });
      if (state.sel && state.sel.type === "task" && state.sel.name === c.task) g.classList.add("sel");
    });

    plan.components.forEach((c) => {
      const t = tasks.get(c.task);
      for (const a of c.after) {
        const p = centers.get(a);
        const q = centers.get(c.name);
        if (!p || !q) continue;
        const s1 = clip(boxes.get(a), p.x, p.y, q.x, q.y);
        const s2 = clip(boxes.get(c.name), q.x, q.y, p.x, p.y);
        const x1 = s1.x;
        const y1 = s1.y + 2;
        const x2 = s2.x;
        const y2 = s2.y - 2;
        svgEl("path", { class: "edge", d: "M" + x1 + " " + y1 + " L" + x2 + " " + y2, "marker-end": "url(#arrow)" }, edgeLayer);
        const anteComp = plan.components.find((x) => x.name === a);
        const lbl = t ? branchLabel(t, tasks.get(anteComp ? anteComp.task : a)) : "";
        if (lbl) {
          // beside a mostly vertical edge, above a mostly horizontal one
          const vertical = Math.abs(x2 - x1) < Math.abs(y2 - y1);
          svgEl("text", vertical
            ? { class: "edge-label", x: (x1 + x2) / 2 + 10, y: (y1 + y2) / 2 + 4, "text-anchor": "start" }
            : { class: "edge-label", x: (x1 + x2) / 2, y: (y1 + y2) / 2 - 6 }, edgeLayer).textContent = lbl;
        }
      }
    });
    applyView();
    $("#zlayout").classList.toggle("on", !!state.autoUsed);
  }

  function renderCrumbs() {
    const el = $("#crumbs");
    const plan = tasks.get(state.plan);
    el.innerHTML = state.trail
      .map((p, i) =>
        i === state.trail.length - 1 ? '<span class="here">' + esc(p) + "</span>" : '<a data-i="' + i + '">' + esc(p) + '</a><span class="sep">›</span>'
      )
      .join("") + (plan && plan.caption ? '<span class="plan-cap" title="' + esc(plan.caption) + '">' + esc(plan.caption) + "</span>" : "");
    for (const a of el.querySelectorAll("a")) {
      a.onclick = () => {
        const i = Number(a.dataset.i);
        state.trail = state.trail.slice(0, i + 1);
        state.plan = state.trail[i];
        render();
        fit(false);
      };
    }
  }

  function openPlan(name, trail) {
    const t = tasks.get(name);
    if (!t || t.kind !== "plan") return;
    if (trail) state.trail = trail;
    else {
      const at = state.trail.indexOf(name);
      state.trail = at >= 0 ? state.trail.slice(0, at + 1) : state.trail.concat(name);
    }
    state.plan = name;
    selectTask(name, { open: false, silent: true });
    render();
    fit(false);
  }

  // path from the root to the plan that contains a task (first path in the export)
  function trailTo(planName) {
    const t = tasks.get(planName);
    if (!t || !t.paths || !t.paths.length) return [planName];
    // paths are component names; map them to task names along the way
    const parts = t.paths[0].split("/");
    const out = [parts[0]];
    let cur = tasks.get(parts[0]);
    for (const p of parts.slice(1)) {
      const comp = cur && cur.components.find((c) => c.name === p);
      if (!comp) break;
      out.push(comp.task);
      cur = tasks.get(comp.task);
    }
    return out;
  }

  // ---------------------------------------------------------------- pan / zoom
  function applyView() {
    vp.setAttribute("transform", "translate(" + view.x + "," + view.y + ") scale(" + view.k + ")");
    $("#zoomlabel").textContent = Math.round(view.k * 100) + "%";
  }
  function fit(animate) {
    const box = vp.getBBox();
    const W = svg.clientWidth;
    const H = svg.clientHeight;
    if (!box.width || !W || !H) return;
    const k = Math.min(1.4, Math.max(0.15, Math.min((W - 60) / box.width, (H - 60) / box.height)));
    view.k = k;
    view.x = (W - box.width * k) / 2 - box.x * k;
    view.y = (H - box.height * k) / 2 - box.y * k;
    applyView();
  }
  function zoomAt(f, px, py) {
    const k = Math.min(4, Math.max(0.1, view.k * f));
    const r = k / view.k;
    view.x = px - (px - view.x) * r;
    view.y = py - (py - view.y) * r;
    view.k = k;
    applyView();
  }
  svg.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      const rect = svg.getBoundingClientRect();
      zoomAt(Math.exp(-e.deltaY * 0.0015), e.clientX - rect.left, e.clientY - rect.top);
    },
    { passive: false }
  );
  let drag = null;
  const pointers = new Map();
  svg.addEventListener("pointerdown", (e) => {
    // nodes keep their own click / double-click; pan only from the background
    if (e.target.closest && e.target.closest(".node") && e.pointerType === "mouse") return;
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 1) {
      drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false };
      svg.classList.add("panning");
    }
    if (e.pointerType === "mouse") svg.setPointerCapture(e.pointerId);
  });
  svg.addEventListener("pointermove", (e) => {
    if (!pointers.has(e.pointerId)) return;
    const prev = pointers.get(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 2) {
      // pinch zoom
      const [a, b] = [...pointers.values()];
      const other = [...pointers.entries()].find(([id]) => id !== e.pointerId)[1];
      const before = Math.hypot(prev.x - other.x, prev.y - other.y);
      const after = Math.hypot(a.x - b.x, a.y - b.y);
      const rect = svg.getBoundingClientRect();
      if (before > 0) zoomAt(after / before, (a.x + b.x) / 2 - rect.left, (a.y + b.y) / 2 - rect.top);
      return;
    }
    if (!drag) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
    view.x = drag.vx + dx;
    view.y = drag.vy + dy;
    applyView();
  });
  const endDrag = (e) => {
    pointers.delete(e.pointerId);
    if (pointers.size === 0) {
      svg.classList.remove("panning");
      drag = null;
    }
  };
  svg.addEventListener("pointerup", endDrag);
  svg.addEventListener("pointercancel", endDrag);
  $("#zin").onclick = () => zoomAt(1.25, svg.clientWidth / 2, svg.clientHeight / 2);
  $("#zout").onclick = () => zoomAt(0.8, svg.clientWidth / 2, svg.clientHeight / 2);
  $("#zfit").onclick = () => fit(true);
  $("#z100").onclick = () => {
    const cx = svg.clientWidth / 2;
    const cy = svg.clientHeight / 2;
    zoomAt(1 / view.k, cx, cy);
  };
  $("#zup").onclick = () => {
    if (state.trail.length > 1) openPlan(state.trail[state.trail.length - 2], state.trail.slice(0, -1));
  };
  $("#zlayout").onclick = (e) => {
    state.layout = state.layout === "stored" ? "auto" : "stored";
    e.currentTarget.classList.toggle("on", state.layout === "auto");
    e.currentTarget.title = state.layout === "auto" ? "Layout: automatic (click for stored positions)" : "Layout: stored positions (click for automatic)";
    render();
    fit(false);
  };
  $("#zlabel").onclick = (e) => {
    state.label = state.label === "name" ? "caption" : "name";
    e.currentTarget.classList.toggle("on", state.label === "caption");
    render();
  };
  window.addEventListener("resize", () => fit(false));
  document.addEventListener("keydown", (e) => {
    if (!$("#workspace").classList.contains("on") || e.target.closest("input,textarea")) return;
    if (e.key === "+" || e.key === "=") $("#zin").click();
    else if (e.key === "-") $("#zout").click();
    else if (e.key === "0") $("#zfit").click();
    else if (e.key === "Backspace" || (e.key === "ArrowUp" && e.altKey)) $("#zup").click();
  });

  // ---------------------------------------------------------------- selection + properties
  function selectTask(name, opts) {
    const t = tasks.get(name);
    state.sel = { type: "task", name };
    if (opts && opts.open && t) {
      if (t.kind === "plan") {
        openPlan(name, trailTo(name));
        showProps();
        return;
      }
      // show the plan that contains this task
      const parent = t.used_by && t.used_by[0];
      if (parent && parent !== state.plan) openPlan(parent, trailTo(parent));
    }
    if (!opts || !opts.silent) {
      for (const n of vp.querySelectorAll(".node")) n.classList.toggle("sel", n.dataset.task === name);
    }
    showProps();
  }
  function selectData(name) {
    state.sel = { type: "data", name };
    showProps();
  }

  function metapropsHtml(mp) {
    const keys = Object.keys(mp || {});
    if (!keys.length) return "";
    const groups = [
      ["Goal", (k) => k.startsWith("goal.")],
      ["ServiceRequest", (k) => k.startsWith("ServiceRequest.")],
      ["Sources", (k) => k === "source" || /^source\.\d+$/.test(k)],
      ["Provenance and operationalization", () => true],
    ];
    const used = new Set();
    let html = "";
    for (const [title, test] of groups) {
      const ks = keys.filter((k) => !used.has(k) && test(k));
      if (!ks.length) continue;
      ks.forEach((k) => used.add(k));
      html += "<h3>" + esc(title) + '</h3><table class="mp">' +
        ks.map((k) => '<tr><td class="k">' + esc(k).replace(/./g, ".<wbr>") + "</td><td>" + rich(mp[k]) + "</td></tr>").join("") + "</table>";
    }
    return html;
  }

  function kv(rows) {
    const r = rows.filter((x) => x[1] != null && x[1] !== "" && x[1] !== false);
    if (!r.length) return "";
    return '<dl class="kv">' + r.map(([k, v]) => "<dt>" + esc(k) + "</dt><dd>" + v + "</dd>").join("") + "</dl>";
  }

  function showProps() {
    const el = $("#props");
    markTree();
    if (!state.sel) {
      el.innerHTML = '<div class="empty">Select a task on the canvas or in the tree. Double-click a plan to open it.</div>';
      return;
    }
    if (state.sel.type === "data") {
      const d = dataItems.get(state.sel.name);
      el.innerHTML =
        "<h2>" + iconSvg("data") + esc(d.name) + '</h2><div class="kindline">data item · ' + esc(d.type || "untyped") + "</div>" +
        kv([
          ["Caption", d.caption && rich(d.caption)],
          ["Description", d.description && rich(d.description)],
          ["Type", d.type && "<code>" + esc(d.type) + "</code>"],
          ["Unit", d.unit && esc(d.unit)],
          ["Range", d.range.length > 0 && d.range.map((r) => "<code>" + esc(r) + "</code>").join(" ")],
          ["Default", d.default_value && "<code>" + esc(d.default_value) + "</code>"],
          ["Derivation", d.derivation && "<code>" + linkExpr(d.derivation) + "</code>"],
          ["Asked by", d.asked_by.map((a) => '<a class="ref" data-task="' + esc(a) + '">' + esc(a) + "</a>").join(", ")],
          ["Used in", usedIn(d.name)],
        ]) + metapropsHtml(d.metaprops);
      wireRefs(el);
      return;
    }
    const t = tasks.get(state.sel.name);
    if (!t) return;
    const comp = findComponent(t.name);
    let html =
      "<h2>" + iconSvg(t.kind) + esc(t.name) + '</h2><div class="kindline">' + esc(t.kind) + (comp ? " · in plan " + esc(comp.plan) : t.name === G.root ? " · root plan" : "") + "</div>" +
      kv([
        ["Caption", t.caption && rich(t.caption)],
        ["Description", t.description && rich(t.description)],
        ["Precondition", t.precondition && "<code>" + linkExpr(t.precondition) + "</code>"],
        ["Wait condition", t.wait_condition && "<code>" + linkExpr(t.wait_condition) + "</code>"],
        ["Postcondition", t.postcondition && "<code>" + linkExpr(t.postcondition) + "</code>"],
        ["Goal (expr)", t.goal && "<code>" + linkExpr(t.goal) + "</code>"],
        ["Procedure", t.procedure && rich(t.procedure)],
        ["Trigger", t.trigger && esc(t.trigger)],
        ["Terminate", t.terminate && "<code>" + linkExpr(t.terminate) + "</code>"],
        ["Abort", t.abort && "<code>" + linkExpr(t.abort) + "</code>"],
        ["Starts after", comp && comp.c.after.length > 0 && comp.c.after.map((a) => '<a class="ref" data-task="' + esc(a) + '">' + esc(a) + "</a>").join(", ")],
        ["Cycles", comp && [comp.c.number_of_cycles && "number " + esc(comp.c.number_of_cycles), comp.c.cycle_repeat && "repeat every " + esc(comp.c.cycle_repeat), comp.c.cycle_until && "until <code>" + linkExpr(comp.c.cycle_until) + "</code>"].filter(Boolean).join("; ")],
        ["Flags", comp && ["optional", "autonomous", "terminal"].filter((f) => comp.c[f]).join(", ")],
        ["Choice", t.kind === "decision" && esc((t.choice_mode || "single") + " choice, " + (t.support_mode || "symbolic") + " support")],
      ]);
    if (t.kind === "plan" && t.components.length) {
      html += "<h3>Components</h3><div>" + t.components.map((c) => '<a class="ref" data-task="' + esc(c.task) + '">' + esc(c.name) + "</a>").join(", ") +
        '</div><p><button class="icon" id="openplan">Open plan ▸</button></p>';
    }
    if (t.sources.length) {
      html += "<h3>Asks for (sources)</h3><table class=\"mp\">" + t.sources.map((s) => {
        const d = dataItems.get(s.data);
        return '<tr><td class="k"><a class="ref" data-data="' + esc(s.data) + '">' + esc(s.data) + "</a></td><td>" + (d ? esc(d.type || "") + (d.unit ? " (" + esc(d.unit) + ")" : "") + (d.description ? " — " + rich(d.description) : "") : "") + "</td></tr>";
      }).join("") + "</table>";
    }
    if (t.candidates.length) {
      html += "<h3>Candidates</h3>" + t.candidates.map((c) =>
        '<div class="cand"><div class="cn">' + esc(c.name) + (c.caption ? ' <span class="tag">' + rich(c.caption) + "</span>" : "") + "</div>" +
        (c.description ? '<div class="muted">' + rich(c.description) + "</div>" : "") +
        '<ul class="args">' + (c.arguments.length ? c.arguments.map((a) =>
          '<li><span class="sup ' + esc(a.support) + '">' + esc(a.support) + "</span><code>" + linkExpr(a.condition) + "</code>" +
          (a.caption ? '<div class="muted">' + rich(a.caption) + "</div>" : "") + "</li>").join("") : '<li class="muted">no arguments</li>') + "</ul>" +
        '<div class="muted">recommended when <code>' + linkExpr(c.recommendation || "netsupport() > 0") + "</code></div></div>").join("");
    }
    html += metapropsHtml(t.metaprops);
    el.innerHTML = html;
    const ob = $("#openplan", el);
    if (ob) ob.onclick = () => openPlan(t.name, trailTo(t.name));
    wireRefs(el);
  }
  function findComponent(taskName) {
    for (const p of G.tasks) {
      if (p.kind !== "plan") continue;
      const c = p.components.find((x) => x.task === taskName);
      if (c) return { plan: p.name, c };
    }
    return null;
  }
  function usedIn(dataName) {
    const re = new RegExp("\\b" + dataName + "\\b");
    const hits = [];
    for (const t of G.tasks) {
      const exprs = [t.precondition, t.wait_condition, ...t.candidates.flatMap((c) => [c.recommendation, ...c.arguments.map((a) => a.condition)])];
      if (exprs.some((e) => e && re.test(e))) hits.push(t.name);
    }
    return hits.map((h) => '<a class="ref" data-task="' + esc(h) + '">' + esc(h) + "</a>").join(", ");
  }
  function wireRefs(root) {
    for (const a of root.querySelectorAll("a.ref")) {
      a.onclick = () => {
        if (a.dataset.task) {
          showTab("workspace");
          selectTask(a.dataset.task, { open: true });
        } else if (a.dataset.data) {
          showTab("workspace");
          selectData(a.dataset.data);
        }
      };
    }
  }

  // ---------------------------------------------------------------- other views
  function buildDataView() {
    const rows = G.data.map((d) =>
      "<tr><td><a class=\"ref\" data-data=\"" + esc(d.name) + "\"><code>" + esc(d.name) + "</code></a></td><td>" + esc(d.type || "") + (d.unit ? " (" + esc(d.unit) + ")" : "") +
      "</td><td>" + rich(d.caption || d.description || "") + "</td><td>" + (d.range.length ? d.range.map((r) => "<code>" + esc(r) + "</code>").join(" ") : d.derivation ? "derived: <code>" + esc(d.derivation) + "</code>" : '<span class="muted">—</span>') +
      "</td><td>" + d.asked_by.map((a) => '<a class="ref" data-task="' + esc(a) + '">' + esc(a) + "</a>").join(", ") + "</td></tr>").join("");
    $("#dataview").innerHTML = '<div class="inner"><h2>Data definitions</h2><div class="card tablewrap"><table class="grid"><thead><tr><th>Name</th><th>Type</th><th>Description</th><th>Values</th><th>Asked by</th></tr></thead><tbody>' +
      (rows || '<tr><td colspan="5" class="muted">No data items.</td></tr>') + "</tbody></table></div></div>";
    wireRefs($("#dataview"));
  }
  function buildDecisionView() {
    const ds = G.tasks.filter((t) => t.kind === "decision");
    $("#decisionview").innerHTML = '<div class="inner"><h2>Decisions</h2>' + (ds.length ? ds.map((d) =>
      '<section class="card"><h3>' + rich(d.caption || d.name) + ' <span class="tag">' + esc(d.name) + " · " + esc(d.choice_mode || "single") + " choice</span></h3>" +
      (d.description ? "<p>" + rich(d.description) + "</p>" : "") +
      '<div class="tablewrap"><table class="grid"><thead><tr><th>Candidate</th><th>Arguments</th><th>Recommended when</th></tr></thead><tbody>' +
      d.candidates.map((c) => "<tr><td><b>" + esc(c.name) + "</b>" + (c.caption ? '<div class="muted">' + rich(c.caption) + "</div>" : "") + (c.description ? '<div class="muted">' + rich(c.description) + "</div>" : "") +
        '</td><td><ul class="args">' + (c.arguments.length ? c.arguments.map((a) => '<li><span class="sup ' + esc(a.support) + '">' + esc(a.support) + "</span><code>" + linkExpr(a.condition) + "</code>" + (a.caption ? '<div class="muted">' + rich(a.caption) + "</div>" : "") + "</li>").join("") : '<li class="muted">no arguments</li>') +
        "</ul></td><td><code>" + linkExpr(c.recommendation || "netsupport() > 0") + "</code></td></tr>").join("") + "</tbody></table></div></section>").join("") : '<p class="muted">No decisions.</p>') + "</div>";
    wireRefs($("#decisionview"));
  }
  function buildLintView() {
    const L = D.lint || [];
    $("#lintview").innerHTML = '<div class="inner"><h2>Lint</h2><div class="card tablewrap">' + (L.length
      ? '<table class="grid"><thead><tr><th>Severity</th><th>Code</th><th>Where</th><th>Message</th></tr></thead><tbody>' +
        L.map((d) => '<tr class="lint-row ' + esc(d.severity) + '"><td>' + esc(d.severity) + "</td><td><code>" + esc(d.code) + "</code></td><td>" + (d.component ? '<a class="ref" data-task="' + esc(d.component) + '">' + esc(d.component) + "</a>" : "") + (d.line ? " line " + d.line : "") + "</td><td>" + esc(d.message) + "</td></tr>").join("") + "</tbody></table>"
      : '<p>No findings: the guideline lints clean (Composer-equivalent checks).</p>') + "</div></div>";
    wireRefs($("#lintview"));
  }
  function buildSourceView() {
    const kw = /\b(plan|action|enquiry|decision|task|data|function|metadata|end|component|candidate|argument|source|precondition|wait_condition|postcondition|recommendation|schedule_constraint|task_definition|caption|description|metaprops|type|unit|range|choice_mode|support_mode|procedure|ltwh|attributes|argument_name|data_definition|engine_version|title|version|author|desc|default_value|derivation|cycle_repeat|number_of_cycles|optional|terminal|autonomous|goal|trigger)\b(?=\s*::|\.)/g;
    const html = esc(D.source || "")
      .replace(/(\/\*\*[\s\S]*?\*\*\/)/g, '<span class="cmt">$1</span>')
      .replace(/(&#39;(?:[^&]|&(?!#39;))*?&#39;)/g, '<span class="str">$1</span>')
      .replace(kw, '<span class="kw">$1</span>');
    $("#sourceview").innerHTML = '<div class="inner"><h2>PROforma source <span class="tag">' + esc(D.file || "") + "</span></h2><pre class=\"src\">" + html + "</pre></div>";
  }

  // ---------------------------------------------------------------- theme + mobile tree
  $("#themeToggle").onclick = () => {
    const r = document.documentElement;
    const dark = r.dataset.theme ? r.dataset.theme === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
    r.dataset.theme = dark ? "light" : "dark";
  };
  $("#treeToggle").onclick = () => $("aside.left").classList.toggle("show");

  // ---------------------------------------------------------------- start
  buildTree();
  buildDataView();
  buildDecisionView();
  buildLintView();
  buildSourceView();
  showProps();
  render();
  showTab("workspace");
})();
