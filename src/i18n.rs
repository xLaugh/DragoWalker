//! Langue de l'interface : français (par défaut) ou anglais.
//!
//! Chaque texte fixe de l'interface passe par `t("texte français")` : en anglais, la table ci-dessous donne la
//! traduction ; un texte absent de la table reste en français. Couvert aujourd'hui : les menus, la barre
//! d'outils et les fenêtres de sauvegarde, de récupération et de mise à jour. Le reste de l'interface (panneaux,
//! messages d'état qui contiennent des valeurs) est encore en français.
//!
//! Compléter sans recompiler : un fichier `%APPDATA%\DragoWalker\lang\en.json` de la forme
//! `{"Texte français": "English text"}` s'ajoute à la table (et la remplace pour les mêmes textes).
//! Réglage : menu Affichage » Langue, enregistré sous la clé `lang` des réglages.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Table française → anglaise (générée à partir de la liste des textes enveloppés par `t`).
const BUILTIN: &[(&str, &str)] = &[
    ("Fichier", "File"),
    ("Nouveau projet", "New project"),
    ("Ouvrir un projet…", "Open a project…"),
    ("Enregistrer le projet", "Save project"),
    ("Enregistrer sous…", "Save as…"),
    ("Ctrl+Maj+S", "Ctrl+Shift+S"),
    ("Importer ymap / ytyp (XML CodeWalker)…", "Import ymap / ytyp (CodeWalker XML)…"),
    ("Importer une ressource MLO (dossier FiveM)…", "Import an MLO resource (FiveM folder)…"),
    ("Exporter en XML…", "Export as XML…"),
    ("Exporter une ressource FiveM…", "Export a FiveM resource…"),
    ("Ctrl+Maj+E", "Ctrl+Shift+E"),
    ("Contrôle avant l'export…", "Pre-export check…"),
    ("Analyser un plantage FiveM…", "Analyse a FiveM crash…"),
    ("Tester en jeu (serveur local)", "Test in game (local server)"),
    ("Quitter", "Quit"),
    ("Édition", "Edit"),
    ("Annuler", "Undo"),
    ("Rétablir", "Redo"),
    ("Couper", "Cut"),
    ("Copier", "Copy"),
    ("Coller", "Paste"),
    ("Dupliquer la sélection", "Duplicate selection"),
    ("Revenir à tout voir", "Show everything again"),
    ("Isoler la sélection", "Isolate selection"),
    ("Poser au sol", "Snap to ground"),
    ("Maj / Fin", "Shift / End"),
    ("Supprimer la sélection", "Delete selection"),
    ("Suppr", "Del"),
    ("Sélection intelligente", "Smart selection"),
    ("Tout désélectionner", "Deselect all"),
    ("Échap", "Esc"),
    ("Outils", "Tools"),
    ("Sélection", "Selection"),
    ("Poser", "Place"),
    ("Gomme", "Eraser"),
    ("Poignées : déplacer", "Handles: move"),
    ("Poignées : tourner", "Handles: rotate"),
    ("Poignées : agrandir", "Handles: scale"),
    ("Bibliothèque de props", "Prop library"),
    ("Bibliothèque des intérieurs", "Interior library"),
    ("Bibliothèque des bâtiments", "Building library"),
    ("Couper les bâtiments (vue en coupe)", "Cut buildings (section view)"),
    ("H : cacher l'objet du jeu sous la souris", "H: hide the game object under the mouse"),
    ("Créer / modèles de départ", "Create / starter templates"),
    ("Monter", "Raise"),
    ("Creuser", "Lower"),
    ("Pelleteuse", "Flatten"),
    ("Lisser", "Smooth"),
    ("Relief naturel", "Natural relief"),
    ("Peindre", "Paint"),
    ("Route", "Road"),
    ("Végétation", "Vegetation"),
    ("Eau", "Water"),
    ("Effets (particules)", "Effects (particles)"),
    ("Construire la route en cours", "Build the current road"),
    ("Entrée", "Enter"),
    ("Enlever le dernier point", "Remove the last point"),
    ("Retour arrière", "Backspace"),
    ("Rayon du pinceau : + / -  ·  Végétation : Ctrl + pinceau pour retirer", "Brush radius: + / -  ·  Vegetation: Ctrl + brush to remove"),
    ("Affichage", "View"),
    ("Vue 3D", "3D view"),
    ("Vue carte (de dessus)", "Map view (top-down)"),
    ("Afficher les panneaux", "Show panels"),
    ("Cacher les panneaux", "Hide panels"),
    ("Panneau Projet", "Project panel"),
    ("Boîte à outils", "Toolbox"),
    ("Pour déplacer un panneau : attrape son titre et lâche-le contre un bord (rangé) ou ailleurs (fenêtre flottante)", "To move a panel: grab its title and drop it against an edge (docked) or elsewhere (floating window)"),
    ("Grille", "Grid"),
    ("Pièces et portails des intérieurs", "Interior rooms and portals"),
    ("Carte d'exemple", "Sample map"),
    ("Compteur de performance", "Performance counter"),
    ("Images par seconde, objets et triangles dessinés, mémoire graphique, historique d'annulation", "Frames per second, objects and triangles drawn, graphics memory, undo history"),
    ("Aide", "Help"),
    ("Clic droit + glisser : tourner la vue (vue carte : déplacer)", "Right click + drag: rotate the view (map view: pan)"),
    ("Clic molette + glisser : déplacer la vue", "Middle click + drag: pan the view"),
    ("ZQSD / WASD : avancer · Maj : ×5 · Ctrl : ×0.2", "ZQSD / WASD: move · Shift: ×5 · Ctrl: ×0.2"),
    ("Page haut / bas : monter / descendre", "Page up / down: go up / down"),
    ("1 Sélection · 2 Poser · 3 Gomme · 4 Rectangle · 5 Terrain · M vue carte", "1 Selection · 2 Place · 3 Eraser · 4 Rectangle · 5 Terrain · M map view"),
    ("G / R / T : poignées · Flèches : bouger la sélection · [ ] : tourner de 15°", "G / R / T: handles · Arrows: move the selection · [ ]: rotate by 15°"),
    ("Ctrl+C / Ctrl+V / Ctrl+X : copier, coller (devant la caméra, comme CodeWalker), couper", "Ctrl+C / Ctrl+V / Ctrl+X: copy, paste (in front of the camera, like CodeWalker), cut"),
    ("Suppr : supprimer · Ctrl+D : dupliquer · Ctrl+Z / Ctrl+Y · Ctrl+U : panneaux", "Del: delete · Ctrl+D: duplicate · Ctrl+Z / Ctrl+Y · Ctrl+U: panels"),
    ("Terrain : F2–F9 pinceaux · + / - rayon · Entrée : construire la route", "Terrain: F2–F9 brushes · + / - radius · Enter: build the road"),
    ("Assistant IA", "AI assistant"),
    ("Fenêtre de l'assistant : décris ce que tu veux construire", "Assistant window: describe what you want to build"),
    ("Tester en jeu", "Test in game"),
    ("Serveur FiveM local géré par DragoWalker : tester la map et les MLO en jeu, recharger sans se déconnecter", "Local FiveM server managed by DragoWalker: test the map and MLOs in game, reload without disconnecting"),
    ("Vie (PNJ, véhicules)", "Life (NPCs, vehicles)"),
    ("Véhicules garés, parkings, PNJ avec activité ou ronde, circulation sur tes routes", "Parked vehicles, car parks, NPCs with an activity or patrol, traffic on your roads"),
    ("Ouvrir / fermer la boîte à outils", "Open / close the toolbox"),
    ("Projet", "Project"),
    ("Ouvrir / fermer le panneau Projet", "Open / close the Project panel"),
    ("Créateur de terrain", "Terrain creator"),
    ("Fenêtre du créateur de terrain : relief, peinture, routes, végétation, eau, effets (5)", "Terrain creator window: relief, painting, roads, vegetation, water, effects (5)"),
    ("Tout est de nouveau affiché.", "Everything is shown again."),
    ("Choisis d'abord un ou plusieurs objets à isoler.", "First choose one or more objects to isolate."),
    ("Tout revoir (Échap)", "Show everything (Esc)"),
    ("Visible jusqu'à", "Visible up to"),
    ("Proche", "Near"),
    ("100 m : petits objets, détails", "100 m: small objects, details"),
    ("Loin", "Far"),
    ("1 000 m : grands bâtiments", "1,000 m: large buildings"),
    ("Toujours", "Always"),
    ("3 000 m : repères visibles de partout", "3,000 m: landmarks visible from everywhere"),
    ("Comme le jeu", "Like the game"),
    ("La distance de ce type d'objet dans GTA V ; les bâtiments du jeu copiés emportent en plus leur version lointaine à l'export", "This object type's distance in GTA V; copied game buildings also bring their distant version on export"),
    ("Toujours chargé", "Always loaded"),
    ("Le jeu charge toujours ces objets (PRI_REQUIRED) ; sinon il peut les sauter selon ses réglages graphiques (PRI_OPTIONAL_HIGH)", "The game always loads these objects (PRI_REQUIRED); otherwise it may skip them depending on its graphics settings (PRI_OPTIONAL_HIGH)"),
    ("Nouveau projet.", "New project."),
    ("Projet DragoWalker", "DragoWalker project"),
    ("🖱 Sélection", "🖱 Selection"),
    ("Clic : choisir · glisser dans le vide : rectangle · glisser un objet : le déplacer (1)", "Click: choose · drag on empty space: rectangle · drag an object: move it (1)"),
    ("➕ Poser", "➕ Place"),
    ("Clic : poser le prop choisi · glisser : en poser le long du trait (2)", "Click: place the chosen prop · drag: place several along the line (2)"),
    ("⭕ Gomme", "⭕ Eraser"),
    ("Glisser : supprimer tout ce qui passe sous le cercle (3)", "Drag: delete everything under the circle (3)"),
    ("Pinceaux de terrain : monter, creuser, pelleteuse, lisser, peindre, routes (5)", "Terrain brushes: raise, lower, flatten, smooth, paint, roads (5)"),
    ("Glisser n'importe où : sélectionner tout ce qui est dans le rectangle, objets du jeu compris · Ctrl ou Maj : ajouter à la sélection (4) · avec l'outil Sélection : Alt + glisser", "Drag anywhere: select everything inside the rectangle, game objects included · Ctrl or Shift: add to the selection (4) · with the Selection tool: Alt + drag"),
    ("Intérieurs", "Interiors"),
    ("Bibliothèque des intérieurs du jeu : plan, « Aller voir », « Voir isolé »", "Library of the game's interiors: floor plan, “Go and see”, “View isolated”"),
    ("Bâtiments", "Buildings"),
    ("Bâtiments du jeu : les copier pour les poser ailleurs, sur une carte custom (N)", "Game buildings: copy them to place elsewhere, on a custom map (N)"),
    ("Groupes", "Groups"),
    ("Tes groupes d'objets : les poser à la souris (molette = tourner, Maj + molette = quart de tour)", "Your object groups: place them with the mouse (wheel = rotate, Shift + wheel = quarter turn)"),
    ("Bibliothèque", "Library"),
    ("Tous les props du jeu en images, avec aperçu 3D ; clic pour choisir, double-clic pour poser, glisser sur la carte pour poser à cet endroit (B)", "All the game's props as pictures, with a 3D preview; click to choose, double-click to place, drag onto the map to place there (B)"),
    ("Annuler (Ctrl+Z)", "Undo (Ctrl+Z)"),
    ("Rétablir (Ctrl+Y)", "Redo (Ctrl+Y)"),
    ("🗑 Supprimer", "🗑 Delete"),
    ("Supprimer toute la sélection d'un coup", "Delete the whole selection at once"),
    ("📋 Dupliquer", "📋 Duplicate"),
    ("Déplacer", "Move"),
    ("Flèches pour déplacer la sélection le long d'un axe (G)", "Arrows to move the selection along an axis (G)"),
    ("Tourner", "Rotate"),
    ("Anneaux pour tourner la sélection, par pas de 5° avec la grille (R ou E)", "Rings to rotate the selection, in 5° steps with the grid (R or E)"),
    ("Agrandir", "Scale"),
    ("Carrés pour changer la taille ; carré du centre = partout (T)", "Squares to change the size; centre square = everywhere (T)"),
    ("Axes de l'objet", "Object axes"),
    ("Poignées alignées sur l'objet choisi au lieu du nord / de l'est (comme « Object space » dans CodeWalker)", "Handles aligned to the chosen object instead of north / east (like “Object space” in CodeWalker)"),
    ("🗺 Vue carte", "🗺 Map view"),
    ("🎥 Vue 3D", "🎥 3D view"),
    ("Changer de vue (M)", "Switch view (M)"),
    ("Aligner les positions sur une grille", "Snap positions to a grid"),
    ("Carte de destination des nouveaux objets", "Destination map for new objects"),
    ("Une erreur interne a été enregistrée. Copiez ce rapport et envoyez-le pour qu'elle soit corrigée (les chemins personnels en ont été retirés).", "An internal error was recorded. Copy this report and send it so it can be fixed (personal paths have been removed)."),
    ("La session précédente ne s'est pas fermée normalement (fermeture forcée, coupure, arrêt brutal). Aucune erreur interne n'a été enregistrée.", "The previous session did not close normally (forced close, power cut, abrupt stop). No internal error was recorded."),
    ("Copier le rapport", "Copy the report"),
    ("Ouvrir le dossier", "Open the folder"),
    ("Signaler sur GitHub", "Report on GitHub"),
    ("Fermer", "Close"),
    ("Le récupérer ouvre une copie : enregistrez-la sous le nom voulu.", "Recovering it opens a copy: save it under the name you want."),
    ("Récupérer", "Recover"),
    ("Supprimer", "Delete"),
    ("La sauvegarde automatique est illisible.", "The automatic backup is unreadable."),
    ("Enregistrer et quitter", "Save and quit"),
    ("Quitter sans enregistrer", "Quit without saving"),
    ("Mettre à jour maintenant", "Update now"),
    ("Télécharge la nouvelle version, l'installe et redémarre DragoWalker. Enregistrez votre projet avant.", "Downloads the new version, installs it and restarts DragoWalker. Save your project first."),
    ("Voir les nouveautés", "See what's new"),
    ("Plus tard", "Later"),
    ("Réessayer", "Retry"),
];

