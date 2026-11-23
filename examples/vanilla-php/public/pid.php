<?php
// P0-A probe: proves one-shot process lifetime is request-scoped.
// Each request runs in a fresh `php` process, so getmypid() must differ
// across sequential requests. Persistent mode (P0-B) would reuse PIDs.
echo "pid=" . getmypid();
