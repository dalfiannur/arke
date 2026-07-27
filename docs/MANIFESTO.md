# Manifesto

> Identitas dan keyakinan Arke. Dokumen terpendek dan paling stabil di repo ini. Jika sesuatu di sini berubah, hampir semua hal lain ikut berubah.

## Apa ini

Arke adalah pustaka Entity-Component-System standalone untuk Rust — cara berkinerja tinggi dan deterministik untuk menyimpan, mengueri, dan mengembangkan data berbasis entitas, tanpa terikat pada game engine mana pun.

## Untuk siapa

Developer Rust yang perlu mengelola banyak objek yang berbeda-beda secara terstruktur — game, simulasi, tooling, atau backend — dan tidak mau memilih antara *ergonomi*, *kecepatan*, atau *reproducibility*. Alat yang ada memaksa kompromi: minimalis tetapi miskin fitur, kaya fitur tetapi terkunci ke satu engine, atau cepat tetapi rumit dipakai. Belum ada yang menjadikan determinisme dan "ergonomis = cepat" sebagai janji inti sekaligus tetap standalone.

## Apa yang kami yakini

1. Jalur yang paling nyaman ditulis **harus** menjadi jalur yang paling cepat dijalankan. Ergonomi dan performa bukan trade-off — bila keduanya bertabrakan, itu cacat desain kami, bukan pilihan pengguna.
2. Hasil komputasi harus **deterministik dan dapat direproduksi** — apa pun jumlah thread atau urutan penjadwalan.
3. Sebuah ECS adalah struktur data, bukan framework. Ia melayani program *milikmu*, bukan sebaliknya.

## Apa yang kami tolak

- Ketergantungan pada game engine, renderer, atau runtime tertentu sebagai syarat untuk memakainya.
- "Fast path" yang hanya dapat diakses lewat API tak-aman (`unsafe`) atau bentuk yang tidak wajar.
- Paralelisme yang menukar determinisme secara diam-diam demi angka benchmark.

## Janji

Data entitasmu selalu milikmu: dapat di-snapshot, diserialisasi ke format terbuka, dan direkonstruksi persis di mesin lain. API yang aman selalu menjadi API yang cepat — kamu tidak perlu turun ke `unsafe` untuk mendapatkan performa. Dan hasil yang sama akan kamu peroleh setiap kali, berapa pun thread yang berjalan.

---

Manifesto ini memandu Vision, Philosophy, dan setiap keputusan arsitektur. Perubahan padanya memerlukan tinjauan tertinggi (lihat [ADR-0001](ADR/ADR-0001-documentation-first.md)).
