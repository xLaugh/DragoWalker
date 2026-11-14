//! Son des intérieurs créés dans DragoWalker : sans ces fichiers, on entend la rue à l'intérieur sous FiveM.
//!
//! Format relevé sur les fichiers du jeu (x64\audio\occlusion.rpf, 1 674 fichiers ; dat151 du DLC mpbiker) et lu dans
//! CodeWalker 30 dev48 (classes naOcclusionInteriorMetadata, Dat151InteriorSettings / Dat151InteriorRoom) :
//! - occlusion, un fichier par placement d'intérieur, nommé `<I>.ymt` (I en décimal non signé) avec
//!   I = joaat(nom du type) ^ (int)(x·100) ^ (int)(y·100) ^ (int)(z·100), produits en f32 tronqués vers zéro, x y z =
//!   position du placement (754 placements du jeu sur 806 retrouvent ainsi leur fichier) ;
//! - deux entrées par portail (une dans chaque sens), PortalIdx = rang du portail parmi ceux qui touchent la pièce ;
//!   le dehors est la pièce 0 du même intérieur ;
//! - nœuds de chemin entre deux pièces reliées : Key = u(de) − u(vers) + profondeur (1, 2, 3), u(pièce) = I ^
//!   joaat(nom de la pièce), u(limbo) = joaat("outside") ; vérifié sur tous les intérieurs à une pièce du jeu, à
//!   ~87 % sur ceux à plusieurs pièces (chemins à plusieurs portails non faits) — **à valider en jeu** ;
//! - réglages audio des pièces (dat151) : InteriorSettings (nom = nom du type), une InteriorRoom par pièce
//!   (RoomName = nom de la pièce), valeurs reprises des pièces du DLC mpbiker.
//! Les nombres sont écrits en entiers signés 32 bits (CodeWalker les lit ainsi).

use crate::app::DwApp;
use crate::gta::crypto::joaat;
use crate::math::V3;
use crate::model::MloArchetype;

/// Fichier des réglages audio de toutes les pièces.
pub const AUDIO_REL: &str = "dw_audio_game.dat151.rel";

/// Clé d'un intérieur placé (nom de son fichier d'occlusion).
pub fn proxy_hash(name: &str, p: V3) -> u32 {
    let c = |v: f32| (v * 100.0f32) as i32 as u32;
    joaat(name) ^ c(p.x) ^ c(p.y) ^ c(p.z)
}

fn room_key(ihash: u32, m: &MloArchetype, r: usize) -> u32 {
    if r == 0 {
        joaat("outside")
    } else {
        ihash ^ joaat(&m.rooms.get(r).map(|x| x.name.to_lowercase()).unwrap_or_default())
    }
}

