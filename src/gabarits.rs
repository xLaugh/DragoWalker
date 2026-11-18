//! Intérieurs complets tout prêts (Studio MLO) : « Appartement T2 », « Commissariat », « Bar »… DragoWalker crée
//! les pièces, les passages entre elles, la porte d'entrée, les vraies portes du jeu, l'ambiance et l'aménagement de
//! chaque pièce ; tout se retouche ensuite comme un intérieur fait à la main.

use crate::app::DwApp;
use crate::decor::*;
use crate::look::ic;
use crate::math::{v3, V3};

/// Un intérieur tout prêt : nom, icône, pièces (nom, type d'aménagement, rectangle x0 y0 x1 y1 en m), passages
/// (pièces reliées, indices dans la liste) ; l'entrée est au milieu du mur sud de la première pièce.
pub struct Gabarit {
    pub name: &'static str,
    pub icon: &'static str,
    pub slug: &'static str,
    pub rooms: &'static [(&'static str, usize, [f32; 4])],
    pub doors: &'static [(usize, usize)],
}

pub const GABARITS: &[Gabarit] = &[
    Gabarit {
        name: "Appartement T2",
        icon: ic::HOUSE_LINE,
        slug: "appartement",
        rooms: &[("salon", K_SALON, [0.0, 0.0, 7.0, 5.0]), ("cuisine", K_CUISINE, [7.0, 0.0, 10.0, 5.0]), ("chambre", K_CHAMBRE, [0.0, 5.0, 5.0, 9.0]), ("salle_de_bain", K_SDB, [5.0, 5.0, 8.0, 9.0])],
        doors: &[(0, 1), (0, 2), (0, 3)],
    },
    Gabarit {
        name: "Maison familiale",
        icon: ic::HOUSE,
        slug: "maison",
        rooms: &[
            ("salon", K_SALON, [0.0, 0.0, 8.0, 5.5]),
            ("cuisine", K_CUISINE, [8.0, 0.0, 11.0, 5.5]),
            ("chambre", K_CHAMBRE, [0.0, 5.5, 4.0, 9.5]),
            ("salle_de_bain", K_SDB, [4.0, 5.5, 6.5, 9.5]),
            ("chambre_enfant", K_ENFANT, [6.5, 5.5, 10.0, 9.5]),
        ],
        doors: &[(0, 1), (0, 2), (0, 3), (0, 4)],
    },
    Gabarit {
        name: "Bureaux d'entreprise",
        icon: ic::BUILDINGS,
        slug: "bureaux",
        rooms: &[("accueil", K_ACCUEIL, [0.0, 0.0, 6.0, 5.0]), ("open_space", K_OPEN, [6.0, 0.0, 14.0, 7.0]), ("bureau_direction", K_BUREAU, [0.0, 5.0, 6.0, 9.0]), ("reunion", K_REUNION, [6.0, 7.0, 14.0, 11.0])],
        doors: &[(0, 1), (0, 2), (1, 3)],
    },
    Gabarit {
        name: "Commissariat",
        icon: ic::SIREN,
        slug: "commissariat",
        rooms: &[
            ("accueil", K_ACCUEIL, [0.0, 0.0, 7.0, 6.0]),
            ("open_space", K_OPEN, [7.0, 0.0, 15.0, 6.0]),
            ("bureau_chef", K_BUREAU, [0.0, 6.0, 4.0, 10.0]),
            ("vestiaire", K_VESTIAIRE, [4.0, 6.0, 8.0, 10.0]),
            ("armurerie", K_ARMURERIE, [8.0, 6.0, 11.0, 10.0]),
            ("cellule_1", K_CELLULE, [11.0, 6.0, 13.0, 10.0]),
            ("cellule_2", K_CELLULE, [13.0, 6.0, 15.0, 10.0]),
        ],
        doors: &[(0, 1), (0, 2), (0, 3), (1, 4), (1, 5), (1, 6)],
    },
    Gabarit {
        name: "Bar",
        icon: ic::BEER_STEIN,
        slug: "bar",
        rooms: &[("bar", K_BAR, [0.0, 0.0, 10.0, 7.0]), ("reserve", K_ENTREPOT, [0.0, 7.0, 4.0, 10.0]), ("toilettes", K_WC, [4.0, 7.0, 7.0, 10.0])],
        doors: &[(0, 1), (0, 2)],
    },
    Gabarit {
        name: "Restaurant",
        icon: ic::HAMBURGER,
        slug: "restaurant",
        rooms: &[("restaurant", K_RESTO, [0.0, 0.0, 10.0, 7.0]), ("cuisine", K_CUISINE, [0.0, 7.0, 6.0, 11.0]), ("toilettes", K_WC, [6.0, 7.0, 9.0, 10.0])],
        doors: &[(0, 1), (0, 2)],
    },
    Gabarit {
        name: "Supérette",
        icon: ic::STOREFRONT,
        slug: "superette",
        rooms: &[("magasin", K_SHOP, [0.0, 0.0, 10.0, 8.0]), ("reserve", K_ENTREPOT, [0.0, 8.0, 5.0, 11.0]), ("bureau", K_BUREAU, [5.0, 8.0, 9.0, 11.0])],
        doors: &[(0, 1), (0, 2)],
    },
    Gabarit {
        name: "Boîte de nuit",
        icon: ic::MUSIC_NOTES,
        slug: "boite_de_nuit",
        rooms: &[("club", K_CLUB, [0.0, 0.0, 14.0, 10.0]), ("toilettes", K_WC, [0.0, 10.0, 4.0, 13.0]), ("bureau_gerant", K_BUREAU, [4.0, 10.0, 9.0, 14.0]), ("reserve", K_ENTREPOT, [9.0, 10.0, 14.0, 13.0])],
        doors: &[(0, 1), (0, 2), (0, 3)],
    },
    Gabarit {
        name: "Clinique",
        icon: ic::HOSPITAL,
        slug: "clinique",
        rooms: &[("accueil", K_ACCUEIL, [0.0, 0.0, 8.0, 5.0]), ("salle_de_soins_1", K_HOPITAL, [0.0, 5.0, 5.0, 9.5]), ("salle_de_soins_2", K_HOPITAL, [5.0, 5.0, 10.0, 9.5])],
        doors: &[(0, 1), (0, 2)],
    },
    Gabarit {
        name: "Garage",
        icon: ic::WRENCH,
        slug: "garage",
        rooms: &[("garage", K_GARAGE, [0.0, 0.0, 10.0, 8.0]), ("bureau", K_BUREAU, [10.0, 0.0, 14.0, 4.0]), ("vestiaire", K_VESTIAIRE, [10.0, 4.0, 14.0, 8.0])],
        doors: &[(0, 1), (0, 2)],
    },
    Gabarit {
        name: "Salon de coiffure",
        icon: ic::SCISSORS,
        slug: "coiffeur",
        rooms: &[("salon_de_coiffure", K_COIFFEUR, [0.0, 0.0, 9.0, 6.0]), ("buanderie", K_BUANDERIE, [0.0, 6.0, 4.0, 9.0]), ("toilettes", K_WC, [4.0, 6.0, 7.0, 9.0])],
        doors: &[(0, 1), (0, 2)],
    },
    Gabarit {
        name: "Armurerie",
        icon: ic::CROSSHAIR,
        slug: "armurerie",
        rooms: &[("armurerie", K_ARMURERIE, [0.0, 0.0, 9.0, 6.0]), ("reserve", K_ENTREPOT, [0.0, 6.0, 5.0, 9.0]), ("bureau", K_BUREAU, [5.0, 6.0, 9.0, 9.0])],
        doors: &[(0, 1), (0, 2)],
    },
    Gabarit {
        name: "Labo clandestin",
        icon: ic::FLASK,
        slug: "labo",
        rooms: &[("labo", K_LABO, [0.0, 0.0, 9.0, 7.0]), ("vestiaire", K_VESTIAIRE, [0.0, 7.0, 4.0, 10.0]), ("stock", K_ENTREPOT, [4.0, 7.0, 9.0, 10.0])],
        doors: &[(0, 1), (0, 2)],
    },
];

