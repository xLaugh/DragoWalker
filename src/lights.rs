//! Lumières d'intérieur : comme le jeu, elles sont rangées dans le modèle (liste « Lights » du Drawable,
//! Drawable::ReadXml 0x06004bb0 → LightAttributes::ReadXml 0x06004b3d). Chaque pièce d'un intérieur créé dans
//! DragoWalker reçoit des spots au plafond, tournés vers le bas, avec les réglages les plus fréquents des
//! lumières de ce genre dans les modèles des intérieurs du jeu (sonde DW_LIGHTS : 12 791 spots vers le bas
//! allumés toute la journée, sur 42 215 lumières lues dans 23 494 modèles d'intérieur).

use crate::math::{v3, V3};

/// Type de lumière « Spot » (énumération LightType de CodeWalker, lue par nom : LightAttributes::ReadXml
/// 0x06004b3d, GetChildEnumInnerText ; valeur 2 dans le fichier, la plus fréquente : 29 160 sur 42 215).
pub const LIGHT_TYPE: &str = "Spot";
/// Heures d'allumage : un bit par heure, 0xFFFFFF = toute la journée (35 830 sur 42 215).
pub const LIGHT_HOURS: u32 = 0xFF_FFFF;
/// Puissance (mode des spots vers le bas : 32 pour 3 361 sur 12 791 ; 8 vient ensuite) — on garde 8, plus
/// proche d'une pièce d'habitation (le 32 va surtout avec une portée de 0,1 m) — **estimé** parmi les valeurs du jeu.
pub const LIGHT_INTENSITY: f32 = 8.0;
/// Exposant de l'atténuation (mode : 32, 3 716 sur 12 791).
pub const LIGHT_FALLOFF_EXP: f32 = 32.0;
/// Cône intérieur / extérieur en degrés (60 / 90 : 986 sur 12 791, le plus fréquent après 0 / 90).
pub const LIGHT_CONE_INNER: f32 = 60.0;
pub const LIGHT_CONE_OUTER: f32 = 90.0;
/// Couleur blanc chaud (231, 212, 188 : la couleur des réglages complets les plus fréquents, 528 fois).
pub const LIGHT_COLOR: [u8; 3] = [231, 212, 188];
/// Drapeaux : 0 (le plus fréquent toutes lumières confondues : 7 205 sur 42 215).
pub const LIGHT_FLAGS: u32 = 0;
/// Halo : taille 0, intensité 1 (26 151 sur 42 215) ; volume 1 (37 712) ; plan proche des ombres 0,01 (41 009).
pub const LIGHT_CORONA_SIZE: f32 = 0.0;
pub const LIGHT_CORONA_INTENSITY: f32 = 1.0;
pub const LIGHT_VOLUME_INTENSITY: f32 = 1.0;
pub const LIGHT_SHADOW_NEAR: f32 = 0.01;
/// Écart maximal entre deux spots d'une même pièce (m) — **estimé**.
pub const LIGHT_SPACING: f32 = 5.0;
/// Distance sous le plafond (m) — **estimé**.
pub const LIGHT_BELOW_CEIL: f32 = 0.1;
/// Portée en plus de ce qu'il faut pour atteindre le coin du sol éclairé par le spot (m) — **estimé**.
pub const LIGHT_REACH_MARGIN: f32 = 1.0;

/// Une lumière posée dans un modèle (repère du modèle).
#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub pos: V3,
    pub falloff: f32,
}

/// Spots du plafond d'une pièce (boîte `a`–`b`) : grille régulière, un spot par case d'au plus LIGHT_SPACING
/// de côté ; portée = distance au coin de sol de la case + LIGHT_REACH_MARGIN.
pub fn room_lights(a: V3, b: V3) -> Vec<Light> {
    let (w, d, h) = (b.x - a.x, b.y - a.y, b.z - a.z);
    if w <= 0.1 || d <= 0.1 || h <= 0.1 {
        return Vec::new();
    }
    let nx = (w / LIGHT_SPACING).ceil().max(1.0) as usize;
    let ny = (d / LIGHT_SPACING).ceil().max(1.0) as usize;
    let (cw, cd) = (w / nx as f32, d / ny as f32);
    let z = b.z - LIGHT_BELOW_CEIL.min(h * 0.5);
    let drop = z - a.z;
    let reach = (drop * drop + (cw * 0.5).powi(2) + (cd * 0.5).powi(2)).sqrt() + LIGHT_REACH_MARGIN;
    let mut out = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            out.push(Light { pos: v3(a.x + cw * (i as f32 + 0.5), a.y + cd * (j as f32 + 0.5), z), falloff: reach });
        }
    }
    out
}

/// Orientation d'une lumière neuve de CodeWalker (ModelLightForm::NewLightAttribute 0x06001660) : Direction =
/// Vector3.BackwardLH (0, 0, −1, vers le bas), Tangent = Vector3.Left (−1, 0, 0), Extent = (1, 1, 1),
/// LightHash laissé à 0 ; c'est aussi un Spot (Type 2) allumé toute la journée (TimeFlags 16 777 215).
pub const LIGHT_TANGENT_X: f32 = -1.0;

