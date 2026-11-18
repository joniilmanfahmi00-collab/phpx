<?php
// Crash fixture: non-zero exit, stderr diagnostic.
fwrite(STDERR, "boom from crash fixture");
exit(1);
