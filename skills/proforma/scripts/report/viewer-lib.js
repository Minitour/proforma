/* Pure helpers for the PROforma guideline viewer, shared by the page (inlined before
   viewer.js), build_report.mjs and the tests (loaded with node:vm). No DOM access, no
   dependencies: Composer's Markdown subset in captions and descriptions, and ontology codings. */
var PFLib = (function () {
  "use strict";

  const esc = (s) =>
    String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  // evidence grades such as [I, A] or [IV-V, B]
  const LEVEL = "(?:I{1,3}|IV|V)";
  const GRADE_RE = new RegExp("\\[(" + LEVEL + "(?:\\s*[-–]\\s*" + LEVEL + ")?),\\s*([A-E])\\]", "g");
  const grades = (html) => html.replace(GRADE_RE, (m, l, g) => '<span class="grade g' + g + '">' + l + ", " + g + "</span>");

  // ---------------------------------------------------------------- Composer Markdown
  // Composer renders a Markdown subset in captions and descriptions: headers (# to ####),
  // *emphasis* / _emphasis_, **strong** / __strong__, bulleted (* item) and numbered (1. item)
  // lists nested with tabs, paragraphs (blank line), line breaks (two trailing spaces), a
  // horizontal rule (three underscores) and preformatted text (four leading spaces).
  // Escape first, then add the markup, so nothing in the text can inject HTML.

  // emphasis delimiters: never inside a word for "_" (identifiers such as on_insulin_x), and
  // never next to a space on the inside for either (so "5 * 3 * 2" stays as it is)
  const STRONG_STAR_RE = /\*\*(?=\S)([\s\S]*?\S)\*\*(?!\*)/g;
  const STRONG_UND_RE = /(^|[^A-Za-z0-9_])__(?=\S)([\s\S]*?\S)__(?![A-Za-z0-9_])/g;
  const EM_STAR_RE = /\*(?=[^\s*])([^*]*?[^\s*])\*/g;
  const EM_UND_RE = /(^|[^A-Za-z0-9_])_(?=[^\s_])([^_]*?[^\s_])_(?![A-Za-z0-9_])/g;
  function inline(s) {
    let h = esc(s).replace(/\\([*_#\\])/g, (m, c) => "&#" + c.charCodeAt(0) + ";");
    h = h.replace(STRONG_STAR_RE, "<strong>$1</strong>").replace(STRONG_UND_RE, "$1<strong>$2</strong>");
    h = h.replace(EM_STAR_RE, "<em>$1</em>");
    h = h.replace(EM_UND_RE, "$1<em>$2</em>");
    return grades(h);
  }

  const HEADER_RE = /^ {0,3}(#{1,4})[ \t]+(.*?)[ \t]*#*[ \t]*$/;
  const HR_RE = /^ {0,3}(?:_[ \t]*){3,}$|^ {0,3}(?:-[ \t]*){3,}$|^ {0,3}(?:\*[ \t]*){3,}$/;
  const ITEM_RE = /^(\t*) {0,3}(?:([*+-])|(\d{1,9})[.)])[ \t]+(.*)$/;
  const PRE_RE = /^ {4}(.*)$/;
  const hardBreak = (line) => / {2,}$/.test(line);

  /** Composer Markdown to HTML. One plain paragraph comes back as inline HTML (no <p>), so a
   *  one-line caption fits where it always did; anything else is a block in <div class="md">. */
  function md(src) {
    const text = String(src == null ? "" : src).replace(/\r\n?/g, "\n");
    if (!text.trim()) return "";
    const lines = text.split("\n");
    const out = [];
    let para = null; // lines of the open paragraph
    const lists = []; // open lists: {tag}
    let pre = null;
    let blank = false;
    let blocks = 0;
    const lineHtml = (ls) => ls.map((l, i) => inline(l.trim()) + (i < ls.length - 1 ? (hardBreak(l) ? "<br>" : " ") : "")).join("");
    const closePara = () => {
      if (para) out.push({ p: lineHtml(para) });
      para = null;
    };
    const closePre = () => {
      if (pre) {
        while (pre.length && !pre[pre.length - 1].trim()) pre.pop();
        out.push("<pre>" + esc(pre.join("\n")) + "</pre>");
        blocks++;
      }
      pre = null;
    };
    const closeLists = (depth) => {
      while (lists.length > depth) out.push("</li></" + lists.pop().tag + ">");
    };
    let item = null; // lines of the open list item
    const flushItem = () => {
      if (item) out.push(lineHtml(item));
      item = null;
    };
    for (const raw of lines) {
      const line = raw.replace(/\s+$/, (m) => (/ {2,}$/.test(m) ? "  " : ""));
      if (!line.trim()) {
        closePara();
        if (pre) pre.push("");
        blank = true;
        continue;
      }
      let li = ITEM_RE.exec(line);
      // as in CommonMark, only a list starting at 1 may interrupt a paragraph ("2023. Next")
      if (li && para && li[3] && Number(li[3]) !== 1) li = null;
      if (pre && PRE_RE.test(line) && !li) {
        pre.push(PRE_RE.exec(line)[1]);
        continue;
      }
      closePre();
      if (li) {
        closePara();
        flushItem();
        const tag = li[2] ? "ul" : "ol";
        const depth = Math.min(li[1].length, lists.length);
        closeLists(depth + 1);
        if (lists.length === depth + 1 && lists[depth].tag !== tag) closeLists(depth);
        if (lists.length === depth + 1) out.push("</li><li>");
        else {
          if (!lists.length) blocks++;
          const start = li[3] && Number(li[3]) !== 1 ? ' start="' + Number(li[3]) + '"' : "";
          out.push("<" + tag + start + "><li>");
          lists.push({ tag });
        }
        item = [li[4]];
        blank = false;
        continue;
      }
      // a paragraph line after a list item continues the item unless a blank line came first
      if (lists.length && !blank && item) {
        item.push(line.replace(/^[\t ]+/, ""));
        continue;
      }
      flushItem();
      closeLists(0);
      const h = HEADER_RE.exec(line);
      if (h) {
        closePara();
        out.push('<div class="mdh mdh' + h[1].length + '" role="heading" aria-level="' + h[1].length + '">' + inline(h[2]) + "</div>");
        blocks++;
      } else if (HR_RE.test(line) && !(para && /^ {0,3}-/.test(line))) {
        closePara();
        out.push("<hr>");
        blocks++;
      } else if (!para && PRE_RE.test(line)) {
        pre = [PRE_RE.exec(line)[1]];
      } else {
        if (!para) para = [];
        para.push(line);
      }
      blank = false;
    }
    closePara();
    closePre();
    flushItem();
    closeLists(0);
    const paras = out.filter((x) => typeof x === "object");
    if (paras.length === 1 && blocks === 0) return paras[0].p;
    return '<div class="md">' + out.map((x) => (typeof x === "object" ? "<p>" + x.p + "</p>" : x)).join("") + "</div>";
  }

  /** A caption or description as plain text: Markdown markers stripped, one line per line. */
  function plainLines(src) {
    return String(src == null ? "" : src)
      .replace(/\r\n?/g, "\n")
      .split("\n")
      .filter((l) => l.trim() && !HR_RE.test(l))
      .map((l) =>
        l
          .replace(HEADER_RE, "$2")
          .replace(ITEM_RE, (m, tabs, b, n, rest) => (n ? n + ". " : "") + rest)
          .replace(/^ {4}/, "")
          .replace(STRONG_STAR_RE, "$1")
          .replace(STRONG_UND_RE, "$1$2")
          .replace(EM_STAR_RE, "$1")
          .replace(EM_UND_RE, "$1$2")
          .replace(/\\([*_#\\])/g, "$1")
          .trim()
      );
  }
  /** The first line of a caption, plain (canvas labels, tree rows, breadcrumbs). */
  const plain = (src) => plainLines(src)[0] || "";

  // ---------------------------------------------------------------- ontology coding
  // Composer writes ontology.coding as "<prefix>:<code> [<caption>]" with a 3-letter
  // prefix; the caption is everything after the first " [" with the outer brackets
  // stripped (it may hold brackets of its own, as LOINC names do). Older files use a
  // scheme name (SNOMED:<id>, LOINC:<code>) and put the term in ontology.label.
  const SCHEMES = {
    SCT: { name: "SNOMED CT", url: (c) => "https://browser.ihtsdotools.org/?perspective=full&conceptId1=" + encodeURIComponent(c) },
    LNC: { name: "LOINC", url: (c) => "https://loinc.org/" + encodeURIComponent(c) + "/" },
    IC0: { name: "ICD-10-CM" },
    IC9: { name: "ICD-9-CM" },
  };
  const ALIASES = {
    SNOMED: "SCT", SNOMEDCT: "SCT", "SNOMED-CT": "SCT", "SNOMED CT": "SCT",
    LOINC: "LNC",
    ICD10: "IC0", ICD10CM: "IC0", "ICD-10": "IC0", "ICD-10-CM": "IC0",
    ICD9: "IC9", ICD9CM: "IC9", "ICD-9": "IC9", "ICD-9-CM": "IC9",
  };

  /** Parse an ontology.coding value (plus the older ontology.label) into
   *  {prefix, scheme, system, code, caption, url}; null when there is no coding. */
  function parseCoding(value, label) {
    const v = String(value == null ? "" : value).trim();
    if (!v) return null;
    const cut = v.indexOf(" [");
    const head = (cut < 0 ? v : v.slice(0, cut)).trim();
    let caption = cut < 0 ? "" : v.slice(cut + 2).trim();
    if (caption.endsWith("]")) caption = caption.slice(0, -1).trim();
    const colon = head.indexOf(":");
    const prefix = colon < 0 ? "" : head.slice(0, colon).trim();
    const code = colon < 0 ? head : head.slice(colon + 1).trim();
    const up = prefix.toUpperCase();
    const scheme = SCHEMES[up] ? up : ALIASES[up] || null;
    const s = scheme && SCHEMES[scheme];
    return {
      prefix,
      scheme,
      system: s ? s.name : prefix,
      code,
      caption: caption || String(label == null ? "" : label).trim(),
      url: s && s.url && code ? s.url(code) : null,
    };
  }

  /** Parse rxnorm.coding (RXNORM:<rxcui>) plus rxnorm.label; null when absent. */
  function parseRxnorm(value, label) {
    const v = String(value == null ? "" : value).trim();
    if (!v) return null;
    const m = /^(?:RXNORM|RXCUI)\s*:\s*(\S+)(?:\s+\[(.*)\])?$/i.exec(v);
    const code = m ? m[1] : v;
    return {
      prefix: "RXNORM",
      scheme: "RXNORM",
      system: "RxNorm",
      code,
      caption: ((m && m[2]) || String(label == null ? "" : label)).trim(),
      url: /^\d+$/.test(code) ? "https://mor.nlm.nih.gov/RxNav/search?searchBy=RXCUI&searchTerm=" + encodeURIComponent(code) : null,
    };
  }

  /** Metaprop keys the properties panel shows elsewhere or not at all. */
  const hiddenMeta = (k) => k.startsWith("ontology.") || k.startsWith("rxnorm.") || k.startsWith("ae.private.");

  /** A task's codings as Composer reads them: ontology.coding from the component that runs the
   *  task in its parent plan (the first coded one), falling back to the task definition;
   *  rxnorm.coding from the task definition, falling back to the component. */
  function taskCodings(model, taskName) {
    const comps = [];
    for (const p of model.tasks || []) for (const c of p.components || []) if (c.task === taskName) comps.push(c);
    const t = (model.tasks || []).find((x) => x.name === taskName) || {};
    const tm = t.metaprops || {};
    const coded = comps.find((c) => c.metaprops && String(c.metaprops["ontology.coding"] || "").trim());
    const om = coded ? coded.metaprops : tm;
    const rxComp = comps.find((c) => c.metaprops && String(c.metaprops["rxnorm.coding"] || "").trim());
    const rm = String(tm["rxnorm.coding"] || "").trim() ? tm : rxComp ? rxComp.metaprops : {};
    return {
      ontology: parseCoding(om["ontology.coding"], om["ontology.label"]),
      ontologyFrom: coded ? "component" : om["ontology.coding"] ? "definition" : null,
      rxnorm: parseRxnorm(rm["rxnorm.coding"], rm["rxnorm.label"]),
    };
  }

  /** One coding as HTML: the prefix:code chip (linked when the scheme has a browser) and its caption. */
  function codingHtml(c) {
    if (!c) return "";
    const chip = '<code title="' + esc(c.system) + '">' + esc(c.prefix ? c.prefix + ":" + c.code : c.code) + "</code>";
    return '<span class="coding">' + (c.url ? '<a class="ext" href="' + esc(c.url) + '" target="_blank" rel="noopener">' + chip + "</a>" : chip) +
      (c.caption ? ' <span class="coding-cap">' + esc(c.caption) + "</span>" : "") + "</span>";
  }

  /** The ontology row: the ontology coding and the RxNorm coding, one per line. */
  function codingsHtml(ontology, rxnorm) {
    return [ontology, rxnorm].filter(Boolean).map(codingHtml).join("<br>");
  }

  return { esc, grades, inline, md, plain, plainLines, parseCoding, parseRxnorm, hiddenMeta, taskCodings, codingHtml, codingsHtml };
})();