/// Fichier d'occlusion d'un intérieur placé (XML de CodeWalker, naOcclusionInteriorMetadata).
pub fn occlusion_xml(m: &MloArchetype, ihash: u32) -> String {
    let i = ihash as i32;
    // rang de chaque portail parmi ceux qui touchent une pièce
    let local = |room: u32, pi: usize| m.portals.iter().take(pi).filter(|p| p.room_from == room || p.room_to == room).count();
    let mut infos: Vec<(u32, usize, u32, usize)> = Vec::new(); // (pièce, rang local, pièce d'arrivée, portail)
    let nr = m.rooms.len() as u32;
    for (pi, p) in m.portals.iter().enumerate() {
        // portail vers une pièce qui n'existe pas (pièce supprimée, fichier abîmé) : ignoré
        if p.room_from >= nr || p.room_to >= nr {
            continue;
        }
        infos.push((p.room_to, local(p.room_to, pi), p.room_from, pi));
        infos.push((p.room_from, local(p.room_from, pi), p.room_to, pi));
    }
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<naOcclusionInteriorMetadata>\n <PortalInfoList itemType=\"naOcclusionPortalInfoMetadata\">\n");
    for (room, idx, dest, pi) in &infos {
        s += &format!("  <Item>\n   <InteriorProxyHash value=\"{i}\" />\n   <PortalIdx value=\"{idx}\" />\n   <RoomIdx value=\"{room}\" />\n   <DestInteriorHash value=\"{i}\" />\n   <DestRoomIdx value=\"{dest}\" />\n");
        // porte du passage : LinkType 1, MaxOcclusion 0,7, IsDoor (valeurs des portes des fichiers du jeu)
        let doors = m.portal_attached(*pi);
        if doors.is_empty() {
            s += "   <PortalEntityList itemType=\"naOcclusionPortalEntityMetadata\" />\n";
        } else {
            s += "   <PortalEntityList itemType=\"naOcclusionPortalEntityMetadata\">\n";
            for d in doors {
                let h = joaat(&m.entities[d].archetype) as i32;
                s += &format!("    <Item>\n     <LinkType value=\"1\" />\n     <MaxOcclusion value=\"0.7\" />\n     <EntityModelHashkey value=\"{h}\" />\n     <IsDoor value=\"true\" />\n     <IsGlass value=\"false\" />\n    </Item>\n");
            }
            s += "   </PortalEntityList>\n";
        }
        s += "  </Item>\n";
    }
    s += " </PortalInfoList>\n <PathNodeList itemType=\"naOcclusionPathNodeMetadata\">\n";
    // une paire de pièces reliées par au moins un portail : 3 nœuds (profondeurs 1 à 3), enfants = ses portails
    let mut pairs: Vec<(u32, u32)> = infos.iter().map(|(r, _, d, _)| (*r, *d)).collect();
    pairs.sort();
    pairs.dedup();
    for (a, b) in pairs {
        if a == b {
            continue;
        }
        let children: Vec<usize> = infos.iter().enumerate().filter(|(_, x)| x.0 == a && x.2 == b).map(|(k, _)| k).collect();
        let base = room_key(ihash, m, a as usize).wrapping_sub(room_key(ihash, m, b as usize));
        for d in 1..=3u32 {
            s += &format!("  <Item>\n   <Key value=\"{}\" />\n   <PathNodeChildList itemType=\"naOcclusionPathNodeChildMetadata\">\n", base.wrapping_add(d) as i32);
            for c in &children {
                s += &format!("    <Item>\n     <PathNodeKey value=\"0\" />\n     <PortalInfoIdx value=\"{c}\" />\n    </Item>\n");
            }
            s += "   </PathNodeChildList>\n  </Item>\n";
        }
    }
    s += " </PathNodeList>\n</naOcclusionInteriorMetadata>\n";
    s
}

/// Réglages audio des pièces de plusieurs intérieurs (XML de CodeWalker, Dat151).
pub fn rel_xml(mlos: &[&MloArchetype]) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Dat151>\n <Version value=\"7314721\" />\n <Items>\n");
    for m in mlos {
        let mn = m.name.to_lowercase();
        s += &format!("  <Item type=\"InteriorSettings\">\n   <Name>{mn}</Name>\n   <Flags value=\"0xAAAAA844\" />\n   <InteriorWallaSoundSet>hash_D4855127</InteriorWallaSoundSet>\n   <InteriorReflections />\n   <Rooms>\n");
        for r in &m.rooms {
            s += &format!("    <Item>{mn}_{}</Item>\n", r.name.to_lowercase());
        }
        s += "   </Rooms>\n  </Item>\n";
        for (ri, r) in m.rooms.iter().enumerate() {
            let rn = r.name.to_lowercase();
            let (small, medium, large, tone, ext, nmpo, d1, d2) = if ri == 0 { (0.0, 0.0, 0.0, "null_sound", 1, 0, 0, 0) } else { (0.5, 0.6, 0.1, "hash_D8DAB44F", 0, 1, 5, 15) };
            s += &format!(
                "  <Item type=\"InteriorRoom\">\n   <Name>{mn}_{rn}</Name>\n   <Flags value=\"0xAAAAAAAA\" />\n   <RoomName>{rn}</RoomName>\n   <AmbientZone />\n   <InteriorType value=\"0\" />\n   <ReverbSmall value=\"{small}\" />\n   <ReverbMedium value=\"{medium}\" />\n   <ReverbLarge value=\"{large}\" />\n   <RoomToneSound>{tone}</RoomToneSound>\n   <RainType value=\"0\" />\n   <ExteriorAudibility value=\"{ext}\" />\n   <RoomOcclusionDamping value=\"0\" />\n   <NonMarkedPortalOcclusion value=\"{nmpo}\" />\n   <DistanceFromPortalForOcclusion value=\"{d1}\" />\n   <DistanceFromPortalFadeDistance value=\"{d2}\" />\n   <WeaponMetrics />\n   <InteriorWallaSoundSet>hash_D4855127</InteriorWallaSoundSet>\n  </Item>\n"
            );
        }
    }
    s += " </Items>\n</Dat151>\n";
    s
}

/// Lignes du fxmanifest qui déclarent les réglages audio (retirées si leur binaire manque, export::convert_and_tidy).
pub fn audio_manifest_lines() -> String {
    format!("files {{ 'audio/{AUDIO_REL}' }}\ndata_file 'AUDIO_GAMEDATA' 'audio/dw_audio_game.dat'\n")
}

