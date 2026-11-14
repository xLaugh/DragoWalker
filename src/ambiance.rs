//! Ambiances du projet (timecycle modifiers personnalisés) : copiées d'une ambiance du jeu (tous ses
//! modificateurs), puis réglées (luminosité, couleur et intensité de la lumière de l'intérieur, saturation…).
//! À l'export FiveM elles sont écrites dans `dw_timecycmod.xml`, au format des fichiers timecycle_mods_*.xml du jeu
//! (relu dans common.rpf\data\timecycle\timecycle_mods_1.xml et le MLO burgershot, bs_timecycmod.xml) :
//! `<timecycle_modifier_data version="1.000000"><modifier name=".." numMods=".." userFlags=".."><param>a b</param>…`.
//! Dans les fichiers du jeu, la seconde valeur vaut 1.000 pour les couleurs (…_col_r/g/b) et 0.000 pour le reste
//! (relevé sur tous les modificateurs des fichiers timecycle_mods_1.xml) : même règle pour un paramètre ajouté ici.

use serde::{Deserialize, Serialize};

/// Nom du fichier écrit à l'export (racine de la ressource) et déclaré dans le fxmanifest.
pub const TC_FILE: &str = "dw_timecycmod.xml";

/// Un modificateur : paramètre du jeu et ses deux valeurs, telles que dans les fichiers du jeu.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TcMod {
    pub name: String,
    pub a: f32,
    pub b: f32,
}

/// Ambiance du projet : nom choisi (le nom dans le jeu est `dw_` + nom nettoyé), ambiance du jeu de départ,
/// drapeaux et tous les modificateurs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjAmbiance {
    pub name: String,
    /// ambiance du jeu dont elle a été copiée (vide : partie de rien)
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub user_flags: u32,
    #[serde(default)]
    pub mods: Vec<TcMod>,
}

/// Réglage lisible : libellé, paramètre(s) du jeu, bornes, valeur quand l'ambiance ne le change pas, aide.
pub struct Reglage {
    pub label: &'static str,
    pub params: &'static [&'static str],
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub help: &'static str,
}

/// Couleur de la lumière artificielle de l'intérieur (vers le bas et vers le haut).
pub const COL_DOWN: [&str; 3] = ["light_artificial_int_down_col_r", "light_artificial_int_down_col_g", "light_artificial_int_down_col_b"];
pub const COL_UP: [&str; 3] = ["light_artificial_int_up_col_r", "light_artificial_int_up_col_g", "light_artificial_int_up_col_b"];

/// Réglages proposés (en plus de la couleur). Bornes **estimées** d'après les valeurs des ambiances du jeu.
pub const REGLAGES: [Reglage; 5] = [
    Reglage { label: "Luminosité (exposition)", params: &["postfx_exposure"], min: -3.0, max: 3.0, default: 0.0, help: "En crans d'exposition : -1 = deux fois plus sombre, +1 = deux fois plus clair (postfx_exposure)" },
    Reglage { label: "Intensité de la lumière", params: &["light_artificial_int_down_intensity"], min: 0.0, max: 8.0, default: 1.0, help: "Lumière artificielle de l'intérieur, vers le bas (light_artificial_int_down_intensity)" },
    Reglage { label: "Lumière d'ambiance", params: &["artificial_int_ambient_multiplier"], min: 0.0, max: 2.0, default: 1.0, help: "Lumière diffuse de l'intérieur (artificial_int_ambient_multiplier)" },
    Reglage { label: "Saturation", params: &["postfx_desaturation"], min: 0.0, max: 2.0, default: 1.0, help: "1 = couleurs normales, 0 = noir et blanc, au-dessus de 1 = couleurs plus vives (postfx_desaturation ; ex. CAMERA_BW = 0)" },
    Reglage { label: "Vignettage", params: &["postfx_vignetting_intensity"], min: 0.0, max: 1.0, default: 0.0, help: "Bords de l'image assombris (postfx_vignetting_intensity)" },
];

/// Nom dans le jeu : `dw_` + nom en minuscules, lettres accentuées sans accent, autres caractères → `_`
/// (jamais deux de suite ni au bout). Un nom qui commence déjà par `dw_` ne le prend pas deux fois.
pub fn clean_name(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().to_lowercase().chars() {
        let c = match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ÿ' => 'y',
            c => c,
        };
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            s.push(c);
        } else if !s.is_empty() && !s.ends_with('_') {
            s.push('_');
        }
    }
    let s = s.trim_end_matches('_');
    let s = s.strip_prefix("dw_").unwrap_or(s).trim_start_matches('_');
    if s.is_empty() || s == "dw" {
        "dw_ambiance".into()
    } else {
        format!("dw_{s}")
    }
}

