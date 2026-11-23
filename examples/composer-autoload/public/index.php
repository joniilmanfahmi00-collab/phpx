<?php
require dirname(__DIR__) . "/vendor/autoload.php";
$rid = getenv("PHPX_REQUEST_ID"); if ($rid === false) { $rid = "direct"; }
header("X-Phpx-Request: " . $rid);
header("X-Fixture: composer-autoload");
$name = isset($_GET["name"]) ? (string)$_GET["name"] : "composer";
$greet = \App\Greeter::hello($name);
$method = $_SERVER["REQUEST_METHOD"] ?? "GET";
$raw = function_exists("phpx_input") ? phpx_input() : file_get_contents("php://input");
if ($raw === false) { $raw = ""; }
$cookie = $_COOKIE["phpx_test"] ?? "";
$ua = $_SERVER["HTTP_USER_AGENT"] ?? "";
$pname = $_POST["pname"] ?? "";
echo "composer-autoload ok|greeter=" . $greet . "|method=" . $method . "|query_name=" . $name . "|cookie=" . $cookie . "|ua=" . $ua . "|body=" . substr((string)$raw, 0, 200) . "|post_pname=" . $pname . "|rid=" . $rid;
