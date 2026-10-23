#!/usr/bin/env node
/**
 * Postinstall check: verifies that the platform binary was installed. Never fails
 * the install (unsupported platforms can still build from source).
 */
const fs = require("fs");
const path = require("path");
const { BINARY_NAME, platformKey, packageName } = require("./platform");

if (!packageName) {
  console.warn(
    `proforma: platform '${platformKey}' is not officially supported.\n` +
      "Build from source: cargo install --git https://github.com/Minitour/proforma"
  );
  process.exit(0);
}

try {
  const pkgDir = path.dirname(require.resolve(`${packageName}/package.json`));
  const candidate = path.join(pkgDir, "bin", BINARY_NAME);
  if (!fs.existsSync(candidate)) {
    console.warn(`proforma: optional package found but binary missing at ${candidate}`);
  }
} catch {
  console.warn(
    `proforma: platform package '${packageName}' was not installed (optional).\n` +
      "Reinstall without --no-optional, or build from source."
  );
}
