// Composer's Markdown subset in captions and descriptions (viewer-lib.js md / plain).
// Run: node --test tests/viewer
import test from "node:test";
import assert from "node:assert/strict";
import { loadLib } from "./load-lib.mjs";

const L = loadLib();

// the caption of Display_Potential_ADEs in cig/examples/ccd-antithrombotic-monitoring.pf
const ADE_CAPTION =
  "# *Monitoring for ADEs:*  \n1. Bleeding  \n2. Drug interactions  \n3. Spinal or epidural hematoma  \n" +
  "16. Cutaneous lupus erythematosus (PPI)  \n17. Severe cutaneous reactions (PPI)  ";

test("a one-line caption stays inline, escaped, with grade badges", () => {
  assert.equal(L.md("Plain caption"), "Plain caption");
  assert.equal(L.md("NSAIDs <b>first</b> & then"), "NSAIDs &lt;b&gt;first&lt;/b&gt; &amp; then");
  assert.equal(L.md("Refer early [II, B]"), 'Refer early <span class="grade gB">II, B</span>');
  assert.equal(L.md(""), "");
  assert.equal(L.md(null), "");
});

test("multi-line header and numbered list (the ADE display caption)", () => {
  const h = L.md(ADE_CAPTION);
  assert.ok(h.startsWith('<div class="md"><div class="mdh mdh1" role="heading" aria-level="1"><em>Monitoring for ADEs:</em></div><ol>'), h);
  assert.match(h, /<ol><li>Bleeding<\/li><li>Drug interactions<\/li><li>Spinal or epidural hematoma<\/li>/);
  assert.match(h, /<li>Severe cutaneous reactions \(PPI\)<\/li><\/ol><\/div>$/);
  assert.equal((h.match(/<li>/g) || []).length, 5);
  assert.ok(!h.includes("<br>"), "two trailing spaces on a list item add no break");
});

test("headers # to ####, and ##### is text", () => {
  for (let n = 1; n <= 4; n++) assert.match(L.md("#".repeat(n) + " Title\ntext"), new RegExp('<div class="mdh mdh' + n + '"[^>]*>Title</div><p>text</p>'));
  assert.equal(L.md("##### five"), "##### five");
  assert.equal(L.md("#hashtag"), "#hashtag");
});

test("emphasis and strong with * and _, never inside identifiers", () => {
  assert.equal(L.md("*em* _em_ **strong** __strong__"), "<em>em</em> <em>em</em> <strong>strong</strong> <strong>strong</strong>");
  assert.equal(L.md("set on_insulin_or_secretagogue = true"), "set on_insulin_or_secretagogue = true");
  assert.equal(L.md("5 * 3 * 2"), "5 * 3 * 2");
  assert.equal(L.md("a \\*literal\\* star"), "a &#42;literal&#42; star");
  assert.equal(L.md("**bold *and em***"), "<strong>bold <em>and em</em></strong>");
});

test("bulleted lists nested with tabs, and numbered lists keep their start", () => {
  assert.equal(
    L.md("* a\n\t* a.1\n\t\t1. a.1.i\n* b"),
    '<div class="md"><ul><li>a<ul><li>a.1<ol><li>a.1.i</li></ol></li></ul></li><li>b</li></ul></div>'
  );
  assert.equal(L.md("3. three\n4. four"), '<div class="md"><ol start="3"><li>three</li><li>four</li></ol></div>');
  // a bullet list followed by a numbered one at the same level: two lists
  assert.equal(L.md("* a\n1. one"), '<div class="md"><ul><li>a</li></ul><ol><li>one</li></ol></div>');
  // a blank line between items keeps one list; a lazy line continues the item
  assert.equal(L.md("* a\n\n* b\ncontinued"), '<div class="md"><ul><li>a</li><li>b continued</li></ul></div>');
});

test("paragraphs, line breaks, horizontal rule and preformatted text", () => {
  assert.equal(L.md("one\ntwo"), "one two");
  assert.equal(L.md("one  \ntwo"), "one<br>two");
  assert.equal(L.md("first\n\nsecond"), '<div class="md"><p>first</p><p>second</p></div>');
  assert.equal(L.md("above\n\n___\n\nbelow"), '<div class="md"><p>above</p><hr><p>below</p></div>');
  assert.equal(L.md("intro\n\n    x <= 5\n    *y*\n\nafter"), '<div class="md"><p>intro</p><pre>x &lt;= 5\n*y*</pre><p>after</p></div>');
  // a year at the start of a continuation line is not a list
  assert.equal(L.md("Published in\n2023. Updated"), "Published in 2023. Updated");
});

test("Markdown cannot inject HTML", () => {
  const h = L.md('# <img src=x onerror=alert(1)>\n* <script>x</script>\n\n    <a href="javascript:1">');
  assert.ok(!/<(img|script|a)\b/i.test(h), h);
});

test("plain: first line without markers, for canvas labels and tree rows", () => {
  assert.equal(L.plain(ADE_CAPTION), "Monitoring for ADEs:");
  assert.equal(L.plain("Assess **patient**"), "Assess patient");
  assert.equal(L.plain("___\n* _first_ item"), "first item");
  assert.equal(L.plain(""), "");
  assert.deepEqual([...L.plainLines("# Head\n1. one\n\t* sub")], ["Head", "1. one", "sub"]);
});
