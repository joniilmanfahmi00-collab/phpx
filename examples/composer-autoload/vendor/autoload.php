<?php
// Minimal Composer-like autoloader stub (no network). Maps App\\ -> src/.
spl_autoload_register(function ($class) {
    $prefix = "App\\";
    if (strncmp($class, $prefix, strlen($prefix)) !== 0) { return; }
    $rel = str_replace("\\", "/", substr($class, strlen($prefix)));
    $file = dirname(__DIR__) . "/src/" . $rel . ".php";
    if (is_file($file)) { require $file; }
});
