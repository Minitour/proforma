/**
 * Shared platform -> package resolution for the launcher and the postinstall check.
 */
const PLATFORM_PACKAGES = {
  "linux-x64": "@proforma-mcp/proforma-linux-x64",
  "linux-arm64": "@proforma-mcp/proforma-linux-arm64",
  "darwin-x64": "@proforma-mcp/proforma-darwin-x64",
  "darwin-arm64": "@proforma-mcp/proforma-darwin-arm64",
  "win32-x64": "@proforma-mcp/proforma-win32-x64",
  "win32-arm64": "@proforma-mcp/proforma-win32-arm64",
};

const BINARY_NAME = process.platform === "win32" ? "proforma.exe" : "proforma";
const platformKey = `${process.platform}-${process.arch}`;

module.exports = {
  PLATFORM_PACKAGES,
  BINARY_NAME,
  platformKey,
  packageName: PLATFORM_PACKAGES[platformKey],
};
