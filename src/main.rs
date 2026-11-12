#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod analyse;
mod app;
mod buildings;
mod camera;
mod convert;
mod catalog;
mod cwbridge;
mod docking;
mod export;
mod game_ui;
mod gizmo;
mod ground;
mod gpu;
mod groundtex;
mod mlo;
mod bedit;
mod grass;
mod check;
mod shell;
mod lights;
mod groups;
mod occlusion;
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
mod server;
mod signals;
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
    if args.len() >= 3 && args[1] == "--gta-probe" {
        gta::probe(&args[2], args.get(3).map(|s| s.as_str()).unwrap_or(""));
        return Ok(());
    }
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
