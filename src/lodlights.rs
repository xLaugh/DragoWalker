//! Lumières de nuit vues de loin : les lampadaires du projet deviennent des « lumières de loin » du jeu, deux cartes
//! liées comme celles du jeu (x64e.rpf\levels\gta5\lodlights.rpf, relues en XML par CodeWalker le 18 nov. 2026) :
//!
//! - dw_distlights (flags 2, contentFlags 256) : DistantLODLightsSOA — position, couleur et intensité (RGBI,
//!   0xIIRRGGBB), numStreetLights (les N premières sont des lampadaires de rue), category 0 (petites) ;
//! - dw_lodlights (parent dw_distlights, flags 0, contentFlags 128) : LODLightsSOA — direction, falloff,
//!   falloffExponent, timeAndStateFlags, hash, coneInnerAngle, coneOuterAngleOrCapExt, coronaIntensity, une valeur
//!   par lumière, dans le même ordre que les positions.
//!
//! Réglages repris des lampadaires de rue du jeu (lodlights_small002, deux premières lumières) : falloff 5, exposant
//! 32, cône 14 / 35, couronne 0, timeAndStateFlags 0x05F0003F (allumé de 20 h à 5 h 59 : heures 0–5 et 20–23), RGBI
//! 0xAABAE6FF. Les lumières sont rangées par hash croissant, comme dans les fichiers du jeu.

use crate::math::{v3, Quat, V3};

/// Lampadaires de rue du jeu repris (préfixes de noms de modèles).
const STREET_LAMPS: [&str; 4] = ["prop_streetlight", "prop_street_light", "prop_traffic_light", "prop_wall_light"];
/// Réglages d'un lampadaire de rue (voir l'en-tête).
const FALLOFF: u32 = 5;
const FALLOFF_EXP: u32 = 32;
const CONE_IN: u32 = 14;
const CONE_OUT: u32 = 35;
const FLAGS: u32 = 0x05F0_003F;
const RGBI: u32 = 0xAABA_E6FF;
/// Marge de chargement autour des lumières (m) : celle des cartes du jeu (≈ 440 m autour des lumières).
const STREAM_MARGIN: f32 = 450.0;

pub struct LodLight {
    pub pos: V3,
    pub dir: V3,
    pub hash: u32,
}

/// Lampadaire de rue ? (pas les feux tricolores : ce sont des « traffic_light » sans éclairage de rue)
pub fn is_street_lamp(name: &str) -> bool {
    let n = name.to_lowercase();
    STREET_LAMPS.iter().take(2).any(|p| n.starts_with(p))
}

/// Têtes de lampe d'un lampadaire posé (boîte du modèle dans son repère : le bras part vers -Y, la tête est en haut
/// au bout du bras ; un modèle à deux bras « 03 » a une tête à chaque bout) — **estimé**.
pub fn heads(pos: V3, rot: Quat, scale_z: f32, bb_min: V3, bb_max: V3, double: bool) -> Vec<(V3, V3)> {
    let top = bb_max.z * scale_z - 0.4;
    let cx = (bb_min.x + bb_max.x) * 0.5;
    let mut out = Vec::new();
    let mut add = |y: f32, d: f32| {
        let local = v3(cx, y, top);
        let p = pos + rot.rotate(local);
        let dir = rot.rotate(v3(0.0, d, 0.0));
        let dir = v3(dir.x, dir.y, 0.0).norm();
        out.push((p, dir));
    };
    add(bb_min.y + 0.3, -1.0);
    if double {
        add(bb_max.y - 0.3, 1.0);
    }
    out
}

