use aes::Aes128;
use base64::{engine::general_purpose, Engine as _};
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit};
use des::TdesEde3;
use rand::{distr::Alphanumeric, rng, RngExt as _};
use std::time::{SystemTime, UNIX_EPOCH};

type TdesCbcEnc = cbc::Encryptor<TdesEde3>;
type TdesCbcDec = cbc::Decryptor<TdesEde3>;
#[allow(dead_code)]
type Aes128EcbEnc = ecb::Encryptor<Aes128>;

pub struct CMCCCrypto {
    key_3des: Vec<u8>,
    iv_3des: Vec<u8>,
}

impl CMCCCrypto {
    pub fn new() -> Self {
        let key_3des = general_purpose::STANDARD
            .decode("SHV5VnI3N3BiVTdwOEFFX05PdHhjTWNj")
            .unwrap();
        let iv_3des = general_purpose::STANDARD.decode("QCMkJVJFV1E=").unwrap();
        Self { key_3des, iv_3des }
    }

    pub fn encrypt(&self, ip: &str) -> String {
        let mut buf = vec![0u8; ip.len() + 8];
        buf[..ip.len()].copy_from_slice(ip.as_bytes());
        let ct = TdesCbcEnc::new(
            self.key_3des.as_slice().into(),
            self.iv_3des.as_slice().into(),
        )
        .encrypt_padded_mut::<Pkcs7>(&mut buf, ip.len())
        .unwrap();
        general_purpose::STANDARD.encode(ct)
    }

    pub fn decrypt(&self, token: &str) -> String {
        let mut buf = match general_purpose::STANDARD.decode(token) {
            Ok(b) => b,
            Err(_) => return String::new(),
        };
        match TdesCbcDec::new(
            self.key_3des.as_slice().into(),
            self.iv_3des.as_slice().into(),
        )
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        {
            Ok(pt) => String::from_utf8_lossy(pt).into_owned(),
            Err(_) => String::new(),
        }
    }

    #[allow(dead_code)]
    pub fn encrypt_task_id(&self, data: &str) -> String {
        let start = std::time::Instant::now();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .to_string();

        let data_enc = urlencoding::encode(data);
        let raw = format!("{}${}", data_enc, ts);
        let raw_b64 = general_purpose::STANDARD.encode(raw);

        let rand_key: String = rng()
            .sample_iter(&Alphanumeric)
            .take(16)
            .map(|c| (c as char).to_ascii_lowercase())
            .collect();

        let mut buf = vec![0u8; raw_b64.len() + 16];
        buf[..raw_b64.len()].copy_from_slice(raw_b64.as_bytes());

        let ct = Aes128EcbEnc::new(rand_key.as_bytes().into())
            .encrypt_padded_mut::<Pkcs7>(&mut buf, raw_b64.len())
            .unwrap();
        let ct_b64 = general_purpose::STANDARD.encode(ct);

        let insert_pos = rng().random_range(1..=20).min(ct_b64.len());

        let mut p = String::with_capacity(ct_b64.len() + rand_key.len() + ts.len());
        p.push_str(&ct_b64[..insert_pos]);
        p.push_str(&rand_key);
        p.push_str(&ts);
        p.push_str(&ct_b64[insert_pos..]);

        // Optimized string reversal and transformation
        let c_str: String = p.chars().rev().collect();
        let g = format!("{},{}.{}", insert_pos, c_str, rand_key.len());

        // Optimized character rotating logic without Vec<char> if possible
        let mut v_bytes = g.into_bytes();
        if v_bytes.len() >= 3 {
            v_bytes.rotate_left(3);
        }

        let v_b64 = general_purpose::STANDARD.encode(&v_bytes);

        // Final transformation: reverse and shift characters
        let mut result = String::with_capacity(v_b64.len());
        for c in v_b64.chars().rev() {
            result.push((c as u8).wrapping_add(3) as char);
        }

        tracing::debug!("Task ID encrypted in {:?}", start.elapsed());
        result
    }
}
