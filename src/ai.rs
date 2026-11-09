//! Assistant IA : transforme une phrase en actions sur la carte.
//! - Sans clé : un interpréteur local comprend les commandes françaises courantes.
//! - Avec une clé API Anthropic : Claude choisit les actions (outil `executer_actions`).

use crate::catalog;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::mpsc;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Poser des props. pattern : point | ligne | cercle | grille | aleatoire
    Place {
        archetype: String,
        #[serde(default = "one")]
        count: u32,
        #[serde(default = "pattern_point")]
        pattern: String,
        /// espacement (ligne/grille) ou rayon (cercle/aléatoire), en mètres
        #[serde(default = "ten")]
        size: f32,
    },
    /// Supprimer tout dans un rayon autour du centre de la vue (filtre optionnel sur le nom).
    DeleteRadius {
        radius: f32,
        #[serde(default)]
        filter: String,
    },
    DeleteSelection,
    /// Sélectionner (filtre sur le nom, rayon 0 = partout).
    Select {
        #[serde(default)]
        filter: String,
        #[serde(default)]
        radius: f32,
    },
    ClearSelection,
    MoveSelection {
        #[serde(default)]
        dx: f32,
        #[serde(default)]
        dy: f32,
        #[serde(default)]
        dz: f32,
    },
    RotateSelection { degrees: f32 },
    ScaleSelection { factor: f32 },
    DuplicateSelection {
        #[serde(default)]
        dx: f32,
        #[serde(default)]
        dy: f32,
    },
    /// Créer un intérieur (MLO) en grille de pièces au centre de la vue.
    CreateMlo {
        #[serde(default)]
        name: String,
        #[serde(default = "one")]
        cols: u32,
        #[serde(default = "one")]
        rows: u32,
        #[serde(default = "six")]
        width: f32,
        #[serde(default = "six")]
        depth: f32,
        #[serde(default = "three")]
        height: f32,
    },
    /// Meubler l'intérieur courant : salon | bureau | bar | magasin | cuisine
    FurnishMlo { style: String },
    Goto { x: f32, y: f32 },
    Undo,
}

fn one() -> u32 {
    1
}
fn ten() -> f32 {
    10.0
}
fn six() -> f32 {
    6.0
}
fn three() -> f32 {
    3.0
}
fn pattern_point() -> String {
    "point".into()
}

#[derive(Clone)]
pub struct ChatLine {
    pub from_user: bool,
    pub text: String,
}

pub struct AiReply {
    pub message: String,
    pub actions: Vec<Action>,
}

fn strip_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'œ' => 'o',
            _ => c,
        })
        .collect()
}

fn numbers(s: &str) -> Vec<f32> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() || ((c == '.' || c == ',') && !cur.is_empty()) || (c == '-' && cur.is_empty()) {
            cur.push(if c == ',' { '.' } else { c });
        } else {
            if let Ok(v) = cur.trim_end_matches('.').parse::<f32>() {
                out.push(v);
            }
            cur.clear();
        }
    }
    out
}

fn number_after(s: &str, keys: &[&str]) -> Option<f32> {
    for k in keys {
        if let Some(i) = s.find(k) {
            if let Some(v) = numbers(&s[i + k.len()..]).first() {
                return Some(*v);
            }
        }
    }
    None
}

fn word_count(s: &str) -> Option<u32> {
    let words = [("un ", 1), ("une ", 1), ("deux", 2), ("trois", 3), ("quatre", 4), ("cinq", 5), ("six", 6), ("sept", 7), ("huit", 8), ("neuf", 9), ("dix", 10), ("vingt", 20), ("cent", 100)];
    for (w, n) in words {
        if s.contains(w) {
            return Some(n);
        }
    }
    None
}

