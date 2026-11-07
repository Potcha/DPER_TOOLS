Set-StrictMode -Version Latest

function Get-DperPackageVersion {
    param([Parameter(Mandatory)][string]$ManifestPath)
    $manifest = Get-Content -LiteralPath $ManifestPath -Raw
    $package = [regex]::Match($manifest, '(?ms)^\[package\]\s*\r?\n(?<body>.*?)(?=^\[|\z)')
    $version = [regex]::Match($package.Groups['body'].Value, '(?m)^version\s*=\s*"(?<value>[0-9]+\.[0-9]+\.[0-9]+)"\s*$')
    if (-not $version.Success) { throw 'Version package Cargo absente ou non prise en charge (attendu : X.Y.Z).' }
    return $version.Groups['value'].Value
}

function Assert-DperSourceRevision {
    param([Parameter(Mandatory)][string]$RepoRoot, [Parameter(Mandatory)][string]$SourceCommit)
    # Immutable object identifiers only, never an option or a mutable branch name.
    if ($SourceCommit -notmatch '^[0-9a-fA-F]{7,40}$') { throw 'SourceCommit doit être un identifiant de commit Git de 7 à 40 caractères hexadécimaux.' }
    $resolved = & git -C $RepoRoot rev-parse --verify "$SourceCommit^{commit}" 2>$null
    if ($LASTEXITCODE -ne 0) { throw 'Commit source introuvable dans le dépôt local.' }
    $head = & git -C $RepoRoot rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or "$head" -ne "$resolved") { throw 'HEAD ne correspond pas au commit demandé. Sélectionnez la version à livrer avant de lancer le packaging.' }
    $status = & git -C $RepoRoot status --porcelain --untracked-files=normal -- .
    if ($LASTEXITCODE -ne 0) { throw 'Impossible de vérifier les modifications locales.' }
    if ($status) { throw "Le dossier DPER_MEASURE contient des modifications non livrées. Packaging refusé ; aucun fichier local n'a été modifié." }
    return "$resolved".Trim()
}

function Get-DperPayloadPlan {
    param([Parameter(Mandatory)][string]$RepoRoot, [Parameter(Mandatory)][string]$BinaryPath)
    $entries = @(
        @{ Source = $BinaryPath; RelativePath = 'dper_measure.exe' },
        @{ Source = (Join-Path $RepoRoot 'assets/demo_calibration_virtuelle.png'); RelativePath = 'assets/demo_calibration_virtuelle.png' },
        @{ Source = (Join-Path $RepoRoot 'LICENSE'); RelativePath = 'LICENSE.txt' },
        @{ Source = (Join-Path $RepoRoot 'packaging/NOTICE_UTILISATEUR.md'); RelativePath = 'NOTICE_UTILISATEUR.md' }
    )
    foreach ($entry in $entries) {
        if (-not (Test-Path -LiteralPath $entry.Source -PathType Leaf)) { throw "Fichier requis absent : $($entry.RelativePath)" }
        $item = Get-Item -LiteralPath $entry.Source -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Lien de fichier interdit dans le package : $($entry.RelativePath)" }
        if ($item.Length -eq 0) { throw "Fichier requis vide : $($entry.RelativePath)" }
        $parent = $item.Directory
        while ($null -ne $parent) {
            if (($parent.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Dossier redirigé interdit : $($entry.RelativePath)" }
            $parent = $parent.Parent
        }
        [pscustomobject]@{ Source = $item.FullName; RelativePath = $entry.RelativePath }
    }
}

function Assert-DperX64Executable {
    param([Parameter(Mandatory)][string]$Path)
    $stream = [IO.File]::OpenRead($Path)
    $reader = [IO.BinaryReader]::new($stream)
    try {
        if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5A4D) { throw 'Exécutable Windows invalide (signature MZ).' }
        $stream.Position = 0x3C
        $offset = $reader.ReadInt32()
        if ($offset -lt 64 -or $offset -gt $stream.Length - 6) { throw 'En-tête PE invalide.' }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne 0x8664) { throw 'Le package exige un exécutable Windows x64 (AMD64).' }
    } finally { $reader.Dispose() }
}
