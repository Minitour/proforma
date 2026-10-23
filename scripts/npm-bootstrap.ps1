# One-time npm bootstrap for Windows PowerShell (same steps as npm-bootstrap.sh).
#
# npm only lets you attach a trusted publisher (GitHub OIDC) to a package that
# already exists, so the very first version is published from YOUR npm session
# (`npm login`: browser + 2FA, no long-lived token). After that every release is
# published by .github/workflows/release.yml through OIDC; no npm token is stored.
#
# Usage (from anywhere):
#   powershell -ExecutionPolicy Bypass -File <repo>\scripts\npm-bootstrap.ps1 <release-run-id>
param(
    [Parameter(Mandatory = $true)][string]$RunId
)
# 'Continue': in Windows PowerShell 5.1 'Stop' turns any stderr output of native
# commands (npm prints notices there) into a terminating error. Failures are
# detected through $LASTEXITCODE instead (Invoke-Checked).
$ErrorActionPreference = 'Continue'

$Repo = 'Minitour/proforma'
$Workflow = 'release.yml'
$Platforms = 'linux-x64', 'linux-arm64', 'darwin-x64', 'darwin-arm64', 'win32-x64', 'win32-arm64'
$Packages = @($Platforms | ForEach-Object { "@proforma-mcp/proforma-$_" }) + 'proforma-mcp'

function Invoke-Checked([string]$What, [scriptblock]$Block) {
    & $Block
    if ($LASTEXITCODE -ne 0) { throw "$What failed (exit code $LASTEXITCODE)" }
}

$dir = Join-Path ([System.IO.Path]::GetTempPath()) ("proforma-npm-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $dir | Out-Null
Write-Host "Downloading npm packages from run $RunId ..."
Invoke-Checked 'gh run download' { gh run download $RunId -R $Repo -n npm-packages -D $dir }
Get-ChildItem $dir | ForEach-Object { Write-Host "  $($_.Name)" }

$null = npm whoami 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host 'Log in to npm (interactive; uses your 2FA):'
    Invoke-Checked 'npm login' { npm login }
}
$user = npm whoami
Write-Host "Publishing as $user"

foreach ($p in $Packages) {
    # npm pack file name: '@scope/name' -> 'scope-name-<version>.tgz'
    $file = ($p -replace '^@', '') -replace '/', '-'
    $tgz = Get-ChildItem $dir -File | Where-Object { $_.Name -like "$file-[0-9]*.tgz" } | Select-Object -First 1
    if (-not $tgz) { throw "package tarball for $p not found in $dir" }
    # resumable: skip versions that are already on the registry
    $version = $tgz.BaseName.Substring($file.Length + 1)
    $existing = npm view "$p@$version" version 2>&1
    if ($LASTEXITCODE -eq 0 -and "$existing".Trim() -eq $version) {
        Write-Host "== skip $p@$version (already published)"
        continue
    }
    Write-Host "== npm publish $($tgz.Name)"
    Invoke-Checked "npm publish $($tgz.Name)" { npm publish $tgz.FullName --access public }
}

# `npm trust` needs npm >= 11.15; run it through npx (npm 11; npm 12 needs a newer Node) so the global npm is untouched.
foreach ($p in $Packages) {
    Write-Host "== trusted publisher for $p -> $Repo / $Workflow"
    Invoke-Checked "npm trust $p" { npx -y npm@11 trust github $p --repo $Repo --file $Workflow --allow-publish --yes }
}

Write-Host ''
Write-Host 'Done. Verify with: npx -y npm@11 trust list proforma-mcp'
Write-Host "Optional: on npmjs.com set each package's publishing access to"
Write-Host "'require 2FA and disallow tokens' so only the GitHub workflow can publish."
