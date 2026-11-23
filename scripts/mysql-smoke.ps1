# Isolated MySQL for WordPress smoke tests (local dev only; CI uses a service).
# Usage: scripts/mysql-smoke.ps1 [start|stop]
# Env: PHPX_MYSQLD (mysqld binary, default `mysqld` on PATH),
#      PHPX_MYSQL_DATADIR (default data/smoke/mysql, resolved absolute),
#      PHPX_WP_DB_PORT (default 13306), PHPX_WP_DB_NAME/USER/PASS.
$ErrorActionPreference = 'Stop'
$Mysqld = if ($env:PHPX_MYSQLD) { $env:PHPX_MYSQLD } else { 'mysqld' }
$DataDirRel = if ($env:PHPX_MYSQL_DATADIR) { $env:PHPX_MYSQL_DATADIR } else { 'data/smoke/mysql' }
$DataDir = [IO.Path]::GetFullPath((Join-Path (Get-Location) $DataDirRel))
$Port = if ($env:PHPX_WP_DB_PORT) { $env:PHPX_WP_DB_PORT } else { '13306' }
$Db = if ($env:PHPX_WP_DB_NAME) { $env:PHPX_WP_DB_NAME } else { 'wordpress' }
$User = if ($env:PHPX_WP_DB_USER) { $env:PHPX_WP_DB_USER } else { 'wp' }
$Pass = if ($env:PHPX_WP_DB_PASS) { $env:PHPX_WP_DB_PASS } else { 'wp' }
$PidFile = "$DataDir/phpx-mysqld.pid"
$cmd = if ($args.Count -ge 1) { $args[0] } else { 'start' }

if ($cmd -eq 'stop') {
  if (Test-Path $PidFile) {
    $pid = Get-Content $PidFile
    Stop-Process -Id $pid -Force -ErrorAction SilentlyContinue
    Remove-Item $PidFile -Force
    Write-Host '[mysql-smoke] stopped'
  } else {
    Write-Host '[mysql-smoke] not running (no pid file)'
  }
  exit 0
}

$up = (& mysqladmin --protocol=TCP -h 127.0.0.1 -P $Port -u root ping) 2>$null
if ($LASTEXITCODE -eq 0) {
  Write-Host "[mysql-smoke] already up on 127.0.0.1:$Port"
} else {
  if (-not (Get-Command $Mysqld -ErrorAction SilentlyContinue)) { Write-Error "mysqld not found (set PHPX_MYSQLD)"; exit 2 }
  if (-not (Test-Path "$DataDir/auto.cnf")) {
    Write-Host "[mysql-smoke] initializing datadir $DataDir..."
    & $Mysqld --initialize-insecure --datadir="$DataDir"
    if ($LASTEXITCODE -ne 0) { exit 1 }
  }
  Write-Host "[mysql-smoke] starting mysqld on 127.0.0.1:$Port..."
  $p = Start-Process -FilePath $Mysqld -ArgumentList "--datadir=`"$DataDir`"", "--port=$Port", '--bind-address=127.0.0.1' -PassThru
  $p.Id | Out-File $PidFile
  for ($i = 0; $i -lt 30; $i++) {
    (& mysqladmin --protocol=TCP -h 127.0.0.1 -P $Port -u root ping) 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { break }
    Start-Sleep -Seconds 1
  }
  (& mysqladmin --protocol=TCP -h 127.0.0.1 -P $Port -u root ping) 2>$null | Out-Null
  if ($LASTEXITCODE -ne 0) { Write-Error 'mysqld failed to start'; exit 1 }
}
& mysql --protocol=TCP -h 127.0.0.1 -P $Port -u root -e "CREATE DATABASE IF NOT EXISTS ``$Db``; CREATE USER IF NOT EXISTS '$User'@'%' IDENTIFIED BY '$Pass'; GRANT ALL PRIVILEGES ON ``$Db``.* TO '$User'@'%'; FLUSH PRIVILEGES;"
if ($LASTEXITCODE -ne 0) { exit 1 }
Write-Host "[mysql-smoke] ready: 127.0.0.1:$Port db=$Db user=$User"
