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
// Per-request SAPI reset (partial; see limits below): $_GET/$_POST/$_COOKIE/
// $_REQUEST are rebuilt from the frame, PHPX_* vars are putenv()d per request
// (process env cannot change per request from the supervisor side), and the
// entry file is included. `phpx_input()` returns the frame body.
// Output buffering is reset per request; http_response_code(200) restored.
//
// Known limits (P0-B, same as sapi-audit): $_FILES/multipart and $_SESSION
// persistence are NOT emulated; `exit()`/`die()` in app code terminates the
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
  $_GET = [];
  $_POST = [];
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
  // P1-B integration parity with the one-shot bootstrap: populate $_POST for
  // urlencoded form submissions (CLI SAPI never does). Multipart stays
  // unsupported; raw bytes remain available via phpx_input().
  $__phpx_ctype = strtolower(trim(explode(";", isset($headers["content-type"]) ? (string)$headers["content-type"] : (isset($headers["Content-Type"]) ? (string)$headers["Content-Type"] : ""))[0]));
  if ($__phpx_ctype === "application/x-www-form-urlencoded" && $bodyStr !== "") {
    parse_str($bodyStr, $__phpx_post);
    if (is_array($__phpx_post)) {
      $_POST = array_merge($_POST, $__phpx_post);
      $_REQUEST = array_merge($_REQUEST, $__phpx_post);
    }
    unset($__phpx_post);
  }
  unset($__phpx_ctype);

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
