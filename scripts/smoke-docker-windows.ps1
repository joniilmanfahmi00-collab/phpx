# PHPX M3 smoke test Docker - Windows native (padanan scripts/smoke-docker.sh).
# Cara pakai: .\scripts\smoke-docker-windows.ps1 [-Port 18081] [-Image "phpx:0.1.0-dev"]
param(
  [int]$Port = $(if ($env:PHPX_DOCKER_PORT) { [int]$env:PHPX_DOCKER_PORT } else { 18081 }),
  [string]$Image = $(if ($env:PHPX_IMAGE) { $env:PHPX_IMAGE } else { "phpx:0.1.0-dev" })
)
$ErrorActionPreference = "Stop"

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
  Write-Host "Aa, docker teu kapendak di komputer Aa. Install heula Docker Desktop for Windows,"
  Write-Host "terus jalankeun deui skrip ieu. Neng antosan nya!"
  exit 2
}

$Name = "phpx-smoke-$Port"
$RepoRoot = Split-Path $PSScriptRoot -Parent
Push-Location $RepoRoot
try {
  Write-Host "[docker-smoke] building $Image..."
  docker build -t $Image .
  if ($LASTEXITCODE -ne 0) { throw "docker build gagal" }

  Write-Host "[docker-smoke] non-root check..."
  $who = docker run --rm $Image whoami
  if ($who.Trim() -ne "phpx") { throw "expected user phpx, got: $who" }

  Write-Host "[docker-smoke] running detached on $Port..."
  docker run -d --rm --name $Name -p "$Port`:8080" $Image | Out-Null

  try {
    $ok = $false
    for ($i = 1; $i -le 30; $i++) {
      try {
        curl.exe -fsS "http://127.0.0.1:$Port/healthz" 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) { $ok = $true; break }
      } catch {}
      Start-Sleep -Seconds 1
    }
    if (-not $ok) { docker logs $Name | Select-Object -Last 20; throw "gateway docker teu naek" }

    curl.exe -fsS "http://127.0.0.1:$Port/healthz" | Select-String -Quiet "ok" | Out-Null
    if ((curl.exe -fsS "http://127.0.0.1:$Port/healthz") -notmatch "ok") { throw "healthz gagal" }
    if ((curl.exe -fsS "http://127.0.0.1:$Port/readyz") -notmatch "ready") { throw "readyz gagal" }
    curl.exe -fsS "http://127.0.0.1:$Port/" | Out-Null
    curl.exe -fsS "http://127.0.0.1:$Port/style.css" | Out-Null
    $code = curl.exe --path-as-is -o NUL -s -w "%{http_code}" "http://127.0.0.1:$Port/%2e%2e/x"
    if ($code.Trim() -ne "400") {
      docker logs $Name | Select-Object -Last 20
      throw "FAIL: traversal expected 400 got $code"
    }

    Write-Host "[docker-smoke] ALL GREEN ($Image as non-root)"
  }
  finally {
    docker stop $Name 2>$null | Out-Null
  }
}
finally {
  Pop-Location
}
