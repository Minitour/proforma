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
    const width = Math.max(...[...rows.values()].map((r) => (r.some((i) => comps[i].after.length) ? r.length : Math.min(PER_ROW, r.length))));
    let y = 40;
    for (const l of [...rows.keys()].sort((a, b) => a - b)) {
      const idx = rows.get(l);
      const per = idx.some((i) => comps[i].after.length) ? idx.length : PER_ROW;
      for (let start = 0; start < idx.length; start += per) {
        const chunk = idx.slice(start, start + per);
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

  // Task icons as the PROforma plan editor draws them: small shapes with a diagonal
  // light-to-dark gradient and a 2 px outline, centred on (cx, cy). Returns the icon's
  // bounding rectangle, which is what scheduling arrows attach to.
  const ICON = {
    action: { light: "#E4FAFA", dark: "#6CDCDC", line: "#008585" },
    plan: { light: "#FFDCD9", dark: "#FFB5AE", line: "#E55344" },
    decision: { light: "#FFFFFF", dark: "#F8A2D3", line: "#B40F78" },
    enquiry: { light: "#E6FFE6", dark: "#82F082", line: "#009700" },
    task: { light: "#FEFBED", dark: "#F5DC7A", line: "#D9AB0A" },
  };
  const ICON_SCALE = 1.5; // the editor's icons, enlarged for reading at 100 %

  // icon height per kind, so a task's icon top can line up with the frames in its row
  const ICON_H = { plan: 35, action: 30, decision: 30, enquiry: 40, task: 36 };
  const iconHeight = (kind) => (ICON_H[kind] || ICON_H.task) * ICON_SCALE;

  function gradientFor(kind) {
    const id = "grad-" + kind;
    if (!svg.querySelector("#" + id)) {
      const c = ICON[kind] || ICON.task;
      let defs = svg.querySelector("defs.icons");
      if (!defs) defs = svgEl("defs", { class: "icons" }, svg);
      const g = svgEl("linearGradient", { id, x1: "100%", y1: "0%", x2: "0%", y2: "100%" }, defs);
      svgEl("stop", { offset: "0%", "stop-color": c.light }, g);
      svgEl("stop", { offset: "100%", "stop-color": c.dark }, g);
    }
    return "url(#" + id + ")";
  }

  function drawShape(g, kind, cx, cy) {
    const k = ICON[kind] ? kind : "task";
    const c = ICON[k];
    const s = ICON_SCALE;
    const common = { class: "shape", fill: gradientFor(k), stroke: c.line, "stroke-width": 2 };
    let w;
    let h;
    if (k === "plan") {
      w = 50 * s;
      h = 35 * s;
      svgEl("rect", Object.assign({ x: cx - w / 2, y: cy - h / 2, width: w, height: h, rx: 11 * s }, common), g);
    } else if (k === "action") {
      w = h = 30 * s;
      svgEl("rect", Object.assign({ x: cx - w / 2, y: cy - h / 2, width: w, height: h, rx: 2 * s }, common), g);
    } else if (k === "decision") {
      w = h = 30 * s;
      svgEl("circle", Object.assign({ cx, cy, r: 15 * s }, common), g);
    } else if (k === "enquiry") {
      w = h = 40 * s;
      const r = 20 * s;
      svgEl("path", Object.assign({ d: "M" + cx + " " + (cy - r) + " L" + (cx + r) + " " + cy + " L" + cx + " " + (cy + r) + " L" + (cx - r) + " " + cy + "Z" }, common), g);
    } else {
      w = 36 * s;
      h = 36 * s;
      const b = 10 * s;
      const t = 18 * s;
      svgEl("path", Object.assign({ d: "M" + (cx - b) + " " + (cy + h / 2) + " L" + (cx - t) + " " + (cy - h / 2) + " H" + (cx + t) + " L" + (cx + b) + " " + (cy + h / 2) + "Z" }, common), g);
    }
    return { x1: cx - w / 2, x2: cx + w / 2, y1: cy - h / 2, y2: cy + h / 2, cx, cy };
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

  // where the ray from (cx, cy) towards (tx, ty) leaves box b
  function clip(b, cx, cy, tx, ty) {
    const dx = tx - cx;
    const dy = ty - cy;
    let t = 1;
    if (dx > 0) t = Math.min(t, (b.x2 - cx) / dx);
    if (dx < 0) t = Math.min(t, (b.x1 - cx) / dx);
    if (dy > 0) t = Math.min(t, (b.y2 - cy) / dy);
    if (dy < 0) t = Math.min(t, (b.y1 - cy) / dy);
    return { x: cx + dx * t, y: cy + dy * t };
  }

  const labelOf = (c, t) => (state.label === "caption" && t.caption ? t.caption : c.name);
  const labelLines = (c, t) => wrapLabel(labelOf(c, t), state.label === "caption" ? 26 : 22);

  function badges(g, c, t, ifX, ifY, cycX, cycY) {
    if (t.precondition) {
      svgEl("circle", { cx: ifX, cy: ifY, r: 9, fill: "var(--decision-stroke)" }, g);
      svgEl("text", { class: "badge", x: ifX, y: ifY }, g).textContent = "if";
    }
    if (c.cycle_repeat || c.number_of_cycles || c.cycle_until) {
      svgEl("circle", { cx: cycX, cy: cycY, r: 9, fill: "var(--action-stroke)" }, g);
      svgEl("text", { class: "badge", x: cycX, y: cycY }, g).textContent = "↻";
    }
  }

  function wireNode(g, c, t) {
    const title = svgEl("title", {}, g);
    title.textContent = (t.caption ? t.caption + "\n" : "") + t.kind + " " + c.task + (t.precondition ? "\nprecondition: " + t.precondition : "");
    g.addEventListener("click", (e) => {
      e.stopPropagation();
      selectTask(c.task, { open: false });
    });
    g.addEventListener("dblclick", (e) => {
      e.stopPropagation();
      // a frame can sit several levels below the current plan: open it with its full path
      if (t.kind === "plan") openPlan(c.task, trailTo(c.task));
    });
    if (state.sel && state.sel.type === "task" && state.sel.name === c.task) g.classList.add("sel");
  }

  // one task as the editor shows it: the icon centred on (cx, cy) with its caption
  // below; returns the icon's rectangle, which arrows attach to
  function drawNode(layer, c, t, cx, cy) {
    const g = svgEl("g", { class: "node" + (c.optional ? " optional" : "") + (c.terminal ? " terminal" : ""), "data-task": c.task }, layer);
    const halo = svgEl("rect", { class: "halo", rx: 8 }, g);
    const icon = drawShape(g, t.kind, cx, cy);
    badges(g, c, t, icon.x1 - 4, icon.y1 - 4, icon.x2 + 4, icon.y1 - 4);
    const lines = labelLines(c, t);
    const top = icon.y2 + 4;
    lines.forEach((ln, i) => {
      svgEl("text", { class: "lbl", x: cx, y: top + 14 + i * 17 }, g).textContent = ln;
    });
    let bottom = top + lines.length * 17;
    if (t.kind === "plan") {
      svgEl("text", { class: "plan-drill", x: cx, y: bottom + 12 }, g).textContent =
        "▸ " + (t.components ? t.components.length : 0) + " tasks · double-click to open";
      bottom += 16;
    }
    const half = Math.max((icon.x2 - icon.x1) / 2, (Math.max(...lines.map((l) => l.length)) * 7.4) / 2) + 6;
    halo.setAttribute("x", cx - half);
    halo.setAttribute("y", icon.y1 - 6);
    halo.setAttribute("width", 2 * half);
    halo.setAttribute("height", bottom - icon.y1 + 10);
    wireNode(g, c, t);
    return icon;
  }

  // Scheduling arrows as the PROforma plan editor draws them: a straight line between
  // the two task icons (not their labels) with a filled triangle at the target.
  // does the segment p-q pass through box b (shrunk by m)?
  function segHitsBox(p, q, b, m) {
    let t0 = 0;
    let t1 = 1;
    const dx = q.x - p.x;
    const dy = q.y - p.y;
    const edges = [
      [-dx, p.x - (b.x1 + m)],
      [dx, b.x2 - m - p.x],
      [-dy, p.y - (b.y1 + m)],
      [dy, b.y2 - m - p.y],
    ];
    for (const [pp, qq] of edges) {
      if (pp === 0) {
        if (qq < 0) return false;
      } else {
        const r = qq / pp;
        if (pp < 0) t0 = Math.max(t0, r);
        else t1 = Math.min(t1, r);
        if (t0 > t1) return false;
      }
    }
    return true;
  }

  function drawEdges(layer, planTask, boxes) {
    planTask.components.forEach((c) => {
      const t = tasks.get(c.task);
      for (const a of c.after) {
        const pb = boxes.get(a);
        const qb = boxes.get(c.name);
        if (!pb || !qb) continue;
        // rectangular anchors as in the editor; when one box lies wholly below the other,
        // aim at the facing edge's midpoint so the line never cuts a neighbour in that row
        let s1;
        let s2;
        if (qb.y1 >= pb.y2) {
          s1 = { x: pb.cx, y: pb.y2 };
          s2 = { x: qb.cx, y: qb.y1 };
        } else if (qb.y2 <= pb.y1) {
          s1 = { x: pb.cx, y: pb.y1 };
          s2 = { x: qb.cx, y: qb.y2 };
        } else {
          s1 = clip(pb, pb.cx, pb.cy, qb.cx, qb.cy);
          s2 = clip(qb, qb.cx, qb.cy, pb.cx, pb.cy);
        }
        // a straight line that would pass through another task (e.g. one that skips a
        // level) detours around the blockers with straight segments
        let pts = [s1, s2];
        const blockers = [...boxes.entries()].filter(([n, b]) => n !== a && n !== c.name && segHitsBox(s1, s2, b, 1)).map(([, b]) => b);
        if (blockers.length) {
          const vertical = Math.abs(s2.y - s1.y) >= Math.abs(s2.x - s1.x);
          if (vertical) {
            const right = Math.max(...blockers.map((b) => b.x2)) + 28;
            const left = Math.min(...blockers.map((b) => b.x1)) - 28;
            const side = Math.abs(right - s1.x) <= Math.abs(s1.x - left) ? right : left;
            const dir = s2.y > s1.y ? 1 : -1;
            pts = [s1, { x: side, y: s1.y + dir * 18 }, { x: side, y: s2.y - dir * 18 }, s2];
          } else {
            const below = Math.max(...blockers.map((b) => b.y2)) + 28;
            const above = Math.min(...blockers.map((b) => b.y1)) - 28;
            const side = Math.abs(below - s1.y) <= Math.abs(s1.y - above) ? below : above;
            const dir = s2.x > s1.x ? 1 : -1;
            pts = [s1, { x: s1.x + dir * 18, y: side }, { x: s2.x - dir * 18, y: side }, s2];
          }
        }
        svgEl("path", {
          class: "edge",
          d: "M" + pts.map((pt) => pt.x + " " + pt.y).join(" L"),
          "marker-end": "url(#arrow)",
          "data-plan": planTask.name,
          "data-from": a,
          "data-to": c.name,
        }, layer);
        const anteComp = planTask.components.find((x) => x.name === a);
        const lbl = t ? branchLabel(t, tasks.get(anteComp ? anteComp.task : a)) : "";
        if (lbl) {
          // beside a mostly vertical edge, above a mostly horizontal one
          const m0 = pts[Math.floor((pts.length - 1) / 2)];
          const m1 = pts[Math.floor((pts.length - 1) / 2) + 1];
          const vertical = Math.abs(m1.x - m0.x) < Math.abs(m1.y - m0.y);
          const mx = (m0.x + m1.x) / 2;
          const my = (m0.y + m1.y) / 2;
          svgEl("text", vertical
            ? { class: "edge-label", x: mx + 8, y: my + 4, "text-anchor": "start" }
            : { class: "edge-label", x: mx, y: my - 6 }, layer).textContent = lbl;
        }
      }
    });
  }

  // ---- expanded view: every sub-plan drawn inline as a frame, recursively
  const LEAF_W = 150;
  const LEAF_H = 124;
  const PAD = 22;
  const HEAD = 34;
  const GAP_X = 36;
  const GAP_Y = 46;
  const PER_ROW = 5;

  function levelsOf(comps) {
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
    return level;
  }

  function layoutExpanded(planName, stack) {
    const p = tasks.get(planName);
    const items = p.components.map((c) => {
      const t = tasks.get(c.task) || { kind: "task", name: c.task, components: [] };
      const sub = t.kind === "plan" && t.components.length && !stack.includes(t.name) ? layoutExpanded(t.name, stack.concat(t.name)) : null;
      return { c, t, sub, w: sub ? sub.w + 2 * PAD : LEAF_W, h: sub ? sub.h + HEAD + PAD : LEAF_H };
    });
    const level = levelsOf(p.components);
    const byLevel = new Map();
    items.forEach((it, i) => {
      const l = level.get(it.c.name) || 0;
      if (!byLevel.has(l)) byLevel.set(l, []);
      byLevel.get(l).push(i);
    });
    const rows = [];
    // a layer whose tasks have antecedents stays on one row, so straight arrows from the
    // row above reach every target without crossing a sibling; tasks with no incoming
    // arrows (parallel work) wrap into a grid
    for (const l of [...byLevel.keys()].sort((a, b) => a - b)) {
      const idx = byLevel.get(l);
      const per = idx.some((i) => items[i].c.after.length) ? idx.length : PER_ROW;
      for (let s = 0; s < idx.length; s += per) rows.push(idx.slice(s, s + per));
    }
    const info = rows.map((r) => ({
      r,
      w: r.reduce((n, i) => n + items[i].w, 0) + GAP_X * (r.length - 1),
      h: Math.max(...r.map((i) => items[i].h)),
    }));
    const W = Math.max(LEAF_W, ...info.map((x) => x.w));
    let y = 0;
    for (const row of info) {
      let x = (W - row.w) / 2;
      for (const i of row.r) {
        items[i].x = x;
        items[i].y = y;
        x += items[i].w + GAP_X;
      }
      y += row.h + GAP_Y;
    }
    return { w: W, h: Math.max(LEAF_H, y - GAP_Y), items };
  }

  function drawExpanded(L, ox, oy, planTask, layers) {
    const boxes = new Map();
    for (const it of L.items) {
      const x = ox + it.x;
      const y = oy + it.y;
      if (it.sub) {
        const g = svgEl("g", { class: "node container" + (it.c.optional ? " optional" : ""), "data-task": it.c.task }, layers.frames);
        svgEl("rect", { class: "halo", x: x - 4, y: y - 4, width: it.w + 8, height: it.h + 8, rx: 17 }, g);
        svgEl("rect", { class: "shape frame", x, y, width: it.w, height: it.h, rx: 14, fill: "var(--plan-fill)", stroke: "var(--plan-stroke)" }, g);
        const title = labelOf(it.c, it.t);
        svgEl("text", { class: "frame-title", x: x + 14, y: y + 22 }, g).textContent = title.length > 60 ? title.slice(0, 58) + "…" : title;
        badges(g, it.c, it.t, x + it.w - 18, y + 17, x + it.w - 42, y + 17);
        wireNode(g, it.c, it.t);
        boxes.set(it.c.name, { x1: x, x2: x + it.w, y1: y, y2: y + it.h, cx: x + it.w / 2, cy: y + it.h / 2 });
        drawExpanded(it.sub, x + PAD, y + HEAD, it.t, layers);
      } else {
        boxes.set(it.c.name, drawNode(layers.nodes, it.c, it.t, x + it.w / 2, y + iconHeight(it.t.kind) / 2));
      }
    }
    drawEdges(layers.edges, planTask, boxes);
  }

  function render() {
    vp.innerHTML = "";
    const plan = tasks.get(state.plan);
    renderCrumbs();
    if (!plan) return;
    const defs = svgEl("defs", {}, vp);
    const mk = svgEl("marker", { id: "arrow", viewBox: "0 0 12 12", refX: 12, refY: 6, markerWidth: 12, markerHeight: 12, markerUnits: "userSpaceOnUse", orient: "auto" }, defs);
    svgEl("path", { d: "M0 1 L12 6 L0 11 z", fill: "var(--edge)" }, mk);
    const layers = { frames: svgEl("g", {}, vp), edges: svgEl("g", {}, vp), nodes: svgEl("g", {}, vp) };

    if (state.expanded) {
      drawExpanded(layoutExpanded(plan.name, [plan.name]), 0, 0, plan, layers);
    } else {
      const pos = planLayout(plan);
      const boxes = new Map();
      plan.components.forEach((c, i) => {
        const t = tasks.get(c.task) || { kind: "task", name: c.task };
        boxes.set(c.name, drawNode(layers.nodes, c, t, pos[i].x + NODE / 2, pos[i].y + NODE / 2));
      });
      drawEdges(layers.edges, plan, boxes);
    }
    applyView();
    const lb = $("#zlayout");
    lb.classList.toggle("on", !state.expanded && !!state.autoUsed);
    lb.disabled = !!state.expanded;
    $("#zexpand").classList.toggle("on", !!state.expanded);
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
  // Panning starts anywhere, including on tasks and plan frames: a press that
  // moves more than a few pixels pans (and its click is swallowed); a press that
  // does not move stays a click / double-click on whatever is under it.
  const PAN_SLOP = 4;
  let suppressClick = false;
  svg.addEventListener("pointerdown", (e) => {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 1) drag = { id: e.pointerId, x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false };
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
    if (!drag || drag.id !== e.pointerId) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (!drag.moved) {
      if (Math.abs(dx) + Math.abs(dy) <= PAN_SLOP) return;
      drag.moved = true;
      svg.classList.add("panning");
      // capture only once it is a pan, so plain clicks still reach the node
      try {
        svg.setPointerCapture(e.pointerId);
      } catch (err) {
        /* pointer already released */
      }
    }
    view.x = drag.vx + dx;
    view.y = drag.vy + dy;
    applyView();
  });
  const endDrag = (e) => {
    pointers.delete(e.pointerId);
    if (pointers.size === 0) {
      if (drag && drag.moved) {
        // the click that follows a pan must not select whatever is under the pointer
        suppressClick = true;
        setTimeout(() => (suppressClick = false), 0);
      }
      svg.classList.remove("panning");
      drag = null;
    }
  };
  svg.addEventListener(
    "click",
    (e) => {
      if (suppressClick) {
        e.stopPropagation();
        e.preventDefault();
      }
    },
    true
  );
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
  $("#zexpand").onclick = () => {
    state.expanded = !state.expanded;
    render();
    fit(false);
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
    else if (e.key === "e" || e.key === "E") $("#zexpand").click();
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

  // ---------------------------------------------------------------- resizable side panes
  function makeResizer(handleId, cssVar, key, def, widthFromPointer) {
    const ws = $("#workspace");
    const handle = $("#" + handleId);
    if (!ws || !handle) return;
    const clamp = (w) => Math.round(Math.max(160, Math.min(w, ws.clientWidth * 0.5)));
    const setW = (w) => ws.style.setProperty(cssVar, clamp(w) + "px");
    try {
      const saved = Number(localStorage.getItem(key));
      if (saved) setW(saved);
    } catch (e) {
      /* storage unavailable: keep the default */
    }
    handle.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      handle.setPointerCapture(e.pointerId);
      handle.classList.add("drag");
      document.body.classList.add("resizing");
    });
    handle.addEventListener("pointermove", (e) => {
      if (handle.classList.contains("drag")) setW(widthFromPointer(e, ws.getBoundingClientRect()));
    });
    const end = () => {
      if (!handle.classList.contains("drag")) return;
      handle.classList.remove("drag");
      document.body.classList.remove("resizing");
      try {
        localStorage.setItem(key, String(parseInt(getComputedStyle(ws).getPropertyValue(cssVar), 10) || def));
      } catch (e) {
        /* ignore */
      }
    };
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
    handle.addEventListener("dblclick", () => {
      setW(def);
      try {
        localStorage.removeItem(key);
      } catch (e) {
        /* ignore */
      }
    });
  }
  makeResizer("leftResizer", "--left-w", "pf-viewer-left-width", 250, (e, r) => e.clientX - r.left);
  makeResizer("rightResizer", "--right-w", "pf-viewer-right-width", 340, (e, r) => r.right - e.clientX);

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
