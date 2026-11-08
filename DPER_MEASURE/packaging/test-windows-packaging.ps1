$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'package-functions.ps1')
$toolRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixture = Join-Path $toolRoot ('output/dpm31-packaging-tests/' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $fixture 'packaging'), (Join-Path $fixture 'assets'), (Join-Path $fixture 'dist') -Force | Out-Null
$script:passed = 0
function Check([string]$Name, [scriptblock]$Body) {
    & $Body
    $script:passed++
    Write-Output "PASS $Name"
}
function Require([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Reject([scriptblock]$Body, [string]$MessagePattern) {
    $rejected = $false
    try { & $Body | Out-Null } catch {
        $rejected = $_.Exception.Message -like $MessagePattern
        if (-not $rejected) { throw "Refus inattendu : $($_.Exception.Message) ; attendu : $MessagePattern" }
    }
    Require $rejected "Le refus attendu n'a pas eu lieu : $MessagePattern"
}
function Write-Pe([string]$Path, [uint16]$Machine = 0x8664) {
    New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force | Out-Null
    $bytes = [byte[]]::new(128)
    $bytes[0] = 0x4D; $bytes[1] = 0x5A; $bytes[0x3C] = 64
    $bytes[64] = 0x50; $bytes[65] = 0x45
    [BitConverter]::GetBytes($Machine).CopyTo($bytes, 68)
    [IO.File]::WriteAllBytes($Path, $bytes)
}

foreach ($name in @('build-windows-installer.ps1','package-functions.ps1','DPER_MEASURE.iss','NOTICE_UTILISATEUR.md')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $fixture "packaging/$name")
}
@'
[package]
name = "synthetic_packaging_fixture"
version = "1.2.3"
[dependencies]
example = "9.8.7"
'@ | Set-Content -LiteralPath (Join-Path $fixture 'Cargo.toml')
'fixture lock' | Set-Content -LiteralPath (Join-Path $fixture 'Cargo.lock')
'synthetic license' | Set-Content -LiteralPath (Join-Path $fixture 'LICENSE')
'synthetic image, never used as an actual PNG' | Set-Content -LiteralPath (Join-Path $fixture 'assets/demo_calibration_virtuelle.png')
'private photo' | Set-Content -LiteralPath (Join-Path $fixture 'assets/private.png')
'private project' | Set-Content -LiteralPath (Join-Path $fixture 'private.dperproj')
'/dist/' | Set-Content -LiteralPath (Join-Path $fixture '.gitignore')
& git -C $fixture init --quiet
if ($LASTEXITCODE -ne 0) { throw 'Initialisation du dépôt synthétique impossible.' }
& git -C $fixture add -- .
& git -C $fixture -c user.name=PackagingTest -c user.email=packaging-test@example.invalid commit --quiet -m 'synthetic fixture'
if ($LASTEXITCODE -ne 0) { throw 'Création du commit synthétique impossible.' }
$revision = (& git -C $fixture rev-parse HEAD).Trim()
$binary = Join-Path $fixture 'dist/input.exe'
Write-Pe $binary

Check 'syntaxe PowerShell des trois scripts' {
    foreach ($name in @('build-windows-installer.ps1','package-functions.ps1','test-windows-packaging.ps1')) {
        $parseErrors = $null; $tokens = $null
        $null = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $name), [ref]$tokens, [ref]$parseErrors)
        Require ($parseErrors.Count -eq 0) "Erreur de syntaxe : $name"
    }
}
Check 'version du package uniquement' {
    Require ((Get-DperPackageVersion (Join-Path $fixture 'Cargo.toml')) -eq '1.2.3') 'Mauvaise version.'
    'version = "8.8.8"' | Set-Content -LiteralPath (Join-Path $fixture 'dist/invalid.toml')
    Reject { Get-DperPackageVersion (Join-Path $fixture 'dist/invalid.toml') } '*Version package Cargo absente*'
}
Check 'commit explicite et refus branche/options/commit différent' {
    Require ((Assert-DperSourceRevision $fixture $revision) -eq $revision) 'Commit perdu.'
    Reject { Assert-DperSourceRevision $fixture 'dev' } '*identifiant de commit*'
    Reject { Assert-DperSourceRevision $fixture '--help' } '*identifiant de commit*'
    & git -C $fixture -c user.name=PackagingTest -c user.email=packaging-test@example.invalid commit --quiet --allow-empty -m 'second fixture'
    Reject { Assert-DperSourceRevision $fixture $revision } '*HEAD ne correspond*'
    $script:revision = (& git -C $fixture rev-parse HEAD).Trim()
}
Check 'refus modifications suivies et fichiers non suivis' {
    $path = Join-Path $fixture 'LICENSE'
    $original = [IO.File]::ReadAllBytes($path)
    try { 'changed' | Set-Content $path; Reject { Assert-DperSourceRevision $fixture $revision } '*modifications non livrées*' }
    finally { [IO.File]::WriteAllBytes($path, $original) }
    $untracked = Join-Path $fixture 'extra.txt'
    try { 'extra' | Set-Content $untracked; Reject { Assert-DperSourceRevision $fixture $revision } '*modifications non livrées*' }
    finally { Remove-Item -LiteralPath $untracked }
}
Check 'liste fermée sans photo/projet privés' {
    $files = @(Get-DperPayloadPlan $fixture $binary)
    Require ($files.Count -eq 4) 'Liste incomplète.'
    Require (($files.RelativePath -join ',') -eq 'dper_measure.exe,assets/demo_calibration_virtuelle.png,LICENSE.txt,NOTICE_UTILISATEUR.md') 'Liste non conforme.'
    $iss = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'DPER_MEASURE.iss') -Raw
    Require ($iss -notmatch '(?im)^Source:.*\*|recursesubdirs') 'Inclusion récursive interdite.'
    Require (($iss -split '\r?\n' | Where-Object { $_ -match '^Source:' }).Count -eq 4) 'Liste Inno incohérente.'
}
Check 'fichier requis absent et fichier vide refusés' {
    $path = Join-Path $fixture 'LICENSE'
    $original = [IO.File]::ReadAllBytes($path)
    try {
        Remove-Item -LiteralPath $path
        Reject { Get-DperPayloadPlan $fixture $binary } '*requis absent*'
        [IO.File]::WriteAllBytes($path, [byte[]]::new(0))
        Reject { Get-DperPayloadPlan $fixture $binary } '*requis vide*'
    } finally { [IO.File]::WriteAllBytes($path, $original) }
}
Check 'architecture PE x64, x86 et signature invalide' {
    Assert-DperX64Executable $binary
    Write-Pe (Join-Path $fixture 'dist/x86.exe') 0x14C
    Reject { Assert-DperX64Executable (Join-Path $fixture 'dist/x86.exe') } '*exige un exécutable Windows x64*'
    Reject { Assert-DperX64Executable (Join-Path $fixture 'LICENSE') } '*signature MZ*'
}

