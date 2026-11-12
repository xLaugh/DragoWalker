//! Lecture / écriture des fichiers XML ymap (CMapData) et ytyp (CMapTypes)
//! au format que CodeWalker importe et exporte.

use crate::math::{v3, Quat, V3};
use crate::model::*;
use std::fmt::Write;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
fn f(x: f32) -> String {
    if x == x.trunc() && x.abs() < 1e9 {
        format!("{}", x as i64)
    } else {
        format!("{:.7}", x).trim_end_matches('0').trim_end_matches('.').to_string()
    }
}
fn vec_tag(out: &mut String, ind: &str, tag: &str, v: V3) {
    let _ = writeln!(out, "{ind}<{tag} x=\"{}\" y=\"{}\" z=\"{}\" />", f(v.x), f(v.y), f(v.z));
}
fn val(out: &mut String, ind: &str, tag: &str, v: impl std::fmt::Display) {
    let _ = writeln!(out, "{ind}<{tag} value=\"{v}\" />");
}
fn text(out: &mut String, ind: &str, tag: &str, t: &str) {
    if t.is_empty() {
        let _ = writeln!(out, "{ind}<{tag} />");
    } else {
        let _ = writeln!(out, "{ind}<{tag}>{}</{tag}>", esc(t));
    }
}

/// Écrit les champs d'une CEntityDef. La rotation du fichier est l'inverse de l'orientation.
fn entity_fields(out: &mut String, ind: &str, e: &Entity, invert: bool) {
    text(out, ind, "archetypeName", &e.archetype);
    val(out, ind, "flags", e.flags);
    val(out, ind, "guid", e.guid);
    vec_tag(out, ind, "position", e.pos);
    let q = if invert && !e.rot.is_identity() { e.rot.inverse() } else { e.rot };
    let _ = writeln!(out, "{ind}<rotation x=\"{}\" y=\"{}\" z=\"{}\" w=\"{}\" />", f(q.x), f(q.y), f(q.z), f(q.w));
    val(out, ind, "scaleXY", f(e.scale_xy));
    val(out, ind, "scaleZ", f(e.scale_z));
    val(out, ind, "parentIndex", e.parent_index);
    val(out, ind, "lodDist", f(e.lod_dist));
    val(out, ind, "childLodDist", f(e.child_lod_dist));
    text(out, ind, "lodLevel", &e.lod_level);
    val(out, ind, "numChildren", e.num_children);
    text(out, ind, "priorityLevel", &e.priority);
    if e.extensions_xml.trim().is_empty() {
        let _ = writeln!(out, "{ind}<extensions />");
    } else {
        let _ = writeln!(out, "{ind}{}", e.extensions_xml.trim());
    }
    val(out, ind, "ambientOcclusionMultiplier", e.ao_mult);
    val(out, ind, "artificialAmbientOcclusion", e.art_ao);
    val(out, ind, "tintValue", e.tint);
}

