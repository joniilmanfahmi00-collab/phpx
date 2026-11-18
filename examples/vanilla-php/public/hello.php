<?php
$rid = getenv("PHPX_REQUEST_ID");
if ($rid === false) { $rid = "direct"; }
echo "hello direct php|rid=" . $rid . "|path=" . getenv("PHPX_PATH");
