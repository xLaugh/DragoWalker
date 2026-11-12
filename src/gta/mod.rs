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
                // DW_YBN=1 : écrit le plus petit fichier de collisions (.ybn) parmi les 300 premiers, pour voir son format
                if std::env::var("DW_YBN").is_ok() {
                    let mut best: Option<(usize, Vec<u8>, String)> = None;
                    for f in w.vfs.files.iter().filter(|f| f.name.ends_with(".ybn")).take(300) {
                        if let Some(raw) = w.vfs.raw_resource_file(f) {
                            if raw.len() > 2000 && best.as_ref().map(|b| raw.len() < b.0).unwrap_or(true) {
                                best = Some((raw.len(), raw, f.name.clone()));
                            }
                        }
                    }
                    if let Some((n, raw, name)) = best {
                        let p = std::env::temp_dir().join("dragowalker_cw").join(&name);
                        let _ = std::fs::create_dir_all(p.parent().unwrap());
                        let _ = std::fs::write(&p, raw);
                        println!("ybn : {} ({n} octets) -> {}", name, p.display());
                    }
                }
                // DW_ARCH_GREP="prefixe1,prefixe2" : types d'objets du jeu dont le nom commence ainsi, taille et
                // nombre de placements sur la carte (pour choisir les arbres / rochers les plus utilisés par le jeu)
                if let Ok(list) = std::env::var("DW_ARCH_GREP") {
                    let pre: Vec<String> = list.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
                    let mut uses: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
                    for e in &w.ents {
                        *uses.entry(e.arch).or_insert(0) += 1;
                    }
                    let mut v: Vec<(String, usize, crate::math::V3)> = Vec::new();
                    for h in w.archetypes.keys() {
                        let n = w.name_of(*h).to_lowercase();
                        // « *mot » : le nom contient ce mot (sinon : commence par)
                        if pre.iter().any(|p| match p.strip_prefix('*') { Some(w) => n.contains(w), None => n.starts_with(p.as_str()) }) {
                            let (a, b) = w.arch_box(*h);
                            v.push((n, uses.get(h).copied().unwrap_or(0), b - a));
                        }
                    }
                    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                    println!("types trouvés : {}", v.len());
                    for (n, u, s) in v.iter().take(400) {
                        println!("   {n:<36} placé {u:>6} fois  taille {:.1} x {:.1} x {:.1}", s.x, s.y, s.z);
                    }
                }
                // DW_TRAFFIC_STATS=1 : feux du jeu regroupés par carrefour ; pour chaque feu, le côté où il se tient
                // (avant / après le centre pour les voitures qu'il regarde) et le sens de son bras
                // DW_MLOROT=1 : placements d'intérieur du jeu tournés (cap hors multiple de 90°) et le grand bâtiment le
                // plus proche (à moins de 20 m) : écart de cap modulo 90° selon que la rotation du placement se lit
                // telle quelle (lecture de CodeWalker) ou inversée (comme un objet ordinaire)
                if std::env::var("DW_MLOROT").is_ok() {
                    let m90 = |d: f32| { let r = d.rem_euclid(90.0); r.min(90.0 - r) };
                    let (mut n, mut ok_as_is, mut ok_inv) = (0usize, 0usize, 0usize);
                    let mut ex = Vec::new();
                    for (mi, m) in w.ents.iter().enumerate().filter(|(_, e)| e.mlo && e.lod_level == 0) {
                        let ym = m.rot.heading_deg();
                        if m90(ym) < 3.0 {
                            continue;
                        }
                        let best = w.ents.iter().filter(|b| !b.mlo && b.lod_level == 0 && (b.pos - m.pos).len() < 20.0).filter(|b| { let n = w.name_of(b.arch); !crate::buildings::is_prop_name(&n) && !n.contains("tree") && !n.contains("bush") && !n.contains("decal") }).filter(|b| { let (a, c) = w.arch_box(b.arch); let s = c - a; s.x.max(s.y) > 8.0 && s.z > 4.0 }).min_by(|a, b| (a.pos - m.pos).len().partial_cmp(&(b.pos - m.pos).len()).unwrap_or(std::cmp::Ordering::Equal));
                        let Some(b) = best else { continue };
                        let yb = b.rot.heading_deg();
                        let d_as_is = m90(ym - yb);
                        let d_inv = m90(-ym - yb);
                        n += 1;
                        ok_as_is += (d_as_is < 3.0) as usize;
                        ok_inv += (d_inv < 3.0) as usize;
                        if ex.len() < 8 {
                            ex.push(format!("{} cap {ym:.1}° / {} cap {yb:.1}° : écart tel quel {d_as_is:.1}°, inversé {d_inv:.1}°", w.name_of(m.arch), w.name_of(b.arch)));
                        }
                        let _ = mi;
                    }
                    println!("intérieurs tournés comparés : {n} ; alignés lu tel quel : {ok_as_is} ; alignés lu inversé : {ok_inv}");
                    for s in ex {
                        println!("   {s}");
                    }
                }
                // DW_LODFLAGS=1 : objets qui ont un parent (version de loin) — bit 0x8 des drapeaux (CodeWalker
                // YmapEntityDef::get_LodInParentYmap 0x06002d03) selon que le parent est dans la carte parente
                // (CMapData.parent) ou dans la même carte ; 400 premières cartes HD de la carte de base
                if std::env::var("DW_LODFLAGS").is_ok() {
                    let (mut pp, mut pp8, mut same, mut same8, mut orphan8, mut orphan) = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
                    let mut ex: Vec<String> = Vec::new();
                    for yi in (0..w.ymaps.len()).filter(|&i| w.ymaps[i].parent != 0).take(400) {
                        let Some(r) = w.ymap_resource(yi) else { continue };
                        let Some(y) = meta::parse_ymap(&r) else { continue };
                        let n = y.entities.len();
                        for (i, e) in y.entities.iter().enumerate() {
                            let bit = e.flags & 8 != 0;
                            if e.parent < 0 {
                                orphan += 1;
                                orphan8 += bit as usize;
                                continue;
                            }
                            // même règle que YmapFile::EnsureEntities 0x06002cb0 pour « parent dans la même carte »
                            let in_same = (e.parent as usize) < n && y.entities[e.parent as usize].lod_level > e.lod_level && (y.entities[e.parent as usize].lod_level != 5 || e.lod_level == 5);
                            if in_same {
                                same += 1;
                                same8 += bit as usize;
                            } else {
                                pp += 1;
                                pp8 += bit as usize;
                                if ex.len() < 4 {
                                    ex.push(format!("{} #{i} {} lod {} parent {} flags {:#x}", w.ymaps[yi].name, w.name_of(e.arch), e.lod_level, e.parent, e.flags));
                                }
                            }
                        }
                    }
                    println!("parent dans la carte parente : {pp} objets, bit 0x8 sur {pp8}");
                    println!("parent dans la même carte : {same} objets, bit 0x8 sur {same8}");
                    println!("sans parent : {orphan} objets, bit 0x8 sur {orphan8}");
                    for s in ex {
                        println!("   ex. {s}");
                    }
                }
                if std::env::var("DW_TRAFFIC_STATS").is_ok() {
                    use crate::math::v3;
                    let names = ["prop_traffic_01a", "prop_traffic_01b", "prop_traffic_01d", "prop_traffic_03a", "prop_sign_road_01a"];
                    for nm in names {
                        let h = crypto::joaat(nm);
                        let list: Vec<&world::WEnt> = w.ents.iter().filter(|e| e.arch == h).collect();
                        let lights: Vec<&world::WEnt> = w.ents.iter().filter(|e| [crypto::joaat("prop_traffic_01a"), crypto::joaat("prop_traffic_01b"), crypto::joaat("prop_traffic_01d")].contains(&e.arch)).collect();
                        let (mut near, mut far, mut arm_in, mut arm_out, mut groups) = (0, 0, 0, 0, 0);
                        let mut dists = Vec::new();
                        let mut lat = Vec::new();
                        for e in &list {
                            // carrefour : feux à moins de 35 m (centre = moyenne)
                            let g: Vec<&&world::WEnt> = lights.iter().filter(|o| o.pos.dist2d(e.pos) < 35.0).collect();
                            if g.len() < 3 {
                                continue;
                            }
                            groups += 1;
                            let c = g.iter().fold(v3(0.0, 0.0, 0.0), |a, o| a + o.pos) * (1.0 / g.len() as f32);
                            let f = e.rot.rotate(v3(0.0, -1.0, 0.0));
                            let arm = e.rot.rotate(v3(-1.0, 0.0, 0.0));
                            let rel = e.pos - c;
                            let along = rel.x * f.x + rel.y * f.y;
                            let side = rel.x * arm.x + rel.y * arm.y;
                            if along > 0.0 { near += 1 } else { far += 1 }
                            if side < 0.0 { arm_in += 1 } else { arm_out += 1 }
                            dists.push(along.abs());
                            lat.push(side.abs());
                        }
                        dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        let med = |v: &Vec<f32>| v.get(v.len() / 2).copied().unwrap_or(0.0);
                        println!("{nm} : {} placés, {groups} dans un carrefour ; du côté des voitures regardées (avant le centre) {near}, après le centre {far} ; bras vers le centre {arm_in}, vers l'extérieur {arm_out} ; distance médiane au centre le long {:.1} m, en travers {:.1} m", list.len(), med(&dists), med(&lat));
                    }
                }
                // DW_TXD_GREP="mot1,mot2" : textures des dictionnaires du jeu (.ytd) dont le nom contient un de ces
                // mots (nom de la texture : pointeur à 0x28 de TextureBase, comme TextureBase::Read de CodeWalker)
                if let Ok(list) = std::env::var("DW_TXD_GREP") {
                    let words: Vec<String> = list.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
                    let only = std::env::var("DW_TXD_FILE").unwrap_or_default().to_lowercase();
                    let mut shown = 0;
                    for f in w.vfs.files.iter().filter(|f| f.name.ends_with(".ytd") && f.name.contains(only.as_str())) {
                        let Some(r) = w.vfs.read_resource(f, &w.keys) else { continue };
                        let Some(texs) = r.ptr(r.u64(0x30)) else { continue };
                        let n = r.u16(0x28) as usize;
                        let mut hits = Vec::new();
                        for i in 0..n.min(4096) {
                            let Some(t) = r.ptr(r.u64(texs + i * 8)) else { continue };
                            let Some(np) = r.ptr(r.u64(t + 0x28)) else { continue };
                            let mut e = np;
                            while e < r.data.len() && r.data[e] != 0 && e - np < 128 {
                                e += 1;
                            }
                            let name = String::from_utf8_lossy(&r.data[np..e]).to_lowercase();
                            if words.iter().any(|wd| name.contains(wd.as_str())) {
                                hits.push(format!("{name} {}x{}", r.u16(t + 0x50), r.u16(t + 0x52)));
                            }
                        }
                        if !hits.is_empty() && shown < 600 {
                            println!("   {} : {}", f.path, hits.join(", "));
                            shown += 1;
                        }
                    }
                    println!("dictionnaires affichés : {shown}");
                }
                // DW_TEXT_DUMP="nom" : écrit les fichiers non-ressource (xml, meta, dat…) de ce nom, de toutes les archives,
                // dans le dossier temporaire dragowalker_text (ex. gameconfig.xml : tailles des pools du jeu)
                if let Ok(name) = std::env::var("DW_TEXT_DUMP") {
                    let dir = std::env::temp_dir().join("dragowalker_text");
                    let _ = std::fs::create_dir_all(&dir);
                    for (i, f) in w.vfs.files.iter().filter(|f| f.name.eq_ignore_ascii_case(&name)).enumerate() {
                        match w.vfs.read_binary(f, &w.keys) {
                            Some(d) => {
                                let out = dir.join(format!("{i}_{}", f.name));
                                let _ = std::fs::write(&out, &d);
                                println!("   {} ({} octets) -> {}", f.path, d.len(), out.display());
                            }
                            None => println!("   {} illisible", f.path),
                        }
                    }
                }
                // DW_BLOCK_DUMP="structure,extension,nombre" : écrit les fichiers qui contiennent ce type de bloc
                if let Ok(v) = std::env::var("DW_BLOCK_DUMP") {
                    let p: Vec<&str> = v.split(',').collect();
                    let first = p.first().copied().unwrap_or("1860713439");
                    let want = first.parse::<u32>().unwrap_or_else(|_| crypto::joaat(&first.to_lowercase()));
                    let ext = p.get(1).copied().unwrap_or(".ymap");
                    let max: usize = p.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
                    let dir = std::env::temp_dir().join("dragowalker_dump2");
                    let _ = std::fs::create_dir_all(&dir);
                    let mut n = 0;
                    for f in w.vfs.files.iter().filter(|f| f.name.ends_with(ext)) {
                        if n >= max {
                            break;
                        }
                        let Some(r) = w.vfs.read_resource(f, &w.keys) else { continue };
                        let Some(m) = meta::Meta::parse(&r) else { continue };
                        if m.blocks.iter().any(|b| b.0 == want) {
                            if let Some(raw) = w.vfs.raw_resource_file(f) {
                                let _ = std::fs::write(dir.join(&f.name), raw);
                                println!("   {} ", f.path);
                                n += 1;
                            }
                        }
                    }
                    println!("fichiers avec ce bloc : {n} dans {}", dir.display());
                }
                // DW_FILE_DUMP="extension,mot[,nombre]" : écrit les fichiers du jeu correspondants (ressources brutes)
                // dans le dossier temporaire, pour les relire en XML avec CodeWalker
                if let Ok(v) = std::env::var("DW_FILE_DUMP") {
                    let p: Vec<&str> = v.split(',').collect();
                    let (ext, word) = (p.first().copied().unwrap_or(".ymap"), p.get(1).copied().unwrap_or(""));
                    let max: usize = p.get(2).and_then(|s| s.parse().ok()).unwrap_or(40);
                    let dir = std::env::temp_dir().join("dragowalker_dump");
                    let _ = std::fs::create_dir_all(&dir);
                    let mut n = 0;
                    for f in w.vfs.files.iter().filter(|f| f.name.ends_with(ext) && f.name.contains(word)) {
                        if n >= max {
                            break;
                        }
                        if let Some(raw) = w.vfs.raw_resource_file(f) {
                            let _ = std::fs::write(dir.join(&f.name), raw);
                            n += 1;
                        }
                    }
                    println!("fichiers écrits : {n} dans {}", dir.display());
                }
                // DW_YTYP_FX=1 : types d'objets du jeu qui portent un effet de particules ; écrit les .ytyp qui en
                // contiennent dans le dossier temporaire (pour les relire en XML avec CodeWalker)
                if std::env::var("DW_YTYP_FX").is_ok() {
                    let dir = std::env::temp_dir().join("dragowalker_fx");
                    let _ = std::fs::create_dir_all(&dir);
                    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                    let mut dumped = 0;
                    for f in w.vfs.files.iter().filter(|f| f.name.ends_with(".ytyp")) {
                        let Some(r) = w.vfs.read_resource(f, &w.keys) else { continue };
                        let d = &r.data;
                        let mut found = false;
                        let mut i = 0;
                        while i + 8 < d.len() {
                            if d[i..].starts_with(b"amb_") && (i == 0 || d[i - 1] == 0) {
                                let mut j = i;
                                while j < d.len() && (d[j].is_ascii_alphanumeric() || d[j] == b'_') {
                                    j += 1;
                                }
                                if j - i > 8 && j < d.len() && d[j] == 0 {
                                    *counts.entry(String::from_utf8_lossy(&d[i..j]).to_string()).or_insert(0) += 1;
                                    found = true;
                                }
                                i = j;
                            } else {
                                i += 1;
                            }
                        }
                        if found && dumped < 400 {
                            if let Some(raw) = w.vfs.raw_resource_file(f) {
                                let _ = std::fs::write(dir.join(&f.name), raw);
                                dumped += 1;
                            }
                        }
                    }
                    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
                    v.sort_by(|a, b| b.1.cmp(&a.1));
                    println!("effets trouvés : {} noms, {dumped} ytyp écrits dans {}", v.len(), dir.display());
                    for (n, c) in v.iter().take(150) {
                        println!("   {n:<40} {c}");
                    }
                }
                // DW_SCEN=1 : blocs d'un fichier de scénarios (.ymt)
                if std::env::var("DW_SCEN").is_ok() {
                    let files: Vec<&rpf::FileEntry> = w.vfs.files.iter().filter(|f| f.name.ends_with(".ymt") && f.path.to_lowercase().contains("scenario")).collect();
                    println!("scénarios : {} fichiers .ymt", files.len());
                    for f in files.iter().take(2) {
                        if let Some(r) = w.vfs.read_resource(f, &w.keys) {
                            if let Some(m) = meta::Meta::parse(&r) {
                                println!("  {} : {:?}", f.path, m.blocks.iter().map(|b| (b.0, b.2)).collect::<Vec<_>>());
                            }
                        }
                    }
                }
                // DW_LIGHTS=N : lumières rangées dans les modèles des N premiers intérieurs du jeu (réglages
                // les plus fréquents, pour les lumières des intérieurs créés dans DragoWalker)
                if let Some(nm) = std::env::var("DW_LIGHTS").ok().and_then(|v| v.parse::<usize>().ok()) {
                    let src = w.model_source();
                    let mut keys: Vec<&u32> = w.mlo_defs.keys().collect();
                    keys.sort();
                    let mut seen = std::collections::HashSet::new();
                    let mut all = Vec::new();
                    for k in keys.into_iter().take(nm) {
                        for c in &w.mlo_defs[k].children {
                            if seen.insert(c.arch) {
                                for l in src.lights(c.arch) {
                                    all.push((c.arch, l));
                                }
                            }
                        }
                    }
                    println!("lumières : {} dans {} modèles d'intérieur", all.len(), seen.len());
                    let mut hist = |label: &str, f: &dyn Fn(&drawable::GameLight) -> String| {
                        let mut m: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                        for (_, l) in &all {
                            *m.entry(f(l)).or_insert(0) += 1;
                        }
                        let mut v: Vec<_> = m.into_iter().collect();
                        v.sort_by(|a, b| b.1.cmp(&a.1));
                        println!("  {label} : {:?}", v.into_iter().take(8).collect::<Vec<_>>());
                    };
                    hist("type", &|l| l.kind.to_string());
                    hist("flags", &|l| format!("{:#x}", l.flags));
                    hist("heures", &|l| format!("{:#x}", l.time_flags));
                    hist("intensité", &|l| format!("{:.1}", l.intensity));
                    hist("portée", &|l| format!("{:.0}", l.falloff));
                    hist("exposant", &|l| format!("{:.0}", l.falloff_exp));
                    hist("couleur", &|l| format!("{:?}", l.color));
                    hist("cône", &|l| format!("{:.0}/{:.0}", l.cone_inner, l.cone_outer));
                    hist("halo", &|l| format!("{:.1}/{:.1}", l.corona_size, l.corona_intensity));
                    hist("volume", &|l| format!("{:.1}", l.volume_intensity));
                    hist("près ombre", &|l| format!("{:.2}", l.shadow_near_clip));
                    hist("fondu", &|l| l.light_fade.to_string());
                    hist("direction", &|l| format!("{:.0} {:.0} {:.0}", l.dir[0], l.dir[1], l.dir[2]));
                    hist("étendue", &|l| format!("{:.1} {:.1} {:.1}", l.extent[0], l.extent[1], l.extent[2]));
                    hist("clignote", &|l| l.flashiness.to_string());
                    // spots vers le bas allumés toute la journée : réglages pris ensemble
                    all.retain(|(_, l)| l.kind == 2 && l.dir[2] < -0.99 && l.time_flags == 0xFF_FFFF);
                    println!("  spots vers le bas, toute la journée : {}", all.len());
                    let mut hist2 = |label: &str, f: &dyn Fn(&drawable::GameLight) -> String| {
                        let mut m: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                        for (_, l) in &all {
                            *m.entry(f(l)).or_insert(0) += 1;
                        }
                        let mut v: Vec<_> = m.into_iter().collect();
                        v.sort_by(|a, b| b.1.cmp(&a.1));
                        println!("  {label} : {:?}", v.into_iter().take(8).collect::<Vec<_>>());
                    };
                    hist2("ensemble", &|l| format!("i{:.0} p{:.1} e{:.0} c{:.0}/{:.0} f{:#x} h{:.1}/{:.1} v{:.1} {:?}", l.intensity, l.falloff, l.falloff_exp, l.cone_inner, l.cone_outer, l.flags, l.corona_size, l.corona_intensity, l.volume_intensity, l.color));
                    hist2("portée", &|l| format!("{:.0}", l.falloff));
                    hist2("intensité", &|l| format!("{:.0}", l.intensity));
                    hist2("cône", &|l| format!("{:.0}/{:.0}", l.cone_inner, l.cone_outer));
                    hist2("exposant", &|l| format!("{:.0}", l.falloff_exp));
                    hist2("flags", &|l| format!("{:#x}", l.flags));
                    for (a, l) in all.iter().take(4) {
                        println!("  ex. {} : {:?}", w.name_of(*a), l);
                    }
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
                                    // centre et normale moyenne de la partie (pour savoir de quel côté regarde un feu, un panneau)
                                    let (mut c, mut nn, mut k) = ([0f32; 3], [0f32; 3], 0f32);
                                    for t in (g.first as usize)..((g.first + g.count) as usize).min(m.idx.len()) {
                                        let v = &m.verts[m.idx[t] as usize];
                                        for q in 0..3 {
                                            c[q] += v.pos[q];
                                            nn[q] += v.nrm[q];
                                        }
                                        k += 1.0;
                                    }
                                    let k = k.max(1.0);
                                    println!("   partie {gi} : {} triangles, texture {} (clé {:x}), découpe {}, centre ({:.2} {:.2} {:.2}), normale moyenne ({:.2} {:.2} {:.2})", g.count / 3, w.name_of(g.tex), l.part_keys.get(gi).copied().unwrap_or(0), g.cutout, c[0] / k, c[1] / k, c[2] / k, nn[0] / k, nn[1] / k, nn[2] / k);
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
