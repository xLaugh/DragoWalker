//! Pont avec le CodeWalker installé sur ce PC : conversion XML ↔ fichiers binaires du jeu
//! (XmlMeta.GetData 0x06003faf → ResourceBuilder.Build(meta, 2, compressé) ; MetaXml.GetXml 0x06003d6a).
//! DragoWalker ne réécrit pas ce format : il demande à CodeWalker de le faire, exactement comme lui.

use std::path::{Path, PathBuf};

const SCRIPT: &str = include_str!("../tools/cw_convert.ps1");

/// CodeWalker.Core.dll (dossier réglé dans l'onglet Jeu, sinon recherche automatique).
pub fn dll(hint: &str) -> Option<PathBuf> {
    crate::gta::keys::find_codewalker_dll(hint)
}

fn script_path() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("dragowalker_cw");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join("cw_convert.ps1");
    // le script est écrit avec un BOM UTF-8 pour que PowerShell 5 lise bien les accents
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(SCRIPT.as_bytes());
    std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
    Ok(p)
}

fn run(args: &[&str]) -> Result<Vec<(bool, String, String)>, String> {
    let script = script_path()?;
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&script).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("PowerShell introuvable : {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut res = Vec::new();
    for line in text.lines() {
        let p: Vec<&str> = line.split('\t').collect();
        if p.len() >= 2 {
            res.push((p[0] == "OK", p[1].to_string(), p.get(2).unwrap_or(&"").to_string()));
        }
    }
    if res.is_empty() && !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).chars().take(400).collect());
    }
    Ok(res)
}

/// Convertit tous les .xml d'un dossier en fichiers binaires du jeu. Rend (réussis, erreurs).
pub fn to_binary(dll: &Path, dir: &Path) -> Result<(usize, Vec<String>), String> {
    let r = run(&["-Dll", &dll.to_string_lossy(), "-ToBinary", &dir.to_string_lossy()])?;
    let ok = r.iter().filter(|x| x.0).count();
    let errs = r.iter().filter(|x| !x.0).map(|x| format!("{} : {}", x.1, x.2)).collect();
    Ok((ok, errs))
}

/// Convertit des fichiers binaires du jeu en XML (CodeWalker), en une seule fois.
pub fn to_xml(dll: &Path, pairs: &[(PathBuf, PathBuf)]) -> Result<usize, String> {
    // déjà converti et à jour (XML plus récent que le fichier d'origine) : pas relu — chaque lancement de
    // CodeWalker par PowerShell coûte plusieurs secondes et bloquait DragoWalker à chaque test en jeu
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let todo: Vec<&(PathBuf, PathBuf)> = pairs.iter().filter(|(a, b)| !matches!((mtime(a), mtime(b)), (Some(ta), Some(tb)) if tb >= ta)).collect();
    if todo.is_empty() {
        return Ok(pairs.len());
    }
    let list = std::env::temp_dir().join("dragowalker_cw").join("liste.txt");
    let txt: Vec<String> = todo.iter().map(|(a, b)| format!("{}|{}", a.display(), b.display())).collect();
    std::fs::write(&list, txt.join("\n")).map_err(|e| e.to_string())?;
    let r = run(&["-Dll", &dll.to_string_lossy(), "-ToXmlList", &list.to_string_lossy()])?;
    Ok(r.iter().filter(|x| x.0).count())
}
