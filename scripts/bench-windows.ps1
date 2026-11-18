# PHPX M3 baseline benchmark - Windows native (padanan scripts/bench.sh).
# Sane benchmarking klaim SLA, mung baseline mesin Aa wungkul.
# Cara pakai: .\scripts\bench-windows.ps1 [-Requests 200] [-Concurrency 10] [-Port 18082]
param(
  [int]$Requests = 200,
  [int]$Concurrency = 10,
  [int]$Port = $(if ($env:PHPX_BENCH_PORT) { [int]$env:PHPX_BENCH_PORT } else { 18082 }),
  [string]$Bin = $(if ($env:PHPX_BIN) { $env:PHPX_BIN } else { "" })
)
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path $PSScriptRoot -Parent
if (-not $Bin -or $Bin -eq "") { $Bin = Join-Path $RepoRoot "target\debug\phpx.exe" }
$RootFs = (Join-Path $RepoRoot "examples\vanilla-php").Replace("\", "/")
$Cfg = Join-Path $env:TEMP "phpx-bench.toml"
$Log = Join-Path $env:TEMP "phpx-bench.log"

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
max_requests = 1000
request_timeout_seconds = 30
startup_timeout_seconds = 10
memory_limit = "256M"
[runtime.embedded]
enabled = false
"@ | Set-Content -Path $Cfg -Encoding utf8

$env:RUST_LOG = "warn"
$proc = Start-Process -FilePath $Bin -ArgumentList @("serve", "--config", $Cfg) -RedirectStandardOutput $Log -RedirectStandardError "$Log.err" -NoNewWindow -PassThru
try {
  $ok = $false
  for ($i = 1; $i -le 30; $i++) {
    try {
      curl.exe -fsS "http://127.0.0.1:$Port/healthz" 2>$null | Out-Null
      if ($LASTEXITCODE -eq 0) { $ok = $true; break }
    } catch {}
    Start-Sleep -Milliseconds 500
  }
  if (-not $ok) { throw "gateway teu naek, cek $Log" }

  $csw=[Diagnostics.Stopwatch]::StartNew()
  try { curl.exe -fsS "http://127.0.0.1:$Port/" 2>$null | Out-Null; $coldMs=[int]$csw.Elapsed.TotalMilliseconds } catch { $coldMs=0 }
  Write-Host ("[bench] cold_ms={0} (request 200 pertama, termasuk spawn PHP pertama)" -f $coldMs)
  Write-Host "[bench] M3 baseline: $Requests requests, concurrency $Concurrency, vanilla / (one-shot PHP, workers=4)"
  try { $phpv = php -r "echo PHP_VERSION;" 2>$null } catch { $phpv = "unknown" }
  try { $rustv = rustc --version 2>$null } catch { $rustv = "unknown" }
  Write-Host ("[bench] date={0} php={1} rust={2}" -f ([DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")), $phpv, $rustv)

  if (Get-Command wrk -ErrorAction SilentlyContinue) {
    wrk -t"$Concurrency" -c"$Concurrency" -d15s "http://127.0.0.1:$Port/"
  } else {
    Write-Host "[bench] wrk teu kapendak, pake curl loop (timing kasar)..."
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    if ($PSVersionTable.PSVersion.Major -ge 7) {
      1..$Requests | ForEach-Object -Parallel {
        curl.exe -fsS "http://127.0.0.1:$using:Port/" 2>$null | Out-Null
      } -ThrottleLimit $Concurrency
    } else {
      Write-Host "[bench] PS 5.1 terdeteksi, fallback sequential..."
      foreach ($i in 1..$Requests) {
        curl.exe -fsS "http://127.0.0.1:$Port/" 2>$null | Out-Null
      }
    }
    $sw.Stop()
    $elapsed = [int]$sw.Elapsed.TotalSeconds
    if ($elapsed -le 0) { $elapsed = 1 }
    $rps = [int]($Requests / $elapsed)
    Write-Host "[bench] completed $Requests requests in ${elapsed}s (~$rps req/s, developer machine baseline only)"
  }
  Write-Host "[bench] done. See $Log for access logs. Do not use as SLA."
}
finally {
  if ($proc -and -not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
  }
}
