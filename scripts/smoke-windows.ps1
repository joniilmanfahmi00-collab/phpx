# PHPX M3 smoke test - Windows native (padanan scripts/smoke-linux.sh).
# Wilujeng! Cara pakai dari root repo: .\scripts\smoke-windows.ps1 [-Port 18080]
# Fail-fast, sami sareng versi Linux. Reproducible, sanes benchmark.
param(
  [int]$Port = $(if ($env:PHPX_SMOKE_PORT) { [int]$env:PHPX_SMOKE_PORT } else { 18080 }),
  [string]$Bin = $(if ($env:PHPX_BIN) { $env:PHPX_BIN } else { "" })
)
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path $PSScriptRoot -Parent
if (-not $Bin -or $Bin -eq "") { $Bin = Join-Path $RepoRoot "target\debug\phpx.exe" }
$RootFs = (Join-Path $RepoRoot "examples\vanilla-php").Replace("\", "/")
$Cfg = Join-Path $env:TEMP "phpx-smoke.toml"
$Log = Join-Path $env:TEMP "phpx-smoke.log"

Write-Host "[smoke] building phpx (debug)..."
cargo build -p phpx-cli -q
if ($LASTEXITCODE -ne 0) { throw "cargo build gagal" }

Write-Host "[smoke] writing $Cfg (root=$RootFs)"
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
request_timeout_seconds = 10
startup_timeout_seconds = 10
memory_limit = "256M"
[runtime.embedded]
enabled = false
"@ | Set-Content -Path $Cfg -Encoding utf8

Write-Host "[smoke] starting gateway on 127.0.0.1:$Port..."
$env:RUST_LOG = "info"
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

  Write-Host "[smoke] healthz..."
  curl.exe -fsS -i "http://127.0.0.1:$Port/healthz" | Select-Object -First 5
  if ($LASTEXITCODE -ne 0) { throw "healthz gagal" }

  Write-Host "[smoke] readyz..."
  $ready = curl.exe -fsS "http://127.0.0.1:$Port/readyz"
  if ($ready -notmatch "ready") { throw "readyz gagal: $ready" }

  Write-Host "[smoke] front controller / ..."
  $homeBody = curl.exe -fsS "http://127.0.0.1:$Port/"
  if ($homeBody -notmatch "(?i)phpx|vanilla|hello|index") { throw "body / teu cocok: $homeBody" }

  Write-Host "[smoke] static /style.css ..."
  curl.exe -fsS "http://127.0.0.1:$Port/style.css" | Select-Object -First 3

  Write-Host "[smoke] php dispatch /hello.php (no source leak) ..."
  $helloBody = curl.exe -fsS "http://127.0.0.1:$Port/hello.php"
  $helloBody | Select-Object -First 3
  if ($helloBody -match "<\?php") { throw "FAIL: php source leaked" }

  Write-Host "[smoke] traversal blocked ..."
  $code = curl.exe --path-as-is -o NUL -s -w "%{http_code}" "http://127.0.0.1:$Port/%2e%2e/x"
  if ($code.Trim() -ne "400") { throw "FAIL: traversal expected 400 got $code" }

  Write-Host "[smoke] crash isolation (/crash.php -> 500, then / -> 200) ..."
  $code = curl.exe --path-as-is -o NUL -s -w "%{http_code}" "http://127.0.0.1:$Port/crash.php"
  if ($code.Trim() -ne "500") { throw "FAIL: crash expected 500 got $code" }
  curl.exe -fsS "http://127.0.0.1:$Port/" | Out-Null

  Write-Host "[smoke] CLI contract ..."
  & $Bin doctor | Out-Null
  if (((& $Bin version) -join "`n") -notmatch "developer preview") { throw "version contract gagal" }
  if (((& $Bin runtime list) -join "`n") -notmatch "process") { throw "runtime list contract gagal" }
  if (((& $Bin workers status) -join "`n") -notmatch "bounded pool") { throw "workers status contract gagal" }
  if (((& $Bin metrics) -join "`n") -notmatch "healthz") { throw "metrics contract gagal" }
  & $Bin queue status | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "FAIL: queue status should exit 0 (M5 phase 1 real)" }
  if (((& $Bin queue status) -join "`n") -notmatch "driver=") { throw "queue status contract gagal" }
  & $Bin scheduler run | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "FAIL: scheduler run should exit 0 (M5 phase 2 real)" }
  if (((& $Bin scheduler run) -join "`n") -notmatch "(?i)scheduler") { throw "scheduler run contract gagal" }
  if (((& $Bin --json scheduler run) -join "`n") -notmatch "dispatched") { throw "scheduler json contract gagal" }
  if (((& $Bin cache status) -join "`n") -notmatch "cache") { throw "cache status contract gagal" }
  if (((& $Bin --json cache status) -join "`n") -notmatch "entries") { throw "cache json contract gagal" }
  if (((& $Bin storage status) -join "`n") -notmatch "storage") { throw "storage status contract gagal" }
  if (((& $Bin --json storage status) -join "`n") -notmatch "objects") { throw "storage json contract gagal" }

  Write-Host "[smoke] ALL GREEN (M3+M5 developer preview: gateway + queue + scheduler + cache + storage)"
}
finally {
  if ($proc -and -not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
  }
}
