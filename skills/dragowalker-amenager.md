---
name: dragowalker-amenager
description: Placer des meubles et objets à la main dans DragoWalker (dans une pièce d'intérieur ou sur la carte) de façon logique — vrais modèles du jeu, orientation, tailles, passages. À utiliser pour « ajoute un canapé », « mets des tables », « décore », « déplace ce meuble ».
---

# Poser des objets logiquement

Outils : `search_props`, `place_prop`, `move_object`, `delete_objects`, `get_interior`, `screenshot`, `undo`.

## Avant de poser

- Une pièce entière à meubler : préférer `furnish_room` (meubles, passages, déco, ambiance d'un coup).
- Un modèle précis : **toujours** le chercher avec `search_props` (mots anglais : sofa, couch, armchair, chair,
  table, desk, bed, wardrobe, fridge, sink, toilet, tv, plant, lamp, shelf, counter, stool…). Ne jamais inventer un
  nom : un nom faux n'apparaît pas en jeu.
- `get_interior` donne les pièces (rectangle, sol) et les objets déjà là (position, cap) : en tenir compte.

## Orientation (cap en degrés)

- Les meubles du jeu ont leur **face avant vers -Y (le sud) à cap 0**. Cap 90 : face vers l'est (+X) ; 180 : vers le
  nord ; 270 : vers l'ouest.
- Contre un mur, le dos au mur : mur nord (y max) → cap 0 ; mur sud (y min) → cap 180 ; mur est (x max) → cap 270 ;
  mur ouest (x min) → cap 90.
- Exceptions connues : `v_41_desk01` (on s'assoit de l'autre côté, +180), `v_res_mbath` (+90).
- Une chaise regarde la table, un canapé la télé, un fauteuil de bureau son bureau.

## Placement

- `place_prop` avec `interior` : x, y locaux ; sans `z`, l'objet est posé sur le sol de la pièce.
- Centre de l'objet à au moins la moitié de sa profondeur + 0,07 m du mur (taille donnée par `search_props`).
- Laisser libres : 1 m devant chaque porte, 0,7 m devant armoires, frigos, éviers, WC, casiers ; 0,8 m autour
  d'une table (chaises reculées) ; 0,6 m de chaque côté d'un lit ; le passage entre canapé et télé.
- Pas de meuble haut devant une fenêtre. Pas deux objets l'un dans l'autre.
- Objets posés sur un meuble (lampe sur table de chevet, télé sur meuble télé) : `z` = haut du meuble.
- Pièges : `v_res_m_sinkunit` est un meuble vasque **avec WC** (salle de bain seulement) ; `prop_ff_counter_*` sont
  des tables inox de cuisine pro, pas des comptoirs ; `v_med_benchset1` est un poste de bureau, pas un banc.

## Vérifier

`set_camera` sur l'intérieur, `top: true`, puis `screenshot` ; corriger avec `move_object` (ou `undo`).
