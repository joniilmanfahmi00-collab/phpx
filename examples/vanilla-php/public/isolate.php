<?php
// Isolation probe: each one-shot process starts clean.
// If this file ever saw a previous $leaked value, it would echo LEAK.
if (isset($GLOBALS["leaked"])) { echo "LEAK"; exit(0); }
$GLOBALS["leaked"] = "yes";
$rid = getenv("PHPX_REQUEST_ID");
if ($rid === false) { $rid = "direct"; }
echo "clean|rid=" . $rid;
