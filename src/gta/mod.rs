//! Lecture des fichiers de GTA V installés sur l'ordinateur (lecture seule).

pub mod crypto;
pub mod drawable;
pub mod heightmap;
pub mod keys;
pub mod loader;
pub mod meta;
pub mod rpf;
pub mod world;

use std::path::Path;

/// Mode test : `dragowalker --gta-probe <dossier du jeu> [dossier CodeWalker]`.
pub fn probe(game: &str, cw: &str) {
    if std::env::var("DW_WORLD").is_ok() {
        let t0 = std::time::Instant::now();
        let prog = std::sync::Arc::new(std::sync::Mutex::new(world::Progress { text: String::new(), frac: 0.0 }));
        match world::GameWorld::load(Path::new(game), cw, prog) {
            Ok(w) => {
                println!("monde chargé en {:.1} s : {} archives, {} fichiers, {} ymaps, {} objets, {} types, {} noms, {} modèles, catalogue {}, visibles {}", t0.elapsed().as_secs_f32(), w.vfs.archives.len(), w.vfs.files.len(), w.ymaps.len(), w.ents.len(), w.archetypes.len(), w.names.len(), w.models.len(), w.catalog.len(), w.visible_ents);
                for g in &w.groups { if g.ymaps > 0 || g.archetypes > 0 { println!("  {:<45} ymaps {:>5} types {:>6}", g.label, g.ymaps, g.archetypes); } }
                let near = w.visible_near(crate::math::v3(200.0, -900.0, 150.0), crate::math::v3(200.0, -900.0, 30.0), 1500.0, 1.0);
                println!("visibles près de Legion Square : {}", near.len());
                for &i in near.iter().take(5) { let e = &w.ents[i as usize]; println!("   {} {:?} lod {} box {:?}", w.name_of(e.arch), e.pos, e.lod_level, w.arch_box(e.arch)); }
                for h in w.catalog.iter().take(5) { println!("   cat {}", w.name_of(*h)); }
                for k in ["mpheist4", "mpheist"] { let pre = format!("dlcpacks\\{k}\\"); let n = w.vfs.files.iter().filter(|f| f.path.starts_with(&pre)).count(); let ny = w.vfs.files.iter().filter(|f| f.path.starts_with(&pre) && f.name.ends_with(".ymap")).count(); println!("fichiers {k} : {n} dont ymap {ny}"); }
                println!("ordre : {:?}", w.groups.iter().map(|g| g.key.as_str()).collect::<Vec<_>>());
                for f in w.vfs.files.iter().filter(|f| f.name.starts_with("heightmap") && f.name.ends_with(".dat")) {
                    let d = w.vfs.read_binary(f, &w.keys);
                    match d.as_ref().and_then(|d| heightmap::Heightmap::parse(d)) {
                        Some(hm) => {
                            println!("{} : {}x{} {:?}..{:?}", f.path, hm.w, hm.h, hm.min, hm.max);
                            for (n, x, y) in [("Legion Square", 195.0, -934.0), ("Chiliad", 501.0, 5593.0), ("ocean", -3000.0, 0.0), ("Sandy", 1900.0, 3700.0), ("Cayo", 4890.0, -5170.0), ("NY", 3360.0, -4849.0)] {
                                println!("   {n:<14} {:?}", hm.height(x, y));
                            }
                        }
                        None => println!("{} : illisible ({} octets) {:02x?}", f.path, d.as_ref().map(|d| d.len()).unwrap_or(0), d.as_ref().map(|d| d[..64.min(d.len())].to_vec())),
                    }
                }
                for n in ["prop_bench_01a", "prop_streetlight_01", "prop_tree_pine_01", "prop_bin_01a", "prop_dumpster_01a", "prop_barrier_work05"] {
                    let h = crypto::joaat(n);
                    let t = std::time::Instant::now();
                    match w.load_model(h, 20000) {
                        Some(m) => println!("modèle {n} : {} sommets {} triangles, boîte {:?}..{:?} (type {:?}) {:.0} ms", m.pos.len(), m.tris.len(), m.min, m.max, w.arch_box(h), t.elapsed().as_millis()),
                        None => println!("modèle {n} : introuvable (fichiers {:?})", w.models.get(&h).map(|v| v.iter().map(|&i| w.vfs.files[i].path.clone()).collect::<Vec<_>>())),
                    }
                }
                for (lbl, x, y, ez) in [("legion", 195.0f32, -934.0f32, 600.0f32), ("cayo", 4890.0, -5170.0, 2000.0), ("carte", 0.0, 0.0, 6000.0)] {
                    let mut v: Vec<(f32, u32)> = w.visible_near(crate::math::v3(x, y, ez), crate::math::v3(x, y, 0.0), 3000.0, 1.0).into_iter().map(|i| { let (a, b) = w.arch_box(w.ents[i as usize].arch); ((b - a).len(), i) }).collect();
                    v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
                    println!("{lbl} : {} visibles", v.len());
                    for (d, i) in v.iter().take(12) { let e = &w.ents[*i as usize]; let (a, b) = w.arch_box(e.arch); let s = b - a; println!("   {:>7.0} {:<40} lod {} taille {:.0}x{:.0}x{:.0} ymap {}", d, w.name_of(e.arch), e.lod_level, s.x, s.y, s.z, w.ymaps[e.ymap as usize].name); }
                    let mut by = [0usize; 8]; for (_, i) in &v { by[w.ents[*i as usize].lod_level.min(7) as usize] += 1; } println!("   par niveau {:?}", by);
                }
                // DW_MODEL="nom1,nom2" : détail d'un modèle du jeu (parties, triangles, découpe, fichiers)
                if let Ok(list) = std::env::var("DW_MODEL") {
                    let src = w.model_source();
                    let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
                    let mut cache = Vec::new();
                    for n in list.split(',') {
                        let h = crypto::joaat(n.trim());
                        println!("modèle {n} : fichiers {:?}, type {:?}", w.models.get(&h).map(|v| v.iter().map(|&i| w.vfs.files[i].path.clone()).collect::<Vec<_>>()), w.archetypes.get(&h).map(|a| (w.name_of(a.drawable_dict), w.name_of(a.texture_dict), a.asset_type)));
                        let l = src.load(h, &claimed, &mut cache);
                        match &l.model {
                            Some(m) => {
                                println!("   {} sommets, {} triangles, {} parties", m.verts.len(), m.idx.len() / 3, m.geoms.len());
                                for (gi, g) in m.geoms.iter().enumerate() {
                                    println!("   partie {gi} : {} triangles, texture {} (clé {:x}), découpe {}", g.count / 3, w.name_of(g.tex), l.part_keys.get(gi).copied().unwrap_or(0), g.cutout);
                                }
                            }
                            None => println!("   illisible"),
                        }
                    }
                }
                {
                    let src = w.model_source();
                    let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
                    let mut cache = Vec::new();
                    let near = w.visible_near(crate::math::v3(195.0, -934.0, 200.0), crate::math::v3(195.0, -934.0, 30.0), 800.0, 1.0);
                    let mut archs: Vec<u32> = near.iter().map(|&i| w.ents[i as usize].arch).collect();
                    archs.sort();
                    archs.dedup();
                    let t = std::time::Instant::now();
                    let (mut ok, mut parts, mut textured, mut tris, mut texs, mut fmts) = (0, 0, 0, 0, 0, std::collections::HashMap::new());
                    for &a in archs.iter().take(300) {
                        let l = src.load(a, &claimed, &mut cache);
                        if let Some(m) = &l.model {
                            ok += 1;
                            parts += l.part_keys.len();
                            textured += l.part_keys.iter().filter(|k| **k != 0).count();
                            tris += m.idx.len() / 3;
                        }
                        texs += l.textures.len();
                        for (_, t) in &l.textures { *fmts.entry(format!("{:?}", t.fmt)).or_insert(0) += 1; }
                    }
                    println!("chargement : {} types, {} modèles, {} parties dont {} texturées, {} triangles, {} textures {:?} en {} ms", archs.len().min(300), ok, parts, textured, tris, texs, fmts, t.elapsed().as_millis());
                }
                {
                    let mut by: std::collections::HashMap<&str, Vec<usize>> = std::collections::HashMap::new();
                    for (i, y) in w.ymaps.iter().enumerate() { by.entry(y.name.as_str()).or_default().push(i); }
                    let mut shown = 0;
                    for (n, ids) in &by {
                        let Some(base) = n.strip_prefix("hei_") else { continue };
                        let Some(bids) = by.get(base) else { continue };
                        let y = &w.ymaps[ids[0]]; let b = &w.ymaps[bids[0]];
                        if shown < 12 && (y.min.x - 195.0).abs() < 800.0 && (y.min.y + 934.0).abs() < 800.0 {
                            let names = |yy: &world::WYmap| { let mut v: Vec<String> = (yy.first..yy.first + yy.count.min(4)).map(|i| w.name_of(w.ents[i].arch)).collect(); v.sort(); v };
                            println!("hei {n}: {} objets ({}) / base {base}: {} objets ({}) {:?} | {:?}", y.count, w.groups[y.group].key, b.count, w.groups[b.group].key, names(y), names(b));
                            shown += 1;
                        }
                    }
                }
                {
                    use crate::math::v3;
                    let eye = v3(195.0 - 190.0, -934.0 - 190.0, 30.0 + 272.0);
                    let vis: std::collections::HashSet<u32> = w.visible_near(eye, v3(195.0, -934.0, 30.0), 1500.0, 1.0).into_iter().collect();
                    let all = w.ents_in_radius(v3(195.0, -934.0, 0.0), 700.0);
                    let mut uncovered = 0;
                    let mut reported = 0;
                    for gx in -6..=6 {
                        for gy in -6..=6 {
                            let p = v3(195.0 + gx as f32 * 40.0, -934.0 + gy as f32 * 40.0, 0.0);
                            let covers = |ei: u32| { let e = &w.ents[ei as usize]; let (a, b) = w.arch_box(e.arch); let lo = e.pos + a.min(b); let hi = e.pos + a.max(b); let r = (b - a).len() * 0.5; (e.pos.x - p.x).abs() < r && (e.pos.y - p.y).abs() < r && (b.x - a.x) > 15.0 && (b.y - a.y) > 15.0 && hi.z - lo.z < 400.0 };
                            let hit = vis.iter().any(|&ei| covers(ei));
                            if !hit {
                                uncovered += 1;
                                if reported < 5 {
                                    reported += 1;
                                    println!("trou en {:?}", (p.x, p.y));
                                    for &ei in all.iter().filter(|&&ei| covers(ei)).take(6) {
                                        let e = &w.ents[ei as usize];
                                        let d = (e.pos - eye).len();
                                        println!("    {:<34} lod {} lodDist {:.0} childLod {:.0} enfants {} dist {:.0} ymap {} visible {}", w.name_of(e.arch), e.lod_level, e.lod_dist, e.child_lod_dist, e.nchild, d, w.ymaps[e.ymap as usize].name, w.ymaps[e.ymap as usize].visible);
                                    }
                                }
                            }
                        }
                    }
                    println!("points sans sol : {uncovered} / 169");
                }
                {
                    use crate::math::v3;
                    let src = w.model_source();
                    let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
                    let mut cache = Vec::new();
                    let mut archs: Vec<u32> = w.ents_in_radius(v3(195.0, -934.0, 0.0), 500.0).into_iter().map(|i| w.ents[i as usize].arch).filter(|a| { let (x, y) = w.arch_box(*a); (y.x - x.x) > 30.0 && (y.y - x.y) > 30.0 }).collect();
                    archs.sort(); archs.dedup();
                    for &a in archs.iter().take(25) {
                        let l = src.load(a, &claimed, &mut cache);
                        let pv = w.load_model(a, 200000);
                        println!("sol {:<30} rendu: {} tris {} parties | aperçu: {} tris", w.name_of(a), l.model.as_ref().map(|m| m.idx.len() / 3).unwrap_or(0), l.model.as_ref().map(|m| m.geoms.len()).unwrap_or(0), pv.as_ref().map(|m| m.tris.len()).unwrap_or(0));
                    }
                }
                {
                    let g: Vec<&rpf::FileEntry> = w.vfs.files.iter().filter(|f| f.name.contains("gtxd")).collect();
                    println!("gtxd : {} fichiers, {} parents", g.len(), w.txd_parents.len());
                    for f in g.iter().take(6) {
                        let d = w.vfs.read_binary(f, &w.keys);
                        println!("  {} {:?} {:02x?}", f.path, d.as_ref().map(|d| d.len()), d.as_ref().map(|d| d[..d.len().min(220)].to_vec()));
                    }
                }
                {
                    let n_mlo = w.ents.iter().filter(|e| e.mlo).count();
                    let kids: usize = w.mlo_defs.values().map(|d| d.children.len()).sum();
                    println!("intérieurs : {} types, {} objets dedans, {} placés", w.mlo_defs.len(), kids, n_mlo);
                    let near: Vec<usize> = (0..w.ents.len()).filter(|&i| w.ents[i].mlo && w.ents[i].pos.dist2d(crate::math::v3(-662.0, -935.0, 0.0)) < 60.0).collect();
                    for i in near.iter().take(3) {
                        let e = &w.ents[*i];
                        if let Some(d) = w.mlo_defs.get(&e.arch) {
                            let n = d.children.len().max(1) as f32;
                            let c = d.children.iter().fold(crate::math::v3(0.0, 0.0, 0.0), |a, k| a + e.pos + e.rot.rotate(k.pos)) * (1.0 / n);
                            println!("   centre des objets de {} : {:?}", w.name_of(e.arch), c);
                        }
                    }
                    for i in near.iter().take(3) { let e = &w.ents[*i]; println!("   {} à {:?} : {} objets, sets {:?}", w.name_of(e.arch), e.pos, w.mlo_defs.get(&e.arch).map(|d| d.children.len()).unwrap_or(0), w.mlo_inst_sets.get(&(*i as u32))); }
                }
                {
                    let mut shown = 0;
                    for (ei, sets) in w.mlo_inst_sets.iter() {
                        let e = &w.ents[*ei as usize];
                        let Some(d) = w.mlo_defs.get(&e.arch) else { continue };
                        if sets.is_empty() || d.sets.is_empty() || !w.ymaps[e.ymap as usize].visible || e.pos.dist2d(crate::math::v3(0.0, 0.0, 0.0)) > 3000.0 {
                            continue;
                        }
                        let in_sets = d.children.iter().filter(|c| c.set >= 0 && d.sets.get(c.set as usize).map(|h| sets.contains(h) || sets.contains(&(c.set as u32))).unwrap_or(false)).count();
                        let center = d.children.iter().fold(crate::math::v3(0.0, 0.0, 0.0), |a, k| a + e.pos + e.rot.rotate(k.pos)) * (1.0 / d.children.len().max(1) as f32);
                        let by_name: Vec<bool> = sets.iter().map(|h| d.sets.contains(h)).collect();
                        println!("groupes {} à {:?} centre {:?} : {} groupes {:?}, défaut {:?} (noms trouvés {:?}), {} objets activés par les groupes", w.name_of(e.arch), e.pos, center, d.sets.len(), d.sets.iter().map(|h| w.name_of(*h)).collect::<Vec<_>>(), sets.iter().map(|h| w.name_of(*h)).collect::<Vec<_>>(), by_name, in_sets);
                        shown += 1;
                        if shown >= 6 { break; }
                    }
                }
                println!("erreurs : {}", w.vfs.errors.len()); for e in w.vfs.errors.iter().take(5) { println!("  ! {e}"); }
            }
            Err(e) => println!("ERREUR {e}"),
        }
        return;
    }
    let t0 = std::time::Instant::now();
    let keys = match keys::load_keys(Path::new(game), cw, &|s| println!("  {s}")) {
        Ok(k) => k,
        Err(e) => {
            println!("ERREUR clés : {e}");
            return;
        }
    };
    println!("clés OK ({:.1} s)", t0.elapsed().as_secs_f32());
    let mut vfs = rpf::Vfs::new();
    for n in ["x64a.rpf", "update\\update.rpf"] {
        vfs.open_disk_archive(&Path::new(game).join(n), n, &keys);
    }
    println!("archives {} fichiers {} erreurs {} ({:.1} s)", vfs.archives.len(), vfs.files.len(), vfs.errors.len(), t0.elapsed().as_secs_f32());
    for e in vfs.errors.iter().take(5) {
        println!("  ! {e}");
    }
    let ymaps = vfs.files.iter().filter(|f| f.name.ends_with(".ymap")).count();
    let ytyps = vfs.files.iter().filter(|f| f.name.ends_with(".ytyp")).count();
    println!("ymap {ymaps} ytyp {ytyps}");
    if let Some(f) = vfs.files.iter().find(|f| f.name == "dlclist.xml") {
        match vfs.read_binary(f, &keys) {
            Some(d) => println!("{} :\n{}", f.path, String::from_utf8_lossy(&d[..d.len().min(400)])),
            None => println!("{} illisible", f.path),
        }
    }
    println!("hachages : CMapData {} CEntityDef {} CMloInstanceDef {} CMapTypes {}", meta::struct_hash("CMapData"), meta::struct_hash("CEntityDef"), meta::struct_hash("CMloInstanceDef"), meta::struct_hash("CMapTypes"));
    for f in vfs.files.iter().filter(|f| f.name.ends_with(".ymap") && !f.path.contains("lodlights")).take(3) {
        let Some(r) = vfs.read_resource(f, &keys) else { println!("{} illisible", f.path); continue };
        let m = meta::Meta::parse(&r);
        println!("{} : blocs {:?}", f.path, m.as_ref().map(|m| m.blocks.iter().map(|b| (b.0, b.2)).collect::<Vec<_>>()));
        if let Some(y) = meta::parse_ymap(&r) {
            println!("  name {:08x} entités {} ext {:?}..{:?}", y.name, y.entities.len(), y.streaming_min, y.streaming_max);
            for e in y.entities.iter().take(3) { println!("   {:?}", e); }
        }
    }
    for f in vfs.files.iter().filter(|f| f.name.ends_with(".ytyp")).take(2) {
        let Some(r) = vfs.read_resource(f, &keys) else { continue };
        let a = meta::parse_ytyp(&r);
        println!("{} : {} archétypes", f.path, a.len());
        for x in a.iter().take(3) { println!("   {:?}", x); }
    }
}

/// Mode test : `dragowalker --read-ymap <fichier.ymap>` relit un ymap binaire (RSC7) posé sur le disque.
pub fn read_ymap_file(path: &str) {
    let Ok(d) = std::fs::read(path) else { println!("illisible"); return };
    if d.len() < 16 || &d[0..4] != b"RSC7" { println!("pas un fichier RSC7"); return };
    let sys = u32::from_le_bytes(d[8..12].try_into().unwrap());
    let gfx = u32::from_le_bytes(d[12..16].try_into().unwrap());
    let Some(data) = crypto::inflate(&d[16..]) else { println!("décompression impossible"); return };
    let r = rpf::Resource::new(data, sys, gfx);
    match meta::parse_ymap(&r) {
        Some(y) => println!("{} : {} objets, parent {:08x}, flags {} contentFlags {}", path, y.entities.len(), y.parent, y.flags, y.content_flags),
        None => println!("{} : ymap illisible", path),
    }
}
