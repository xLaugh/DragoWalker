//! Serveur MCP de DragoWalker : Claude Code (l'assistant intégré, ou ton propre Claude Code) pilote l'application
//! avec de vrais outils — lire la scène, chercher dans le catalogue du jeu, créer et aménager des intérieurs, poser
//! et déplacer des objets, regarder la vue en image — et voit le résultat de chaque étape.
//!
//! Protocole MCP en « HTTP simple » (streamable HTTP, réponses JSON) sur 127.0.0.1, adresse protégée par un jeton.
//! Les demandes arrivent sur un fil à part et sont exécutées dans la boucle de l'application (`mcp_tick`), qui
//! répond quand c'est fait. Les skills (savoir-faire écrit) et la configuration sont écrits dans un plugin Claude
//! Code (`claude-plugin` dans le dossier de données de DragoWalker).

use crate::app::DwApp;
use crate::decor::{self, KINDS, THEMES};
use crate::math::{v3, Quat, V3};
use eframe::egui;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;

/// Port préféré (un autre si déjà pris, par exemple par un second DragoWalker).
pub const PORT: u16 = 47615;
const SERVER_NAME: &str = "dragowalker";

pub type Reply = mpsc::Sender<Result<Vec<Value>, String>>;

/// Une demande d'outil à exécuter dans l'application.
pub struct McpCall {
    pub name: String,
    pub args: Value,
    pub reply: Reply,
}

pub struct McpServer {
    pub port: u16,
    pub token: String,
    pub rx: mpsc::Receiver<McpCall>,
}

impl McpServer {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp/{}", self.port, self.token)
    }
}

/// Démarre le serveur (fil de fond) ; None si aucun port n'est libre.
pub fn start(ctx: egui::Context, token: String) -> Option<McpServer> {
    let listener = TcpListener::bind(("127.0.0.1", PORT)).or_else(|_| TcpListener::bind(("127.0.0.1", 0))).ok()?;
    let port = listener.local_addr().ok()?.port();
    let (tx, rx) = mpsc::channel::<McpCall>();
    let path = format!("/mcp/{token}");
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let (tx, ctx, path) = (tx.clone(), ctx.clone(), path.clone());
            std::thread::spawn(move || {
                let _ = serve(s, &tx, &ctx, &path);
            });
        }
    });
    Some(McpServer { port, token, rx })
}

fn respond(s: &mut TcpStream, code: &str, body: &str) -> std::io::Result<()> {
    let head = format!("HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    s.write_all(head.as_bytes())?;
    s.write_all(body.as_bytes())?;
    s.flush()
}

/// Une connexion : une requête HTTP, une réponse.
fn serve(mut s: TcpStream, tx: &mpsc::Sender<McpCall>, ctx: &egui::Context, path: &str) -> std::io::Result<()> {
    s.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
    let mut r = BufReader::new(s.try_clone()?);
    let mut line = String::new();
    r.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if target.split('?').next() != Some(path) {
        return respond(&mut s, "404 Not Found", "{}");
    }
    match method.as_str() {
        "POST" => {}
        "DELETE" => return respond(&mut s, "200 OK", "{}"),
        _ => return respond(&mut s, "405 Method Not Allowed", "{}"),
    }
    let mut body = vec![0u8; len.min(16 << 20)];
    r.read_exact(&mut body)?;
    let Ok(msg) = serde_json::from_slice::<Value>(&body) else {
        return respond(&mut s, "400 Bad Request", &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "JSON illisible"}}).to_string());
    };
    let out = match &msg {
        Value::Array(list) => {
            let v: Vec<Value> = list.iter().filter_map(|m| handle(m, tx, ctx)).collect();
            if v.is_empty() { None } else { Some(Value::Array(v)) }
        }
        m => handle(m, tx, ctx),
    };
    match out {
        Some(v) => respond(&mut s, "200 OK", &v.to_string()),
        None => {
            s.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
            s.flush()
        }
    }
}

/// Un message JSON-RPC ; None pour une notification.
fn handle(m: &Value, tx: &mpsc::Sender<McpCall>, ctx: &egui::Context) -> Option<Value> {
    let id = m.get("id").cloned()?;
    let method = m["method"].as_str().unwrap_or("");
    let ok = |result: Value| json!({"jsonrpc": "2.0", "id": id.clone(), "result": result});
    let err = |code: i64, message: &str| json!({"jsonrpc": "2.0", "id": id.clone(), "error": {"code": code, "message": message}});
    Some(match method {
        "initialize" => ok(json!({
            "protocolVersion": m["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": SERVER_NAME, "title": "DragoWalker", "version": env!("CARGO_PKG_VERSION")},
            "instructions": INSTRUCTIONS,
        })),
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = m["params"]["name"].as_str().unwrap_or("").to_string();
            let args = m["params"].get("arguments").cloned().unwrap_or(json!({}));
            let (rtx, rrx) = mpsc::channel();
            if tx.send(McpCall { name, args, reply: rtx }).is_err() {
                return Some(err(-32603, "DragoWalker se ferme"));
            }
            ctx.request_repaint();
            match rrx.recv_timeout(std::time::Duration::from_secs(600)) {
                Ok(Ok(content)) => ok(json!({"content": content, "isError": false})),
                Ok(Err(e)) => ok(json!({"content": [{"type": "text", "text": e}], "isError": true})),
                Err(_) => ok(json!({"content": [{"type": "text", "text": "DragoWalker n'a pas répondu (fenêtre réduite ou occupée ?)"}], "isError": true})),
            }
        }
        _ => err(-32601, "méthode inconnue"),
    })
}

