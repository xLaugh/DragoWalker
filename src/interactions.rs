//! Points d'interaction des intérieurs pour FiveM : les meubles qu'on utilise en jeu (caisse, frigo, vestiaire,
//! armoire d'armes, lit, paillasse de labo…) sont repérés d'après leur modèle et exportés en zones ox_target ou
//! qb-target (dw_targets.lua) ; choisir une zone déclenche l'évènement client « dragowalker:interaction », à brancher
//! sur les scripts du serveur. Aussi en JSON (dw_targets.json) pour qui veut les lire autrement.

use crate::app::DwApp;
use crate::look::{self, ic};
use crate::math::{v3, V3};
use eframe::egui::{self, RichText};
use std::path::Path;

/// Sortes de points : (mots du modèle, mots exclus, identifiant, libellé, icône Font Awesome).
const KINDS: &[(&[&str], &[&str], &str, &str, &str)] = &[
    (&["till"], &[], "caisse", "Caisse", "fa-solid fa-cash-register"),
    (&["gunlocker", "gunrack", "guncase", "gun_case"], &["ammo"], "armes", "Armes", "fa-solid fa-gun"),
    (&["locker"], &["door", "light"], "vestiaire", "Vestiaire", "fa-solid fa-shirt"),
    (&["wardrobe"], &[], "garde_robe", "Garde-robe", "fa-solid fa-shirt"),
    (&["_safe", "safe_"], &[], "coffre", "Coffre", "fa-solid fa-vault"),
    (&["fridge", "cooler"], &["door", "water"], "frigo", "Frigo", "fa-solid fa-temperature-low"),
    (&["cooker", "ovenhob", "int_oven", "kebab_grill"], &[], "cuisiner", "Cuisiner", "fa-solid fa-fire-burner"),
    (&["meth_table", "coke_table", "weed_table"], &["powder"], "labo", "Paillasse", "fa-solid fa-flask"),
    (&["washer"], &[], "laver", "Machine à laver", "fa-solid fa-soap"),
    (&["hd_chair"], &[], "coiffure", "Fauteuil de coiffure", "fa-solid fa-scissors"),
    (&["med_bed", "gurney"], &[], "soins", "Lit de soins", "fa-solid fa-kit-medical"),
    (&["dj_desk"], &[], "dj", "Platines", "fa-solid fa-music"),
    (&["jukebox"], &[], "jukebox", "Jukebox", "fa-solid fa-music"),
    (&["vend_"], &[], "distributeur", "Distributeur", "fa-solid fa-cookie-bite"),
    (&["toolchest"], &[], "etabli", "Établi", "fa-solid fa-wrench"),
    (&["receptiondesk", "reception_desk"], &[], "accueil", "Accueil", "fa-solid fa-bell-concierge"),
    (&["toilet"], &["roll", "brush", "soap", "shamp", "door", "light"], "wc", "Toilettes", "fa-solid fa-toilet"),
    (&["sink"], &["filler"], "lavabo", "Lavabo", "fa-solid fa-hand-holding-droplet"),
    (&["mbath", "shower"], &["door", "towel", "rack"], "bain", "Bain", "fa-solid fa-bath"),
    (&["weight_rack", "muscle_bench", "weight_bench"], &[], "sport", "Musculation", "fa-solid fa-dumbbell"),
    (&["bed"], &["bedside", "bedtable", "lamp", "clock", "books"], "lit", "Lit", "fa-solid fa-bed"),
    (&["coffee_mac", "coffeemachine"], &[], "cafe", "Café", "fa-solid fa-mug-hot"),
    (&["copier"], &[], "photocopieuse", "Photocopieuse", "fa-solid fa-print"),
];

/// Sorte de point d'un modèle : (identifiant, libellé, icône), sinon None.
pub fn kind_of(model: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let n = model.to_lowercase();
    if crate::decor::is_desk(&n) {
        return Some(("ordinateur", "Ordinateur", "fa-solid fa-computer"));
    }
    KINDS.iter().find(|(w, no, ..)| w.iter().any(|w| n.contains(w)) && !no.iter().any(|x| n.contains(x))).map(|k| (k.2, k.3, k.4))
}

/// Un point d'interaction, dans le monde.
#[derive(Clone, Debug)]
pub struct Point {
    pub kind: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub mlo: String,
    pub room: String,
    pub model: String,
    pub coords: V3,
    pub size: V3,
    pub heading: f32,
}

