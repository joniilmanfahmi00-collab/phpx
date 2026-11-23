<?php
// P1-A state-leakage probe: $GLOBALS set by app code persists across requests
// on a reused persistent worker (documented limit), but resets every time in
// one-shot. (Named functions/static locals would fatal on re-include — that
// is a separate app-compat limit, not probed here.)
$GLOBALS["phpx_leak_g"] = ($GLOBALS["phpx_leak_g"] ?? 0) + 1;
echo "global=" . $GLOBALS["phpx_leak_g"];
