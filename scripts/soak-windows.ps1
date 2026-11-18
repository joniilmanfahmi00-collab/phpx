# PHPX M4 soak - Windows native (padanan scripts/soak.sh).
# Cara pakai: .\scripts\soak-windows.ps1 [-Port 18081] [-Requests 2000] [-Concurrency 8]
param(
  [int]$Port = 18081,
  [int]$Requests = 2000,
  [int]$Concurrency = 8,
  [string]$Bin = $(if ($env:PHPX_BIN) { $env:PHPX_BIN } else { "" })
)
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path $PSScriptRoot -Parent
if (-not $Bin -or $Bin -eq "") { $Bin = Join-Path $RepoRoot "target\debug\phpx.exe" }
$RootFs = (Join-Path $RepoRoot "examples\vanilla-php").Replace("\", "/")
$Cfg = Join-Path $env:TEMP "phpx-soak.toml"
$Log = Join-Path $env:TEMP "phpx-soak.log"
$OutMetrics = Join-Path $env:TEMP "phpx-soak.out.metrics"

Write-Host "[soak] building phpx (debug)..."
cargo build -p phpx-cli -q
if ($LASTEXITCODE -ne 0) { throw "cargo build gagal" }

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
workers = 4
min_workers = 1
max_requests = 500
request_timeout_seconds = 10
startup_timeout_seconds = 10
memory_limit = "256M"
queue_wait_timeout_seconds = 2
shutdown_drain_timeout_seconds = 10
restart_max_retries = 5
restart_backoff_ms = 250
restart_backoff_max_ms = 30000
[runtime.embedded]
enabled = false
"@ | Set-Content -Path $Cfg -Encoding utf8

Write-Host "[soak] starting gateway on 127.0.0.1:$Port..."
$env:RUST_LOG = "warn"
$proc = Start-Process -FilePath $Bin -ArgumentList @("serve", "--config", $Cfg) -RedirectStandardOutput $Log -RedirectStandardError "$Log.err" -NoNewWindow -PassThru
try {
  Start-Sleep -Milliseconds 1500

  Write-Host "[soak] $Requests requests, concurrency $Concurrency..."
  $start = Get-Date
  if ($PSVersionTable.PSVersion.Major -ge 7) {
    1..$Requests | ForEach-Object -Parallel {
      $p = $using:Port
      $m = $_ % 4
      if ($m -eq 0) { curl.exe -sf -o NUL "http://127.0.0.1:$p/" 2>$null | Out-Null }
      elseif ($m -eq 1) { curl.exe -sf -o NUL "http://127.0.0.1:$p/hello.php" 2>$null | Out-Null }
      elseif ($m -eq 2) { curl.exe -sf -o NUL "http://127.0.0.1:$p/style.css" 2>$null | Out-Null }
      else { curl.exe -sf -o NUL "http://127.0.0.1:$p/healthz" 2>$null | Out-Null }
    } -ThrottleLimit $Concurrency
  } else {
    Write-Host "[soak] PS 5.1 terdeteksi, fallback sequential (pake pwsh 7 kanggo paralel beneran)..."
    foreach ($i in 1..$Requests) {
      $m = $i % 4
      if ($m -eq 0) { curl.exe -sf -o NUL "http://127.0.0.1:$Port/" 2>$null | Out-Null }
      elseif ($m -eq 1) { curl.exe -sf -o NUL "http://127.0.0.1:$Port/hello.php" 2>$null | Out-Null }
      elseif ($m -eq 2) { curl.exe -sf -o NUL "http://127.0.0.1:$Port/style.css" 2>$null | Out-Null }
      else { curl.exe -sf -o NUL "http://127.0.0.1:$Port/healthz" 2>$null | Out-Null }
    }
  }
  $dur = ((Get-Date) - $start).TotalSeconds
  if ($dur -lt 1) { $dur = 1 }
  $rps = [int]($Requests / $dur)

  Write-Host "[soak] scraping /metrics..."
  try {
    $metrics = curl.exe -sf "http://127.0.0.1:$Port/metrics"
  } catch { $metrics = "METRICS_FAILED" }
  $metrics | Set-Content -Path $OutMetrics -Encoding utf8
  $metrics -split "`n" | Select-String "^phpx_pool_"

  Write-Host "--- soak report (M4 fixed-pool baseline, adaptive disabled) ---"
  Write-Host "requests=$Requests concurrency=$Concurrency duration_s=$([int]$dur) rps=$rps"
  try {
    $rss = (Get-Process -Id $proc.Id -ErrorAction Stop).WorkingSet64 / 1KB
    Write-Host ("gateway_rss_kb={0:N0}" -f $rss)
  } catch { Write-Host "gateway_rss_kb=unknown" }
  Write-Host "metrics_saved=$OutMetrics log=$Log"
  Write-Host "[soak] done. Drain test: stopping gateway, cek log kanggo drain complete."
}
finally {
  if ($proc -and -not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
  }
  if (Test-Path $Log) {
    Select-String -Path $Log -Pattern "drain|shutdown" -CaseSensitive:$false | Select-Object -Last 5
  }
}
