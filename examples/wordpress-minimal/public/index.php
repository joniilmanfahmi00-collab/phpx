<?php
// WordPress-minimal SIMULATION: wp-config style constants, loop, shortcode, ?p=, ?s=.
$rid = getenv("PHPX_REQUEST_ID"); if ($rid === false) { $rid = "direct"; }
header("X-Phpx-Request: " . $rid);
header("X-Fixture: wordpress-minimal");
define("WP_SIM", true);
define("DB_NAME", "phpx_wp_sim");
$posts = [["ID" => 1, "title" => "Hello WP"], ["ID" => 2, "title" => "PHPX sim"]];
$content = "Welcome [phpx] to WP sim. Loop: ";
$titles = [];
foreach ($posts as $post) { $titles[] = $post["title"]; }
$content .= implode(", ", $titles);
$content = str_replace("[phpx]", "PHPX", $content);
$p = isset($_GET["p"]) ? (string)$_GET["p"] : "";
$s = isset($_GET["s"]) ? (string)$_GET["s"] : "";
if ($p !== "") { $content .= " | single p=" . preg_replace("/[^0-9]/", "", $p); }
if ($s !== "") { $content .= " | search s=" . substr(preg_replace("/[^a-zA-Z0-9 ]/", "", $s), 0, 40); }
$cookie = $_COOKIE["phpx_test"] ?? "";
echo "wordpress-minimal ok|" . $content . "|cookie=" . $cookie . "|rid=" . $rid;
