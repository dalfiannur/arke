# Philosophy

> Prinsip untuk keputusan produk dan desain. Ketika dua opsi sama-sama masuk akal secara teknis, prinsip inilah yang memutuskan.

## Prinsip

### 1. Jalur aman adalah jalur cepat

API yang paling idiomatik dan aman harus ter-*compile* menjadi jalur panas yang optimal. Kecepatan bukan hadiah untuk yang berani memakai `unsafe`.

**Kapan berlaku:** setiap keputusan desain API — query, iterasi, mutasi komponen.
**Konsekuensi:** jalur panas memakai monomorfisasi/static dispatch tanpa boxing, `dyn`, atau alokasi tersembunyi. Bila sebuah fitur nyaman hanya bisa cepat lewat `unsafe`, desainnya belum selesai.

### 2. Deterministik lebih dulu, cepat kemudian

Reproducibility adalah kontrak, bukan mode opsional. Kecepatan dikejar di dalam batas determinisme, bukan sebaliknya.

**Kapan berlaku:** penjadwalan sistem, urutan iterasi, alokasi entity id.
**Konsekuensi:** urutan iterasi dan alokasi id harus stabil dan hanya bergantung pada urutan operasi. Paralelisme hanya diizinkan bila terbukti menghasilkan keadaan akhir yang setara dengan eksekusi serial.

### 3. Pesan error mengajari, bukan menyalahkan

Ketika sesuatu salah, pengguna harus tahu *apa* dan *bagaimana memperbaikinya* — idealnya sebelum program berjalan.

**Kapan berlaku:** konflik borrow query, komponen tak terdaftar, akses entity tak valid.
**Konsekuensi:** error runtime menyebut entity/komponen/sistem yang terlibat dan menyarankan perbaikan; kegagalan yang bisa dideteksi saat *compile* tidak boleh lolos ke runtime.

## Trade-off yang kami pilih secara sadar

| Kami utamakan | Di atas | Alasan |
| --- | --- | --- |
| Determinisme | Throughput puncak absolut | Reproducibility adalah janji inti; kecepatan dikejar *di dalam* batas itu. |
| Kejelasan API | Keringkasan / kepintaran | Developer umum harus benar sejak percobaan pertama. |
| Dependensi kecil | Kenyamanan fitur bawaan | Sifat standalone adalah alasan pustaka ini ada. |

## Tanda keputusan yang buruk

- Jalur cepat hanya bisa dicapai lewat `unsafe` dari sisi pengguna.
- Hasil berubah antar-run atau antar jumlah thread.
- Error hanya berupa `panic` tanpa konteks entity/komponen/sistem.
- Sebuah fitur menyeret dependensi berat (engine, runtime async, I/O) ke dalam core.

---

Filosofi memandu penilaian sehari-hari. Ketika sebuah keputusan besar dan konsekuensial, ia harus melewati **Architecture decision test** di [ARCHITECTURE_BIBLE](ARCHITECTURE_BIBLE.md).