pub fn write_ymap(y: &Ymap, proj: &Project) -> String {
    let mut y = y.clone();
    y.calc_flags();
    let (emn, emx, smn, smx) = y.extents(proj);
    let mut o = String::new();
    o.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n");
    text(&mut o, " ", "name", &y.name);
    text(&mut o, " ", "parent", &y.parent);
    val(&mut o, " ", "flags", y.flags);
    val(&mut o, " ", "contentFlags", y.content_flags);
    vec_tag(&mut o, " ", "streamingExtentsMin", smn);
    vec_tag(&mut o, " ", "streamingExtentsMax", smx);
    vec_tag(&mut o, " ", "entitiesExtentsMin", emn);
    vec_tag(&mut o, " ", "entitiesExtentsMax", emx);
    if y.entities.is_empty() {
        o.push_str(" <entities />\n");
    } else {
        o.push_str(" <entities>\n");
        for e in &y.entities {
            match &e.mlo {
                None => {
                    o.push_str("  <Item type=\"CEntityDef\">\n");
                    entity_fields(&mut o, "   ", e, true);
                }
                Some(m) => {
                    // un placement d'intérieur : contrairement aux objets, le fichier stocke l'orientation telle quelle
                    // (CodeWalker YmapEntityDef::.ctor(CMloInstanceDef) 0x06002d07 : Orientation = rotation, sans
                    // Quaternion.Invert ; .ctor(CEntityDef) 0x06002d06 l'inverse)
                    o.push_str("  <Item type=\"CMloInstanceDef\">\n");
                    entity_fields(&mut o, "   ", e, false);
                    val(&mut o, "   ", "groupId", m.group_id);
                    val(&mut o, "   ", "floorId", m.floor_id);
                    if m.default_entity_sets.is_empty() {
                        o.push_str("   <defaultEntitySets />\n");
                    } else {
                        o.push_str("   <defaultEntitySets>\n");
                        for s in &m.default_entity_sets {
                            let _ = writeln!(o, "    <Item>{}</Item>", esc(s));
                        }
                        o.push_str("   </defaultEntitySets>\n");
                    }
                    // sorties vers l'extérieur recomptées depuis l'intérieur du projet s'il y est (les pièces et portails
                    // peuvent avoir été corrigés depuis), comme MloArchetype::UpdatePortalCounts (0x06003221) compte
                    // les portails qui touchent une pièce — ici la pièce 0, l'extérieur
                    let exits = proj.find_mlo(&e.archetype).map(|d| d.portal_count(0) as u32).unwrap_or(m.num_exit_portals);
                    val(&mut o, "   ", "numExitPortals", exits);
                    val(&mut o, "   ", "MLOInstflags", m.mlo_inst_flags);
                }
            }
            o.push_str("  </Item>\n");
        }
        o.push_str(" </entities>\n");
    }
    let defaults: &[(&str, &str)] = &[
        ("containerLods", " <containerLods />\n"),
        ("boxOccluders", " <boxOccluders />\n"),
        ("occludeModels", " <occludeModels />\n"),
        ("physicsDictionaries", " <physicsDictionaries />\n"),
        ("instancedData", " <instancedData>\n  <ImapLink />\n  <PropInstanceList />\n  <GrassInstanceList />\n </instancedData>\n"),
        ("timeCycleModifiers", " <timeCycleModifiers />\n"),
        ("carGenerators", " <carGenerators />\n"),
        ("LODLightsSOA", " <LODLightsSOA>\n  <direction />\n  <falloff />\n  <falloffExponent />\n  <timeAndStateFlags />\n  <hash />\n  <coneInnerAngle />\n  <coneOuterAngleOrCapExt />\n  <coronaIntensity />\n </LODLightsSOA>\n"),
        ("DistantLODLightsSOA", " <DistantLODLightsSOA>\n  <position />\n  <RGBI />\n  <numStreetLights value=\"0\" />\n  <category value=\"0\" />\n </DistantLODLightsSOA>\n"),
    ];
    for (tag, def) in defaults {
        // boîtes d'occlusion du projet (avec celles d'origine d'une carte du jeu copiée)
        if *tag == "boxOccluders" && !y.occluders.is_empty() {
            let ours = crate::occlusion::occluders_xml(&y.occluders);
            match y.raw_sections.iter().find(|(t, _)| t == tag).map(|(_, r)| r.trim().to_string()).filter(|r| r.contains("</boxOccluders>")) {
                Some(raw) => {
                    let items = ours.trim().trim_start_matches("<boxOccluders>").trim_end_matches("</boxOccluders>").to_string();
                    let _ = writeln!(o, " {}", raw.replacen("</boxOccluders>", &format!("{items}</boxOccluders>"), 1));
                }
                None => o.push_str(&ours),
            }
            continue;
        }
        // véhicules garés du projet (avec ceux d'origine d'une carte du jeu copiée)
        if *tag == "carGenerators" && !y.car_gens.is_empty() {
            let ours = crate::life::cargens_xml(&y.car_gens);
            match y.raw_sections.iter().find(|(t, _)| t == tag).map(|(_, r)| r.trim().to_string()).filter(|r| r.contains("</carGenerators>")) {
                Some(raw) => {
                    let items = ours.trim().trim_start_matches("<carGenerators>").trim_end_matches("</carGenerators>").to_string();
                    let _ = writeln!(o, " {}", raw.replacen("</carGenerators>", &format!("{items}</carGenerators>"), 1));
                }
                None => o.push_str(&ours),
            }
            continue;
        }
        if let Some((_, raw)) = y.raw_sections.iter().find(|(t, _)| t == tag) {
            let _ = writeln!(o, " {}", raw.trim());
        } else {
            o.push_str(def);
        }
    }
    o.push_str(" <block>\n");
    val(&mut o, "  ", "version", 0);
    val(&mut o, "  ", "flags", 0);
    text(&mut o, "  ", "name", &y.name);
    text(&mut o, "  ", "exportedBy", "DragoWalker");
    text(&mut o, "  ", "owner", "");
    text(&mut o, "  ", "time", "");
    o.push_str(" </block>\n</CMapData>\n");
    o
}