/// Trouve un prop cité dans la phrase.
fn find_prop(s: &str) -> Option<&'static catalog::Prop> {
    for w in s.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if w.len() < 3 {
            continue;
        }
        if w.starts_with("prop_") || w.starts_with("v_") {
            if let Some(p) = catalog::find(w) {
                return Some(p);
            }
        }
        if let Some(p) = catalog::guess(w) {
            return Some(p);
        }
    }
    None
}

fn filter_word(s: &str) -> String {
    let n = strip_accents(s);
    if n.contains("batiment") || n.contains("immeuble") || n.contains("maison") || n.contains("building") {
        return "dw_batiment".into();
    }
    if n.contains("arbre") || n.contains("vegetation") || n.contains("foret") {
        return "tree".into();
    }
    if n.contains("palmier") {
        return "palm".into();
    }
    if let Some(p) = find_prop(s) {
        return p.name.to_string();
    }
    String::new()
}

/// Interpréteur local (sans Internet).
pub fn local_parse(input: &str) -> AiReply {
    let s = input.to_lowercase();
    let n = strip_accents(&s);
    let nums = numbers(&n);
    let mut actions = Vec::new();
    let msg;

    if n.contains("annule") {
        actions.push(Action::Undo);
        msg = "J'annule la dernière action.".to_string();
    } else if (n.contains("interieur") || n.contains("mlo") || n.contains("appartement") || n.contains("piece") || n.contains("maison"))
        && (n.contains("cree") || n.contains("fais") || n.contains("fait") || n.contains("construi") || n.contains("genere") || n.contains("nouve"))
    {
        let rooms = number_after(&n, &[""]).map(|v| v as u32).or_else(|| word_count(&n)).unwrap_or(2).clamp(1, 16);
        let cols = (rooms as f32).sqrt().ceil() as u32;
        let rows = rooms.div_ceil(cols);
        let width = number_after(&n, &["largeur", "de large"]).unwrap_or(6.0);
        let height = number_after(&n, &["hauteur", "de haut"]).unwrap_or(3.0);
        actions.push(Action::CreateMlo { name: String::new(), cols, rows, width, depth: width, height });
        msg = format!("Je crée un intérieur de {} pièce(s) ({} x {}), portails et porte d'entrée compris.", cols * rows, cols, rows);
    } else if n.contains("meubl") || n.contains("decor") || n.contains("amenag") {
        let style = ["bureau", "bar", "magasin", "cuisine", "salon"].iter().find(|st| n.contains(*st)).unwrap_or(&"salon").to_string();
        actions.push(Action::FurnishMlo { style: style.clone() });
        msg = format!("Je meuble l'intérieur courant (style {style}).");
    } else if n.contains("deselection") || n.contains("vide la selection") {
        actions.push(Action::ClearSelection);
        msg = "Sélection vidée.".into();
    } else if n.contains("supprim") || n.contains("efface") || n.contains("enleve") || n.contains("retire") || n.contains("detrui") || n.contains("vire") || n.contains("rase") {
        if n.contains("selection") && !n.contains("rayon") {
            actions.push(Action::DeleteSelection);
            msg = "Je supprime la sélection.".into();
        } else {
            let radius = number_after(&n, &["rayon de", "rayon", "dans", "sur"]).or_else(|| nums.first().copied()).unwrap_or(30.0).abs();
            let filter = filter_word(&s);
            actions.push(Action::DeleteRadius { radius, filter: filter.clone() });
            msg = if filter.is_empty() {
                format!("Je supprime tout dans un rayon de {radius} m autour du centre de la vue.")
            } else {
                format!("Je supprime les « {filter} » dans un rayon de {radius} m.")
            };
        }
    } else if n.contains("selectionne") || n.contains("choisi") {
        let filter = filter_word(&s);
        let radius = number_after(&n, &["rayon de", "rayon", "dans"]).unwrap_or(0.0);
        actions.push(Action::Select { filter: filter.clone(), radius });
        msg = format!(
            "Je sélectionne {}{}.",
            if filter.is_empty() { "tout".to_string() } else { format!("les « {filter} »") },
            if radius > 0.0 { format!(" dans un rayon de {radius} m") } else { String::new() }
        );
    } else if n.contains("tourne") || n.contains("pivote") || n.contains("rotation") {
        let deg = nums.first().copied().unwrap_or(90.0);
        actions.push(Action::RotateSelection { degrees: deg });
        msg = format!("Je tourne la sélection de {deg}°.");
    } else if n.contains("monte") || n.contains("leve") || n.contains("descend") || n.contains("baisse") {
        let v = nums.first().copied().unwrap_or(1.0).abs();
        let dz = if n.contains("descend") || n.contains("baisse") { -v } else { v };
        actions.push(Action::MoveSelection { dx: 0.0, dy: 0.0, dz });
        msg = format!("Je déplace la sélection de {dz} m en hauteur.");
    } else if n.contains("deplace") || n.contains("bouge") || n.contains("decale") {
        let v = nums.first().copied().unwrap_or(5.0);
        let (dx, dy) = if n.contains("nord") || n.contains("haut") {
            (0.0, v)
        } else if n.contains("sud") || n.contains("bas") {
            (0.0, -v)
        } else if n.contains("ouest") || n.contains("gauche") {
            (-v, 0.0)
        } else {
            (v, 0.0)
        };
        actions.push(Action::MoveSelection { dx, dy, dz: 0.0 });
        msg = format!("Je déplace la sélection ({dx}, {dy}).");
    } else if n.contains("agrandi") || n.contains("plus grand") || n.contains("reduis") || n.contains("plus petit") || n.contains("echelle") {
        let f = nums.first().copied().unwrap_or(if n.contains("redui") || n.contains("petit") { 0.66 } else { 1.5 });
        actions.push(Action::ScaleSelection { factor: f });
        msg = format!("Je change la taille de la sélection (×{f}).");
    } else if n.contains("duplique") || n.contains("copie") || n.contains("clone") {
        actions.push(Action::DuplicateSelection { dx: 2.0, dy: 0.0 });
        msg = "Je duplique la sélection (décalée de 2 m).".into();
    } else if n.contains("va ") || n.contains("aller") || n.contains("teleport") || n.contains("centre sur") {
        if nums.len() >= 2 {
            actions.push(Action::Goto { x: nums[0], y: nums[1] });
            msg = format!("Je centre la vue sur ({}, {}).", nums[0], nums[1]);
        } else {
            msg = "Donne-moi des coordonnées, par exemple : « va à 120 -340 ».".into();
        }
    } else if let Some(p) = find_prop(&s) {
        let count = nums.iter().find(|v| **v >= 1.0 && v.fract() == 0.0).map(|v| *v as u32).or_else(|| word_count(&n)).unwrap_or(1).clamp(1, 500);
        let pattern = if n.contains("ligne") || n.contains("rangee") || n.contains("alignes") || n.contains("le long") {
            "ligne"
        } else if n.contains("cercle") || n.contains("rond") || n.contains("autour") {
            "cercle"
        } else if n.contains("grille") || n.contains("quadrill") {
            "grille"
        } else if n.contains("aleatoire") || n.contains("partout") || n.contains("eparpill") || n.contains("foret") || n.contains("disperse") || count > 1 {
            if n.contains("ligne") {
                "ligne"
            } else {
                "aleatoire"
            }
        } else {
            "point"
        };
        let size = number_after(&n, &["tous les", "espace de", "espaces de", "rayon de", "rayon", "ecart de"]).unwrap_or(match pattern {
            "cercle" => 15.0,
            "aleatoire" => 25.0,
            _ => (p.size[0].max(p.size[1]) * 1.6).max(2.0),
        });
        actions.push(Action::Place { archetype: p.name.to_string(), count, pattern: pattern.to_string(), size });
        msg = format!("Je pose {count} × {} ({}) en {pattern}.", p.label, p.name);
    } else {
        msg = "Je n'ai pas compris. Exemples : « pose 10 arbres en ligne », « supprime tout dans un rayon de 40 m », « supprime les arbres dans un rayon de 80 m », « crée un intérieur de 4 pièces », « meuble en bureau », « tourne de 45° », « sélectionne les bancs ». Avec une clé API (onglet Options), Claude comprend des demandes plus libres.".into();
    }
    AiReply { message: msg, actions }
}

