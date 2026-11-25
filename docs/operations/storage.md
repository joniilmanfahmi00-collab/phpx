# Storage PHPX (M5 fase 3)

Status: fase 3 SELESAI — filesystem lokal nyata, prototipe S3 (config nyata, transport ditunda).
Session tetap memakai session PHP konvensional; ini object storage untuk file aplikasi.

SPEC 7.4: filesystem lokal dulu, S3-kompatibel kemudian.

## Driver

- `local`: file di bawah `[storage] root` (mis. `./data/storage`).
  Parent otomatis dibuat. Path dikanonikalisasi + cek `starts_with` root,
  traversal (`..`, absolut, NUL, segmen kosong) ditolak dengan exit 2.
  `list` menyusuri root, dibatasi maks 1000, terurut. Bagus untuk baseline satu-node.
- `s3`: prototipe. Config (`endpoint` + `bucket` + `region`) divalidasi nyata;
  transport ditunda — setiap op data mengembalikan
  `not implemented: s3 storage prototype deferred ... use driver=local` yang actionable.
  `status` jalan (driver `s3-prototype`, 0 objek) agar operator bisa memvalidasi config.
  Butuh jaringan + kredensial + justifikasi integrasi sebelum transport nyata.

## Semantik (SPEC 7.4)

- Terbatas: path 1-512 char, segmen `[A-Za-z0-9 . _ - @]`, hanya pemisah `/`,
  objek maks 4MB, list maks 1000. Penuh/kelebihan = error (backpressure, tidak pernah tanpa batas).
- Operasi: `put` / `get` / `delete` / `list` / `status`.
- Ack tahan-lama: `put` lokal difsync via tulis OS; `status` menghitung dari penyusuran disk.
- Exactly-once TIDAK dijanjikan; konsumen wajib idempoten.

## CLI (fase 3 nyata)

- `phpx storage status [--json]` — driver, objek, byte, root.
- `phpx storage put --path docs/hello.txt --value hi [--json]`
- `phpx storage get --path docs/hello.txt [--json]` — miss exit 0 + catatan.
- `phpx storage delete --path docs/hello.txt [--json]`
- `phpx storage list [--prefix docs/] [--json]` — terbatas, terurut.
- Prototipe S3: `status` exit 0, op data exit 2 dengan catatan ditunda.
- Path/objek buruk = exit 2 dengan pesan actionable.

## Config

```toml
[storage]
driver = "local" # local | s3 (prototipe)
root = "./data/storage" # wajib bila driver=local
# Prototipe S3 (driver=s3):
# endpoint = "https://s3.example"
# bucket = "phpx-dev"
# region = "auto"
```

Validasi (`phpx-config`): driver tak dikenal exit 2, local tanpa `root`
exit 2, s3 tanpa `endpoint` + `bucket` exit 2.

## Pengujian

- `crates/phpx-storage/tests/contract.rs` (driver local):
  put/get/list, tahan-dibuka-ulang, menolak traversal.
- `cargo test -p phpx-storage` hijau (2 unit + 3 kontrak).
- CLI: `storage status` exit 0, `--json` punya `objects`.

## Ditunda (M5 lanjutan)

- Transport S3 nyata (SigV4, multipart, retry) dengan justifikasi integrasi.
- Middleware storage terintegrasi gateway.
- Runbook backup/restore untuk root tahan-lama.
