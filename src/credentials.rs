use anyhow::Result;

pub fn resolve_github_token_with(
    env_token: Option<String>,
    gh_token: Option<String>,
) -> Result<String> {
    if let Some(t) = env_token.filter(|s| !s.is_empty()) {
        return Ok(t);
    }
    if let Some(t) = gh_token.filter(|s| !s.is_empty()) {
        return Ok(t);
    }
    anyhow::bail!(
        "fp: no GitHub credentials found.\n  Option 1: export GITHUB_TOKEN=<token>\n  Option 2: gh auth login"
    )
}

pub fn resolve_github_token() -> Result<String> {
    let env_token = std::env::var("GITHUB_TOKEN").ok();
    let gh_token = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());
    resolve_github_token_with(env_token, gh_token)
}

#[cfg(target_os = "macos")]
pub fn derive_chrome_aes_key(password: &[u8]) -> [u8; 16] {
    use hmac::Hmac;
    use sha1::Sha1;
    let mut key = [0u8; 16];
    pbkdf2::pbkdf2::<Hmac<Sha1>>(password, b"saltysalt", 1003, &mut key);
    key
}

#[cfg(target_os = "macos")]
pub fn decrypt_chrome_cookie(encrypted: &[u8], key: &[u8; 16]) -> Result<String> {
    use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
    use aes::Aes128;
    if encrypted.len() < 3 + 16 { anyhow::bail!("encrypted value too short"); }
    let iv: [u8; 16] = [b' '; 16];
    let ciphertext = &encrypted[3..];
    if !ciphertext.len().is_multiple_of(16) { anyhow::bail!("ciphertext not block-aligned"); }
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut plaintext = ciphertext.to_vec();
    for i in (0..plaintext.len()).step_by(16) {
        let prev: [u8; 16] = if i == 0 { iv } else { ciphertext[i-16..i].try_into()? };
        let block = GenericArray::from_mut_slice(&mut plaintext[i..i+16]);
        cipher.decrypt_block(block);
        for j in 0..16 { plaintext[i+j] ^= prev[j]; }
    }
    let pad = *plaintext.last().ok_or_else(|| anyhow::anyhow!("empty plaintext"))? as usize;
    if pad == 0 || pad > 16 || pad > plaintext.len() { anyhow::bail!("invalid PKCS7 padding: pad={}", pad); }
    plaintext.truncate(plaintext.len() - pad);
    if plaintext.len() > 32 {
        plaintext.drain(..32);
    }
    Ok(String::from_utf8(plaintext)?)
}

#[cfg(target_os = "macos")]
pub fn read_chrome_user_session_encrypted(db_path: &std::path::Path) -> Result<Vec<u8>> {
    let conn = rusqlite::Connection::open(db_path)?;
    let blob: Vec<u8> = conn.query_row(
        "SELECT encrypted_value FROM cookies WHERE host_key LIKE '%github.com' AND name = 'user_session' LIMIT 1",
        [],
        |row| row.get(0),
    ).map_err(|e| anyhow::anyhow!("user_session cookie not found in Chrome cookies DB: {}", e))?;
    Ok(blob)
}

#[cfg(target_os = "macos")]
fn get_chrome_safe_storage_password() -> Result<String> {
    use security_framework::passwords::get_generic_password;
    let bytes = get_generic_password("Chrome Safe Storage", "Chrome")
        .map_err(|e| anyhow::anyhow!("failed to read Chrome Safe Storage from Keychain: {}", e))?;
    Ok(String::from_utf8(bytes)?)
}

#[cfg(target_os = "macos")]
pub fn extract_github_session_from_browser_with_chrome_db(chrome_db: &std::path::Path) -> Result<String> {
    if chrome_db.exists() {
        let password = get_chrome_safe_storage_password()?;
        let key = derive_chrome_aes_key(password.as_bytes());
        let encrypted = read_chrome_user_session_encrypted(chrome_db)?;
        return decrypt_chrome_cookie(&encrypted, &key);
    }
    anyhow::bail!("no GitHub session found — set GITHUB_USER_SESSION env var or log into GitHub in Chrome")
}

#[cfg(not(target_os = "macos"))]
pub fn extract_github_session_from_browser_with_chrome_db(_chrome_db: &std::path::Path) -> Result<String> {
    anyhow::bail!("no GitHub session found — set GITHUB_USER_SESSION env var or log into GitHub in Chrome")
}
