// prerender.mjs: run the viewer once at build time, against a minimal fake DOM, and return
// the workspace it draws (task tree, data list, breadcrumb, properties, root plan diagram)
// as static markup. build_report writes that markup into the page, so the first screen is
// meaningful without JavaScript (link previews, thumbnails, script-less renderers); the
// viewer replaces it when it runs.
import vm from "node:vm";

const VOID = new Set(["br", "img", "input", "meta", "link", "hr", "wbr"]);
const escText = (s) => String(s).replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
const escAttr = (s) => String(s).replace(/[&<"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", '"': "&quot;" })[c]);

class El {
  constructor(tag) {
    this.tagName = tag;
    this.attrs = new Map();
    this.children = [];
    this.raw = ""; // markup assigned through innerHTML / textContent
    this.style = { setProperty() {}, removeProperty() {} };
    const el = this;
    this.classList = {
      list: () => (el.attrs.get("class") || "").split(/\s+/).filter(Boolean),
      add: (...c) => el.setAttribute("class", [...new Set([...el.classList.list(), ...c])].join(" ")),
      remove: (...c) => el.setAttribute("class", el.classList.list().filter((x) => !c.includes(x)).join(" ")),
      contains: (c) => el.classList.list().includes(c),
      toggle: (c, on) => {
        const want = on === undefined ? !el.classList.contains(c) : !!on;
        want ? el.classList.add(c) : el.classList.remove(c);
        return want;
      },
    };
    this.dataset = new Proxy({}, {
      get: (_, k) => el.attrs.get("data-" + String(k).replace(/[A-Z]/g, (m) => "-" + m.toLowerCase())),
      set: (_, k, v) => (el.setAttribute("data-" + String(k).replace(/[A-Z]/g, (m) => "-" + m.toLowerCase()), v), true),
    });
  }
  setAttribute(k, v) { this.attrs.set(k, String(v)); }
  getAttribute(k) { return this.attrs.has(k) ? this.attrs.get(k) : null; }
  removeAttribute(k) { this.attrs.delete(k); }
  appendChild(c) { this.children.push(c); c.parentNode = this; return c; }
  addEventListener() {}
  removeEventListener() {}
  setPointerCapture() {}
  click() {}
  closest() { return null; }
  set className(v) { this.setAttribute("class", v); }
  get className() { return this.attrs.get("class") || ""; }
  set title(v) { this.setAttribute("title", v); }
  set hidden(v) { v ? this.setAttribute("hidden", "") : this.removeAttribute("hidden"); }
  set disabled(v) { v ? this.setAttribute("disabled", "") : this.removeAttribute("disabled"); }
  set innerHTML(v) { this.children = []; this.raw = String(v); }
  get innerHTML() { return this.raw + this.children.map(String).join(""); }
  set textContent(v) { this.children = []; this.raw = escText(v); }
  get textContent() { return this.text != null ? this.text : this.raw; }
  get firstChild() { return this.children[0] || new El("div"); }
  get clientWidth() { return 1000; }
  get clientHeight() { return 640; }
  getBoundingClientRect() { return { left: 0, top: 0, right: 1000, bottom: 640, width: 1000, height: 640 }; }
  getBBox() { return bbox(this); }
  find(sel) {
    for (const c of this.children) {
      if (matches(c, sel)) return c;
      const d = c.find(sel);
      if (d) return d;
    }
    return null;
  }
  hasRaw() { return !!this.raw || this.children.some((c) => c.hasRaw()); }
  // markup assigned as a string cannot be searched: for a class or tag selector hand back a
  // detached element so wiring code (onclick = ...) has something to write to; an id lookup
  // stays null, since the viewer uses it to test whether something exists
  querySelector(sel) { return this.find(sel) || (!sel.startsWith("#") && this.hasRaw() ? new El("div") : null); }
  querySelectorAll() { return []; }
  toString() {
    const a = [...this.attrs].map(([k, v]) => (v === "" ? " " + k : " " + k + '="' + escAttr(v) + '"')).join("");
    if (VOID.has(this.tagName)) return "<" + this.tagName + a + ">";
    return "<" + this.tagName + a + ">" + this.innerHTML + "</" + this.tagName + ">";
  }
}

