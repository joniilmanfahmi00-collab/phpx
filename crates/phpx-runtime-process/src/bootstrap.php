<?php
// PHPX SAPI-compat bootstrap (M4): emulates CGI-like superglobals for one-shot CLI workers.
// Loaded via auto_prepend_file by the gateway. No app source changes required.
// Covered: $_GET (from PHPX_QUERY), $_COOKIE (from Cookie header), $_SERVER
// (REQUEST_METHOD/REQUEST_URI/QUERY_STRING/HTTP_*), getallheaders() polyfill.
// Known limits (see docs/operations/sapi-audit.md): php://input is empty under
// CLI; read php://stdin as fallback (helper phpx_input()).
$__phpx_query = getenv("PHPX_QUERY");
if ($__phpx_query === false) { $__phpx_query = ""; }
parse_str((string)$__phpx_query, $__phpx_get);
if (is_array($__phpx_get)) {
    $_GET = array_merge($_GET, $__phpx_get);
    $_REQUEST = array_merge($_REQUEST, $__phpx_get);
}
$__phpx_headers_json = getenv("PHPX_HEADERS_JSON");
$__phpx_headers = [];
if ($__phpx_headers_json !== false && $__phpx_headers_json !== "") {
    $__decoded = json_decode((string)$__phpx_headers_json, true);
    if (is_array($__decoded)) { $__phpx_headers = $__decoded; }
}
foreach ($__phpx_headers as $__hk => $__hv) {
    $__nk = "HTTP_" . strtoupper(str_replace("-", "_", (string)$__hk));
    if (!isset($_SERVER[$__nk])) { $_SERVER[$__nk] = (string)$__hv; }
    if (strtolower((string)$__hk) === "content-type" && !isset($_SERVER["CONTENT_TYPE"])) {
        $_SERVER["CONTENT_TYPE"] = (string)$__hv;
    }
    if (strtolower((string)$__hk) === "content-length" && !isset($_SERVER["CONTENT_LENGTH"])) {
        $_SERVER["CONTENT_LENGTH"] = (string)$__hv;
    }
}
$__phpx_method = getenv("PHPX_METHOD");
$_SERVER["REQUEST_METHOD"] = ($__phpx_method === false || $__phpx_method === "") ? "GET" : (string)$__phpx_method;
$__phpx_path = getenv("PHPX_PATH");
if ($__phpx_path === false || $__phpx_path === "") { $__phpx_path = "/"; }
$_SERVER["REQUEST_URI"] = (string)$__phpx_path . ($__phpx_query !== "" ? "?" . (string)$__phpx_query : "");
$_SERVER["QUERY_STRING"] = (string)$__phpx_query;
$_SERVER["PHPX_SAPI"] = "phpx-one-shot";
if (isset($__phpx_headers["cookie"]) || isset($__phpx_headers["Cookie"])) {
    $__cookie_header = isset($__phpx_headers["cookie"]) ? (string)$__phpx_headers["cookie"] : (string)$__phpx_headers["Cookie"];
    foreach (explode(";", $__cookie_header) as $__pair) {
        $__kv = explode("=", trim($__pair), 2);
        if (count($__kv) === 2 && $__kv[0] !== "") {
            $__ck = trim($__kv[0]);
            $__cv = urldecode(trim($__kv[1], " \""));
            if (!isset($_COOKIE[$__ck])) {
                $_COOKIE[$__ck] = $__cv;
                $_REQUEST[$__ck] = $__cv;
            }
        }
    }
}
if (!function_exists("getallheaders")) {
    function getallheaders() {
        $__out = [];
        foreach ($_SERVER as $__k => $__v) {
            if (strpos($__k, "HTTP_") === 0) {
                $__name = str_replace(" ", "-", ucwords(strtolower(str_replace("_", " ", substr($__k, 5)))));
                $__out[$__name] = $__v;
            }
        }
        return $__out;
    }
}
if (!function_exists("phpx_input")) {
    function phpx_input() {
        $__data = @file_get_contents("php://input");
        if ($__data === false || $__data === "") {
            $__stdin = @file_get_contents("php://stdin");
            if ($__stdin !== false && $__stdin !== "") { return $__stdin; }
        }
        return ($__data === false) ? "" : $__data;
    }
}
unset($__phpx_query, $__phpx_get, $__phpx_headers_json, $__phpx_headers, $__decoded, $__hk, $__hv, $__nk, $__phpx_method, $__phpx_path, $__cookie_header, $__pair, $__kv, $__ck, $__cv);
