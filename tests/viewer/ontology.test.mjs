// Composer's ontology coding and the RxNorm coding (viewer-lib.js parseCoding / parseRxnorm /
// taskCodings / codingHtml). Run: node --test "tests/viewer/*.test.mjs"
import test from "node:test";
import assert from "node:assert/strict";
import { loadLib } from "./load-lib.mjs";

const L = loadLib();
const plain = (o) => JSON.parse(JSON.stringify(o)); // objects from the vm realm

test("Composer form: <prefix>:<code> [<caption>]", () => {
  assert.deepEqual(plain(L.parseCoding("SCT:271807003 [Eruption of skin (disorder)]")), {
    prefix: "SCT",
    scheme: "SCT",
    system: "SNOMED CT",
    code: "271807003",
    caption: "Eruption of skin (disorder)",
    url: "https://browser.ihtsdotools.org/?perspective=full&conceptId1=271807003",
  });
  // the caption is everything after the first " [", outer brackets stripped
  const loinc = L.parseCoding("LNC:718-7 [Hemoglobin [Mass/volume] in Blood]");
  assert.equal(loinc.caption, "Hemoglobin [Mass/volume] in Blood");
  assert.equal(loinc.system, "LOINC");
  assert.equal(loinc.url, "https://loinc.org/718-7/");
  assert.equal(L.parseCoding("LNC:777-3 [Platelets [#/volume] in Blood by Automated count]").caption, "Platelets [#/volume] in Blood by Automated count");
  const icd = L.parseCoding("IC0:K85.9 [Acute pancreatitis, unspecified]");
  assert.equal(icd.system, "ICD-10-CM");
  assert.equal(icd.url, null);
  assert.equal(L.parseCoding("IC9:577.0 [Acute pancreatitis]").system, "ICD-9-CM");
});

test("older form: SNOMED:<id> / LOINC:<code> with ontology.label", () => {
  const s = L.parseCoding("SNOMED:197456007", "Acute pancreatitis (disorder)");
  assert.equal(s.prefix, "SNOMED");
  assert.equal(s.scheme, "SCT");
  assert.equal(s.caption, "Acute pancreatitis (disorder)");
  assert.equal(s.url, "https://browser.ihtsdotools.org/?perspective=full&conceptId1=197456007");
  assert.equal(L.parseCoding("LOINC:2160-0").url, "https://loinc.org/2160-0/");
  // an unknown scheme is shown as written, without a link
  const x = L.parseCoding("MEDDRA:10033645");
  assert.equal(x.system, "MEDDRA");
  assert.equal(x.url, null);
  assert.equal(L.parseCoding(""), null);
  assert.equal(L.parseCoding(undefined), null);
});

test("RxNorm coding", () => {
  const r = L.parseRxnorm("RXNORM:1191", "aspirin");
  assert.equal(r.code, "1191");
  assert.equal(r.caption, "aspirin");
  assert.equal(r.url, "https://mor.nlm.nih.gov/RxNav/search?searchBy=RXCUI&searchTerm=1191");
  assert.equal(L.parseRxnorm(""), null);
});

test("hidden metaprop keys", () => {
  for (const k of ["ontology.coding", "ontology.label", "rxnorm.coding", "rxnorm.label", "ae.private.component.id"]) assert.ok(L.hiddenMeta(k), k);
  for (const k of ["goal.verb", "source.1", "quote.1", "order.code"]) assert.ok(!L.hiddenMeta(k), k);
});

test("a task's coding comes from its component in the parent plan, then the definition", () => {
  const model = {
    tasks: [
      {
        name: "root",
        kind: "plan",
        metaprops: {},
        components: [
          { name: "Monitor_Bleeding", task: "Monitor_Bleeding", metaprops: { "ontology.coding": "SCT:131148009 [Bleeding (finding)]" } },
          { name: "Prescribe_Aspirin", task: "Prescribe_Aspirin", metaprops: { "ontology.coding": "SCT:387458008 [Aspirin (substance)]" } },
          { name: "Legacy", task: "Legacy", metaprops: {} },
          { name: "Uncoded", task: "Uncoded", metaprops: {} },
        ],
      },
      { name: "Monitor_Bleeding", kind: "enquiry", metaprops: { "ontology.coding": "SNOMED:1 [stale definition coding]" }, components: [] },
      { name: "Prescribe_Aspirin", kind: "action", metaprops: { "rxnorm.coding": "RXNORM:1191", "rxnorm.label": "aspirin" }, components: [] },
      { name: "Legacy", kind: "action", metaprops: { "ontology.coding": "SNOMED:197456007", "ontology.label": "Acute pancreatitis (disorder)" }, components: [] },
      { name: "Uncoded", kind: "action", metaprops: {}, components: [] },
    ],
  };
  const b = L.taskCodings(model, "Monitor_Bleeding");
  assert.equal(b.ontology.code, "131148009");
  assert.equal(b.ontologyFrom, "component");
  assert.equal(b.rxnorm, null);
  const a = L.taskCodings(model, "Prescribe_Aspirin");
  assert.equal(a.ontology.caption, "Aspirin (substance)");
  assert.equal(a.rxnorm.code, "1191");
  const g = L.taskCodings(model, "Legacy");
  assert.equal(g.ontologyFrom, "definition");
  assert.equal(g.ontology.caption, "Acute pancreatitis (disorder)");
  const u = L.taskCodings(model, "Uncoded");
  assert.equal(u.ontology, null);
  assert.equal(L.codingsHtml(u.ontology, u.rxnorm), "");
});

test("coding HTML: prefix:code linked, caption escaped", () => {
  const h = L.codingsHtml(L.parseCoding("LNC:718-7 [Hemoglobin <x>]"), L.parseRxnorm("RXNORM:1191", "aspirin"));
  assert.match(h, /<a class="ext" href="https:\/\/loinc\.org\/718-7\/" target="_blank" rel="noopener"><code title="LOINC">LNC:718-7<\/code><\/a>/);
  assert.match(h, /<span class="coding-cap">Hemoglobin &lt;x&gt;<\/span>/);
  assert.match(h, /<br><span class="coding"><a class="ext" href="https:\/\/mor\.nlm\.nih\.gov\/RxNav\/search\?searchBy=RXCUI&amp;searchTerm=1191"/);
  assert.match(L.codingHtml(L.parseCoding("IC0:K85.9 [Acute pancreatitis]")), /^<span class="coding"><code title="ICD-10-CM">IC0:K85\.9<\/code> /);
});
