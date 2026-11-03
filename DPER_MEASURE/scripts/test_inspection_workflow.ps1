param(
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [string]$TargetDirectory,
    [ValidateRange(18765,65535)][int]$Port = 18766
)
$ErrorActionPreference = 'Stop'
$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (!(($output + [IO.Path]::DirectorySeparatorChar).StartsWith(
    $project + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase))) {
    throw 'Le dossier de résultats doit rester dans ce dossier DPER_MEASURE.'
}
if (Test-Path -LiteralPath $output) { throw 'Le dossier de résultats existe déjà ; choisir un nouveau nom.' }
if (!$TargetDirectory) { $TargetDirectory = Join-Path $project 'target' }
$target = [IO.Path]::GetFullPath($TargetDirectory)
$manifest = Join-Path $project 'Cargo.toml'
$profile = Join-Path $env:APPDATA 'DPER_Measure\calibrations.json'
$profileBefore = if (Test-Path -LiteralPath $profile) { (Get-FileHash -LiteralPath $profile).Hash } else { 'absent' }
$oldEnvironment = @{}
foreach ($key in @('TEMP','TMP','DPER_TEST_DECISION_OUTPUT','DPER_TEST_ACCEPTANCE_OUTPUT','DPER_MEASURE_MCP_PORT','DPER_TEST_HOST_STOP')) {
    $oldEnvironment[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
}
$process = $null
$started = $null
$stop = Join-Path $output 'owned-host.stop'
$success = $false
New-Item -ItemType Directory -Path $output | Out-Null
Push-Location $project
try {
    $env:TEMP = Join-Path $output 'rust-tmp'
    $env:TMP = $env:TEMP
    New-Item -ItemType Directory -Path $env:TEMP | Out-Null
    $env:DPER_TEST_DECISION_OUTPUT = Join-Path $output 'decision-exports'
    $env:DPER_TEST_ACCEPTANCE_OUTPUT = Join-Path $output 'native-acceptance'
    $common = @('--locked','--manifest-path',$manifest,'--target-dir',$target)
    & cargo test @common *> (Join-Path $output 'tests.log')
    if ($LASTEXITCODE -ne 0) { throw 'Tests en échec ; consulter tests.log.' }
    $summaries = @(Get-ChildItem -LiteralPath $env:DPER_TEST_ACCEPTANCE_OUTPUT -Recurse -Filter 'acceptance_summary.json')
    if ($summaries.Count -ne 1) { throw 'Synthèse de recette native absente ou ambiguë.' }
    $native = Get-Content -LiteralPath $summaries[0].FullName -Raw | ConvertFrom-Json
    if (!$native.completed -or !$native.independent_mcp_preview_and_commit_geometry_evidence_value_outcome_parity) {
        throw 'La recette native indépendante n''a pas réussi.'
    }
    foreach ($name in @($native.artifacts) + @($native.source_image)) {
        $artifactPath = [IO.Path]::GetFullPath((Join-Path $summaries[0].DirectoryName $name))
        if (!$artifactPath.StartsWith($summaries[0].DirectoryName + [IO.Path]::DirectorySeparatorChar,
            [StringComparison]::OrdinalIgnoreCase) -or !(Test-Path -LiteralPath $artifactPath -PathType Leaf) -or
            (Get-Item -LiteralPath $artifactPath).Length -le 0) {
            throw "Artefact de recette absent ou hors du dossier prévu : $name"
        }
    }
    & cargo build @common *> (Join-Path $output 'build.log')
    if ($LASTEXITCODE -ne 0) { throw 'Compilation en échec ; consulter build.log.' }
    & cargo test @common --no-run --message-format=json *> (Join-Path $output 'test-build.jsonl')
    if ($LASTEXITCODE -ne 0) { throw 'Préparation de l''exécutable de test en échec.' }
    $executables = @(Get-Content -LiteralPath (Join-Path $output 'test-build.jsonl') | ForEach-Object {
        try {
            $artifact = $_ | ConvertFrom-Json -ErrorAction Stop
            if ($artifact.reason -eq 'compiler-artifact' -and $artifact.profile.test -and $artifact.executable) { $artifact.executable }
        } catch { }
    } | Select-Object -Unique)
    if ($executables.Count -ne 1) { throw 'Exécutable de test absent ou ambigu.' }
    $exe = [IO.Path]::GetFullPath($executables[0])
    if (!(Test-Path -LiteralPath $exe)) { throw 'Exécutable de test introuvable.' }
    $probe = [Net.Sockets.TcpClient]::new()
    try {
        try {
            $connected = $probe.ConnectAsync('127.0.0.1', $Port).Wait(300)
            if ($connected -and $probe.Connected) { throw 'Port de test occupé : aucun service existant ne sera arrêté.' }
        } catch [AggregateException] { }
    } finally { $probe.Dispose() }
    $smoke = Join-Path $output 'mcp'
    & python (Join-Path $project 'scripts\smoke_inspection_mcp.py') fixtures $smoke *> (Join-Path $output 'fixtures.log')
    if ($LASTEXITCODE -ne 0) { throw 'Création des fixtures en échec.' }
    $env:DPER_MEASURE_MCP_PORT = $Port.ToString()
    $env:DPER_TEST_HOST_STOP = $stop
    $process = Start-Process -FilePath $exe -ArgumentList '--exact','inspection_mcp_host_tests::owned_headless_mcp_host','--ignored','--nocapture','--test-threads=1' -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $output 'host.stdout.log') -RedirectStandardError (Join-Path $output 'host.stderr.log')
    $started = $process.StartTime
    $ready = $false
    for ($attempt=0; $attempt -lt 300; $attempt++) {
        $status = Get-Content -LiteralPath (Join-Path $output 'host.stderr.log') -Raw -ErrorAction SilentlyContinue
        if ($status -like "*OWNED_MCP_RUNNING port=$Port*") {
            $socket = [Net.Sockets.TcpClient]::new()
            try { $socket.Connect('127.0.0.1', $Port); $ready = $true } finally { $socket.Dispose() }
            break
        }
        $process.Refresh()
        if ($process.HasExited) { break }
        Start-Sleep -Milliseconds 100
    }
    if (!$ready) { throw 'L''hôte MCP de test n''a pas atteint l''état Running ; consulter host.stderr.log.' }
    & python (Join-Path $project 'scripts\smoke_inspection_mcp.py') run $smoke --port $Port *> (Join-Path $output 'mcp.log')
    if ($LASTEXITCODE -ne 0) { throw 'Parcours MCP en échec ; consulter mcp.log.' }
    $remote = Get-Content -LiteralPath (Join-Path $smoke 'smoke_summary.json') -Raw | ConvertFrom-Json
    if (!$remote.passed -or !$remote.valid_contour_forged_authority_safe -or
            @($remote.tools_discovered).Count -lt 18 -or $remote.adversarial_cases_passed -lt 7) {
        throw 'Synthèse du parcours MCP incomplète ou en échec.'
    }
    & python (Join-Path $project 'scripts\verify_drawing_attachments_mcp.py') (Join-Path $output 'attachment-checks') --port $Port *> (Join-Path $output 'attachments.log')
    if ($LASTEXITCODE -ne 0) { throw 'Hypothèses d''attache PDF en échec ; consulter attachments.log.' }
    $attachments = Get-Content -LiteralPath (Join-Path $output 'attachment-checks\attachment_summary.json') -Raw | ConvertFrom-Json
    if (!$attachments.passed -or @($attachments.cases).Count -ne 19) { throw 'Recette PDF incomplète.' }
    [IO.File]::WriteAllText($stop, 'stop')
    if (!$process.WaitForExit(5000) -or $process.ExitCode -ne 0) { throw 'L''hôte de test n''a pas terminé avec succès.' }
    $success = $true
} finally {
    if ($process) {
        [IO.File]::WriteAllText($stop, 'stop')
        if (!$process.WaitForExit(5000)) {
            $owned = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
            if ($owned -and $owned.Path -eq $exe -and $owned.StartTime -eq $started) { Stop-Process -Id $process.Id }
        }
    }
    foreach ($key in $oldEnvironment.Keys) { [Environment]::SetEnvironmentVariable($key, $oldEnvironment[$key], 'Process') }
    Pop-Location
    $profileAfter = if (Test-Path -LiteralPath $profile) { (Get-FileHash -LiteralPath $profile).Hash } else { 'absent' }
    $stopped = !$process -or $null -eq (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)
    [pscustomobject]@{
        completed=($success -and $stopped -and $profileBefore -eq $profileAfter); test_port=$Port; owned_test_process=$(if ($process) {$process.Id} else {$null})
        test_process_stopped=$stopped; user_profile_unchanged=($profileBefore -eq $profileAfter)
        host_kind='test-only real MCP/application transport; synthetic data; no camera/desktop/GPU qualification'
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'isolation_check.json')
    if (!$stopped) { throw 'Le processus propriétaire de test doit être arrêté.' }
    if ($profileBefore -ne $profileAfter) { throw 'Le profil utilisateur a changé pendant la recette.' }
}
Write-Output $output
