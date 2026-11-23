<?php
// P0-D regression probe: responses larger than the OS pipe buffer (~64KB)
// must not deadlock the supervisor (drained concurrently, not after exit).
echo str_repeat("x", 256 * 1024);