fn lua_str(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Script client : une zone ox_target ou qb-target par point.
pub fn targets_lua(points: &[Point]) -> String {
    let mut s = String::from(
        "-- Points d'interaction des intérieurs, écrits par DragoWalker.\n\
         -- Avec ox_target ou qb-target démarré, une zone par point ; la choisir déclenche l'évènement client\n\
         -- 'dragowalker:interaction' avec le point (id, kind, label, mlo, room, model, coords) : à brancher sur tes\n\
         -- scripts, par exemple :\n\
         --   AddEventHandler('dragowalker:interaction', function(p)\n\
         --       if p.kind == 'caisse' then TriggerEvent('mon_script:ouvrir_caisse', p) end\n\
         --   end)\n\
         DW_TARGETS = {\n",
    );
    for (i, p) in points.iter().enumerate() {
        s.push_str(&format!(
            "    {{ id = 'dw_pt_{}', kind = {}, label = {}, icon = {}, mlo = {}, room = {}, model = {}, coords = vec3({:.3}, {:.3}, {:.3}), size = vec3({:.2}, {:.2}, {:.2}), heading = {:.1} }},\n",
            i + 1,
            lua_str(p.kind),
            lua_str(p.label),
            lua_str(p.icon),
            lua_str(&p.mlo),
            lua_str(&p.room),
            lua_str(&p.model),
            p.coords.x,
            p.coords.y,
            p.coords.z,
            p.size.x,
            p.size.y,
            p.size.z,
            p.heading
        ));
    }
    s.push_str(
        "}\n\n\
         CreateThread(function()\n\
         \x20   Wait(1000)\n\
         \x20   if GetResourceState('ox_target') == 'started' then\n\
         \x20       for _, p in ipairs(DW_TARGETS) do\n\
         \x20           exports.ox_target:addBoxZone({\n\
         \x20               name = p.id, coords = p.coords, size = p.size, rotation = p.heading, debug = false,\n\
         \x20               options = { { name = p.id, label = p.label, icon = p.icon, distance = 2.0,\n\
         \x20                   onSelect = function() TriggerEvent('dragowalker:interaction', p) end } },\n\
         \x20           })\n\
         \x20       end\n\
         \x20   elseif GetResourceState('qb-target') == 'started' then\n\
         \x20       for _, p in ipairs(DW_TARGETS) do\n\
         \x20           exports['qb-target']:AddBoxZone(p.id, p.coords, p.size.y, p.size.x, {\n\
         \x20               name = p.id, heading = p.heading, debugPoly = false,\n\
         \x20               minZ = p.coords.z - p.size.z / 2, maxZ = p.coords.z + p.size.z / 2,\n\
         \x20           }, { options = { { icon = p.icon, label = p.label,\n\
         \x20               action = function() TriggerEvent('dragowalker:interaction', p) end } }, distance = 2.0 })\n\
         \x20       end\n\
         \x20   end\n\
         end)\n",
    );
    s
}

/// Les mêmes points en JSON.
pub fn targets_json(points: &[Point]) -> String {
    let rows: Vec<String> = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "  {{\"id\": \"dw_pt_{}\", \"kind\": {}, \"label\": {}, \"mlo\": {}, \"room\": {}, \"model\": {}, \"coords\": [{:.3}, {:.3}, {:.3}], \"size\": [{:.2}, {:.2}, {:.2}], \"heading\": {:.1}}}",
                i + 1,
                json_str(p.kind),
                json_str(p.label),
                json_str(&p.mlo),
                json_str(&p.room),
                json_str(&p.model),
                p.coords.x,
                p.coords.y,
                p.coords.z,
                p.size.x,
                p.size.y,
                p.size.z,
                p.heading
            )
        })
        .collect();
    format!("[\n{}\n]\n", rows.join(",\n"))
}