const MODEL: &str = "claude-opus-5-5";

fn tool_schema() -> Value {
    json!({
        "name": "executer_actions",
        "description": "Exécute une liste d'actions d'édition sur la carte GTA V ouverte dans DragoWalker. Les positions sont relatives au centre de la vue (cible de la caméra), en mètres, Z vers le haut.",
        "input_schema": {
            "type": "object",
            "properties": {
                "message": {"type": "string", "description": "Phrase courte en français pour l'utilisateur."},
                "actions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "action": {"type": "string", "enum": ["place", "delete_radius", "delete_selection", "select", "clear_selection", "move_selection", "rotate_selection", "scale_selection", "duplicate_selection", "create_mlo", "furnish_mlo", "goto", "undo"]},
                            "archetype": {"type": "string", "description": "place : nom d'archétype GTA V (ex. prop_bench_01a)"},
                            "count": {"type": "integer"},
                            "pattern": {"type": "string", "enum": ["point", "ligne", "cercle", "grille", "aleatoire"]},
                            "size": {"type": "number", "description": "espacement (ligne/grille) ou rayon (cercle/aleatoire)"},
                            "radius": {"type": "number"},
                            "filter": {"type": "string", "description": "texte contenu dans le nom d'archétype ; vide = tout"},
                            "dx": {"type": "number"}, "dy": {"type": "number"}, "dz": {"type": "number"},
                            "degrees": {"type": "number"},
                            "factor": {"type": "number"},
                            "name": {"type": "string"},
                            "cols": {"type": "integer"}, "rows": {"type": "integer"},
                            "width": {"type": "number"}, "depth": {"type": "number"}, "height": {"type": "number"},
                            "style": {"type": "string", "enum": ["salon", "bureau", "bar", "magasin", "cuisine"]},
                            "x": {"type": "number"}, "y": {"type": "number"}
                        },
                        "required": ["action"]
                    }
                }
            },
            "required": ["message", "actions"]
        }
    })
}

