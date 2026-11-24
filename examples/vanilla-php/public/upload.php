<?php
// PHPX multipart fixture: dumps $_POST/$_FILES populated by the SAPI shim
// (bootstrap.php one-shot, worker-persistent.php persistent). Text lines for
// easy substring assertions; file bytes read via tmp_name (move_uploaded_file
// stays false under CLI SAPI by design).
$rid = getenv("PHPX_REQUEST_ID");
if ($rid === false) { $rid = "direct"; }
header("X-Phpx-Request: " . $rid);
$out = [];
$out[] = "upload-fixture";
$out[] = "post_note=" . ($_POST["note"] ?? "");
$out[] = "post_tags=" . json_encode($_POST["tags"] ?? null);
$out[] = "post_meta=" . json_encode($_POST["meta"] ?? null);
if (isset($_FILES["doc"]) && is_array($_FILES["doc"])) {
    $out[] = "doc_name=" . ($_FILES["doc"]["name"] ?? "");
    $out[] = "doc_type=" . ($_FILES["doc"]["type"] ?? "");
    $out[] = "doc_size=" . ($_FILES["doc"]["size"] ?? "");
    $out[] = "doc_error=" . ($_FILES["doc"]["error"] ?? "");
    $tmp = $_FILES["doc"]["tmp_name"] ?? "";
    $out[] = "doc_content=" . (($tmp !== "" && is_file($tmp)) ? file_get_contents($tmp) : "");
} else {
    $out[] = "doc_missing";
}
if (isset($_FILES["docs"]["name"]) && is_array($_FILES["docs"]["name"])) {
    $names = $_FILES["docs"]["name"];
    $out[] = "docs_count=" . count($names);
    foreach (array_keys($names) as $i) {
        $t = $_FILES["docs"]["tmp_name"][$i] ?? "";
        $out[] = "docs_$i=" . ($names[$i] ?? "") . ":" . (($t !== "" && is_file($t)) ? file_get_contents($t) : "");
    }
}
if (isset($_FILES["up"]["name"]["file"])) {
    $t = $_FILES["up"]["tmp_name"]["file"] ?? "";
    $out[] = "up_name=" . ($_FILES["up"]["name"]["file"] ?? "");
    $out[] = "up_content=" . (($t !== "" && is_file($t)) ? file_get_contents($t) : "");
}
$raw = function_exists("phpx_input") ? phpx_input() : "";
$out[] = "raw_len=" . strlen((string)$raw);
echo implode("|", $out);