impl DwApp {
    /// Points d'interaction de tous les intérieurs placés du projet (coordonnées du monde).
    pub fn interaction_points(&self) -> Vec<Point> {
        let mut out = Vec::new();
        for inst in self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.entities.iter()).filter(|e| e.mlo.is_some()) {
            let Some(m) = self.proj.find_mlo(&inst.archetype) else { continue };
            for e in &m.entities {
                let Some((kind, label, icon)) = kind_of(&e.archetype) else { continue };
                let Some((s, mn)) = self.decor_dims(&e.archetype) else { continue };
                let sc = v3(e.scale_xy, e.scale_xy, e.scale_z);
                let local = e.pos + e.rot.rotate(v3((mn.x + s.x * 0.5) * sc.x, (mn.y + s.y * 0.5) * sc.y, (mn.z + s.z * 0.5) * sc.z));
                let coords = inst.pos + inst.rot.rotate(local);
                let size = v3((s.x * sc.x + 0.1).max(0.5), (s.y * sc.y + 0.1).max(0.5), (s.z * sc.z + 0.1).max(0.6));
                let heading = inst.rot.mul(e.rot).heading_deg();
                let room = m.rooms.get(e.room).map(|r| r.name.clone()).unwrap_or_default();
                out.push(Point { kind, label, icon, mlo: m.name.clone(), room, model: e.archetype.clone(), coords, size, heading });
            }
        }
        out
    }

    /// Écrit dw_targets.lua et dw_targets.json dans la ressource si l'option est cochée ; sinon les retire. Rend le
    /// nombre de points.
    pub fn export_targets(&self, out: &Path) -> Result<usize, String> {
        let lua = out.join("dw_targets.lua");
        let json = out.join("dw_targets.json");
        if !self.proj.targets {
            let _ = std::fs::remove_file(&lua);
            let _ = std::fs::remove_file(&json);
            return Ok(0);
        }
        let pts = self.interaction_points();
        if pts.is_empty() {
            let _ = std::fs::remove_file(&lua);
            let _ = std::fs::remove_file(&json);
            return Ok(0);
        }
        std::fs::write(&lua, targets_lua(&pts)).map_err(|e| e.to_string())?;
        std::fs::write(&json, targets_json(&pts)).map_err(|e| e.to_string())?;
        Ok(pts.len())
    }

    /// Réglage et résumé des points d'interaction (panneau de l'intérieur).
    pub fn targets_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new(look::label(ic::HAND_POINTING, "Points d'interaction (FiveM)")).strong()).id_salt("targets_hdr").default_open(false).show(ui, |ui| {
            ui.label(
                RichText::new(
                    "Les meubles qu'on utilise en jeu (caisse, frigo, vestiaire, armes, lit, paillasse…) deviennent des zones ox_target ou qb-target à l'export (dw_targets.lua) ; les choisir déclenche l'évènement client « dragowalker:interaction » à brancher sur tes scripts.",
                )
                .small()
                .weak(),
            );
            let mut on = self.proj.targets;
            if ui.checkbox(&mut on, "Exporter les points d'interaction").changed() {
                self.push_undo();
                self.proj.targets = on;
            }
            let pts = self.interaction_points();
            if pts.is_empty() {
                ui.label(RichText::new("Aucun meuble utilisable dans les intérieurs placés.").weak());
                return;
            }
            let mut counts: Vec<(&str, usize)> = Vec::new();
            for p in &pts {
                match counts.iter_mut().find(|c| c.0 == p.label) {
                    Some(c) => c.1 += 1,
                    None => counts.push((p.label, 1)),
                }
            }
            let txt: Vec<String> = counts.iter().map(|(l, n)| format!("{l} ×{n}")).collect();
            ui.label(format!("{} point(s) : {}", pts.len(), txt.join(", ")));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sortes_reconnues() {
        assert_eq!(kind_of("prop_till_01").map(|k| k.0), Some("caisse"));
        assert_eq!(kind_of("p_cs_locker_01").map(|k| k.0), Some("vestiaire"));
        assert_eq!(kind_of("bkr_prop_gunlocker_01a").map(|k| k.0), Some("armes"));
        assert_eq!(kind_of("v_res_tre_bed2").map(|k| k.0), Some("lit"));
        assert_eq!(kind_of("v_res_tre_bedsidetable"), None);
        assert_eq!(kind_of("prop_toilet_roll_01"), None);
        assert_eq!(kind_of("v_corp_officedesk_5").map(|k| k.0), Some("ordinateur"));
        assert_eq!(kind_of("prop_plant_int_01b"), None);
    }

    #[test]
    fn script_lisible() {
        let p = Point { kind: "caisse", label: "Caisse", icon: "fa-solid fa-cash-register", mlo: "bar_l'ouest".into(), room: "salle".into(), model: "prop_till_01".into(), coords: v3(1.0, 2.0, 3.0), size: v3(0.5, 0.5, 0.6), heading: 90.0 };
        let s = targets_lua(&[p.clone()]);
        assert!(s.contains("mlo = 'bar_l\\'ouest'"));
        assert!(s.contains("exports.ox_target:addBoxZone"));
        assert!(s.contains("exports['qb-target']:AddBoxZone"));
        let j = targets_json(&[p]);
        assert!(j.contains("\"kind\": \"caisse\""));
    }
}
