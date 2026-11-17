//! Outils MCP pour cibler et analyser avant d'agir : ce qui est choisi dans la vue, ce qu'il y a autour, analyse
//! d'un bâtiment du jeu et transformation en intérieur, import d'une ressource MLO, intérieurs du jeu, cartes du
//! projet, contrôle avant export, nom des pièces.

use crate::app::DwApp;
use crate::math::{v3, V3};
use serde_json::{json, Value};

fn tool(name: &str, desc: &str, props: Value, required: &[&str]) -> Value {
    json!({"name": name, "description": desc, "inputSchema": {"type": "object", "properties": props, "required": required}})
}

fn txt(s: impl Into<String>) -> Vec<Value> {
    vec![json!({"type": "text", "text": s.into()})]
}

pub const NAMES: [&str; 14] = ["export_resource", "get_selection", "look_around", "analyze_building", "building_to_interior", "import_mlo_resource", "find_game_interiors", "check_project", "maps", "rename_room", "open_here", "create_prop_from_image", "import_model_prop", "create_shape_prop"];

pub fn tools_plus() -> Vec<Value> {
    vec![
        tool(
            "export_resource",
            "Exporte le projet en ressource FiveM dans un dossier (stream/, fxmanifest.lua, collisions, portes, points d'interaction…) et rend le compte rendu et le contrôle de la ressource. Seulement si la personne le demande ou donne le dossier.",
            json!({"path": {"type": "string"}}),
            &["path"],
        ),
        tool(
            "get_selection",
            "Ce que la personne a choisi dans la vue, en détail : pour chaque objet, son id, son modèle, ce que c'est (objet posé, bâtiment du jeu, placement d'un intérieur, objet d'une pièce, bâtiment de l'atelier, MLO importé), sa position, son cap et sa taille. À appeler en premier quand on parle de « ce », « cet », « celui-là ».",
            json!({}),
            &[],
        ),
        tool(
            "look_around",
            "Ce qu'il y a autour du centre de la vue (ou d'un point) : intérieur du projet où se trouve ce point, objets du projet proches (id, modèle, distance), intérieurs du jeu proches.",
            json!({"radius": {"type": "number", "description": "40 m par défaut"}, "filter": {"type": "string", "description": "mot du nom des modèles"}, "x": {"type": "number"}, "y": {"type": "number"}}),
            &[],
        ),
        tool(
            "analyze_building",
            "Analyse un bâtiment du jeu (id, sinon celui choisi) : taille, étages (hauteurs), surfaces de vitres et de murs, pièces proposées par étage. À faire avant building_to_interior.",
            json!({"id": {"type": "integer"}}),
            &[],
        ),
        tool(
            "building_to_interior",
            "Transforme un bâtiment du jeu (id, sinon celui choisi) en intérieur MLO : pièces de chaque étage, passages et entrée, placé exactement sur le bâtiment. Ensuite : get_interior, rename_room, furnish_room, place_doors, check_project.",
            json!({"id": {"type": "integer"}}),
            &[],
        ),
        tool(
            "import_mlo_resource",
            "Importe le dossier d'une ressource MLO FiveM (avec fxmanifest.lua et stream/) donné par la personne : ses intérieurs, cartes et fichiers entrent dans le projet. Un MLO importé est recopié tel quel à l'export (ses pièces ne se modifient pas).",
            json!({"path": {"type": "string"}}),
            &["path"],
        ),
        tool(
            "find_game_interiors",
            "Intérieurs (MLO) du jeu de base : nom, nombre d'objets, position du placement le plus proche de la vue. Pour aller en voir un : set_camera sur sa position.",
            json!({"query": {"type": "string"}, "limit": {"type": "integer", "description": "20 par défaut"}}),
            &[],
        ),
        tool(
            "check_project",
            "Contrôle avant export (comme la fenêtre « Contrôle ») : erreurs et avertissements des cartes et des intérieurs (pièces, portails, objets hors de leur pièce, noms…). À faire à la fin d'un travail.",
            json!({}),
            &[],
        ),
        tool(
            "maps",
            "Cartes (ymap) du projet : nom, nombre d'objets, celle où vont les nouveaux objets. target : choisir la carte de destination ; create : créer une carte (et la choisir).",
            json!({"target": {"type": "string"}, "create": {"type": "string"}}),
            &[],
        ),
        tool(
            "rename_room",
            "Renomme une pièce d'un intérieur (nom simple, sans accents ni espaces : salon, cuisine, chambre_1, couloir…) ; le nom sert aussi à deviner le type d'aménagement.",
            json!({"interior": {"type": "string"}, "room": {"type": ["integer", "string"]}, "name": {"type": "string"}}),
            &["interior", "room", "name"],
        ),
        tool(
            "open_here",
            "Ouvre dans le Studio l'intérieur du projet choisi dans la vue (son placement ou un de ses objets), sinon celui où se trouve le centre de la vue.",
            json!({}),
            &[],
        ),
        tool(
            "create_prop_from_image",
            "Crée un prop FiveM à partir d'une image PNG / JPEG (fichier de la personne) : panneau (plaque épaisse au sol, image des deux côtés), affiche (murale, une face, origine au dos), decoupe (selon la transparence, deux faces), tapis (à plat au sol) ou cube. width en m (la hauteur suit l'image). Le prop va dans « Mes modèles » et part avec l'export ; place=true le pose (at = [x, y, z], sinon au centre de la vue). Devant du prop vers -y.",
            json!({"path": {"type": "string"}, "kind": {"type": "string", "enum": ["panneau", "affiche", "decoupe", "tapis", "cube"]}, "width": {"type": "number"}, "thickness": {"type": "number"}, "name": {"type": "string"}, "collision": {"type": "string", "enum": ["aucune", "boite", "forme"]}, "place": {"type": "boolean"}, "at": {"type": "array", "items": {"type": "number"}}}),
            &["path"],
        ),
        tool(
            "import_model_prop",
            "Crée un prop FiveM à partir d'un modèle 3D : .glb, .gltf, .obj lus directement ; .blend, .fbx, .dae, .stl, .ply convertis par Blender (installé ; 10 à 30 s). 1 unité = 1 m (scale pour changer), posé au sol par défaut (ground). Textures de couleur et transparence reprises. Va dans « Mes modèles » ; place=true le pose.",
            json!({"path": {"type": "string"}, "name": {"type": "string"}, "scale": {"type": "number"}, "ground": {"type": "boolean"}, "collision": {"type": "string", "enum": ["aucune", "boite", "forme"]}, "place": {"type": "boolean"}, "at": {"type": "array", "items": {"type": "number"}}}),
            &["path"],
        ),
        tool(
            "create_shape_prop",
            "Crée (ou recrée, même nom) un prop FiveM fait de formes simples, gardé dans le projet : shapes = liste de {kind: boite|cylindre|plaque, size: [largeur x, profondeur y, hauteur z] en m, pos: [x, y, z du bas de la forme], yaw (°), color: \"#rrggbb\", texture: nom d'une texture importée du projet}. Devant vers -y, origine au sol. place=true le pose.",
            json!({"name": {"type": "string"}, "shapes": {"type": "array", "items": {"type": "object"}}, "collision": {"type": "string", "enum": ["aucune", "boite", "forme"]}, "place": {"type": "boolean"}, "at": {"type": "array", "items": {"type": "number"}}}),
            &["name", "shapes"],
        ),
    ]
}

