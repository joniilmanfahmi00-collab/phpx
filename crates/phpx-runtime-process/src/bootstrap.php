<?php
// PHPX SAPI-compat bootstrap (M4): emulates CGI-like superglobals for one-shot CLI workers.
// Loaded via auto_prepend_file by the gateway. No app source changes required.
// Covered: $_GET (from PHPX_QUERY), $_COOKIE (from Cookie header), $_SERVER
// (REQUEST_METHOD/REQUEST_URI/QUERY_STRING/HTTP_*/CONTENT_*/PHP_AUTH_*),
// getallheaders() polyfill.
// Known limits (see docs/operations/sapi-audit.md): $_POST is populated for
// application/x-www-form-urlencoded only; $_FILES/multipart parsing and
// php://input under CLI stay raw via phpx_input().
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
    if (strtolower((string)$__hk) === "authorization") {
        $_SERVER["HTTP_AUTHORIZATION"] = (string)$__hv;
    }
}
// CGI parity (P1-B integration): map `Authorization: Basic ...` to
// PHP_AUTH_USER/PHP_AUTH_PW like Apache/CGI SAPIs do, so app-level HTTP
// auth (e.g. WordPress application passwords) keeps working. Bearer and
// other schemes are left untouched.
$__phpx_auth_h = null;
if (isset($__phpx_headers["authorization"])) { $__phpx_auth_h = (string)$__phpx_headers["authorization"]; }
elseif (isset($__phpx_headers["Authorization"])) { $__phpx_auth_h = (string)$__phpx_headers["Authorization"]; }
if ($__phpx_auth_h !== null) {
    $_SERVER["HTTP_AUTHORIZATION"] = $__phpx_auth_h;
    $__phpx_auth = trim($__phpx_auth_h);
    if (stripos($__phpx_auth, "Basic ") === 0) {
        $__phpx_cred = base64_decode(substr($__phpx_auth, 6), true);
        if ($__phpx_cred !== false && strpos($__phpx_cred, ":") !== false) {
            $__phpx_parts = explode(":", $__phpx_cred, 2);
            if (!isset($_SERVER["PHP_AUTH_USER"])) { $_SERVER["PHP_AUTH_USER"] = $__phpx_parts[0]; }
            if (!isset($_SERVER["PHP_AUTH_PW"])) { $_SERVER["PHP_AUTH_PW"] = $__phpx_parts[1]; }
            unset($__phpx_parts);
        }
        unset($__phpx_cred);
    }
    unset($__phpx_auth);
}
unset($__phpx_auth_h);
$__phpx_method = getenv("PHPX_METHOD");
$_SERVER["REQUEST_METHOD"] = ($__phpx_method === false || $__phpx_method === "") ? "GET" : (string)$__phpx_method;
$__phpx_path = getenv("PHPX_PATH");
if ($__phpx_path === false || $__phpx_path === "") { $__phpx_path = "/"; }
$_SERVER["REQUEST_URI"] = (string)$__phpx_path . ($__phpx_query !== "" ? "?" . (string)$__phpx_query : "");
$_SERVER["QUERY_STRING"] = (string)$__phpx_query;
$_SERVER["PHPX_SAPI"] = "phpx-one-shot";
// CGI parity (P1-B integration): SERVER_NAME/SERVER_PORT from Host, loopback
// REMOTE_ADDR default. Real apps read these (e.g. WP comment author IP and
// script-host dedup); unset keys emit warnings into responses.
$__phpx_host = isset($__phpx_headers["host"]) ? trim((string)$__phpx_headers["host"]) : (isset($__phpx_headers["Host"]) ? trim((string)$__phpx_headers["Host"]) : "");
if ($__phpx_host === "") { $__phpx_host = "localhost"; }
$_SERVER["HTTP_HOST"] = $__phpx_host;
if (substr($__phpx_host, 0, 1) === "[") {
    // IPv6 literal, e.g. [::1]:18312.
    $__phpx_re = explode("]", substr($__phpx_host, 1), 2);
    $_SERVER["SERVER_NAME"] = ($__phpx_re[0] !== "" ? $__phpx_re[0] : "localhost");
    $__phpx_rp = ltrim(isset($__phpx_re[1]) ? (string)$__phpx_re[1] : "", ":");
    $_SERVER["SERVER_PORT"] = (is_numeric($__phpx_rp) && $__phpx_rp !== "" ? $__phpx_rp : "80");
    unset($__phpx_re, $__phpx_rp);
} elseif (strpos($__phpx_host, ":") !== false) {
    $__phpx_hp = explode(":", $__phpx_host, 2);
    $_SERVER["SERVER_NAME"] = ($__phpx_hp[0] !== "" ? $__phpx_hp[0] : "localhost");
    $_SERVER["SERVER_PORT"] = (is_numeric($__phpx_hp[1]) ? $__phpx_hp[1] : "80");
    unset($__phpx_hp);
} else {
    $_SERVER["SERVER_NAME"] = $__phpx_host;
    $_SERVER["SERVER_PORT"] = "80";
}
unset($__phpx_host);
if (!isset($_SERVER["REMOTE_ADDR"])) { $_SERVER["REMOTE_ADDR"] = "127.0.0.1"; }
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
        if (isset($GLOBALS["PHPX_RAW_BODY"])) { return (string)$GLOBALS["PHPX_RAW_BODY"]; }
        $__data = @file_get_contents("php://input");
        if ($__data === false || $__data === "") {
            $__stdin = @file_get_contents("php://stdin");
            if ($__stdin !== false && $__stdin !== "") { return $__stdin; }
        }
        return ($__data === false) ? "" : $__data;
    }
}
// P1-B integration: CLI SAPI never populates $_POST/$_FILES from the request
// body. Emulate the urlencoded part (like the web SAPI would): read the raw
// body once, stash it for phpx_input(), and parse it into $_POST/$_REQUEST
// when the content type is a form submission. Multipart stays unsupported
// (see sapi-audit.md): $_FILES is not populated, raw bytes stay readable
// via phpx_input().
$GLOBALS["PHPX_RAW_BODY"] = "";
if (($_SERVER["REQUEST_METHOD"] ?? "GET") !== "GET" && ($_SERVER["REQUEST_METHOD"] ?? "GET") !== "HEAD") {
    $__phpx_raw = @file_get_contents("php://stdin");
    if ($__phpx_raw === false) { $__phpx_raw = ""; }
    $GLOBALS["PHPX_RAW_BODY"] = $__phpx_raw;
    $__phpx_ctype = strtolower(trim(explode(";", (string)($_SERVER["CONTENT_TYPE"] ?? ""))[0]));
    if ($__phpx_ctype === "application/x-www-form-urlencoded" && $__phpx_raw !== "") {
        parse_str($__phpx_raw, $__phpx_post);
        if (is_array($__phpx_post)) {
            $_POST = array_merge($_POST, $__phpx_post);
            $_REQUEST = array_merge($_REQUEST, $__phpx_post);
        }
        unset($__phpx_post);
    }
    unset($__phpx_ctype, $__phpx_raw);
}
unset($__phpx_query, $__phpx_get, $__phpx_headers_json, $__phpx_headers, $__decoded, $__hk, $__hv, $__nk, $__phpx_method, $__phpx_path, $__cookie_header, $__pair, $__kv, $__ck, $__cv);
