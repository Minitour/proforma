// Load the viewer's pure helpers (skills/proforma/scripts/report/viewer-lib.js) the way the
// page sees them: a classic script that defines the global PFLib.
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { fileURLToPath } from "node:url";

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
export const SCRIPTS = path.join(ROOT, "skills", "proforma", "scripts");

export function loadLib() {
  const src = fs.readFileSync(path.join(SCRIPTS, "report", "viewer-lib.js"), "utf8");
  return vm.runInNewContext(src + "\n;PFLib", { TextEncoder, atob }, { filename: "viewer-lib.js" });
}