const INSTRUCTIONS: &str = "DragoWalker est un éditeur de cartes GTA V et d'intérieurs MLO pour FiveM. Commence par get_scene, et get_selection \
quand la personne parle de « ce », « cet », « celui-là » ; pour un intérieur, get_interior. Analyse avant d'agir, termine par check_project. Les noms de modèles viennent de search_props (jamais inventés). Après une modification \
visible, regarde le résultat avec screenshot. Chaque modification s'annule avec undo. Coordonnées en mètres, Z vers le haut, \
cap en degrés (0 = face avant vers le sud, -Y ; 90 = vers l'est).";

fn tool(name: &str, desc: &str, props: Value, required: &[&str]) -> Value {
    json!({"name": name, "description": desc, "inputSchema": {"type": "object", "properties": props, "required": required}})
}

/// Les outils, décrits pour l'IA.
pub fn tools() -> Vec<Value> {
    let mut v = tools_base();
    v.extend(crate::mcp_plus::tools_plus());
    v.extend(crate::mcp_game::tools_game());
    v
}

fn tools_base() -> Vec<Value> {
    let room = json!({"description": "numéro de la pièce (1, 2…) ou son nom", "type": ["integer", "string"]});
    let interior = json!({"type": "string", "description": "nom de l'intérieur (MLO), voir get_scene"});
    vec![
        tool("get_scene", "Résumé du projet : centre de la vue, sélection, cartes, intérieurs (placés ou non, pièces), studio ouvert, jeu chargé.", json!({}), &[]),
        tool(
            "get_interior",
            "Détail d'un intérieur : pièces (numéro, nom, rectangle local x0 y0 x1 y1, sol z, ambiance, nombre d'objets), passages, et objets (id, modèle, pièce, position locale, cap).",
            json!({"interior": interior, "max_objects": {"type": "integer", "description": "200 par défaut"}}),
            &["interior"],
        ),
        tool(
            "list_room_types",
            "Types de pièce de l'aménagement automatique (numéro, nom, styles possibles), styles, et intérieurs complets tout prêts.",
            json!({}),
            &[],
        ),
        tool(
            "search_props",
            "Cherche des modèles du jeu par mots du nom en anglais (ex. « sofa », « chair office », « fridge », « bed », « plant », « cellgate », « door »). Rend nom, taille (x y z en m). Par défaut dans les meubles ; all=true (ou peu de résultats) : dans tous les modèles du jeu (portes, murs, objets de décor). À utiliser avant de poser un modèle.",
            json!({"query": {"type": "string"}, "limit": {"type": "integer", "description": "30 par défaut"}, "all": {"type": "boolean"}}),
            &["query"],
        ),
        tool(
            "create_interior",
            "Crée un intérieur et l'ouvre dans le Studio. Avec template (nom ou numéro de list_room_types) : intérieur complet tout prêt, pièces, portes et aménagement compris. Sans : vide (rooms=0) ou une pièce de 6 × 5 m (rooms=1).",
            json!({"template": {"type": ["string", "integer"]}, "name": {"type": "string"}, "theme": {"type": "integer", "description": "0 au hasard, 1 moderne, 2 classique, 3 luxe, 4 industriel"}, "rooms": {"type": "integer"}}),
            &[],
        ),
        tool("open_studio", "Ouvre un intérieur du projet dans le Studio (seul l'intérieur est affiché).", json!({"interior": interior}), &["interior"]),
        tool("close_studio", "Quitte le Studio.", json!({}), &[]),
        tool(
            "add_room",
            "Ajoute une pièce rectangulaire à un intérieur (coordonnées locales, en m), au sol et à la hauteur des pièces de l'étage (z0, height pour forcer). Une porte se fait avec chaque pièce voisine qui la touche, ou seulement celles de connect. Murs, sol et plafond générés.",
            json!({"interior": interior, "x0": {"type": "number"}, "y0": {"type": "number"}, "x1": {"type": "number"}, "y1": {"type": "number"}, "name": {"type": "string"}, "floor": {"type": "integer", "description": "étage, 0 par défaut"}, "z0": {"type": "number"}, "height": {"type": "number"}, "connect": {"type": "array", "items": {"type": ["integer", "string"]}, "description": "pièces à relier par une porte (par défaut toutes celles qui touchent)"}}),
            &["interior", "x0", "y0", "x1", "y1"],
        ),
        tool(
            "furnish_room",
            "Aménage et décore une pièce automatiquement (vrais meubles du jeu, passages libres, ambiance assortie). kind : numéro ou nom de list_room_types. keep_existing : garder les objets déjà là. seed : autre variante.",
            json!({"interior": interior, "room": room, "kind": {"type": ["integer", "string"]}, "theme": {"type": "integer"}, "keep_existing": {"type": "boolean"}, "seed": {"type": "integer"}}),
            &["interior", "room", "kind"],
        ),
        tool("set_room_ambiance", "Change l'ambiance (timecycle du jeu) d'une pièce.", json!({"interior": interior, "room": room, "timecycle": {"type": "string"}}), &["interior", "room", "timecycle"]),
        tool(
            "place_doors",
            "Pose de vraies portes du jeu dans les passages d'un intérieur qui n'en ont pas. model : un modèle de porte précis (ex. v_ilev_ph_cellgate pour une porte de cellule à barreaux) ; rooms : seulement les passages de ces pièces. Poser d'abord les portes spéciales (cellules), puis les autres.",
            json!({"interior": interior, "model": {"type": "string"}, "rooms": {"type": "array", "items": {"type": ["integer", "string"]}}}),
            &["interior"],
        ),
        tool(
            "place_prop",
            "Pose un modèle. Avec interior : position locale de l'intérieur (z absent = sol de la pièce) ; sinon position du monde (z absent = posé au sol). heading en degrés.",
            json!({"model": {"type": "string"}, "x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}, "heading": {"type": "number"}, "interior": interior, "room": room}),
            &["model", "x", "y"],
        ),
        tool(
            "move_object",
            "Déplace ou tourne un objet par son id (position dans le même repère que get_interior pour un objet d'intérieur, sinon le monde) ; ground=true le pose ensuite au sol.",
            json!({"id": {"type": "integer"}, "x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}, "heading": {"type": "number"}, "ground": {"type": "boolean"}}),
            &["id"],
        ),
        tool("delete_objects", "Supprime des objets par leurs id.", json!({"ids": {"type": "array", "items": {"type": "integer"}}}), &["ids"]),
        tool("select_objects", "Sélectionne des objets dans la vue (ids ; liste vide = rien).", json!({"ids": {"type": "array", "items": {"type": "integer"}}}), &["ids"]),
        tool(
            "set_camera",
            "Place la caméra : sur un intérieur (interior) ou un point du monde (x y z), distance, angles en degrés (yaw 0 = regarde le nord, pitch 0 = horizontal, 90 = de dessus), top=true pour le plan vu de dessus, top=false pour revenir en 3D (sinon la vue garde son mode).",
            json!({"interior": interior, "x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}, "dist": {"type": "number"}, "yaw": {"type": "number"}, "pitch": {"type": "number"}, "top": {"type": "boolean"}}),
            &[],
        ),
        tool("screenshot", "Image de la fenêtre de DragoWalker telle qu'elle est (vue 3D ou plan) pour vérifier le résultat.", json!({"width": {"type": "integer", "description": "largeur max, 1280 par défaut"}}), &[]),
        tool("undo", "Annule la dernière modification (comme Ctrl+Z).", json!({}), &[]),
        tool(
            "interaction_points",
            "Points d'interaction FiveM (ox_target / qb-target) trouvés dans les intérieurs ; enable=true/false coche ou décoche leur export.",
            json!({"enable": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "run_actions",
            "Actions sur la carte autour du centre de la vue (poser en ligne/cercle/grille, supprimer dans un rayon, sélectionner, déplacer, tourner, dupliquer, créer un intérieur en grille…), au format des actions de l'assistant.",
            json!({"actions": {"type": "array", "items": {"type": "object"}}}),
            &["actions"],
        ),
    ]
}

fn txt(s: impl Into<String>) -> Vec<Value> {
    vec![json!({"type": "text", "text": s.into()})]
}

/// Base64 (pour les images).
pub(crate) fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

fn num(a: &Value, k: &str) -> Option<f32> {
    a.get(k).and_then(|v| v.as_f64()).map(|v| v as f32)
}

/// Type de pièce : numéro ou nom (« cuisine », « Salle de bain »…).
fn kind_arg(v: &Value) -> Option<usize> {
    if let Some(n) = v.as_u64() {
        return ((n as usize) < KINDS.len()).then_some(n as usize);
    }
    let s = v.as_str()?.trim().to_lowercase();
    KINDS.iter().position(|k| k.0.to_lowercase() == s).or_else(|| decor::kind_from_name(&s))
}

/// Dossier du plugin Claude Code de DragoWalker (skills + serveur MCP).
pub fn plugin_dir() -> std::path::PathBuf {
    crate::safety::data_dir().join("claude-plugin")
}

const SKILLS: [(&str, &str); 6] = [
    ("dragowalker-architecture", include_str!("../skills/dragowalker-architecture.md")),
    ("dragowalker-methode", include_str!("../skills/dragowalker-methode.md")),
    ("dragowalker-interieurs", include_str!("../skills/dragowalker-interieurs.md")),
    ("dragowalker-amenager", include_str!("../skills/dragowalker-amenager.md")),
    ("dragowalker-carte", include_str!("../skills/dragowalker-carte.md")),
    ("dragowalker-fivem", include_str!("../skills/dragowalker-fivem.md")),
];

/// Écrit le plugin : manifeste, adresse du serveur MCP, skills.
pub fn write_plugin(url: &str) -> std::io::Result<std::path::PathBuf> {
    let d = plugin_dir();
    std::fs::create_dir_all(d.join(".claude-plugin"))?;
    std::fs::write(
        d.join(".claude-plugin").join("plugin.json"),
        json!({"name": "dragowalker", "version": env!("CARGO_PKG_VERSION"), "description": "Piloter DragoWalker (éditeur de cartes GTA V et d'intérieurs MLO pour FiveM) : outils MCP et savoir-faire."}).to_string(),
    )?;
    std::fs::write(d.join(".mcp.json"), mcp_config(url))?;
    for (name, body) in SKILLS {
        let sd = d.join("skills").join(name);
        std::fs::create_dir_all(&sd)?;
        std::fs::write(sd.join("SKILL.md"), body)?;
    }
    Ok(d)
}

/// Configuration MCP pour Claude Code (--mcp-config ou .mcp.json).
pub fn mcp_config(url: &str) -> String {
    json!({"mcpServers": {SERVER_NAME: {"type": "http", "url": url}}}).to_string()
}

/// Jeton aléatoire de l'adresse du serveur (gardé dans les réglages).
pub fn new_token() -> String {
    use sha2::Digest;
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let h = sha2::Sha256::digest(format!("{t}-{}-{:p}", std::process::id(), &t).as_bytes());
    h.iter().take(12).map(|b| format!("{b:02x}")).collect()
}

/// Résultat d'un outil : contenu, ou image à prendre (réponse plus tard).
enum Out {
    Done(Result<Vec<Value>, String>),
    Shot(u32),
}

impl DwApp {
    /// À chaque image : démarre le serveur au besoin, exécute les demandes, prend les captures demandées.
    pub fn mcp_tick(&mut self, ctx: &egui::Context) {
        if self.mcp.is_none() && self.mcp_on && !self.mcp_failed {
            let token = {
                let t = crate::app::load_settings()["mcp_token"].as_str().unwrap_or("").to_string();
                if t.len() >= 16 {
                    t
                } else {
                    let t = new_token();
                    crate::app::save_setting("mcp_token", json!(t));
                    t
                }
            };
            match start(ctx.clone(), token) {
                Some(s) => {
                    let _ = write_plugin(&s.url());
                    self.mcp = Some(s);
                }
                None => self.mcp_failed = true,
            }
        }
        // capture demandée : on laisse la vue se redessiner deux images, puis on la prend
        if let Some((at, w, _)) = &self.mcp_shot {
            let (at, w) = (*at, *w);
            let img = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, user_data, .. } if user_data.data.as_ref().and_then(|d| d.downcast_ref::<&str>()).copied() == Some("mcp") => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = img {
                if let Some((_, _, reply)) = self.mcp_shot.take() {
                    let _ = reply.send(encode_shot(&img, w));
                }
            } else if self.frame == at {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new("mcp")));
            } else if self.frame > at + 240 {
                if let Some((_, _, reply)) = self.mcp_shot.take() {
                    let _ = reply.send(Err("La capture n'est pas venue (fenêtre réduite ?)".into()));
                }
            }
            ctx.request_repaint();
        }
        let calls: Vec<McpCall> = match &self.mcp {
            Some(s) => s.rx.try_iter().collect(),
            None => return,
        };
        for c in calls {
            self.status = format!("IA : {}…", c.name);
            match self.mcp_run(&c.name, &c.args) {
                Out::Done(r) => {
                    let _ = c.reply.send(r);
                }
                Out::Shot(w) => match &self.mcp_shot {
                    Some(_) => {
                        let _ = c.reply.send(Err("Une capture est déjà en cours.".into()));
                    }
                    None => self.mcp_shot = Some((self.frame + 3, w, c.reply)),
                },
            }
            ctx.request_repaint();
        }
    }

    pub(crate) fn find_interior(&self, name: &str) -> Result<(usize, usize), String> {
        for (t, y) in self.proj.ytyps.iter().enumerate() {
            for (m, mlo) in y.mlos.iter().enumerate() {
                if mlo.name.eq_ignore_ascii_case(name.trim()) {
                    return Ok((t, m));
                }
            }
        }
        Err(format!("Intérieur « {name} » introuvable (voir get_scene)."))
    }

    pub(crate) fn room_arg(&self, t: usize, m: usize, v: &Value) -> Result<usize, String> {
        let mlo = &self.proj.ytyps[t].mlos[m];
        let r = match v {
            Value::Number(n) => n.as_u64().map(|n| n as usize),
            Value::String(s) => s.trim().parse::<usize>().ok().or_else(|| mlo.rooms.iter().position(|r| r.name.eq_ignore_ascii_case(s.trim()))),
            _ => None,
        };
        match r {
            Some(r) if r >= 1 && r < mlo.rooms.len() => Ok(r),
            _ => Err(format!("Pièce {v} introuvable dans « {} » (pièces 1 à {}).", mlo.name, mlo.rooms.len().saturating_sub(1))),
        }
    }

    fn mcp_run(&mut self, name: &str, a: &Value) -> Out {
        let r = match name {
            "get_scene" => Ok(txt(self.mcp_scene())),
            "get_interior" => self.mcp_interior(a),
            "list_room_types" => Ok(txt(mcp_room_types())),
            "search_props" => Ok(txt(self.mcp_search(a))),
            "create_interior" => self.mcp_create(a),
            "open_studio" => self.find_interior(a["interior"].as_str().unwrap_or("")).map(|(t, m)| {
                let n = self.proj.ytyps[t].mlos[m].name.clone();
                self.studio_enter(&n);
                txt(format!("Studio ouvert sur « {n} »."))
            }),
            "close_studio" => {
                self.studio_exit(false);
                Ok(txt("Studio fermé."))
            }
            "add_room" => self.mcp_add_room(a),
            "furnish_room" => self.mcp_furnish(a),
            "set_room_ambiance" => self.find_interior(a["interior"].as_str().unwrap_or("")).and_then(|(t, m)| {
                let ri = self.room_arg(t, m, &a["room"])?;
                let tc = a["timecycle"].as_str().unwrap_or("").trim().to_string();
                self.push_undo();
                self.proj.ytyps[t].mlos[m].rooms[ri].timecycle = tc.clone();
                Ok(txt(format!("Ambiance de la pièce {ri} : « {tc} ».")))
            }),
            "place_doors" => self.find_interior(a["interior"].as_str().unwrap_or("")).and_then(|(t, m)| {
                self.cur_mlo = Some((t, m));
                let rooms = match a["rooms"].as_array() {
                    Some(list) => Some(list.iter().map(|v| self.room_arg(t, m, v)).collect::<Result<Vec<usize>, String>>()?),
                    None => None,
                };
                let keep = std::mem::replace(&mut self.plan.door_model, a["model"].as_str().unwrap_or("").trim().to_lowercase());
                self.plan.door_rooms = rooms;
                let msg = self.plan_doors();
                self.plan.door_model = keep;
                self.plan.door_rooms = None;
                Ok(txt(msg))
            }),
            "place_prop" => self.mcp_place(a),
            "move_object" => self.mcp_move(a),
            "delete_objects" => {
                let ids: Vec<u64> = a["ids"].as_array().map(|v| v.iter().filter_map(|x| x.as_u64()).collect()).unwrap_or_default();
                let found: Vec<u64> = ids.iter().copied().filter(|id| self.proj.find_id(*id).is_some()).collect();
                if found.is_empty() {
                    Err("Aucun de ces objets n'existe.".into())
                } else {
                    self.set_selection(found.clone());
                    self.delete_selected();
                    Ok(txt(format!("{} objet(s) supprimé(s).", found.len())))
                }
            }
            "select_objects" => {
                let ids: Vec<u64> = a["ids"].as_array().map(|v| v.iter().filter_map(|x| x.as_u64()).filter(|id| self.proj.find_id(*id).is_some()).collect()).unwrap_or_default();
                let n = ids.len();
                self.set_selection(ids);
                Ok(txt(format!("{n} objet(s) sélectionné(s).")))
            }
            "set_camera" => self.mcp_camera(a),
            "screenshot" => return Out::Shot(a["width"].as_u64().unwrap_or(1280).clamp(320, 1920) as u32),
            "undo" => {
                self.do_undo();
                Ok(txt(format!("Annulé. {}", self.status)))
            }
            "interaction_points" => {
                if let Some(on) = a["enable"].as_bool() {
                    self.push_undo();
                    self.proj.targets = on;
                }
                let pts = self.interaction_points();
                let mut s = format!("Export des points d'interaction : {}. {} point(s).\n", if self.proj.targets { "coché" } else { "décoché" }, pts.len());
                for p in pts.iter().take(80) {
                    s += &format!("- {} ({}) : {} dans {}/{} à ({:.2}, {:.2}, {:.2})\n", p.label, p.kind, p.model, p.mlo, p.room, p.coords.x, p.coords.y, p.coords.z);
                }
                Ok(txt(s))
            }
            "run_actions" => {
                let v = json!({"message": "", "actions": a["actions"].clone()});
                let r = crate::ai::reply_from_json(&v);
                let mut out = r.message.clone();
                for act in &r.actions {
                    out.push_str(&format!("\n✔ {}", self.run_action(act)));
                }
                Ok(txt(out))
            }
            n if crate::mcp_plus::NAMES.contains(&n) => self.mcp_plus(n, a),
            n if crate::mcp_game::NAMES.contains(&n) => self.mcp_game(n, a),
            _ => Err(format!("Outil inconnu : {name}")),
        };
        Out::Done(r)
    }

    fn mcp_scene(&self) -> String {
        let mut s = self.scene_summary();
        s += &format!("Jeu chargé : {}.\n", if self.game.world.is_some() { "oui" } else { "non (catalogue et portes du jeu limités)" });
        if self.studio.on {
            s += &format!("Studio ouvert sur « {} ».\n", self.studio.mlo);
        }
        for y in &self.proj.ytyps {
            for m in &y.mlos {
                let inst = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(&m.name)).map(|e| e.id);
                let place = self
                    .proj
                    .mlo_placement(&m.name)
                    .map(|(p, _)| format!("placé en ({:.1}, {:.1}, {:.1}), id du placement {}", p.x, p.y, p.z, inst.unwrap_or(0)))
                    .unwrap_or_else(|| "pas placé".into());
                let rooms: Vec<String> = m.rooms.iter().enumerate().skip(1).map(|(i, r)| format!("{i}:{}", r.name)).collect();
                s += &format!("Intérieur « {} » ({place}) : pièces {}\n", m.name, rooms.join(", "));
            }
        }
        s
    }

    fn mcp_interior(&self, a: &Value) -> Result<Vec<Value>, String> {
        let (t, m) = self.find_interior(a["interior"].as_str().unwrap_or(""))?;
        let mlo = &self.proj.ytyps[t].mlos[m];
        let max = a["max_objects"].as_u64().unwrap_or(200) as usize;
        let mut s = format!("Intérieur « {} » : {} pièce(s), {} passage(s), {} objet(s).\n", mlo.name, mlo.rooms.len() - 1, mlo.portals.len(), mlo.entities.len());
        if let Some((p, q)) = self.proj.mlo_placement(&mlo.name) {
            s += &format!("Placé en ({:.2}, {:.2}, {:.2}), cap {:.0}° ; les positions ci-dessous sont locales.\n", p.x, p.y, p.z, q.heading_deg());
        }
        s += "Pièces :\n";
        for (i, r) in mlo.rooms.iter().enumerate().skip(1) {
            let n = mlo.entities.iter().filter(|e| e.room == i && !e.archetype.starts_with("dw_")).count();
            s += &format!(
                "  {i} « {} » : x {:.2}..{:.2}, y {:.2}..{:.2}, sol z {:.2}, hauteur {:.2}, ambiance « {} », {n} objet(s){}\n",
                r.name,
                r.bb_min.x,
                r.bb_max.x,
                r.bb_min.y,
                r.bb_max.y,
                r.bb_min.z,
                r.bb_max.z - r.bb_min.z,
                r.timecycle,
                decor::kind_from_name(&r.name).map(|k| format!(", type deviné {}", KINDS[k].0)).unwrap_or_default()
            );
        }
        s += "Passages : ";
        let ps: Vec<String> = mlo.portals.iter().map(|p| format!("{}↔{}", p.room_from, p.room_to)).collect();
        s += &ps.join(", ");
        s += "\nObjets (id, modèle, pièce, x y z, cap) :\n";
        for e in mlo.entities.iter().filter(|e| !e.archetype.starts_with("dw_")).take(max) {
            s += &format!("  {} {} p{} ({:.2}, {:.2}, {:.2}) {:.0}°\n", e.id, e.archetype, e.room, e.pos.x, e.pos.y, e.pos.z, e.rot.heading_deg());
        }
        Ok(txt(s))
    }

    fn mcp_search(&mut self, a: &Value) -> String {
        let q = a["query"].as_str().unwrap_or("").to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        let limit = a["limit"].as_u64().unwrap_or(30).clamp(1, 200) as usize;
        let mut rows: Vec<(String, V3)> = Vec::new();
        let items = self.furn_items_pub();
        if items.is_empty() {
            for n in decor::all_models() {
                if let Some((s, _)) = decor::fallback_dims(n) {
                    rows.push((n.to_string(), s));
                }
            }
        } else {
            rows = items.into_iter().map(|i| (i.name, i.size)).collect();
        }
        let mut hits: Vec<&(String, V3)> = rows.iter().filter(|(n, _)| words.iter().all(|w| n.to_lowercase().contains(w))).collect();
        // tous les modèles du jeu : demandé, ou trop peu de meubles trouvés
        let all_rows: Vec<(String, V3)>;
        if a["all"].as_bool() == Some(true) || hits.len() < 5 {
            if let Some(w) = self.game.world.as_ref() {
                all_rows = w
                    .archetypes
                    .iter()
                    .filter(|(_, ar)| !ar.mlo)
                    .filter_map(|(h, ar)| w.names.get(h).map(|n| (n.clone(), ar.bb_max - ar.bb_min)))
                    .filter(|(n, s)| s.x.max(s.y).max(s.z) < 30.0 && words.iter().all(|w| n.to_lowercase().contains(w)))
                    .collect();
                let have: std::collections::HashSet<&str> = hits.iter().map(|(n, _)| n.as_str()).collect();
                let extra: Vec<&(String, V3)> = all_rows.iter().filter(|(n, _)| !have.contains(n.as_str())).collect();
                hits.extend(extra);
            }
        }
        // les noms courts (prop_…, v_res_…) d'abord : souvent les meubles simples
        hits.sort_by_key(|(n, _)| (!n.starts_with("prop_") && !n.starts_with("v_"), n.len()));
        let total = hits.len();
        let mut s = format!("{total} modèle(s) pour « {q} »{}.\n", if total > limit { format!(", les {limit} premiers") } else { String::new() });
        for (n, sz) in hits.into_iter().take(limit) {
            s += &format!("  {n} : {:.2} × {:.2} × {:.2} m\n", sz.x, sz.y, sz.z);
        }
        s
    }

    fn mcp_create(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        if let Some(n) = a["name"].as_str() {
            self.studio.new_name = n.to_string();
        }
        let theme = a["theme"].as_u64().unwrap_or(0).min(4) as usize;
        let tpl = &a["template"];
        if !tpl.is_null() {
            let gi = match tpl {
                Value::Number(n) => n.as_u64().map(|n| n as usize),
                Value::String(s) => {
                    let s = s.trim().to_lowercase();
                    crate::gabarits::GABARITS.iter().position(|g| g.name.to_lowercase() == s || g.slug == s).or_else(|| crate::gabarits::GABARITS.iter().position(|g| g.name.to_lowercase().contains(&s)))
                }
                _ => None,
            }
            .filter(|g| *g < crate::gabarits::GABARITS.len())
            .ok_or("Intérieur tout prêt inconnu (voir list_room_types).")?;
            self.studio_new_gabarit(gi, theme);
        } else {
            self.studio.new_kind = if a["rooms"].as_u64() == Some(0) { 0 } else { 1 };
            self.studio_new();
        }
        Ok(txt(format!("{}\nIntérieur ouvert dans le studio : « {} ».", self.status, self.studio.mlo)))
    }

    fn mcp_add_room(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let (t, m) = self.find_interior(a["interior"].as_str().unwrap_or(""))?;
        let (x0, y0, x1, y1) = (num(a, "x0").unwrap_or(0.0), num(a, "y0").unwrap_or(0.0), num(a, "x1").unwrap_or(0.0), num(a, "y1").unwrap_or(0.0));
        let (x0, x1, y0, y1) = (x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1));
        if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
            return Err("Pièce trop petite (au moins 1 m de côté).".into());
        }
        let mlo = &self.proj.ytyps[t].mlos[m];
        if mlo.rooms.len() >= 32 {
            return Err("31 pièces au plus dans un intérieur.".into());
        }
        // hauteur : celle des pièces de l'étage (la plus basse au sol de l'étage demandé), sinon 0 et 2,75 m
        let floor = a["floor"].as_u64().unwrap_or(0) as usize;
        let mut levels: Vec<(f32, f32)> = mlo.rooms.iter().skip(1).map(|r| (r.bb_min.z, r.bb_max.z - r.bb_min.z)).collect();
        levels.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));
        levels.dedup_by(|p, q| (p.0 - q.0).abs() < 0.6);
        let (lz, lh) = levels.get(floor).copied().unwrap_or((floor as f32 * 3.0, 2.75));
        // pièce du jeu à cet étage : son vrai sol (la boîte de Rockstar descend dessous), même plafond
        let game_room = mlo.rooms.iter().enumerate().skip(1).find(|(_, r)| r.game && (r.bb_min.z - lz).abs() < 0.6).map(|(i, r)| (i, r.bb_max.z));
        let (lz, lh) = match game_room {
            Some((gi, top)) => match self.game_room_floor(t, m, gi) {
                Some(f) => (f, (top - f).max(2.2)),
                None => (lz, lh),
            },
            None => (lz, lh),
        };
        let mlo = &self.proj.ytyps[t].mlos[m];
        let z0 = num(a, "z0").unwrap_or(lz);
        let h = num(a, "height").unwrap_or(lh).clamp(2.0, 8.0);
        let (mn, mx) = (v3(x0, y0, z0), v3(x1, y1, z0 + h));
        let same_level = |r: &crate::model::Room| r.bb_min.z < mx.z - 0.1 && r.bb_max.z > mn.z + 0.1;
        if let Some((i, r)) = mlo.rooms.iter().enumerate().skip(1).find(|(_, r)| same_level(r) && mn.x < r.bb_max.x - 0.01 && mx.x > r.bb_min.x + 0.01 && mn.y < r.bb_max.y - 0.01 && mx.y > r.bb_min.y + 0.01) {
            return Err(format!("Cette pièce chevaucherait la pièce {i} « {} » (x {:.2}..{:.2}, y {:.2}..{:.2}).", r.name, r.bb_min.x, r.bb_max.x, r.bb_min.y, r.bb_max.y));
        }
        let connect: Option<Vec<usize>> = match a["connect"].as_array() {
            Some(list) => Some(list.iter().map(|v| self.room_arg(t, m, v)).collect::<Result<Vec<usize>, String>>()?),
            None => None,
        };
        self.push_undo();
        let mlo = &mut self.proj.ytyps[t].mlos[m];
        let ri = mlo.rooms.len();
        let name = a["name"].as_str().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("piece_{ri}"));
        mlo.rooms.push(crate::model::Room::new(&name, mn, mx));
        let mut doors = Vec::new();
        for oi in 1..ri {
            if !same_level(&mlo.rooms[oi]) || connect.as_ref().map(|c| !c.contains(&oi)).unwrap_or(false) {
                continue;
            }
            let zf = mlo.rooms[oi].bb_min.z.max(z0);
            if let Some(c) = crate::plan::shared_door(&mlo.rooms[oi], &mlo.rooms[ri], zf) {
                mlo.portals.push(crate::model::Portal::new(oi as u32, ri as u32, c));
                doors.push(format!("{oi} « {} »", mlo.rooms[oi].name));
            }
        }
        let warn = if doors.is_empty() { " Attention : elle ne touche aucune pièce (mur commun d'au moins 1,4 m) : pas de passage, on ne peut pas y entrer." } else { "" };
        Ok(txt(format!(
            "Pièce {ri} « {name} » ajoutée : x {x0:.2}..{x1:.2}, y {y0:.2}..{y1:.2}, sol z {z0:.2}, hauteur {h:.2} (murs, sol et plafond générés). Passages vers : {}.{warn}",
            if doors.is_empty() { "aucune".to_string() } else { doors.join(", ") }
        )))
    }

    fn mcp_furnish(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let (t, m) = self.find_interior(a["interior"].as_str().unwrap_or(""))?;
        let ri = self.room_arg(t, m, &a["room"])?;
        let kind = kind_arg(&a["kind"]).ok_or("Type de pièce inconnu (voir list_room_types).")?;
        let theme = a["theme"].as_u64().unwrap_or(0).min(4) as usize;
        let seed = a["seed"].as_u64().unwrap_or(self.frame.wrapping_mul(6364136223846793005) | 1);
        self.push_undo();
        let (n, msg) = self.decorate_room(t, m, ri, kind, theme, seed, a["keep_existing"].as_bool().unwrap_or(false), true);
        let mut s = format!("{msg}\n{n} objet(s) posé(s). Style : {}.", THEMES[theme]);
        let ids: Vec<String> = self.proj.ytyps[t].mlos[m].entities.iter().filter(|e| e.room == ri && !e.archetype.starts_with("dw_")).map(|e| format!("{} {}", e.id, e.archetype)).collect();
        s += &format!("\nObjets de la pièce : {}", ids.join(", "));
        Ok(txt(s))
    }

    fn mcp_place(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let model = a["model"].as_str().unwrap_or("").trim().to_string();
        if model.is_empty() {
            return Err("Indique le modèle (voir search_props).".into());
        }
        if let Some(w) = self.game.world.as_ref() {
            if !w.archetypes.contains_key(&crate::gta::crypto::joaat(&model.to_lowercase())) && self.sollumz.find(&model).is_none() {
                return Err(format!("Le modèle « {model} » n'existe ni dans le jeu ni dans « Mes modèles » : cherche-le avec search_props."));
            }
        }
        let (x, y) = (num(a, "x").unwrap_or(0.0), num(a, "y").unwrap_or(0.0));
        let heading = num(a, "heading").unwrap_or(0.0);
        self.push_undo();
        if let Some(iname) = a["interior"].as_str() {
            let (t, m) = self.find_interior(iname)?;
            let ri = if a["room"].is_null() { crate::model::room_at(&self.proj.ytyps[t].mlos[m], v3(x, y, num(a, "z").unwrap_or(0.0) + 0.5)) } else { self.room_arg(t, m, &a["room"])? };
            let floor = self.proj.ytyps[t].mlos[m].rooms.get(ri).map(|r| r.bb_min.z).unwrap_or(0.0);
            // z absent : le bas du modèle sur le sol de la pièce
            let z = num(a, "z").unwrap_or_else(|| floor - self.decor_dims(&model).map(|d| d.1.z).unwrap_or(0.0));
            let mut e = crate::model::Entity::new(&model, v3(x, y, z));
            e.flags = crate::mech::MLO_ENTITY_FLAGS;
            e.room = ri;
            e.rot = Quat::from_z(heading.to_radians());
            let id = e.id;
            self.proj.ytyps[t].mlos[m].entities.push(e);
            self.sync_game_project();
            return Ok(txt(format!("Posé : id {id}, {model} dans la pièce {ri} de « {iname} » en ({x:.2}, {y:.2}, {z:.2}), cap {heading:.0}°.")));
        }
        let z = num(a, "z").unwrap_or(self.ground_z);
        let id = self.place_prop(&model, v3(x, y, z), Some(heading));
        if num(a, "z").is_none() {
            self.snap_ids(&[id]);
        }
        let p = self.proj.find_id(id).and_then(|r| self.proj.world_entity(r)).map(|e| e.pos).unwrap_or(V3::ZERO);
        self.sync_game_project();
        Ok(txt(format!("Posé : id {id}, {model} en ({:.2}, {:.2}, {:.2}), cap {heading:.0}°.", p.x, p.y, p.z)))
    }

    fn mcp_move(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let id = a["id"].as_u64().ok_or("id manquant")?;
        let r = self.proj.find_id(id).ok_or("Objet introuvable.")?;
        self.push_undo();
        if let Some(e) = self.proj.ent_mut(r) {
            if let Some(x) = num(a, "x") {
                e.pos.x = x;
            }
            if let Some(y) = num(a, "y") {
                e.pos.y = y;
            }
            if let Some(z) = num(a, "z") {
                e.pos.z = z;
            }
            if let Some(h) = num(a, "heading") {
                e.rot = Quat::from_z(h.to_radians());
            }
        }
        if a["ground"].as_bool() == Some(true) {
            self.snap_ids(&[id]);
        }
        self.sync_game_project();
        let e = self.proj.ent(r).cloned().ok_or("Objet introuvable.")?;
        Ok(txt(format!("{} {} en ({:.2}, {:.2}, {:.2}), cap {:.0}°.", id, e.archetype, e.pos.x, e.pos.y, e.pos.z, e.rot.heading_deg())))
    }

    fn mcp_camera(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        if let Some(n) = a["interior"].as_str() {
            let (t, m) = self.find_interior(n)?;
            let name = self.proj.ytyps[t].mlos[m].name.clone();
            let (p, _) = self.proj.mlo_placement(&name).ok_or("Cet intérieur n'est pas placé.")?;
            self.cam.target = p;
        }
        if let (Some(x), Some(y)) = (num(a, "x"), num(a, "y")) {
            self.cam.target = v3(x, y, num(a, "z").unwrap_or(self.cam.target.z));
        }
        if let Some(d) = num(a, "dist") {
            self.cam.dist = d.clamp(1.0, 5000.0);
            self.cam.ortho = self.cam.dist;
        }
        if let Some(y) = num(a, "yaw") {
            self.cam.yaw = y.to_radians();
        }
        if let Some(p) = num(a, "pitch") {
            self.cam.pitch = p.to_radians().clamp(0.05, 1.55);
        }
        if let Some(t) = a["top"].as_bool() {
            self.cam.map_view = t;
        }
        self.cam.snap();
        Ok(txt(format!("Caméra sur ({:.1}, {:.1}, {:.1}), distance {:.0} m{}.", self.cam.target.x, self.cam.target.y, self.cam.target.z, self.cam.dist, if self.cam.map_view { ", plan vu de dessus" } else { "" })))
    }
}

