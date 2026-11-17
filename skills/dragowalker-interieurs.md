---
name: dragowalker-interieurs
description: Créer ou modifier un intérieur MLO FiveM dans DragoWalker (pièces, passages, portes, intérieurs tout prêts, aménagement des pièces, ambiances). À utiliser dès qu'on demande un intérieur, un appartement, un commissariat, un bar, une pièce ou un MLO.
---

# Faire un intérieur MLO dans DragoWalker

Outils : serveur MCP `dragowalker` (get_scene, get_interior, list_room_types, create_interior, add_room,
furnish_room, place_doors, set_room_ambiance, set_camera, screenshot, undo).

## Marche à suivre

1. `get_scene` : voir ce qui existe déjà (intérieurs, studio ouvert, jeu chargé).
2. Un intérieur courant (appartement, maison, bureaux, commissariat, bar, restaurant, supérette, boîte de nuit,
   clinique, garage, coiffeur, armurerie, labo) : `list_room_types`, puis
   `create_interior` avec `template` et `theme`. Tout est fait : pièces, passages, porte d'entrée, vraies portes du
   jeu, ambiance, meubles, déco. C'est presque toujours le meilleur départ ; ensuite on ajuste.
3. Sur mesure : `create_interior` avec `rooms: 0` (vide), puis `add_room` pour chaque pièce, en rectangles qui se
   touchent (une porte se crée entre deux pièces dont les murs se touchent sur au moins 1,4 m). Garder des pièces
   d'au moins 2,5 × 2,5 m (3 × 3 m pour une chambre, 4 × 4 m pour un salon). 31 pièces au plus.
4. `place_doors` pour les vraies portes du jeu (le jeu doit être chargé).
5. `furnish_room` pour chaque pièce, avec le type qui va (numéro ou nom de `list_room_types`) et un style
   (1 moderne, 2 classique, 3 luxe, 4 industriel). Pour une autre variante : même appel avec un autre `seed`.
   `keep_existing: true` garde ce qui est déjà posé.
6. Regarder : `set_camera` (interior, `top: true` pour le plan, ou `pitch` 60 et `dist` 15 pour la 3D), puis
   `screenshot`. Corriger ce qui ne va pas.

## Règles

- Coordonnées des pièces : locales à l'intérieur, en mètres, Z vers le haut ; le sol du rez-de-chaussée est à z = 0.
- Les noms de pièces aident : « cuisine », « chambre », « salle_de_bain », « toilettes », « bureau »,
  « open_space », « accueil », « cellule », « vestiaire », « armurerie », « labo »… (sans accents ni espaces).
- Une erreur ? `undo` annule la dernière modification.
- Dire à la personne ce qui a été fait, et qu'elle peut tout retoucher dans le Studio puis tester en jeu (F12).
