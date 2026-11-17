---
name: dragowalker-methode
description: Méthode de travail dans DragoWalker — comprendre exactement de quoi la personne parle (« ce MLO », « ce bâtiment », « cette pièce »), analyser avant d'agir, faire les choses dans le bon ordre, contrôler et rendre compte. À suivre pour toute demande qui modifie le projet.
---

# Travailler proprement dans DragoWalker

## 1. Savoir exactement de quoi on parle

- « ce », « cet », « celui-là », « ici » : `get_selection` d'abord (ce qui est choisi dans la vue), puis
  `look_around` (ce qu'il y a au centre de la vue). Ne jamais deviner.
- Rien de choisi et plusieurs candidats : demander à la personne de **cliquer l'objet dans la vue**, puis s'arrêter
  là (une seule question, courte).
- Ce que peut être « ce MLO » :
  - un **intérieur du projet** (placement choisi, ou un de ses objets) → `open_here`, puis `get_interior` ;
  - un **dossier de ressource MLO FiveM** que la personne donne → `import_mlo_resource` (recopié tel quel à
    l'export : il se regarde et se déplace, ses pièces ne se modifient pas) ;
  - un **intérieur du jeu de base** → `find_game_interiors`, puis `set_camera` dessus (visible, pas modifiable) ;
  - un **bâtiment du jeu** sans intérieur → `analyze_building`, puis `building_to_interior` si on veut y entrer.

## 2. Analyser avant d'agir

- `get_scene`, `maps` (où vont les nouveaux objets), `get_interior` (pièces, rectangles, sols, objets, passages).
- Pour un bâtiment : `analyze_building` (étages, hauteurs, pièces proposées) avant de le transformer.
- `screenshot` (après `set_camera`, `top: true` pour un plan) pour voir l'état de départ si c'est utile.

## 3. Faire, dans l'ordre

1. Structure : pièces (`add_room` ou `building_to_interior`), noms clairs (`rename_room` : salon, cuisine, chambre_1,
   couloir, salle_de_bain…), passages, puis `place_doors`.
2. Contenu : `furnish_room` pièce par pièce avec le bon type (le nom de la pièce aide), puis les retouches à la main
   (`place_prop`, `move_object`) en suivant la skill dragowalker-amenager.
3. Finitions : ambiances (`set_room_ambiance`), points d'interaction si c'est pour un serveur RP
   (`interaction_points`).

Une étape à la fois ; relire le résultat de chaque outil avant la suivante. Ne rien supprimer ni écraser que la
personne n'a pas demandé ; en cas d'erreur, `undo`.

## 4. Contrôler

- `check_project` : corriger les erreurs qui viennent du travail fait (objet hors de sa pièce, pièce sans passage…).
- `screenshot` de l'intérieur : d'abord le plan (`set_camera` avec `top: true`), puis la 3D (`top: false`,
  `pitch` 55, `dist` 15) : rien qui flotte, rien devant les portes, meubles dos aux murs, tableaux à bonne hauteur.

## 5. Rendre compte

En français simple, court : ce qui a été fait (noms des intérieurs et des pièces), ce qui reste à faire ou à
vérifier en jeu (F12), et ce que la personne peut retoucher dans le Studio.
