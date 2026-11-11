//! Le monde du jeu : toutes les cartes (ymap) et tous les types d'objets (ytyp) de GTA V,
//! avec leurs groupes (carte de base, North Yankton, mises à jour, DLC dont Cayo Perico).
//! Une version d'un fichier ymap est remplacée par la même, plus récente, d'un groupe activé
//! (ordre de dlclist.xml), comme le fait le jeu.

use super::crypto::{joaat, Keys};
use super::heightmap::Heightmap;
use super::meta::{self, GArchetype};
use super::rpf::{Kind, Vfs};
use crate::math::{v3, Quat, V3};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupKind {
    Base,
    NorthYankton,
    Update,
    Dlc,
}

pub struct Group {
    pub key: String,
    pub label: String,
    pub kind: GroupKind,
    pub enabled: bool,
    pub ymaps: usize,
    pub archetypes: usize,
}

#[derive(Clone, Copy)]
pub struct WEnt {
    pub pos: V3,
    pub rot: Quat,
    pub sxy: f32,
    pub sz: f32,
    pub arch: u32,
    pub lod_dist: f32,
    pub child_lod_dist: f32,
    pub lod_level: u8,
    pub nchild: u16,
    pub mlo: bool,
    pub ymap: u32,
    /// index du parent (niveau de détail plus grossier) dans le ymap parent, -1 sinon
    pub parent: i32,
}

pub struct WYmap {
    pub name: String,
    pub file: usize,
    pub group: usize,
    pub prio: u32,
    pub first: usize,
    pub count: usize,
    pub min: V3,
    pub max: V3,
    pub visible: bool,
    /// copié dans le projet pour être modifié : la version du jeu est cachée
    pub in_project: bool,
    /// hachage du nom du ymap parent (niveaux de détail), 0 sinon
    pub parent: u32,
}

#[derive(Clone)]
pub struct Arch {
    pub name: u32,
    /// drapeaux du type (CBaseArchetypeDef.flags)
    pub flags: u32,
    pub bb_min: V3,
    pub bb_max: V3,
    pub radius: f32,
    pub lod_dist: f32,
    pub drawable_dict: u32,
    pub texture_dict: u32,
    pub asset_type: u32,
    pub mlo: bool,
    pub group: usize,
    pub prio: u32,
    /// fichier .ytyp qui le définit (index dans vfs.files)
    pub ytyp_file: usize,
}

pub struct GameWorld {
    pub game_dir: PathBuf,
    pub vfs: Arc<Vfs>,
    pub keys: Arc<Keys>,
    pub groups: Vec<Group>,
    pub ymaps: Vec<WYmap>,
    pub ents: Vec<WEnt>,
    pub archetypes: HashMap<u32, Arch>,
    pub names: HashMap<u32, String>,
    /// modèles (ydr / ydd / yft) par hachage de nom
    pub models: Arc<HashMap<u32, Vec<usize>>>,
    /// dictionnaires de textures (ytd) par hachage de nom
    pub txds: Arc<HashMap<u32, Vec<usize>>>,
    /// type d'objet → (dictionnaire de textures, dictionnaire de modèles)
    pub arch_dicts: Arc<HashMap<u32, (u32, u32)>>,
    /// dictionnaire de textures → dictionnaire parent (gtxd)
    pub txd_parents: Arc<HashMap<u32, u32>>,
    /// liste des props posables, triée par nom
    pub catalog: Vec<u32>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    /// racines de l'arbre des niveaux de détail (objets sans parent visible)
    root_grid: HashMap<(i32, i32), Vec<u32>>,
    /// enfants de chaque objet (liste compacte : début dans `kids`, puis `kids`)
    kid_start: Vec<u32>,
    kids: Vec<u32>,
    pub visible_ents: usize,
    /// relief du jeu (carte principale, puis île de Cayo Perico)
    pub heightmaps: Vec<Heightmap>,
    /// couleur d'affichage de chaque type d'objet
    pub colors: HashMap<u32, [u8; 3]>,
    /// types jamais dessinés (volumes de reflets, d'occlusion…)
    pub hidden_archs: std::collections::HashSet<u32>,
    /// intérieurs : leurs objets
    pub mlo_defs: HashMap<u32, Arc<MloDef>>,
    /// zones d'eau du jeu (water.xml) : min x, min y, max x, max y, hauteur
    pub water: Arc<Vec<[f32; 5]>>,
    /// groupes d'entités par défaut de chaque intérieur placé (index de l'objet de carte)
    pub mlo_inst_sets: HashMap<u32, Vec<u32>>,
}

/// Un objet d'un intérieur, dans le repère de l'intérieur (MloInstanceData::UpdateEntity 0x0600323d).
#[derive(Clone, Copy)]
pub struct MloChild {
    pub arch: u32,
    pub pos: V3,
    /// orientation (le fichier stocke l'inverse, comme pour les objets de carte)
    pub rot: Quat,
    pub sxy: f32,
    pub sz: f32,
    pub lod_dist: f32,
    /// groupe d'entités (-1 = toujours présent)
    pub set: i32,
    /// drapeaux de l'objet (CEntityDef.flags)
    pub flags: u32,
}

pub struct MloDef {
    pub children: Vec<MloChild>,
    pub sets: Vec<u32>,
}

/// Côté d'une case de la grille de recherche (mètres).
pub const GRID_CELL: f32 = 256.0;

pub struct Progress {
    pub text: String,
    pub frac: f32,
}

pub type ProgressHandle = Arc<Mutex<Progress>>;

