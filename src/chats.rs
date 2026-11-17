//! Conversations de l'assistant IA gardées sur ce PC (%APPDATA%\DragoWalker\conversations) : une par fichier
//! JSON, avec ses images dans un dossier du même nom. Liste, reprise, suppression ; images jointes (fichier,
//! glisser-déposer, presse-papiers, capture de la vue) mises en PNG de 1568 px au plus.

use crate::ai::ChatLine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Côté maximal d'une image envoyée à l'IA (px) : au-delà, les API la réduisent elles-mêmes.
const MAX_SIDE: u32 = 1568;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created: u64,
    pub updated: u64,
    pub lines: Vec<ChatLine>,
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os("DW_CHATS_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("DragoWalker").join("conversations")
}

/// Nouvel identifiant (ordre de création).
pub fn new_id() -> String {
    format!("c{}_{:04x}", now(), crate::cwbridge::unique() as u16)
}

fn clean_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
}

/// Titre d'une conversation : début du premier message de la personne.
pub fn title_of(lines: &[ChatLine]) -> String {
    let t = lines.iter().find(|l| l.from_user).map(|l| l.text.trim().replace('\n', " ")).unwrap_or_default();
    let t = if t.is_empty() && lines.iter().any(|l| !l.images.is_empty()) { "Image".to_string() } else { t };
    let mut s: String = t.chars().take(60).collect();
    if t.chars().count() > 60 {
        s.push('…');
    }
    s
}

/// Enregistre la conversation (rien tant que la personne n'a rien écrit).
pub fn save(id: &str, lines: &[ChatLine]) -> Result<(), String> {
    if !lines.iter().any(|l| l.from_user) {
        return Ok(());
    }
    let d = dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let p = d.join(format!("{}.json", clean_id(id)));
    let created = load(id).map(|c| c.created).unwrap_or_else(now);
    let c = Conversation { id: id.to_string(), title: title_of(lines), created, updated: now(), lines: lines.to_vec() };
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

pub fn load(id: &str) -> Option<Conversation> {
    let s = std::fs::read_to_string(dir().join(format!("{}.json", clean_id(id)))).ok()?;
    serde_json::from_str(&s).ok()
}

/// Conversations enregistrées, la plus récente d'abord (sans leurs messages).
pub fn list() -> Vec<Conversation> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir()).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().map(|x| x == "json").unwrap_or(false) {
            if let Some(mut c) = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str::<Conversation>(&s).ok()) {
                let n = c.lines.iter().filter(|l| l.from_user).count();
                c.lines.clear();
                c.created = n as u64; // nombre de messages de la personne (pour la liste)
                out.push(c);
            }
        }
    }
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    out
}

pub fn delete(id: &str) {
    let id = clean_id(id);
    let _ = std::fs::remove_file(dir().join(format!("{id}.json")));
    let _ = std::fs::remove_dir_all(dir().join(&id));
}

/// « il y a 5 min », « il y a 3 h », « il y a 2 j », sinon la date.
pub fn ago(t: u64) -> String {
    let d = now().saturating_sub(t);
    match d {
        0..=59 => "à l'instant".into(),
        60..=3599 => format!("il y a {} min", d / 60),
        3600..=86399 => format!("il y a {} h", d / 3600),
        86400..=604799 => format!("il y a {} j", d / 86400),
        _ => {
            // date (UTC) : jours depuis 1970 → année, mois, jour (algorithme des dates civiles)
            let z = (t / 86400) as i64 + 719468;
            let era = z.div_euclid(146097);
            let doe = z - era * 146097;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let day = doy - (153 * mp + 2) / 5 + 1;
            let month = if mp < 10 { mp + 3 } else { mp - 9 };
            let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
            format!("{day:02}/{month:02}/{year}")
        }
    }
}

/// Range une image jointe dans le dossier de la conversation, en PNG (1568 px au plus). Rend son chemin.
pub fn store_image(id: &str, img: image::RgbaImage) -> Result<PathBuf, String> {
    let d = dir().join(clean_id(id));
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let (w, h) = img.dimensions();
    let img = if w.max(h) > MAX_SIDE {
        let k = MAX_SIDE as f32 / w.max(h) as f32;
        image::imageops::resize(&img, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1), image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut n = 1;
    let p = loop {
        let p = d.join(format!("image_{n}.png"));
        if !p.exists() {
            break p;
        }
        n += 1;
    };
    img.save(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Image d'un fichier (PNG, JPEG).
pub fn read_image(p: &Path) -> Result<image::RgbaImage, String> {
    let d = std::fs::read(p).map_err(|e| e.to_string())?;
    Ok(image::load_from_memory(&d).map_err(|e| format!("image illisible ({e}) : PNG ou JPEG"))?.to_rgba8())
}

/// Image du presse-papiers, s'il y en a une.
pub fn clipboard_image() -> Option<image::RgbaImage> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let i = cb.get_image().ok()?;
    image::RgbaImage::from_raw(i.width as u32, i.height as u32, i.bytes.into_owned())
}

/// Image en base64 (PNG), pour les API.
pub fn image_b64(p: &str) -> Option<String> {
    let d = std::fs::read(p).ok()?;
    Some(crate::mcp::b64(&d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titres_et_dates() {
        let l = vec![ChatLine { from_user: false, text: "Salut".into(), images: vec![] }, ChatLine { from_user: true, text: "Pose 10 bancs\nen ligne".into(), images: vec![] }];
        assert_eq!(title_of(&l), "Pose 10 bancs en ligne");
        assert_eq!(ago(0), "01/01/1970");
        assert_eq!(ago(1_700_000_000), "14/11/2023");
        assert_eq!(ago(now() - 120), "il y a 2 min");
    }

    #[test]
    fn enregistrer_relire_supprimer() {
        let d = std::env::temp_dir().join(format!("dw_test_chats_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::env::set_var("DW_CHATS_DIR", &d);
        let id = new_id();
        let img = store_image(&id, image::RgbaImage::from_pixel(3000, 1000, image::Rgba([1, 2, 3, 255]))).unwrap();
        assert_eq!(image::image_dimensions(&img).unwrap(), (1568, 522));
        let l = vec![ChatLine { from_user: false, text: "Salut".into(), images: vec![] }, ChatLine { from_user: true, text: "Regarde".into(), images: vec![img.to_string_lossy().to_string()] }];
        save(&id, &l).unwrap();
        let c = load(&id).unwrap();
        assert_eq!(c.lines.len(), 2);
        assert_eq!(c.lines[1].images.len(), 1);
        let ls = list();
        assert_eq!(ls.len(), 1);
        assert_eq!(ls[0].title, "Regarde");
        delete(&id);
        assert!(list().is_empty() && !img.exists());
        std::env::remove_var("DW_CHATS_DIR");
    }
}
