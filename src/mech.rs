//! Valeurs de la fiche MECHANICS.md / mechanics.json (relevées dans CodeWalker).

// Caméra (CodeWalker.World.Camera)
pub const CAM_PITCH_LIMIT: f32 = 1.55;
pub const CAM_SMOOTHNESS: f32 = 10.0;
pub const CAM_SENSITIVITY: f32 = 0.005;
pub const CAM_FOV: f32 = 1.0;
pub const CAM_MAX_DIST: f32 = 11000.0;
pub const CAM_MIN_DIST: f32 = 0.0001;
pub const CAM_ZOOM_FACTOR: f32 = 1.1;
/// estimé : en dessous de cette distance à la cible, la molette avance la caméra (vers la souris) au lieu de la
/// rapprocher encore de la cible
pub const ZOOM_DOLLY_DIST: f32 = 1.5;
/// Plan proche le plus petit de la carte graphique : ZNear de CodeWalker (Camera::.ctor 0x06000162, ldc.r4 0.01)
pub const ZOOM_NEAR_MIN: f32 = 0.01;
/// Part de l'écart de distance rattrapée à chaque image : ZoomSpeed (Camera::.ctor 0x06000162, ldc.r4 0.1),
/// appliquée par Camera::UpdateFollow 0x06000166
pub const CAM_ZOOM_SPEED: f32 = 0.1;
/// images par seconde auxquelles ZoomSpeed est ramené au temps écoulé (CodeWalker l'applique par image) — estimé
pub const CAM_ZOOM_SPEED_FPS: f32 = 60.0;
pub const ORTHO_MAX: f32 = 20000.0;
pub const ORTHO_MIN: f32 = 1.0;
/// estimé
pub const CAM_START_DIST: f32 = 420.0;
/// estimé
pub const CAM_START_PITCH: f32 = 0.8;

// Déplacement (WorldForm::UpdateControlInputs)
pub const MAX_FRAME_STEP: f32 = 0.1;
pub const BASE_MOVE_SPEED: f32 = 50.0;
pub const SHIFT_MULT: f32 = 5.0;
pub const CTRL_MULT: f32 = 0.2;
pub const MOVE_DIST_CAP: f32 = 20.0;
pub const MAP_MOVE_FACTOR: f32 = 0.01;
pub const MAP_MOVE_CAP: f32 = 50.0;

// Nouvelle entité (ProjectForm::NewEntity / NewMloEntity)
#[allow(dead_code)]
pub const DEFAULT_ARCHETYPE: &str = "prop_alien_egg_01";
pub const SPAWN_DIST: f32 = 5.0;
/// Copie : distance = rayon de la sphère englobante × 2.5 (ProjectForm::NewEntity 0x06000d3e, IL_0047)
pub const SPAWN_COPY_RADIUS_MULT: f32 = 2.5;
/// Pas de grille par défaut (réglage SnapGridSize de CodeWalker.exe.config, utilisé par WorldForm::SnapPosition 0x0600027d)
pub const SNAP_GRID_SIZE: f32 = 10.0;
pub const ENTITY_FLAGS: u32 = 32;
/// Bit des drapeaux d'un objet : sa version de loin (parentIndex) est dans la carte parente (CMapData.parent) —
/// CodeWalker YmapEntityDef::get_LodInParentYmap 0x06002d03 : (flags >> 3) & 1 ; vérifié sur les cartes du jeu.
pub const LOD_IN_PARENT_YMAP: u32 = 8;
/// Survol : taille d'une boîte touchée = |max − min| × 0,5 ; la plus petite l'emporte — CodeWalker
/// WorldForm::UpdateMouseHits(DrawableBase, Archetype, YmapEntityDef) 0x060002ab (IL_05b6–IL_05d4).
pub const PICK_HALF_DIAG: f32 = 0.5;
pub const MLO_ENTITY_FLAGS: u32 = 1572872;
/// Drapeaux du placement d'un intérieur (CMloInstanceDef) : ceux de 45 des 63 placements des MLO FiveM analysés
/// le 14 nov. 2026 (G:\MLOFIVEM).
pub const MLO_INSTANCE_FLAGS: u32 = 1572864;
pub const ENTITY_LOD_DIST: f32 = 200.0;
pub const ENTITY_LOD_LEVEL: &str = "LODTYPES_DEPTH_ORPHANHD";
pub const ENTITY_PRIORITY: &str = "PRI_REQUIRED";
pub const ENTITY_PARENT_INDEX: i32 = -1;
pub const ENTITY_AO: i32 = 255;

// Ymap / Ytyp (ProjectForm::NewYmap / NewYtyp)
pub const NEW_YMAP_CONTENT_FLAGS: u32 = 65;
pub const NEW_YMAP_PREFIX: &str = "map";
pub const NEW_YTYP_PREFIX: &str = "types";

// MLO (ProjectForm::NewMloRoom / NewMloPortal / NewMloEntitySet)
pub const ROOM_FLAGS: u32 = 96;
pub const ROOM_BLEND: f32 = 1.0;
pub const ROOM_EXT_VIS_DEPTH: i32 = -1;
pub const ROOM_DEFAULT_NAME: &str = "NewRoom";
pub const PORTAL_ROOM_FROM: u32 = 1;
pub const PORTAL_ROOM_TO: u32 = 0;
pub const ENTITY_SET_DEFAULT_NAME: &str = "NewEntitySet";
