<?php
// PHPX SAPI-compat bootstrap (M4): emulates CGI-like superglobals for one-shot CLI workers.
// Loaded via auto_prepend_file by the gateway. No app source changes required.
// Covered: $_GET (from PHPX_QUERY), $_COOKIE (from Cookie header), $_SERVER
// (REQUEST_METHOD/REQUEST_URI/QUERY_STRING/HTTP_*/CONTENT_*/PHP_AUTH_*),
// getallheaders() polyfill.
// Known limits (see docs/operations/sapi-audit.md): $_POST is populated for
// application/x-www-form-urlencoded and multipart/form-data (parsed in PHP,
// $_FILES materialized to temp files); php://input under CLI stays empty —
// use phpx_input() for raw bytes. move_uploaded_file()/is_uploaded_file()
// stay false under CLI (read tmp_name directly or rename() it).
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
if (!function_exists("phpx_cleanup_tmp_files")) {
    function phpx_cleanup_tmp_files() {
        if (!isset($GLOBALS["PHPX_TMP_FILES"]) || !is_array($GLOBALS["PHPX_TMP_FILES"])) { return; }
        foreach ($GLOBALS["PHPX_TMP_FILES"] as $__f) {
            if (is_string($__f) && $__f !== "" && is_file($__f)) { @unlink($__f); }
        }
        $GLOBALS["PHPX_TMP_FILES"] = [];
    }
}
if (!function_exists("phpx_assign_post_value")) {
    function phpx_assign_post_value(&$__dest, $__name, $__value) {
        $__pos = strpos($__name, "[");
        if ($__pos === false) {
            if ($__name !== "") { $__dest[$__name] = $__value; }
            return;
        }
        $__base = substr($__name, 0, $__pos);
        if ($__base === "") { return; }
        preg_match_all('/\[([^\]]*)\]/', $__name, $__m);
        $__keys = $__m[1];
        if (!isset($__dest[$__base]) || !is_array($__dest[$__base])) { $__dest[$__base] = []; }
        $__ref =& $__dest[$__base];
        $__n = count($__keys);
        foreach ($__keys as $__i => $__k) {
            $__last = ($__i === $__n - 1);
            if ($__k === "") {
                if ($__last) { $__ref[] = $__value; }
                else {
                    $__ref[] = [];
                    end($__ref); $__idx = key($__ref);
                    $__ref =& $__ref[$__idx];
                }
            } else {
                if ($__last) { $__ref[$__k] = $__value; }
                else {
                    if (!isset($__ref[$__k]) || !is_array($__ref[$__k])) { $__ref[$__k] = []; }
                    $__ref =& $__ref[$__k];
                }
            }
        }
        unset($__ref);
    }
}
if (!function_exists("phpx_assign_file_entry")) {
    function phpx_assign_file_entry($__field, $__entry) {
        $__pos = strpos($__field, "[");
        if ($__pos === false) {
            if ($__field !== "") { $_FILES[$__field] = $__entry; }
            return;
        }
        $__base = substr($__field, 0, $__pos);
        if ($__base === "") { return; }
        preg_match_all('/\[([^\]]*)\]/', $__field, $__m);
        $__keys = $__m[1];
        if (!isset($_FILES[$__base]) || !is_array($_FILES[$__base]) || !isset($_FILES[$__base]["name"])) {
            $_FILES[$__base] = ["name" => [], "type" => [], "tmp_name" => [], "error" => [], "size" => []];
        }
        foreach (["name", "type", "tmp_name", "error", "size"] as $__k) {
            if (!isset($_FILES[$__base][$__k]) || !is_array($_FILES[$__base][$__k])) { $_FILES[$__base][$__k] = []; }
        }
        $__rn =& $_FILES[$__base]["name"];
        $__rt =& $_FILES[$__base]["type"];
        $__rp =& $_FILES[$__base]["tmp_name"];
        $__re =& $_FILES[$__base]["error"];
        $__rs =& $_FILES[$__base]["size"];
        $__n = count($__keys);
        foreach ($__keys as $__i => $__k) {
            $__last = ($__i === $__n - 1);
            if ($__k === "") {
                if ($__last) {
                    $__rn[] = $__entry["name"]; $__rt[] = $__entry["type"]; $__rp[] = $__entry["tmp_name"];
                    $__re[] = $__entry["error"]; $__rs[] = $__entry["size"];
                } else {
                    $__rn[] = []; $__rt[] = []; $__rp[] = []; $__re[] = []; $__rs[] = [];
                    end($__rn); $__idx = key($__rn);
                    $__rn =& $__rn[$__idx]; $__rt =& $__rt[$__idx]; $__rp =& $__rp[$__idx]; $__re =& $__re[$__idx]; $__rs =& $__rs[$__idx];
                }
            } else {
                if ($__last) {
                    $__rn[$__k] = $__entry["name"]; $__rt[$__k] = $__entry["type"]; $__rp[$__k] = $__entry["tmp_name"];
                    $__re[$__k] = $__entry["error"]; $__rs[$__k] = $__entry["size"];
                } else {
                    if (!isset($__rn[$__k]) || !is_array($__rn[$__k])) { $__rn[$__k] = []; }
                    if (!isset($__rt[$__k]) || !is_array($__rt[$__k])) { $__rt[$__k] = []; }
                    if (!isset($__rp[$__k]) || !is_array($__rp[$__k])) { $__rp[$__k] = []; }
                    if (!isset($__re[$__k]) || !is_array($__re[$__k])) { $__re[$__k] = []; }
                    if (!isset($__rs[$__k]) || !is_array($__rs[$__k])) { $__rs[$__k] = []; }
                    $__rn =& $__rn[$__k]; $__rt =& $__rt[$__k]; $__rp =& $__rp[$__k]; $__re =& $__re[$__k]; $__rs =& $__rs[$__k];
                }
            }
        }
        unset($__rn, $__rt, $__rp, $__re, $__rs);
    }
}
if (!function_exists("phpx_parse_multipart")) {
    function phpx_parse_multipart($__raw, $__ctype) {
        if (!is_string($__raw) || $__raw === "") { return 0; }
        if (!is_string($__ctype) || $__ctype === "") { return 0; }
        $__semi = strpos($__ctype, ";");
        $__type = $__semi === false ? trim($__ctype) : trim(substr($__ctype, 0, $__semi));
        if (strcasecmp($__type, "multipart/form-data") !== 0) { return 0; }
        if (!preg_match('/boundary=(?:"([^"]+)"|([^;\s]+))/i', $__ctype, $__m)) { return 0; }
        $__boundary = (isset($__m[1]) && $__m[1] !== "") ? $__m[1] : trim($__m[2] ?? "");
        $__boundary = trim($__boundary, '"');
        if ($__boundary === "" || strlen($__boundary) > 200) { return 0; }
        $__delim = "--" . $__boundary;
        $__ld = strlen($__delim);
        $__start = strpos($__raw, $__delim);
        if ($__start === false) { return 0; }
        $__pos = $__start + $__ld;
        $__chan = substr($__raw, $__pos, 2);
        if ($__chan === "--") { return 0; }
        if ($__chan === "\r\n") { $__pos += 2; }
        elseif (substr($__raw, $__pos, 1) === "\n") { $__pos += 1; }
        $__parsed = 0;
        $__guard = 0;
        $__rlen = strlen($__raw);
        if (!isset($GLOBALS["PHPX_TMP_FILES"]) || !is_array($GLOBALS["PHPX_TMP_FILES"])) { $GLOBALS["PHPX_TMP_FILES"] = []; }
        while ($__pos < $__rlen && $__guard < 200) {
            $__guard++;
            $__next = false; $__nlLen = 2; $__after = 0; $__tail = "";
            $__search = $__pos; $__inner = 0;
            while ($__search < $__rlen && $__inner < 50) {
                $__inner++;
                $__cand = strpos($__raw, "\r\n" . $__delim, $__search);
                $__nl = 2;
                if ($__cand === false) {
                    $__cand = strpos($__raw, "\n" . $__delim, $__search);
                    $__nl = 1;
                    if ($__cand === false) { break; }
                }
                $__a = $__cand + $__nl + $__ld;
                $__t2 = substr($__raw, $__a, 2);
                $__t1 = substr($__raw, $__a, 1);
                if ($__t2 === "--" || $__t2 === "\r\n" || $__t1 === "\n" || $__a >= $__rlen) {
                    $__next = $__cand; $__nlLen = $__nl; $__after = $__a; $__tail = $__t2;
                    break;
                }
                $__search = $__cand + $__nl + 1;
            }
            if ($__next === false) { break; }
            $__part = substr($__raw, $__pos, $__next - $__pos);
            $__isFinal = ($__tail === "--");
            $__sep = strpos($__part, "\r\n\r\n");
            $__sepLen = 4;
            if ($__sep === false) {
                $__sep = strpos($__part, "\n\n");
                $__sepLen = 2;
            }
            if ($__sep !== false) {
                $__headStr = substr($__part, 0, $__sep);
                $__value = substr($__part, $__sep + $__sepLen);
                $__headNorm = str_replace("\r\n", "\n", $__headStr);
                $__disp = "";
                $__ptype = "application/octet-stream";
                foreach (explode("\n", $__headNorm) as $__line) {
                    $__c = strpos($__line, ":");
                    if ($__c === false) { continue; }
                    $__hn = strtolower(trim(substr($__line, 0, $__c)));
                    $__hv = trim(substr($__line, $__c + 1));
                    if ($__hn === "content-disposition") { $__disp = $__hv; }
                    elseif ($__hn === "content-type") {
                        $__sc = strpos($__hv, ";");
                        $__ptype = $__sc === false ? trim($__hv) : trim(substr($__hv, 0, $__sc));
                        if ($__ptype === "") { $__ptype = "application/octet-stream"; }
                    }
                }
                $__fname = null; $__filename = null;
                if (preg_match('/name="([^"]*)"/', $__disp, $__nm)) { $__fname = $__nm[1]; }
                elseif (preg_match('/name=([^;\s]+)/i', $__disp, $__nm)) { $__fname = trim($__nm[1], '"'); }
                if (preg_match('/filename="([^"]*)"/', $__disp, $__fm)) { $__filename = $__fm[1]; }
                elseif (preg_match('/filename=([^;\s]+)/i', $__disp, $__fm)) { $__filename = trim($__fm[1], '"'); }
                if ($__fname !== null && $__fname !== "") {
                    if ($__filename === null) {
                        phpx_assign_post_value($_POST, $__fname, $__value);
                        phpx_assign_post_value($_REQUEST, $__fname, $__value);
                        $__parsed++;
                    } else {
                        if ($__filename === "") {
                            phpx_assign_file_entry($__fname, ["name" => "", "type" => "", "tmp_name" => "", "error" => 4, "size" => 0]);
                            $__parsed++;
                        } else {
                            $__safe = basename(str_replace("\\", "/", $__filename));
                            $__tmp = @tempnam(sys_get_temp_dir(), "phpx");
                            if ($__tmp === false) {
                                phpx_assign_file_entry($__fname, ["name" => $__safe, "type" => $__ptype, "tmp_name" => "", "error" => 6, "size" => 0]);
                            } else {
                                $__w = @file_put_contents($__tmp, $__value);
                                if ($__w === false) {
                                    phpx_assign_file_entry($__fname, ["name" => $__safe, "type" => $__ptype, "tmp_name" => "", "error" => 7, "size" => 0]);
                                } else {
                                    phpx_assign_file_entry($__fname, ["name" => $__safe, "type" => $__ptype, "tmp_name" => $__tmp, "error" => 0, "size" => strlen($__value)]);
                                    $GLOBALS["PHPX_TMP_FILES"][] = $__tmp;
                                }
                            }
                            $__parsed++;
                        }
                    }
                }
                unset($__headStr, $__value, $__headNorm, $__disp, $__ptype, $__line, $__c, $__hn, $__hv, $__sc, $__fname, $__filename, $__nm, $__fm, $__safe, $__tmp, $__w);
            }
            unset($__part, $__sep, $__sepLen);
            if ($__isFinal) { break; }
            $__pos = $__after;
            if (substr($__raw, $__pos, 2) === "\r\n") { $__pos += 2; }
            elseif (substr($__raw, $__pos, 1) === "\n") { $__pos += 1; }
            if ($__tail === "--") { break; }
            unset($__next, $__nlLen, $__after, $__tail, $__search, $__inner, $__cand, $__nl, $__a, $__t2, $__t1, $__isFinal);
        }
        unset($__semi, $__type, $__m, $__boundary, $__delim, $__ld, $__start, $__pos, $__chan, $__guard, $__rlen);
        return $__parsed;
    }
}
// Multipart/form-data (RC track): parsed in PHP so $_POST/$_FILES work under
// CLI SAPI. Raw bytes stay readable via phpx_input(). move_uploaded_file() /
// is_uploaded_file() still return false under CLI (documented limit); read
// file bytes via file_get_contents($_FILES[...]["tmp_name"]) or rename() it.
$GLOBALS["PHPX_RAW_BODY"] = "";
$GLOBALS["PHPX_TMP_FILES"] = [];
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
    } elseif ($__phpx_ctype === "multipart/form-data" && $__phpx_raw !== "") {
        $__phpx_n = phpx_parse_multipart($__phpx_raw, (string)($_SERVER["CONTENT_TYPE"] ?? ""));
        unset($__phpx_n);
        if (!empty($GLOBALS["PHPX_TMP_FILES"])) { register_shutdown_function("phpx_cleanup_tmp_files"); }
    }
    unset($__phpx_ctype, $__phpx_raw);
}
unset($__phpx_query, $__phpx_get, $__phpx_headers_json, $__phpx_headers, $__decoded, $__hk, $__hv, $__nk, $__phpx_method, $__phpx_path, $__cookie_header, $__pair, $__kv, $__ck, $__cv);
