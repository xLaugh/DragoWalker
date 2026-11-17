---
name: dragowalker-fivem
description: Préparer un intérieur ou une carte DragoWalker pour un serveur FiveM — points d'interaction ox_target / qb-target, portes (ox_doorlock, qb-doorlock), export, test en jeu, limites des MLO. À utiliser pour les questions de serveur RP, de scripts, d'export ou de test.
---

# DragoWalker et FiveM

## Points d'interaction

- `interaction_points` liste les meubles utilisables repérés dans les intérieurs (caisse, frigo, vestiaire, armes,
  ordinateur, lit, labo, coiffure, lavabo, toilettes…). `enable: true` coche leur export.
- À l'export, `dw_targets.lua` crée une zone **ox_target** ou **qb-target** par meuble (selon la ressource
  démarrée) ; la choisir envoie l'évènement client `dragowalker:interaction` avec `p.kind`, `p.label`, `p.mlo`,
  `p.room`, `p.model`, `p.coords`. Exemple à donner à la personne :

```lua
AddEventHandler('dragowalker:interaction', function(p)
    if p.kind == 'caisse' then TriggerEvent('mon_script:ouvrir_caisse', p) end
end)
```

## Portes

`place_doors` pose de vraies portes du jeu. L'export écrit `dw_doors.lua` (portes ouvertes) et, dans `doorlock/`,
`ox_doorlock.sql` et `qb-doorlock.lua` à reprendre dans ces scripts.

## Export et test

- L'export se fait dans DragoWalker : menu **Fichier » Exporter une ressource FiveM…** (Ctrl+Maj+E ; ressource avec `fxmanifest.lua`). Le
  bouton orange **Tester en jeu** (F12) lance le serveur de test local. Ce sont des gestes de la personne : les
  lui indiquer plutôt que de les faire.
- Limites d'un MLO : 31 pièces au plus ; un intérieur doit être **placé** sur la carte pour s'exporter.
- Les modèles posés doivent exister dans le jeu (sinon invisibles) : vérifier avec `search_props`.

## Props personnels (Studio, onglet Props)

Quand la personne veut un objet qui n'existe pas dans le jeu (enseigne à son logo, affiche, tapis, meuble fait sur
Blender…), le créer plutôt que d'en chercher un proche :

- `create_prop_from_image` : image PNG / JPEG → `panneau` (enseigne au sol), `affiche` (au mur, origine au dos),
  `decoupe` (selon la transparence), `tapis` (à plat) ou `cube`. `width` en mètres, la hauteur suit l'image.
- `import_model_prop` : modèle Blender (.blend, .fbx, .glb, .gltf, .obj…) ; 1 unité Blender = 1 m, posé au sol.
- `create_shape_prop` : objet fait de boîtes, cylindres et plaques (comptoir sur mesure, socle, présentoir…),
  couleurs `#rrggbb` ou textures importées du projet. Gardé dans le projet : le rappeler avec le même nom le recrée.
- Collision : `boite` par défaut (sûr) ; `forme` pour un objet qu'on contourne de près ; `aucune` pour une affiche.
- Le prop va dans « Mes modèles », se pose comme un objet du jeu (`place: true` ou ensuite `place_prop` avec son nom)
  et part avec l'export (modèle, textures, collision, type d'objet déclaré dans le fxmanifest).
- Devant d'un prop vers -y (comme ceux du jeu) : une affiche murale se pose avec le même cap que le mur regardé.
