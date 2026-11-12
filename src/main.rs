#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod analyse;
mod app;
mod assistant;
mod buildings;
mod camera;
mod convert;
mod crashlog;
mod catalog;
mod cwbridge;
mod docking;
mod edit_ops;
mod export;
mod files;
mod frame;
mod i18n;
mod game_ui;
mod gizmo;
mod ground;
mod gpu;
mod groundtex;
mod mlo;
mod bedit;
#[allow(dead_code)]
mod grass;
mod check;
mod shell;
mod lights;
mod groups;
mod occlusion;
mod openings;
mod intlib;
mod library;
mod life;
mod lists;
mod lods;
mod roads;
mod terrain;
mod vegetation;
mod gta;
mod math;
mod mech;
mod model;
mod render;
mod sample;
mod secret;
mod server;
mod signals;
mod safety;
mod scenery;
mod sollumz;
mod texdecode;
mod traffic;
mod update;
mod sollumz_ui;
mod ui;
mod view;
mod xmlio;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--read-ydr" {
        for f in &args[2..] {
            match sollumz::read_ydr_binary(std::path::Path::new(f), 0, &std::collections::HashMap::new()) {
                Ok(m) => println!("{f} : {} sommets, {} triangles, {} textures, boîte {:?}..{:?}, textures cherchées {:?}, attendu {:08x}", m.model.verts.len(), m.tris, m.textures.len(), m.bb_min, m.bb_max, m.model.geoms.iter().map(|g| format!("{:08x}", g.tex)).collect::<Vec<_>>(), gta::crypto::joaat("prop_bench_01a_t6ba514ac")),
                Err(e) => println!("{f} : {e}"),
            }
        }
        return Ok(());
    }
    if args.len() >= 3 && args[1] == "--read-ymap" {
        for f in &args[2..] {
            gta::read_ymap_file(f);
        }
        return Ok(());
    }
    if args.len() >= 3 && args[1] == "--analyse-crash" {
        match crashlog::analyse_folder(std::path::Path::new(&args[2])) {
            Ok(a) => print!("{}", crashlog::summary(&a)),
            Err(e) => println!("{e}"),
        }
        return Ok(());
    }
    if args.len() >= 12 && args[1] == "--open-ydr-xml" {
        // dragowalker --open-ydr-xml <entrée.ydr.xml> <sortie.ydr.xml> x y z dirx diry largeur hauteur profondeur [appui]
        let f = |i: usize| args[i].parse::<f32>().unwrap_or(0.0);
        let o = openings::Opening { pos: [f(4), f(5), f(6)], dir: [f(7), f(8)], w: f(9), h: f(10), depth: f(11), kind: "test".into(), sill: args.get(12).is_some(), prop: String::new() };
        match std::fs::read_to_string(&args[2]).ok().and_then(|s| openings::apply_to_ydr_xml(&s, &[o])) {
            Some((x, n)) => {
                let _ = std::fs::write(&args[3], x);
                println!("{n} triangle(s) découpé(s), écrit dans {}", args[3]);
            }
            None => println!("rien à faire ou fichier illisible"),
        }
        return Ok(());
    }
    if args.len() >= 6 && args[1] == "--ground-probe" {
        let f = |i: usize| args[i].parse::<f32>().unwrap_or(0.0);
        gta::ground_probe(&args[2], f(3), f(4), f(5));
        return Ok(());
    }
    if args.len() >= 5 && args[1] == "--dump-ydr-xml" {
        // dragowalker --dump-ydr-xml <dossier du jeu> <modèle> <dossier de sortie> : le .ydr du jeu lu en XML par CodeWalker
        gta::dump_ydr_xml(&args[2], &args[3], &args[4]);
        return Ok(());
    }
    if args.len() >= 4 && args[1] == "--gta-region" {
        gta::region_probe(&args[2], &args[3..]);
        return Ok(());
    }
    if args.len() >= 3 && args[1] == "--gta-probe" {
        gta::probe(&args[2], args.get(3).map(|s| s.as_str()).unwrap_or(""));
        return Ok(());
    }
    safety::install_panic_hook();
    i18n::init();
    groundtex::init_setting();
    let mut screenshot = None;
    let mut open_files = Vec::new();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" && i + 1 < args.len() {
            screenshot = Some(std::path::PathBuf::from(&args[i + 1]));
            i += 2;
            continue;
        }
        open_files.push(std::path::PathBuf::from(&args[i]));
        i += 1;
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("DragoWalker — éditeur de cartes GTA V")
            .with_inner_size([1600.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        // profondeur pour les vrais modèles 3D
        depth_buffer: 24,
        ..Default::default()
    };
    eframe::run_native(
        "DragoWalker",
        options,
        Box::new(move |cc| Ok(Box::new(app::DwApp::new(cc, screenshot, open_files)))),
    )
}
