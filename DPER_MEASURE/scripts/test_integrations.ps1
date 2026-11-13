param([int]$Port = 18776)
$ErrorActionPreference = 'Stop'
if ($Port -lt 18765 -or $Port -gt 65535) { throw 'Port de test dédié requis.' }
$project = Split-Path $PSScriptRoot -Parent
$output = Join-Path $project ('output/integrations-mcp-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $output | Out-Null
# Reuse the existing build cache; no full test suite is executed.
& cargo test --manifest-path (Join-Path $project 'Cargo.toml') --locked --no-run --message-format=json 1> (Join-Path $output 'build.jsonl') 2> (Join-Path $output 'build.log')
if ($LASTEXITCODE -ne 0) { throw "Compilation des tests en échec : $output" }
$executables = @(Get-Content (Join-Path $output 'build.jsonl') | ForEach-Object {
    $artifact = $_ | ConvertFrom-Json
    if ($artifact.reason -eq 'compiler-artifact' -and $artifact.profile.test -and $artifact.executable) { $artifact.executable }
} | Select-Object -Unique)
if ($executables.Count -ne 1) { throw 'Exécutable de test absent ou ambigu.' }
$probe = [Net.Sockets.TcpClient]::new()
try { $probe.Connect('127.0.0.1', $Port); throw 'Port occupé ; aucun service existant ne sera arrêté.' }
catch [Net.Sockets.SocketException] { }
finally { $probe.Dispose() }
$previousPort = $env:DPER_MEASURE_MCP_PORT
$previousStop = $env:DPER_TEST_HOST_STOP
$previousUtf8 = $env:PYTHONUTF8
$stop = Join-Path $output 'stop'
$process = $null
try {
    $env:DPER_MEASURE_MCP_PORT = "$Port"
    $env:DPER_TEST_HOST_STOP = $stop
    $env:PYTHONUTF8 = '1'
    $process = Start-Process -FilePath $executables[0] -WorkingDirectory $project -ArgumentList '--exact','inspection_mcp_host_tests::owned_headless_mcp_host','--ignored','--nocapture','--test-threads=1' -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $output 'host.stdout.log') -RedirectStandardError (Join-Path $output 'host.stderr.log')
    $ready = $false
    for ($attempt=0; $attempt -lt 100; $attempt++) {
        if ((Get-Content (Join-Path $output 'host.stderr.log') -Raw -ErrorAction SilentlyContinue) -like "*OWNED_MCP_RUNNING port=$Port*") { $ready = $true; break }
        $process.Refresh()
        if ($process.HasExited) { break }
        Start-Sleep -Milliseconds 100
    }
    if (!$ready) { throw 'Hôte MCP non prêt.' }
    & python (Join-Path $PSScriptRoot 'verify_integrations_mcp.py') (Join-Path $output 'recipe') --test-port $Port *> (Join-Path $output 'recipe.log')
    if ($LASTEXITCODE -ne 0) { Get-Content (Join-Path $output 'recipe.log') -Tail 16; throw 'Recette MCP en échec.' }
    Get-Content (Join-Path $output 'recipe/summary.json')
} finally {
    try {
        if ($process) {
            New-Item -ItemType File -Path $stop -Force | Out-Null
            if (!$process.WaitForExit(10000)) { throw 'Hôte encore actif ; consulter les journaux.' }
            $process.Refresh()
            if ($process.ExitCode -ne 0) { throw "Hôte terminé en échec : $($process.ExitCode)" }
        }
    } finally {
        $env:DPER_MEASURE_MCP_PORT = $previousPort
        $env:DPER_TEST_HOST_STOP = $previousStop
        $env:PYTHONUTF8 = $previousUtf8
        Write-Output "Preuves : $output"
    }
}
