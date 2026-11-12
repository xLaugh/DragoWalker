//! Lecture des fichiers « Meta » (ymap, ytyp) et de leurs structures.
//! En-tête Meta, blocs de données et pointeurs : MetaPOINTER (0x06003389..0x0600338c),
//! Array_StructurePointer (0x060032f2..0x060032f4). Disposition des structures : CMapData, CEntityDef,
//! CMloInstanceDef, CMapTypes, CBaseArchetypeDef (CodeWalker.GameFiles, ev_2a93113d).

use super::crypto::joaat;
use super::rpf::Resource;

pub struct Meta<'a> {
    pub r: &'a Resource,
    /// (hachage du nom de structure, position, longueur)
    pub blocks: Vec<(u32, usize, usize)>,
}

impl<'a> Meta<'a> {
    pub fn parse(r: &'a Resource) -> Option<Meta<'a>> {
        let blocks_ptr = r.ptr(r.u64(0x30))?;
        let count = r.u16(0x4C) as usize;
        let mut blocks = Vec::with_capacity(count);
        for i in 0..count {
            let b = blocks_ptr + i * 16;
            let hash = r.u32(b);
            let len = r.i32(b + 4).max(0) as usize;
            let pos = r.ptr(r.u64(b + 8))?;
            blocks.push((hash, pos, len));
        }
        Some(Meta { r, blocks })
    }

    /// Position d'un pointeur Meta (bloc numéroté à partir de 1, décalage sur 20 bits).
    pub fn resolve(&self, p: u64) -> Option<(u32, usize)> {
        let id = (p & 0xFFF) as usize;
        if id == 0 {
            return None;
        }
        let off = ((p >> 12) & 0xF_FFFF) as usize;
        let (h, pos, len) = *self.blocks.get(id - 1)?;
        (off < len.max(1)).then_some((h, pos + off))
    }

    /// Tableau de structures : (position du premier élément, nombre).
    pub fn array(&self, at: usize) -> Option<(u32, usize, usize)> {
        let p = self.r.u64(at);
        let n = self.r.u16(at + 8) as usize;
        if n == 0 {
            return None;
        }
        let (h, pos) = self.resolve(p)?;
        Some((h, pos, n))
    }

    pub fn first_block(&self, hash: u32) -> Option<usize> {
        self.blocks.iter().find(|b| b.0 == hash).map(|b| b.1)
    }
}

/// Identifiants des structures (énumération MetaName de CodeWalker).
pub const H_CMAPDATA: u32 = 3545841574;
pub const H_CENTITYDEF: u32 = 3461354627;
pub const H_CMLOINSTANCEDEF: u32 = 164374718;
pub const H_CMAPTYPES: u32 = 3649811809;
pub const H_CMLOARCHETYPEDEF: u32 = 273704021;

pub fn struct_hash(name: &str) -> u32 {
    match name {
        "CMapData" => H_CMAPDATA,
        "CEntityDef" => H_CENTITYDEF,
        "CMloInstanceDef" => H_CMLOINSTANCEDEF,
        "CMapTypes" => H_CMAPTYPES,
        "CMloArchetypeDef" => H_CMLOARCHETYPEDEF,
        _ => joaat(name),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GEntity {
    pub arch: u32,
    pub flags: u32,
    pub pos: [f32; 3],
    /// quaternion (x, y, z, w) tel qu'écrit dans le fichier
    pub rot: [f32; 4],
    pub scale_xy: f32,
    pub scale_z: f32,
    pub parent: i32,
    pub lod_dist: f32,
    pub child_lod_dist: f32,
    pub lod_level: u8,
    pub num_children: u16,
    pub mlo: bool,
    pub guid: u32,
    pub priority: u32,
    pub ao_mult: i32,
    pub art_ao: i32,
    pub tint: u32,
    pub group_id: u32,
    pub floor_id: u32,
    pub num_exit_portals: u32,
    pub mlo_inst_flags: u32,
}

pub struct GYmap {
    pub name: u32,
    pub parent: u32,
    pub flags: u32,
    pub content_flags: u32,
    pub streaming_min: [f32; 3],
    pub streaming_max: [f32; 3],
    pub entities: Vec<GEntity>,
    /// groupes d'entités activés par défaut de chaque intérieur placé : (index de l'objet, hachages)
    pub mlo_sets: Vec<(usize, Vec<u32>)>,
}

const CMAPDATA_SIZE: usize = 512;

fn read_entity(r: &Resource, p: usize, mlo: bool) -> GEntity {
    GEntity {
        arch: r.u32(p + 8),
        flags: r.u32(p + 12),
        pos: r.v3(p + 32),
        rot: [r.f32(p + 48), r.f32(p + 52), r.f32(p + 56), r.f32(p + 60)],
        scale_xy: r.f32(p + 64),
        scale_z: r.f32(p + 68),
        parent: r.i32(p + 72),
        lod_dist: r.f32(p + 76),
        child_lod_dist: r.f32(p + 80),
        lod_level: r.u32(p + 84) as u8,
        num_children: r.u32(p + 88).min(u16::MAX as u32) as u16,
        mlo,
        guid: r.u32(p + 16),
        priority: r.u32(p + 92),
        ao_mult: r.i32(p + 112),
        art_ao: r.i32(p + 116),
        tint: r.u32(p + 120),
        group_id: if mlo { r.u32(p + 128) } else { 0 },
        floor_id: if mlo { r.u32(p + 132) } else { 0 },
        num_exit_portals: if mlo { r.u32(p + 152) } else { 0 },
        mlo_inst_flags: if mlo { r.u32(p + 156) } else { 0 },
    }
}

pub fn parse_ymap(r: &Resource) -> Option<GYmap> {
    let m = Meta::parse(r)?;
    let h_map = struct_hash("CMapData");
    let h_mlo = struct_hash("CMloInstanceDef");
    let p = m.first_block(h_map).or_else(|| m.blocks.first().map(|b| b.1))?;
    if p + CMAPDATA_SIZE > r.data.len() {
        return None;
    }
    let mut y = GYmap {
        name: r.u32(p + 8),
        parent: r.u32(p + 12),
        flags: r.u32(p + 16),
        content_flags: r.u32(p + 20),
        streaming_min: r.v3(p + 32),
        streaming_max: r.v3(p + 48),
        entities: Vec::new(),
        mlo_sets: Vec::new(),
    };
    // entities : Array_StructurePointer à +96 ; chaque élément est un MetaPOINTER de 8 octets
    if let Some((_, arr, n)) = m.array(p + 96) {
        y.entities.reserve(n);
        for i in 0..n {
            let ptr = r.u64(arr + i * 8);
            if let Some((h, ep)) = m.resolve(ptr) {
                if h == h_mlo {
                    // CMloInstanceDef : defaultEntitySets (Array_uint) à +136
                    if let Some((_, sp, sn)) = m.array(ep + 136) {
                        let sets = (0..sn.min(64)).map(|k| r.u32(sp + k * 4)).collect();
                        y.mlo_sets.push((y.entities.len(), sets));
                    }
                }
                y.entities.push(read_entity(r, ep, h == h_mlo));
            }
        }
    }
    Some(y)
}

#[derive(Clone, Debug)]
pub struct GArchetype {
    pub name: u32,
    pub bb_min: [f32; 3],
    pub bb_max: [f32; 3],
    #[allow(dead_code)]
    pub bs_centre: [f32; 3],
    pub bs_radius: f32,
    /// distance des textures détaillées (CBaseArchetypeDef.hdTextureDist, +84 : entre bsRadius +80 et name +88)
    pub hd_texture_dist: f32,
    pub lod_dist: f32,
    pub flags: u32,
    pub texture_dict: u32,
    pub drawable_dict: u32,
    pub asset_type: u32,
    #[allow(dead_code)]
    pub asset_name: u32,
    pub mlo: bool,
    /// intérieur : objets (avec l'index de leur groupe d'entités, -1 = toujours là) et noms des groupes
    pub mlo_children: Vec<(GEntity, i32)>,
    pub mlo_sets: Vec<u32>,
}

/// CMapTypes : archetypes = Array_StructurePointer à +24.
pub fn parse_ytyp(r: &Resource) -> Vec<GArchetype> {
    let mut out = Vec::new();
    let Some(m) = Meta::parse(r) else { return out };
    let h_types = struct_hash("CMapTypes");
    let h_mlo = struct_hash("CMloArchetypeDef");
    let Some(p) = m.first_block(h_types).or_else(|| m.blocks.first().map(|b| b.1)) else { return out };
    if let Some((_, arr, n)) = m.array(p + 24) {
        for i in 0..n {
            if let Some((h, a)) = m.resolve(r.u64(arr + i * 8)) {
                // CMloArchetypeDefData après les 144 octets de base : entities +152, entitySets +200
                let mut mlo_children = Vec::new();
                let mut mlo_sets = Vec::new();
                if h == h_mlo {
                    if let Some((_, ea, en)) = m.array(a + 152) {
                        for k in 0..en.min(4096) {
                            if let Some((_, ep)) = m.resolve(r.u64(ea + k * 8)) {
                                mlo_children.push((read_entity(r, ep, false), -1));
                            }
                        }
                    }
                    // CMloEntitySet (48 octets) : name +8, entities +32
                    if let Some((_, sa, sn)) = m.array(a + 200) {
                        for si in 0..sn.min(256) {
                            let s = sa + si * 48;
                            mlo_sets.push(r.u32(s + 8));
                            if let Some((_, ea, en)) = m.array(s + 32) {
                                for k in 0..en.min(4096) {
                                    if let Some((_, ep)) = m.resolve(r.u64(ea + k * 8)) {
                                        mlo_children.push((read_entity(r, ep, false), si as i32));
                                    }
                                }
                            }
                        }
                    }
                }
                out.push(GArchetype {
                    mlo_children,
                    mlo_sets,
                    lod_dist: r.f32(a + 8),
                    flags: r.u32(a + 12),
                    bb_min: r.v3(a + 32),
                    bb_max: r.v3(a + 48),
                    bs_centre: r.v3(a + 64),
                    bs_radius: r.f32(a + 80),
                    hd_texture_dist: r.f32(a + 84),
                    name: r.u32(a + 88),
                    texture_dict: r.u32(a + 92),
                    drawable_dict: r.u32(a + 100),
                    asset_type: r.u32(a + 108),
                    asset_name: r.u32(a + 112),
                    mlo: h == h_mlo,
                });
            }
        }
    }
    out
}
