// The viewer's ZIP writer (viewer-lib.js zip / attachmentsZip) and build_report --attach-dir.
// Each archive is read back with an independent reader (below, CRC-32 from node:zlib) and,
// when the tool is on PATH, checked with `unzip -t`. Run: node --test "tests/viewer/*.test.mjs"
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { spawnSync } from "node:child_process";
import { loadLib, ROOT, SCRIPTS } from "./load-lib.mjs";

const L = loadLib();

// ---------------------------------------------------------------- independent reader
// Reads the end record, then every central directory entry, then the local header it points
// to, and checks that the two headers agree and that the CRC-32 matches the stored data.
function readZip(buf) {
  buf = Buffer.from(buf.buffer, buf.byteOffset, buf.byteLength);
  const eocd = buf.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  assert.ok(eocd >= 0 && eocd === buf.length - 22, "end of central directory record at the end, no comment");
  const count = buf.readUInt16LE(eocd + 10);
  assert.equal(buf.readUInt16LE(eocd + 8), count, "entries on this disk = total entries");
  const cdSize = buf.readUInt32LE(eocd + 12);
  const cdOff = buf.readUInt32LE(eocd + 16);
  assert.equal(cdOff + cdSize, eocd, "central directory ends where the end record starts");
  const entries = [];
  let p = cdOff;
  for (let i = 0; i < count; i++) {
    assert.equal(buf.readUInt32LE(p), 0x02014b50, "central directory signature");
    const flags = buf.readUInt16LE(p + 8);
    const method = buf.readUInt16LE(p + 10);
    const time = buf.readUInt16LE(p + 12);
    const date = buf.readUInt16LE(p + 14);
    const crc = buf.readUInt32LE(p + 16);
    const csize = buf.readUInt32LE(p + 20);
    const usize = buf.readUInt32LE(p + 24);
    const nlen = buf.readUInt16LE(p + 28);
    const xlen = buf.readUInt16LE(p + 30);
    const clen = buf.readUInt16LE(p + 32);
    const off = buf.readUInt32LE(p + 42);
    const name = buf.subarray(p + 46, p + 46 + nlen).toString("utf8");
    p += 46 + nlen + xlen + clen;
    assert.equal(flags & 0x0800, 0x0800, "UTF-8 name flag");
    assert.equal(method, 0, "stored");
    assert.equal(csize, usize, "stored: sizes agree");
    // the local header repeats the same values
    assert.equal(buf.readUInt32LE(off), 0x04034b50, "local header signature");
    assert.equal(buf.readUInt16LE(off + 6), flags);
    assert.equal(buf.readUInt16LE(off + 8), method);
    assert.equal(buf.readUInt16LE(off + 10), time);
    assert.equal(buf.readUInt16LE(off + 12), date);
    assert.equal(buf.readUInt32LE(off + 14), crc);
    assert.equal(buf.readUInt32LE(off + 18), csize);
    assert.equal(buf.readUInt32LE(off + 22), usize);
    const lnlen = buf.readUInt16LE(off + 26);
    const lxlen = buf.readUInt16LE(off + 28);
    assert.equal(buf.subarray(off + 30, off + 30 + lnlen).toString("utf8"), name);
    const data = buf.subarray(off + 30 + lnlen + lxlen, off + 30 + lnlen + lxlen + csize);
    assert.equal(zlib.crc32(data), crc, `CRC-32 of ${name}`);
    entries.push({ name, data: Buffer.from(data), time, date });
  }
  assert.equal(p, cdOff + cdSize);
  return entries;
}

function unzipTest(bytes) {
  const probe = spawnSync("unzip", ["-v"], { encoding: "utf8" });
  if (probe.error || probe.status !== 0) return null;
  const f = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "pfzip-")), "t.zip");
  fs.writeFileSync(f, bytes);
  const r = spawnSync("unzip", ["-t", f], { encoding: "utf8" });
  return r;
}

const crc = (s) => L.crc32(new TextEncoder().encode(s));

test("CRC-32 matches the standard check value and node:zlib", () => {
  assert.equal(crc("123456789"), 0xcbf43926);
  assert.equal(crc(""), 0);
  const bytes = new Uint8Array(70000).map((_, i) => (i * 131 + 7) & 0xff);
  assert.equal(L.crc32(bytes), zlib.crc32(bytes));
});