/// Seconde valeur d'un paramètre ajouté : 1 pour une couleur, 0 sinon (règle des fichiers du jeu).
pub fn default_b(param: &str) -> f32 {
    if param.contains("_col_") {
        1.0
    } else {
        0.0
    }
}

impl ProjAmbiance {
    /// Nom écrit dans le jeu (pièces, fichier dw_timecycmod.xml).
    pub fn game_name(&self) -> String {
        clean_name(&self.name)
    }

    pub fn get(&self, param: &str) -> Option<f32> {
        self.mods.iter().find(|m| m.name.eq_ignore_ascii_case(param)).map(|m| m.a)
    }

    /// Première valeur d'un paramètre ; ajouté s'il manque (seconde valeur : règle du jeu).
    pub fn set(&mut self, param: &str, a: f32) {
        match self.mods.iter_mut().find(|m| m.name.eq_ignore_ascii_case(param)) {
            Some(m) => m.a = a,
            None => self.mods.push(TcMod { name: param.to_string(), a, b: default_b(param) }),
        }
    }

    /// Couleur de la lumière de l'intérieur (blanc si l'ambiance ne la change pas).
    pub fn color(&self) -> [f32; 3] {
        [0, 1, 2].map(|i| self.get(COL_DOWN[i]).unwrap_or(1.0))
    }

    pub fn set_color(&mut self, c: [f32; 3]) {
        for i in 0..3 {
            self.set(COL_DOWN[i], c[i]);
            self.set(COL_UP[i], c[i]);
        }
    }

    /// Valeurs (paramètre, première valeur) pour l'aperçu (meubler::amb_from_values).
    pub fn values(&self) -> Vec<(String, f32)> {
        self.mods.iter().map(|m| (m.name.clone(), m.a)).collect()
    }
}

/// Modificateurs des fichiers timecycle_mods*.xml : (nom, userFlags, [(paramètre, valeur a, valeur b)]). Une seule
/// valeur : b reprend la règle du jeu.
pub fn parse_tc_full(xml: &str) -> Vec<(String, u32, Vec<TcMod>)> {
    let mut out = Vec::new();
    for part in xml.split("<modifier ").skip(1) {
        let Some(head_end) = part.find('>') else { continue };
        let head = &part[..head_end];
        let attr = |k: &str| head.split(&format!("{k}=\"")).nth(1).and_then(|x| x.split('"').next()).map(|x| x.to_string());
        let Some(name) = attr("name") else { continue };
        let flags = attr("userFlags").and_then(|x| x.trim().parse::<u32>().ok()).unwrap_or(0);
        let body = part[head_end + 1..].split("</modifier>").next().unwrap_or("");
        let mut mods: Vec<TcMod> = Vec::new();
        let mut rest = body;
        while let Some(s) = rest.find('<') {
            let after = &rest[s + 1..];
            if after.starts_with('/') || after.starts_with('!') {
                rest = &after[1..];
                continue;
            }
            let Some(gt) = after.find('>') else { break };
            let tag = after[..gt].split_whitespace().next().unwrap_or("").trim_end_matches('/').to_string();
            let txt = &after[gt + 1..];
            let end = txt.find('<').unwrap_or(txt.len());
            let mut nums = txt[..end].split_whitespace().filter_map(|x| x.parse::<f32>().ok());
            if let Some(a) = nums.next() {
                let b = nums.next().unwrap_or_else(|| default_b(&tag));
                if !tag.is_empty() && !mods.iter().any(|m| m.name.eq_ignore_ascii_case(&tag)) {
                    mods.push(TcMod { name: tag, a, b });
                }
            }
            rest = &txt[end..];
        }
        out.push((name, flags, mods));
    }
    out
}

fn num(v: f32) -> String {
    if v.is_finite() {
        format!("{v:.3}")
    } else {
        "0.000".into()
    }
}

