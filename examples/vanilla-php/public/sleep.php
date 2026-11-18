<?php
// Sleep fixture for timeout tests.
$rid = getenv("PHPX_REQUEST_ID");
if ($rid === false) { $rid = "direct"; }
sleep(5);
echo "woke|rid=" . $rid;
