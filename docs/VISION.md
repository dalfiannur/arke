# Vision

> Masa depan yang ingin diciptakan Arke. Lebih konkret dari Manifesto, tetapi masih bebas dari detail implementasi.

## Keadaan dunia yang kami tuju

Membangun sistem berbasis entitas di Rust tidak lagi berarti memilih antara pustaka minimalis yang cepat tumbuh rumit, atau game engine besar yang memaksakan seluruh arsitekturnya. Arke membuat ECS berperforma tinggi dan deterministik menjadi *default yang membosankan* — sebuah dependensi kecil yang bisa kamu percaya, seperti halnya sebuah struktur data standar.

## Bagi pengguna

Seorang **developer Rust** seharusnya dapat:

1. Menyimpan dan mengueri jutaan entitas dengan iterasi cache-friendly tanpa menulis kode `unsafe`.
2. Menjalankan sistem secara paralel dan memperoleh hasil yang bit-identik dengan eksekusi single-thread.
3. Meng-snapshot seluruh keadaan dunia ke format terbuka lalu memulihkannya persis — untuk save, replay, netcode, atau pengujian.
4. Menambahkan pustaka ini ke proyek apa pun tanpa menyeret game engine atau runtime async.

## Tanda keberhasilan

| Horizon | Seperti apa keberhasilannya |
| --- | --- |
| Jangka pendek | Core storage + query minimal berjalan, teruji, dengan pesan error yang mengajari. |
| Jangka menengah | Scheduler deterministik + serialisasi snapshot tersedia; dipakai pada ≥1 proyek nyata. |
| Jangka panjang | Menjadi pilihan default untuk ECS standalone di Rust ketika reproducibility penting. |

## Apa yang berubah, apa yang tetap

- **Berubah**: backend penyimpanan, strategi paralelisme, dan permukaan API yang ergonomis akan terus berevolusi.
- **Tetap**: determinisme, kepemilikan/portabilitas data, dan invarian "ergonomis = cepat" — hal yang harus benar meski segalanya berubah (lihat [ARCHITECTURE_BIBLE](ARCHITECTURE_BIBLE.md) §2).

---

Vision menjelaskan *ke mana*; [Philosophy](PHILOSOPHY.md) menjelaskan *bagaimana kami memutuskan* di sepanjang jalan.
