# PHPX bench repeat harness - Windows native (padanan scripts/bench-repeat.sh).
# Cara pakai: .\scripts\bench-repeat.ps1 [-Runs 3] [-Requests 200] [-Concurrency 10]
#   [-Mode persistent] [-Profile release] [-Backend phpx] [-Workers 4] [-Port 18082]
#   [-OutDir data/bench/<auto>] [-MaxFailures 0] [-MaxCvPct 30]
# CV gate hanya rps + warm p50/p95 (cold_ms spawn noise, dilaporkan tanpa gate).
param(
  [int]$Runs = 3,
  [int]$Requests = 200,
  [int]$Concurrency = 10,
  [string]$Mode = "persistent",
  [string]$Profile = "release",
  [string]$Backend = "phpx",
  [int]$Workers = 4,
  [int]$Port = 18082,
  [string]$OutDir = "",
  [int]$MaxFailures = 0,
  [double]$MaxCvPct = 30
)
$ErrorActionPreference = "Stop"
if ($Runs -lt 2 -or $Runs -gt 10) { throw "Runs must be 2-10" }
if ($Mode -notin @("one-shot", "persistent")) { throw "Mode must be one-shot or persistent" }
if ($Profile -notin @("debug", "release")) { throw "Profile must be debug or release" }
if ($Backend -notin @("phpx", "php-builtin")) { throw "Backend must be phpx or php-builtin" }
$RepoRoot = Split-Path $PSScriptRoot -Parent
if ($OutDir -eq "") { $OutDir = "data/bench/" + (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ") + "-repeat-$Backend-$Mode-$Profile" + "x$Runs" }
if (-not [IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path $RepoRoot $OutDir }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
for ($i = 1; $i -le $Runs; $i++) {
  Write-Host "[bench-repeat] run $i/$Runs ..."
  & (Join-Path $PSScriptRoot "bench-lab.ps1") -Port $Port -Requests $Requests -Concurrency $Concurrency `
    -Mode $Mode -Profile $Profile -Backend $Backend -Workers $Workers `
    -OutDir (Join-Path $OutDir "run-$i") -MaxFailures $MaxFailures
  if ($LASTEXITCODE -ne 0) { throw "run $i failed (bench-lab exit $LASTEXITCODE)" }
}
$Paths = @(
  @{ Path = "rps"; Name = "rps" },
  @{ Path = "cold_ms"; Name = "cold_ms" },
  @{ Path = "warm.mean_ms"; Name = "warm_mean_ms" },
  @{ Path = "warm.p50_ms"; Name = "warm_p50_ms" },
  @{ Path = "warm.p95_ms"; Name = "warm_p95_ms" },
  @{ Path = "warm.p99_ms"; Name = "warm_p99_ms" },
  @{ Path = "warm.max_ms"; Name = "warm_max_ms" }
)
function Get-Metric($Summary, $Path) {
  $v = $Summary
  foreach ($k in ($Path -split '\.')) {
    if ($null -eq $v) { return $null }
    $v = $v.$k
  }
  return $v
}
$Metrics = @{}
foreach ($m in $Paths) {
  $Vals = @()
  for ($i = 1; $i -le $Runs; $i++) {
    $s = Get-Content (Join-Path $OutDir "run-$i/summary.json") -Raw | ConvertFrom-Json
    $v = Get-Metric $s $m.Path
    if ($null -eq $v) { throw "missing $($m.Path) in run-$i/summary.json" }
    $Vals += [double]$v
  }
  $Mean = ($Vals | Measure-Object -Average).Average
  $Var = ($Vals | ForEach-Object { ($_ - $Mean) * ($_ - $Mean) } | Measure-Object -Average).Average
  $Sd = [Math]::Sqrt($Var)
  $Cv = if ($Mean -ne 0) { [Math]::Round($Sd / [Math]::Abs($Mean) * 100, 2) } else { 0 }
  $Metrics[$m.Name] = [ordered]@{
    n = $Runs; values = $Vals
    mean = [Math]::Round($Mean, 3)
    min = ($Vals | Measure-Object -Minimum).Minimum
    max = ($Vals | Measure-Object -Maximum).Maximum
    stdev = [Math]::Round($Sd, 3); cv_pct = $Cv
  }
  Write-Host ("{0,-12} mean={1,-10} min={2,-10} max={3,-10} cv={4}%" -f $m.Name, $Metrics[$m.Name].mean, $Metrics[$m.Name].min, $Metrics[$m.Name].max, $Cv)
}
$Gated = @("warm_p50_ms", "warm_p95_ms", "rps")
$Viol = @()
foreach ($g in $Gated) {
  if ($Metrics[$g].cv_pct -gt $MaxCvPct) { $Viol += "$g cv=$($Metrics[$g].cv_pct)% exceeds -MaxCvPct $MaxCvPct" }
}
$Rep = [ordered]@{
  schema = "phpx-bench-repeat/1"; runs = $Runs; metrics = $Metrics
  gates = [ordered]@{ max_cv_pct = $MaxCvPct; gated_metrics = $Gated; pass = ($Viol.Count -eq 0); note = "cold_ms reported without CV gate (spawn noise by design)" }
}
$Rep | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $OutDir "reproducibility.json")
if ($Viol.Count -gt 0) { throw ("[bench-repeat] REPRODUCIBILITY VIOLATION: " + ($Viol -join "; ")) }
Write-Host "reproducibility: PASS (max CV $MaxCvPct% on rps/p50/p95)"
Write-Host "[bench-repeat] results=$OutDir/reproducibility.json (NOT an SLA)"