/// Écrit les deux cartes (XML de CodeWalker) dans `stream`. Rend les fichiers écrits (vides : rien à écrire).
pub fn write(stream: &std::path::Path, lights: &mut [LodLight]) -> Result<Vec<String>, String> {
    if lights.is_empty() {
        return Ok(Vec::new());
    }
    lights.sort_by_key(|l| l.hash);
    let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
    for l in lights.iter() {
        mn = mn.min(l.pos);
        mx = mx.max(l.pos);
    }
    let m = v3(STREAM_MARGIN, STREAM_MARGIN, STREAM_MARGIN);
    let ext = |a: V3, b: V3| format!(" <streamingExtentsMin x=\"{}\" y=\"{}\" z=\"{}\" />\n <streamingExtentsMax x=\"{}\" y=\"{}\" z=\"{}\" />\n <entitiesExtentsMin x=\"{}\" y=\"{}\" z=\"{}\" />\n <entitiesExtentsMax x=\"{}\" y=\"{}\" z=\"{}\" />\n", a.x - m.x, a.y - m.y, a.z - m.z, b.x + m.x, b.y + m.y, b.z + m.z, a.x, a.y, a.z, b.x, b.y, b.z);
    let common = " <entities />\n <containerLods />\n <boxOccluders />\n <occludeModels />\n <physicsDictionaries />\n <instancedData>\n  <ImapLink />\n  <PropInstanceList />\n  <GrassInstanceList />\n </instancedData>\n <timeCycleModifiers />\n <carGenerators />\n";
    let block = |name: &str| format!(" <block>\n  <version value=\"0\" />\n  <flags value=\"0\" />\n  <name>{name}</name>\n  <exportedBy>DragoWalker</exportedBy>\n  <owner />\n  <time />\n </block>\n");
    let ints = |v: Vec<u32>| -> String {
        let mut s = String::new();
        for (i, c) in v.chunks(10).enumerate() {
            if i > 0 {
                s.push('\n');
            }
            s += "   ";
            s += &c.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ");
        }
        s
    };
    let n = lights.len() as u32;
    let rep = |v: u32| vec![v; lights.len()];
    // lumières de loin
    let mut pos = String::new();
    for l in lights.iter() {
        pos += &format!("   <Item>\n    <x value=\"{}\" />\n    <y value=\"{}\" />\n    <z value=\"{}\" />\n   </Item>\n", l.pos.x, l.pos.y, l.pos.z);
    }
    let dist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>dw_distlights</name>\n <parent />\n <flags value=\"2\" />\n <contentFlags value=\"256\" />\n{}{common} <LODLightsSOA>\n  <direction />\n  <falloff />\n  <falloffExponent />\n  <timeAndStateFlags />\n  <hash />\n  <coneInnerAngle />\n  <coneOuterAngleOrCapExt />\n  <coronaIntensity />\n </LODLightsSOA>\n <DistantLODLightsSOA>\n  <position>\n{pos}  </position>\n  <RGBI>\n{}\n  </RGBI>\n  <numStreetLights value=\"{n}\" />\n  <category value=\"0\" />\n </DistantLODLightsSOA>\n{}</CMapData>\n",
        ext(mn, mx),
        ints(rep(RGBI)),
        block("dw_distlights")
    );
    // lumières proches (même ordre)
    let mut dir = String::new();
    for l in lights.iter() {
        dir += &format!("   <Item>\n    <x value=\"{}\" />\n    <y value=\"{}\" />\n    <z value=\"{}\" />\n   </Item>\n", l.dir.x, l.dir.y, l.dir.z);
    }
    let lod = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>dw_lodlights</name>\n <parent>dw_distlights</parent>\n <flags value=\"0\" />\n <contentFlags value=\"128\" />\n{}{common} <LODLightsSOA>\n  <direction>\n{dir}  </direction>\n  <falloff>\n{}\n  </falloff>\n  <falloffExponent>\n{}\n  </falloffExponent>\n  <timeAndStateFlags>\n{}\n  </timeAndStateFlags>\n  <hash>\n{}\n  </hash>\n  <coneInnerAngle>\n{}\n  </coneInnerAngle>\n  <coneOuterAngleOrCapExt>\n{}\n  </coneOuterAngleOrCapExt>\n  <coronaIntensity>\n{}\n  </coronaIntensity>\n </LODLightsSOA>\n <DistantLODLightsSOA>\n  <position />\n  <RGBI />\n  <numStreetLights value=\"0\" />\n  <category value=\"0\" />\n </DistantLODLightsSOA>\n{}</CMapData>\n",
        ext(mn, mx),
        ints(rep(FALLOFF)),
        ints(rep(FALLOFF_EXP)),
        ints(rep(FLAGS)),
        ints(lights.iter().map(|l| l.hash).collect()),
        ints(rep(CONE_IN)),
        ints(rep(CONE_OUT)),
        ints(rep(0)),
        block("dw_lodlights")
    );
    std::fs::write(stream.join("dw_distlights.ymap.xml"), dist).map_err(|e| e.to_string())?;
    std::fs::write(stream.join("dw_lodlights.ymap.xml"), lod).map_err(|e| e.to_string())?;
    Ok(vec!["dw_distlights.ymap".into(), "dw_lodlights.ymap".into()])
}