fn uint_array(o: &mut String, ind: &str, tag: &str, list: &[usize]) {
    if list.is_empty() {
        let _ = writeln!(o, "{ind}<{tag} />");
    } else {
        let _ = writeln!(o, "{ind}<{tag} content=\"int_array\">");
        for v in list {
            let _ = writeln!(o, "{ind} {v}");
        }
        let _ = writeln!(o, "{ind}</{tag}>");
    }
}

pub fn write_ytyp(t: &Ytyp) -> String {
    // CodeWalker refuse un objet d'intérieur dont la pièce n'existe pas (ProjectForm::NewMloEntity 0x06000d68,
    // « Room at index {0} does not exist ») : après suppression ou correction de pièces, un tel objet est rangé
    // dans la pièce où il se trouve vraiment
    let mut fixed = t.clone();
    for m in &mut fixed.mlos {
        let n = m.rooms.len();
        let moved: Vec<(usize, usize)> = m
            .entities
            .iter()
            .enumerate()
            .filter(|(_, e)| e.room >= n)
            .map(|(i, e)| (i, crate::model::room_at(m, e.pos)))
            .collect();
        for (i, r) in moved {
            m.entities[i].room = r;
        }
    }
    let t = &fixed;
    let mut o = String::new();
    o.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapTypes>\n <extensions />\n");
    if t.mlos.is_empty() && t.raw_archetypes.is_empty() {
        o.push_str(" <archetypes />\n");
    } else {
        o.push_str(" <archetypes>\n");
        for raw in &t.raw_archetypes {
            let _ = writeln!(o, "  {}", raw.trim());
        }
        for m in &t.mlos {
            let (mn, mx) = (m.bb_min(), m.bb_max());
            let c = (mn + mx) * 0.5;
            let r = (mx - mn).len() * 0.5;
            o.push_str("  <Item type=\"CMloArchetypeDef\">\n");
            val(&mut o, "   ", "lodDist", f(m.lod_dist));
            val(&mut o, "   ", "flags", m.flags);
            val(&mut o, "   ", "specialAttribute", 0);
            vec_tag(&mut o, "   ", "bbMin", mn);
            vec_tag(&mut o, "   ", "bbMax", mx);
            vec_tag(&mut o, "   ", "bsCentre", c);
            val(&mut o, "   ", "bsRadius", f(r));
            val(&mut o, "   ", "hdTextureDist", f(m.lod_dist));
            text(&mut o, "   ", "name", &m.name);
            text(&mut o, "   ", "textureDictionary", "");
            text(&mut o, "   ", "clipDictionary", "");
            text(&mut o, "   ", "drawableDictionary", "");
            text(&mut o, "   ", "physicsDictionary", "");
            text(&mut o, "   ", "assetType", "ASSET_TYPE_ASSETLESS");
            text(&mut o, "   ", "assetName", &m.name);
            o.push_str("   <extensions />\n");
            val(&mut o, "   ", "mloFlags", m.mlo_flags);
            if m.entities.is_empty() {
                o.push_str("   <entities />\n");
            } else {
                o.push_str("   <entities>\n");
                for e in &m.entities {
                    o.push_str("    <Item type=\"CEntityDef\">\n");
                    entity_fields(&mut o, "     ", e, true);
                    o.push_str("    </Item>\n");
                }
                o.push_str("   </entities>\n");
            }
            o.push_str("   <rooms>\n");
            for (ri, r) in m.rooms.iter().enumerate() {
                o.push_str("    <Item>\n");
                text(&mut o, "     ", "name", &r.name);
                vec_tag(&mut o, "     ", "bbMin", r.bb_min);
                vec_tag(&mut o, "     ", "bbMax", r.bb_max);
                val(&mut o, "     ", "blend", f(r.blend));
                text(&mut o, "     ", "timecycleName", &r.timecycle);
                text(&mut o, "     ", "secondaryTimecycleName", &r.secondary_timecycle);
                val(&mut o, "     ", "flags", r.flags);
                val(&mut o, "     ", "portalCount", m.portal_count(ri));
                val(&mut o, "     ", "floorId", r.floor_id);
                val(&mut o, "     ", "exteriorVisibiltyDepth", r.ext_vis_depth);
                uint_array(&mut o, "     ", "attachedObjects", &m.attached(ri));
                o.push_str("    </Item>\n");
            }
            o.push_str("   </rooms>\n");
            if m.portals.is_empty() {
                o.push_str("   <portals />\n");
            } else {
                o.push_str("   <portals>\n");
                for p in &m.portals {
                    o.push_str("    <Item>\n");
                    val(&mut o, "     ", "roomFrom", p.room_from);
                    val(&mut o, "     ", "roomTo", p.room_to);
                    val(&mut o, "     ", "flags", p.flags);
                    val(&mut o, "     ", "mirrorPriority", p.mirror_priority);
                    val(&mut o, "     ", "opacity", p.opacity);
                    val(&mut o, "     ", "audioOcclusion", p.audio_occlusion);
                    o.push_str("     <corners>\n");
                    for c in &p.corners {
                        vec_tag(&mut o, "      ", "Item", *c);
                    }
                    o.push_str("     </corners>\n");
                    o.push_str("     <attachedObjects />\n");
                    o.push_str("    </Item>\n");
                }
                o.push_str("   </portals>\n");
            }
            if m.entity_sets.is_empty() {
                o.push_str("   <entitySets />\n");
            } else {
                o.push_str("   <entitySets>\n");
                for s in &m.entity_sets {
                    o.push_str("    <Item>\n");
                    text(&mut o, "     ", "name", &s.name);
                    o.push_str("     <locations />\n");
                    if s.entities.is_empty() {
                        o.push_str("     <entities />\n");
                    } else {
                        o.push_str("     <entities>\n");
                        for e in &s.entities {
                            o.push_str("      <Item type=\"CEntityDef\">\n");
                            entity_fields(&mut o, "       ", e, true);
                            o.push_str("      </Item>\n");
                        }
                        o.push_str("     </entities>\n");
                    }
                    o.push_str("    </Item>\n");
                }
                o.push_str("   </entitySets>\n");
            }
            o.push_str("   <timeCycleModifiers />\n");
            o.push_str("  </Item>\n");
        }
        o.push_str(" </archetypes>\n");
    }
    text(&mut o, " ", "name", &t.name);
    o.push_str(" <dependencies />\n <compositeEntityTypes />\n</CMapTypes>\n");
    o
}

