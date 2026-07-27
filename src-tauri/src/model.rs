use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataRootDto {
    pub path: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterDto {
    pub id: u16,
    pub name: String,
    pub nickname: String,
    pub body_type: i32,
    pub body_type_name: String,
    pub face_index: i32,
    pub hair_palette: String,
    pub skin_palette: String,
    pub vest_palette: String,
    pub pants_palette: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDto {
    pub id: u16,
    pub name: String,
    pub item_class: i32,
    pub compatible_slots: Vec<String>,
    pub lbe_class: Option<i32>,
    pub lbe_combo: Option<i32>,
    pub camouflage_kit: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentOptionDto {
    pub id: u16,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticDto {
    pub severity: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl DiagnosticDto {
    pub fn info(code: &str, message: impl Into<String>, source: Option<String>) -> Self {
        Self {
            severity: "info".into(),
            code: code.into(),
            message: message.into(),
            source,
        }
    }

    pub fn warning(code: &str, message: impl Into<String>, source: Option<String>) -> Self {
        Self {
            severity: "warning".into(),
            code: code.into(),
            message: message.into(),
            source,
        }
    }

    pub fn error(code: &str, message: impl Into<String>, source: Option<String>) -> Self {
        Self {
            severity: "error".into(),
            code: code.into(),
            message: message.into(),
            source,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummaryDto {
    pub roots: Vec<DataRootDto>,
    pub characters: Vec<CharacterDto>,
    pub items: Vec<ItemDto>,
    pub layers: usize,
    pub surfaces: usize,
    pub filters: usize,
    pub body_types: usize,
    pub warning_count: usize,
    pub diagnostics: Vec<DiagnosticDto>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub character_id: u16,
    #[serde(default)]
    pub inventory: HashMap<String, u16>,
    #[serde(default)]
    pub attachments: HashMap<String, Vec<u16>>,
    #[serde(default)]
    pub scenario: ScenarioState,
    #[serde(default)]
    pub animation: String,
    #[serde(default)]
    pub direction: u8,
    #[serde(default)]
    pub frame: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScenarioState {
    pub team: i32,
    pub soldier_class: i32,
    pub civilian_group: i32,
    pub camo: i32,
    pub urban_camo: i32,
    pub desert_camo: i32,
    pub snow_camo: i32,
    pub in_water: bool,
    pub injured: bool,
    pub big_merc_alt: bool,
    pub big_merc_badass: bool,
    pub second_hand_usable: bool,
    pub second_hand_loaded: bool,
    pub burst: bool,
}

impl Default for ScenarioState {
    fn default() -> Self {
        Self {
            team: 0,
            soldier_class: 0,
            civilian_group: 0,
            camo: 0,
            urban_camo: 0,
            desert_camo: 0,
            snow_camo: 0,
            in_water: false,
            injured: false,
            big_merc_alt: false,
            big_merc_badass: false,
            second_hand_usable: true,
            second_hand_loaded: true,
            burst: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationDto {
    pub id: String,
    pub label: String,
    pub group: String,
    pub resolved_surface: String,
    pub variant: String,
    pub frames_per_direction: u16,
    pub layer_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewContextDto {
    pub body_type: String,
    pub weapon_mode: String,
    pub profile_palette: ProfilePaletteDto,
    pub camouflage: CamouflageDto,
    pub animations: Vec<AnimationDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CamouflageDto {
    pub applied_limit: i32,
    pub applied: [i32; 4],
    pub worn: [i32; 4],
    pub total: [i32; 4],
    pub stealth: i32,
    pub palette: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePaletteDto {
    pub hair: String,
    pub skin: String,
    pub vest: String,
    pub pants: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLayerDto {
    pub layer: String,
    pub z_index: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprite_direction: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_index: Option<u32>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png_data_url: Option<String>,
    pub width: u32,
    pub height: u32,
    pub body_type: String,
    pub animation_state: String,
    pub resolved_surface: String,
    pub animation_variant: String,
    pub sprite_direction: u8,
    pub image_index: u32,
    pub layers: Vec<PreviewLayerDto>,
    pub diagnostics: Vec<DiagnosticDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFindingDto {
    pub severity: String,
    pub code: String,
    pub animation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditDto {
    pub animations_checked: usize,
    pub surfaces_checked: usize,
    pub issue_count: usize,
    pub truncated: bool,
    pub findings: Vec<AuditFindingDto>,
}

#[derive(Clone, Debug)]
pub struct Character {
    pub id: u16,
    pub name: String,
    pub nickname: String,
    pub profile_type: i32,
    pub body_type: i32,
    pub face_index: i32,
    pub sex: i32,
    pub exp_level: i32,
    pub strength: i32,
    pub leadership: i32,
    pub wisdom: i32,
    pub hair_palette: String,
    pub skin_palette: String,
    pub vest_palette: String,
    pub pants_palette: String,
}

impl Character {
    pub fn body_type_name(&self) -> &'static str {
        body_type_name(self.body_type)
    }
}

pub fn body_type_name(body_type: i32) -> &'static str {
    const NAMES: &[&str] = &[
        "REGMALE",
        "BIGMALE",
        "STOCKYMALE",
        "REGFEMALE",
        "ADULTFEMALEMONSTER",
        "AM_MONSTER",
        "YAF_MONSTER",
        "YAM_MONSTER",
        "LARVAE_MONSTER",
        "INFANT_MONSTER",
        "QUEENMONSTER",
        "FATCIV",
        "MANCIV",
        "MINICIV",
        "DRESSCIV",
        "HATKIDCIV",
        "KIDCIV",
        "CRIPPLECIV",
        "COW",
        "CROW",
        "BLOODCAT",
        "ROBOTNOWEAPON",
        "HUMVEE",
        "TANK_NW",
        "TANK_NE",
        "ELDORADO",
        "ICECREAMTRUCK",
        "JEEP",
        "COMBAT_JEEP",
    ];
    NAMES
        .get(body_type.max(0) as usize)
        .copied()
        .unwrap_or("UNKNOWN")
}

#[derive(Clone, Debug, Default)]
pub struct WeaponStats {
    pub class: i32,
    pub kind: i32,
    pub calibre: i32,
    pub shots_per_burst: i32,
}

#[derive(Clone, Debug, Default)]
pub struct ArmourStats {
    pub class: i32,
    pub protection: i32,
    pub coverage: i32,
}

#[derive(Clone, Debug, Default)]
pub struct LoadBearingStats {
    pub class: i32,
    pub combo: i32,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: u16,
    pub name: String,
    pub item_class: i32,
    pub class_index: usize,
    pub two_handed: bool,
    pub rocket_launcher: bool,
    pub grenade_launcher: bool,
    pub shots_per_burst: i32,
    pub camo_bonus: i32,
    pub urban_camo_bonus: i32,
    pub desert_camo_bonus: i32,
    pub snow_camo_bonus: i32,
    pub stealth_bonus: i32,
    pub camouflage_kit: bool,
}

#[derive(Clone, Debug)]
pub struct EngineSettings {
    pub camo_kit_area: i32,
    pub camo_lbe_over_vest: f32,
    pub camo_lbe_over_pants: f32,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            camo_kit_area: 5,
            camo_lbe_over_vest: 0.2,
            camo_lbe_over_pants: 0.6,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PaletteReplacement {
    pub start: u8,
    pub colors: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct PaletteReplacementDb {
    pub replacements: HashMap<String, PaletteReplacement>,
}

#[derive(Clone, Debug)]
pub struct LayerDef {
    pub name: String,
    pub render: bool,
    pub render_shadows: bool,
    pub z_index: [i32; 8],
    pub declaration_order: usize,
}

#[derive(Clone, Debug)]
pub struct SurfaceDef {
    pub name: String,
    pub file: String,
    pub directions: u16,
    pub frames_per_direction: u16,
    pub alpha: bool,
}

#[derive(Clone, Debug)]
pub struct PaletteDef {
    pub name: String,
    pub file: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    And,
    Or,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompareOp {
    Eq,
    In,
    Between,
    Greater,
    Less,
}

#[derive(Clone, Debug)]
pub struct Criterion {
    pub field: String,
    pub operation: Operation,
    pub compare: CompareOp,
    pub negate: bool,
    pub values: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct FilterDef {
    pub criteria: Vec<Criterion>,
}

#[derive(Clone, Debug)]
pub struct SurfaceMapping {
    pub surface: String,
    pub animation_surface: Option<String>,
    pub animation_state: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LayerProp {
    pub filter: Option<String>,
    pub palette: Option<String>,
    pub surfaces: Vec<SurfaceMapping>,
}

#[derive(Clone, Debug, Default)]
pub struct LayerConfig {
    pub render: Option<bool>,
    pub render_shadows: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct BodyType {
    pub filter: Option<String>,
    pub label: String,
    pub layer_configs: HashMap<String, LayerConfig>,
    pub layer_props: HashMap<String, Vec<LayerProp>>,
}

#[derive(Clone, Debug)]
pub struct SoldierState {
    pub character: Character,
    pub inventory: HashMap<String, u16>,
    pub attachments: HashMap<String, Vec<u16>>,
    pub scenario: ScenarioState,
}