/// Hauteur des pièces des intérieurs tout prêts (m), comme un intérieur créé à la main.
const ROOM_H: f32 = 3.0;

impl DwApp {
    /// Crée l'intérieur tout prêt `gi` (style `theme`, 0 = au hasard), l'ouvre dans le studio et aménage chaque pièce.
    pub fn studio_new_gabarit(&mut self, gi: usize, theme: usize) {
        let Some(g) = GABARITS.get(gi) else { return };
        let name = if self.studio.new_name.trim().is_empty() {
            let mut i = 1;
            loop {
                let n = format!("dw_{}_{i}", g.slug);
                if self.proj.find_mlo(&n).is_none() {
                    break n;
                }
                i += 1;
            }
        } else {
            self.studio.new_name.trim().to_string()
        };
        self.create_mlo(&name, 1, 1, 6.0, 5.0, ROOM_H);
        let Some((ti, mi)) = self.cur_mlo else { return };
        // pièces centrées sur l'origine de l'intérieur
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (_, _, r) in g.rooms {
            x0 = x0.min(r[0]);
            y0 = y0.min(r[1]);
            x1 = x1.max(r[2]);
            y1 = y1.max(r[3]);
        }
        let (ox, oy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
        {
            let m = &mut self.proj.ytyps[ti].mlos[mi];
            m.rooms.truncate(1);
            m.portals.clear();
            m.entities.clear();
            for (n, _, r) in g.rooms {
                m.rooms.push(crate::model::Room::new(n, v3(r[0] - ox, r[1] - oy, 0.0), v3(r[2] - ox, r[3] - oy, ROOM_H)));
            }
            for &(a, b) in g.doors {
                if let Some(c) = crate::plan::shared_door(&m.rooms[a + 1], &m.rooms[b + 1], 0.0) {
                    m.portals.push(crate::model::Portal::new(a as u32 + 1, b as u32 + 1, c));
                }
            }
            // porte d'entrée : au milieu du mur sud de la première pièce
            let r = &m.rooms[1];
            let (cx, y) = ((r.bb_min.x + r.bb_max.x) * 0.5, r.bb_min.y);
            let (hw, h) = (crate::plan::DOOR_W * 0.6, crate::plan::DOOR_H);
            m.portals.push(crate::model::Portal::new(1, 0, [v3(cx - hw, y, 0.0), v3(cx - hw, y, h), v3(cx + hw, y, h), v3(cx + hw, y, 0.0)]));
        }
        // coque modelable : sol, murs et couleurs de chaque pièce, plinthes, éclairage cuit dans les coins
        let kinds: Vec<usize> = g.rooms.iter().map(|r| r.1).collect();
        self.gabarit_shell(ti, mi, &kinds);
        let mname = self.proj.ytyps[ti].mlos[mi].name.clone();
        for y in &mut self.proj.ymaps {
            for e in &mut y.entities {
                if e.archetype.eq_ignore_ascii_case(&mname) {
                    if let Some(d) = e.mlo.as_mut() {
                        d.num_exit_portals = 1;
                    }
                }
            }
        }
        self.studio.new_name.clear();
        self.studio_enter(&mname);
        // vraies portes du jeu dans les passages (jeu chargé), puis l'aménagement de chaque pièce, d'un même style
        let doors = self.plan_doors();
        let theme = if theme == 0 { [1usize, 2, 3][(self.frame as usize) % 3] } else { theme };
        let seed = self.frame.wrapping_mul(6364136223846793005) | 1;
        let mut n = 0;
        for (i, (_, kind, _)) in g.rooms.iter().enumerate() {
            n += self.decorate_room(ti, mi, i + 1, *kind, theme, seed ^ (i as u64 * 7919), false, true).0;
        }
        self.status = format!("« {} » créé : {} pièce(s), {n} objet(s) posés, style {}. {doors}", g.name, g.rooms.len(), THEMES[theme]);
    }
}

/// Murs d'une pièce selon son type : (texture WALL_TEX, couleur), comme les intérieurs du jeu (enduits beiges et
/// grèges dans les logements, carrelage dans les pièces d'eau, béton dans les garages, brique sombre dans les bars,
/// boiseries dans les restaurants) — **estimé** d'après les intérieurs du jeu (v_michael, v_franklins, v_31_tun,
/// v_recycle, v_ilev_ra…).
pub fn wall_look(kind: usize) -> (u8, Option<[u8; 3]>) {
    use crate::decor::*;
    match kind {
        K_SALON => (0, Some([214, 200, 178])),
        K_SAM => (0, Some([222, 206, 180])),
        K_CHAMBRE => (0, Some([196, 204, 192])),
        K_ENFANT => (0, Some([196, 214, 228])),
        K_CUISINE | K_SDB | K_WC | K_BUANDERIE | K_VESTIAIRE => (4, Some([232, 234, 232])),
        K_BUREAU | K_OPEN | K_REUNION | K_ACCUEIL => (0, Some([216, 218, 214])),
        K_BAR | K_CLUB => (1, Some([118, 72, 62])),
        K_RESTO => (3, Some([168, 124, 92])),
        K_SHOP | K_HOPITAL => (0, Some([236, 238, 236])),
        K_COIFFEUR => (0, Some([232, 220, 212])),
        K_GARAGE | K_ENTREPOT | K_LABO | K_GYM | K_ARMURERIE | K_CELLULE => (2, None),
        _ => (0, Some([220, 212, 196])),
    }
}

/// Faux plafond (m sous le plafond) des pièces de bureau — **estimé**.
const OFFICE_DROP: f32 = 0.3;
/// Épaisseur des cloisons et des murs extérieurs des intérieurs tout prêts (m).
const PART_T: f32 = 0.12;
const OUTER_T: f32 = 0.2;
/// Face extérieure des murs du tour (vue de dehors à travers la porte d'entrée).
const OUTSIDE: (u8, Option<[u8; 3]>) = (0, Some([200, 196, 188]));

impl DwApp {
    /// Les pièces rectangulaires d'un intérieur tout prêt deviennent une coque modelable (design.rs) : un sol par
    /// pièce, un mur par limite (une seule cloison entre deux pièces, chaque face à la couleur de sa pièce), les
    /// passages percés dans les murs, plinthes et éclairage cuit. L'aménagement qui suit choisit le sol (decor::look).
    pub fn gabarit_shell(&mut self, ti: usize, mi: usize, kinds: &[usize]) {
        let n = self.proj.ytyps[ti].mlos[mi].rooms.len();
        let sel: Vec<(usize, usize, Option<u8>)> = (1..n).map(|ri| (ri, kinds.get(ri - 1).copied().unwrap_or(0), None)).collect();
        self.rooms_to_design(ti, mi, &sel, false);
    }