/// Envoie la demande à Claude dans un fil séparé ; la réponse arrive sur le canal.
pub fn ask_claude(api_key: String, history: Vec<ChatLine>, scene: String) -> mpsc::Receiver<Result<AiReply, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(call_claude(&api_key, &history, &scene));
    });
    rx
}

fn call_claude(api_key: &str, history: &[ChatLine], scene: &str) -> Result<AiReply, String> {
    let catalog_list: Vec<String> = catalog::PROPS.iter().map(|p| format!("{} ({})", p.name, p.label)).collect();
    let system = format!(
        "Tu es l'assistant intégré de DragoWalker, un éditeur de cartes GTA V (ymap, ytyp, intérieurs MLO) pour des créateurs de serveurs FiveM. \
         Tu réponds en français, brièvement. Pour modifier la carte, appelle l'outil executer_actions avec la liste d'actions à faire. \
         Les props connus du catalogue : {}. Tu peux aussi utiliser d'autres noms d'archétypes GTA V réels si l'utilisateur les cite. \
         Si la demande n'est pas une modification, réponds simplement sans outil.\n\nÉtat actuel :\n{}",
        catalog_list.join(", "),
        scene
    );
    let mut messages: Vec<Value> = Vec::new();
    for l in history.iter().rev().take(12).collect::<Vec<_>>().into_iter().rev() {
        let role = if l.from_user { "user" } else { "assistant" };
        if let Some(last) = messages.last_mut() {
            if last["role"] == role {
                let prev = last["content"].as_str().unwrap_or("").to_string();
                last["content"] = Value::String(format!("{prev}\n{}", l.text));
                continue;
            }
        }
        if messages.is_empty() && role == "assistant" {
            continue;
        }
        messages.push(json!({"role": role, "content": l.text}));
    }
    let body = json!({
        "model": MODEL,
        "max_tokens": 16000,
        "fallbacks": "default",
        "output_config": {"effort": "low"},
        "system": system,
        "tools": [tool_schema()],
        "tool_choice": {"type": "auto"},
        "messages": messages,
    });
    let resp = ureq::post("https://api.anthropic.com/v1/messages")
        .set("x-api-key", api_key)
        .set("anthropic-version", "2026-10-21")
        .set("anthropic-beta", "server-side-fallback-2026-07-01")
        .set("content-type", "application/json")
        .timeout(std::time::Duration::from_secs(120))
        .send_json(body);
    let v: Value = match resp {
        Ok(r) => r.into_json().map_err(|e| format!("Réponse illisible : {e}"))?,
        Err(ureq::Error::Status(code, r)) => {
            let t = r.into_string().unwrap_or_default();
            return Err(match code {
                401 => "Clé API refusée (401). Vérifie la clé dans l'onglet Options.".into(),
                429 => "Trop de demandes (429), réessaie dans un instant.".into(),
                c if c >= 500 => format!("Le service Claude est indisponible ({c}), réessaie."),
                c => format!("Erreur {c} : {t}"),
            });
        }
        Err(e) => return Err(format!("Pas de connexion au service Claude : {e}")),
    };
    if v["stop_reason"] == "refusal" {
        return Err("Claude a refusé cette demande.".into());
    }
    let mut message = String::new();
    let mut actions = Vec::new();
    if let Some(blocks) = v["content"].as_array() {
        for b in blocks {
            match b["type"].as_str() {
                Some("text") => {
                    if let Some(t) = b["text"].as_str() {
                        if !message.is_empty() {
                            message.push('\n');
                        }
                        message.push_str(t);
                    }
                }
                Some("tool_use") if b["name"] == "executer_actions" => {
                    if let Some(m) = b["input"]["message"].as_str() {
                        if !message.is_empty() {
                            message.push('\n');
                        }
                        message.push_str(m);
                    }
                    if let Some(list) = b["input"]["actions"].as_array() {
                        for a in list {
                            match serde_json::from_value::<Action>(a.clone()) {
                                Ok(act) => actions.push(act),
                                Err(e) => message.push_str(&format!("\n(action ignorée : {e})")),
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if message.is_empty() {
        message = "C'est fait.".into();
    }
    Ok(AiReply { message, actions })
}


// ---------------------------------------------------------------- autres moteurs d'IA

/// Moteur d'IA choisi dans l'onglet IA.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Provider {
    /// Claude Code installé sur l'ordinateur (abonnement de la personne)
    ClaudeCode,
    /// API Anthropic avec une clé
    ClaudeApi,
    /// API au format OpenAI : OpenAI, Mistral, Groq, OpenRouter, Ollama, LM Studio…
    OpenAiCompat,
    /// autre outil en ligne de commande (gemini, codex, ollama run…)
    Cli,
    /// interpréteur local hors ligne
    Local,
}

impl Provider {
    pub fn key(self) -> &'static str {
        match self {
            Provider::ClaudeCode => "claude_code",
            Provider::ClaudeApi => "claude_api",
            Provider::OpenAiCompat => "openai",
            Provider::Cli => "cli",
            Provider::Local => "local",
        }
    }
    pub fn from_key(k: &str) -> Option<Provider> {
        Some(match k {
            "claude_code" => Provider::ClaudeCode,
            "claude_api" => Provider::ClaudeApi,
            "openai" => Provider::OpenAiCompat,
            "cli" => Provider::Cli,
            "local" => Provider::Local,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Provider::ClaudeCode => "Claude Code (installé sur ce PC)",
            Provider::ClaudeApi => "Claude (clé API Anthropic)",
            Provider::OpenAiCompat => "API compatible OpenAI (OpenAI, Mistral, Groq, Ollama, LM Studio…)",
            Provider::Cli => "Autre IA en ligne de commande",
            Provider::Local => "Interpréteur local (hors ligne)",
        }
    }
    pub const ALL: [Provider; 5] = [Provider::ClaudeCode, Provider::ClaudeApi, Provider::OpenAiCompat, Provider::Cli, Provider::Local];
}

/// Réglages de l'IA.
#[derive(Clone)]
pub struct AiConfig {
    pub provider: Provider,
    pub claude_key: String,
    pub claude_code: Option<std::path::PathBuf>,
    /// modèle de Claude Code (vide = celui par défaut ; ex. sonnet, haiku, opus)
    pub cc_model: String,
    pub oa_base: String,
    pub oa_key: String,
    pub oa_model: String,
    /// commande complète, la demande est envoyée sur l'entrée standard (ex. « gemini -p », « ollama run llama3.1 »)
    pub cli_cmd: String,
}

/// Claude Code : dans le PATH ou dans %USERPROFILE%\.local\bin.
pub fn find_claude_code() -> Option<std::path::PathBuf> {
    let mut cands = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&path) {
            cands.push(p.join("claude.exe"));
            cands.push(p.join("claude.cmd"));
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let h = std::path::PathBuf::from(home);
        cands.push(h.join(".local").join("bin").join("claude.exe"));
        cands.push(h.join("AppData").join("Roaming").join("npm").join("claude.cmd"));
    }
    cands.into_iter().find(|p| p.is_file())
}

/// Outils d'IA en ligne de commande trouvés (pour proposer des exemples).
pub fn find_cli_tools() -> Vec<(&'static str, &'static str)> {
    let known: [(&str, &str); 5] = [("gemini", "gemini -p"), ("codex", "codex exec"), ("opencode", "opencode run"), ("ollama", "ollama run llama3.1"), ("aider", "aider --message-file -")];
    let mut out = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        let dirs: Vec<_> = std::env::split_paths(&path).collect();
        for (exe, cmd) in known {
            if dirs.iter().any(|d| ["exe", "cmd", "bat"].iter().any(|e| d.join(format!("{exe}.{e}")).is_file())) {
                out.push((exe, cmd));
            }
        }
    }
    out
}

fn system_prompt(scene: &str) -> String {
    let catalog_list: Vec<String> = catalog::PROPS.iter().map(|p| format!("{} ({})", p.name, p.label)).collect();
    format!(
        "Tu es l'assistant intégré de DragoWalker, un éditeur de cartes GTA V (ymap, ytyp, intérieurs MLO) pour des créateurs de serveurs FiveM. \
         Tu réponds en français, brièvement. Pour modifier la carte, tu rends une liste d'actions ; sinon une liste vide. \
         Les positions sont relatives au centre de la vue (cible de la caméra), en mètres, Z vers le haut. \
         Props d'exemple : {}. Utilise de préférence de vrais noms d'archétypes GTA V (prop_bench_01a, prop_tree_pine_01, prop_barrier_work05…).\n\nÉtat actuel :\n{}",
        catalog_list.join(", "),
        scene
    )
}

/// Schéma JSON de la réponse attendue : { message, actions[] }.
fn reply_schema() -> Value {
    tool_schema()["input_schema"].clone()
}

fn transcript(history: &[ChatLine]) -> String {
    let mut s = String::new();
    for l in history.iter().rev().take(12).collect::<Vec<_>>().into_iter().rev() {
        s += if l.from_user { "Utilisateur : " } else { "Assistant : " };
        s += &l.text;
        s.push('\n');
    }
    s
}

/// Lit { message, actions } (objet JSON, ou premier objet trouvé dans un texte).
fn reply_from_json(v: &Value) -> AiReply {
    let mut message = v["message"].as_str().unwrap_or("").to_string();
    let mut actions = Vec::new();
    if let Some(list) = v["actions"].as_array() {
        for a in list {
            match serde_json::from_value::<Action>(a.clone()) {
                Ok(act) => actions.push(act),
                Err(e) => message.push_str(&format!("\n(action ignorée : {e})")),
            }
        }
    }
    if message.is_empty() {
        message = if actions.is_empty() { "…".into() } else { "C'est fait.".into() };
    }
    AiReply { message, actions }
}

fn reply_from_text(text: &str) -> AiReply {
    let t = text.trim();
    if let (Some(a), Some(b)) = (t.find('{'), t.rfind('}')) {
        if a < b {
            if let Ok(v) = serde_json::from_str::<Value>(&t[a..=b]) {
                if v.get("actions").is_some() || v.get("message").is_some() {
                    return reply_from_json(&v);
                }
            }
        }
    }
    AiReply { message: if t.is_empty() { "(réponse vide)".into() } else { t.to_string() }, actions: Vec::new() }
}

/// Lance une commande sans fenêtre, envoie `input` sur son entrée et rend sa sortie.
fn run_cmd(mut cmd: std::process::Command, input: &str, timeout_s: u64) -> Result<(String, String, bool), String> {
    use std::io::{Read, Write};
    use std::process::Stdio;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // pas de fenêtre de console
    }
    let dir = std::env::temp_dir().join("dragowalker_ia");
    let _ = std::fs::create_dir_all(&dir);
    cmd.current_dir(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Impossible de lancer l'IA : {e}"))?;
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(input.as_bytes());
    }
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let h1 = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let h2 = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let t0 = std::time::Instant::now();
    loop {
        if let Ok(Some(st)) = child.try_wait() {
            let out = h1.join().unwrap_or_default();
            let err = h2.join().unwrap_or_default();
            return Ok((out, err, st.success()));
        }
        if t0.elapsed().as_secs() > timeout_s {
            let _ = child.kill();
            return Err(format!("L'IA n'a pas répondu en {timeout_s} s."));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Claude Code en mode non interactif : aucun outil, réponse structurée selon le schéma.
fn call_claude_code(cfg: &AiConfig, history: &[ChatLine], scene: &str) -> Result<AiReply, String> {
    let exe = cfg.claude_code.clone().or_else(find_claude_code).ok_or("Claude Code est introuvable sur ce PC (commande « claude »).")?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["-p", "--output-format", "json", "--tools", "", "--no-session-persistence", "--strict-mcp-config"]);
    cmd.arg("--system-prompt").arg(system_prompt(scene));
    cmd.arg("--json-schema").arg(reply_schema().to_string());
    if !cfg.cc_model.trim().is_empty() {
        cmd.arg("--model").arg(cfg.cc_model.trim());
    }
    let input = format!("{}\nRéponds à la dernière demande de l'utilisateur.", transcript(history));
    let (out, err, _ok) = run_cmd(cmd, &input, 240)?;
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| {
        let msg = if err.trim().is_empty() { out.trim().to_string() } else { err.trim().to_string() };
        format!("Claude Code : {}", msg.chars().take(400).collect::<String>())
    })?;
    if v["is_error"] == true {
        return Err(format!("Claude Code : {}", v["result"].as_str().unwrap_or("erreur")));
    }
    if v["structured_output"].is_object() {
        return Ok(reply_from_json(&v["structured_output"]));
    }
    Ok(reply_from_text(v["result"].as_str().unwrap_or("")))
}

/// API au format OpenAI (/chat/completions), réponse JSON demandée.
fn call_openai(cfg: &AiConfig, history: &[ChatLine], scene: &str) -> Result<AiReply, String> {
    let base = cfg.oa_base.trim().trim_end_matches('/');
    if base.is_empty() || cfg.oa_model.trim().is_empty() {
        return Err("Indique l'adresse et le modèle de l'API dans les réglages de l'IA.".into());
    }
    let mut messages = vec![json!({"role": "system", "content": format!("{}\n\nRéponds UNIQUEMENT par un objet JSON de ce schéma : {}", system_prompt(scene), reply_schema())})];
    for l in history.iter().rev().take(12).collect::<Vec<_>>().into_iter().rev() {
        messages.push(json!({"role": if l.from_user { "user" } else { "assistant" }, "content": l.text}));
    }
    let body = json!({"model": cfg.oa_model.trim(), "messages": messages, "response_format": {"type": "json_object"}});
    let mut req = ureq::post(&format!("{base}/chat/completions")).set("content-type", "application/json").timeout(std::time::Duration::from_secs(180));
    if !cfg.oa_key.trim().is_empty() {
        req = req.set("authorization", &format!("Bearer {}", cfg.oa_key.trim()));
    }
    let v: Value = match req.send_json(body) {
        Ok(r) => r.into_json().map_err(|e| format!("Réponse illisible : {e}"))?,
        Err(ureq::Error::Status(code, r)) => return Err(format!("Erreur {code} : {}", r.into_string().unwrap_or_default().chars().take(300).collect::<String>())),
        Err(e) => return Err(format!("Pas de connexion à {base} : {e}")),
    };
    Ok(reply_from_text(v["choices"][0]["message"]["content"].as_str().unwrap_or("")))
}

/// Autre outil en ligne de commande : la demande complète sur l'entrée standard, la réponse sur la sortie.
fn call_cli(cfg: &AiConfig, history: &[ChatLine], scene: &str) -> Result<AiReply, String> {
    let parts: Vec<&str> = cfg.cli_cmd.split_whitespace().collect();
    let (exe, args) = parts.split_first().ok_or("Indique la commande de l'IA dans les réglages (ex. « gemini -p »).")?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args);
    let input = format!(
        "{}\n\nRéponds UNIQUEMENT par un objet JSON de ce schéma, sans autre texte : {}\n\nConversation :\n{}",
        system_prompt(scene),
        reply_schema(),
        transcript(history)
    );
    let (out, err, ok) = run_cmd(cmd, &input, 300)?;
    if !ok && out.trim().is_empty() {
        return Err(format!("La commande a échoué : {}", err.trim().chars().take(300).collect::<String>()));
    }
    Ok(reply_from_text(&out))
}

/// Envoie la demande au moteur choisi dans un fil séparé ; la réponse arrive sur le canal.
pub fn ask(cfg: AiConfig, history: Vec<ChatLine>, scene: String) -> mpsc::Receiver<Result<AiReply, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = match cfg.provider {
            Provider::ClaudeCode => call_claude_code(&cfg, &history, &scene),
            Provider::ClaudeApi => call_claude(&cfg.claude_key, &history, &scene),
            Provider::OpenAiCompat => call_openai(&cfg, &history, &scene),
            Provider::Cli => call_cli(&cfg, &history, &scene),
            Provider::Local => Ok(local_parse(&history.iter().rev().find(|l| l.from_user).map(|l| l.text.clone()).unwrap_or_default())),
        };
        let _ = tx.send(r);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_place_line() {
        let r = local_parse("Pose 10 arbres en ligne");
        assert!(matches!(&r.actions[0], Action::Place { count: 10, pattern, .. } if pattern == "ligne"));
    }
    #[test]
    fn parse_delete_radius() {
        let r = local_parse("supprime tout dans un rayon de 40 m");
        assert_eq!(r.actions[0], Action::DeleteRadius { radius: 40.0, filter: String::new() });
    }
    #[test]
    fn parse_mlo() {
        let r = local_parse("crée un intérieur de 4 pièces");
        assert!(matches!(r.actions[0], Action::CreateMlo { cols: 2, rows: 2, .. }));
    }
    #[test]
    fn json_action() {
        let a: Action = serde_json::from_str(r#"{"action":"delete_radius","radius":12}"#).unwrap();
        assert_eq!(a, Action::DeleteRadius { radius: 12.0, filter: String::new() });
    }
}
