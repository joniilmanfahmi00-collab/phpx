# PHPX P1-B benchmark laboratory - Windows native (padanan scripts/bench-lab.sh).
# Cara pakai: .\scripts\bench-lab.ps1 [-Port 18082] [-Requests 200] [-Concurrency 10]
#   [-Mode one-shot|persistent] [-Profile debug|release] [-Backend phpx|php-builtin]
#   [-Workers 4] [-OutDir data/bench/<auto>]
param(
  [int]$Port = 18082,
  [int]$Requests = 200,
  [int]$Concurrency = 10,
  [string]$Mode = "one-shot",
  [string]$Profile = "debug",
  [string]$Backend = "phpx",
  [int]$Workers = 4,
  [string]$OutDir = ""
)
$ErrorActionPreference = "Stop"
if ($Mode -notin @("one-shot", "persistent")) { throw "Mode must be one-shot or persistent" }
if ($Profile -notin @("debug", "release")) { throw "Profile must be debug or release" }
if ($Backend -notin @("phpx", "php-builtin")) { throw "Backend must be phpx or php-builtin" }

$RepoRoot = Split-Path $PSScriptRoot -Parent
if ($OutDir -eq "") { $OutDir = "data/bench/" + (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ") + "-$Backend-$Mode-$Profile" }
# Jobs run with $HOME as cwd, so OutDir must be absolute.
if (-not [IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path $RepoRoot $OutDir }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
if (-not (Get-Command php -ErrorAction SilentlyContinue)) { throw "php not found in PATH" }
if (-not (Get-Command curl.exe -ErrorAction SilentlyContinue)) { throw "curl.exe not found" }

if ($Backend -eq "phpx") {
  if ($Profile -eq "release") { $Bin = Join-Path $RepoRoot "target\release\phpx.exe"; cargo build --release -p phpx-cli }
  else { $Bin = Join-Path $RepoRoot "target\debug\phpx.exe"; cargo build -p phpx-cli }
  if ($LASTEXITCODE -ne 0) { throw "cargo build gagal" }
  $RootFs = (Join-Path $RepoRoot "examples\vanilla-php").Replace("\", "/")
  $Cfg = Join-Path $OutDir "phpx-bench.toml"
  @"
schema_version = 1
[server]
host = "127.0.0.1"
port = $Port
tls = false
request_body_limit = "16MB"
[application]
root = "$RootFs"
document_root = "public"
entrypoint = "index.php"
[runtime]
mode = "process"
php_version = "8.4"
[runtime.process]
execution_mode = "$Mode"
workers = $Workers
min_workers = 1
max_requests = 1000000
request_timeout_seconds = 30
startup_timeout_seconds = 15
memory_limit = "256M"
memory_soft_limit = "0"
memory_hard_limit = "0"
memory_sample_interval_seconds = 5
queue_wait_timeout_seconds = 5
shutdown_drain_timeout_seconds = 10
restart_max_retries = 5
restart_backoff_ms = 250
restart_backoff_max_ms = 30000
[runtime.embedded]
enabled = false
"@ | Set-Content $Cfg
  $Srv = Start-Process -FilePath $Bin -ArgumentList "serve", "--config", $Cfg -RedirectStandardOutput (Join-Path $OutDir "gateway.log") -RedirectStandardError (Join-Path $OutDir "gateway.err.log") -PassThru
} else {
  $Bin = "php-builtin"
  $DocRoot = Join-Path $RepoRoot "examples\vanilla-php\public"
  $Srv = Start-Process -FilePath "php" -ArgumentList "-S", "127.0.0.1:$Port", "-t", $DocRoot -RedirectStandardOutput (Join-Path $OutDir "php-builtin.log") -RedirectStandardError (Join-Path $OutDir "php-builtin.err.log") -PassThru
}
try {
  for ($i = 0; $i -lt 60; $i++) {
    try { Invoke-WebRequest -Uri "http://127.0.0.1:$Port/healthz" -UseBasicParsing -TimeoutSec 2 | Out-Null; break } catch { Start-Sleep -Milliseconds 500 }
  }
  if ($Backend -eq "php-builtin") { Start-Sleep -Seconds 1 }
  # Fail loudly when the backend never came up (in-run flakes are recorded, not fatal).
  $Probe = if ($Backend -eq "phpx") { "http://127.0.0.1:$Port/healthz" } else { "http://127.0.0.1:$Port/" }
  try { Invoke-WebRequest -Uri $Probe -UseBasicParsing -TimeoutSec 5 | Out-Null }
  catch { throw "bench-lab: backend failed to start on :$Port (see $Log)" }

  # Gateway RSS sampler (max WorkingSet64 over the run).
  $RssLog = Join-Path $OutDir "rss.log"
  $RssJob = $null
  [long]$RssStart = 0
  if ($Backend -eq "phpx") {
    $RssStart = (Get-Process -Id $Srv.Id -ErrorAction SilentlyContinue).WorkingSet64
    if (-not $RssStart) { $RssStart = 0 }
    $RssJob = Start-Job -ScriptBlock {
      param($Pid_, $Log_)
      while (Get-Process -Id $Pid_ -ErrorAction SilentlyContinue) {
        (Get-Process -Id $Pid_ -ErrorAction SilentlyContinue).WorkingSet64 | Out-File -Append $Log_
        Start-Sleep -Seconds 1
      }
    } -ArgumentList $Srv.Id, $RssLog
  }

  # Cold start: first request after healthz.
  $ColdRaw = curl.exe -s -o NUL -w "%{time_total}" "http://127.0.0.1:$Port/"
  [int]$ColdMs = [int][Math]::Round([double]$ColdRaw * 1000)
  Write-Host "[bench-lab] cold_ms=$ColdMs"

  # Warm path: CONC jobs, each a sequential curl loop over its share.
  $Td = Join-Path $OutDir "times.d"
  New-Item -ItemType Directory -Force -Path $Td | Out-Null
  $PerJob = [Math]::Ceiling($Requests / $Concurrency)
  $Sw = [System.Diagnostics.Stopwatch]::StartNew()
  $Jobs = 0..($Concurrency - 1) | ForEach-Object {
    $j = $_
    Start-Job -ScriptBlock {
      param($Base_, $Dir_, $JobIdx_, $PerJob_, $Total_)
      for ($k = 0; $k -lt $PerJob_; $k++) {
        $n = $JobIdx_ * $PerJob_ + $k + 1
        if ($n -gt $Total_) { break }
        $t = curl.exe -s -o NUL -w "%{time_total}" "$Base_/" 2>$null
        if ($LASTEXITCODE -eq 0 -and $t -match '^[0-9.]+$') { $t | Out-File (Join-Path $Dir_ "$n") } else { "FAIL" | Out-File (Join-Path $Dir_ "$n") }
      }
    } -ArgumentList "http://127.0.0.1:$Port", $Td, $j, $PerJob, $Requests
  }
  $Jobs | Wait-Job | Out-Null
  $Jobs | Remove-Job
  $Sw.Stop()
  [double]$ElapsedS = $Sw.Elapsed.TotalSeconds
  if ($ElapsedS -le 0) { $ElapsedS = 0.001 }

  if ($RssJob) { Stop-Job $RssJob -ErrorAction SilentlyContinue; Remove-Job $RssJob -Force -ErrorAction SilentlyContinue }
  [long]$RssEnd = 0
  [long]$RssMax = 0
  if ($Backend -eq "phpx") {
    $RssEnd = (Get-Process -Id $Srv.Id -ErrorAction SilentlyContinue).WorkingSet64
    if (-not $RssEnd) { $RssEnd = 0 }
    if (Test-Path $RssLog) { $RssMax = (Get-Content $RssLog | Measure-Object -Maximum).Maximum }
  }

  $Times = Get-ChildItem $Td | ForEach-Object { Get-Content $_.FullName } | Where-Object { $_ -match '^[0-9.]+$' } | ForEach-Object { [double]$_ } | Sort-Object
  $Fails = (Get-ChildItem $Td | Where-Object { (Get-Content $_.FullName) -notmatch '^[0-9.]+$' } | Measure-Object).Count
  $Warm = @{ n = 0 }
  if ($Times.Count -gt 0) {
    $N = $Times.Count
    $qi = { param($p) $Times[[Math]::Max(0, [Math]::Min($N - 1, [int][Math]::Ceiling($p * $N) - 1))] }
    $Warm = @{
      n      = $N
      mean_ms = [Math]::Round(($Times | Measure-Object -Average).Average * 1000, 3)
      p50_ms = [Math]::Round((& $qi 0.50) * 1000, 3)
      p95_ms = [Math]::Round((& $qi 0.95) * 1000, 3)
      p99_ms = [Math]::Round((& $qi 0.99) * 1000, 3)
      max_ms = [Math]::Round($Times[-1] * 1000, 3)
    }
    $Times -join "`n" | Set-Content (Join-Path $OutDir "times.txt")
  }
  Remove-Item -Recurse -Force $Td
  $Rps = [Math]::Round($Requests / $ElapsedS, 2)
  $PhpV = (php -r 'echo PHP_VERSION;')
  $RustV = (rustc --version 2>$null); if (-not $RustV) { $RustV = "unknown" }
  $PhpxV = if ($Backend -eq "phpx") { (& $Bin version 2>$null | Select-Object -First 1) } else { "n/a" }
  if (-not $PhpxV) { $PhpxV = "unknown" }
  $Summary = [ordered]@{
    schema = "phpx-bench-lab/1"; date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    os = "$([System.Environment]::OSVersion.VersionString) $env:PROCESSOR_ARCHITECTURE"
    php_version = $PhpV; rustc = "$RustV"; phpx_version = "$PhpxV"
    backend = $Backend; execution_mode = $(if ($Backend -eq "phpx") { $Mode } else { "n/a" }); profile = $Profile; workers = $Workers
    requests = $Requests; concurrency = $Concurrency; failures = $Fails
    cold_ms = $ColdMs; elapsed_s = [Math]::Round($ElapsedS, 3); rps = $Rps
    warm = $Warm; rss_kb = @{ start = [int]($RssStart / 1KB); max = [int]($RssMax / 1KB); end = [int]($RssEnd / 1KB) }
  }
  $Summary | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $OutDir "summary.json")
  $Summary | ConvertTo-Json -Depth 5 | Write-Output
  Write-Host "[bench-lab] failures=$Fails rps=$Rps results=$OutDir/summary.json (NOT an SLA)"
  if ($Backend -eq "phpx") {
    try { Invoke-WebRequest -Uri "http://127.0.0.1:$Port/metrics" -UseBasicParsing -TimeoutSec 5 | Select-Object -ExpandProperty Content | Set-Content (Join-Path $OutDir "metrics.txt") } catch {}
  }
} finally {
  Stop-Process -Id $Srv.Id -Force -ErrorAction SilentlyContinue
}
