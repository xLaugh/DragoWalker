---
name: dragowalker-carte
description: Modifier la carte de GTA V dans DragoWalker (poser, aligner, supprimer, déplacer des props dans le monde, autour du centre de la vue). À utiliser pour « mets des barrières », « plante des arbres », « supprime les poubelles ici », « aligne des bancs ».
---

# Modifier la carte

Outils : `get_scene`, `search_props`, `place_prop`, `move_object`, `delete_objects`, `select_objects`,
`run_actions`, `set_camera`, `screenshot`, `undo`.

- `get_scene` donne le centre de la vue (là où la personne regarde) et le sol : travailler autour.
- Plusieurs objets en ligne, cercle, grille ou au hasard : `run_actions` avec une action `place`
  (`archetype`, `count`, `pattern` = point | ligne | cercle | grille | aleatoire, `size` = espacement ou rayon,
  positions relatives au centre de la vue). Supprimer dans un rayon : action `delete_radius`.
- Un objet précis : `place_prop` (position du monde ; sans `z`, posé au sol réel — relief, routes, bâtiments).
- Toujours des noms trouvés avec `search_props` (ou des modèles connus du jeu de base : prop_bench_01a,
  prop_tree_pine_01, prop_barrier_work05, prop_bin_01a, prop_streetlight_01…).
- Les modifications vont dans la carte du projet choisie (ymap) ; rien n'est écrit dans les fichiers du jeu.
- Vérifier avec `screenshot` (après `set_camera` si besoin) ; `undo` en cas d'erreur.