// ---------------------------------------------------------------- lecture

type Node<'a, 'b> = roxmltree::Node<'a, 'b>;

fn child<'a, 'b>(n: Node<'a, 'b>, tag: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == tag)
}
fn ctext(n: Node, tag: &str) -> String {
    child(n, tag).and_then(|c| c.text()).unwrap_or("").trim().to_string()
}
fn cval_f(n: Node, tag: &str, d: f32) -> f32 {
    child(n, tag).and_then(|c| c.attribute("value")).and_then(|v| v.parse().ok()).unwrap_or(d)
}
fn cval_i(n: Node, tag: &str, d: i64) -> i64 {
    child(n, tag)
        .and_then(|c| c.attribute("value"))
        .and_then(|v| {
            let v = v.trim();
            if let Some(h) = v.strip_prefix("0x") {
                i64::from_str_radix(h, 16).ok()
            } else {
                v.parse::<i64>().ok().or_else(|| v.parse::<f64>().ok().map(|x| x as i64))
            }
        })
        .unwrap_or(d)
}
fn attr_f(n: Node, a: &str) -> f32 {
    n.attribute(a).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}
fn cvec(n: Node, tag: &str) -> V3 {
    child(n, tag).map(|c| v3(attr_f(c, "x"), attr_f(c, "y"), attr_f(c, "z"))).unwrap_or(V3::ZERO)
}
fn raw<'a>(src: &'a str, n: Node) -> &'a str {
    &src[n.range()]
}

