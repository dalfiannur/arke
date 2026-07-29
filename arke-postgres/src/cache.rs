//! Cache read-through per baris komponen (RFC-0033).
//!
//! Lapisan **transparan-performa** opsional di depan Postgres: `PgStore` membaca
//! baris komponen dari cache (hit) dan mengisi ke Postgres (miss); tulis
//! meng-*invalidate*. **Postgres tetap sumber kebenaran** — cache bukan otoritas.
//! Backend (Redis/DragonflyDB/KeyDB) hidup di crate `arke-cache`.
//!
//! Konsistensi: benar selama **semua tulis lewat store ber-cache**; tulis
//! luar-proses langsung ke Postgres dibatasi basi-nya oleh **TTL** (tanggung
//! jawab backend).

use async_trait::async_trait;

use crate::PgValue;

/// Cache baris komponen berkunci `(table, entity_id)` (RFC-0033). Batch-oriented
/// agar backend dapat MGET/MSET (efisien untuk bulk-load).
#[async_trait]
pub trait ComponentCache: Send + Sync {
    /// Ambil baris ter-cache untuk tiap `id` (urutan sama; `None` = miss).
    async fn get_many(&self, table: &str, ids: &[i64]) -> Vec<Option<Vec<u8>>>;
    /// Simpan baris `(id, bytes)` ke cache.
    async fn put_many(&self, table: &str, entries: &[(i64, Vec<u8>)]);
    /// Batalkan cache untuk `(table, ids)` (dipakai `save_incremental`/`update_entity`).
    async fn invalidate(&self, table: &str, ids: &[i64]);
    /// Kosongkan seluruh cache (dipakai `save` penuh — menulis-ulang semua).
    async fn clear(&self);
}

/// Encode baris komponen (`Vec<PgValue>`) → byte biner ringkas (0-dependensi).
pub(crate) fn encode_row(row: &[PgValue]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(row.len() as u32).to_le_bytes());
    for v in row {
        match v {
            PgValue::Int(i) => {
                out.push(0);
                out.extend_from_slice(&i.to_le_bytes());
            }
            PgValue::Float(f) => {
                out.push(1);
                out.extend_from_slice(&f.to_le_bytes());
            }
            PgValue::Bool(b) => {
                out.push(2);
                out.push(u8::from(*b));
            }
            PgValue::Text(s) => {
                out.push(3);
                push_str(&mut out, s);
            }
            PgValue::Json(s) => {
                out.push(4);
                push_str(&mut out, s);
            }
            PgValue::Numeric(s) => {
                out.push(5);
                push_str(&mut out, s);
            }
            PgValue::Null => out.push(6),
            // Relasi (RFC-0034 Am.3): cache menyimpan pid mentah (`Int`), jadi `Ref`
            // umumnya tak sampai sini; tetap di-encode demi ekshaustif.
            PgValue::Ref(i) => {
                out.push(7);
                out.extend_from_slice(&i.to_le_bytes());
            }
        }
    }
    out
}

fn push_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Decode byte → baris komponen; `None` bila korup → diperlakukan cache-miss.
pub(crate) fn decode_row(bytes: &[u8]) -> Option<Vec<PgValue>> {
    let mut p = 0usize;
    let n = read_u32(bytes, &mut p)? as usize;
    let mut row = Vec::with_capacity(n);
    for _ in 0..n {
        let tag = *bytes.get(p)?;
        p += 1;
        row.push(match tag {
            0 => PgValue::Int(i64::from_le_bytes(read_arr::<8>(bytes, &mut p)?)),
            1 => PgValue::Float(f64::from_le_bytes(read_arr::<8>(bytes, &mut p)?)),
            2 => {
                let b = *bytes.get(p)?;
                p += 1;
                PgValue::Bool(b != 0)
            }
            3 => PgValue::Text(read_str(bytes, &mut p)?),
            4 => PgValue::Json(read_str(bytes, &mut p)?),
            5 => PgValue::Numeric(read_str(bytes, &mut p)?),
            6 => PgValue::Null,
            7 => PgValue::Ref(i64::from_le_bytes(read_arr::<8>(bytes, &mut p)?)),
            _ => return None,
        });
    }
    Some(row)
}

fn read_u32(b: &[u8], p: &mut usize) -> Option<u32> {
    let a = read_arr::<4>(b, p)?;
    Some(u32::from_le_bytes(a))
}
fn read_arr<const N: usize>(b: &[u8], p: &mut usize) -> Option<[u8; N]> {
    let a: [u8; N] = b.get(*p..*p + N)?.try_into().ok()?;
    *p += N;
    Some(a)
}
fn read_str(b: &[u8], p: &mut usize) -> Option<String> {
    let len = read_u32(b, p)? as usize;
    let s = b.get(*p..*p + len)?;
    *p += len;
    String::from_utf8(s.to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip() {
        let row = vec![
            PgValue::Int(-42),
            PgValue::Float(1.5),
            PgValue::Bool(true),
            PgValue::Text("héllo".to_string()),
            PgValue::Numeric("999999999999".to_string()),
            PgValue::Null,
        ];
        let bytes = encode_row(&row);
        assert_eq!(decode_row(&bytes), Some(row));
    }

    #[test]
    fn decode_korup_jadi_none() {
        assert_eq!(decode_row(&[0xff]), None); // count minta 255 elemen, byte habis
        assert_eq!(decode_row(&[1, 0, 0, 0, 9]), None); // tag 9 tak dikenal
    }
}
