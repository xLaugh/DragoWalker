//! Version lointaine reprise du jeu, « comme dans GTA V de base » :
//! - un bâtiment du jeu copié reçoit une copie de SA version lointaine (son parent LOD dans la carte d'origine),
//!   déplacée et tournée comme la copie ; la carte du projet devient la carte détaillée et une carte « _lod »
//!   est écrite à côté (CMapData.parent, CEntityDef.parentIndex, lodLevel LOD) ;
//! - seulement si ce parent ne sert qu'à ce bâtiment (numChildren = 1) : un parent commun à tout un pâté de
//!   maisons ferait apparaître les voisins de loin ; sinon le bâtiment garde sa version détaillée jusqu'à la
//!   distance de ce parent ;
//! - props, arbres… : la distance d'affichage de leur type dans le jeu (CBaseArchetypeDef.lodDist) au lieu de
//!   la valeur par défaut de 200 m, comme le jeu les pose.

use crate::app::DwApp;
use crate::math::Quat;
use crate::model::{Entity, Ymap};

impl DwApp {
    /// Objet de la carte d'origine posé avec ce type (version détaillée qui a un parent), mis en mémoire.
    fn original_of(&self, h: u32) -> Option<usize> {
        let w = self.game.world.as_ref()?;
        w.ents.iter().position(|e| e.arch == h && e.lod_level == 0 && e.parent >= 0 && w.ymaps[e.ymap as usize].visible).or_else(|| w.ents.iter().position(|e| e.arch == h && e.lod_level == 0))
    }

    /// Pour l'export d'une carte du projet : (carte détaillée, carte des versions lointaines éventuelle).
    pub fn lod_split(&self, y: &Ymap) -> (Ymap, Option<Ymap>, usize) {
        let mut hd = y.clone();
        let Some(w) = self.game.world.as_ref() else { return (hd, None, 0) };
        let mut lod = Ymap::new(&format!("{}_lod", y.name));
        let mut cache: std::collections::HashMap<u32, Option<usize>> = std::collections::HashMap::new();
        for e in hd.entities.iter_mut() {
            if e.mlo.is_some() {
                continue;
            }
            let h = crate::gta::crypto::joaat(&e.archetype.to_lowercase());
            let Some(a) = w.archetypes.get(&h) else { continue };
            // distance du jeu pour ce type quand l'objet a gardé la valeur par défaut
            if (e.lod_dist - crate::mech::ENTITY_LOD_DIST).abs() < 0.01 && a.lod_dist > 0.0 {
                e.lod_dist = a.lod_dist;
            }
            if crate::buildings::is_prop_name(&e.archetype.to_lowercase()) {
                continue;
            }
            let orig = *cache.entry(h).or_insert_with(|| self.original_of(h));
            let Some(oi) = orig else { continue };
            let o = &w.ents[oi];
            let Some(pi) = w.lod_parent(oi) else { continue };
            let p = &w.ents[pi];
            if p.nchild != 1 {
                // parent commun à plusieurs bâtiments : on garde la version détaillée jusqu'à sa distance
                e.lod_dist = e.lod_dist.max(p.lod_dist.min(1500.0));
                continue;
            }
            // copie de la version lointaine, déplacée et tournée comme la copie du bâtiment
            let dq = e.rot.mul(o.rot.inverse()).normalized();
            let mut l = Entity::new(&w.name_of(p.arch), e.pos + dq.rotate(p.pos - o.pos));
            l.rot = dq.mul(p.rot).normalized();
            l.scale_xy = p.sxy;
            l.scale_z = p.sz;
            // les niveaux au-dessus (SLOD) sont communs à tout un quartier et ne sont pas copiés : la version
            // lointaine reste affichée jusqu'à la distance du plus haut d'entre eux (3 000 m au plus)
            let mut far = p.lod_dist;
            let mut up = w.lod_parent(pi);
            let mut guard = 0;
            while let Some(q) = up {
                far = far.max(w.ents[q].lod_dist);
                up = w.lod_parent(q);
                guard += 1;
                if guard > 6 {
                    break;
                }
            }
            l.lod_dist = far.min(3000.0);
            l.child_lod_dist = p.child_lod_dist;
            l.lod_level = "LODTYPES_DEPTH_LOD".into();
            l.num_children = 1;
            l.parent_index = -1;
            e.parent_index = lod.entities.len() as i32;
            e.lod_level = "LODTYPES_DEPTH_HD".into();
            e.lod_dist = o.lod_dist;
            lod.entities.push(l);
        }
        let n = lod.entities.len();
        if n == 0 {
            return (hd, None, 0);
        }
        hd.parent = lod.name.clone();
        let _ = Quat::IDENTITY;
        (hd, Some(lod), n)
    }
}