test("zip round-trip: text, binary, empty, nested and UTF-8 names", () => {
  const bin = new Uint8Array(4096).map((_, i) => (i * 37) & 0xff);
  const files = [
    { path: "project.metadata", data: new TextEncoder().encode('{"displayName":"X","mpwProject":false}'), mtime: Date.UTC(2026, 10, 3, 12, 30, 44) },
    { path: "proforma/X.pf", data: new TextEncoder().encode("metadata ::\r\n     title :: 'X' ;\r\n") },
    { path: "proforma/X.deo", data: bin },
    { path: "proforma/empty.txt", data: new Uint8Array(0) },
    { path: "notes/résumé ü 日本.md", data: new TextEncoder().encode("ü") },
  ];
  const out = readZip(L.zip(files));
  assert.deepEqual(out.map((e) => e.name), files.map((f) => f.path));
  out.forEach((e, i) => assert.ok(Buffer.from(files[i].data).equals(e.data), e.name));
  // MS-DOS time and date of the first entry (local time, 2-second resolution)
  const d = new Date(Date.UTC(2026, 10, 3, 12, 30, 44));
  assert.equal(out[0].date, ((d.getFullYear() - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate());
  assert.equal(out[0].time, (d.getHours() << 11) | (d.getMinutes() << 5) | (d.getSeconds() >> 1));
  const r = unzipTest(L.zip(files));
  if (r) assert.equal(r.status, 0, r.stdout + r.stderr);
});

test("an empty archive is just the end record", () => {
  const z = L.zip([]);
  assert.equal(z.length, 22);
  assert.deepEqual(readZip(z), []);
});

test("attachmentsZip puts the files under the root folder, or at the zip root", () => {
  const att = {
    name: "p.zip",
    root: "My_Project",
    files: [
      { path: "project.metadata", mtime: 0, base64: Buffer.from("{}").toString("base64") },
      { path: "proforma/x.pf", mtime: 0, base64: Buffer.from([0, 1, 2, 255]).toString("base64") },
    ],
  };
  const a = readZip(L.attachmentsZip(att));
  assert.deepEqual(a.map((e) => e.name), ["My_Project/project.metadata", "My_Project/proforma/x.pf"]);
  assert.ok(a[1].data.equals(Buffer.from([0, 1, 2, 255])));
  const b = readZip(L.attachmentsZip({ ...att, root: "" }));
  assert.deepEqual(b.map((e) => e.name), ["project.metadata", "proforma/x.pf"]);
});

// ---------------------------------------------------------------- build_report --attach-dir
function proformaBin() {
  if (process.env.PROFORMA_BIN && fs.existsSync(process.env.PROFORMA_BIN)) return process.env.PROFORMA_BIN;
  const exe = process.platform === "win32" ? "proforma.exe" : "proforma";
  for (const dir of ["release", "debug"]) {
    const f = path.join(ROOT, "target", dir, exe);
    if (fs.existsSync(f)) return f;
  }
  return null;
}

test("build_report --attach-dir embeds a Composer project that unzips to the same files", (t) => {
  const bin = proformaBin();
  if (!bin) return t.skip("no proforma binary (cargo build, or set PROFORMA_BIN)");
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "pfattach-"));
  const proj = path.join(tmp, "Sore_Throat_Project");
  fs.mkdirSync(path.join(proj, "proforma"), { recursive: true });
  const guideline = path.join(ROOT, "examples", "sore-throat", "sore_throat.pf");
  const want = {
    "project.metadata": Buffer.from('{"displayName":"Sore throat","description":"test","mpwProject":false}'),
    "proforma/Sore_Throat.pf": fs.readFileSync(guideline),
    "proforma/Sore_Throat.deo": Buffer.from(JSON.stringify({ file_format_version: "1.1", claims: [] }, null, 4)),
    "proforma/blob.bin": Buffer.from(Array.from({ length: 1000 }, (_, i) => (i * 97) & 0xff)),
  };
  for (const [rel, data] of Object.entries(want)) fs.writeFileSync(path.join(proj, rel), data);
  const out = path.join(tmp, "report.html");
  const run = (...extra) =>
    spawnSync(process.execPath, [path.join(SCRIPTS, "build_report.mjs"), guideline, "--out", out, "--bin", bin, "--attach-dir", proj, ...extra], { encoding: "utf8" });

  let r = run("--attach-name", "Sore_Throat_Project.zip");
  assert.equal(r.status, 0, r.stderr);
  let html = fs.readFileSync(out, "utf8");
  assert.match(html, /<button class="icon dl" id="downloadBtn" type="button" hidden title="Download Composer project: Sore_Throat_Project\.zip \(4 files, /);
  let att = JSON.parse(/<script type="application\/json" id="pf-attachments">([\s\S]*?)<\/script>/.exec(html)[1]);
  assert.equal(att.name, "Sore_Throat_Project.zip");
  assert.equal(att.root, "Sore_Throat_Project");
  const entries = readZip(L.attachmentsZip(att));
  assert.deepEqual(entries.map((e) => e.name).sort(), Object.keys(want).map((k) => "Sore_Throat_Project/" + k).sort());
  for (const e of entries) assert.ok(e.data.equals(want[e.name.replace(/^Sore_Throat_Project\//, "")]), e.name);
  const u = unzipTest(L.attachmentsZip(att));
  if (u) assert.equal(u.status, 0, u.stdout + u.stderr);

  // defaults, a custom label and --attach-root . (files at the zip root)
  r = run("--attach-label", "Get the project", "--attach-root", ".");
  assert.equal(r.status, 0, r.stderr);
  html = fs.readFileSync(out, "utf8");
  att = JSON.parse(/id="pf-attachments">([\s\S]*?)<\/script>/.exec(html)[1]);
  assert.equal(att.name, "sore_throat-composer-project.zip");
  assert.equal(att.root, "");
  assert.match(html, /<span class="dl-label">Get the project<\/span>/);
  assert.ok(readZip(L.attachmentsZip(att)).some((e) => e.name === "project.metadata"));

  // without --attach-dir there is no button and no attachments block
  r = spawnSync(process.execPath, [path.join(SCRIPTS, "build_report.mjs"), guideline, "--out", out, "--bin", bin], { encoding: "utf8" });
  assert.equal(r.status, 0, r.stderr);
  html = fs.readFileSync(out, "utf8");
  assert.ok(!html.includes('id="downloadBtn"') && !html.includes('id="pf-attachments"'));
  // the attach options need --attach-dir
  r = spawnSync(process.execPath, [path.join(SCRIPTS, "build_report.mjs"), guideline, "--attach-name", "x.zip"], { encoding: "utf8" });
  assert.equal(r.status, 2);
});