fn read_entity(src: &str, n: Node, invert: bool) -> Entity {
    let mut e = Entity::new(&ctext(n, "archetypeName"), cvec(n, "position"));
    e.flags = cval_i(n, "flags", e.flags as i64) as u32;
    e.guid = cval_i(n, "guid", 0) as u32;
    if let Some(r) = child(n, "rotation") {
        let q = Quat { x: attr_f(r, "x"), y: attr_f(r, "y"), z: attr_f(r, "z"), w: r.attribute("w").and_then(|v| v.parse().ok()).unwrap_or(1.0) };
        e.rot = if invert && !q.is_identity() { q.inverse().normalized() } else { q.normalized() };
    }
    e.scale_xy = cval_f(n, "scaleXY", 1.0);
    e.scale_z = cval_f(n, "scaleZ", 1.0);
    e.parent_index = cval_i(n, "parentIndex", -1) as i32;
    e.lod_dist = cval_f(n, "lodDist", e.lod_dist);
    e.child_lod_dist = cval_f(n, "childLodDist", 0.0);
    let ll = ctext(n, "lodLevel");
    if !ll.is_empty() {
        e.lod_level = ll;
    }
    e.num_children = cval_i(n, "numChildren", 0) as u32;
    let pr = ctext(n, "priorityLevel");
    if !pr.is_empty() {
        e.priority = pr;
    }
    e.ao_mult = cval_i(n, "ambientOcclusionMultiplier", 255) as i32;
    e.art_ao = cval_i(n, "artificialAmbientOcclusion", 255) as i32;
    e.tint = cval_i(n, "tintValue", 0) as u32;
    if let Some(x) = child(n, "extensions") {
        if x.children().any(|c| c.is_element()) {
            e.extensions_xml = raw(src, x).to_string();
        }
    }
    e
}

