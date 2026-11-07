[CmdletBinding()]
param([Parameter(Mandatory)][string]$SourceCommit)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'package-functions.ps1')
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'Le packaging exige Windows.' }
foreach ($command in @('git', 'cargo', 'rustup', 'rustc', 'ISCC.exe')) {
    if ($null -eq (Get-Command $command -ErrorAction SilentlyContinue)) { throw "Prérequis absent : $command. Installez-le et rendez-le accessible dans PATH." }
}
$revision = Assert-DperSourceRevision -RepoRoot $repoRoot -SourceCommit $SourceCommit
$appVersion = Get-DperPackageVersion (Join-Path $repoRoot 'Cargo.toml')
$target = 'x86_64-pc-windows-msvc'
$targets = & rustup target list --installed
if ($LASTEXITCODE -ne 0 -or $targets -notcontains $target) { throw "Cible Rust absente : $target. Utilisez rustup target add $target ; les outils MSVC sont également requis." }
$outputRoot = Join-Path $repoRoot "dist/installer/${appVersion}_$($revision.Substring(0, 12))"
if (Test-Path -LiteralPath $outputRoot) { throw "La livraison existe déjà : $outputRoot. Aucun écrasement automatique." }
# Explicit target directory avoids CARGO_TARGET_DIR and stale binaries.
$buildRoot = Join-Path $repoRoot 'dist/packaging-build'
$stage = Join-Path $repoRoot ('dist/packaging-stage/' + [guid]::NewGuid().ToString('N'))
$payload = Join-Path $stage 'payload'
$compilerOutput = Join-Path $stage 'installer'
New-Item -ItemType Directory -Path $payload, $compilerOutput -Force | Out-Null
Push-Location $repoRoot
try {
    & cargo build --release --locked --bin dper_measure --target $target --target-dir $buildRoot
    if ($LASTEXITCODE -ne 0) { throw "La compilation Release a échoué avec le code $LASTEXITCODE. Aucun installateur publié." }
    $binary = Join-Path $buildRoot "$target/release/dper_measure.exe"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'La compilation ne fournit pas le binaire attendu.' }
    Assert-DperX64Executable $binary
    $files = @(Get-DperPayloadPlan -RepoRoot $repoRoot -BinaryPath $binary)
    $hashes = foreach ($file in $files) {
        $destination = Join-Path $payload $file.RelativePath
        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
        Copy-Item -LiteralPath $file.Source -Destination $destination
        $sourceHash = (Get-FileHash -LiteralPath $file.Source -Algorithm SHA256).Hash
        $stagedHash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
        if ($sourceHash -ne $stagedHash) { throw "Copie incohérente : $($file.RelativePath)." }
        [ordered]@{ path = $file.RelativePath; sha256 = $stagedHash }
    }
    $null = Assert-DperSourceRevision -RepoRoot $repoRoot -SourceCommit $revision
    & ISCC.exe "/DAppVersion=$appVersion" "/DPackageRoot=$payload" "/O$compilerOutput" (Join-Path $PSScriptRoot 'DPER_MEASURE.iss')
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup a échoué avec le code $LASTEXITCODE. Aucun installateur publié." }
    $installer = Join-Path $compilerOutput "DPER_Measure_Setup_${appVersion}_x64.exe"
    if (-not (Test-Path -LiteralPath $installer -PathType Leaf) -or (Get-Item -LiteralPath $installer).Length -eq 0) { throw 'Inno Setup ne fournit pas le fichier attendu.' }
    $null = Assert-DperSourceRevision -RepoRoot $repoRoot -SourceCommit $revision
    $rustVersion = & rustc --version
    if ($LASTEXITCODE -ne 0) { throw 'Impossible de lire la version du compilateur Rust.' }
    $manifest = [ordered]@{
        source_commit = $revision; package_version = $appVersion; target = $target
        cargo_lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $repoRoot 'Cargo.lock') -Algorithm SHA256).Hash
        files = $hashes; installer_sha256 = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
        rustc = "$rustVersion"; generated_at_utc = [DateTime]::UtcNow.ToString('o')
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $compilerOutput 'manifest.json') -Encoding utf8
    New-Item -ItemType Directory -Path (Split-Path -Parent $outputRoot) -Force | Out-Null
    # Fail if another run published the same revision in the meantime.
    [IO.Directory]::Move($compilerOutput, $outputRoot)
    Write-Output (Join-Path $outputRoot (Split-Path -Leaf $installer))
} finally { Pop-Location }
