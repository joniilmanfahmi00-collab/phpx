#!/usr/bin/env bash
# P0-D: provision WooCommerce (real upstream plugin) on the scripted WordPress
# install. Unmodified app/plugin source; deployment config + catalog content
# only. Idempotent: skips when the plugin is active and the smoke product exists.
# Requires: provisioned WordPress (scripts/setup-wordpress-smoke.sh) + wp-cli.
set -euo pipefail
DIR="${PHPX_WORDPRESS_DIR:-data/smoke/wordpress}"
WP="${WP_CLI:-wp}"
export WP_CLI_SUPPRESS_GLOBAL_PARAMS=1

if [ ! -f "$DIR/wp-config.php" ]; then
  echo "[wc-smoke] no WordPress install at $DIR (run setup-wordpress-smoke.sh first)"; exit 2
fi
if ! command -v "$WP" >/dev/null && [ "$WP" = "wp" ]; then
  echo "wp-cli not found (set WP_CLI)"; exit 2
fi
if ! "$WP" plugin is-active woocommerce --path="$DIR" 2>/dev/null; then
  "$WP" plugin install woocommerce --activate --path="$DIR"
fi
# Fixed smoke product (stable slug for tests). Reuses id 12 if present.
if ! "$WP" post get 12 --path="$DIR" >/dev/null 2>&1; then
  "$WP" post create --path="$DIR" --post_type=product --post_title="Smoke Pixel" \
    --post_status=publish --porcelain >/dev/null
fi
ID=$("$WP" post list --path="$DIR" --post_type=product --fields=ID --format=ids | tr ' ' '\n' | head -1)
"$WP" post meta set "$ID" _price 25 --path="$DIR" >/dev/null
"$WP" post meta set "$ID" _regular_price 25 --path="$DIR" >/dev/null
"$WP" post meta set "$ID" _stock_status instock --path="$DIR" >/dev/null
"$WP" post meta set "$ID" _stock 100 --path="$DIR" >/dev/null
"$WP" post meta set "$ID" _visibility visible --path="$DIR" >/dev/null 2>&1 || true
echo "[wc-smoke] ready at $DIR ($("$WP" plugin get woocommerce --field=version --path="$DIR" 2>/dev/null || echo version-unknown), product id=$ID)"
