<?php
// P0-C controlled memory workload: ?mb=N allocates N MB and HOLDS it in
// $GLOBALS so the allocation persists across requests in a persistent worker
// (simulating a leak for guardian tests). Also reports the effective PHP
// heap limit so tests can prove `-d memory_limit` is enforced.
$mb = isset($_GET["mb"]) ? max(0, (int)$_GET["mb"]) : 0;
if ($mb > 0) {
  $GLOBALS["phpx_mem_hog"] = str_repeat("x", $mb * 1024 * 1024);
}
$held = isset($GLOBALS["phpx_mem_hog"]) ? strlen($GLOBALS["phpx_mem_hog"]) : 0;
echo "mem_ok|mb=" . $mb . "|held=" . $held . "|heap_limit=" . ini_get("memory_limit");