/// Collision demandée à un outil de prop (boîte par défaut).
fn coll_arg(a: &Value) -> u8 {
    match a["collision"].as_str().unwrap_or("boite") {
        "aucune" | "none" => 0,
        "forme" | "mesh" | "exacte" => 2,
        _ => 1,
    }
}

impl DwApp {
    /// Intérieur du projet auquel appartient un objet (placement ou objet d'une pièce).
    fn interior_of(&self, id: u64) -> Option<String> {
        let r = self.proj.find_id(id)?;
        match r {
            crate::model::ItemRef::MloEnt(t, m, _) => Some(self.proj.ytyps[t].mlos[m].name.clone()),
            crate::model::ItemRef::Ent(..) => {
                let e = self.proj.ent(r)?;
                (e.mlo.is_some() && self.proj.find_mlo(&e.archetype).is_some()).then(|| e.archetype.clone())
            }
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }

    pub fn mcp_plus(&mut self, name: &str, a: &Value) -> Result<Vec<Value>, String> {
        match name {
            "get_selection" => Ok(txt(self.mcp_selection())),
            "export_resource" => {
                let p = std::path::PathBuf::from(a["path"].as_str().unwrap_or("").trim().trim_matches('"'));
                if p.as_os_str().is_empty() {
                    return Err("Dossier manquant.".into());
                }
                let r = self.export_project(&p, true);
                let mut s = format!("{r}
");
                for (lv, m) in crate::check::scan_resource_dir(&p) {
                    s += &format!("- [{}] {m}
", match lv { crate::check::Level::Error => "ERREUR", crate::check::Level::Warning => "attention", crate::check::Level::Ok => "ok" });
                }
                Ok(txt(s))
            }
            "look_around" => Ok(txt(self.mcp_around(a))),
            "analyze_building" => {
                self.mcp_select_arg(a)?;
                self.analyse_selected();
                let Some(an) = self.game.analysis.as_ref() else { return Err(self.status.clone()) };
                let mut s = format!("{}\nTaille {:.1} × {:.1} × {:.1} m, toit à z {:.1}.\nÉtages (sol, plafond) :\n", self.status, an.size.x, an.size.y, an.size.z, an.roof_z);
                for (li, (z0, z1)) in an.levels.iter().enumerate() {
                    let prop = crate::analyse::propose_rooms(an, li);
                    let rooms: Vec<String> = prop.as_ref().map(|p| p.rooms.iter().map(|r| format!("{} ({:.1} × {:.1} m)", r.0, r.2.x - r.1.x, r.2.y - r.1.y)).collect()).unwrap_or_default();
                    s += &format!("  {} : z {:.2} à {:.2} ; pièces proposées : {}\n", li + 1, z0, z1, if rooms.is_empty() { "aucune (pas assez de sol)".into() } else { rooms.join(", ") });
                }
                Ok(txt(s))
            }
            "building_to_interior" => {
                self.mcp_select_arg(a)?;
                let before: std::collections::HashSet<String> = self.proj.ytyps.iter().flat_map(|t| t.mlos.iter().map(|m| m.name.clone())).collect();
                let msg = self.convert_to_interior();
                let new: Vec<String> = self.proj.ytyps.iter().flat_map(|t| t.mlos.iter().map(|m| m.name.clone())).filter(|n| !before.contains(n)).collect();
                if new.is_empty() {
                    return Err(msg);
                }
                Ok(txt(format!("{msg}\nNouvel intérieur : {}. Suite : get_interior, puis rename_room, furnish_room, place_doors, check_project.", new.join(", "))))
            }
            "import_mlo_resource" => {
                let p = std::path::PathBuf::from(a["path"].as_str().unwrap_or("").trim().trim_matches('"'));
                if !p.is_dir() {
                    return Err(format!("Dossier introuvable : {}", p.display()));
                }
                self.push_undo();
                let before = self.proj.ytyps.iter().map(|t| t.mlos.len()).sum::<usize>();
                let msg = self.import_mlo(&p);
                let names: Vec<String> = self.proj.ytyps.iter().flat_map(|t| t.mlos.iter().map(|m| m.name.clone())).collect();
                Ok(txt(format!("{msg}\nIntérieurs du projet : {} (avant : {before}). {}", names.join(", "), "Un MLO importé est recopié tel quel à l'export.")))
            }
            "find_game_interiors" => Ok(txt(self.mcp_game_interiors(a))),
            "check_project" => {
                let mut s = String::new();
                let mut n = (0, 0);
                for (lv, msg) in self.project_checks().into_iter().chain(self.mlo_checks()) {
                    let tag = match lv {
                        crate::check::Level::Error => {
                            n.0 += 1;
                            "ERREUR"
                        }
                        crate::check::Level::Warning => {
                            n.1 += 1;
                            "attention"
                        }
                        crate::check::Level::Ok => "ok",
                    };
                    s += &format!("- [{tag}] {msg}\n");
                }
                Ok(txt(format!("{} erreur(s), {} avertissement(s).\n{s}", n.0, n.1)))
            }
            "maps" => {
                if let Some(c) = a["create"].as_str().filter(|c| !c.trim().is_empty()) {
                    self.push_undo();
                    self.proj.ymaps.push(crate::model::Ymap::new(c.trim()));
                    self.target_ymap = self.proj.ymaps.len() - 1;
                }
                if let Some(t) = a["target"].as_str() {
                    let i = self.proj.ymaps.iter().position(|y| y.name.eq_ignore_ascii_case(t.trim())).ok_or(format!("Carte « {t} » introuvable."))?;
                    if self.proj.ymaps[i].sample {
                        return Err("La carte d'exemple ne reçoit pas d'objets.".into());
                    }
                    self.target_ymap = i;
                }
                let mut s = String::from("Cartes du projet :\n");
                for (i, y) in self.proj.ymaps.iter().enumerate() {
                    let mlos = y.entities.iter().filter(|e| e.mlo.is_some()).count();
                    s += &format!(
                        "  {} : {} objet(s){}{}{}{}\n",
                        y.name,
                        y.entities.len(),
                        if mlos > 0 { format!(", {mlos} placement(s) d'intérieur") } else { String::new() },
                        if y.from_game { ", copiée du jeu" } else { "" },
                        if y.sample { ", carte d'exemple (non exportée)" } else { "" },
                        if i == self.target_ymap { " ← les nouveaux objets vont ici" } else { "" }
                    );
                }
                Ok(txt(s))
            }
            "rename_room" => {
                let iname = a["interior"].as_str().unwrap_or("");
                let (t, m) = self.find_interior(iname)?;
                let ri = self.room_arg(t, m, &a["room"])?;
                let n: String = a["name"].as_str().unwrap_or("").trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c.to_ascii_lowercase() } else { '_' }).collect();
                if n.is_empty() {
                    return Err("Nom vide.".into());
                }
                self.push_undo();
                self.proj.ytyps[t].mlos[m].rooms[ri].name = n.clone();
                Ok(txt(format!("Pièce {ri} renommée « {n} »{}.", crate::decor::kind_from_name(&n).map(|k| format!(" (type deviné : {})", crate::decor::KINDS[k].0)).unwrap_or_default())))
            }
            "open_here" => {
                let from_sel = self.selected.iter().find_map(|id| self.interior_of(*id));
                let name = from_sel.or_else(|| {
                    let c = self.cam.target;
                    self.mlo_at(c).map(|(t, m, _, _)| self.proj.ytyps[t].mlos[m].name.clone())
                });
                let Some(n) = name else { return Err("Aucun intérieur du projet choisi ni au centre de la vue (voir get_scene, look_around).".into()) };
                self.studio_enter(&n);
                Ok(txt(format!("Studio ouvert sur « {n} ».")))
            }
            "create_prop_from_image" | "import_model_prop" | "create_shape_prop" => self.mcp_prop(name, a),
            _ => Err(format!("Outil inconnu : {name}")),
        }
    }

    /// Outils de props du Studio.
    fn mcp_prop(&mut self, tool: &str, a: &Value) -> Result<Vec<Value>, String> {
        use crate::propstudio as ps;
        let path = std::path::PathBuf::from(a["path"].as_str().unwrap_or("").trim().trim_matches('"'));
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let name = a["name"].as_str().map(|s| s.to_string()).filter(|s| !s.trim().is_empty()).unwrap_or(stem);
        let coll = coll_arg(a);
        let msg = match tool {
            "create_prop_from_image" => {
                let img = std::fs::read(&path).ok().and_then(|d| image::load_from_memory(&d).ok()).ok_or_else(|| format!("Image illisible : {}", path.display()))?.to_rgba8();
                let kind = match a["kind"].as_str().unwrap_or("panneau") {
                    "affiche" => 1,
                    "decoupe" | "découpe" => 2,
                    "tapis" => 3,
                    "cube" => 4,
                    _ => 0,
                };
                let w = a["width"].as_f64().unwrap_or(1.0) as f32;
                let t = a["thickness"].as_f64().unwrap_or(0.04) as f32;
                let m = ps::image_prop(&img, kind, w, t);
                self.studio_write_prop(&name, &m, coll)?
            }
            "import_model_prop" => {
                let mut m = ps::read_model(&path, self.sollumz.blender.as_deref())?;
                m.fit(a["scale"].as_f64().unwrap_or(1.0) as f32, a["ground"].as_bool().unwrap_or(true));
                self.studio_write_prop(&name, &m, coll)?
            }
            _ => {
                let mut shapes = Vec::new();
                for s in a["shapes"].as_array().cloned().unwrap_or_default() {
                    let arr3 = |k: &str, d: [f32; 3]| -> [f32; 3] {
                        let v: Vec<f32> = s[k].as_array().map(|x| x.iter().filter_map(|n| n.as_f64()).map(|n| n as f32).collect()).unwrap_or_default();
                        if v.len() >= 3 {
                            [v[0], v[1], v[2]]
                        } else {
                            d
                        }
                    };
                    let color = s["color"].as_str().and_then(|c| {
                        let c = c.trim_start_matches('#');
                        (c.len() == 6).then(|| [0, 2, 4].map(|i| u8::from_str_radix(&c[i..i + 2], 16).unwrap_or(128)))
                    });
                    shapes.push(ps::Shape {
                        kind: match s["kind"].as_str().unwrap_or("boite") {
                            "cylindre" => 1,
                            "plaque" => 2,
                            _ => 0,
                        },
                        size: arr3("size", [1.0, 1.0, 1.0]),
                        off: arr3("pos", [0.0; 3]),
                        yaw: s["yaw"].as_f64().unwrap_or(0.0) as f32,
                        color: color.unwrap_or([180, 170, 160]),
                        tex: s["texture"].as_str().map(|t| t.to_string()),
                    });
                }
                let n = ps::clean_name(&name);
                let def = ps::PropDef { name: n.clone(), shapes, coll };
                self.push_undo();
                match self.proj.props.iter().position(|p| p.name.eq_ignore_ascii_case(&n)) {
                    Some(i) => self.proj.props[i] = def.clone(),
                    None => self.proj.props.push(def.clone()),
                }
                self.props.coll = coll;
                self.build_shape_prop(&def)?
            }
        };
        let mut out = msg;
        if a["place"].as_bool().unwrap_or(false) {
            let n = self.props.last.clone();
            let at = a["at"].as_array().and_then(|v| {
                let f: Vec<f32> = v.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect();
                (f.len() >= 3).then(|| v3(f[0], f[1], f[2]))
            });
            let p = at.unwrap_or(self.cam.target);
            self.push_undo();
            let id = self.place_prop(&n, p, Some(0.0));
            self.set_selection(vec![id]);
            out += &format!("\nPosé : id {id} en {:.2}, {:.2}, {:.2}.", p.x, p.y, p.z);
        }
        Ok(txt(out))
    }

    fn mcp_select_arg(&mut self, a: &Value) -> Result<(), String> {
        if let Some(id) = a["id"].as_u64() {
            self.proj.find_id(id).ok_or("Objet introuvable.")?;
            self.set_selection(vec![id]);
        }
        if self.selected.is_empty() {
            return Err("Aucun bâtiment choisi : demande à la personne de cliquer dessus dans la vue, ou donne son id.".into());
        }
        Ok(())
    }

    fn mcp_selection(&self) -> String {
        if self.selected.is_empty() {
            return "Rien n'est choisi dans la vue. Demande à la personne de cliquer l'objet (Ctrl+clic pour plusieurs), ou utilise look_around.".into();
        }
        let mut s = format!("{} objet(s) choisi(s) :\n", self.selected.len());
        for id in self.selected.iter().take(40) {
            let Some(r) = self.proj.find_id(*id) else { continue };
            let Some(e) = self.proj.world_entity(r) else { continue };
            let what = match r {
                crate::model::ItemRef::MloEnt(t, m, _) => {
                    let mlo = &self.proj.ytyps[t].mlos[m];
                    let room = self.proj.ent(r).map(|x| x.room).unwrap_or(0);
                    format!("objet de l'intérieur « {} », pièce {} « {} »", mlo.name, room, mlo.rooms.get(room).map(|x| x.name.as_str()).unwrap_or("?"))
                }
                crate::model::ItemRef::Ent(y, _) => {
                    let ym = &self.proj.ymaps[y];
                    if e.mlo.is_some() {
                        match self.proj.find_mlo(&e.archetype) {
                            Some(m) => format!("placement de l'intérieur « {} » du projet ({} pièce(s)) — open_here ou open_studio pour l'ouvrir", m.name, m.rooms.len().saturating_sub(1)),
                            None => format!("placement d'un intérieur du jeu « {} » (pas modifiable)", e.archetype),
                        }
                    } else if self.house_of(&e.archetype).is_some() {
                        "bâtiment de l'atelier (se transforme en intérieur depuis l'atelier)".to_string()
                    } else if ym.from_game || self.proj.building_cuts.iter().any(|c| c.name.eq_ignore_ascii_case(&e.archetype)) {
                        format!("bâtiment ou objet du jeu (carte « {} ») — analyze_building puis building_to_interior pour en faire un intérieur", ym.name)
                    } else {
                        format!("objet posé dans la carte « {} »", ym.name)
                    }
                }
                #[allow(unreachable_patterns)]
                _ => "objet".to_string(),
            };
            let (mn, mx) = e.local_box(&self.proj);
            let sz = (mx - mn) * e.scale_xy;
            s += &format!("- id {} « {} » : {what} ; position ({:.2}, {:.2}, {:.2}), cap {:.0}°, taille {:.1} × {:.1} × {:.1} m\n", id, e.archetype, e.pos.x, e.pos.y, e.pos.z, e.rot.heading_deg(), sz.x, sz.y, sz.z);
        }
        s
    }

    fn mcp_around(&self, a: &Value) -> String {
        let c = match (a["x"].as_f64(), a["y"].as_f64()) {
            (Some(x), Some(y)) => v3(x as f32, y as f32, self.cam.target.z),
            _ => self.cam.target,
        };
        let radius = a["radius"].as_f64().unwrap_or(40.0).clamp(1.0, 500.0) as f32;
        let filter = a["filter"].as_str().unwrap_or("");
        let mut s = format!("Autour de ({:.1}, {:.1}, {:.1}), rayon {radius:.0} m.\n", c.x, c.y, c.z);
        match self.mlo_at(c) {
            Some((t, m, ri, _)) => {
                let mlo = &self.proj.ytyps[t].mlos[m];
                s += &format!("Ce point est dans l'intérieur du projet « {} », pièce {ri} « {} ».\n", mlo.name, mlo.rooms.get(ri).map(|r| r.name.as_str()).unwrap_or("?"));
            }
            None => s += "Ce point n'est dans aucun intérieur du projet.\n",
        }
        let mut near: Vec<(f32, u64, String)> = self
            .items_in_radius(c, radius, filter, true)
            .into_iter()
            .filter_map(|id| self.proj.find_id(id).and_then(|r| self.proj.world_entity(r)).map(|e| ((e.pos - c).len(), id, e.archetype)))
            .collect();
        near.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        s += &format!("{} objet(s) du projet :\n", near.len());
        for (d, id, n) in near.iter().take(40) {
            s += &format!("  id {id} {n} à {d:.1} m\n");
        }
        if let Some(w) = self.game.world.as_ref() {
            let mut ints: Vec<(f32, String)> = w.ents.iter().filter(|e| e.mlo && (e.pos - c).len() < radius.max(60.0)).map(|e| ((e.pos - c).len(), w.name_of(e.arch))).collect();
            ints.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
            if !ints.is_empty() {
                s += "Intérieurs du jeu proches :\n";
                for (d, n) in ints.iter().take(10) {
                    s += &format!("  {n} à {d:.0} m\n");
                }
            }
        }
        s
    }

    fn mcp_game_interiors(&self, a: &Value) -> String {
        let Some(w) = self.game.world.as_ref() else { return "Le jeu n'est pas chargé.".into() };
        let q = a["query"].as_str().unwrap_or("").to_lowercase();
        let limit = a["limit"].as_u64().unwrap_or(20).clamp(1, 100) as usize;
        let c = self.cam.target;
        let mut map: std::collections::HashMap<u32, (f32, V3)> = std::collections::HashMap::new();
        for e in w.ents.iter().filter(|e| e.mlo) {
            let d = (e.pos - c).len();
            let ent = map.entry(e.arch).or_insert((f32::MAX, e.pos));
            if d < ent.0 {
                *ent = (d, e.pos);
            }
        }
        let mut v: Vec<(String, f32, V3, usize)> = map
            .into_iter()
            .map(|(arch, (d, p))| (w.name_of(arch), d, p, w.mlo_defs.get(&arch).map(|m| m.children.len()).unwrap_or(0)))
            .filter(|x| q.is_empty() || x.0.to_lowercase().contains(&q))
            .collect();
        v.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut s = format!("{} intérieur(s) du jeu{} (les plus proches d'abord) :\n", v.len(), if q.is_empty() { String::new() } else { format!(" pour « {q} »") });
        for (n, d, p, k) in v.into_iter().take(limit) {
            s += &format!("  {n} : {k} objet(s), à {d:.0} m, en ({:.1}, {:.1}, {:.1})\n", p.x, p.y, p.z);
        }
        s
    }
}
