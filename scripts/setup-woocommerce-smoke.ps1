# P0-D: provision WooCommerce (real upstream plugin) on the scripted WordPress install (Windows dev).
# Unmodified app/plugin source; deployment config + catalog content only. Idempotent.
# Requires: provisioned WordPress (scripts/setup-wordpress-smoke.ps1) + wp-cli.
$ErrorActionPreference = 'Stop'
$Dir = if ($env:PHPX_WORDPRESS_DIR) { $env:PHPX_WORDPRESS_DIR } else { 'data/smoke/wordpress' }
$Wp = if ($env:WP_CLI) { $env:WP_CLI } else { 'wp' }
$env:WP_CLI_SUPPRESS_GLOBAL_PARAMS = '1'

if (-not (Test-Path "$Dir/wp-config.php")) { Write-Error "no WordPress install at $Dir (run setup-wordpress-smoke.ps1 first)"; exit 2 }
if (-not (Get-Command $Wp -ErrorAction SilentlyContinue) -and $Wp -eq 'wp') { Write-Error 'wp-cli not found (set WP_CLI)'; exit 2 }
$active = (& $Wp plugin is-active woocommerce --path="$Dir") 2>$null
if ($LASTEXITCODE -ne 0) { & $Wp plugin install woocommerce --activate --path="$Dir"; if ($LASTEXITCODE -ne 0) { exit 1 } }
(& $Wp post get 12 --path="$Dir") 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) {
  & $Wp post create --path="$Dir" --post_type=product --post_title='Smoke Pixel' --post_status=publish --porcelain | Out-Null
  if ($LASTEXITCODE -ne 0) { exit 1 }
}
$Id = ((& $Wp post list --path="$Dir" --post_type=product --fields=ID --format=ids) -split '\s+' | Select-Object -First 1)
& $Wp post meta set $Id _price 25 --path="$Dir" | Out-Null
& $Wp post meta set $Id _regular_price 25 --path="$Dir" | Out-Null
& $Wp post meta set $Id _stock_status instock --path="$Dir" | Out-Null
& $Wp post meta set $Id _stock 100 --path="$Dir" | Out-Null
& $Wp post meta set $Id _visibility visible --path="$Dir" >$null 2>&1
$ver = (& $Wp plugin get woocommerce --field=version --path="$Dir") 2>$null
Write-Host "[wc-smoke] ready at $Dir ($ver, product id=$Id)"