impl crate::app::DwApp {
    /// Lampadaires de tout le projet (cartes du projet), une lumière de loin par tête de lampe.
    pub fn lod_lights(&self) -> Vec<LodLight> {
        let mut out = Vec::new();
        for y in self.proj.ymaps.iter().filter(|y| !y.sample) {
            for e in y.entities.iter().filter(|e| is_street_lamp(&e.archetype)) {
                let h = crate::gta::crypto::joaat(&e.archetype.to_lowercase());
                // boîte du modèle : celle du jeu, sinon celle de prop_streetlight_01 (y de -2,42 à 0,46, ~8,5 m)
                let (bmin, bmax) = self.game.world.as_ref().and_then(|w| w.archetypes.get(&h)).map(|a| (a.bb_min, a.bb_max)).unwrap_or((v3(-0.3, -2.42, 0.0), v3(0.3, 0.46, 8.5)));
                let double = e.archetype.to_lowercase().starts_with(crate::roads::LAMP_DOUBLE);
                for (k, (p, d)) in heads(e.pos, e.rot, e.scale_z, bmin, bmax, double).into_iter().enumerate() {
                    let hash = crate::gta::crypto::joaat(&format!("dw_lodlight_{}_{k}", e.id));
                    out.push(LodLight { pos: p, dir: d, hash });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deux_cartes_liees() {
        let dir = std::env::temp_dir().join("dw_test_lodlights");
        let _ = std::fs::create_dir_all(&dir);
        let mut l = vec![LodLight { pos: v3(10.0, 20.0, 8.0), dir: v3(0.0, -1.0, 0.0), hash: 50 }, LodLight { pos: v3(40.0, 20.0, 8.0), dir: v3(1.0, 0.0, 0.0), hash: 7 }];
        let f = write(&dir, &mut l).unwrap();
        assert_eq!(f.len(), 2);
        let lod = std::fs::read_to_string(dir.join("dw_lodlights.ymap.xml")).unwrap();
        let dist = std::fs::read_to_string(dir.join("dw_distlights.ymap.xml")).unwrap();
        assert!(lod.contains("<parent>dw_distlights</parent>") && dist.contains("<numStreetLights value=\"2\" />"));
        // rangées par hash : la lumière de hash 7 (x = 40) d'abord
        assert!(lod.contains("   7 50") && dist.find("40").unwrap() < dist.find(">10<").unwrap_or(usize::MAX).min(dist.find("\"10\"").unwrap()));
        // têtes : prop_streetlight_01 tourné de 90° — la tête part vers +X (le -Y du modèle tourné)
        let h = heads(v3(0.0, 0.0, 0.0), Quat::from_z(std::f32::consts::FRAC_PI_2), 1.0, v3(-0.3, -2.42, 0.0), v3(0.3, 0.46, 8.5), false);
        assert_eq!(h.len(), 1);
        assert!(h[0].0.x > 2.0 && (h[0].0.z - 8.1).abs() < 0.01 && h[0].1.x > 0.99, "{:?}", h[0].0);
        assert!(is_street_lamp("prop_streetlight_03d") && !is_street_lamp("prop_traffic_01a"));
    }
}

#[cfg(test)]
mod tests_cw {
    use super::*;

    /// Conversion réelle (DW_CW_DIR) : `cargo test -- --ignored lodlights_cw --nocapture`.
    #[test]
    #[ignore]
    fn lodlights_cw() {
        let dll = crate::cwbridge::dll(&std::env::var("DW_CW_DIR").unwrap_or_default()).expect("CodeWalker");
        let dir = std::env::temp_dir().join("dw_test_lodlights_cw");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut l: Vec<LodLight> = (0..12).map(|i| LodLight { pos: v3(100.0 + i as f32 * 30.0, -900.0, 38.0), dir: v3(0.0, -1.0, 0.0), hash: crate::gta::crypto::joaat(&format!("dw_lodlight_{i}_0")) }).collect();
        write(&dir, &mut l).unwrap();
        let (n, errs) = crate::cwbridge::to_binary(&dll, &dir).unwrap();
        println!("convertis {n}, erreurs {errs:?}");
        assert!(errs.is_empty() && dir.join("dw_lodlights.ymap").is_file() && dir.join("dw_distlights.ymap").is_file());
        let back = dir.join("back");
        std::fs::create_dir_all(&back).unwrap();
        crate::cwbridge::to_xml(&dll, &[(dir.join("dw_lodlights.ymap"), back.join("l.xml")), (dir.join("dw_distlights.ymap"), back.join("d.xml"))]).unwrap();
        let l2 = std::fs::read_to_string(back.join("l.xml")).unwrap();
        let d2 = std::fs::read_to_string(back.join("d.xml")).unwrap();
        println!("{}", &l2[l2.find("<timeAndStateFlags>").unwrap()..][..200]);
        let parent = format!("<parent>hash_{:08X}</parent>", crate::gta::crypto::joaat("dw_distlights"));
        assert!((l2.contains("<parent>dw_distlights</parent>") || l2.contains(&parent)) && l2.contains("99614783") && d2.contains("<numStreetLights value=\"12\" />"));
        assert_eq!(d2.matches("<Item>").count(), 12);
    }
}
