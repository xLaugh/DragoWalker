---
name: dragowalker-architecture
description: Faire un intérieur qui s'intègre à l'univers de GTA V — dans un bâtiment existant du jeu (s'appuyer sur l'intérieur que Rockstar y a mis, ou sur l'enveloppe du bâtiment), en s'inspirant de l'architecture des intérieurs du jeu (commissariat, hôpital, bureaux…). À utiliser dès qu'on parle d'un lieu de la carte (« le shérif de Paleto », « le poste de Mission Row », « ce bâtiment ») ou d'un intérieur « lore ».
---

# Un intérieur qui a sa place dans GTA

## 1. Partir de ce qui existe, jamais de boîtes au jugé

1. Mettre la vue sur le lieu (`set_camera`), puis `look_around` : intérieur du projet ici ? intérieurs **du jeu**
   proches ? `find_game_interiors` avec un mot du lieu (sheriff, police, cop, hospital, office, bank, garage…).
2. **Le bâtiment a déjà un intérieur du jeu** (cas le plus courant) :
   - `study_game_interior` : pièces, tailles, sols, passages, meubles de chaque pièce ;
   - `copy_game_interior` en mode `structure` : le même intérieur, à sa place exacte, avec ses murs, sols et
     escaliers, sans les meubles ; celui du jeu est caché automatiquement (pas de conflit) ;
   - adapter : `rename_room`, `split_room` pour découper une grande salle (cellules, salle d'interrogatoire,
     bureaux fermés : une cloison percée d'une porte est posée), puis `furnish_room` **avec `keep_existing: true`**
     pour garder la structure, `place_doors`, `check_project`.
   - Les entrées sont déjà là (celles de Rockstar) : ne pas en ajouter sauf demande.
3. **Pas d'intérieur du jeu** : `select_game_object` (nom du bâtiment), `analyze_building` (taille, étages, hauteurs),
   puis `building_to_interior`. Si l'analyse ne trouve pas de pièces : créer les pièces avec `add_room` **dans
   l'emprise mesurée**, placer l'intérieur avec `place_interior` (même position et même cap que le bâtiment), puis
   vérifier vu de dessus (`set_camera` top) que les pièces restent dans le bâtiment. `add_entrance` face à la porte
   du bâtiment.

## 2. S'inspirer de Rockstar

- Avant de concevoir un type de lieu, `find_game_interiors` puis `study_game_interior` sur un ou deux intérieurs du
  même genre (police, hôpital, bureaux, appartements) : reprendre leurs tailles de pièces, leur enchaînement, et leurs
  **familles de meubles** (même préfixe de modèle, à retrouver avec `search_props`).
- Rester dans le ton de l'endroit : un shérif de campagne (Paleto, Sandy) est plus modeste et boisé qu'un poste du
  centre de Los Santos ; un bâtiment ancien garde des pièces moyennes, pas un open space géant.

## 3. Règles d'architecture

- **Zonage** : accueil côté entrée publique ; bureaux ensuite ; zone sécurisée (cellules, salle d'interrogatoire,
  armurerie, preuves) au fond, accessible seulement par un couloir ou depuis les bureaux, jamais depuis l'accueil.
  Vestiaires près de l'entrée du personnel.
- **Tailles** : couloir 1,5 à 2 m ; porte 1 à 1,2 m ; cellule 2 × 3 à 3 × 4 m (lit, WC, lavabo) ; salle
  d'interrogatoire 3 × 3 à 4 × 4 m (table, 3 chaises, caméra) ; vestiaire 3 × 4 m au moins ; salle de réunion
  4 × 6 m ; open space 2,5 × 2,5 m par poste. Hauteur sous plafond 2,7 à 3,2 m.
- Les pièces suivent les murs du bâtiment ; les fenêtres seulement sur les murs extérieurs ; une pièce sans fenêtre
  au cœur du bâtiment (cellules, archives, interrogatoire).
- Noms de pièces simples et utiles : accueil, couloir, open_space, bureau_chef, salle_reunion, interrogatoire,
  vestiaire, cellule_1, cellule_2, armurerie.

## 4. Murs, portes, emprise

- Avant d'ajouter des pièces à côté d'un intérieur repris : `building_bounds` donne la boîte du bâtiment dans le repère
  de l'intérieur ; chaque `add_room` doit tenir dedans (0,3 m de marge pour les murs), sans chevaucher les pièces
  existantes, et toucher une pièce existante (sinon pas de passage).
- Les pièces ajoutées ont leurs murs, sol et plafond générés ; celles reprises du jeu gardent ceux de Rockstar ;
  `split_room` dans une pièce du jeu pose une cloison percée d'une porte.
- Portes : d'abord les spéciales avec `place_doors` `model` + `rooms` (cellules : `v_ilev_ph_cellgate` ou
  `v_ilev_ph_cellgate02`), ensuite `place_doors` sans modèle pour le reste. `search_props` avec `all: true` trouve
  aussi les portes et objets de décor du jeu.
- Ne pas copier de carte du jeu sans besoin : `building_bounds` et `look_around` suffisent pour mesurer et repérer.

## 5. Fenêtres, portails, pièces : bien les faire

- **Fenêtres** : `building_windows` liste les vraies vitres du bâtiment (et des objets posés dessus) à hauteur de
  l'intérieur. Chaque pièce contre la façade a une fenêtre (`add_window`) en face de chaque vitre, même largeur et
  même allège ; sinon on voit un mur derrière la vitre. Pas de vitres (fenêtres peintes) : rien à percer.
- **Portails** : une pièce par volume fermé ; chaque passage entre deux pièces est un portail (porte) ; les portails
  ne doivent jamais être coupés par une cloison (`split_room` refuse) ; la pièce 0 (limbo) est le dehors.
- **Pièces** : leurs boîtes se touchent sans se chevaucher et couvrent tout l'intérieur ; le jeu sait dans quelle
  pièce est le joueur grâce au sol (collision avec le numéro de la pièce) : sinon la pièce « disparaît ».
- **Intérieur repris avec une ancienne version** : `repair_game_interior` (collision de Rockstar, vrais sols, réglages
  d'origine des portes).

## 6. Finir proprement

`check_project` sans erreur ; captures en plan puis en 3D ; dire à la personne ce qui vient du jeu (structure
reprise) et ce qui a été ajouté, et qu'il faut tester en jeu (F12).
