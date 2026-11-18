<?php
namespace App;
class Greeter {
    public static function hello(string $name): string {
        $name = trim($name) !== "" ? trim($name) : "composer";
        return "hello " . preg_replace("/[^a-zA-Z0-9 _.-]/", "", $name);
    }
}
