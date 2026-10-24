$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$cargoToml = Get-Content -LiteralPath (Join-Path $repoRoot "Cargo.toml") -Raw
if ($cargoToml -notmatch '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"') {
    throw "Impossible de lire la version semver de Cargo.toml."
}
$appVersion = $Matches[1]
$installerCompiler = Get-Command "ISCC.exe" -ErrorAction SilentlyContinue
if ($null -eq $installerCompiler) {
    throw "Inno Setup Compiler (ISCC.exe) doit être installé et accessible dans PATH."
}

Push-Location $repoRoot
try {
    & cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "La compilation Release a échoué avec le code $LASTEXITCODE." }
    $scriptPath = Join-Path $PSScriptRoot "DPER_MEASURE.iss"
    & $installerCompiler.Source "/DAppVersion=$appVersion" $scriptPath
    if ($LASTEXITCODE -ne 0) { throw "La création de l'installateur a échoué avec le code $LASTEXITCODE." }
    Write-Output (Join-Path $repoRoot "dist\installer\DPER_Measure_Setup_${appVersion}_x64.exe")
}
finally {
    Pop-Location
}
