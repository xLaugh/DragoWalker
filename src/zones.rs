//! Zones de quartier : rectangles tracés sur la carte, avec un nom (affiché en jeu quand on y entre, comme les noms
//! de quartier du jeu), la densité des piétons et des voitures, et une ambiance (modificateur de timecycle du jeu).
//! À l'export, un script client (dw_zones.lua) suit le joueur : densités réglées à chaque image dans la zone
//! (SetPedDensityMultiplierThisFrame, SetVehicleDensityMultiplierThisFrame…), ambiance posée à l'entrée
//! (SetTimecycleModifier + SetTimecycleModifierStrength) et retirée à la sortie.

use crate::app::DwApp;
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, Pos2, RichText};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Zone {
    pub name: String,
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// densité des piétons et des voitures (1 = comme le jeu, 0 = personne)
    pub peds: f32,
    pub cars: f32,
    /// ambiance : modificateur de timecycle du jeu (vide = aucune) et sa force (0 à 1)
    pub mood: String,
    pub strength: f32,
}

/// Ambiances proposées : (modificateur de timecycle, nom affiché). Noms relevés dans timecycle_mods_1..4.xml du jeu
/// (sonde DW_TEXT_DUMP, 18 nov. 2026).
pub const MOODS: [(&str, &str); 13] = [
    ("", "Aucune"),
    ("rply_saturation", "Couleurs vives"),
    ("hud_def_desat_cold", "Froid et terne"),
    ("glasses_brown", "Chaud, sépia léger"),
    ("nervousRON_fog", "Brumeux"),
    ("MP_Arena_theme_morning", "Matin doré"),
    ("MP_Arena_theme_evening", "Soirée"),
    ("MP_Arena_theme_sandstorm", "Tempête de sable"),
    ("MP_Arena_theme_toxic", "Toxique"),
    ("Hicksbar", "Campagne chaude"),
    ("BloomLight", "Lumineux"),
    ("rply_vignette", "Bords sombres"),
    ("prologue", "Hiver"),
];

impl Zone {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.min[0] && x <= self.max[0] && y >= self.min[1] && y <= self.max[1]
    }
}

/// Script client des zones (dw_zones.lua).
pub fn zones_lua(zones: &[Zone]) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'");
    let mut list = String::new();
    for z in zones {
        list += &format!(
            "  {{ name = '{}', x0 = {:.1}, y0 = {:.1}, x1 = {:.1}, y1 = {:.1}, peds = {:.2}, cars = {:.2}, mood = '{}', strength = {:.2} }},\n",
            esc(&z.name),
            z.min[0],
            z.min[1],
            z.max[0],
            z.max[1],
            z.peds,
            z.cars,
            esc(&z.mood),
            z.strength
        );
    }
    format!(
        r#"-- DragoWalker : zones de quartier (nom affiché à l'entrée, piétons et voitures, ambiance)
local zones = {{
{list}}}

local current = nil
local shownAt = 0

local function zoneAt(p)
  for i, z in ipairs(zones) do
    if p.x >= z.x0 and p.x <= z.x1 and p.y >= z.y0 and p.y <= z.y1 then return i end
  end
  return nil
end

CreateThread(function()
  while true do
    local p = GetEntityCoords(PlayerPedId())
    local i = zoneAt(p)
    if i ~= current then
      if current and zones[current].mood ~= '' then ClearTimecycleModifier() end
      current = i
      if i then
        local z = zones[i]
        if z.mood ~= '' then
          SetTimecycleModifier(z.mood)
          SetTimecycleModifierStrength(z.strength + 0.0)
        end
        shownAt = GetGameTimer()
      end
    end
    Wait(500)
  end
end)

-- à chaque image dans une zone : densités, et le nom pendant 5 s (en bas à droite, comme les quartiers du jeu)
CreateThread(function()
  while true do
    if current then
      local z = zones[current]
      SetPedDensityMultiplierThisFrame(z.peds + 0.0)
      SetScenarioPedDensityMultiplierThisFrame(z.peds + 0.0, z.peds + 0.0)
      SetVehicleDensityMultiplierThisFrame(z.cars + 0.0)
      SetRandomVehicleDensityMultiplierThisFrame(z.cars + 0.0)
      SetParkedVehicleDensityMultiplierThisFrame(z.cars + 0.0)
      local t = GetGameTimer() - shownAt
      if t < 5000 then
        local a = t < 4000 and 255 or math.floor(255 * (5000 - t) / 1000)
        SetTextFont(4)
        SetTextScale(0.6, 0.6)
        SetTextColour(255, 255, 255, a)
        SetTextOutline()
        SetTextRightJustify(true)
        SetTextWrap(0.0, 0.985)
        BeginTextCommandDisplayText('STRING')
        AddTextComponentSubstringPlayerName(z.name)
        EndTextCommandDisplayText(0.985, 0.88)
      end
      Wait(0)
    else
      Wait(300)
    end
  end
end)
"#
    )
}

impl DwApp {
    /// Outil Zone : glisser un rectangle sur la carte.
    pub fn zone_tool(&mut self, resp: &egui::Response) {
        use eframe::egui::PointerButton;
        let g = self.mouse_ground;
        if resp.drag_started_by(PointerButton::Primary) {
            self.life.drag_start = g;
        }
        if resp.drag_stopped() {
            if let (Some(a), Some(b)) = (self.life.drag_start.take(), g) {
                let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
                let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
                if x1 - x0 > 5.0 && y1 - y0 > 5.0 {
                    self.push_undo();
                    let name = format!("Quartier {}", self.proj.zones.len() + 1);
                    self.proj.zones.push(Zone { name: name.clone(), min: [x0, y0], max: [x1, y1], peds: 1.0, cars: 1.0, mood: String::new(), strength: 0.6 });
                    self.status = format!("Zone « {name} » créée ({:.0} × {:.0} m) : renomme-la et règle-la dans la fenêtre Vie de la carte.", x1 - x0, y1 - y0);
                }
            }
        }
    }

