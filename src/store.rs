use crate::crypto::{self, KEY_LEN};
use crate::local;
use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

type Values = BTreeMap<String, serde_yaml::Value>;

const SERVICE: &str = "envy";

pub struct LoadedLocal {
    pub values: Values,
    #[allow(dead_code)]
    pub encrypted: bool,
}

pub fn project_id(project_dir: &Path) -> String {
    let canonical = project_dir.to_string_lossy().to_lowercase();
    let digest = Sha256::digest(canonical.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn entry(project_dir: &Path) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, &project_id(project_dir))?)
}

fn load_key(project_dir: &Path) -> Result<[u8; KEY_LEN]> {
    match entry(project_dir)?.get_password() {
        Ok(hex) => crypto::key_from_hex(&hex),
        Err(keyring::Error::NoEntry) => Err(anyhow!(
            "no encryption key found in the OS keystore for this project — was it ever locked?"
        )),
        Err(err) => Err(anyhow!("keystore error: {err}")),
    }
}

pub fn key_exists(project_dir: &Path) -> bool {
    match entry(project_dir) {
        Ok(entry) => matches!(entry.get_password(), Ok(_)),
        Err(_) => false,
    }
}

pub fn load(path: &Path) -> Result<LoadedLocal> {
    if !path.is_file() {
        return Ok(LoadedLocal {
            values: BTreeMap::new(),
            encrypted: false,
        });
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    if let Some(blob) = crypto::split_encrypted(&text) {
        let dir = path.parent().unwrap_or(Path::new("."));
        let key = load_key(dir)?;
        let plain = crypto::unseal(&key, blob)?;
        let values = local::parse_str(&plain)
            .context("decrypting succeeded but the plaintext inside is invalid YAML")?;
        return Ok(LoadedLocal {
            values,
            encrypted: true,
        });
    }
    Ok(LoadedLocal {
        values: local::parse_str(&text)?,
        encrypted: false,
    })
}

/// Save values to the same shape the file already has: encrypted stays encrypted.
pub fn save_smart(path: &Path, values: &Values) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    if path.is_file() && looks_encrypted(path)? {
        save_encrypted(path, values)?;
    } else if key_exists(dir) {
        save_encrypted(path, values)?;
    } else {
        local::save(path, values)?;
    }
    Ok(())
}

fn looks_encrypted(path: &Path) -> Result<bool> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    Ok(crypto::split_encrypted(&text).is_some())
}

fn serialize_plain(values: &Values) -> Result<String> {
    let wrapper = local::LocalFileRef { values };
    let body = serde_yaml::to_string(&wrapper)?;
    Ok(body.trim_start_matches("---\n").trim_end().to_string())
}

pub fn save_encrypted(path: &Path, values: &Values) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let key = load_key(dir)?;
    let plain = serialize_plain(values)?;
    let blob = crypto::seal(&key, &plain)?;
    std::fs::write(path, crypto::wrap_file(&blob))
        .with_context(|| format!("writing {}", path.display()))
}

pub fn lock(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("{} does not exist yet — nothing to lock", path.display());
    }
    let text = std::fs::read_to_string(path)?;
    if crypto::split_encrypted(&text).is_some() {
        bail!("already locked — envy.local.yaml is encrypted at rest");
    }
    let values = local::parse_str(&text)?;

    let dir = path.parent().unwrap_or(Path::new("."));
    let key = crypto::generate_key();
    entry(dir)?
        .set_password(&crypto::key_to_hex(&key))
        .map_err(|err| anyhow!("could not write key to the OS keystore: {err}"))?;

    let plain = serialize_plain(&values)?;
    let blob =
        crypto::seal(&key, &plain).map_err(|err| anyhow!("encryption failed: {err}"))?;
    std::fs::write(path, crypto::wrap_file(&blob))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn unlock(path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let blob = crypto::split_encrypted(&text)
        .ok_or_else(|| anyhow!("not locked — envy.local.yaml is already plaintext"))?;

    let dir = path.parent().unwrap_or(Path::new("."));
    let key = load_key(dir)?;
    let plain = crypto::unseal(&key, blob)?;
    local::save(path, &local::parse_str(&plain)?)?;

    if let Err(err) = entry(dir)?.delete_credential() {
        if !matches!(err, keyring::Error::NoEntry) {
            eprintln!("warning: could not remove keystore entry ({err})");
        }
    }
    Ok(())
}
