# PHPX RC compare skeleton - Windows (NOT SLA): phpx vs php-builtin, workload sami.
# Cara pakai: .\scripts\bench-compare-windows.ps1 [-Requests 200] [-Concurrency 10] [-Port 18082]
param([int]$Requests=200,[int]$Concurrency=10,[int]$Port=18082)
$ErrorActionPreference="Stop"
$RepoRoot=Split-Path $PSScriptRoot -Parent
$Bin=Join-Path $RepoRoot "target\debug\phpx.exe"
$RootFs=Join-Path $RepoRoot "examples\vanilla-php\public"
Write-Host "[compare] baseline lock: $Requests req, conc $Concurrency, fixture vanilla-php/public"
try { $v=& $Bin --version 2>$null } catch { $v="unknown" }
try { $phpv=php -r "echo PHP_VERSION;" 2>$null } catch { $phpv="unknown" }
Write-Host "[compare] phpx=$v php=$phpv"
Write-Host "[compare] 1) phpx..."
& (Join-Path $RepoRoot "scripts\bench-windows.ps1") -Requests $Requests -Concurrency $Concurrency -Port $Port
Write-Host "[compare] 2) php-builtin (php -S, same docroot)..."
$pbPort=$Port+10
$pb=Start-Process -FilePath "php" -ArgumentList @("-S","127.0.0.1:$pbPort","-t",$RootFs) -NoNewWindow -PassThru
try {
  Start-Sleep -Seconds 1
  $sw=[Diagnostics.Stopwatch]::StartNew()
  if ($PSVersionTable.PSVersion.Major -ge 7) {
    1..$Requests | ForEach-Object -Parallel { curl.exe -fsS "http://127.0.0.1:$using:pbPort/" 2>$null | Out-Null } -ThrottleLimit $Concurrency
  } else { foreach ($i in 1..$Requests) { curl.exe -fsS "http://127.0.0.1:$pbPort/" 2>$null | Out-Null } }
  $sw.Stop(); $el=[int]$sw.Elapsed.TotalSeconds; if ($el -le 0) { $el=1 }
  Write-Host ("[compare] php-builtin: $Requests req in {0}s (~{1} req/s, mesin dev saja)" -f $el, [int]($Requests/$el))
} finally { if ($pb -and -not $pb.HasExited) { Stop-Process -Id $pb.Id -Force -ErrorAction SilentlyContinue } }
Write-Host "[compare] done. Klaim = delta vs baseline, bukan SLA. php-fpm/franken/rr: ganti langkah 2 dgn backend sama."
