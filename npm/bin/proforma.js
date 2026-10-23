#!/usr/bin/env node
/**
 * Entry point for `npx proforma` / `proforma`.
 * Resolves the platform-specific native binary (optionalDependencies), falling
 * back to a locally built binary, and runs it with the same arguments and stdio.
 */
const { spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");
const { BINARY_NAME, platformKey, packageName } = require("./platform");

function resolveBinary() {
  if (process.env.PROFORMA_BINARY && fs.existsSync(process.env.PROFORMA_BINARY)) {
    return process.env.PROFORMA_BINARY;
  }
  if (packageName) {
    try {
      const pkgDir = path.dirname(require.resolve(`${packageName}/package.json`));
      const candidate = path.join(pkgDir, "bin", BINARY_NAME);
      if (fs.existsSync(candidate)) return candidate;
    } catch {
      // optional package not installed
    }
  }
  for (const profile of ["release", "debug"]) {
    const candidate = path.join(__dirname, "..", "..", "target", profile, BINARY_NAME);
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

const binary = resolveBinary();
if (!binary) {
  console.error(
    `proforma: no pre-built binary found for platform '${platformKey}'.\n` +
      "Build from source: cargo install --git https://github.com/Minitour/proforma"
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit", env: process.env });
if (result.error) {
  console.error("proforma: failed to start:", result.error.message);
  process.exit(1);
}
process.exit(result.status ?? 0);
