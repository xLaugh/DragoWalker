//! Circulation : réseau de points de route (nœuds de véhicules, fichiers nodesNNN.ynd) généré depuis les routes
//! tracées dans le créateur de terrain, pour que les voitures du jeu y roulent.
//!
//! Format relevé sur les fichiers du jeu (nodes464.ynd, Los Santos, converti en XML par YndXml.GetXml) :
//! - un fichier par carré de 512 m : AreaID = ⌊(y + 8192) / 512⌋ × 32 + ⌊(x + 8192) / 512⌋ ;
//! - nœud : AreaID, NodeID (numérotés dans le carré), Position, Flags0..5 ; nœud de rue ordinaire le plus
//!   courant : Flags0 2, Flags1 0, Flags2 0, Flags4 8 (densité 8 — YndNode::get_Density 0x06002eee : Flags4 & 15),
//!   Flags5 2 ; Flags0 & 8 = hors route (YndNode::get_OffRoad) pour les chemins de terre ;
//! - lien : ToAreaID, ToNodeID, Flags0, Flags1, Flags2, LinkLength ; voies : Flags2 = avant × 32 + arrière × 4
//!   (YndLink::set_LaneCountForward 0x06002f17 : << 5, set_LaneCountBackward 0x06002f19 : << 2) ;
//!   longueur min(255, distance) (YndLink::UpdateLength 0x06002f24) ;
//! - les liens se font dans les deux sens (CodeWalker : AddLink(node, bidirectional = vrai)).

use crate::math::{v3, V3};
use crate::terrain::Terrain;

/// Voitures de PNJ qui roulent sur les routes tracées : un script client les fait apparaître sur les points de
/// route proches du joueur et rouler au hasard (TaskVehicleDriveWander) ; elles disparaissent quand on s'éloigne.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NpcTraffic {
    pub enabled: bool,
    /// voitures en même temps autour du joueur
    pub count: u32,
    /// modèles de voitures (au hasard parmi eux)
    pub models: Vec<String>,
    /// conducteurs (au hasard parmi eux)
    pub drivers: Vec<String>,
    /// vitesse visée (km/h)
    pub speed_kmh: f32,
    /// style de conduite : 0 prudente, 1 pressée, 2 brûle les feux
    pub style: u8,
    /// rayon autour du joueur (m)
    pub radius: f32,
}

impl Default for NpcTraffic {
    fn default() -> Self {
        NpcTraffic {
            enabled: false,
            count: 10,
            models: vec!["asea".into(), "premier".into(), "blista".into(), "sultan".into(), "granger".into()],
            drivers: vec!["a_m_y_business_01".into(), "a_f_y_business_01".into(), "a_m_y_hipster_01".into(), "a_m_m_beach_01".into()],
            speed_kmh: 50.0,
            style: 0,
            radius: 300.0,
        }
    }
}

/// Styles de conduite de TaskVehicleDriveWander — valeurs d'usage courant dans les scripts FiveM, **estimé**.
pub const DRIVING_STYLES: [(&str, u32); 3] = [("Prudente (respecte le code)", 786603), ("Pressée", 1074528293), ("Brûle les feux", 2883621)];

