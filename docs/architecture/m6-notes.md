# Catatan M6 PHPX - jalur eksperimental Embedded Runtime

Tujuan: memvalidasi backend runtime kedua tanpa melemahkan Process Runtime.
Status: stub M6.1 SELESAI (pelaporan kapabilitas + domain entri-tunggal + uji kontrak bersama).
Linking php-embed native ditunda ke M6 lanjutan (butuh Linux + dev lib php-embed + pin ABI).

## Stub M6.1 (SELESAI)

- Crate baru phpx-runtime-embedded: laporan EmbeddedCapabilities, penjaga EmbeddedDomain entri-tunggal, execute_stub.
- Aturan entri-tunggal: maksimal satu entri engine PHP dalam satu waktu; entri konkuren ditolak, tidak pernah diantrekan.
- Stub tidak pernah mengeksekusi PHP: memvalidasi envelope dulu, lalu mengembalikan error unavailable yang actionable.
- Gate config tidak berubah: mode=embedded tetap mewajibkan [runtime.embedded] enabled=true (eksperimental).
- CLI nyata: runtime list menampilkan experimental-disabled atau experimental-enabled plus available=false.
- CLI nyata: runtime inspect menyertakan objek embedded berisi enabled, available, single_threaded, max_concurrent_entries.
- Pengujian: 4 unit + 4 kontrak hijau (kapabilitas, entri-tunggal, roundtrip envelope, stub-never-executes).
- Dokumen: docs/operations/embedded.md. Contoh tetap process-first; embedded hanya opt-in.

## Yang berubah vs M5

- Workspace Cargo plus anggota crate phpx-runtime-embedded dan dependensi.
- phpx-cli bergantung pada phpx-runtime-embedded untuk output list dan inspect.
- roadmap.md: 5 kotak M6 dicentang dengan catatan stub M6.1; kotak linking native tetap terbuka.
- overview.md: baris lingkup M6.1 plus Future diperbarui menjadi observabilitas saja.

## Ditunda (M6 lanjutan)

- Build dan linking php-embed native dengan header ter-pin dan matriks ABI.
- Bridge C ABI sempit dengan aturan memori-lifecycle dan kepemilikan.
- Uji kendala NTS dan ZTS plus uji containment crash.
- Subset kompatibilitas Laravel dan WordPress lewat embedded.

Ketertelusuran: SPEC 3.1 backend kedua tercakup sebagai stub eksperimental oleh M6.1;
ekor SPEC 15 embedded tercakup sebagian (opt-in, kapabilitas, kontrak); eksekusi native masih terbuka.


## Pin ABI M6.2 (SELESAI)

- Modul baru phpx-runtime-embedded abi: nilai ter-pin AbiPin, kecocokan prefix supports_version, check dengan error actionable, describe untuk manusia.
- Pin: PHP 8.3/8.4/8.5, SAPI embed, header sapi/embed/php_embed.h + main/php.h + Zend/zend_API.h, lib Linux libphp.so + libphp8.so, pkg php-embed, hanya NTS, hanya non-debug.
- CLI: runtime list --json dan runtime inspect --json kini menyertakan embedded.abi dari AbiPin. Output manusia tidak berubah.
- Pengujian: 4 unit di modul abi + 1 uji kontrak baru abi_pin_contract. Windows dev PHP 8.3.30 ZTS ditolak oleh check by design.
- Link native tetap ditunda: butuh Linux + dev lib php-embed. Pin adalah kode, bukan link.


## Stub penuh M6 (SELESAI)

Semua butir kerja M6 selesai sebagai kode plus pengujian plus dokumen, dengan link
php-embed native tetap ditunda secara jujur ke dev lib Linux nyata.

- Butir 1 probe link: modul link dengan LinkProbe plus build_plan. found tetap false, tried plus how_to_enable bersifat actionable.
- Butir 2 pin ABI: modul abi dengan AbiPin, selesai di M6.2, kini diperluas dengan uji kontrak matriks.
- Butir 3 bridge sempit: modul bridge, 3 op diizinkan, header repr C, stub memvalidasi op plus envelope lalu melaporkan unlinked.
- Butir 4 kepemilikan: modul ownership, 5 aturan plus pelacak RequestScope.
- Domain: penjaga entri-tunggal EmbeddedDomain sejak M6.1, plus uji stres 8-thread yang membuktikan tepat 1 pemenang.
- Butir 6 NTS dan ZTS: uji kontrak matriks atas 8.3 sampai 8.5 plus versi lama; ZTS plus Debug selalu ditolak.
- Pelaporan kapabilitas plus kontrak bersama: sejak M6.1, JSON CLI kini menyertakan embedded.abi.
- Butir 9 kompat: modul compat dengan 4 subset yang memirror fixture M4; eksekusi ditunda, process tetap hijau.
- Butir 10 containment: modul containment dengan batas standar plus urutan terjaga; gateway tidak pernah meng-host engine live.
- Exit: opt-in eksperimental ya, kontrak hijau ya, tanpa entri konkuren arbitrer terbukti oleh uji stres, matriks plus limitasi dipublikasikan di embedded.md plus limitations.md, fallback process tetap default.
