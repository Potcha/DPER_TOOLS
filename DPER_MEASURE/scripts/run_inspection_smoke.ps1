param([int]$Port = 18765, [string]$OutputDirectory, [switch]$SkipBuild, [string]$ExecutablePath)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
if ($Port -le 0 -or $Port -gt 65535 -or $Port -eq 8765) { throw 'Choisissez un port de test distinct de 8765.' }
if (@(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue).Count) { throw 'Port de test déjà occupé.' }
if (-not $SkipBuild) {
    cargo build --locked
    if ($LASTEXITCODE -ne 0) { throw 'Compilation échouée.' }
}
if (-not $OutputDirectory) { $OutputDirectory = 'output\automation-validation\smoke-'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'-'+[guid]::NewGuid().ToString('N').Substring(0,8) }
$folder = [IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))
if (-not $folder.StartsWith(($root.TrimEnd('\')+'\'), [StringComparison]::OrdinalIgnoreCase)) { throw 'Les sorties doivent rester dans DPER_MEASURE.' }
if (Test-Path -LiteralPath $folder) { throw 'Le dossier de sortie existe déjà ; aucun écrasement.' }
$profilePath = Join-Path $env:APPDATA 'DPER_Measure\calibrations.json'
if (Test-Path -LiteralPath $profilePath) {
    $profile = Get-Content -LiteralPath $profilePath -Raw | ConvertFrom-Json
    if (@($profile.calibrations | Where-Object { -not $_.id }).Count) { throw 'Migration de configuration possible ; test suspendu.' }
    $before = (Get-FileHash -LiteralPath $profilePath).Hash
} else {
    if (Test-Path -LiteralPath (Join-Path $env:APPDATA 'MicroCraft\calibrations.json')) { throw 'Migration de configuration historique possible ; test suspendu.' }
    $before = 'absent'
}
python scripts\smoke_inspection_mcp.py fixtures $folder
if ($LASTEXITCODE -ne 0) { throw 'Génération des données synthétiques échouée.' }
$before | Set-Content -LiteralPath (Join-Path $folder 'profile_sha_before.txt')
$executable = if ($ExecutablePath) { [IO.Path]::GetFullPath($ExecutablePath) } else { Join-Path $root 'target\debug\dper_measure.exe' }
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'Exécutable de test absent.' }
$previousPort = $env:DPER_MEASURE_MCP_PORT
$testProcess = $null
try {
    $env:DPER_MEASURE_MCP_PORT = $Port.ToString()
    $testProcess = Start-Process -FilePath $executable -WorkingDirectory $root -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $folder 'application.stdout.log') -RedirectStandardError (Join-Path $folder 'application.stderr.log')
    $startedAt = $testProcess.StartTime
    $testProcess.Id | Set-Content -LiteralPath (Join-Path $folder 'owned_process_id.txt')
    $ready = $false
    for ($attempt=0; $attempt -lt 30; $attempt++) {
        $listener = @(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
        if ($listener.Count) {
            if ($listener.Count -ne 1 -or $listener[0].OwningProcess -ne $testProcess.Id) { throw 'Le port appartient à un autre processus ; aucun appel MCP.' }
            $ready = $true
            break
        }
        if ($testProcess.HasExited) { break }
        Start-Sleep -Milliseconds 200
    }
    if (-not $ready) { throw 'Instance de test indisponible.' }
    python scripts\smoke_inspection_mcp.py run $folder --port $Port
    if ($LASTEXITCODE -ne 0) { throw 'Scénario MCP échoué ; traces conservées.' }
} finally {
    $env:DPER_MEASURE_MCP_PORT = $previousPort
    if ($testProcess) {
        $owned = Get-Process -Id $testProcess.Id -ErrorAction SilentlyContinue
        if ($owned -and $owned.Path -eq $executable -and $owned.StartTime -eq $startedAt) { Stop-Process -Id $testProcess.Id }
    }
    $after = if (Test-Path -LiteralPath $profilePath) { (Get-FileHash -LiteralPath $profilePath).Hash } else { 'absent' }
    [pscustomobject]@{owned_test_process=$(if($testProcess){$testProcess.Id}else{$null});user_profile_unchanged=($before -eq $after);test_port=$Port;test_process_stopped=$(if($testProcess){$null -eq (Get-Process -Id $testProcess.Id -ErrorAction SilentlyContinue)}else{$true})} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $folder 'isolation_check.json')
    if ($before -ne $after) { throw 'Le profil utilisateur a changé pendant le test ; contrôler isolation_check.json.' }
}
Write-Output ('Résultats : '+$folder)
