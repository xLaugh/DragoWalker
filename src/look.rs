//! Apparence de DragoWalker : thème sombre, police d'icônes (Phosphor), logo DW, boutons de barre d'outils
//! (icône + texte, groupes encadrés, bouton d'accent orange).

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};
pub use egui_phosphor::regular as ic;

/// Orange de DragoWalker (sélection, accents) et sa version claire.
pub const ACCENT: Color32 = Color32::from_rgb(232, 118, 34);
pub const ACCENT_LIGHT: Color32 = Color32::from_rgb(255, 160, 70);
/// Fond des groupes de la barre d'outils.
const GROUP_FILL: Color32 = Color32::from_rgb(45, 49, 57);

/// Thème sombre et polices (texte d'egui + icônes Phosphor mêlées au texte).
pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
    // toujours le thème sombre, même si Windows est en mode clair
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(34, 37, 43);
    v.window_fill = Color32::from_rgb(36, 39, 46);
    v.extreme_bg_color = Color32::from_rgb(22, 24, 28);
    v.faint_bg_color = Color32::from_rgb(40, 44, 51);
    v.selection.bg_fill = Color32::from_rgb(214, 110, 30);
    v.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(255, 214, 170));
    v.hyperlink_color = ACCENT_LIGHT;
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(62, 67, 78));
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(56, 60, 70));
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(5);
    }
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(50, 54, 63);
    v.widgets.inactive.bg_fill = Color32::from_rgb(50, 54, 63);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(68, 74, 86);
    v.widgets.hovered.bg_fill = Color32::from_rgb(68, 74, 86);
    v.widgets.active.bg_fill = Color32::from_rgb(214, 110, 30);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(214, 110, 30);
    ctx.set_visuals(v);
    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = egui::vec2(6.0, 5.0);
    s.spacing.button_padding = egui::vec2(7.0, 3.0);
    s.spacing.menu_margin = egui::Margin::same(6);
    ctx.set_style(s);
}

/// Texture du logo DW (chargée une fois).
pub fn logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("dw_logo_tex");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    let img = image::load_from_memory(include_bytes!("../assets/logo_64.png")).map(|i| i.to_rgba8()).ok();
    let ci = match img {
        Some(i) => {
            let (w, h) = i.dimensions();
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], i.as_raw())
        }
        None => egui::ColorImage::new([1, 1], Color32::from_rgb(232, 118, 34)),
    };
    let t = ctx.load_texture("dw_logo", ci, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// Logo DW à la taille voulue.
pub fn logo(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let t = logo_texture(ui.ctx());
    ui.add(egui::Image::new((t.id(), egui::vec2(size, size))))
}

/// Texte « icône  libellé ».
pub fn label(icon: &str, text: &str) -> String {
    if text.is_empty() {
        icon.to_string()
    } else {
        format!("{icon}  {text}")
    }
}

/// Bouton d'outil qu'on allume ou éteint (icône + texte, ou icône seule si `text` est vide).
pub fn tool(ui: &mut egui::Ui, on: bool, icon: &str, text: &str, tip: &str) -> egui::Response {
    let r = ui.selectable_label(on, RichText::new(label(icon, text)).size(14.5));
    if tip.is_empty() {
        r
    } else {
        r.on_hover_text(tip)
    }
}

/// Bouton d'action (icône + texte facultatif), grisé si `enabled` est faux.
pub fn action(ui: &mut egui::Ui, enabled: bool, icon: &str, text: &str, tip: &str) -> egui::Response {
    let r = ui.add_enabled(enabled, egui::Button::new(RichText::new(label(icon, text)).size(14.5)).frame(false));
    if tip.is_empty() {
        r
    } else {
        r.on_hover_text(tip)
    }
}

/// Bouton plein, orange : l'action principale d'une barre (Tester en jeu…).
pub fn accent(ui: &mut egui::Ui, icon: &str, text: &str, tip: &str) -> egui::Response {
    let b = egui::Button::new(RichText::new(label(icon, text)).strong().color(Color32::WHITE)).fill(ACCENT).corner_radius(CornerRadius::same(6));
    ui.add(b).on_hover_text(tip)
}

/// Groupe encadré de la barre d'outils.
pub fn group<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(GROUP_FILL)
        .corner_radius(CornerRadius::same(7))
        .inner_margin(egui::Margin::symmetric(5, 2))
        .show(ui, |ui| ui.horizontal(|ui| add(ui)).inner)
        .inner
}

/// Élément de menu avec icône et raccourci ; vrai si cliqué.
pub fn menu_item(ui: &mut egui::Ui, enabled: bool, icon: &str, text: &str, keys: &str) -> bool {
    ui.add_enabled(enabled, egui::Button::new(label(icon, text)).shortcut_text(keys)).clicked()
}

/// Titre de section dans un panneau (icône orange + texte fort).
pub fn heading(ui: &mut egui::Ui, icon: &str, text: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon).size(17.0).color(ACCENT_LIGHT));
        ui.label(RichText::new(text).strong().size(15.0));
    });
}