/// Fichier dw_timecycmod.xml (fins de ligne CRLF, indentation de deux espaces comme les fichiers du jeu).
/// Ambiances de même nom dans le jeu : la première seulement.
pub fn to_xml(ambs: &[&ProjAmbiance]) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n\r\n<timecycle_modifier_data version=\"1.000000\">\r\n");
    let mut seen = std::collections::HashSet::new();
    for a in ambs {
        let name = a.game_name();
        if !seen.insert(name.clone()) {
            continue;
        }
        let mods: Vec<&TcMod> = a.mods.iter().filter(|m| !m.name.is_empty() && m.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).collect();
        s += &format!("  <modifier name=\"{name}\" numMods=\"{}\" userFlags=\"{}\">\r\n", mods.len(), a.user_flags);
        for m in mods {
            s += &format!("    <{0}>{1} {2}</{0}>\r\n", m.name, num(m.a), num(m.b));
        }
        s += "  </modifier>\r\n";
    }
    s += "</timecycle_modifier_data>\r\n";
    s
}

/// Nom choisi rendu unique (nom dans le jeu différent de ceux de `others`) : « nom 2 », « nom 3 »…
pub fn unique_name(others: &[ProjAmbiance], wanted: &str) -> String {
    let taken = |n: &str| others.iter().any(|a| a.game_name() == clean_name(n));
    if !taken(wanted) {
        return wanted.to_string();
    }
    (2..).map(|i| format!("{wanted} {i}")).find(|n| !taken(n)).unwrap()
}

/// Ambiances du projet utilisées par une pièce (ambiance ou ambiance secondaire), sans doublon de nom dans le jeu.
pub fn used(proj: &crate::model::Project) -> Vec<&ProjAmbiance> {
    let rooms: Vec<&crate::model::Room> = proj.ytyps.iter().flat_map(|y| y.mlos.iter()).flat_map(|m| m.rooms.iter()).collect();
    let mut seen = std::collections::HashSet::new();
    proj.ambiances
        .iter()
        .filter(|a| {
            let gn = a.game_name();
            rooms.iter().any(|r| r.timecycle.eq_ignore_ascii_case(&gn) || r.secondary_timecycle.eq_ignore_ascii_case(&gn)) && seen.insert(gn)
        })
        .collect()
}

