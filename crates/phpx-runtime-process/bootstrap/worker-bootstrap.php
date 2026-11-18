<?php
// PHPX M1 prototype: persistent framed worker (experimental).
// Framing: 4-byte big-endian length + JSON PhpRequest on STDIN,
// same framing back for PhpResponse on STDOUT. Stderr is diagnostics only.
// One-shot execution (execute_one_shot) remains the M1 default; this file
// proves the framed protocol can drive a long-lived PHP process.
// Usage (manual): php worker-bootstrap.php
// Note: request isolation for persistent workers requires reset hooks
// (ADR-0014 2.8); M1 isolation tests use one-shot processes.

function read_exact($n) {
  $buf = "";
  while (strlen($buf) < $n) {
    $chunk = fread(STDIN, $n - strlen($buf));
    if ($chunk === false || $chunk === "") { return null; }
    $buf .= $chunk;
  }
  return $buf;
}

while (true) {
  $hlen = read_exact(4);
  if ($hlen === null) { break; } // EOF: supervisor shutting down
  $len = unpack("N", $hlen)[1];
  if ($len > 16 * 1024 * 1024) { fwrite(STDERR, "frame too large\n"); exit(2); }
  $json = read_exact($len);
  if ($json === null) { break; }
  $req = json_decode($json, true);
  if (!is_array($req) || !isset($req["request_id"])) {
    fwrite(STDERR, "bad frame\n");
    continue;
  }
  $rid = $req["request_id"];
  $t0 = microtime(true);
  // Minimal request env for the app entrypoint.
  $_SERVER["PHPX_REQUEST_ID"] = $rid;
  putenv("PHPX_REQUEST_ID=" . $rid);
  $appRoot = $req["app_root"] ?? ".";
  $entry = $req["entrypoint"] ?? "index.php";
  $file = rtrim($appRoot, "/\\") . DIRECTORY_SEPARATOR . $entry;
  http_response_code(200);
  ob_start();
  $status = 200;
  $outcome = "ok";
  try {
    if (!is_file($file)) { $status = 404; echo "not found"; }
    else { include $file; }
  } catch (Throwable $e) {
    $status = 500;
    $outcome = "crashed";
    fwrite(STDERR, "worker exception: " . $e->getMessage() . "\n");
  }
  $body = ob_get_clean();
  $resp = [
    "request_id" => $rid,
    "status" => $status,
    "headers" => [],
    "body" => array_values(unpack("C*", $body === false ? "" : $body)),
    "duration_ms" => (int)((microtime(true) - $t0) * 1000),
    "outcome" => $outcome,
  ];
  // Note: body as byte array here for prototype clarity; M1 one-shot
  // path captures raw stdout bytes directly (no JSON byte-array overhead).
  $out = json_encode($resp);
  fwrite(STDOUT, pack("N", strlen($out)) . $out);
  fflush(STDOUT);
}
