use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::RngCore;

pub const MAGIC: &str = "ENVY-ENCRYPTED-V1";
const NONCE_LEN: usize = 12;
pub const KEY_LEN: usize = 32;
const AAD: &[u8] = b"envy-local-store-v1";

pub fn generate_key() -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    rand::rngs::OsRng.fill_bytes(&mut key);
    key
}

pub fn key_to_hex(key: &[u8; KEY_LEN]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn key_from_hex(hex: &str) -> Result<[u8; KEY_LEN]> {
    let bytes = hex.as_bytes();
    if bytes.len() != KEY_LEN * 2 {
        return Err(anyhow!("stored key has wrong length"));
    }
    let mut key = [0u8; KEY_LEN];
    for i in 0..KEY_LEN {
        key[i] = u8::from_str_radix(
            std::str::from_utf8(&bytes[i * 2..i * 2 + 2]).context("bad hex in stored key")?,
            16,
        )
        .context("bad hex in stored key")?;
    }
    Ok(key)
}

fn cipher_for(key: &[u8; KEY_LEN]) -> Result<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("bad key length"))
}

pub fn seal(key: &[u8; KEY_LEN], plaintext: &str) -> Result<String> {
    use aes_gcm::AeadInPlace;
    let cipher = cipher_for(key)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let mut buffer = plaintext.as_bytes().to_vec();
    cipher
        .encrypt_in_place(nonce, AAD, &mut buffer)
        .map_err(|_| anyhow!("encryption failed"))?;

    let mut packed = Vec::with_capacity(NONCE_LEN + buffer.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&buffer);
    Ok(B64.encode(packed))
}

pub fn unseal(key: &[u8; KEY_LEN], blob: &str) -> Result<String> {
    use aes_gcm::AeadInPlace;
    let packed = B64
        .decode(blob.trim())
        .context("encrypted payload is not valid base64")?;
    if packed.len() < NONCE_LEN {
        return Err(anyhow!("encrypted payload too short"));
    }
    let (nonce_bytes, ciphertext) = packed.split_at(NONCE_LEN);
    let cipher = cipher_for(key)?;
    let mut buffer = ciphertext.to_vec();
    cipher
        .decrypt_in_place(Nonce::from_slice(nonce_bytes), AAD, &mut buffer)
        .map_err(|_| anyhow!("decryption failed — wrong key or tampered file"))?;
    String::from_utf8(buffer).context("decrypted payload is not valid UTF-8")
}

pub fn wrap_file(body_blob: &str) -> String {
    format!("# Managed by envy — ENCRYPTED local store. NEVER commit this file.\n{MAGIC}\n{body_blob}\n")
}

pub fn split_encrypted(text: &str) -> Option<&str> {
    let mut lines = text.lines();
    let _comment = lines.next()?;
    let magic = lines.next()?;
    if magic.trim() != MAGIC {
        return None;
    }
    lines.next().map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = generate_key();
        let sealed = seal(&key, "STRIPE_SECRET_KEY: sk_live_whatever\n").expect("seal");
        let opened = unseal(&key, &sealed).expect("unseal");
        assert_eq!(opened, "STRIPE_SECRET_KEY: sk_live_whatever\n");
    }

    #[test]
    fn wrong_key_fails() {
        let sealed = seal(&generate_key(), "top secret").expect("seal");
        assert!(unseal(&generate_key(), &sealed).is_err());
    }

    #[test]
    fn hex_roundtrip() {
        let key = generate_key();
        assert_eq!(key_from_hex(&key_to_hex(&key)).expect("hex"), key);
    }

    #[test]
    fn file_wrapper_detection() {
        let body = wrap_file("AAAA");
        assert_eq!(split_encrypted(&body), Some("AAAA"));
        assert_eq!(split_encrypted("values:\n  A: b\n"), None);
    }
}
