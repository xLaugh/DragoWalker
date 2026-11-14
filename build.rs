//! Icône de l'exécutable (logo DW) et nom affiché dans les propriétés du fichier.
fn main() {
    println!("cargo:rerun-if-changed=assets/dragowalker.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/dragowalker.ico");
        res.set("FileDescription", "DragoWalker");
        res.set("ProductName", "DragoWalker");
        // une icône manquante ne doit pas empêcher de compiler
        if let Err(e) = res.compile() {
            println!("cargo:warning=icône de l'exe non ajoutée : {e}");
        }
    }
}