fn set(p: &ProgressHandle, text: &str, frac: f32) {
    if let Ok(mut g) = p.lock() {
        g.text = text.to_string();
        g.frac = frac;
    }
}

fn file_stem(name: &str) -> &str {
    name.rsplit_once('.').map(|(a, _)| a).unwrap_or(name)
}

pub fn dlc_label(key: &str) -> String {
    let known: &[(&str, &str)] = &[
        ("mpheist4", "Cayo Perico (The Cayo Perico Heist)"),
        ("mpbeach", "Beach Bum"),
        ("mpbusiness", "Business"),
        ("mpbusiness2", "High Life"),
        ("mphipster", "I'm Not a Hipster"),
        ("mpindependence", "Independence Day"),
        ("mpheist", "Braquages (Heists)"),
        ("mpapartment", "Executives and Other Criminals"),
        ("mpexecutive", "Finance and Felony"),
        ("mpbiker", "Bikers"),
        ("mpimportexport", "Import/Export"),
        ("mpgunrunning", "Gunrunning"),
        ("mpsmuggler", "Smuggler's Run"),
        ("mpchristmas2017", "The Doomsday Heist"),
        ("mpassault", "Arena / Assault"),
        ("mpbattle", "After Hours (boîtes de nuit)"),
        ("mpchristmas2018", "Arena War"),
        ("mpvinewood", "Diamond Casino & Resort"),
        ("mpheist3", "Diamond Casino Heist"),
        ("mpsum", "Los Santos Summer Special"),
        ("mptuner", "Los Santos Tuners"),
        ("mpsecurity", "The Contract"),
        ("mpsum2", "The Criminal Enterprises"),
        ("mpchristmas3", "Los Santos Drug Wars"),
        ("mpstunt", "Cunning Stunts"),
        ("mplowrider", "Lowriders"),
        ("mplowrider2", "Lowriders: Custom Classics"),
        ("mpluxe", "Ill-Gotten Gains 1"),
        ("mpluxe2", "Ill-Gotten Gains 2"),
        ("mpjanuary2016", "January 2016"),
        ("mpvalentines2", "Be My Valentine 2"),
        ("mphalloween", "Halloween Surprise"),
        ("mpspecialraces", "Courses spéciales"),
        ("mpairraces", "Courses aériennes"),
    ];
    if let Some((_, l)) = known.iter().find(|(k, _)| *k == key) {
        return format!("{l} ({key})");
    }
    if key.starts_with("patchday") || key.starts_with("patch") {
        return format!("Mise à jour {key}");
    }
    key.to_string()
}

fn lerp_prog(a: f32, b: f32, i: usize, n: usize) -> f32 {
    a + (b - a) * (i as f32 / n.max(1) as f32)
}

