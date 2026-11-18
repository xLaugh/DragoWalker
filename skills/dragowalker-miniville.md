---
name: dragowalker-miniville
description: Bâtir une ville, un village ou un quartier entier dans DragoWalker (« construis-moi une mini-ville », « un petit village de pêcheurs », « un quartier style Grove Street », « une rue commerçante »). Questions d'abord, puis un plan montré dans la vue et validé par la personne, puis la construction (terrain, routes, bâtiments de l'atelier ou recopiés du jeu, intérieurs, végétation, vie) et la vérification.
---

# Bâtir une mini-ville

Une ville se fait en **quatre temps**, jamais d'un coup : questions → plan montré → accord → construction.
La personne doit pouvoir dire non au plan avant que rien ne soit posé.

## 1. Comprendre la demande (questions)

Lire d'abord `get_scene` et `city_catalog`. Puis, s'il manque une réponse importante, **poser les questions dans
un seul message court et s'arrêter là** (la personne répond au message suivant). Les questions utiles :

- **Où** : sur un terrain neuf (île, plaine : `create_terrain`) ou dans la carte de GTA V, à l'endroit regardé ?
- **Style** : moderne, banlieue américaine (Mirror Park), quartier populaire (Grove Street, Davis), villas de luxe
  (Vinewood Hills, Rockford Hills), village de montagne (Paleto Bay), désert (Sandy Shores), bord de mer (Vespucci,
  Chumash), industriel (Cypress Flats, Port)…
- **Taille** : nombre de rues ou de bâtiments, ou surface (ex. 300 × 300 m).
- **Intérieurs** : vue de l'extérieur seulement (plus léger) ou certains bâtiments où l'on entre (lesquels :
  commerces, maisons, commissariat…) ?
- **Bâtiments du jeu** : d'accord pour recopier de vrais bâtiments de GTA V (plus naturel) en plus de ceux faits
  par DragoWalker ?

Si la demande est déjà précise (« un petit village style Paleto de 6 maisons sur une île, sans intérieurs »), ne pas
poser de questions : passer au plan. Proposer des choix par défaut raisonnables plutôt que trop de questions.

## 2. Le plan (montré, pas construit)

Préparer un plan en coordonnées du monde (x vers l'est, y vers le nord, mètres), autour du centre de la vue ou du
terrain prévu :

- **Rues** en premier : un quadrillage simple ou une grand-rue avec des rues qui en partent. Rue de ville :
  chaussée 11 m + 2 trottoirs de 3 m → **façades à 9–10 m de l'axe** de la rue. Pâtés de 40 à 80 m. Les bouts de
  route près d'une autre font un carrefour tout seuls (feux, passages piétons).
- **Bâtiments** le long des rues, entrée tournée vers la rue (`heading` : 0 = vers le sud, 90 = est, 180 = nord,
  270 = ouest), 2 à 4 m entre deux maisons, jardins derrière.
- **Places, parcs, eau** : un parc ou une place au centre donne vie ; quelques arbres le long des rues.
- Varier : pas deux mêmes maisons côte à côte (alterner modèles, couleurs, hauteurs).

Montrer le plan avec `show_plan` (rues « road », bâtiments « building » avec leur nom : « Maison 1 (Mirror Park) »,
« Supérette (intérieur) »…, parcs « park », places « plaza »), puis `screenshot`. Répondre à la personne avec :
le plan en quelques lignes (rues, nombre et genre de bâtiments, lesquels ont un intérieur, d'où viennent les
bâtiments recopiés), et **demander son accord**. S'arrêter là.

## 3. Construire (après « oui »)

Dans cet ordre, en vérifiant chaque grande étape avec `screenshot` :

1. **Terrain** si besoin : `create_terrain` (« Plat » pour une ville ; « Île » si demandé, en gardant la ville au
   centre, sur la partie plate). Sur un terrain en relief : `flatten_area` sous les quartiers.
2. **Rues** : `add_road` (une rue par appel ; les longues lignes droites sont découpées toutes seules).
3. **Bâtiments du jeu** (style le plus naturel) : `find_game_buildings` avec `district` (ex. « Mirror Park »,
   `max_width` ≈ la place disponible), aller en voir un avec `set_camera` + `screenshot` si on hésite, puis
   `copy_game_building` (x, y, heading). Regarder de quel côté est l'entrée (screenshot) et corriger le cap avec
   `move_object` / `undo` si elle ne donne pas sur la rue. Les copies n'ont pas d'intérieur.
4. **Bâtiments de l'atelier** (réglables, avec intérieur possible) : `add_building` avec `preset` (Maison de
   banlieue, Petit immeuble, Commerce, Entrepôt), couleurs et façades variées (`color`, `facade`, `game_facade`
   pour une vraie texture du jeu), `shop` pour des vitrines, `interior: true` pour ceux où l'on entre (pièces,
   meubles, lumières, porte qui s'ouvre). Puis, pour un intérieur, aménager avec le skill dragowalker-amenager
   (`furnish_room`) si la personne veut mieux que l'aménagement automatique.
5. **Végétation et détails** : `plant_vegetation` (parcs, jardins ; ensemble selon le style : Palmiers au bord de
   mer, Forêt de pins à Paleto, Désert à Sandy Shores), puis bancs, lampadaires, poubelles avec `place_prop` ou
   `run_actions` (noms trouvés avec `search_props`).
6. **Vie** (si demandé) : zones de quartier, piétons et voitures (voir l'onglet Vie ; pas d'outil direct : le dire
   à la personne).

Enfin : `show_plan` avec `clear: true`, `check_project`, `screenshot` d'ensemble (vue de haut puis vue à hauteur de
rue), et un compte rendu court : ce qui a été construit, ce qui a un intérieur, ce que la personne peut retoucher
(Atelier pour les bâtiments, Terrain pour les rues) et « Tester en jeu ».

## Règles

- Jamais de construction sans accord sur le plan (sauf si la personne dit « fais tout sans me demander »).
- Coordonnées toujours cohérentes avec le plan montré ; ne pas poser de bâtiment sur une route (garder 9 m de
  l'axe d'une rue de ville, 13 m d'une avenue).
- Une erreur : `undo` (annule le dernier outil) plutôt que d'empiler des corrections.
- Beaucoup de bâtiments avec intérieur alourdissent le serveur : en proposer quelques-uns (commerces, lieux clés)
  plutôt que tous, sauf demande contraire.
