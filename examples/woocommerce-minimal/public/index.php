<?php
// WooCommerce-minimal SIMULATION (runs after WP baseline): cart + checkout via query.
$rid = getenv("PHPX_REQUEST_ID"); if ($rid === false) { $rid = "direct"; }
header("X-Phpx-Request: " . $rid);
header("X-Fixture: woocommerce-minimal");
$catalog = ["101" => ["name" => "Tee", "price" => 1500], "202" => ["name" => "Mug", "price" => 900]];
$cart = [];
if (isset($_COOKIE["woo_cart"])) {
    $rawCart = (string)$_COOKIE["woo_cart"];
    foreach (explode(",", $rawCart) as $id) {
        $id = preg_replace("/[^0-9]/", "", trim($id));
        if (isset($catalog[$id])) { $cart[] = $id; }
    }
}
$add = isset($_GET["add_to_cart"]) ? preg_replace("/[^0-9]/", "", (string)$_GET["add_to_cart"]) : "";
if ($add !== "" && isset($catalog[$add])) { $cart[] = $add; }
$total = 0;
foreach ($cart as $id) { $total += $catalog[$id]["price"]; }
if (isset($_GET["checkout"])) {
    header("Content-Type: application/json");
    echo json_encode(["shop" => "woo-sim", "items" => array_values($cart), "total_cents" => $total, "checkout" => "ok", "rid" => $rid]);
    exit;
}
echo "woocommerce-minimal ok|cart=" . implode(",", $cart) . "|total_cents=" . $total . "|rid=" . $rid;
