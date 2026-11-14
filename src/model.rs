//! Données du projet : fichiers ymap (placements), ytyp (types / intérieurs MLO).

use crate::catalog;
use crate::math::{v3, Quat, V3};
use crate::mech;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub fn new_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
/// Après un chargement, s'assurer que les nouveaux identifiants ne se répètent pas.
pub fn bump_ids(max: u64) {
    NEXT_ID.fetch_max(max + 1, Ordering::Relaxed);
}

/// Données en plus pour un placement d'intérieur (CMloInstanceDef).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MloInstData {
    pub group_id: u32,
    pub floor_id: u32,
    pub default_entity_sets: Vec<String>,
    pub num_exit_portals: u32,
    pub mlo_inst_flags: u32,
}

/// Un objet placé (CEntityDef). `rot` est l'orientation réelle dans le monde ;
/// le fichier stocke son inverse (voir MECHANICS.md, YmapEntityDef::.ctor).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub id: u64,
    pub archetype: String,
    pub pos: V3,
    pub rot: Quat,
    pub scale_xy: f32,
    pub scale_z: f32,
    pub flags: u32,
    pub guid: u32,
    pub parent_index: i32,
    pub lod_dist: f32,
    pub child_lod_dist: f32,
    pub lod_level: String,
    pub num_children: u32,
    pub priority: String,
    pub ao_mult: i32,
    pub art_ao: i32,
    pub tint: u32,
    /// Pièce de l'intérieur (pour les entités d'un MLO).
    pub room: usize,
    pub mlo: Option<MloInstData>,
    /// Extensions (lumières, particules...) gardées telles quelles depuis le XML.
    #[serde(default)]
    pub extensions_xml: String,
    /// position dans le ymap du jeu d'origine (copie d'un ymap du jeu), -1 sinon
    #[serde(default = "minus_one")]
    pub game_index: i32,
    /// pose d'un groupe dont l'objet fait partie (0 = aucune) : un clic prend tout le bloc (groups.rs)
    #[serde(default)]
    pub group_inst: u64,
}

fn minus_one() -> i32 {
    -1
}

