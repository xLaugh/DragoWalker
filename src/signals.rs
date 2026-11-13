//! Feux rouges et panneaux stop posés automatiquement aux carrefours des routes de DragoWalker, et réglage
//! des points de route pour que les voitures s'y arrêtent.
//!
//! Relevé dans le jeu (sondes DW_TRAFFIC_STATS, DW_MODEL, points de route nodes464.ynd lus par CodeWalker) :
//! - prop_traffic_01a (bras de 5,5 m) et prop_traffic_01b (bras de 10,2 m) : les feux regardent vers −Y du
//!   modèle, le bras part vers −X ; dans le jeu, 1451 sur 1670 sont après le centre du carrefour pour les
//!   voitures qu'ils regardent (de l'autre côté), 1395 sur 1670 ont le bras tourné vers le centre ;
//! - prop_sign_road_01a (panneau stop octogonal, face vers −Y) : 450 sur 525 sont avant le carrefour ;
//! - points de route : le point d'arrêt, relié au point du carrefour (9,8 à 27,5 m avant, 18,4 m en médiane),
//!   porte Special = 15 « TrafficLightJunctionStop » (feux) ou 16 « StopSign » (stop) ; Special est rangé dans
//!   Flags1 >> 3 (YndNode::get_Special 0x06002ede, set_Special 0x06002edf).

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use crate::roads::Junction;

pub const LIGHT_SHORT: &str = "prop_traffic_01a";
pub const LIGHT_LONG: &str = "prop_traffic_01b";
pub const STOP_SIGN: &str = "prop_sign_road_01a";
/// Valeurs de YndNodeSpecialType (CodeWalker) écrites sur le point d'arrêt.
pub const SPECIAL_LIGHT: u8 = 15;
pub const SPECIAL_STOP: u8 = 16;
/// Demi-chaussée au-delà de laquelle on prend le feu à long bras (m) — **estimé** (bras de 5,5 m pour 01a).
const LONG_ARM_FROM: f32 = 6.0;

pub const CONTROL_NAMES: [&str; 4] = ["Automatique", "Feux", "Stops", "Rien"];

/// Valeur par défaut (serde) : feux et stops automatiques activés.
pub fn yes() -> bool {
    true
}

/// Signalisation de chaque branche : 0 rien, 1 feu, 2 stop.
/// Automatique — **estimé** : si toutes les branches ont la même largeur, feux en ville (routes à trottoirs,
/// avenues, autoroutes), stops partout à la campagne ; sinon stops sur les branches plus étroites (une rue qui
/// rejoint une avenue), rien sur la grande route.
pub fn arm_controls(j: &Junction) -> Vec<u8> {
    let n = j.arms.len();
    match j.control {
        1 => return vec![1; n],
        2 => return vec![2; n],
        3 => return vec![0; n],
        _ => {}
    }
    if n < 3 {
        return vec![0; n];
    }
    let maxw = j.arms.iter().map(|a| a.hw).fold(0.0, f32::max);
    let minw = j.arms.iter().map(|a| a.hw).fold(f32::MAX, f32::min);
    if maxw - minw < 0.5 {
        let city = j.arms.iter().all(|a| a.total - a.hw > 1.0 || a.hw >= 7.0);
        return vec![if city && !j.dirt { 1 } else { 2 }; n];
    }
    j.arms.iter().map(|a| if a.hw < maxw - 0.5 { 2 } else { 0 }).collect()
}

/// Orientation d'un feu ou d'un panneau qui regarde les voitures roulant dans le sens `d` (le +Y du modèle
/// suit `d`, sa face −Y est tournée vers elles).
fn facing(d: V3) -> Quat {
    Quat::from_z((-d.x).atan2(d.y))
}