/// Liste « Lights » au format XML de CodeWalker (balises de LightAttributes::ReadXml 0x06004b3d, dans le même
/// ordre que les modèles du jeu convertis par CodeWalker). Vide si aucune lumière.
pub fn lights_xml(lights: &[Light]) -> String {
    use std::fmt::Write;
    if lights.is_empty() {
        return String::new();
    }
    let [r, g, b] = LIGHT_COLOR;
    let mut s = String::from(" <Lights>\n");
    for l in lights {
        let _ = write!(
            s,
            "  <Item>\n   <Position x=\"{}\" y=\"{}\" z=\"{}\" />\n   <Colour r=\"{r}\" g=\"{g}\" b=\"{b}\" />\n   <Flashiness value=\"0\" />\n   <Intensity value=\"{LIGHT_INTENSITY}\" />\n   <Flags value=\"{LIGHT_FLAGS}\" />\n   <BoneId value=\"0\" />\n   <Type>{LIGHT_TYPE}</Type>\n   <GroupId value=\"0\" />\n   <TimeFlags value=\"{LIGHT_HOURS}\" />\n   <Falloff value=\"{}\" />\n   <FalloffExponent value=\"{LIGHT_FALLOFF_EXP}\" />\n   <CullingPlaneNormal x=\"0\" y=\"0\" z=\"-1\" />\n   <CullingPlaneOffset value=\"0\" />\n   <Unknown45 value=\"0\" />\n   <Unknown46 value=\"0\" />\n   <VolumeIntensity value=\"{LIGHT_VOLUME_INTENSITY}\" />\n   <VolumeSizeScale value=\"1\" />\n   <VolumeOuterColour r=\"255\" g=\"255\" b=\"255\" />\n   <LightHash value=\"0\" />\n   <VolumeOuterIntensity value=\"1\" />\n   <CoronaSize value=\"{LIGHT_CORONA_SIZE}\" />\n   <VolumeOuterExponent value=\"1\" />\n   <LightFadeDistance value=\"0\" />\n   <ShadowBlur value=\"0\" />\n   <ShadowFadeDistance value=\"0\" />\n   <SpecularFadeDistance value=\"0\" />\n   <VolumetricFadeDistance value=\"0\" />\n   <ShadowNearClip value=\"{LIGHT_SHADOW_NEAR}\" />\n   <CoronaIntensity value=\"{LIGHT_CORONA_INTENSITY}\" />\n   <CoronaZBias value=\"0.1\" />\n   <Direction x=\"0\" y=\"0\" z=\"-1\" />\n   <Tangent x=\"{LIGHT_TANGENT_X}\" y=\"0\" z=\"0\" />\n   <ConeInnerAngle value=\"{LIGHT_CONE_INNER}\" />\n   <ConeOuterAngle value=\"{LIGHT_CONE_OUTER}\" />\n   <Extent x=\"1\" y=\"1\" z=\"1\" />\n   <ProjectedTextureHash />\n  </Item>\n",
            l.pos.x,
            l.pos.y,
            l.pos.z,
            l.falloff
        );
    }
    s.push_str(" </Lights>\n");
    s
}

/// Ajoute la liste des lumières à un modèle déjà écrit (.ydr.xml), juste avant la fin du Drawable.
pub fn add_lights_to_ydr_xml(path: &std::path::Path, lights: &[Light]) -> Result<(), String> {
    if lights.is_empty() {
        return Ok(());
    }
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let Some(at) = s.rfind("</Drawable>") else { return Err("fin du modèle introuvable".into()) };
    let out = format!("{}{}{}", &s[..at], lights_xml(lights), &s[at..]);
    std::fs::write(path, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_room_one_light_big_room_grid() {
        let l = room_lights(v3(0.0, 0.0, 0.0), v3(4.0, 3.0, 3.0));
        assert_eq!(l.len(), 1);
        assert!((l[0].pos.x - 2.0).abs() < 1e-4 && (l[0].pos.z - 2.9).abs() < 1e-4);
        // atteint le coin du sol
        assert!(l[0].falloff > (2.9f32 * 2.9 + 4.0 + 2.25).sqrt());
        assert_eq!(room_lights(v3(0.0, 0.0, 0.0), v3(12.0, 6.0, 3.0)).len(), 3 * 2);
        assert!(room_lights(v3(0.0, 0.0, 0.0), v3(0.0, 3.0, 3.0)).is_empty());
    }

    #[test]
    fn xml_has_every_tag_codewalker_reads() {
        let x = lights_xml(&room_lights(v3(0.0, 0.0, 0.0), v3(4.0, 4.0, 3.0)));
        for t in ["Position", "Colour", "Intensity", "Type>Spot", "TimeFlags value=\"16777215\"", "Falloff ", "FalloffExponent", "Direction", "ConeOuterAngle", "Extent"] {
            assert!(x.contains(t), "{t}");
        }
    }
}
