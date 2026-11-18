<?php
// Laravel-minimal SIMULATION (no framework download): front-controller routing,
// .env-style config, middleware header, query/headers/cookies/body echo.
$rid = getenv("PHPX_REQUEST_ID"); if ($rid === false) { $rid = "direct"; }
header("X-Phpx-Request: " . $rid);
header("X-Fixture: laravel-minimal");
header("X-Middleware: laravel-sim");
$path = getenv("PHPX_PATH"); if ($path === false) { $path = $_SERVER["REQUEST_URI"] ?? "/"; }
$pathOnly = explode("?", (string)$path, 2)[0];
$method = $_SERVER["REQUEST_METHOD"] ?? "GET";
$raw = function_exists("phpx_input") ? phpx_input() : file_get_contents("php://input"); if ($raw === false || $raw === null) { $raw = ""; }
$cookie = $_COOKIE["phpx_test"] ?? "";
$accept = $_SERVER["HTTP_ACCEPT"] ?? "";
$appEnv = getenv("APP_ENV"); if ($appEnv === false || $appEnv === "") { $appEnv = "testing"; }
if ($pathOnly === "/api/hello") {
    header("Content-Type: application/json");
    echo json_encode(["framework" => "laravel-sim", "message" => "hello laravel", "env" => $appEnv, "rid" => $rid]);
    exit;
}
echo "laravel-minimal ok|path=" . $pathOnly . "|method=" . $method . "|env=" . $appEnv . "|cookie=" . $cookie . "|accept=" . substr((string)$accept, 0, 60) . "|body=" . substr((string)$raw, 0, 200) . "|rid=" . $rid;
