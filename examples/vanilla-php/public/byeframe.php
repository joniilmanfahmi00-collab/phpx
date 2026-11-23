<?php
// P1-B: apps that terminate the process (exit/die, e.g. WordPress redirect
// and REST paths) kill a persistent worker outright. The supervisor must
// turn that into a 500 + restart with a new incarnation, then serve the next
// request cleanly (stream resync). One-shot captures the flushed output as
// the body instead (different contract, same app code).
echo "about-to-exit";
exit(0);
