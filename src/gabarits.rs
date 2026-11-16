//! Intérieurs complets tout prêts (Studio MLO) : « Appartement T2 », « Commissariat », « Bar »… DragoWalker crée
//! les pièces, les passages entre elles, la porte d'entrée, les vraies portes du jeu, l'ambiance et l'aménagement de
//! chaque pièce ; tout se retouche ensuite comme un intérieur fait à la main.

use crate::app::DwApp;
use crate::decor::*;
use crate::look::ic;
use crate::math::v3;

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
