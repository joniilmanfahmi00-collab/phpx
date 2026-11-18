<?php
// PHPX M1 vanilla fixture: no framework, no changes needed for hosting.
$rid = getenv("PHPX_REQUEST_ID");
if ($rid === false) { $rid = "direct"; }
header("X-Phpx-Request: " . $rid);
echo "hello from phpx vanilla|rid=" . $rid;
