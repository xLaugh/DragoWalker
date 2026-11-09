# DragoWalker

Éditeur de cartes GTA V (ymap, ytyp, intérieurs MLO) qui lit directement les fichiers du jeu, avec un assistant IA.

## Télécharger

Rendez-vous sur la page [Releases](https://github.com/xLaugh/DragoWalker/releases/latest) :

- **`DragoWalker-x.y.z-setup.exe`** : installeur (menu Démarrer, désinstallation, sans droits administrateur).
- **`DragoWalker-x.y.z.zip`** : version portable, à décompresser puis lancer `DragoWalker.exe`.

Au premier lancement, Windows peut afficher « Windows a protégé votre PC » : cliquez sur
« Informations complémentaires » puis « Exécuter quand même ».

## Nécessaire

- Windows 10 ou 11, 64 bits
- Une carte graphique compatible OpenGL 3.1 ou plus
- GTA V installé (version PC)
- [CodeWalker](https://github.com/dexyfex/CodeWalker) : indiquez son dossier dans le panneau « Jeu » au premier lancement

Optionnel :

- Blender + Sollumz, pour les modèles 3D
- Claude Code, une clé API Claude ou un serveur compatible OpenAI, pour l'assistant IA

## Compiler

Avec [Rust](https://rustup.rs) (toolchain MSVC) :

```
cargo build --release
```

Pour créer la version portable et l'installeur (Inno Setup 6 requis) :

```
powershell -ExecutionPolicy Bypass -File tools\package.ps1
```

## Licence

GPL-3.0, voir [LICENSE](LICENSE).

DragoWalker n'est pas affilié à Rockstar Games. Aucun fichier du jeu n'est fourni.
