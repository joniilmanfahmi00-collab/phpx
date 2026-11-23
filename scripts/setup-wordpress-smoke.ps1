# P0-D: provision a REAL upstream WordPress install for smoke tests (Windows dev).
# Source is never modified (deployment config only). Idempotent.
# Requires: php + mysql reachable + wp-cli (`wp` on PATH or $env:WP_CLI).
# Env: PHPX_WORDPRESS_DIR (default data/smoke/wordpress),
#      PHPX_WP_DB_HOST/PORT/NAME/USER/PASS, PHPX_WP_URL (default http://127.0.0.1:18312).
$ErrorActionPreference = 'Stop'
$Dir = if ($env:PHPX_WORDPRESS_DIR) { $env:PHPX_WORDPRESS_DIR } else { 'data/smoke/wordpress' }
$DbHost = if ($env:PHPX_WP_DB_HOST) { $env:PHPX_WP_DB_HOST } else { '127.0.0.1' }
$DbPort = if ($env:PHPX_WP_DB_PORT) { $env:PHPX_WP_DB_PORT } else { '13306' }
$DbName = if ($env:PHPX_WP_DB_NAME) { $env:PHPX_WP_DB_NAME } else { 'wordpress' }
$DbUser = if ($env:PHPX_WP_DB_USER) { $env:PHPX_WP_DB_USER } else { 'wp' }
$DbPass = if ($env:PHPX_WP_DB_PASS) { $env:PHPX_WP_DB_PASS } else { 'wp' }
$Url = if ($env:PHPX_WP_URL) { $env:PHPX_WP_URL } else { 'http://127.0.0.1:18312' }
$Wp = if ($env:WP_CLI) { $env:WP_CLI } else { 'wp' }
$env:WP_CLI_SUPPRESS_GLOBAL_PARAMS = '1'

function Test-WpInstalled {
  if (-not (Test-Path "$Dir/wp-config.php")) { return $false }
  & $Wp core is-installed --path="$Dir" 2>$null | Out-Null
  return ($LASTEXITCODE -eq 0)
}

function Ensure-CanaryPost {
  # Fixed canary post asserted by p0d_wordpress (idempotent by title).
  $titles = (& $Wp post list --path="$Dir" --post_type=post --fields=post_title --format=csv) 2>$null
  $has = $titles | Where-Object { $_ -match 'Hello PHPX Smoke' }
  if (-not $has) {
    & $Wp post create --path="$Dir" --post_type=post --post_title='Hello PHPX Smoke' --post_name=hello-phpx-smoke --post_content='PHPX smoke canary post.' --post_status=publish | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'canary post creation failed' }
  }
}

function Ensure-AppPassword {
  # Application password for authenticated REST tests (media upload, wc/v3).
  # Stored deployment-side; tests read the file, never the DB.
  if (-not (Test-Path "$Dir/.phpx-app-password")) {
    $rows = (& $Wp user application-password list admin --fields=uuid,name --format=csv --path="$Dir") 2>$null
    $uuid = $rows | Where-Object { $_ -match ',phpx-smoke$' } | ForEach-Object { ($_ -split ',')[0] } | Select-Object -First 1
    if ($uuid) { & $Wp user application-password delete admin $uuid --path="$Dir" >$null 2>&1 }
    & $Wp user application-password create admin phpx-smoke --porcelain --path="$Dir" 2>$null | Set-Content "$Dir/.phpx-app-password"
    $f = Get-Item "$Dir/.phpx-app-password" -ErrorAction SilentlyContinue
    if (($null -eq $f) -or ($f.Length -eq 0)) { Remove-Item "$Dir/.phpx-app-password" -Force -ErrorAction SilentlyContinue }
  }
}

if (Test-WpInstalled) {
  $ver = (& $Wp core version --path="$Dir") 2>$null
  Write-Host "[wp-smoke] already installed at $Dir ($ver)"
  Ensure-AppPassword
  Ensure-CanaryPost
  exit 0
}
if (-not (Get-Command php -ErrorAction SilentlyContinue)) { Write-Error 'php not found in PATH'; exit 2 }
New-Item -ItemType Directory -Force -Path $Dir | Out-Null
if (-not (Test-Path "$Dir/wp-includes/version.php")) {
  Write-Host '[wp-smoke] downloading WordPress...'
  Invoke-WebRequest -Uri 'https://wordpress.org/latest.zip' -OutFile "$env:TEMP/wordpress-latest.zip" -UseBasicParsing
  Expand-Archive -Path "$env:TEMP/wordpress-latest.zip" -DestinationPath "$env:TEMP/wp-smoke-tmp" -Force
  Copy-Item "$env:TEMP/wp-smoke-tmp/wordpress/*" "$Dir/" -Recurse -Force
  Remove-Item -Recurse -Force "$env:TEMP/wp-smoke-tmp", "$env:TEMP/wordpress-latest.zip"
}
if (-not (Test-Path "$Dir/wp-config.php")) {
  & $Wp config create --path="$Dir" --dbname="$DbName" --dbuser="$DbUser" --dbpass="$DbPass" --dbhost="$DbHost`:$DbPort" --skip-check --force
  if ($LASTEXITCODE -ne 0) { exit 1 }
  & $Wp config set WP_ENVIRONMENT_TYPE local --path="$Dir" 2>$null
}
if (-not (Test-WpInstalled)) {
  & $Wp core install --path="$Dir" --url="$Url" --title='PHPX Smoke' --admin_user=admin --admin_password=admin --admin_email=smoke@phpx.test --skip-email
  if ($LASTEXITCODE -ne 0) { exit 1 }
}
& $Wp rewrite structure '/%postname%/' --path="$Dir" --hard 2>$null
if ($LASTEXITCODE -ne 0) { & $Wp rewrite structure '/%postname%/' --path="$Dir" }
# P1-B integration: auto-approve comments so the comment round-trip test needs
# no wp-cli at test time (writes are observed via the REST API only).
foreach ($opt in @('comment_moderation', 'comment_whitelist', 'comment_previously_approved')) {
  & $Wp option update $opt 0 --path="$Dir" >$null 2>&1
}
Ensure-AppPassword
Ensure-CanaryPost
$ver = (& $Wp core version --path="$Dir") 2>$null
Write-Host "[wp-smoke] ready at $Dir ($ver) url=$Url"