// "#id", "tag", "tag.class", ".class"
function matches(el, sel) {
  const m = /^([a-zA-Z]*)(?:#([\w-]+))?(?:\.([\w-]+))?$/.exec(sel);
  if (!m) return false;
  const [, tag, id, cls] = m;
  return (!tag || el.tagName === tag) && (!id || el.getAttribute("id") === id) && (!cls || el.classList.contains(cls));
}

// bounding box of drawn SVG content (shapes, paths, labels), skipping defs
function bbox(root) {
  let x1 = Infinity, y1 = Infinity, x2 = -Infinity, y2 = -Infinity;
  const add = (x, y) => {
    if (!isFinite(x) || !isFinite(y)) return;
    x1 = Math.min(x1, x); y1 = Math.min(y1, y); x2 = Math.max(x2, x); y2 = Math.max(y2, y);
  };
  const n = (el, k) => parseFloat(el.getAttribute(k));
  const walk = (el) => {
    const t = el.tagName;
    if (t === "defs" || t === "title") return;
    if (t === "rect" && el.getAttribute("x") != null) {
      add(n(el, "x"), n(el, "y"));
      add(n(el, "x") + n(el, "width"), n(el, "y") + n(el, "height"));
    } else if (t === "circle") {
      add(n(el, "cx") - n(el, "r"), n(el, "cy") - n(el, "r"));
      add(n(el, "cx") + n(el, "r"), n(el, "cy") + n(el, "r"));
    } else if (t === "path") {
      const tok = (el.getAttribute("d") || "").match(/[A-Za-z]|-?\d*\.?\d+(?:e-?\d+)?/g) || [];
      let cmd = "M", cx = 0, cy = 0;
      for (let i = 0; i < tok.length; ) {
        if (/[A-Za-z]/.test(tok[i])) { cmd = tok[i++].toUpperCase(); continue; }
        if (cmd === "H") cx = +tok[i++];
        else if (cmd === "V") cy = +tok[i++];
        else { cx = +tok[i++]; cy = +tok[i++]; }
        add(cx, cy);
      }
    } else if (t === "text") {
      const half = (el.textContent.length * 7.4) / 2;
      const x = n(el, "x"), y = n(el, "y");
      const start = el.getAttribute("text-anchor") === "start";
      add(start ? x : x - half, y - 14);
      add(start ? x + 2 * half : x + half, y + 4);
    }
    el.children.forEach(walk);
  };
  walk(root);
  return x1 === Infinity ? { x: 0, y: 0, width: 0, height: 0 } : { x: x1, y: y1, width: x2 - x1, height: y2 - y1 };
}

/** Run the viewer on the page's data; returns { ids: {id: innerHTML}, canvasAttrs }. */
export function prerender(viewerJs, payloadJson) {
  const byId = new Map();
  const get = (id) => {
    if (!byId.has(id)) {
      const el = new El(id === "canvas" ? "svg" : "div");
      el.setAttribute("id", id);
      if (id === "pf-data") el.text = payloadJson;
      byId.set(id, el);
    }
    return byId.get(id);
  };
  const document = {
    getElementById: get,
    querySelector: (sel) => (sel.startsWith("#") ? get(sel.slice(1)) : new El("div")),
    querySelectorAll: () => [],
    createElement: (tag) => new El(tag),
    createElementNS: (_, tag) => new El(tag),
    addEventListener() {},
    documentElement: new El("html"),
    body: new El("body"),
  };
  const ctx = {
    document,
    console,
    requestAnimationFrame() {},
    setTimeout() {},
    getComputedStyle: () => ({ getPropertyValue: () => "" }),
    matchMedia: () => ({ matches: false }),
  };
  ctx.window = ctx;
  ctx.addEventListener = () => {};
  vm.runInNewContext(viewerJs, ctx, { filename: "viewer.js", timeout: 10000 });

  const ids = {};
  for (const id of ["tree", "datatree", "crumbs", "props", "zoomlabel"]) if (byId.has(id)) ids[id] = byId.get(id).innerHTML;

  // the diagram: drop the pan/zoom transform and give the SVG a viewBox that fits it
  const svg = byId.get("canvas");
  const vp = svg.children.find((c) => c.tagName === "g");
  let canvasAttrs = "";
  if (vp) {
    vp.removeAttribute("transform");
    const b = bbox(vp);
    if (b.width && b.height) {
      const pad = 30;
      canvasAttrs = ` viewBox="${(b.x - pad).toFixed(1)} ${(b.y - pad).toFixed(1)} ${(b.width + 2 * pad).toFixed(1)} ${(b.height + 2 * pad).toFixed(1)}" preserveAspectRatio="xMidYMid meet"`;
    }
  }
  ids.canvas = svg.innerHTML;
  return { ids, canvasAttrs };
}