/// Export : écrit `dw_timecycmod.xml` à la racine de la ressource s'il y a des ambiances du projet utilisées (rend
/// leur nombre), sinon retire celui d'un export précédent.
pub fn export_file(proj: &crate::model::Project, out: &std::path::Path) -> std::io::Result<usize> {
    let list = used(proj);
    let path = out.join(TC_FILE);
    if list.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(0);
    }
    std::fs::write(&path, to_xml(&list))?;
    Ok(list.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nettoyage_du_nom() {
        assert_eq!(clean_name("Bar Tequila"), "dw_bar_tequila");
        assert_eq!(clean_name("  Garage été n°2 !! "), "dw_garage_ete_n_2");
        assert_eq!(clean_name("dw_salon"), "dw_salon");
        assert_eq!(clean_name("DW Salon"), "dw_salon");
        assert_eq!(clean_name("***"), "dw_ambiance");
        assert_eq!(clean_name(""), "dw_ambiance");
        let a = ProjAmbiance { name: "Salon".into(), base: String::new(), user_flags: 0, mods: vec![] };
        assert_eq!(unique_name(std::slice::from_ref(&a), "salon"), "salon 2");
        assert_eq!(unique_name(std::slice::from_ref(&a), "Cuisine"), "Cuisine");
    }

    #[test]
    fn copie_des_valeurs_et_reglages() {
        // extrait au format du jeu (timecycle_mods_1.xml) et du MLO burgershot
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n\r\n<timecycle_modifier_data version=\"1.000000\">\r\n  <modifier name=\"int_test\" numMods=\"4\" userFlags=\"2\">\r\n    <light_artificial_int_down_col_r>0.733 1.000</light_artificial_int_down_col_r>\r\n    <light_artificial_int_down_intensity>0.100 0.000</light_artificial_int_down_intensity>\r\n    <postfx_exposure>-2.00 0.000</postfx_exposure>\r\n    <far_clip>0.000 1000.000</far_clip>\r\n  </modifier>\r\n  <modifier name=\"autre\" numMods=\"0\" userFlags=\"0\">\r\n  </modifier>\r\n</timecycle_modifier_data>";
        let mods = parse_tc_full(xml);
        assert_eq!(mods.len(), 2);
        let (n, f, m) = &mods[0];
        assert_eq!((n.as_str(), *f, m.len()), ("int_test", 2, 4));
        assert_eq!(m[3], TcMod { name: "far_clip".into(), a: 0.0, b: 1000.0 });
        let mut a = ProjAmbiance { name: "Mon bar".into(), base: n.clone(), user_flags: *f, mods: m.clone() };
        // les modificateurs non réglés sont gardés tels quels
        a.set("postfx_exposure", 0.5);
        a.set_color([1.0, 0.5, 0.25]);
        assert_eq!(a.get("postfx_exposure"), Some(0.5));
        assert_eq!(a.get("far_clip"), Some(0.0));
        assert_eq!(a.color(), [1.0, 0.5, 0.25]);
        // ajoutés : seconde valeur 1 pour une couleur, 0 sinon
        assert_eq!(a.mods.iter().find(|m| m.name == "light_artificial_int_up_col_g").unwrap().b, 1.0);
        a.set("postfx_desaturation", 0.3);
        assert_eq!(a.mods.last().unwrap().b, 0.0);
        assert_eq!(a.mods.iter().find(|m| m.name == "light_artificial_int_down_col_r").unwrap().b, 1.0);
        // relu depuis le fichier écrit : mêmes valeurs
        let back = parse_tc_full(&to_xml(&[&a]));
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].0, "dw_mon_bar");
        assert_eq!(back[0].1, 2);
        assert_eq!(back[0].2, a.mods);
    }

    #[test]
    fn fichier_ecrit_seulement_si_utilisee() {
        use crate::math::v3;
        use crate::model::{MloArchetype, Project, Room, Ytyp};
        let mut room = Room::new("salle", v3(0.0, 0.0, 0.0), v3(1.0, 1.0, 1.0));
        room.timecycle = "int_garage_02".into();
        let mlo = MloArchetype { name: "int_essai".into(), lod_dist: 100.0, flags: 0, mlo_flags: 0, entities: vec![], rooms: vec![Room::new("limbo", v3(0.0, 0.0, 0.0), v3(0.0, 0.0, 0.0)), room], portals: vec![], entity_sets: vec![] };
        let mut proj = Project { name: "essai".into(), ytyps: vec![Ytyp { name: "essai".into(), mlos: vec![mlo], visible: true, raw_archetypes: vec![] }], ..Default::default() };
        proj.ambiances.push(ProjAmbiance { name: "Garage froid".into(), base: "int_garage_02".into(), user_flags: 0, mods: vec![TcMod { name: "postfx_exposure".into(), a: -0.5, b: 0.0 }] });
        let dir = std::env::temp_dir().join(format!("dw_tc_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert_eq!(export_file(&proj, &dir).unwrap(), 0);
        assert!(!dir.join(TC_FILE).exists());
        proj.ytyps[0].mlos[0].rooms[1].timecycle = "DW_garage_froid".into();
        assert_eq!(export_file(&proj, &dir).unwrap(), 1);
        let x = std::fs::read_to_string(dir.join(TC_FILE)).unwrap();
        assert!(x.contains("<modifier name=\"dw_garage_froid\" numMods=\"1\" userFlags=\"0\">"));
        proj.ytyps[0].mlos[0].rooms[1].timecycle.clear();
        assert_eq!(export_file(&proj, &dir).unwrap(), 0);
        assert!(!dir.join(TC_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_du_fichier() {
        let a = ProjAmbiance { name: "Salon chaud".into(), base: "int_test".into(), user_flags: 0, mods: vec![TcMod { name: "postfx_exposure".into(), a: -1.0, b: 0.0 }, TcMod { name: "light_artificial_int_down_col_r".into(), a: 1.0, b: 1.0 }] };
        let dup = ProjAmbiance { name: "salon-chaud".into(), ..a.clone() };
        let x = to_xml(&[&a, &dup]);
        assert!(x.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n\r\n<timecycle_modifier_data version=\"1.000000\">\r\n"));
        assert!(x.contains("  <modifier name=\"dw_salon_chaud\" numMods=\"2\" userFlags=\"0\">\r\n"));
        assert!(x.contains("    <postfx_exposure>-1.000 0.000</postfx_exposure>\r\n"));
        assert!(x.contains("    <light_artificial_int_down_col_r>1.000 1.000</light_artificial_int_down_col_r>\r\n"));
        assert!(x.ends_with("  </modifier>\r\n</timecycle_modifier_data>\r\n"));
        // même nom dans le jeu : écrit une fois
        assert_eq!(x.matches("<modifier ").count(), 1);
        assert!(!x.contains('\n') || x.matches('\n').count() == x.matches("\r\n").count());
    }
}