    /// Zones dans la vue : cadre et nom ; rectangle en cours de tracé.
    pub fn draw_zones_overlay(&self, painter: &egui::Painter) {
        let col = Color32::from_rgb(230, 140, 255);
        let z = |x: f32, y: f32| -> Option<Pos2> {
            let h = self.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(self.ground_z);
            self.cam.project(v3(x, y, h + 1.0)).map(|q| q.0)
        };
        let rect = |painter: &egui::Painter, a: [f32; 2], b: [f32; 2], name: Option<&str>| {
            let mut pts = Vec::new();
            let n = 12;
            for (p, q) in [((a[0], a[1]), (b[0], a[1])), ((b[0], a[1]), (b[0], b[1])), ((b[0], b[1]), (a[0], b[1])), ((a[0], b[1]), (a[0], a[1]))] {
                for k in 0..n {
                    let f = k as f32 / n as f32;
                    if let Some(s) = z(p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f) {
                        pts.push(s);
                    }
                }
            }
            if pts.len() > 2 {
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(2.0_f32, col)));
            }
            if let (Some(name), Some(c)) = (name, z((a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5)) {
                painter.text(c, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(15.0), col);
            }
        };
        for zn in &self.proj.zones {
            let c = V3 { x: (zn.min[0] + zn.max[0]) * 0.5, y: (zn.min[1] + zn.max[1]) * 0.5, z: self.ground_z };
            if (c - self.cam.eye).len() > 3000.0 {
                continue;
            }
            rect(painter, zn.min, zn.max, Some(&zn.name));
        }
        if self.tool == crate::app::Tool::Life && self.life.tool == crate::life::LifeTool::Zone {
            if let (Some(a), Some(b)) = (self.life.drag_start, self.mouse_ground) {
                rect(painter, [a.x.min(b.x), a.y.min(b.y)], [a.x.max(b.x), a.y.max(b.y)], None);
            }
        }
    }

    /// Liste et réglages des zones (fenêtre Vie de la carte).
    pub fn zones_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Zones de quartier").strong());
        ui.label(RichText::new("Glisse un rectangle dans la vue. En jeu, le nom s'affiche en bas à droite quand on entre dans la zone, la densité des piétons et des voitures y est réglée, et l'ambiance (filtre de couleur du jeu) y est appliquée.").small().weak());
        let mut zones = self.proj.zones.clone();
        let mut del = None;
        for (i, z) in zones.iter_mut().enumerate() {
            egui::Frame::new().fill(Color32::from_rgb(34, 30, 42)).corner_radius(4.0).inner_margin(4.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut z.name).desired_width(170.0));
                    let here = if z.contains(self.cam.target.x, self.cam.target.y) { " · ici" } else { "" };
                    ui.label(RichText::new(format!("{:.0} × {:.0} m{here}", z.max[0] - z.min[0], z.max[1] - z.min[1])).small().weak());
                    if ui.small_button("Aller").clicked() {
                        self.cam.target = v3((z.min[0] + z.max[0]) * 0.5, (z.min[1] + z.max[1]) * 0.5, self.cam.target.z);
                    }
                    if ui.small_button("Supprimer").clicked() {
                        del = Some(i);
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut z.peds, 0.0..=2.0).text("piétons").max_decimals(1));
                    ui.add(egui::Slider::new(&mut z.cars, 0.0..=2.0).text("voitures").max_decimals(1));
                });
                ui.horizontal(|ui| {
                    let cur = MOODS.iter().find(|m| m.0 == z.mood).map(|m| m.1).unwrap_or(&z.mood).to_string();
                    egui::ComboBox::from_id_salt(("zone_mood", i)).width(150.0).selected_text(cur).show_ui(ui, |ui| {
                        for (m, l) in MOODS {
                            ui.selectable_value(&mut z.mood, m.to_string(), l);
                        }
                    });
                    ui.add_enabled(!z.mood.is_empty(), egui::Slider::new(&mut z.strength, 0.1..=1.0).text("force").max_decimals(1));
                });
            });
        }
        if let Some(i) = del {
            zones.remove(i);
        }
        if zones != self.proj.zones {
            self.undo_once(ui.ctx());
            self.proj.zones = zones;
        }
    }

    /// Export : script des zones à la racine de la ressource ; rend vrai s'il y en a.
    pub fn export_zones(&self, root: &std::path::Path) -> Result<bool, String> {
        let _ = std::fs::remove_file(root.join("dw_zones.lua"));
        if self.proj.zones.is_empty() {
            return Ok(false);
        }
        std::fs::write(root.join("dw_zones.lua"), zones_lua(&self.proj.zones)).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_des_zones() {
        let z = vec![Zone { name: "L'Anse".into(), min: [0.0, 0.0], max: [100.0, 50.0], peds: 0.5, cars: 0.0, mood: "prologue".into(), strength: 0.7 }];
        let s = zones_lua(&z);
        assert!(s.contains("name = 'L\\'Anse'") && s.contains("mood = 'prologue'") && s.contains("SetTimecycleModifier"));
        assert!(z[0].contains(50.0, 25.0) && !z[0].contains(150.0, 25.0));
        // mêmes nombres d'accolades ouvrantes et fermantes (table Lua bien formée)
        assert_eq!(s.matches('{').count(), s.matches('}').count());
        let _ = std::fs::write(std::env::temp_dir().join("dw_zones_test.lua"), &s);
    }
}