pub fn is_english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn set_english(on: bool) {
    ENGLISH.store(on, Ordering::Relaxed);
}

/// Lit le réglage de langue au démarrage (« lang » : "en" ou "fr" ; sans réglage, l'anglais si Windows est en anglais).
pub fn init() {
    let s = crate::app::load_settings();
    let forced = std::env::var("DW_LANG").ok(); // test : DW_LANG=en
    let en = match forced.as_deref().or(s["lang"].as_str()) {
        Some(l) => l.eq_ignore_ascii_case("en"),
        None => ["LANG", "LC_ALL"].iter().filter_map(|k| std::env::var(k).ok()).any(|v| v.to_lowercase().starts_with("en")),
    };
    set_english(en);
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| {
        let mut m: HashMap<&'static str, &'static str> = BUILTIN.iter().copied().collect();
        // fichier de la communauté : les textes sont gardés pour toute la durée du programme
        let path = std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("DragoWalker").join("lang").join("en.json"));
        if let Some(Ok(text)) = path.map(std::fs::read_to_string) {
            if let Ok(serde_json::Value::Object(o)) = serde_json::from_str::<serde_json::Value>(&text) {
                for (k, v) in o {
                    if let Some(v) = v.as_str() {
                        m.insert(Box::leak(k.into_boxed_str()), Box::leak(v.to_string().into_boxed_str()));
                    }
                }
            }
        }
        m
    })
}

/// Le texte dans la langue choisie.
pub fn t(fr: &'static str) -> &'static str {
    if !is_english() {
        return fr;
    }
    table().get(fr).copied().unwrap_or(fr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_sans_doublon_ni_vide() {
        let mut seen = std::collections::HashSet::new();
        for (fr, en) in BUILTIN {
            assert!(!fr.is_empty() && !en.is_empty(), "entrée vide : {fr:?}");
            assert!(seen.insert(*fr), "doublon : {fr:?}");
            assert_ne!(fr, en, "traduction identique : {fr:?}");
        }
        assert!(BUILTIN.len() > 150);
    }

    #[test]
    fn francais_par_defaut_et_repli() {
        // ce test change l'état global : il est le seul à le faire
        set_english(false);
        assert_eq!(t("Fichier"), "Fichier");
        set_english(true);
        assert_eq!(t("Fichier"), "File");
        assert_eq!(t("Ctrl+Maj+S"), "Ctrl+Shift+S");
        assert_eq!(t("Texte jamais traduit"), "Texte jamais traduit", "texte absent : reste en français");
        set_english(false);
    }
}
