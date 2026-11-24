<?php
// PHPX persistent worker v2 (P0-B supervised pool).
// Supervisor spawns: php worker-persistent.php
//
// Startup: emit one handshake line on STDOUT, then serve framed requests:
//   {"phpx":"ready","protocol_version":1,"pid":<getmypid>}\n
// Framing after that: 4-byte big-endian length + JSON PhpRequest on STDIN,
// same framing back for PhpResponse on STDOUT. STDERR is diagnostics only.
//
// FRAMED-PROTOCOL RULE (P1-B hardening): STDOUT carries ONLY the handshake
// line and length-prefixed frames. PHP warnings/notices print to STDOUT by
// default under CLI and would desync the stream, so this worker disables
// on-screen errors and routes them to STDERR instead (log_errors=1):
// app echo/print output is still captured by output buffering and returned
// normally; only raw `fwrite(STDOUT)` from app code remains forbidden.
// See docs/operations/sapi-audit.md.

error_reporting(E_ALL);
ini_set("display_errors", "0");
ini_set("log_errors", "1");
//
// Per-request SAPI reset (partial; see limits below): $_GET/$_POST/$_FILES/
// $_COOKIE/$_REQUEST are rebuilt from the frame, PHPX_* vars are putenv()d per request
// (process env cannot change per request from the supervisor side), and the
// entry file is included. `phpx_input()` returns the frame body.
// Output buffering is reset per request; http_response_code(200) restored.
//
// Known limits (P0-B, same as sapi-audit): move_uploaded_file() /
// is_uploaded_file() return false under CLI SAPI (read bytes via
// file_get_contents($_FILES[...]["tmp_name"]) or rename() it instead);
// $_SESSION persistence is NOT emulated; `exit()`/`die()` in app code terminates the
// whole worker (supervisor restarts it with a new incarnation); static
// variables and $GLOBALS set by app code persist across requests in the same
// worker (documented state-leakage limit of persistent mode; use one-shot
// when strict isolation is required).

$__handshake = json_encode([
  "phpx" => "ready",
  "protocol_version" => 1,
  "pid" => getmypid(),
]);
fwrite(STDOUT, $__handshake . "\n");
fflush(STDOUT);
unset($__handshake);

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
    return isset($GLOBALS["PHPX_BODY"]) ? (string)$GLOBALS["PHPX_BODY"] : "";
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
register_shutdown_function("phpx_cleanup_tmp_files");

function phpx_read_exact($n) {
  $buf = "";
  while (strlen($buf) < $n) {
    $chunk = fread(STDIN, $n - strlen($buf));
    if ($chunk === false || $chunk === "") { return null; }
    $buf .= $chunk;
  }
  return $buf;
}

