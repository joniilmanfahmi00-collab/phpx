# PHPX RC soak-long - Windows (NOT SLA): default 60 menit, sample RSS tiap 60 detik.
# Cara pakai: .\scripts\soak-long-windows.ps1 [-Minutes 60] [-Concurrency 8] [-Port 18081] [-Mode one-shot|persistent] [-Profile debug|release] [-OutDir data/bench/<auto>]
param([int]$Minutes=60,[int]$Concurrency=8,[int]$Port=18081,[string]$Mode="one-shot",[string]$Profile="debug",[string]$OutDir="")
$ErrorActionPreference="Stop"
if ($Mode -ne "one-shot" -and $Mode -ne "persistent") { throw "Mode must be one-shot or persistent" }
if ($Profile -ne "debug" -and $Profile -ne "release") { throw "Profile must be debug or release" }
$RepoRoot=Split-Path $PSScriptRoot -Parent
if ($OutDir -eq "") { $OutDir = $env:TEMP }
if (-not [IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path $RepoRoot $OutDir }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
if ($Profile -eq "release") { $Bin=Join-Path $RepoRoot "target\release\phpx.exe" } else { $Bin=Join-Path $RepoRoot "target\debug\phpx.exe" }
$RootFs=(Join-Path $RepoRoot "examples\vanilla-php").Replace("\","/")
$Cfg=Join-Path $OutDir "phpx-soaklong.toml"
$Log=Join-Path $OutDir "phpx-soaklong.log"
$Csv=Join-Path $OutDir "phpx-soaklong-rss.csv"
if ($Profile -eq "release") { cargo build --release -p phpx-cli -q } else { cargo build -p phpx-cli -q }
if ($LASTEXITCODE -ne 0) { throw "cargo build gagal" }
@"
schema_version = 1
[server]
host = "127.0.0.1"
port = $Port
tls = false
[application]
root = "$RootFs"
document_root = "public"
entrypoint = "index.php"
[runtime]
mode = "process"
php_version = "8.4"
[runtime.process]
execution_mode = "$Mode"
workers = 4
min_workers = 1
max_requests = 500
request_timeout_seconds = 10
startup_timeout_seconds = 10
memory_limit = "256M"
memory_soft_limit = "0"
memory_hard_limit = "0"
memory_sample_interval_seconds = 5
queue_wait_timeout_seconds = 2
shutdown_drain_timeout_seconds = 10
restart_max_retries = 5
restart_backoff_ms = 250
restart_backoff_max_ms = 30000
[runtime.embedded]
enabled = false
"@ | Set-Content -Path $Cfg -Encoding utf8
"t,rss_kb" | Set-Content -Path $Csv -Encoding utf8
$env:RUST_LOG="warn"
$proc=Start-Process -FilePath $Bin -ArgumentList @("serve","--config",$Cfg) -RedirectStandardOutput $Log -NoNewWindow -PassThru
try {
  for ($i=1; $i -le 30; $i++) { try { curl.exe -fsS "http://127.0.0.1:$Port/healthz" 2>$null | Out-Null; break } catch {}; Start-Sleep -Milliseconds 500 }
  $end=(Get-Date).AddMinutes($Minutes)
  while ((Get-Date) -lt $end) {
    if ($PSVersionTable.PSVersion.Major -ge 7) { 1..200 | ForEach-Object -Parallel { curl.exe -fsS "http://127.0.0.1:$using:Port/" 2>$null | Out-Null } -ThrottleLimit $Concurrency } else { foreach ($n in 1..200) { curl.exe -fsS "http://127.0.0.1:$Port/" 2>$null | Out-Null } }
    try { $rss=(Get-Process -Id $proc.Id -ErrorAction Stop).WorkingSet64/1KB } catch { $rss=0 }
    ("{0},{1:N0}" -f ([DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")),$rss) | Add-Content -Path $Csv -Encoding utf8
    Start-Sleep -Seconds 50
  }
  try { curl.exe -fsS "http://127.0.0.1:$Port/metrics" | Set-Content -Path (Join-Path $OutDir "phpx-soaklong.out.metrics") -Encoding utf8 } catch {}
  Write-Host "[soak-long] out=$OutDir mode=$Mode profile=$Profile"
  Get-Content $Csv | Select-Object -Last 3
  Write-Host "[soak-long] slope=(rss_akhir-rss_awal)/menit, gateway harus flat."
} finally { if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue } }
