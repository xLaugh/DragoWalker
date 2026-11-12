//! Clés secrètes (clés API de l'assistant IA) chiffrées avec la protection des données de Windows (DPAPI,
//! CryptProtectData) : seul ce compte Windows, sur ce PC, peut les relire. Dans settings.json, une clé est rangée
//! sous « <nom>_dpapi » (octets chiffrés, en hexadécimal) ; l'ancienne valeur en clair « <nom> » est convertie puis
//! retirée au premier chargement.

/// Chiffre un texte pour ce compte Windows. None si le chiffrement échoue (ou hors de Windows).
pub fn protect(plain: &str) -> Option<String> {
    let out = dpapi(plain.as_bytes(), true)?;
    Some(out.iter().map(|b| format!("{b:02x}")).collect())
}

/// Déchiffre un texte produit par `protect` (même compte Windows, même PC).
pub fn unprotect(hex: &str) -> Option<String> {
    if hex.len() % 2 != 0 || hex.is_empty() {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect();
    let plain = dpapi(&bytes?, false)?;
    String::from_utf8(plain).ok()
}

#[cfg(windows)]
fn dpapi(data: &[u8], encrypt: bool) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY : pointeurs valides pendant l'appel ; le tampon de sortie est alloué par Windows et libéré par LocalFree
    unsafe {
        let ok = if encrypt {
            CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        };
        if ok == 0 || output.pbData.is_null() {
            return None;
        }
        let v = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        Some(v)
    }
}

#[cfg(not(windows))]
fn dpapi(_data: &[u8], _encrypt: bool) -> Option<Vec<u8>> {
    None
}

/// Clé enregistrée sous `name` : la version chiffrée, sinon l'ancienne en clair (aussitôt chiffrée et retirée).
pub fn load_secret(name: &str) -> String {
    let s = crate::app::load_settings();
    if let Some(v) = s[format!("{name}_dpapi")].as_str().and_then(unprotect) {
        return v;
    }
    match s[name].as_str() {
        Some(plain) if !plain.is_empty() => {
            let plain = plain.to_string();
            save_secret(name, &plain);
            plain
        }
        _ => String::new(),
    }
}

/// Enregistre une clé chiffrée (vide : la retire). Si le chiffrement est impossible, elle n'est pas enregistrée.
pub fn save_secret(name: &str, value: &str) {
    let value = value.trim();
    crate::app::remove_setting(name);
    if value.is_empty() {
        crate::app::remove_setting(&format!("{name}_dpapi"));
        return;
    }
    if let Some(enc) = protect(value) {
        crate::app::save_setting(&format!("{name}_dpapi"), serde_json::json!(enc));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chiffrement_aller_retour() {
        let k = "sk-ant-api03-exemple-de-clé-éèà";
        let enc = protect(k).expect("chiffrement");
        assert!(!enc.contains("sk-ant"), "la clé ne doit pas apparaître en clair");
        assert!(enc.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(unprotect(&enc).as_deref(), Some(k));
        // deux chiffrements du même texte diffèrent (sel aléatoire de DPAPI) mais se relisent tous les deux
        let enc2 = protect(k).unwrap();
        assert_ne!(enc, enc2);
        assert_eq!(unprotect(&enc2).as_deref(), Some(k));
        // données abîmées : rien
        assert!(unprotect("zz").is_none() && unprotect("abc").is_none() && unprotect("").is_none());
        assert!(unprotect(&enc[..enc.len() - 4]).is_none());
    }
}