fn mcp_room_types() -> String {
    let mut s = String::from("Types de pièce (numéro : nom, styles) :\n");
    for (k, (n, _)) in KINDS.iter().enumerate() {
        let th: Vec<&str> = decor::themes_for(k).iter().map(|t| THEMES[*t]).collect();
        s += &format!("  {k} : {n} ({})\n", th.join(", "));
    }
    s += "Styles : 0 au hasard, 1 moderne, 2 classique, 3 luxe, 4 industriel.\nIntérieurs complets tout prêts (template) :\n";
    for (i, g) in crate::gabarits::GABARITS.iter().enumerate() {
        let rooms: Vec<&str> = g.rooms.iter().map(|r| r.0).collect();
        s += &format!("  {i} : {} ({})\n", g.name, rooms.join(", "));
    }
    s
}

/// Image de la fenêtre réduite à `w` de large, en PNG pour MCP.
fn encode_shot(img: &egui::ColorImage, w: u32) -> Result<Vec<Value>, String> {
    use image::ImageEncoder;
    let [iw, ih] = img.size;
    let mut rgb = Vec::with_capacity(iw * ih * 3);
    for p in &img.pixels {
        rgb.extend_from_slice(&[p.r(), p.g(), p.b()]);
    }
    let buf = image::RgbImage::from_raw(iw as u32, ih as u32, rgb).ok_or("image vide")?;
    let buf = if iw as u32 > w { image::imageops::resize(&buf, w, (ih as u32 * w / iw as u32).max(1), image::imageops::FilterType::Triangle) } else { buf };
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(buf.as_raw(), buf.width(), buf.height(), image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())?;
    Ok(vec![json!({"type": "image", "data": b64(&png), "mimeType": "image/png"}), json!({"type": "text", "text": format!("Vue de DragoWalker ({} × {}).", buf.width(), buf.height())})])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_standard() {
        assert_eq!(b64(b""), "");
        assert_eq!(b64(b"f"), "Zg==");
        assert_eq!(b64(b"fo"), "Zm8=");
        assert_eq!(b64(b"foo"), "Zm9v");
        assert_eq!(b64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn outils_decrits() {
        let t = tools();
        assert!(t.len() >= 37);
        let names: std::collections::HashSet<&str> = t.iter().map(|x| x["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), t.len(), "noms d'outils en double");
        for n in crate::mcp_plus::NAMES {
            assert!(names.contains(n), "{n}");
        }
        for x in &t {
            assert!(x["name"].as_str().unwrap().chars().all(|c| c.is_ascii_lowercase() || c == '_'));
            assert_eq!(x["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn types_par_nom() {
        assert_eq!(kind_arg(&json!(2)), Some(decor::K_CUISINE));
        assert_eq!(kind_arg(&json!("Salle de bain")), Some(decor::K_SDB));
        assert_eq!(kind_arg(&json!("commissariat cellule")), Some(decor::K_CELLULE));
        assert_eq!(kind_arg(&json!(99)), None);
    }
}