/// Script client des voitures de PNJ (dw_traffic.lua) à la racine de la ressource ; vrai s'il est écrit.
pub fn export_npc_script(t: &Terrain, root: &std::path::Path) -> Result<bool, String> {
    let cfg = &t.npc_traffic;
    if !cfg.enabled || cfg.models.is_empty() || cfg.drivers.is_empty() {
        return Ok(false);
    }
    let net = build_network(t);
    if net.nodes.is_empty() {
        return Ok(false);
    }
    let mut nodes = String::new();
    for n in &net.nodes {
        // cap du jeu : 0 = nord (+y), sens inverse des aiguilles
        let h = (-n.dir.x).atan2(n.dir.y).to_degrees().rem_euclid(360.0);
        nodes += &format!("    {{{:.1}, {:.1}, {:.2}, {:.0}}},\n", n.pos.x, n.pos.y, n.pos.z, h);
    }
    let list = |v: &[String]| v.iter().map(|m| format!("\"{m}\"")).collect::<Vec<_>>().join(", ");
    let style = DRIVING_STYLES[(cfg.style as usize).min(DRIVING_STYLES.len() - 1)].1;
    let lua = format!(
        r#"-- DragoWalker : voitures de PNJ qui roulent sur les routes de la carte
local CFG = {{ count = {count}, speed = {speed:.2}, style = {style}, radius = {radius:.1} }}
local MODELS = {{ {models} }}
local DRIVERS = {{ {drivers} }}
local NODES = {{
{nodes}}}
local cars = {{}}

local function load(m)
    local h = GetHashKey(m)
    RequestModel(h)
    local t = GetGameTimer()
    while not HasModelLoaded(h) and GetGameTimer() - t < 5000 do Wait(10) end
    return h
end

CreateThread(function()
    while true do
        local me = GetEntityCoords(PlayerPedId())
        -- voitures trop loin : retirées
        for i = #cars, 1, -1 do
            local c = cars[i]
            if not DoesEntityExist(c.veh) or #(GetEntityCoords(c.veh) - me) > CFG.radius * 1.3 then
                if DoesEntityExist(c.ped) then DeleteEntity(c.ped) end
                if DoesEntityExist(c.veh) then DeleteEntity(c.veh) end
                table.remove(cars, i)
            end
        end
        -- une de plus sur un point de route proche, mais pas sous le nez du joueur
        if #cars < CFG.count then
            local cand = {{}}
            for _, n in ipairs(NODES) do
                local d = #(vector3(n[1], n[2], n[3]) - me)
                if d > 60.0 and d < CFG.radius then cand[#cand + 1] = n end
            end
            if #cand > 0 then
                local n = cand[math.random(#cand)]
                local p = vector3(n[1], n[2], n[3])
                local free = true
                for _, c in ipairs(cars) do
                    if DoesEntityExist(c.veh) and #(GetEntityCoords(c.veh) - p) < 12.0 then free = false end
                end
                if free then
                    local vm = load(MODELS[math.random(#MODELS)])
                    local pm = load(DRIVERS[math.random(#DRIVERS)])
                    local veh = CreateVehicle(vm, n[1], n[2], n[3] + 0.5, n[4], false, true)
                    SetVehicleOnGroundProperly(veh)
                    local ped = CreatePedInsideVehicle(veh, 4, pm, -1, false, true)
                    SetBlockingOfNonTemporaryEvents(ped, true)
                    SetDriverAbility(ped, 1.0)
                    TaskVehicleDriveWander(ped, veh, CFG.speed, CFG.style)
                    SetModelAsNoLongerNeeded(vm)
                    SetModelAsNoLongerNeeded(pm)
                    cars[#cars + 1] = {{ veh = veh, ped = ped }}
                end
            end
        end
        Wait(1500)
    end
end)
"#,
        count = cfg.count,
        speed = cfg.speed_kmh / 3.6,
        radius = cfg.radius,
        models = list(&cfg.models),
        drivers = list(&cfg.drivers),
    );
    std::fs::write(root.join("dw_traffic.lua"), lua).map_err(|e| e.to_string())?;
    Ok(true)
}

pub struct TNode {
    pub pos: V3,
    pub off_road: bool,
    /// réglages de la route : densité, vitesse, pas de gros véhicules, autoroute
    pub density: u8,
    pub speed: u8,
    pub no_big: bool,
    pub highway: bool,
    /// direction de la route à ce point (pour le script des voitures de PNJ)
    pub dir: V3,
}

pub struct Network {
    pub nodes: Vec<TNode>,
    /// (a, b, voies dans le sens a → b, voies dans l'autre sens)
    pub links: Vec<(usize, usize, u8, u8)>,
}

/// Écart entre deux points de route le long d'une route (m) — **estimé** (liens de 1 à 43 m dans le jeu, 6 m
/// en médiane en ville ; 15 m suffit pour des routes tracées en courbes douces).
pub const NODE_STEP: f32 = 15.0;
/// Distance maximale pour relier deux routes (bout d'une route sur une autre, croisement) (m) — **estimé**.
pub const JOIN_DIST: f32 = 14.0;

/// Voies par sens selon le type de route : rue 1, avenue 2, autoroute 3, campagne 1, chemin 1.
pub fn lanes(kind: usize) -> u8 {
    match kind {
        1 => 2,
        2 => 3,
        _ => 1,
    }
}

/// Point dans la grille des points de route, comme SpaceNodeGrid::GetCellForPosition (0x0600036a) : cellule =
/// (int)((x + 8192) / 512), arrondi vers zéro, gardée si 0 ≤ cellule < 32 (grille de 32 × 32 cellules de 512 m,
/// constructeur 0x06000368) ; hors de la grille, CodeWalker ne range le point nulle part.
pub fn in_grid(p: V3) -> bool {
    let c = |v: f32| ((v + 8192.0) / 512.0) as i32;
    (0..32).contains(&c(p.x)) && (0..32).contains(&c(p.y))
}

/// Numéro de carré : y × 32 + x (SpaceNodeGrid::GetCell 0x06000369 : x = id % 32, y = id / 32).
pub fn area_of(p: V3) -> u32 {
    let cx = (((p.x + 8192.0) / 512.0).floor() as i32).clamp(0, 31);
    let cy = (((p.y + 8192.0) / 512.0).floor() as i32).clamp(0, 31);
    (cy * 32 + cx) as u32
}

pub fn build_network(t: &Terrain) -> Network {
    let mut nodes: Vec<TNode> = Vec::new();
    let mut links: Vec<(usize, usize, u8, u8)> = Vec::new();
    // nœuds de chaque route : début, tous les ~15 m, fin
    let mut per_road: Vec<Vec<usize>> = Vec::new();
    for r in &t.roads {
        let mut ids = Vec::new();
        if r.samples.len() < 2 || !r.traffic.enabled || r.kind().foot {
            per_road.push(ids);
            continue;
        }
        let len = r.length();
        let n = ((len / NODE_STEP).ceil() as usize).max(if r.ring { 3 } else { 1 });
        let mut k = 0;
        // rond-point : le dernier point est le premier
        for i in 0..=(if r.ring { n - 1 } else { n }) {
            let d = len * i as f32 / n as f32;
            while k + 1 < r.samples.len() - 1 && r.samples[k + 1][3] < d {
                k += 1;
            }
            let (a, b) = (r.samples[k], r.samples[(k + 1).min(r.samples.len() - 1)]);
            let f = if (b[3] - a[3]).abs() > 1e-4 { ((d - a[3]) / (b[3] - a[3])).clamp(0.0, 1.0) } else { 0.0 };
            let tr = &r.traffic;
            nodes.push(TNode {
                pos: v3(a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f + 0.1),
                off_road: r.kind().dirt,
                density: tr.density.min(15),
                speed: tr.speed.min(3),
                no_big: tr.no_big,
                highway: r.kind == 2,
                dir: v3(b[0] - a[0], b[1] - a[1], 0.0).norm(),
            });
            ids.push(nodes.len() - 1);
        }
        let l = lanes(r.kind as usize);
        // sens unique (bretelle, rond-point) : aucune voie dans l'autre sens
        let back = if r.oneway { 0 } else { l };
        for w in ids.windows(2) {
            links.push((w[0], w[1], l, back));
        }
        if r.ring && ids.len() > 2 {
            links.push((ids[ids.len() - 1], ids[0], l, back));
        }
        per_road.push(ids);
    }
    // jonctions : le bout d'une route relié au point le plus proche d'une autre route
    let near = |ids: &[usize], p: V3, nodes: &[TNode]| ids.iter().copied().map(|i| (i, (nodes[i].pos - p).len())).min_by(|a, b| a.1.total_cmp(&b.1));
    for (ri, ids) in per_road.iter().enumerate() {
        // un rond-point n'a pas de bout : ses carrefours le relient
        if t.roads[ri].ring {
            continue;
        }
        for &end in [ids.first(), ids.last()].iter().flatten() {
            let p = nodes[*end].pos;
            for (rj, other) in per_road.iter().enumerate() {
                if ri == rj {
                    continue;
                }
                if let Some((j, d)) = near(other, p, &nodes) {
                    if d < JOIN_DIST && !links.iter().any(|l| (l.0 == *end && l.1 == j) || (l.1 == *end && l.0 == j)) {
                        let l = lanes(t.roads[ri].kind as usize).min(lanes(t.roads[rj].kind as usize));
                        // route à sens unique : on y entre par son début, on en sort par sa fin
                        if t.roads[ri].oneway {
                            if Some(end) == ids.first() {
                                links.push((j, *end, l, 0));
                            } else {
                                links.push((*end, j, l, 0));
                            }
                        } else {
                            links.push((*end, j, l, l));
                        }
                    }
                }
            }
        }
    }
    // croisements : les points les plus proches des deux routes, de part et d'autre du carrefour
    for j in &t.junctions {
        let p = v3(j.pos[0], j.pos[1], j.pos[2]);
        let close: Vec<(usize, usize)> = per_road.iter().enumerate().filter_map(|(ri, ids)| near(ids, p, &nodes).filter(|x| x.1 < JOIN_DIST * 1.5).map(|x| (ri, x.0))).collect();
        for a in 0..close.len() {
            for b in a + 1..close.len() {
                let (na, nb) = (close[a].1, close[b].1);
                if na != nb && !links.iter().any(|l| (l.0 == na && l.1 == nb) || (l.1 == na && l.0 == nb)) {
                    // route à sens unique au carrefour : entrée par son début, sortie par sa fin
                    let (ra, rb) = (close[a].0, close[b].0);
                    let first = |ri: usize, n: usize| per_road[ri].first() == Some(&n);
                    links.push(if t.roads[ra].oneway && !t.roads[ra].ring {
                        if first(ra, na) { (nb, na, 1, 0) } else { (na, nb, 1, 0) }
                    } else if t.roads[rb].oneway && !t.roads[rb].ring {
                        if first(rb, nb) { (na, nb, 1, 0) } else { (nb, na, 1, 0) }
                    } else {
                        (na, nb, 1, 1)
                    });
                }
            }
        }
    }
    Network { nodes, links }
}

/// Fichiers nodesNNN.ynd.xml (format de CodeWalker, NodeDictionary) écrits dans `stream`. Renvoie (carrés écrits,
/// carrés laissés au jeu).
///
/// Un nodesNNN.ynd REMPLACE celui du jeu, et les carrés voisins du jeu sont reliés à ses points par leur numéro
/// (ToAreaID / ToNodeID ; le nodes464.ynd du jeu a 927 points de voiture et 584 de piétons) : un fichier plus court
/// laisse ces liens pointer dans le vide et fait planter le jeu quand il charge la circulation autour du joueur
/// (crash GTA5+E93DB6 environ 10 s après l'apparition, ressource exportée avec 8 carrés du centre de Los Santos).
/// Donc `game_has(carré)` vrai → carré non écrit, et les liens de nos points vers ses points sont retirés.
pub fn export_ynds(t: &Terrain, stream: &std::path::Path, game_has: &dyn Fn(u32) -> bool) -> Result<(usize, usize), String> {
    let mut net = build_network(t);
    if net.nodes.is_empty() {
        return Ok((0, 0));
    }
    let skipped: std::collections::BTreeSet<u32> = net.nodes.iter().filter(|n| in_grid(n.pos)).map(|n| area_of(n.pos)).filter(|a| game_has(*a)).collect();
    // points hors de la grille (au-delà de ±8192 m) : aucun fichier ne peut les porter, comme dans CodeWalker
    let keep = |p: V3| in_grid(p) && !skipped.contains(&area_of(p));
    net.links.retain(|l| keep(net.nodes[l.0].pos) && keep(net.nodes[l.1].pos));
    // numérotation par carré
    let mut ids: Vec<(u32, u32)> = vec![(0, 0); net.nodes.len()];
    let mut per_area: std::collections::BTreeMap<u32, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, n) in net.nodes.iter().enumerate() {
        if keep(n.pos) {
            per_area.entry(area_of(n.pos)).or_default().push(i);
        }
    }
    for (area, list) in &per_area {
        for (k, i) in list.iter().enumerate() {
            ids[*i] = (*area, k as u32);
        }
    }
    // points d'arrêt aux feux et aux stops (Special, rangé dans Flags1 >> 3 : YndNode::set_Special 0x06002edf)
    let specials = crate::signals::node_specials(t, &net.nodes);
    // liens dans les deux sens
    let mut out_links: Vec<Vec<(usize, u8, u8)>> = vec![Vec::new(); net.nodes.len()];
    for (a, b, f, r) in &net.links {
        out_links[*a].push((*b, *f, *r));
        out_links[*b].push((*a, *r, *f));
    }
    for (area, list) in &per_area {
        let mut s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<NodeDictionary>\n <VehicleNodeCount value=\"{}\" />\n <PedNodeCount value=\"0\" />\n <Nodes>\n", list.len());
        for &i in list {
            let n = &net.nodes[i];
            let (ar, id) = ids[i];
            // Flags0 : 2 (rue ordinaire), + 8 hors route (get_OffRoad), + 32 pas de gros véhicules
            // (get_NoBigVehicles 0x06002ed4) ; Flags2 : + 64 autoroute (get_Highway 0x06002ee4) ;
            // Flags4 : densité (get_Density 0x06002eee) ; Flags5 : vitesse << 1 (get_Speed 0x06002ed0)
            let flags0 = 2 | if n.off_road { 8 } else { 0 } | if n.no_big { 32 } else { 0 };
            // carrefour (Flags2 + 4) : comme YndNode::CheckIfJunction (0x06002efb), quand les liens du nœud touchent
            // plus de 3 nœuds distincts, lui compris (au moins 3 voisins) ; set_IsJunction 0x06002ee3 : Flags2 | 4
            let neighbours: std::collections::HashSet<usize> = out_links[i].iter().map(|l| l.0).collect();
            let junction = neighbours.len() + 1 > 3;
            let flags2 = if n.highway { 64 } else { 0 } | if junction { 4 } else { 0 };
            let (flags4, flags5) = (n.density as u32, (n.speed as u32) << 1);
            let flags1 = (specials[i] as u32) << 3;
            s += &format!(
                "  <Item>\n   <AreaID value=\"{ar}\" />\n   <NodeID value=\"{id}\" />\n   <StreetName />\n   <Position x=\"{}\" y=\"{}\" z=\"{}\" />\n   <Flags0 value=\"{flags0}\" />\n   <Flags1 value=\"{flags1}\" />\n   <Flags2 value=\"{flags2}\" />\n   <Flags3 value=\"0\" />\n   <Flags4 value=\"{flags4}\" />\n   <Flags5 value=\"{flags5}\" />\n",
                n.pos.x, n.pos.y, n.pos.z
            );
            if out_links[i].is_empty() {
                s += "   <Links />\n";
            } else {
                s += "   <Links>\n";
                for (to, f, r) in &out_links[i] {
                    let (ta, tid) = ids[*to];
                    let len = (net.nodes[*to].pos - n.pos).len().round().clamp(1.0, 255.0) as u32;
                    let flags2 = (*f as u32 & 7) << 5 | (*r as u32 & 7) << 2;
                    s += &format!("    <Item>\n     <ToAreaID value=\"{ta}\" />\n     <ToNodeID value=\"{tid}\" />\n     <Flags0 value=\"0\" />\n     <Flags1 value=\"0\" />\n     <Flags2 value=\"{flags2}\" />\n     <LinkLength value=\"{len}\" />\n    </Item>\n");
                }
                s += "   </Links>\n";
            }
            s += "  </Item>\n";
        }
        s += " </Nodes>\n <Junctions />\n <JunctionRefs />\n</NodeDictionary>\n";
        std::fs::write(stream.join(format!("nodes{area}.ynd.xml")), s).map_err(|e| e.to_string())?;
    }
    Ok((per_area.len(), skipped.len()))
}
