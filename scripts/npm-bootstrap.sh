#!/usr/bin/env bash
# One-time npm bootstrap (same approach as owl-mcp).
#
# npm only lets you attach a trusted publisher (GitHub OIDC) to a package that
# already exists, so the very first version is published from YOUR npm session
# (`npm login`: browser + 2FA, no long-lived token). After that every release is
# published by .github/workflows/release.yml through OIDC; no npm token is stored.
#
# 1. Build the packages in CI without publishing:
#      gh workflow run release.yml -R Minitour/proforma -f version=0.1.0
# 2. When it finishes:
#      scripts/npm-bootstrap.sh <run-id>      (gh run list -w release.yml shows the id)
# 3. Later releases: git tag v0.1.1 && git push origin v0.1.1
set -euo pipefail

REPO="Minitour/proforma"
WORKFLOW="release.yml"
RUN="${1:?usage: scripts/npm-bootstrap.sh <release workflow run id>}"
PLATFORMS="linux-x64 linux-arm64 darwin-x64 darwin-arm64 win32-x64 win32-arm64"
PACKAGES="$(for p in $PLATFORMS; do printf '@proforma-mcp/proforma-%s ' "$p"; done)proforma-mcp"

dir="$(mktemp -d)"
echo "Downloading npm packages from run $RUN ..."
gh run download "$RUN" -R "$REPO" -n npm-packages -D "$dir"
ls "$dir"

if ! npm whoami >/dev/null 2>&1; then
  echo "Log in to npm (interactive; uses your 2FA):"
  npm login
fi
echo "Publishing as $(npm whoami)"

for p in $PACKAGES; do
  # npm pack file name: '@scope/name' -> 'scope-name-<version>.tgz'
  file="${p#@}"; file="${file/\//-}"
  tgz="$(ls "$dir"/"$file"-[0-9]*.tgz)"
  version="$(basename "$tgz" .tgz)"; version="${version#"$file"-}"
  # resumable: skip versions that are already on the registry
  if [ "$(npm view "$p@$version" version 2>/dev/null)" = "$version" ]; then
    echo "== skip $p@$version (already published)"
    continue
  fi
  echo "== npm publish $(basename "$tgz")"
  npm publish "$tgz" --access public
done

# `npm trust` needs npm >= 11.15; run it through npx (npm 11; npm 12 needs a newer Node) so the global npm is untouched.
for p in $PACKAGES; do
  echo "== trusted publisher for $p -> $REPO / $WORKFLOW"
  npx -y npm@11 trust github "$p" --repo "$REPO" --file "$WORKFLOW" --allow-publish --yes
done

echo
echo "Done. Verify with: npx -y npm@11 trust list proforma-mcp"
echo "Optional: on npmjs.com set each package's publishing access to"
echo "'require 2FA and disallow tokens' so only the GitHub workflow can publish."