impl GameWorld {
    /// Charge tout le monde du jeu. Peut prendre de quelques secondes à une minute.
    pub fn load(game_dir: &Path, codewalker_dir: &str, prog: ProgressHandle) -> Result<GameWorld, String> {
        set(&prog, "Lecture des clés du jeu…", 0.01);
        let p2 = prog.clone();
        let keys = super::keys::load_keys(game_dir, codewalker_dir, &move |s| set(&p2, &s, 0.02))?;

        // 1. archives
        set(&prog, "Ouverture des archives…", 0.05);
        let mut vfs = Vfs::new();
        let mut disk_rpfs: Vec<(PathBuf, String)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(game_dir) {
            let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("rpf")).unwrap_or(false)).collect();
            v.sort();
            for p in v {
                let n = p.file_name().unwrap().to_string_lossy().to_string();
                disk_rpfs.push((p, n));
            }
        }
        for n in ["update.rpf", "update2.rpf"] {
            let p = game_dir.join("update").join(n);
            if p.is_file() {
                disk_rpfs.push((p, format!("update\\{n}")));
            }
        }
        let base_archives = disk_rpfs.len();
        for (p, l) in &disk_rpfs {
            vfs.open_disk_archive(p, l, &keys);
        }
        if vfs.archives.is_empty() {
            return Err("Aucune archive du jeu n'a pu être ouverte.".into());
        }
        // ordre des DLC (dlclist.xml)
        let mut dlc_order: Vec<String> = Vec::new();
        if let Some(f) = vfs.files.iter().filter(|f| f.name == "dlclist.xml").last().cloned() {
            if let Some(d) = vfs.read_binary(&f, &keys) {
                let s = String::from_utf8_lossy(&d).to_lowercase();
                for item in s.split("<item>").skip(1) {
                    let it = item.split("</item>").next().unwrap_or("").trim().trim_end_matches('/');
                    if let Some(k) = it.rsplit('/').next() {
                        if !k.is_empty() && !dlc_order.iter().any(|x| x == k) {
                            dlc_order.push(k.to_string());
                        }
                    }
                }
            }
        }
        let mut dlc_dirs: HashMap<String, PathBuf> = HashMap::new();
        for root in [game_dir.join("x64").join("dlcpacks"), game_dir.join("update").join("x64").join("dlcpacks")] {
            if let Ok(rd) = std::fs::read_dir(&root) {
                for e in rd.flatten() {
                    let k = e.file_name().to_string_lossy().to_lowercase();
                    if e.path().join("dlc.rpf").is_file() {
                        dlc_dirs.insert(k.clone(), e.path());
                        if !dlc_order.contains(&k) {
                            dlc_order.push(k);
                        }
                    }
                }
            }
        }
        let n_dlc = dlc_order.len();
        let mut dlc_first_file: Vec<(usize, usize)> = Vec::new(); // (index DLC, premier fichier)
        for (i, k) in dlc_order.iter().enumerate() {
            set(&prog, &format!("Ouverture du DLC {k}…"), lerp_prog(0.05, 0.2, i, n_dlc));
            if let Some(dir) = dlc_dirs.get(k) {
                dlc_first_file.push((i, vfs.files.len()));
                // un pack peut être découpé en dlc.rpf, dlc1.rpf, dlc2.rpf…
                let mut parts: Vec<PathBuf> = std::fs::read_dir(dir)
                    .map(|rd| {
                        rd.flatten()
                            .map(|e| e.path())
                            .filter(|p| {
                                let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                                n.starts_with("dlc") && n.ends_with(".rpf")
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                parts.sort();
                for p in parts {
                    let n = p.file_name().unwrap().to_string_lossy().to_lowercase();
                    vfs.open_disk_archive(&p, &format!("dlcpacks\\{k}\\{n}"), &keys);
                }
            }
        }
        dlc_first_file.push((usize::MAX, vfs.files.len()));

        // 2. groupes
        let mut groups = vec![
            Group { key: "base".into(), label: "Carte de base (Los Santos et Blaine County)".into(), kind: GroupKind::Base, enabled: true, ymaps: 0, archetypes: 0 },
            Group { key: "north_yankton".into(), label: "North Yankton (prologue)".into(), kind: GroupKind::NorthYankton, enabled: false, ymaps: 0, archetypes: 0 },
        ];
        let mut dlc_group: HashMap<String, usize> = HashMap::new();
        for k in &dlc_order {
            let kind = if k.starts_with("patch") { GroupKind::Update } else { GroupKind::Dlc };
            dlc_group.insert(k.clone(), groups.len());
            // les packs « _g9ec » sont pour la version Enhanced du jeu : désactivés par défaut
            groups.push(Group { key: k.clone(), label: dlc_label(k), kind, enabled: !k.contains("g9ec"), ymaps: 0, archetypes: 0 });
        }
        let base_files_end = dlc_first_file.first().map(|x| x.1).unwrap_or(vfs.files.len());
        let disk_of_update: Vec<bool> = vfs.disk_paths.iter().map(|p| p.to_string_lossy().to_lowercase().contains("\\update\\")).collect();
        // groupe et priorité de chaque fichier
        let classify = |fi: usize, path: &str, disk: usize| -> (usize, u32) {
            if fi < base_files_end {
                if let Some(rest) = path.strip_prefix("update\\update.rpf\\dlc_patch\\") {
                    let k = rest.split('\\').next().unwrap_or("");
                    if let Some(&g) = dlc_group.get(k) {
                        return (g, 10 + 2 * (g as u32) + 1);
                    }
                }
                let prio = if disk_of_update.get(disk).copied().unwrap_or(false) { if path.starts_with("update\\update2") { 2 } else { 1 } } else { 0 };
                return (0, prio);
            }
            let k = path.strip_prefix("dlcpacks\\").and_then(|r| r.split('\\').next()).unwrap_or("");
            let g = dlc_group.get(k).copied().unwrap_or(0);
            (g, 10 + 2 * g as u32)
        };
        let _ = base_archives;

        // 3. noms connus (noms de fichiers) et modèles
        set(&prog, "Index des noms…", 0.22);
        let mut names: HashMap<u32, String> = HashMap::new();
        let mut models: HashMap<u32, Vec<usize>> = HashMap::new();
        let mut txds: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, f) in vfs.files.iter().enumerate() {
            let stem = file_stem(&f.name);
            let h = joaat(stem);
            names.entry(h).or_insert_with(|| stem.to_string());
            if f.name.ends_with(".ydr") || f.name.ends_with(".ydd") || f.name.ends_with(".yft") {
                models.entry(h).or_default().push(i);
            }
            if f.name.ends_with(".ytd") {
                txds.entry(h).or_default().push(i);
            }
        }
        // listes de noms du jeu (.nametable : noms séparés par un octet nul), lues par CodeWalker en
        // construisant son index de noms (« Building jenkindex... », GameFileCache, CodeWalker.Core)
        let tables: Vec<Vec<u8>> = vfs.files.par_iter().filter(|f| f.name.ends_with(".nametable")).filter_map(|f| vfs.read_binary(f, &keys)).collect();
        for t in &tables {
            for part in t.split(|b| *b == 0) {
                if let Ok(s) = std::str::from_utf8(part) {
                    let l = s.trim();
                    if !l.is_empty() && l.len() < 128 {
                        names.entry(joaat(l)).or_insert_with(|| l.to_lowercase());
                    }
                }
            }
        }
        // noms supplémentaires de CodeWalker (strings.txt), s'il est là
        if let Some(dll) = super::keys::find_codewalker_dll(codewalker_dir) {
            if let Some(dir) = dll.parent() {
                if let Ok(s) = std::fs::read_to_string(dir.join("strings.txt")) {
                    for line in s.lines() {
                        let l = line.trim();
                        if !l.is_empty() && l.len() < 128 {
                            names.entry(joaat(l)).or_insert_with(|| l.to_lowercase());
                        }
                    }
                }
            }
        }

        // 4. types d'objets (ytyp)
        let ytyp_files: Vec<usize> = (0..vfs.files.len()).filter(|&i| vfs.files[i].name.ends_with(".ytyp") && matches!(vfs.files[i].kind, Kind::Resource { .. })).collect();
        set(&prog, &format!("Lecture de {} fichiers de types d'objets…", ytyp_files.len()), 0.25);
        let parsed_types: Vec<(usize, Vec<GArchetype>)> = ytyp_files
            .par_iter()
            .map(|&i| {
                let f = &vfs.files[i];
                let v = vfs.read_resource(f, &keys).map(|r| meta::parse_ytyp(&r)).unwrap_or_default();
                (i, v)
            })
            .collect();
        let mut archetypes: HashMap<u32, Arch> = HashMap::new();
        let mut mlo_defs: HashMap<u32, (u32, Arc<MloDef>)> = HashMap::new();
        for (fi, list) in parsed_types {
            let f = &vfs.files[fi];
            let disk = vfs.archives[f.archive as usize].disk;
            let (g, prio) = classify(fi, &f.path, disk);
            for a in list {
                if a.mlo && !a.mlo_children.is_empty() && mlo_defs.get(&a.name).map(|d| d.0 <= prio).unwrap_or(true) {
                    let children = a
                        .mlo_children
                        .iter()
                        .map(|(e, set)| {
                            let q = Quat { x: e.rot[0], y: e.rot[1], z: e.rot[2], w: e.rot[3] };
                            MloChild {
                                arch: e.arch,
                                pos: v3(e.pos[0], e.pos[1], e.pos[2]),
                                rot: if q.is_identity() { q } else { q.inverse().normalized() },
                                sxy: e.scale_xy,
                                sz: e.scale_z,
                                lod_dist: e.lod_dist,
                                set: *set,
                                flags: e.flags,
                            }
                        })
                        .collect();
                    mlo_defs.insert(a.name, (prio, Arc::new(MloDef { children, sets: a.mlo_sets.clone() })));
                }
                let arch = Arch {
                    flags: a.flags,
                    name: a.name,
                    bb_min: v3(a.bb_min[0], a.bb_min[1], a.bb_min[2]),
                    bb_max: v3(a.bb_max[0], a.bb_max[1], a.bb_max[2]),
                    radius: a.bs_radius,
                    lod_dist: a.lod_dist,
                    drawable_dict: a.drawable_dict,
                    texture_dict: a.texture_dict,
                    asset_type: a.asset_type,
                    mlo: a.mlo,
                    group: g,
                    prio,
                    ytyp_file: fi,
                };
                match archetypes.get(&a.name) {
                    Some(old) if old.prio > prio => {}
                    _ => {
                        archetypes.insert(a.name, arch);
                    }
                }
            }
        }
        for a in archetypes.values() {
            groups[a.group].archetypes += 1;
        }

        // 5. cartes (ymap)
        let ymap_files: Vec<usize> = (0..vfs.files.len()).filter(|&i| vfs.files[i].name.ends_with(".ymap") && matches!(vfs.files[i].kind, Kind::Resource { .. })).collect();
        let total = ymap_files.len();
        set(&prog, &format!("Lecture de {total} cartes…"), 0.45);
        let done = std::sync::atomic::AtomicUsize::new(0);
        let parsed: Vec<(usize, meta::GYmap)> = ymap_files
            .par_iter()
            .filter_map(|&i| {
                let f = &vfs.files[i];
                let r = vfs.read_resource(f, &keys)?;
                let y = meta::parse_ymap(&r)?;
                let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if d % 200 == 0 {
                    set(&prog, &format!("Lecture des cartes : {d} / {total}"), lerp_prog(0.45, 0.95, d, total));
                }
                Some((i, y))
            })
            .collect();
        let mut ymaps: Vec<WYmap> = Vec::with_capacity(parsed.len());
        let mut ents: Vec<WEnt> = Vec::new();
        let mut mlo_inst_sets: HashMap<u32, Vec<u32>> = HashMap::new();
        for (fi, y) in parsed {
            if y.entities.is_empty() {
                continue;
            }
            let f = &vfs.files[fi];
            let disk = vfs.archives[f.archive as usize].disk;
            let (mut g, prio) = classify(fi, &f.path, disk);
            let stem = file_stem(&f.name).to_string();
            if g == 0 && (stem.starts_with("prologue") || stem.starts_with("plg_")) {
                g = 1;
            }
            let yi = ymaps.len() as u32;
            let first = ents.len();
            for (li, sets) in &y.mlo_sets {
                mlo_inst_sets.insert((first + li) as u32, sets.clone());
            }
            let mut mn = v3(f32::MAX, f32::MAX, f32::MAX);
            let mut mx = v3(f32::MIN, f32::MIN, f32::MIN);
            for e in &y.entities {
                let pos = v3(e.pos[0], e.pos[1], e.pos[2]);
                let q = Quat { x: e.rot[0], y: e.rot[1], z: e.rot[2], w: e.rot[3] };
                // le fichier stocke l'inverse de l'orientation (YmapEntityDef)
                let rot = if q.is_identity() { q } else { q.inverse().normalized() };
                mn = mn.min(pos);
                mx = mx.max(pos);
                ents.push(WEnt {
                    pos,
                    rot,
                    sxy: e.scale_xy,
                    sz: e.scale_z,
                    arch: e.arch,
                    lod_dist: e.lod_dist,
                    child_lod_dist: e.child_lod_dist,
                    lod_level: e.lod_level,
                    nchild: e.num_children,
                    mlo: e.mlo,
                    ymap: yi,
                    parent: e.parent,
                });
            }
            groups[g].ymaps += 1;
            ymaps.push(WYmap { name: stem, file: fi, group: g, prio, first, count: ents.len() - first, min: mn, max: mx, visible: false, in_project: false, parent: y.parent });
        }

        // 6. catalogue : objets posables (pas les intérieurs ni les morceaux de carte en LOD)
        let mut catalog: Vec<u32> = archetypes
            .values()
            .filter(|a| !a.mlo && models.contains_key(&a.name) || (!a.mlo && a.drawable_dict != 0))
            .map(|a| a.name)
            .collect();
        catalog.sort_by(|a, b| {
            let na = names.get(a).map(|s| s.as_str()).unwrap_or("~");
            let nb = names.get(b).map(|s| s.as_str()).unwrap_or("~");
            na.cmp(nb)
        });

        // 7. relief : la version la plus récente de chaque fichier heightmap*.dat
        let mut hm_files: HashMap<String, usize> = HashMap::new();
        for (i, f) in vfs.files.iter().enumerate() {
            if f.name.starts_with("heightmap") && f.name.ends_with(".dat") {
                hm_files.insert(f.name.clone(), i);
            }
        }
        let mut hm_names: Vec<&String> = hm_files.keys().collect();
        hm_names.sort();
        let mut heightmaps = Vec::new();
        for n in hm_names {
            let f = &vfs.files[hm_files[n]];
            if let Some(h) = vfs.read_binary(f, &keys).and_then(|d| Heightmap::parse(&d)) {
                heightmaps.push(h);
            }
        }
        // zones d'eau (water.xml, et celles des DLC comme l'île de Cayo Perico)
        let mut water = Vec::new();
        let mut water_files: HashMap<String, usize> = HashMap::new();
        for (i, f) in vfs.files.iter().enumerate() {
            if f.name.starts_with("water") && f.name.ends_with(".xml") && f.path.contains("levels") {
                water_files.insert(f.name.clone(), i);
            }
        }
        for (_, fi) in water_files {
            if let Some(d) = vfs.read_binary(&vfs.files[fi], &keys) {
                water.extend(parse_water(&String::from_utf8_lossy(&d)));
            }
        }
        let colors: HashMap<u32, [u8; 3]> = archetypes.keys().map(|h| (*h, arch_color(names.get(h).map(|s| s.as_str()), *h))).collect();
        let hidden_archs: std::collections::HashSet<u32> = archetypes
            .keys()
            .copied()
            .filter(|h| names.get(h).map(|n| n.contains("proxy") || n.contains("_occl") || n.ends_with("_refl")).unwrap_or(false))
            .collect();

        let arch_dicts: HashMap<u32, (u32, u32)> = archetypes.values().map(|a| (a.name, (a.texture_dict, a.drawable_dict))).collect();
        // parents des dictionnaires de textures : gtxd.ide (texte), gtxd.ymt (RBF), gtxd.meta des DLC (XML)
        let mut txd_parents: HashMap<u32, u32> = HashMap::new();
        for f in vfs.files.iter().filter(|f| f.name == "gtxd.ide" || f.name == "gtxd.ymt" || f.name == "gtxd.meta") {
            let Some(d) = vfs.read_binary(f, &keys) else { continue };
            for (parent, child) in parse_gtxd(&f.name, &d) {
                txd_parents.insert(joaat(&child), joaat(&parent));
            }
        }
        let mut w = GameWorld {
            game_dir: game_dir.to_path_buf(),
            vfs: Arc::new(vfs),
            keys: Arc::new(keys),
            txds: Arc::new(txds),
            arch_dicts: Arc::new(arch_dicts),
            txd_parents: Arc::new(txd_parents),
            groups,
            ymaps,
            ents,
            archetypes,
            names,
            models: Arc::new(models),
            catalog,
            grid: HashMap::new(),
            root_grid: HashMap::new(),
            kid_start: Vec::new(),
            kids: Vec::new(),
            visible_ents: 0,
            heightmaps,
            colors,
            hidden_archs,
            mlo_defs: mlo_defs.into_iter().map(|(k, v)| (k, v.1)).collect(),
            mlo_inst_sets,
            water: Arc::new(water),
        };
        set(&prog, "Préparation de l'affichage…", 0.97);
        w.refresh_visibility();
        set(&prog, "Terminé", 1.0);
        Ok(w)
    }

    /// Choisit la version visible de chaque ymap et reconstruit la grille.
    pub fn refresh_visibility(&mut self) {
        let mut best: HashMap<&str, usize> = HashMap::new();
        for (i, y) in self.ymaps.iter().enumerate() {
            if !self.groups[y.group].enabled {
                continue;
            }
            match best.get(y.name.as_str()) {
                Some(&j) if self.ymaps[j].prio > y.prio => {}
                _ => {
                    best.insert(y.name.as_str(), i);
                }
            }
        }
        // les versions « hei_… » des DLC remplacent la carte du même nom (ex. hei_ch1_lod remplace ch1_lod)
        let replaced: Vec<String> = best.keys().filter_map(|n| n.strip_prefix("hei_").map(|s| s.to_string())).collect();
        for r in replaced {
            best.remove(r.as_str());
        }
        let chosen: std::collections::HashSet<usize> = best.values().copied().collect();
        for (i, y) in self.ymaps.iter_mut().enumerate() {
            y.visible = chosen.contains(&i) && !y.in_project;
        }
        self.grid.clear();
        let mut n = 0;
        for y in self.ymaps.iter().filter(|y| y.visible) {
            for ei in y.first..y.first + y.count {
                let p = self.ents[ei].pos;
                let key = ((p.x / GRID_CELL).floor() as i32, (p.y / GRID_CELL).floor() as i32);
                self.grid.entry(key).or_default().push(ei as u32);
                n += 1;
            }
        }
        self.visible_ents = n;

        // arbre des niveaux de détail : chaque objet visible rattaché à son parent du ymap parent
        let by_name: HashMap<u32, usize> = self.ymaps.iter().enumerate().filter(|(_, y)| y.visible).map(|(i, y)| (super::crypto::joaat(&y.name), i)).collect();
        let n_ents = self.ents.len();
        let mut parent_of = vec![u32::MAX; n_ents];
        let mut count = vec![0u32; n_ents + 1];
        for y in self.ymaps.iter().filter(|y| y.visible) {
            let py = if y.parent != 0 { by_name.get(&y.parent).map(|&i| &self.ymaps[i]) } else { None };
            for ei in y.first..y.first + y.count {
                let pi = self.ents[ei].parent;
                if let Some(py) = py {
                    if pi >= 0 && (pi as usize) < py.count {
                        let p = py.first + pi as usize;
                        parent_of[ei] = p as u32;
                        count[p] += 1;
                    }
                }
            }
        }
        let mut start = vec![0u32; n_ents + 1];
        for i in 0..n_ents {
            start[i + 1] = start[i] + count[i];
        }
        let mut fill = start.clone();
        let mut kids = vec![0u32; start[n_ents] as usize];
        for (ei, &p) in parent_of.iter().enumerate() {
            if p != u32::MAX {
                kids[fill[p as usize] as usize] = ei as u32;
                fill[p as usize] += 1;
            }
        }
        self.kid_start = start;
        self.kids = kids;
        self.root_grid.clear();
        for y in self.ymaps.iter().filter(|y| y.visible) {
            for ei in y.first..y.first + y.count {
                if parent_of[ei] == u32::MAX {
                    let p = self.ents[ei].pos;
                    let key = ((p.x / GRID_CELL).floor() as i32, (p.y / GRID_CELL).floor() as i32);
                    self.root_grid.entry(key).or_default().push(ei as u32);
                }
            }
        }
    }

    /// Parent (version lointaine) d'un objet : l'objet `parentIndex` de la carte nommée par `CMapData.parent`.
    pub fn lod_parent(&self, ei: usize) -> Option<usize> {
        let e = &self.ents[ei];
        if e.parent < 0 {
            return None;
        }
        let y = &self.ymaps[e.ymap as usize];
        if y.parent == 0 {
            return None;
        }
        let py = self
            .ymaps
            .iter()
            .filter(|q| super::crypto::joaat(&q.name) == y.parent)
            .max_by_key(|q| (q.visible as u8, q.prio))?;
        let p = py.first + e.parent as usize;
        (p < py.first + py.count).then_some(p)
    }

    fn children(&self, ei: usize) -> &[u32] {
        if ei + 1 >= self.kid_start.len() {
            return &[];
        }
        &self.kids[self.kid_start[ei] as usize..self.kid_start[ei + 1] as usize]
    }

    /// Règle des niveaux de détail de CodeWalker (RenderLodManager::GetEntityChildren 0x060009a8,
    /// RecurseAddVisibleLeaves 0x060009a7) : un objet est remplacé par ses enfants quand ils sont tous
    /// présents et que (s'il a un parent) la caméra est à moins de sa distance enfants, ou (s'il est
    /// une racine) qu'au moins un enfant est à moins de sa propre distance. Une racine n'est montrée
    /// qu'à moins de sa distance de détail.
    fn visit_lod(&self, ei: u32, eye: V3, scale: f32, out: &mut Vec<u32>, depth: u32) {
        let e = &self.ents[ei as usize];
        let d = (e.pos - eye).len();
        let lod_of = |x: &WEnt| if x.lod_dist > 0.0 { x.lod_dist } else { 200.0 };
        if depth == 0 && d > lod_of(e) * scale {
            return;
        }
        let kids = self.children(ei as usize);
        if !kids.is_empty() && kids.len() >= e.nchild as usize && depth < 8 {
            let go_down = if depth > 0 {
                d <= e.child_lod_dist * scale
            } else {
                kids.iter().any(|&k| {
                    let c = &self.ents[k as usize];
                    (c.pos - eye).len() <= lod_of(c) * scale
                })
            };
            if go_down {
                for &k in kids {
                    self.visit_lod(k, eye, scale, out, depth + 1);
                }
                return;
            }
        }
        out.push(ei);
    }

    pub fn name_of(&self, h: u32) -> String {
        self.names.get(&h).cloned().unwrap_or_else(|| format!("hash_{h:08X}"))
    }

    /// Boîte locale d'un type d'objet (sinon un petit cube).
    pub fn arch_box(&self, h: u32) -> (V3, V3) {
        match self.archetypes.get(&h) {
            Some(a) if a.bb_max.x >= a.bb_min.x => (a.bb_min, a.bb_max),
            _ => (v3(-0.5, -0.5, 0.0), v3(0.5, 0.5, 1.0)),
        }
    }

    /// Objets à afficher depuis `eye` : règle des niveaux de détail (lodDist / childLodDist).
    pub fn visible_near(&self, eye: V3, center: V3, radius: f32, lod_scale: f32) -> Vec<u32> {
        let mut out = Vec::new();
        let c0 = ((center.x - radius) / GRID_CELL).floor() as i32;
        let c1 = ((center.x + radius) / GRID_CELL).floor() as i32;
        let r0 = ((center.y - radius) / GRID_CELL).floor() as i32;
        let r1 = ((center.y + radius) / GRID_CELL).floor() as i32;
        for cx in c0..=c1 {
            for cy in r0..=r1 {
                let Some(list) = self.root_grid.get(&(cx, cy)) else { continue };
                for &ei in list {
                    self.visit_lod(ei, eye, lod_scale, &mut out, 0);
                }
            }
        }
        out
    }

    /// Objets dont la position est dans un cercle (au sol).
    pub fn ents_in_radius(&self, c: V3, r: f32) -> Vec<u32> {
        let mut out = Vec::new();
        let c0 = ((c.x - r) / GRID_CELL).floor() as i32;
        let c1 = ((c.x + r) / GRID_CELL).floor() as i32;
        let r0 = ((c.y - r) / GRID_CELL).floor() as i32;
        let r1 = ((c.y + r) / GRID_CELL).floor() as i32;
        for cx in c0..=c1 {
            for cy in r0..=r1 {
                if let Some(list) = self.grid.get(&(cx, cy)) {
                    for &ei in list {
                        if self.ents[ei as usize].pos.dist2d(c) <= r {
                            out.push(ei);
                        }
                    }
                }
            }
        }
        out
    }

    /// Les 8 coins d'un objet du jeu.
    pub fn corners(&self, ei: u32) -> [V3; 8] {
        let e = &self.ents[ei as usize];
        let (mn, mx) = self.arch_box(e.arch);
        let s = v3(e.sxy, e.sxy, e.sz);
        let (mn, mx) = (mn.mulv(s), mx.mulv(s));
        let mut out = [V3::ZERO; 8];
        for (i, c) in out.iter_mut().enumerate() {
            let p = v3(if i & 1 == 0 { mn.x } else { mx.x }, if i & 2 == 0 { mn.y } else { mx.y }, if i & 4 == 0 { mn.z } else { mx.z });
            *c = e.pos + e.rot.rotate(p);
        }
        out
    }

    /// Fichier ymap source (pour le copier dans le projet).
    pub fn ymap_resource(&self, yi: usize) -> Option<super::rpf::Resource> {
        let f = &self.vfs.files[self.ymaps[yi].file];
        self.vfs.read_resource(f, &self.keys)
    }
}

/// Couleur d'un type d'objet d'après son nom (sol, route, eau, végétation, bâtiment).
pub fn arch_color(name: Option<&str>, h: u32) -> [u8; 3] {
    let v = (h % 23) as i32 - 11;
    let tint = |c: [i32; 3]| [(c[0] + v).clamp(0, 255) as u8, (c[1] + v).clamp(0, 255) as u8, (c[2] + v).clamp(0, 255) as u8];
    const FACADES: [[i32; 3]; 7] = [[176, 170, 160], [196, 180, 150], [160, 112, 92], [206, 196, 178], [128, 140, 152], [184, 160, 128], [150, 150, 146]];
    let Some(n) = name else { return tint(FACADES[(h >> 8) as usize % FACADES.len()]) };
    let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
    if has(&["water", "_sea", "river", "lake", "_wat"]) {
        tint([60, 110, 160])
    } else if has(&["road", "_rd", "street", "hway", "freeway", "_jn", "junc", "tunnel", "bridge"]) {
        tint([88, 90, 96])
    } else if has(&["tree", "bush", "hedge", "veg", "plant", "grass", "palm", "foliage", "leaf"]) {
        tint([74, 122, 64])
    } else if has(&["ground", "_land", "terrain", "hill", "_grnd", "dirt", "sand", "beach", "cliff", "rock", "decal", "_mnt", "_glue", "slope"]) {
        tint([132, 124, 96])
    } else if n.starts_with("prop_") || n.starts_with("p_") || n.starts_with("v_") || n.starts_with("hei_") || n.starts_with("h4_prop") {
        tint([196, 150, 90])
    } else if has(&["_lod", "slod"]) {
        tint([138, 150, 168])
    } else {
        // bâtiments : quelques teintes de façades (béton, grès, brique, crépi, verre)
        tint(FACADES[(h >> 8) as usize % FACADES.len()])
    }
}

impl GameWorld {
    /// Hauteur du sol (bas de la case) d'après le relief du jeu.
    pub fn ground_min(&self, x: f32, y: f32) -> Option<f32> {
        for hm in self.heightmaps.iter().rev() {
            if hm.contains(x, y) {
                if let Some(z) = hm.height_min(x, y) {
                    return Some(z);
                }
            }
        }
        None
    }

    /// Point du sol touché par un rayon (pas de 4 m puis affinage).
    pub fn ray_ground(&self, o: V3, d: V3) -> Option<V3> {
        if self.heightmaps.is_empty() {
            return None;
        }
        let mut prev = o;
        let mut t = 0.0;
        let step = 4.0;
        while t < 8000.0 {
            t += step * (1.0 + t / 400.0);
            let p = o + d * t;
            if let Some(g) = self.ground_min(p.x, p.y) {
                if p.z <= g {
                    // affinage par dichotomie
                    let (mut a, mut b) = (prev, p);
                    for _ in 0..12 {
                        let m = (a + b) * 0.5;
                        match self.ground_min(m.x, m.y) {
                            Some(gz) if m.z <= gz => b = m,
                            _ => a = m,
                        }
                    }
                    let gz = self.ground_min(b.x, b.y).unwrap_or(b.z);
                    return Some(v3(b.x, b.y, gz));
                }
            } else if p.z <= 0.0 {
                return Some(v3(p.x, p.y, 0.0));
            }
            prev = p;
        }
        None
    }

    pub fn color_of(&self, h: u32) -> [u8; 3] {
        self.colors.get(&h).copied().unwrap_or_else(|| arch_color(None, h))
    }
}

impl GameWorld {
    /// Modèle 3D d'un type d'objet (ydr / yft du même nom, sinon son dictionnaire ydd).
    pub fn load_model(&self, arch: u32, max_tris: usize) -> Option<super::drawable::Mesh> {
        use super::drawable;
        if let Some(list) = self.models.get(&arch) {
            for &fi in list.iter().rev() {
                let f = &self.vfs.files[fi];
                let Some(r) = self.vfs.read_resource(f, &self.keys) else { continue };
                let m = if f.name.ends_with(".ydr") {
                    drawable::read_ydr(&r, max_tris)
                } else if f.name.ends_with(".yft") {
                    drawable::read_yft(&r, max_tris)
                } else {
                    drawable::read_ydd(&r, arch, max_tris)
                };
                if m.is_some() {
                    return m;
                }
            }
        }
        let dict = self.archetypes.get(&arch).map(|a| a.drawable_dict).unwrap_or(0);
        if dict != 0 {
            if let Some(list) = self.models.get(&dict) {
                for &fi in list.iter().rev() {
                    let f = &self.vfs.files[fi];
                    if !f.name.ends_with(".ydd") {
                        continue;
                    }
                    if let Some(r) = self.vfs.read_resource(f, &self.keys) {
                        if let Some(m) = drawable::read_ydd(&r, arch, max_tris) {
                            return Some(m);
                        }
                    }
                }
            }
        }
        None
    }
}

impl GameWorld {
    /// Accès partagé aux fichiers du jeu pour les fils de chargement des modèles.
    pub fn model_source(&self) -> super::loader::ModelSource {
        super::loader::ModelSource {
            vfs: self.vfs.clone(),
            keys: self.keys.clone(),
            models: self.models.clone(),
            txds: self.txds.clone(),
            arch: self.arch_dicts.clone(),
            txd_parents: self.txd_parents.clone(),
        }
    }
}

/// Paires (parent, enfant) des dictionnaires de textures.
pub fn parse_gtxd(name: &str, d: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if name.ends_with(".ymt") {
        // RBF : chaque texte = FD FF, longueur (u32, zéro final compris), octets ; parent puis enfant
        let mut strs = Vec::new();
        let mut i = 0;
        while i + 6 < d.len() {
            if d[i] == 0xFD && d[i + 1] == 0xFF {
                let len = u32::from_le_bytes([d[i + 2], d[i + 3], d[i + 4], d[i + 5]]) as usize;
                if (2..256).contains(&len) && i + 6 + len <= d.len() && d[i + 6 + len - 1] == 0 {
                    strs.push(String::from_utf8_lossy(&d[i + 6..i + 6 + len - 1]).to_lowercase());
                    i += 6 + len;
                    continue;
                }
            }
            i += 1;
        }
        for p in strs.chunks_exact(2) {
            out.push((p[0].clone(), p[1].clone()));
        }
    } else if name.ends_with(".meta") {
        let s = String::from_utf8_lossy(d).to_lowercase();
        for item in s.split("<item>").skip(1) {
            let get = |tag: &str| item.split(&format!("<{tag}>")).nth(1).and_then(|r| r.split(&format!("</{tag}>")).next()).map(|v| v.trim().to_string());
            if let (Some(p), Some(c)) = (get("parent"), get("child")) {
                out.push((p, c));
            }
        }
    } else {
        // gtxd.ide : section « txdp », lignes « enfant, parent »
        let s = String::from_utf8_lossy(d).to_lowercase();
        let mut on = false;
        for line in s.lines() {
            let l = line.trim();
            if l == "txdp" {
                on = true;
            } else if l == "end" {
                on = false;
            } else if on {
                let parts: Vec<&str> = l.split(',').map(|x| x.trim()).collect();
                if parts.len() == 2 {
                    out.push((parts[1].to_string(), parts[0].to_string()));
                }
            }
        }
    }
    out
}

/// Rectangles d'eau visibles d'un water.xml : WaterQuads/Item (minX, maxX, minY, maxY, z, IsInvisible).
pub fn parse_water(s: &str) -> Vec<[f32; 5]> {
    let mut out = Vec::new();
    let Some(start) = s.find("<WaterQuads") else { return out };
    let end = s[start..].find("</WaterQuads>").map(|e| start + e).unwrap_or(s.len());
    let val = |item: &str, tag: &str| -> Option<String> {
        let i = item.find(&format!("<{tag} "))?;
        let rest = &item[i..];
        let v = rest.find("value=\"")? + 7;
        let e = rest[v..].find('"')?;
        Some(rest[v..v + e].to_string())
    };
    for item in s[start..end].split("<Item>").skip(1) {
        if val(item, "IsInvisible").map(|v| v == "true").unwrap_or(false) {
            continue;
        }
        let f = |t: &str| val(item, t).and_then(|v| v.parse::<f32>().ok());
        if let (Some(x0), Some(x1), Some(y0), Some(y1)) = (f("minX"), f("maxX"), f("minY"), f("maxY")) {
            out.push([x0, y0, x1, y1, f("z").unwrap_or(0.0)]);
        }
    }
    out
}