# Deliberately fake external build/compiler tools. No genuine Rust build or Inno
# installer is claimed by these orchestration checks.
$global:dpm31FakeCargoExit = 0
$global:dpm31FakeInnoExit = 0
$global:dpm31EmitInstaller = $true
function cargo {
    $buildIndex = [Array]::IndexOf($args, '--target-dir')
    Require ($buildIndex -ge 0 -and $args -contains '--locked' -and $args -contains 'x86_64-pc-windows-msvc') 'Arguments de build incorrects.'
    if ($global:dpm31FakeCargoExit -eq 0) { Write-Pe (Join-Path $args[$buildIndex + 1] 'x86_64-pc-windows-msvc/release/dper_measure.exe') }
    $global:LASTEXITCODE = $global:dpm31FakeCargoExit
}
function rustup { $global:LASTEXITCODE = 0; 'x86_64-pc-windows-msvc' }
function rustc { $global:LASTEXITCODE = 0; 'rustc synthetic-test-double' }
function ISCC.exe {
    $output = ($args | Where-Object { $_ -like '/O*' }).Substring(2)
    if ($global:dpm31FakeInnoExit -eq 0 -and $global:dpm31EmitInstaller) {
        'NOT A REAL INSTALLER - TEST DOUBLE' | Set-Content -LiteralPath (Join-Path $output 'DPER_Measure_Setup_1.2.3_x64.exe')
    }
    $global:LASTEXITCODE = $global:dpm31FakeInnoExit
}
$builder = Join-Path $fixture 'packaging/build-windows-installer.ps1'
Check 'échec Cargo sans publication' {
    $global:dpm31FakeCargoExit = 17
    try { Reject { & $builder -SourceCommit $revision } '*compilation Release a échoué*' }
    finally { $global:dpm31FakeCargoExit = 0 }
}
Check 'échec Inno sans publication' {
    $global:dpm31FakeInnoExit = 18
    try { Reject { & $builder -SourceCommit $revision } '*Inno Setup a échoué*' }
    finally { $global:dpm31FakeInnoExit = 0 }
}
Check 'sortie Inno absente refusée' {
    $global:dpm31EmitInstaller = $false
    try { Reject { & $builder -SourceCommit $revision } '*ne fournit pas le fichier attendu*' }
    finally { $global:dpm31EmitInstaller = $true }
}
Check 'orchestration et manifestes avec doublures, aucun écrasement' {
    $output = & $builder -SourceCommit $revision
    Require (Test-Path -LiteralPath $output) 'Sortie manquante.'
    $manifest = Get-Content -LiteralPath (Join-Path (Split-Path $output -Parent) 'manifest.json') -Raw | ConvertFrom-Json
    Require ($manifest.source_commit -eq $revision -and $manifest.target -eq 'x86_64-pc-windows-msvc') 'Traçabilité perdue.'
    Require ($manifest.files.Count -eq 4) 'Inventaire incorrect.'
    Require ($manifest.installer_sha256 -eq (Get-FileHash -LiteralPath $output).Hash) 'Empreinte incorrecte.'
    Reject { & $builder -SourceCommit $revision } '*livraison existe déjà*'
}
Write-Output "RESULTAT : $script:passed contrôles réussis. Artefacts synthétiques conservés : $fixture"