impl Entity {
    pub fn new(archetype: &str, pos: V3) -> Entity {
        Entity {
            id: new_id(),
            archetype: archetype.to_string(),
            pos,
            rot: Quat::IDENTITY,
            scale_xy: 1.0,
            scale_z: 1.0,
            flags: mech::ENTITY_FLAGS,
            guid: 0,
            parent_index: mech::ENTITY_PARENT_INDEX,
            lod_dist: mech::ENTITY_LOD_DIST,
            child_lod_dist: 0.0,
            lod_level: mech::ENTITY_LOD_LEVEL.to_string(),
            num_children: 0,
            priority: mech::ENTITY_PRIORITY.to_string(),
            ao_mult: mech::ENTITY_AO,
            art_ao: mech::ENTITY_AO,
            tint: 0,
            room: 0,
            mlo: None,
            extensions_xml: String::new(),
            game_index: -1,
            group_inst: 0,
        }
    }
    pub fn scale(&self) -> V3 {
        v3(self.scale_xy, self.scale_xy, self.scale_z)
    }
    /// Boîte locale (bbMin, bbMax) avant échelle.
    pub fn local_box(&self, proj: &Project) -> (V3, V3) {
        if let Some(a) = proj.find_mlo(&self.archetype) {
            return (a.bb_min(), a.bb_max());
        }
        catalog::box_for(&self.archetype)
    }
    /// Les 8 coins dans le monde.
    pub fn corners(&self, proj: &Project) -> [V3; 8] {
        let (mn, mx) = self.local_box(proj);
        let s = self.scale();
        let mn = mn.mulv(s);
        let mx = mx.mulv(s);
        let mut out = [V3::ZERO; 8];
        for (i, c) in out.iter_mut().enumerate() {
            let p = v3(
                if i & 1 == 0 { mn.x } else { mx.x },
                if i & 2 == 0 { mn.y } else { mx.y },
                if i & 4 == 0 { mn.z } else { mx.z },
            );
            *c = self.pos + self.rot.rotate(p);
        }
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ymap {
    pub name: String,
    pub parent: String,
    pub flags: u32,
    pub content_flags: u32,
    pub entities: Vec<Entity>,
    pub visible: bool,
    /// La carte d'exemple générée (remplace la vraie carte du jeu).
    pub sample: bool,
    /// Sections non gérées (carGenerators, LODLights...) gardées telles quelles.
    #[serde(default)]
    pub raw_sections: Vec<(String, String)>,
    /// Copie d'un ymap du jeu (la version du jeu est alors cachée dans la vue).
    #[serde(default)]
    pub from_game: bool,
    /// véhicules garés (générateurs de véhicules)
    #[serde(default)]
    pub car_gens: Vec<crate::life::CarGen>,
    /// boîtes d'occlusion ajoutées dans DragoWalker (occlusion.rs)
    #[serde(default)]
    pub occluders: Vec<crate::occlusion::BoxOcc>,
    /// PNJ posés (exportés en script client)
    #[serde(default)]
    pub peds: Vec<crate::life::PedSpot>,
}

impl Ymap {
    pub fn new(name: &str) -> Ymap {
        Ymap {
            name: name.to_string(),
            parent: String::new(),
            flags: 0,
            content_flags: mech::NEW_YMAP_CONTENT_FLAGS,
            entities: Vec::new(),
            visible: true,
            sample: false,
            raw_sections: Vec::new(),
            from_game: false,
            car_gens: Vec::new(),
            peds: Vec::new(),
            occluders: Vec::new(),
        }
    }

    /// Recalcule flags et contentFlags (règles de YmapFile::CalcFlags).
    pub fn calc_flags(&mut self) {
        let mut flags = 0u32;
        let mut cf = 0u32;
        for e in &self.entities {
            match e.lod_level.as_str() {
                "LODTYPES_DEPTH_HD" | "LODTYPES_DEPTH_ORPHANHD" => cf |= 1,
                "LODTYPES_DEPTH_LOD" => {
                    cf |= 2;
                    flags |= 2;
                }
                "LODTYPES_DEPTH_SLOD1" => {
                    cf |= 16;
                    flags |= 2;
                }
                _ => {
                    cf |= 4 | 16;
                    flags |= 2;
                }
            }
            if e.mlo.is_some() {
                cf |= 8;
            }
        }
        // boîtes d'occlusion : bit 5 (32) de contentFlags (CodeWalker YmapFile::CalcFlags 0x06002cd4, IL_0161–IL_0172)
        if !self.occluders.is_empty() {
            cf |= 32;
        }
        // garde les bits qu'on ne gère pas (physique, lumières...)
        self.flags = (self.flags & !2) | flags;
        self.content_flags = (self.content_flags & !(1 | 2 | 4 | 8 | 16)) | cf;
    }

    /// (entitiesMin, entitiesMax, streamingMin, streamingMax) — YmapFile::CalcExtents.
    pub fn extents(&self, proj: &Project) -> (V3, V3, V3, V3) {
        let big = 3.0e38f32;
        let (mut emn, mut emx) = (v3(big, big, big), v3(-big, -big, -big));
        let (mut smn, mut smx) = (emn, emx);
        for e in &self.entities {
            for c in e.corners(proj) {
                emn = emn.min(c);
                emx = emx.max(c);
            }
            let (bmn, bmx) = e.local_box(proj);
            let s = e.scale();
            let l = e.lod_dist;
            let mn = bmn.mulv(s) - v3(l, l, l);
            let mx = bmx.mulv(s) + v3(l, l, l);
            for i in 0..8 {
                let p = v3(
                    if i & 1 == 0 { mn.x } else { mx.x },
                    if i & 2 == 0 { mn.y } else { mx.y },
                    if i & 4 == 0 { mn.z } else { mx.z },
                );
                let w = e.pos + e.rot.rotate(p);
                smn = smn.min(w);
                smx = smx.max(w);
            }
        }
        // véhicules garés, comme YmapFile::CalcExtents (0x06002cd5) : ± perpendicularLength pour les objets,
        // ± 2 × perpendicularLength pour le chargement
        for c in &self.car_gens {
            let w = c.width;
            emn = emn.min(c.pos - v3(w, w, w));
            emx = emx.max(c.pos + v3(w, w, w));
            smn = smn.min(c.pos - v3(2.0 * w, 2.0 * w, 2.0 * w));
            smx = smx.max(c.pos + v3(2.0 * w, 2.0 * w, 2.0 * w));
        }
        if self.entities.is_empty() && self.car_gens.is_empty() {
            return (V3::ZERO, V3::ZERO, V3::ZERO, V3::ZERO);
        }
        (emn, emx, smn, smx)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Room {
    pub name: String,
    pub bb_min: V3,
    pub bb_max: V3,
    pub blend: f32,
    pub timecycle: String,
    pub secondary_timecycle: String,
    pub flags: u32,
    pub floor_id: u32,
    pub ext_vis_depth: i32,
    /// pièce de forme libre (design.rs) : sol, plafond et murs viennent de la coque modelable, pas de sa boîte
    #[serde(default)]
    pub free: bool,
}

impl Room {
    pub fn new(name: &str, bb_min: V3, bb_max: V3) -> Room {
        Room {
            name: name.to_string(),
            bb_min,
            bb_max,
            blend: mech::ROOM_BLEND,
            timecycle: String::new(),
            secondary_timecycle: String::new(),
            flags: mech::ROOM_FLAGS,
            floor_id: 0,
            ext_vis_depth: mech::ROOM_EXT_VIS_DEPTH,
            free: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Portal {
    pub room_from: u32,
    pub room_to: u32,
    pub flags: u32,
    pub mirror_priority: u32,
    pub opacity: u32,
    pub audio_occlusion: u32,
    pub corners: [V3; 4],
    /// portail réuni (« Réduire les portails », meubler.rs) : les ouvertures d'origine, seules percées dans la coque
    /// et la collision (le rectangle réuni n'est qu'une zone de visibilité) ; vide = l'ouverture est `corners`.
    /// Gardé dans le projet seulement (pas écrit dans le XML du jeu).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<[V3; 4]>,
}

impl Portal {
    pub fn new(from: u32, to: u32, corners: [V3; 4]) -> Portal {
        Portal { room_from: from, room_to: to, flags: 0, mirror_priority: 0, opacity: 0, audio_occlusion: 0, corners, openings: Vec::new() }
    }

    /// Ouvertures réelles du portail (murs percés) : celles d'origine d'un portail réuni, sinon ses coins.
    pub fn holes(&self) -> Vec<[V3; 4]> {
        if self.openings.is_empty() {
            vec![self.corners]
        } else {
            self.openings.clone()
        }
    }
}

/// Modèle d'une vraie porte : un mot « door » dans le nom (« …_door », « door_… », « …_frontdoor », « door1 »…),
/// sans compter « outdoor » / « indoor », et pas une porte de meuble (armoire, placard, frigo, four, casier…) —
/// **estimé** d'après les noms des portes du jeu (v_ilev_fh_door, prop_com_ls_door_01, v_ilev_fib_door1).
pub fn is_door_model(name: &str) -> bool {
    let n = name.to_lowercase();
    // portes proposées par DragoWalker (plan, atelier)
    if crate::atelier::DOOR_MODELS.iter().any(|d| d.0 == n) {
        return true;
    }
    if crate::meubler::FURN_DOOR_EXCLUDE.iter().any(|x| n.contains(x)) {
        return false;
    }
    n.split(|c: char| c == '_' || c == '-' || c == ' ').any(|t| {
        let t = t.trim_end_matches(|c: char| c.is_ascii_digit());
        if t == "outdoor" || t == "indoor" || t.is_empty() {
            return false;
        }
        // « door », « doors », « doorl » / « doorr » (battants), « frontdoor », « frntdoor »…
        t.ends_with("door") || (t.starts_with("door") && t.len() <= 6)
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitySet {
    pub name: String,
    pub entities: Vec<Entity>,
    /// activé par le script client de l'export (dw_entity_sets.lua) dès que l'intérieur est chargé
    #[serde(default)]
    pub default_on: bool,
}

/// Un intérieur (CMloArchetypeDef).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MloArchetype {
    pub name: String,
    pub lod_dist: f32,
    pub flags: u32,
    pub mlo_flags: u32,
    pub entities: Vec<Entity>,
    pub rooms: Vec<Room>,
    pub portals: Vec<Portal>,
    pub entity_sets: Vec<EntitySet>,
}

impl MloArchetype {
    pub fn bb_min(&self) -> V3 {
        let mut m = v3(0.0, 0.0, 0.0);
        let mut first = true;
        for r in self.rooms.iter().skip(1) {
            m = if first { r.bb_min } else { m.min(r.bb_min) };
            first = false;
        }
        m
    }
    pub fn bb_max(&self) -> V3 {
        let mut m = v3(1.0, 1.0, 1.0);
        let mut first = true;
        for r in self.rooms.iter().skip(1) {
            m = if first { r.bb_max } else { m.max(r.bb_max) };
            first = false;
        }
        m
    }
    /// Indices des entités dans chaque pièce (attachedObjects).
    pub fn attached(&self, room: usize) -> Vec<usize> {
        self.entities.iter().enumerate().filter(|(i, e)| e.room == room && self.door_portal(*i).is_none()).map(|(i, _)| i).collect()
    }

    /// Portail dans lequel est posée une porte de l'intérieur : comme dans les intérieurs du jeu et des ressources
    /// FiveM (ma_diner, burgershot relus avec CodeWalker), une porte est rattachée au passage qu'elle ferme
    /// (attachedObjects du portail) et à aucune pièce — rattachée à une pièce, elle n'apparaissait pas en jeu
    /// (retour de la personne, 13 nov. 2026). Critère : vraie porte (is_door_model : pas un meuble à porte, pas
    /// « outdoor »), origine à moins de 0,8 m du cadre du portail.
    pub fn door_portal(&self, ei: usize) -> Option<usize> {
        let e = self.entities.get(ei)?;
        if !is_door_model(&e.archetype) {
            return None;
        }
        let mut best: Option<(f32, usize)> = None;
        for (pi, p) in self.portals.iter().enumerate() {
            let (mut mn, mut mx) = (p.corners[0], p.corners[0]);
            for c in &p.corners {
                mn = mn.min(*c);
                mx = mx.max(*c);
            }
            let m = 0.8;
            let d = v3((mn.x - m - e.pos.x).max(e.pos.x - mx.x - m).max(0.0), (mn.y - m - e.pos.y).max(e.pos.y - mx.y - m).max(0.0), (mn.z - m - e.pos.z).max(e.pos.z - mx.z - m).max(0.0)).len();
            let centre = (mn + mx) * 0.5;
            let dist = (centre - e.pos).len();
            if d <= 0.0 && best.map(|b| dist < b.0).unwrap_or(true) {
                best = Some((dist, pi));
            }
        }
        best.map(|b| b.1)
    }

    /// Objets rattachés à un portail (les portes posées dedans).
    pub fn portal_attached(&self, pi: usize) -> Vec<usize> {
        (0..self.entities.len()).filter(|&i| self.door_portal(i) == Some(pi)).collect()
    }
    pub fn portal_count(&self, room: usize) -> usize {
        self.portals.iter().filter(|p| p.room_from as usize == room || p.room_to as usize == room).count()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ytyp {
    pub name: String,
    pub mlos: Vec<MloArchetype>,
    pub visible: bool,
    /// Archétypes simples (CBaseArchetypeDef...) gardés tels quels.
    #[serde(default)]
    pub raw_archetypes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub ymaps: Vec<Ymap>,
    pub ytyps: Vec<Ytyp>,
    /// terrain créé dans l'éditeur de terrain (carte vide)
    #[serde(default)]
    pub terrain: Option<crate::terrain::Terrain>,
    /// ressources MLO importées (fichiers du créateur, crédits, objet de référence)
    #[serde(default)]
    pub mlo_res: Vec<crate::mlo::MloRes>,
    /// orientation des placements d'intérieur selon CodeWalker (non inversée dans les fichiers) ; faux pour un
    /// projet enregistré avant la correction : il est corrigé à l'ouverture (fix_mlo_rotations)
    #[serde(default)]
    pub mlo_rot_fixed: bool,
    /// bâtiments du jeu édités (morceaux retirés à la main) : modèles « dw_cut_N »
    #[serde(default)]
    pub building_cuts: Vec<crate::bedit::BuildingCut>,
    /// intérieurs créés depuis un bâtiment (« Transformer en intérieur ») : (placement de l'intérieur, bâtiment) ;
    /// ils bougent et tournent ensemble
    #[serde(default)]
    pub mlo_links: Vec<(u64, u64)>,
    /// angle (radians) de l'intérieur par rapport à son bâtiment relié : direction des murs dans le modèle
    #[serde(default)]
    pub mlo_link_yaw: Vec<(u64, f32)>,
    /// groupes d'objets (« Faire un groupe ») à poser ailleurs
    #[serde(default)]
    pub groups: Vec<crate::groups::Group>,
    /// bâtiments de l'atelier (atelier.rs) : modèles « dw_bat_N »
    #[serde(default)]
    pub houses: Vec<crate::atelier::House>,
    /// coques modelables des intérieurs (design.rs) : murs, pièces de forme libre, blocs maçonnés
    #[serde(default)]
    pub designs: Vec<crate::design::Design>,
    /// textures importées par la personne (images PNG), pour les coques modelables
    #[serde(default)]
    pub custom_textures: Vec<crate::design::CustomTex>,
    /// ambiances du projet (timecycle modifiers personnalisés, ambiance.rs) : écrites dans dw_timecycmod.xml
    #[serde(default)]
    pub ambiances: Vec<crate::ambiance::ProjAmbiance>,
}

impl Project {
    /// Projets enregistrés quand DragoWalker inversait aussi la rotation des placements d'intérieur
    /// (CMloInstanceDef) : orientation lue = inverse de la vraie (R0⁻¹), et un tour G donné avec les poignées
    /// était appliqué par-dessus (G·R0⁻¹). Correction : vraie orientation d'origine R0 = (réf. lue)⁻¹ et
    /// orientation voulue G·R0 = actuelle · réf⁻¹ · réf⁻¹ ; placement d'intérieur sans référence : simplement
    /// inversé. Rend le nombre de placements corrigés.
    pub fn fix_mlo_rotations(&mut self) -> usize {
        if self.mlo_rot_fixed {
            return 0;
        }
        self.mlo_rot_fixed = true;
        let mut n = 0;
        let refs: Vec<(u64, Quat)> = self.mlo_res.iter().map(|r| (r.ref_id, r.ref_rot)).collect();
        for y in self.ymaps.iter_mut() {
            for e in y.entities.iter_mut().filter(|e| e.mlo.is_some()) {
                e.rot = match refs.iter().find(|r| r.0 == e.id) {
                    Some((_, rr)) => e.rot.mul(rr.inverse()).mul(rr.inverse()).normalized(),
                    None => e.rot.inverse().normalized(),
                };
                n += 1;
            }
        }
        let mlo_ids: Vec<u64> = self.ymaps.iter().flat_map(|y| y.entities.iter().filter(|e| e.mlo.is_some()).map(|e| e.id)).collect();
        for r in self.mlo_res.iter_mut().filter(|r| mlo_ids.contains(&r.ref_id)) {
            r.ref_rot = r.ref_rot.inverse().normalized();
        }
        n
    }
}

/// Un élément sélectionnable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemRef {
    /// (ymap, index de l'entité)
    Ent(usize, usize),
    /// (ytyp, mlo, index de l'entité)
    MloEnt(usize, usize, usize),
}

impl Project {
    pub fn find_mlo(&self, name: &str) -> Option<&MloArchetype> {
        self.ytyps.iter().flat_map(|y| y.mlos.iter()).find(|m| m.name.eq_ignore_ascii_case(name))
    }

    /// Placement (position, orientation) d'un intérieur dans le monde.
    pub fn mlo_placement(&self, name: &str) -> Option<(V3, Quat)> {
        for y in &self.ymaps {
            for e in &y.entities {
                if e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(name) {
                    return Some((e.pos, e.rot));
                }
            }
        }
        None
    }

    pub fn ent(&self, r: ItemRef) -> Option<&Entity> {
        match r {
            ItemRef::Ent(y, i) => self.ymaps.get(y)?.entities.get(i),
            ItemRef::MloEnt(t, m, i) => self.ytyps.get(t)?.mlos.get(m)?.entities.get(i),
        }
    }
    pub fn ent_mut(&mut self, r: ItemRef) -> Option<&mut Entity> {
        match r {
            ItemRef::Ent(y, i) => self.ymaps.get_mut(y)?.entities.get_mut(i),
            ItemRef::MloEnt(t, m, i) => self.ytyps.get_mut(t)?.mlos.get_mut(m)?.entities.get_mut(i),
        }
    }

    pub fn find_id(&self, id: u64) -> Option<ItemRef> {
        for (yi, y) in self.ymaps.iter().enumerate() {
            if let Some(i) = y.entities.iter().position(|e| e.id == id) {
                return Some(ItemRef::Ent(yi, i));
            }
        }
        for (ti, t) in self.ytyps.iter().enumerate() {
            for (mi, m) in t.mlos.iter().enumerate() {
                if let Some(i) = m.entities.iter().position(|e| e.id == id) {
                    return Some(ItemRef::MloEnt(ti, mi, i));
                }
            }
        }
        None
    }

    /// Entité exprimée dans le monde (les entités d'intérieur sont transformées par le placement).
    pub fn world_entity(&self, r: ItemRef) -> Option<Entity> {
        let e = self.ent(r)?.clone();
        match r {
            ItemRef::Ent(..) => Some(e),
            ItemRef::MloEnt(t, m, _) => {
                let mlo = &self.ytyps[t].mlos[m];
                let (p, q) = self.mlo_placement(&mlo.name).unwrap_or((V3::ZERO, Quat::IDENTITY));
                let mut w = e;
                w.pos = p + q.rotate(w.pos);
                w.rot = q.mul(w.rot).normalized();
                Some(w)
            }
        }
    }

    /// Change la position (dans le monde) d'un élément.
    pub fn set_world_pos(&mut self, r: ItemRef, world: V3) {
        match r {
            ItemRef::Ent(..) => {
                if let Some(e) = self.ent_mut(r) {
                    e.pos = world;
                }
            }
            ItemRef::MloEnt(t, m, _) => {
                let name = self.ytyps[t].mlos[m].name.clone();
                let (p, q) = self.mlo_placement(&name).unwrap_or((V3::ZERO, Quat::IDENTITY));
                let local = q.inverse().rotate(world - p);
                if let Some(e) = self.ent_mut(r) {
                    e.pos = local;
                }
            }
        }
    }

    /// Tous les éléments visibles.
    pub fn all_items(&self) -> Vec<ItemRef> {
        let mut v = Vec::new();
        for (yi, y) in self.ymaps.iter().enumerate() {
            if !y.visible {
                continue;
            }
            for i in 0..y.entities.len() {
                v.push(ItemRef::Ent(yi, i));
            }
        }
        for (ti, t) in self.ytyps.iter().enumerate() {
            if !t.visible {
                continue;
            }
            for (mi, m) in t.mlos.iter().enumerate() {
                for i in 0..m.entities.len() {
                    v.push(ItemRef::MloEnt(ti, mi, i));
                }
            }
        }
        v
    }

    /// Supprime des éléments par identifiant. Renvoie le nombre supprimé.
    pub fn delete_ids(&mut self, ids: &std::collections::HashSet<u64>) -> usize {
        let mut n = 0;
        for y in &mut self.ymaps {
            let before = y.entities.len();
            y.entities.retain(|e| !ids.contains(&e.id));
            n += before - y.entities.len();
        }
        for t in &mut self.ytyps {
            for m in &mut t.mlos {
                let before = m.entities.len();
                m.entities.retain(|e| !ids.contains(&e.id));
                n += before - m.entities.len();
            }
        }
        n
    }

    pub fn max_id(&self) -> u64 {
        let mut m = 0;
        for y in &self.ymaps {
            for e in &y.entities {
                m = m.max(e.id);
            }
        }
        for t in &self.ytyps {
            for mlo in &t.mlos {
                for e in &mlo.entities {
                    m = m.max(e.id);
                }
            }
        }
        m
    }

    pub fn entity_count(&self) -> usize {
        self.ymaps.iter().map(|y| y.entities.len()).sum::<usize>()
            + self.ytyps.iter().flat_map(|t| t.mlos.iter()).map(|m| m.entities.len()).sum::<usize>()
    }

    pub fn next_ymap_name(&self) -> String {
        let mut i = 1;
        loop {
            let n = format!("{}{}", mech::NEW_YMAP_PREFIX, i);
            if !self.ymaps.iter().any(|y| y.name == n) {
                return n;
            }
            i += 1;
        }
    }
    pub fn next_ytyp_name(&self) -> String {
        let mut i = 1;
        loop {
            let n = format!("{}{}", mech::NEW_YTYP_PREFIX, i);
            if !self.ytyps.iter().any(|y| y.name == n) {
                return n;
            }
            i += 1;
        }
    }
}

/// Crée un intérieur en grille : `cols` x `rows` pièces de `w` x `d` x `h` mètres,
/// avec la pièce « limbo » en 0, des portails entre pièces voisines et une porte vers l'extérieur.
pub fn build_mlo(name: &str, cols: usize, rows: usize, w: f32, d: f32, h: f32) -> MloArchetype {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let mut rooms = vec![Room::new("limbo", V3::ZERO, V3::ZERO)];
    let total_w = cols as f32 * w;
    let total_d = rows as f32 * d;
    let ox = -total_w * 0.5;
    let oy = -total_d * 0.5;
    for r in 0..rows {
        for c in 0..cols {
            let mn = v3(ox + c as f32 * w, oy + r as f32 * d, 0.0);
            let mx = v3(mn.x + w, mn.y + d, h);
            let label = if rows * cols == 1 { "piece_principale".to_string() } else { format!("piece_{}", r * cols + c + 1) };
            rooms.push(Room::new(&label, mn, mx));
        }
    }
    let idx = |c: usize, r: usize| (r * cols + c + 1) as u32;
    let mut portals = Vec::new();
    let door_w = (w.min(d) * 0.3).clamp(1.0, 2.0);
    let door_h = (h * 0.8).min(2.4);
    // portails entre pièces voisines (porte au milieu du mur)
    for r in 0..rows {
        for c in 0..cols {
            if c + 1 < cols {
                let x = ox + (c + 1) as f32 * w;
                let cy = oy + r as f32 * d + d * 0.5;
                portals.push(Portal::new(
                    idx(c, r),
                    idx(c + 1, r),
                    [
                        v3(x, cy - door_w * 0.5, 0.0),
                        v3(x, cy - door_w * 0.5, door_h),
                        v3(x, cy + door_w * 0.5, door_h),
                        v3(x, cy + door_w * 0.5, 0.0),
                    ],
                ));
            }
            if r + 1 < rows {
                let y = oy + (r + 1) as f32 * d;
                let cx = ox + c as f32 * w + w * 0.5;
                portals.push(Portal::new(
                    idx(c, r),
                    idx(c, r + 1),
                    [
                        v3(cx - door_w * 0.5, y, 0.0),
                        v3(cx - door_w * 0.5, y, door_h),
                        v3(cx + door_w * 0.5, y, door_h),
                        v3(cx + door_w * 0.5, y, 0.0),
                    ],
                ));
            }
        }
    }
    // porte d'entrée : de la première pièce vers l'extérieur (pièce 0), mur sud
    let cx = ox + w * 0.5;
    portals.push(Portal::new(
        mech::PORTAL_ROOM_FROM,
        mech::PORTAL_ROOM_TO,
        [
            v3(cx - door_w * 0.5, oy, 0.0),
            v3(cx - door_w * 0.5, oy, door_h),
            v3(cx + door_w * 0.5, oy, door_h),
            v3(cx + door_w * 0.5, oy, 0.0),
        ],
    ));
    MloArchetype {
        name: name.to_string(),
        lod_dist: 200.0,
        flags: 0,
        mlo_flags: 0,
        entities: Vec::new(),
        rooms,
        portals,
        entity_sets: Vec::new(),
    }
}

/// Trouve la pièce (index >= 1) qui contient un point local, sinon 0 (limbo). Avec des pièces sur plusieurs
/// étages, la hauteur compte aussi (à 0,5 m près) ; à défaut, seulement la position au sol.
pub fn room_at(mlo: &MloArchetype, local: V3) -> usize {
    for (i, r) in mlo.rooms.iter().enumerate().skip(1) {
        if local.x >= r.bb_min.x && local.x <= r.bb_max.x && local.y >= r.bb_min.y && local.y <= r.bb_max.y && local.z >= r.bb_min.z - 0.5 && local.z <= r.bb_max.z + 0.5 {
            return i;
        }
    }
    for (i, r) in mlo.rooms.iter().enumerate().skip(1) {
        if local.x >= r.bb_min.x && local.x <= r.bb_max.x && local.y >= r.bb_min.y && local.y <= r.bb_max.y {
            return i;
        }
    }
    if mlo.rooms.len() > 1 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod door_tests {
    use super::is_door_model;

    #[test]
    fn vraies_portes() {
        for n in ["prop_magenta_door", "v_ilev_ra_door4l", "v_ilev_fib_door1", "prop_com_ls_door_01", "v_ilev_fh_frontdoor", "v_ilev_mm_doorm_l", "prop_damdoor_01"] {
            assert!(is_door_model(n), "{n}");
        }
        for n in ["prop_wardrobe_door_01", "prop_fridge_door", "v_res_cabinet_door", "prop_outdoor_fan_01", "prop_indoor_plant", "prop_doorframe_01", "prop_table_03"] {
            assert!(!is_door_model(n), "{n}");
        }
    }
}