pub fn read_ymap(src: &str, fallback_name: &str) -> Result<Ymap, String> {
    let doc = roxmltree::Document::parse(src).map_err(|e| format!("XML invalide : {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "CMapData" {
        return Err(format!("Ce n'est pas un ymap XML (balise <{}>)", root.tag_name().name()));
    }
    let mut name = ctext(root, "name");
    if name.is_empty() {
        name = fallback_name.to_string();
    }
    let mut y = Ymap::new(&name);
    y.parent = ctext(root, "parent");
    y.flags = cval_i(root, "flags", 0) as u32;
    y.content_flags = cval_i(root, "contentFlags", 65) as u32;
    if let Some(ents) = child(root, "entities") {
        for it in ents.children().filter(|c| c.is_element()) {
            let ty = it.attribute("type").unwrap_or("CEntityDef");
            if ty == "CMloInstanceDef" {
                // orientation non inversée dans le fichier (YmapEntityDef::.ctor(CMloInstanceDef) 0x06002d07)
                let mut e = read_entity(src, it, false);
                let mut m = MloInstData {
                    group_id: cval_i(it, "groupId", 0) as u32,
                    floor_id: cval_i(it, "floorId", 0) as u32,
                    num_exit_portals: cval_i(it, "numExitPortals", 0) as u32,
                    mlo_inst_flags: cval_i(it, "MLOInstflags", 0) as u32,
                    default_entity_sets: Vec::new(),
                };
                if let Some(d) = child(it, "defaultEntitySets") {
                    for s in d.children().filter(|c| c.is_element()) {
                        m.default_entity_sets.push(s.text().unwrap_or("").trim().to_string());
                    }
                }
                e.mlo = Some(m);
                y.entities.push(e);
            } else {
                y.entities.push(read_entity(src, it, true));
            }
        }
    }
    for tag in ["containerLods", "boxOccluders", "occludeModels", "physicsDictionaries", "instancedData", "timeCycleModifiers", "carGenerators", "LODLightsSOA", "DistantLODLightsSOA"] {
        if let Some(n) = child(root, tag) {
            if n.descendants().any(|d| d.is_element() && d != n && (d.has_children() || d.attribute("value").map(|v| v != "0").unwrap_or(false))) {
                y.raw_sections.push((tag.to_string(), raw(src, n).to_string()));
            }
        }
    }
    Ok(y)
}

fn parse_uints(n: Option<Node>) -> Vec<usize> {
    n.and_then(|n| n.text()).map(|t| t.split_whitespace().filter_map(|s| s.parse().ok()).collect()).unwrap_or_default()
}

pub fn read_ytyp(src: &str, fallback_name: &str) -> Result<Ytyp, String> {
    let doc = roxmltree::Document::parse(src).map_err(|e| format!("XML invalide : {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "CMapTypes" {
        return Err(format!("Ce n'est pas un ytyp XML (balise <{}>)", root.tag_name().name()));
    }
    let mut name = ctext(root, "name");
    if name.is_empty() {
        name = fallback_name.to_string();
    }
    let mut t = Ytyp { name, mlos: Vec::new(), visible: true, raw_archetypes: Vec::new() };
    if let Some(arcs) = child(root, "archetypes") {
        for it in arcs.children().filter(|c| c.is_element()) {
            if it.attribute("type") != Some("CMloArchetypeDef") {
                t.raw_archetypes.push(raw(src, it).to_string());
                continue;
            }
            let mut m = MloArchetype {
                name: ctext(it, "name"),
                lod_dist: cval_f(it, "lodDist", 200.0),
                flags: cval_i(it, "flags", 0) as u32,
                mlo_flags: cval_i(it, "mloFlags", 0) as u32,
                entities: Vec::new(),
                rooms: Vec::new(),
                portals: Vec::new(),
                entity_sets: Vec::new(),
            };
            if let Some(ents) = child(it, "entities") {
                for e in ents.children().filter(|c| c.is_element()) {
                    m.entities.push(read_entity(src, e, true));
                }
            }
            if let Some(rooms) = child(it, "rooms") {
                for (ri, r) in rooms.children().filter(|c| c.is_element()).enumerate() {
                    let mut room = Room::new(&ctext(r, "name"), cvec(r, "bbMin"), cvec(r, "bbMax"));
                    room.blend = cval_f(r, "blend", 1.0);
                    room.timecycle = ctext(r, "timecycleName");
                    room.secondary_timecycle = ctext(r, "secondaryTimecycleName");
                    room.flags = cval_i(r, "flags", 96) as u32;
                    room.floor_id = cval_i(r, "floorId", 0) as u32;
                    room.ext_vis_depth = cval_i(r, "exteriorVisibiltyDepth", -1) as i32;
                    for idx in parse_uints(child(r, "attachedObjects")) {
                        if let Some(e) = m.entities.get_mut(idx) {
                            e.room = ri;
                        }
                    }
                    m.rooms.push(room);
                }
            }
            if let Some(ports) = child(it, "portals") {
                for p in ports.children().filter(|c| c.is_element()) {
                    let mut corners = [V3::ZERO; 4];
                    if let Some(cs) = child(p, "corners") {
                        for (i, c) in cs.children().filter(|c| c.is_element()).take(4).enumerate() {
                            corners[i] = v3(attr_f(c, "x"), attr_f(c, "y"), attr_f(c, "z"));
                        }
                    }
                    let mut portal = Portal::new(cval_i(p, "roomFrom", 0) as u32, cval_i(p, "roomTo", 0) as u32, corners);
                    portal.flags = cval_i(p, "flags", 0) as u32;
                    portal.mirror_priority = cval_i(p, "mirrorPriority", 0) as u32;
                    portal.opacity = cval_i(p, "opacity", 0) as u32;
                    portal.audio_occlusion = cval_i(p, "audioOcclusion", 0) as u32;
                    m.portals.push(portal);
                }
            }
            if let Some(sets) = child(it, "entitySets") {
                for s in sets.children().filter(|c| c.is_element()) {
                    let mut set = EntitySet { name: ctext(s, "name"), entities: Vec::new() };
                    if let Some(ents) = child(s, "entities") {
                        for e in ents.children().filter(|c| c.is_element()) {
                            set.entities.push(read_entity(src, e, true));
                        }
                    }
                    m.entity_sets.push(set);
                }
            }
            t.mlos.push(m);
        }
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ymap_roundtrip() {
        let mut p = Project::default();
        let mut y = Ymap::new("test");
        let mut e = Entity::new("prop_bench_01a", v3(10.0, 20.0, 30.0));
        e.rot = Quat::from_z(0.5);
        y.entities.push(e);
        p.ymaps.push(y.clone());
        let s = write_ymap(&y, &p);
        let back = read_ymap(&s, "x").unwrap();
        assert_eq!(back.entities.len(), 1);
        assert!((back.entities[0].rot.heading_deg() - 0.5f32.to_degrees()).abs() < 0.01);
        assert_eq!(back.content_flags, 65);
    }
    #[test]
    fn ytyp_roundtrip() {
        let mut m = build_mlo("dw_test", 2, 1, 6.0, 5.0, 3.0);
        let mut e = Entity::new("prop_table_03", v3(1.0, 0.0, 0.0));
        e.room = 2;
        m.entities.push(e);
        let t = Ytyp { name: "t".into(), mlos: vec![m], visible: true, raw_archetypes: vec![] };
        let s = write_ytyp(&t);
        let back = read_ytyp(&s, "t").unwrap();
        assert_eq!(back.mlos[0].rooms.len(), 3);
        assert_eq!(back.mlos[0].portals.len(), 2);
        assert_eq!(back.mlos[0].entities[0].room, 2);
    }
}