/// Feux et panneaux d'un carrefour : (modèle, position, orientation).
pub fn junction_props(j: &Junction) -> Vec<(&'static str, V3, Quat)> {
    let c = v3(j.pos[0], j.pos[1], j.pos[2]);
    let ctl = arm_controls(j);
    let mut out = Vec::new();
    for (i, a) in j.arms.iter().enumerate() {
        if ctl[i] == 0 {
            continue;
        }
        let dir = v3(a.dir[0], a.dir[1], 0.0).norm();
        // voitures qui arrivent par cette branche : sens d, leur droite r
        let d = dir * -1.0;
        let r = v3(d.y, -d.x, 0.0);
        // route croisée : la plus large des branches en travers
        let cross = j.arms.iter().filter(|b| (b.dir[0] * a.dir[0] + b.dir[1] * a.dir[1]).abs() < 0.7);
        let (ohw, ototal) = cross.fold((0.0f32, 0.0f32), |acc, b| (acc.0.max(b.hw), acc.1.max(b.total)));
        let walk = a.total - a.hw > 0.5;
        // sur le trottoir (un peu plus près de la bordure que du bord extérieur), sinon 1,2 m après la chaussée
        let lateral = if walk { a.hw + (a.total - a.hw) * 0.45 } else { a.hw + 1.2 };
        let base = |on_walk: bool| j.pos[2] + 0.08 + if on_walk { 0.18 } else { 0.0 };
        if ctl[i] == 1 {
            let o_walk = ototal - ohw > 0.5;
            let along = if o_walk { ohw + (ototal - ohw) * 0.45 } else { ohw + 1.2 };
            let p = c + d * along + r * lateral;
            let model = if a.hw > LONG_ARM_FROM { LIGHT_LONG } else { LIGHT_SHORT };
            out.push((model, v3(p.x, p.y, base(walk && o_walk)), facing(d)));
        } else {
            let p = c + dir * (a.len + 1.0) + r * lateral;
            out.push((STOP_SIGN, v3(p.x, p.y, base(walk)), facing(d)));
        }
    }
    out
}

/// Special de chaque point de route (0 sinon) : le point d'arrêt de chaque branche signalée est le premier
/// point de cette branche avant le carrefour (entre la moitié du carrefour et un écart de points plus loin).
pub fn node_specials(t: &crate::terrain::Terrain, nodes: &[crate::traffic::TNode]) -> Vec<u8> {
    let mut out = vec![0u8; nodes.len()];
    for j in &t.junctions {
        let c = v3(j.pos[0], j.pos[1], j.pos[2]);
        let ctl = arm_controls(j);
        for (i, a) in j.arms.iter().enumerate() {
            if ctl[i] == 0 {
                continue;
            }
            let dir = v3(a.dir[0], a.dir[1], 0.0).norm();
            let side = v3(dir.y, -dir.x, 0.0);
            let best = nodes
                .iter()
                .enumerate()
                .filter_map(|(k, n)| {
                    let rel = n.pos - c;
                    let along = rel.x * dir.x + rel.y * dir.y;
                    let lat = (rel.x * side.x + rel.y * side.y).abs();
                    (along > a.len * 0.5 && along < a.len + crate::traffic::NODE_STEP + 3.0 && lat < a.hw + 3.0).then_some((k, along))
                })
                .min_by(|x, y| x.1.partial_cmp(&y.1).unwrap());
            if let Some((k, _)) = best {
                out[k] = if ctl[i] == 1 { SPECIAL_LIGHT } else { SPECIAL_STOP };
            }
        }
    }
    out
}