impl DwApp {
    /// Son des intérieurs créés dans DragoWalker : un fichier d'occlusion par placement et les réglages audio de leurs
    /// pièces, dans stream ; `out` : dossier de la ressource. `convert` : CodeWalker fera le binaire des réglages (sans
    /// lui, ils ne sont pas déclarés : le jeu ne lit pas le XML). Rend (compte rendu, lignes du fxmanifest) ; les
    /// lignes sont retirées du fxmanifest si la conversion ne donne pas le fichier (export::convert_and_tidy).
    pub fn export_audio(&self, stream: &std::path::Path, out: &std::path::Path, convert: bool) -> (Vec<String>, String) {
        // le binaire est rangé dans « audio » du dossier de la ressource, hors de stream, après la conversion
        // (export.rs)
        let _ = std::fs::remove_file(out.join("audio").join(AUDIO_REL));
        for f in std::fs::read_dir(stream).into_iter().flatten().flatten() {
            let n = f.file_name().to_string_lossy().to_lowercase();
            let numeric_ymt = n.strip_suffix(".ymt.xml").map(|b| b.chars().all(|c| c.is_ascii_digit())).unwrap_or(false);
            if (numeric_ymt || n.starts_with("dw_audio_game")) && f.path().is_file() {
                let _ = std::fs::remove_file(f.path());
            }
        }
        let ours: Vec<(usize, usize)> = self.created_mlos();
        let mut mlos: Vec<&MloArchetype> = Vec::new();
        let mut n = 0;
        for &(ti, mi) in &ours {
            let m = &self.proj.ytyps[ti].mlos[mi];
            let mut placed = false;
            for inst in self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.entities.iter()).filter(|e| e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(&m.name)) {
                let h = proxy_hash(&m.name, inst.pos);
                if std::fs::write(stream.join(format!("{h}.ymt.xml")), occlusion_xml(m, h)).is_ok() {
                    n += 1;
                    placed = true;
                }
            }
            if placed {
                mlos.push(m);
            }
        }
        if mlos.is_empty() {
            return (Vec::new(), String::new());
        }
        if std::fs::write(stream.join(format!("{AUDIO_REL}.xml")), rel_xml(&mlos)).is_err() {
            return (vec!["Son des intérieurs : écriture impossible.".into()], String::new());
        }
        let rooms: usize = mlos.iter().map(|m| m.rooms.len()).sum();
        if !convert {
            return (vec![format!("Son des intérieurs : {n} fichier(s) d'occlusion et réglages audio de {rooms} pièce(s) laissés en XML (CodeWalker introuvable) ; les réglages ne sont pas déclarés dans le fxmanifest (le jeu ne lit que le binaire).")], String::new());
        }
        (
            vec![format!("Son des intérieurs : {n} fichier(s) d'occlusion (un par placement) et réglages audio de {rooms} pièce(s) ({AUDIO_REL}) — à vérifier en jeu : la rue ne doit plus s'entendre à l'intérieur.")],
            audio_manifest_lines(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v3;
    use crate::model::{MloArchetype, Portal, Room};

    /// Valeurs du fichier du jeu 3407985433.ymt (type de hachage 3407988837, placé en 2330.38, 2572.531, 45.67812,
    /// pièces limbo et Omega2TrailerRm, un portail 1 → 0).
    #[test]
    fn comme_le_fichier_du_jeu() {
        // (position donnée arrondie : la clé est reprise telle quelle du nom du fichier du jeu)
        let h = 3407985433u32;
        let _ = v3(0.0, 0.0, 0.0);
        let m = MloArchetype {
            name: "x".into(),
            lod_dist: 100.0,
            flags: 0,
            mlo_flags: 0,
            entities: vec![],
            rooms: vec![Room::new("limbo", V3::ZERO, V3::ZERO), Room::new("Omega2TrailerRm", V3::ZERO, v3(1.0, 1.0, 1.0))],
            portals: vec![Portal::new(1, 0, [V3::ZERO; 4])],
            entity_sets: vec![],
        };
        let x = occlusion_xml(&m, h);
        for k in ["510687446", "510687447", "510687448", "-510687444", "-510687443", "-510687442", "-886981863"] {
            assert!(x.contains(&format!("value=\"{k}\"")), "{k} absent");
        }
        assert_eq!(x.matches("<InteriorProxyHash").count(), 2);
    }
}