while (true) {
  $hlen = phpx_read_exact(4);
  if ($hlen === null) { break; } // EOF: supervisor shutting down
  $parts = unpack("N", $hlen);
  if ($parts === false) { fwrite(STDERR, "phpx: bad frame header\n"); break; }
  $len = $parts[1];
  if ($len > 16 * 1024 * 1024) { fwrite(STDERR, "phpx: frame too large\n"); exit(2); }
  $json = phpx_read_exact($len);
  if ($json === null) { break; }
  $req = json_decode($json, true);
  if (!is_array($req) || !isset($req["request_id"])) {
    fwrite(STDERR, "phpx: bad frame\n");
    continue;
  }
  $t0 = microtime(true);

  // ---- Per-request SAPI reset ----
  // Drop previous request's temp upload files before resetting superglobals.
  if (isset($GLOBALS["PHPX_TMP_FILES"]) && is_array($GLOBALS["PHPX_TMP_FILES"])) {
    foreach ($GLOBALS["PHPX_TMP_FILES"] as $__old) {
      if (is_string($__old) && $__old !== "" && is_file($__old)) { @unlink($__old); }
    }
  }
  $GLOBALS["PHPX_TMP_FILES"] = [];
  $_GET = [];
  $_POST = [];
  $_FILES = [];
  $_COOKIE = [];
  $_REQUEST = [];
  $rid = (string)$req["request_id"];
  $method = isset($req["method"]) ? (string)$req["method"] : "GET";
  $path = isset($req["path"]) ? (string)$req["path"] : "/";
  $query = isset($req["query"]) ? (string)$req["query"] : "";
  $headers = (isset($req["headers"]) && is_array($req["headers"])) ? $req["headers"] : [];
  $bodyBytes = (isset($req["body"]) && is_array($req["body"])) ? $req["body"] : [];
  $bodyStr = $bodyBytes ? pack("C*", ...array_values(array_map("intval", $bodyBytes))) : "";
  $GLOBALS["PHPX_BODY"] = $bodyStr;
  if (!isset($_SERVER["REMOTE_ADDR"])) { $_SERVER["REMOTE_ADDR"] = "127.0.0.1"; }
  $__phpx_host = isset($headers["host"]) ? trim((string)$headers["host"]) : (isset($headers["Host"]) ? trim((string)$headers["Host"]) : "");
  if ($__phpx_host === "") { $__phpx_host = "localhost"; }
  $_SERVER["HTTP_HOST"] = $__phpx_host;
  if (substr($__phpx_host, 0, 1) === "[") {
    $__phpx_hp = explode("]", substr($__phpx_host, 1), 2);
    $_SERVER["SERVER_NAME"] = ($__phpx_hp[0] !== "" ? $__phpx_hp[0] : "localhost");
    $__phpx_rp = ltrim(isset($__phpx_hp[1]) ? (string)$__phpx_hp[1] : "", ":");
    $_SERVER["SERVER_PORT"] = (is_numeric($__phpx_rp) && $__phpx_rp !== "" ? $__phpx_rp : "80");
    unset($__phpx_hp, $__phpx_rp);
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
  // Multipart/form-data (RC track): parsed in PHP so $_POST/$_FILES work
  // under CLI SAPI, same as the one-shot bootstrap. move_uploaded_file() /
  // is_uploaded_file() still return false under CLI (documented limit).
  $__phpx_ctype = strtolower(trim(explode(";", isset($headers["content-type"]) ? (string)$headers["content-type"] : (isset($headers["Content-Type"]) ? (string)$headers["Content-Type"] : ""))[0]));
  $__phpx_full_ctype = isset($headers["content-type"]) ? (string)$headers["content-type"] : (isset($headers["Content-Type"]) ? (string)$headers["Content-Type"] : "");
  if ($__phpx_ctype === "application/x-www-form-urlencoded" && $bodyStr !== "") {
    parse_str($bodyStr, $__phpx_post);
    if (is_array($__phpx_post)) {
      $_POST = array_merge($_POST, $__phpx_post);
      $_REQUEST = array_merge($_REQUEST, $__phpx_post);
    }
    unset($__phpx_post);
  } elseif ($__phpx_ctype === "multipart/form-data" && $bodyStr !== "") {
    phpx_parse_multipart($bodyStr, $__phpx_full_ctype);
  }
  unset($__phpx_ctype, $__phpx_full_ctype);

  putenv("PHPX_REQUEST_ID=" . $rid);
  $_SERVER["PHPX_REQUEST_ID"] = $rid;
  $_SERVER["REQUEST_METHOD"] = $method !== "" ? $method : "GET";
  $_SERVER["REQUEST_URI"] = $query !== "" ? $path . "?" . $query : $path;
  $_SERVER["QUERY_STRING"] = $query;
  $_SERVER["PHPX_SAPI"] = "phpx-persistent";
  // Drop stale per-request keys from the previous request. HTTP_* (including
  // HTTP_AUTHORIZATION) is rebuilt below; CONTENT_*/PHP_AUTH_* are only set
  // when the current request carries them, so clear them explicitly to avoid
  // leaking one request's auth/content type into the next on reuse.
  foreach (array_keys($_SERVER) as $__sk) {
    if (strpos($__sk, "HTTP_") === 0) { unset($_SERVER[$__sk]); }
  }
  unset($__sk);
  unset($_SERVER["CONTENT_TYPE"], $_SERVER["CONTENT_LENGTH"]);
  unset($_SERVER["PHP_AUTH_USER"], $_SERVER["PHP_AUTH_PW"]);
  foreach ($headers as $__hk => $__hv) {
    $__nk = "HTTP_" . strtoupper(str_replace("-", "_", (string)$__hk));
    $_SERVER[$__nk] = (string)$__hv;
    if (strtolower((string)$__hk) === "content-type") { $_SERVER["CONTENT_TYPE"] = (string)$__hv; }
    if (strtolower((string)$__hk) === "content-length") { $_SERVER["CONTENT_LENGTH"] = (string)$__hv; }
    if (strtolower((string)$__hk) === "authorization") { $_SERVER["HTTP_AUTHORIZATION"] = (string)$__hv; }
  }
  unset($__hk, $__hv, $__nk);
  // CGI parity (P1-B integration): `Authorization: Basic ...` -> PHP_AUTH_USER/PW.
  $__phpx_auth_h = null;
  if (isset($headers["authorization"])) { $__phpx_auth_h = (string)$headers["authorization"]; }
  elseif (isset($headers["Authorization"])) { $__phpx_auth_h = (string)$headers["Authorization"]; }
  if ($__phpx_auth_h !== null) {
    $_SERVER["HTTP_AUTHORIZATION"] = $__phpx_auth_h;
    $__phpx_auth = trim($__phpx_auth_h);
    if (stripos($__phpx_auth, "Basic ") === 0) {
      $__phpx_cred = base64_decode(substr($__phpx_auth, 6), true);
      if ($__phpx_cred !== false && strpos($__phpx_cred, ":") !== false) {
        $__phpx_parts = explode(":", $__phpx_cred, 2);
        $_SERVER["PHP_AUTH_USER"] = $__phpx_parts[0];
        $_SERVER["PHP_AUTH_PW"] = $__phpx_parts[1];
        unset($__phpx_parts);
      }
      unset($__phpx_cred);
    }
    unset($__phpx_auth);
  }
  unset($__phpx_auth_h);
  parse_str($query, $__q);
  if (is_array($__q)) {
    $_GET = $__q;
    $_REQUEST = array_merge($_REQUEST, $__q);
  }
  unset($__q);
  $__cookieHeader = isset($headers["cookie"]) ? (string)$headers["cookie"] : (isset($headers["Cookie"]) ? (string)$headers["Cookie"] : "");
  if ($__cookieHeader !== "") {
    foreach (explode(";", $__cookieHeader) as $__pair) {
      $__kv = explode("=", trim($__pair), 2);
      if (count($__kv) === 2 && trim($__kv[0]) !== "") {
        $__cv = urldecode(trim($__kv[1], " \""));
        $_COOKIE[trim($__kv[0])] = $__cv;
        $_REQUEST[trim($__kv[0])] = $__cv;
      }
    }
  }
  unset($__cookieHeader, $__pair, $__kv, $__cv);

  while (ob_get_level() > 0) { ob_end_clean(); }
  http_response_code(200);
  ob_start();
  $status = 200;
  try {
    $appRoot = isset($req["app_root"]) ? (string)$req["app_root"] : ".";
    $entry = isset($req["entrypoint"]) ? (string)$req["entrypoint"] : "index.php";
    $file = rtrim($appRoot, "/\\") . DIRECTORY_SEPARATOR . $entry;
    if (!is_file($file)) { $status = 404; echo "not found"; }
    else { include $file; }
  } catch (Throwable $e) {
    $status = 500;
    fwrite(STDERR, "phpx worker exception: " . $e->getMessage() . "\n");
  }
  $out = ob_get_clean();
  if ($out === false) { $out = ""; }
  $resp = [
    "request_id" => $rid,
    "status" => $status,
    "headers" => new stdClass(),
    "body" => $out === "" ? [] : array_values(unpack("C*", $out)),
    "duration_ms" => (int)((microtime(true) - $t0) * 1000),
    "outcome" => "ok",
  ];
  $enc = json_encode($resp);
  if ($enc === false) {
    fwrite(STDERR, "phpx: response encode failed\n");
    continue;
  }
  fwrite(STDOUT, pack("N", strlen($enc)) . $enc);
  fflush(STDOUT);
}