    /// Pièces rectangulaires choisies (pièce, type de pièce pour les murs, sol imposé) → coque modelable ; `append` :
    /// ajoutées à la coque déjà là (sinon elle est refaite).
    pub fn rooms_to_design(&mut self, ti: usize, mi: usize, sel: &[(usize, usize, Option<u8>)], append: bool) {
        let m = &mut self.proj.ytyps[ti].mlos[mi];
        let rects: Vec<(String, [f32; 4], f32, f32)> = sel.iter().filter_map(|s| m.rooms.get(s.0)).map(|r| (r.name.clone(), [r.bb_min.x, r.bb_min.y, r.bb_max.x, r.bb_max.y], r.bb_min.z, r.bb_max.z - r.bb_min.z)).collect();
        for s in sel {
            if let Some(r) = m.rooms.get_mut(s.0) {
                r.free = true;
            }
        }
        let holes: Vec<[V3; 4]> = m.portals.iter().flat_map(|p| p.holes()).collect();
        let mname = m.name.clone();
        let kind_of = |i: usize| sel.get(i).map(|s| s.1).unwrap_or(0);
        // limites : (axe 0 : x constant / 1 : y constant, valeur au cm) → (début, fin, pièce, côté + ?)
        let mut lines: std::collections::BTreeMap<(u8, i32), Vec<(f32, f32, usize, bool)>> = std::collections::BTreeMap::new();
        for (i, (_, r, _, _)) in rects.iter().enumerate() {
            let k = |v: f32| (v * 100.0).round() as i32;
            // pièce du côté + de la limite : x1 → côté −, x0 → côté +
            lines.entry((0, k(r[0]))).or_default().push((r[1], r[3], i, true));
            lines.entry((0, k(r[2]))).or_default().push((r[1], r[3], i, false));
            lines.entry((1, k(r[1]))).or_default().push((r[0], r[2], i, true));
            lines.entry((1, k(r[3]))).or_default().push((r[0], r[2], i, false));
        }
        let (z0, h) = rects.first().map(|r| (r.2, r.3)).unwrap_or((0.0, ROOM_H));
        let mut walls: Vec<crate::design::DWall> = Vec::new();
        for ((axis, kv), ivs) in &lines {
            let c = *kv as f32 / 100.0;
            let mut cuts: Vec<f32> = ivs.iter().flat_map(|v| [v.0, v.1]).collect();
            cuts.sort_by(|a, b| a.total_cmp(b));
            cuts.dedup_by(|a, b| (*a - *b).abs() < 0.005);
            // morceaux élémentaires : (début, fin, pièce côté +, pièce côté −)
            let mut pieces: Vec<(f32, f32, Option<usize>, Option<usize>)> = Vec::new();
            for w in cuts.windows(2) {
                let mid = (w[0] + w[1]) * 0.5;
                let side = |plus: bool| ivs.iter().find(|v| v.3 == plus && v.0 <= mid && v.1 >= mid).map(|v| v.2);
                let (p, n) = (side(true), side(false));
                if p.is_none() && n.is_none() {
                    continue;
                }
                match pieces.last_mut() {
                    Some(last) if (last.1 - w[0]).abs() < 0.005 && last.2 == p && last.3 == n => last.1 = w[1],
                    _ => pieces.push((w[0], w[1], p, n)),
                }
            }
            for (u0, u1, plus, minus) in pieces {
                // le mur va dans le sens croissant ; sa face avant (normale à gauche) regarde vers −x (axe 0) ou +y
                // (axe 1) : la pièce de ce côté-là
                let (a, b, front, back) = if *axis == 0 { ([c, u0], [c, u1], minus, plus) } else { ([u0, c], [u1, c], plus, minus) };
                let look = |r: Option<usize>| r.map(|i| wall_look(kind_of(i))).unwrap_or(OUTSIDE);
                let (f, bk) = (look(front), look(back));
                let tiled = |r: Option<usize>| r.map(|i| wall_look(kind_of(i)).0 == 4).unwrap_or(true);
                let outer = front.is_none() || back.is_none();
                let mut dholes = Vec::new();
                for hc in &holes {
                    let ctr = (hc[0] + hc[1] + hc[2] + hc[3]) * 0.25;
                    let (along, across) = if *axis == 0 { (ctr.y, ctr.x) } else { (ctr.x, ctr.y) };
                    if (across - c).abs() > 0.05 || along < u0 || along > u1 {
                        continue;
                    }
                    let (mut lo, mut hi, mut zl, mut zh) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                    for q in hc {
                        let s = if *axis == 0 { q.y } else { q.x };
                        lo = lo.min(s);
                        hi = hi.max(s);
                        zl = zl.min(q.z);
                        zh = zh.max(q.z);
                    }
                    if hi - lo < 0.3 || zh - zl < 0.3 {
                        continue; // passage horizontal (trémie) : pas dans un mur
                    }
                    dholes.push(crate::design::DHole { s: (lo + hi) * 0.5 - u0, w: hi - lo, sill: (zl - z0).max(0.0), h: zh - zl, kind: 0 });
                }
                walls.push(crate::design::DWall { a, b, z0, h, t: if outer { OUTER_T } else { PART_T }, tex: f.0, color: f.1, base: !(tiled(front) && tiled(back)), holes: dholes, back: Some(bk) });
            }
        }
        let d = self.design_mut(&mname);
        if !append {
            d.rooms.clear();
            d.walls.clear();
            d.lights.clear();
            d.baked = true;
        }
        d.walls.extend(walls);
        for (i, (name, r, rz, rh)) in rects.iter().enumerate() {
            let office = matches!(kind_of(i), crate::decor::K_BUREAU | crate::decor::K_OPEN | crate::decor::K_REUNION | crate::decor::K_ACCUEIL | crate::decor::K_HOPITAL);
            d.rooms.retain(|x| x.room != *name);
            d.rooms.push(crate::design::DRoom { room: name.clone(), pts: vec![[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]], z0: *rz, h: *rh, floor: sel.get(i).and_then(|s| s.2).unwrap_or(0), ceil_color: None, drop: if office { OFFICE_DROP } else { 0.0 } });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pieces_sans_chevauchement_et_passages_valides() {
        for g in GABARITS {
            assert!(g.rooms.len() < 31, "{}", g.name);
            for (i, a) in g.rooms.iter().enumerate() {
                for b in g.rooms.iter().skip(i + 1) {
                    let (ra, rb) = (a.2, b.2);
                    let over = ra[0] < rb[2] - 0.01 && rb[0] < ra[2] - 0.01 && ra[1] < rb[3] - 0.01 && rb[1] < ra[3] - 0.01;
                    assert!(!over, "{} : {} et {} se chevauchent", g.name, a.0, b.0);
                }
            }
            let room = |r: [f32; 4]| crate::model::Room::new("x", v3(r[0], r[1], 0.0), v3(r[2], r[3], ROOM_H));
            for &(a, b) in g.doors {
                assert!(crate::plan::shared_door(&room(g.rooms[a].2), &room(g.rooms[b].2), 0.0).is_some(), "{} : pas de passage possible entre {} et {}", g.name, g.rooms[a].0, g.rooms[b].0);
            }
            // chaque pièce est reliée à l'entrée
            let mut seen = vec![false; g.rooms.len()];
            seen[0] = true;
            for _ in 0..g.rooms.len() {
                for &(a, b) in g.doors {
                    if seen[a] || seen[b] {
                        seen[a] = true;
                        seen[b] = true;
                    }
                }
            }
            assert!(seen.iter().all(|s| *s), "{} : pièce sans passage", g.name);
        }
    }
}