impl DwApp {
    /// Repose les feux et panneaux de tous les carrefours (après une nouvelle route ou un changement de réglage) :
    /// les anciens objets posés par DragoWalker sont retirés, les nouveaux vont dans la carte de destination.
    pub fn refresh_signals(&mut self) {
        let Some(t) = self.proj.terrain.as_ref() else { return };
        let auto = t.auto_signals;
        let old: std::collections::HashSet<u64> = t.junctions.iter().flat_map(|j| j.props.iter().copied()).collect();
        if !old.is_empty() {
            self.proj.delete_ids(&old);
        }
        let lod_of = |name: &str, w: Option<&crate::gta::world::GameWorld>| w.and_then(|w| w.archetypes.get(&crate::gta::crypto::joaat(name))).map(|a| a.lod_dist).filter(|d| *d > 0.0).unwrap_or(150.0);
        let mut placed: Vec<Vec<crate::model::Entity>> = Vec::new();
        if let Some(t) = self.proj.terrain.as_ref() {
            for j in &t.junctions {
                let mut v = Vec::new();
                if auto {
                    for (name, p, q) in junction_props(j) {
                        // pied posé sur ce qu'il y a vraiment dessous (trottoir, chaussée, accotement ou terrain)
                        let p = v3(p.x, p.y, t.surface_below(p.x, p.y, j.pos[2] + 2.0).unwrap_or(p.z));
                        let mut e = crate::model::Entity::new(name, p);
                        e.rot = q;
                        e.lod_dist = lod_of(name, self.game.world.as_deref());
                        v.push(e);
                    }
                }
                placed.push(v);
            }
        }
        let yi = self.ensure_target_ymap();
        let mut n = 0;
        if let Some(t) = self.proj.terrain.as_mut() {
            for (j, list) in t.junctions.iter_mut().zip(placed) {
                j.props = list.iter().map(|e| e.id).collect();
                n += list.len();
                self.proj.ymaps[yi].entities.extend(list);
            }
        }
        let _ = n;
        self.clean_selection();
        self.sync_game_project();
    }

    /// Lampadaires de toutes les routes (Road::lamp_spots) : objets du jeu, retirés puis reposés. Le bras de
    /// prop_streetlight_01 part vers −Y du modèle (boîte du jeu : y de −2,42 à 0,46) : −Y est tourné vers la chaussée ;
    /// prop_streetlight_03d a ses deux bras le long de Y, mis en travers de l'autoroute.
    pub fn refresh_furniture(&mut self) {
        let Some(t) = self.proj.terrain.as_ref() else { return };
        let old: std::collections::HashSet<u64> = t.roads.iter().flat_map(|r| r.props.iter().copied()).collect();
        if !old.is_empty() {
            self.proj.delete_ids(&old);
        }
        let lod_of = |name: &str, w: Option<&crate::gta::world::GameWorld>| w.and_then(|w| w.archetypes.get(&crate::gta::crypto::joaat(name))).map(|a| a.lod_dist).filter(|d| *d > 0.0).unwrap_or(150.0);
        let mut placed: Vec<Vec<crate::model::Entity>> = Vec::new();
        if let Some(t) = self.proj.terrain.as_ref() {
            for r in &t.roads {
                let mut v = Vec::new();
                for (p, toward, double) in r.lamp_spots() {
                    let name = if double { crate::roads::LAMP_DOUBLE } else { crate::roads::LAMP_SINGLE };
                    let p = v3(p.x, p.y, t.surface_below(p.x, p.y, p.z + 1.0).unwrap_or(p.z));
                    let mut e = crate::model::Entity::new(name, p);
                    e.rot = if double { facing(toward) } else { facing(toward * -1.0) };
                    e.lod_dist = lod_of(name, self.game.world.as_deref());
                    v.push(e);
                }
                placed.push(v);
            }
        }
        let yi = self.ensure_target_ymap();
        if let Some(t) = self.proj.terrain.as_mut() {
            for (r, list) in t.roads.iter_mut().zip(placed) {
                r.props = list.iter().map(|e| e.id).collect();
                self.proj.ymaps[yi].entities.extend(list);
            }
        }
        self.clean_selection();
        self.sync_game_project();
    }

    /// Nombre de lampadaires posés par DragoWalker.
    pub fn lamps_count(&self) -> usize {
        self.proj.terrain.as_ref().map(|t| t.roads.iter().map(|r| r.props.len()).sum()).unwrap_or(0)
    }

    /// Nombre de feux et de panneaux posés par DragoWalker.
    pub fn signals_count(&self) -> (usize, usize) {
        let ids: std::collections::HashSet<u64> = self.proj.terrain.as_ref().map(|t| t.junctions.iter().flat_map(|j| j.props.iter().copied()).collect()).unwrap_or_default();
        let (mut l, mut s) = (0, 0);
        for e in self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| ids.contains(&e.id)) {
            if e.archetype == STOP_SIGN {
                s += 1;
            } else {
                l += 1;
            }
        }
        (l, s)
    }
}
