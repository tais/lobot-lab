use std::collections::{BTreeMap, HashMap, HashSet};

use roxmltree::{Document, Node};

use crate::{
    animation,
    filter::{self, FilterContext},
    model::{
        AnimationDto, ArmourStats, AttachmentOptionDto, BodyType, Character, CharacterDto,
        CompareOp, Criterion, DiagnosticDto, EngineSettings, FilterDef, Item, ItemDto, LayerConfig,
        LayerDef, LayerProp, LoadBearingStats, Operation, PaletteDef, PaletteReplacement,
        PaletteReplacementDb, PreviewContextDto, PreviewRequest, ProfilePaletteDto, SoldierState,
        SurfaceDef, SurfaceMapping, WeaponStats, WorkspaceSummaryDto,
    },
    sti::StiImage,
    vfs::Vfs,
    xml,
};

const LAYERS_XML: &str = "TableData/LogicalBodyTypes/Layers.xml";
const PALETTES_XML: &str = "TableData/LogicalBodyTypes/Palettes.xml";
const SURFACES_XML: &str = "TableData/LogicalBodyTypes/AnimationSurfaces.xml";
const FILTERS_XML: &str = "TableData/LogicalBodyTypes/Filters.xml";
const BODY_TYPES_XML: &str = "TableData/LogicalBodyTypes/LogicalBodyTypes.xml";
const PROFILES_XML: &str = "TableData/MercProfiles.xml";
const ITEMS_XML: &str = "TableData/Items/Items.xml";
const WEAPONS_XML: &str = "TableData/Items/Weapons.xml";
const ARMOURS_XML: &str = "TableData/Items/Armours.xml";
const LOAD_BEARING_XML: &str = "TableData/Items/LoadBearingEquipment.xml";
const ATTACHMENTS_XML: &str = "TableData/Items/Attachments.xml";
const ITEM_SETTINGS_INI: &str = "Item_Settings.ini";
const JA2_OPTIONS_INI: &str = "Ja2_Options.ini";
const SOLDIER_PALETTES: &str = "BinaryData/JA2PAL.DAT";
const IC_LBEGEAR: i32 = 0x0002_0000;

fn modal_frame_count(values: &[u16]) -> u16 {
    let mut counts = HashMap::<u16, usize>::new();
    for value in values {
        *counts.entry(*value).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(value, count)| (*count, *value))
        .map(|(value, _)| value)
        .unwrap_or(0)
}

pub struct Workspace {
    pub(crate) vfs: Vfs,
    pub(crate) layers: Vec<LayerDef>,
    pub(crate) surfaces: HashMap<String, SurfaceDef>,
    pub(crate) palettes: HashMap<String, PaletteDef>,
    pub(crate) filters: HashMap<String, FilterDef>,
    pub(crate) body_types: Vec<BodyType>,
    pub(crate) characters: HashMap<u16, Character>,
    pub(crate) items: HashMap<u16, Item>,
    pub(crate) weapons: HashMap<u16, WeaponStats>,
    pub(crate) armours: HashMap<usize, ArmourStats>,
    pub(crate) load_bearing: HashMap<usize, LoadBearingStats>,
    pub(crate) attachment_links: HashMap<u16, Vec<u16>>,
    pub(crate) engine_settings: EngineSettings,
    pub(crate) palette_replacements: PaletteReplacementDb,
    pub(crate) diagnostics: Vec<DiagnosticDto>,
    pub(crate) issue_count: usize,
    pub(crate) sti_cache: HashMap<String, Result<StiImage, String>>,
    pub(crate) palette_cache: HashMap<String, Result<Vec<u8>, String>>,
}

impl Workspace {
    pub fn load(roots: Vec<String>) -> Result<Self, String> {
        let vfs = Vfs::new(roots)?;
        let mut diagnostics = Vec::new();

        let layers = parse_layers(&vfs, &mut diagnostics)?;
        let palettes = parse_palettes(&vfs, &mut diagnostics)?;
        let surfaces = parse_surfaces(&vfs, &mut diagnostics)?;
        let filters = parse_filters(&vfs, &mut diagnostics)?;
        let body_types = parse_body_types(&vfs, &mut diagnostics)?;
        let characters = parse_characters(&vfs, &mut diagnostics)?;
        let mut items = parse_items(&vfs, &mut diagnostics)?;
        let weapons = parse_weapons(&vfs, &mut diagnostics)?;
        for (id, weapon) in &weapons {
            if let Some(item) = items.get_mut(id) {
                item.shots_per_burst = weapon.shots_per_burst;
            }
        }
        let armours = parse_armours(&vfs, &mut diagnostics)?;
        let load_bearing = parse_load_bearing(&vfs, &mut diagnostics)?;
        let attachment_links = parse_attachment_links(&vfs, &mut diagnostics);
        let engine_settings = parse_engine_settings(&vfs);
        let palette_replacements = parse_palette_replacements(&vfs, &mut diagnostics);

        validate_configuration(
            &vfs,
            &layers,
            &palettes,
            &surfaces,
            &filters,
            &body_types,
            &mut diagnostics,
        );
        let issue_count = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity != "info")
            .count();

        Ok(Self {
            vfs,
            layers,
            surfaces,
            palettes,
            filters,
            body_types,
            characters,
            items,
            weapons,
            armours,
            load_bearing,
            attachment_links,
            engine_settings,
            palette_replacements,
            diagnostics,
            issue_count,
            sti_cache: HashMap::new(),
            palette_cache: HashMap::new(),
        })
    }

    pub fn summary(&self) -> WorkspaceSummaryDto {
        let mut characters: Vec<_> = self
            .characters
            .values()
            .filter(|character| character.id != 200 && !character.name.trim().is_empty())
            .map(|character| CharacterDto {
                id: character.id,
                name: character.name.clone(),
                nickname: character.nickname.clone(),
                body_type: character.body_type,
                body_type_name: character.body_type_name().to_string(),
                face_index: character.face_index,
                hair_palette: character.hair_palette.clone(),
                skin_palette: character.skin_palette.clone(),
                vest_palette: character.vest_palette.clone(),
                pants_palette: character.pants_palette.clone(),
            })
            .collect();
        characters.sort_by_key(|character| character.id);

        let mut items: Vec<_> = self
            .items
            .values()
            .map(|item| {
                let load_bearing = (item.item_class == IC_LBEGEAR)
                    .then(|| self.load_bearing.get(&item.class_index))
                    .flatten();
                ItemDto {
                    id: item.id,
                    name: item.name.clone(),
                    item_class: item.item_class,
                    compatible_slots: compatible_slots(item, &self.armours, &self.load_bearing),
                    lbe_class: load_bearing.map(|lbe| lbe.class),
                    lbe_combo: load_bearing.map(|lbe| lbe.combo),
                    camouflage_kit: item.camouflage_kit,
                }
            })
            .collect();
        items.sort_by_key(|item| item.id);

        WorkspaceSummaryDto {
            roots: self.vfs.root_dtos(),
            characters,
            items,
            layers: self.layers.len(),
            surfaces: self.surfaces.len(),
            filters: self.filters.len(),
            body_types: self.body_types.len(),
            warning_count: self.issue_count,
            diagnostics: self.diagnostics.iter().take(300).cloned().collect(),
        }
    }

    pub fn attachment_options(&self, host_id: u16) -> Vec<AttachmentOptionDto> {
        let mut options: Vec<_> = self
            .attachment_links
            .get(&host_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.items.get(id))
            .map(|item| AttachmentOptionDto {
                id: item.id,
                name: item.name.clone(),
            })
            .collect();
        options.sort_by_key(|item| item.id);
        options.dedup_by_key(|item| item.id);
        options
    }

    pub fn preview_context(&self, request: &PreviewRequest) -> Result<PreviewContextDto, String> {
        let soldier = self.soldier_for(request)?;
        let body_type = self.find_body_type(&soldier)?;
        let mut animations = Vec::new();
        for record in animation::catalog() {
            let Some(resolved) = animation::resolve(record, &soldier, &self.items) else {
                continue;
            };
            let resolved_layers = self.resolve_logical_layers(&soldier, body_type, &resolved, 2);
            let frame_counts: Vec<_> = resolved_layers
                .iter()
                .filter(|layer| layer.should_render)
                .filter_map(|layer| {
                    layer
                        .surface
                        .as_ref()
                        .map(|surface| surface.frames_per_direction)
                })
                .collect();
            if !frame_counts.is_empty() {
                animations.push(AnimationDto {
                    id: resolved.state,
                    label: resolved.label,
                    group: resolved.group,
                    resolved_surface: resolved.surface,
                    variant: resolved.variant,
                    frames_per_direction: modal_frame_count(&frame_counts),
                    layer_count: frame_counts.len(),
                });
            }
        }

        if animations.is_empty() {
            let mut physical: BTreeMap<String, (u16, HashSet<String>)> = BTreeMap::new();
            for (layer, props) in &body_type.layer_props {
                for prop in props {
                    for mapping in &prop.surfaces {
                        let Some(surface_id) = &mapping.animation_surface else {
                            continue;
                        };
                        let Some(surface) = self.surfaces.get(&mapping.surface) else {
                            continue;
                        };
                        let entry = physical
                            .entry(surface_id.clone())
                            .or_insert((0, HashSet::new()));
                        entry.0 = entry.0.max(surface.frames_per_direction);
                        entry.1.insert(layer.clone());
                    }
                }
            }
            animations.extend(
                physical
                    .into_iter()
                    .map(|(id, (frames_per_direction, layers))| AnimationDto {
                        label: id.clone(),
                        resolved_surface: id.clone(),
                        id,
                        group: "Physical surfaces".into(),
                        variant: "direct physical surface".into(),
                        frames_per_direction,
                        layer_count: layers.len(),
                    }),
            );
        }

        Ok(PreviewContextDto {
            body_type: body_type.label.clone(),
            weapon_mode: animation::weapon_mode_label(&soldier, &self.items).into(),
            profile_palette: ProfilePaletteDto {
                hair: soldier.character.hair_palette.clone(),
                skin: soldier.character.skin_palette.clone(),
                vest: soldier.character.vest_palette.clone(),
                pants: soldier.character.pants_palette.clone(),
            },
            camouflage: self.camouflage_state(&soldier),
            animations,
        })
    }

    pub(crate) fn soldier_for(&self, request: &PreviewRequest) -> Result<SoldierState, String> {
        let character = self
            .characters
            .get(&request.character_id)
            .cloned()
            .ok_or_else(|| format!("Unknown character profile: {}", request.character_id))?;
        let mut scenario = request.scenario.clone();
        let camo_limit = self.engine_settings.camo_kit_area.max(0);
        scenario.camo = scenario.camo.clamp(0, camo_limit);
        scenario.urban_camo = scenario.urban_camo.clamp(0, camo_limit);
        scenario.desert_camo = scenario.desert_camo.clamp(0, camo_limit);
        scenario.snow_camo = scenario.snow_camo.clamp(0, camo_limit);
        Ok(SoldierState {
            character,
            inventory: request.inventory.clone(),
            attachments: request.attachments.clone(),
            scenario,
        })
    }

    pub(crate) fn filter_context(&self) -> FilterContext<'_> {
        FilterContext {
            filters: &self.filters,
            items: &self.items,
            weapons: &self.weapons,
            armours: &self.armours,
        }
    }

    pub(crate) fn find_body_type(&self, soldier: &SoldierState) -> Result<&BodyType, String> {
        let context = self.filter_context();
        let mut errors = Vec::new();
        for body_type in &self.body_types {
            // BodyType::Match in JA2 deliberately treats an absent filter as
            // matching nothing. An empty body type with a matching filter is
            // an exclusion record that disables LOBOT for that soldier.
            if body_type.filter.is_none() {
                continue;
            }
            match filter::matches(body_type.filter.as_deref(), soldier, &context) {
                Ok(true) if body_type.layer_props.is_empty() => {
                    return Err(format!(
                        "LOBOT is disabled for {} by {}",
                        soldier.character.name, body_type.label
                    ));
                }
                Ok(true) => return Ok(body_type),
                Ok(false) => {}
                Err(error) => errors.push(error),
            }
        }
        if errors.is_empty() {
            Err(format!(
                "No LogicalBodyType matches {} ({})",
                soldier.character.name,
                soldier.character.body_type_name()
            ))
        } else {
            Err(format!(
                "No LogicalBodyType could be selected: {}",
                errors.join("; ")
            ))
        }
    }
}

fn parse_layers(vfs: &Vfs, diagnostics: &mut Vec<DiagnosticDto>) -> Result<Vec<LayerDef>, String> {
    let (text, source) = vfs.read_text(LAYERS_XML)?;
    let document = parse_document(&text, &source)?;
    let root = document
        .descendants()
        .find(|node| node.has_tag_name("Layers"))
        .ok_or_else(|| "Layers.xml has no <Layers> root".to_string())?;
    let mut layers = Vec::new();
    let mut order = 0usize;
    for node in root.children().filter(|node| node.has_tag_name("Layer")) {
        parse_layer_node(node, [0; 8], &mut order, &mut layers, diagnostics, &source);
    }
    if layers.is_empty() {
        return Err("Layers.xml does not define any layers".into());
    }
    Ok(layers)
}

fn parse_layer_node(
    node: Node<'_, '_>,
    inherited_z: [i32; 8],
    order: &mut usize,
    layers: &mut Vec<LayerDef>,
    diagnostics: &mut Vec<DiagnosticDto>,
    source: &std::path::Path,
) {
    const Z_ATTRIBUTES: [&str; 8] = [
        "zindex_north",
        "zindex_northeast",
        "zindex_east",
        "zindex_southeast",
        "zindex_south",
        "zindex_southwest",
        "zindex_west",
        "zindex_northwest",
    ];
    let mut z_index = inherited_z;
    for (direction, attribute) in Z_ATTRIBUTES.iter().enumerate() {
        if let Some(value) = node
            .attribute(*attribute)
            .and_then(|value| value.parse().ok())
        {
            z_index[direction] = value;
        }
    }
    if !Z_ATTRIBUTES
        .iter()
        .any(|attribute| node.attribute(*attribute).is_some())
        && node
            .text()
            .is_some_and(|text| text.trim().parse::<i32>().is_ok())
    {
        diagnostics.push(DiagnosticDto::info(
            "legacy-layer-z-text",
            format!(
                "Layer {} stores a numeric z value as text; the current JA2 LOBOT loader ignores it",
                node.attribute("name").unwrap_or("<unnamed>")
            ),
            Some(source.display().to_string()),
        ));
    }
    if let Some(name) = node.attribute("name") {
        layers.push(LayerDef {
            name: name.to_string(),
            render: parse_bool(node.attribute("render")).unwrap_or(true),
            render_shadows: parse_bool(node.attribute("shadow")).unwrap_or(false),
            z_index,
            declaration_order: *order,
        });
        *order += 1;
    }
    for child in node.children().filter(|child| child.has_tag_name("Layer")) {
        parse_layer_node(child, z_index, order, layers, diagnostics, source);
    }
}

fn parse_palettes(
    vfs: &Vfs,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> Result<HashMap<String, PaletteDef>, String> {
    let (text, source) = vfs.read_text(PALETTES_XML)?;
    let document = parse_document(&text, &source)?;
    let mut palettes = HashMap::new();
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("Palette"))
    {
        let (Some(name), Some(file)) = (node.attribute("name"), node.attribute("filename")) else {
            diagnostics.push(DiagnosticDto::error(
                "invalid-palette",
                "Palette is missing name or filename",
                Some(source.display().to_string()),
            ));
            continue;
        };
        palettes.insert(
            name.to_string(),
            PaletteDef {
                name: name.to_string(),
                file: file.to_string(),
            },
        );
    }
    Ok(palettes)
}

fn parse_surfaces(
    vfs: &Vfs,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> Result<HashMap<String, SurfaceDef>, String> {
    let expanded = xml::load_expanded(vfs, SURFACES_XML, "TableData")?;
    diagnostics.extend(expanded.diagnostics);
    let document = parse_document(&expanded.text, &expanded.source)?;
    let mut surfaces = HashMap::new();
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("AnimSurface"))
    {
        let (Some(name), Some(file)) = (node.attribute("name"), node.attribute("file")) else {
            diagnostics.push(DiagnosticDto::error(
                "invalid-surface",
                "AnimSurface is missing name or file",
                Some(expanded.source.display().to_string()),
            ));
            continue;
        };
        let surface = SurfaceDef {
            name: name.to_string(),
            file: file.to_string(),
            directions: node
                .attribute("directions")
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(8),
            frames_per_direction: node
                .attribute("framesperdir")
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(1),
            alpha: node.attribute("alpha") == Some("1"),
        };
        if surfaces.insert(name.to_string(), surface).is_some() {
            diagnostics.push(DiagnosticDto::error(
                "duplicate-surface",
                format!("Animation surface {name} is defined more than once"),
                Some(expanded.source.display().to_string()),
            ));
        }
    }
    if surfaces.is_empty() {
        return Err("AnimationSurfaces.xml produced no <AnimSurface> records".into());
    }
    Ok(surfaces)
}

fn parse_filters(
    vfs: &Vfs,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> Result<HashMap<String, FilterDef>, String> {
    let (text, source) = vfs.read_text(FILTERS_XML)?;
    let document = parse_document(&text, &source)?;
    let mut filters = HashMap::new();
    for filter_node in document
        .descendants()
        .filter(|node| node.has_tag_name("Filter"))
    {
        let Some(name) = filter_node.attribute("name") else {
            continue;
        };
        let mut criteria = Vec::new();
        for child in filter_node.children().filter(Node::is_element) {
            if child.has_tag_name("AND") || child.has_tag_name("OR") {
                let operation = if child.has_tag_name("OR") {
                    Operation::Or
                } else {
                    Operation::And
                };
                for criterion in child.children().filter(Node::is_element) {
                    if let Some(parsed) = parse_criterion(criterion, operation) {
                        criteria.push(parsed);
                    }
                }
            } else if let Some(parsed) = parse_criterion(child, Operation::And) {
                criteria.push(parsed);
            }
        }
        for criterion in criteria
            .iter()
            .filter(|criterion| criterion.field == "FILTER")
        {
            if criterion
                .values
                .first()
                .is_some_and(|reference| !filters.contains_key(reference))
            {
                diagnostics.push(DiagnosticDto::error(
                    "forward-filter-reference",
                    format!(
                        "Filter {name} references {} before it is defined; the game loader rejects this",
                        criterion.values[0]
                    ),
                    Some(source.display().to_string()),
                ));
            }
        }
        let definition = FilterDef { criteria };
        if filters.insert(name.to_string(), definition).is_some() {
            diagnostics.push(DiagnosticDto::error(
                "duplicate-filter",
                format!("Filter {name} is defined more than once"),
                Some(source.display().to_string()),
            ));
        }
    }
    Ok(filters)
}

fn parse_criterion(node: Node<'_, '_>, operation: Operation) -> Option<Criterion> {
    let compare = match node.attribute("op") {
        Some("in") => CompareOp::In,
        Some("btwn") => CompareOp::Between,
        Some("gt") => CompareOp::Greater,
        Some("lt") => CompareOp::Less,
        _ => CompareOp::Eq,
    };
    let text = node.text().unwrap_or("").trim();
    let values = if matches!(compare, CompareOp::In | CompareOp::Between) {
        text.split([',', ' ', '\t', '\r', '\n'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        vec![text.to_string()]
    };
    Some(Criterion {
        field: node.tag_name().name().to_ascii_uppercase(),
        operation,
        compare,
        negate: node.attribute("not").is_some(),
        values,
    })
}

fn parse_body_types(
    vfs: &Vfs,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> Result<Vec<BodyType>, String> {
    let expanded = xml::load_expanded(vfs, BODY_TYPES_XML, "TableData")?;
    diagnostics.extend(expanded.diagnostics);
    let document = parse_document(&expanded.text, &expanded.source)?;
    let mut body_types = Vec::new();
    for (index, node) in document
        .descendants()
        .filter(|node| node.has_tag_name("LogicalBodyType"))
        .enumerate()
    {
        let filter = node
            .attribute("filter")
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let mut layer_configs = HashMap::new();
        for config in node
            .descendants()
            .filter(|descendant| descendant.has_tag_name("LayerConfiguration"))
        {
            if let Some(name) = config.attribute("name") {
                layer_configs.insert(
                    name.to_string(),
                    LayerConfig {
                        render: parse_bool(config.attribute("render")),
                        render_shadows: parse_bool(config.attribute("shadow")),
                    },
                );
            }
        }
        let mut layer_props: HashMap<String, Vec<LayerProp>> = HashMap::new();
        for layer in node
            .descendants()
            .filter(|descendant| descendant.has_tag_name("Layer"))
        {
            let Some(name) = layer.attribute("name") else {
                continue;
            };
            for prop in layer
                .children()
                .filter(|child| child.has_tag_name("LayerProp"))
            {
                let surfaces = prop
                    .children()
                    .filter(|child| child.has_tag_name("Surface"))
                    .filter_map(|surface| {
                        Some(SurfaceMapping {
                            surface: surface.attribute("name")?.to_string(),
                            animation_surface: surface.attribute("animsurface").map(str::to_string),
                            animation_state: surface.attribute("animstate").map(str::to_string),
                        })
                    })
                    .collect();
                layer_props
                    .entry(name.to_string())
                    .or_default()
                    .push(LayerProp {
                        filter: prop
                            .attribute("filter")
                            .filter(|value| !value.is_empty())
                            .map(str::to_string),
                        palette: prop
                            .attribute("palette")
                            .filter(|value| !value.is_empty() && *value != "default")
                            .map(str::to_string),
                        surfaces,
                    });
            }
        }
        body_types.push(BodyType {
            label: filter
                .clone()
                .unwrap_or_else(|| format!("LogicalBodyType {}", index + 1)),
            filter,
            layer_configs,
            layer_props,
        });
    }
    if body_types.is_empty() {
        return Err("LogicalBodyTypes.xml produced no body types".into());
    }
    Ok(body_types)
}

fn parse_characters(
    vfs: &Vfs,
    _diagnostics: &mut Vec<DiagnosticDto>,
) -> Result<HashMap<u16, Character>, String> {
    let (text, source) = vfs.read_text(PROFILES_XML)?;
    let document = parse_document(&text, &source)?;
    let mut characters = HashMap::new();
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("PROFILE"))
    {
        let Some(id) = child_u16(node, "uiIndex") else {
            continue;
        };
        characters.insert(
            id,
            Character {
                id,
                name: child_text(node, "zName").unwrap_or_default(),
                nickname: child_text(node, "zNickname").unwrap_or_default(),
                profile_type: child_i32(node, "Type").unwrap_or(0),
                body_type: child_i32(node, "ubBodyType").unwrap_or(0),
                face_index: child_i32(node, "ubFaceIndex").unwrap_or(200),
                sex: child_i32(node, "bSex").unwrap_or(0),
                exp_level: child_i32(node, "bExpLevel").unwrap_or(1),
                strength: child_i32(node, "bStrength").unwrap_or(50),
                leadership: child_i32(node, "bLeadership").unwrap_or(50),
                wisdom: child_i32(node, "bWisdom").unwrap_or(50),
                hair_palette: child_text(node, "HAIR").unwrap_or_default(),
                skin_palette: child_text(node, "SKIN").unwrap_or_default(),
                vest_palette: child_text(node, "VEST").unwrap_or_default(),
                pants_palette: child_text(node, "PANTS").unwrap_or_default(),
            },
        );
    }
    if characters.is_empty() {
        return Err("MercProfiles.xml produced no character profiles".into());
    }
    Ok(characters)
}

fn parse_items(
    vfs: &Vfs,
    _diagnostics: &mut Vec<DiagnosticDto>,
) -> Result<HashMap<u16, Item>, String> {
    let (text, source) = vfs.read_text(ITEMS_XML)?;
    let document = parse_document(&text, &source)?;
    let mut items = HashMap::new();
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("ITEM"))
    {
        let Some(id) = child_u16(node, "uiIndex") else {
            continue;
        };
        let name = child_text(node, "szLongItemName")
            .filter(|value| !value.is_empty())
            .or_else(|| child_text(node, "szItemName"))
            .unwrap_or_else(|| format!("Item {id}"));
        items.insert(
            id,
            Item {
                id,
                name,
                item_class: child_i32(node, "usItemClass").unwrap_or(0),
                class_index: child_i32(node, "ubClassIndex").unwrap_or(0).max(0) as usize,
                two_handed: child_i32(node, "TwoHanded").unwrap_or(0) != 0,
                rocket_launcher: child_i32(node, "RocketLauncher").unwrap_or(0) != 0,
                grenade_launcher: child_i32(node, "GrenadeLauncher").unwrap_or(0) != 0,
                shots_per_burst: 0,
                camo_bonus: child_i32(node, "CamoBonus").unwrap_or(0),
                urban_camo_bonus: child_i32(node, "UrbanCamoBonus").unwrap_or(0),
                desert_camo_bonus: child_i32(node, "DesertCamoBonus").unwrap_or(0),
                snow_camo_bonus: child_i32(node, "SnowCamoBonus").unwrap_or(0),
                stealth_bonus: child_i32(node, "StealthBonus").unwrap_or(0),
                camouflage_kit: child_i32(node, "CamouflageKit").unwrap_or(0) != 0,
            },
        );
    }
    if items.is_empty() {
        return Err("Items.xml produced no items".into());
    }
    Ok(items)
}

fn parse_weapons(
    vfs: &Vfs,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> Result<HashMap<u16, WeaponStats>, String> {
    let Ok((text, source)) = vfs.read_text(WEAPONS_XML) else {
        diagnostics.push(DiagnosticDto::warning(
            "missing-weapons",
            "Weapons.xml is missing; weapon-driven LOBOT filters will use zero values",
            Some(WEAPONS_XML.into()),
        ));
        return Ok(HashMap::new());
    };
    let document = parse_document(&text, &source)?;
    Ok(document
        .descendants()
        .filter(|node| node.has_tag_name("WEAPON"))
        .filter_map(|node| {
            Some((
                child_u16(node, "uiIndex")?,
                WeaponStats {
                    class: child_i32(node, "ubWeaponClass").unwrap_or(0),
                    kind: child_i32(node, "ubWeaponType").unwrap_or(0),
                    calibre: child_i32(node, "ubCalibre").unwrap_or(0),
                    shots_per_burst: child_i32(node, "ubShotsPerBurst").unwrap_or(0),
                },
            ))
        })
        .collect())
}

fn parse_armours(
    vfs: &Vfs,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> Result<HashMap<usize, ArmourStats>, String> {
    let Ok((text, source)) = vfs.read_text(ARMOURS_XML) else {
        diagnostics.push(DiagnosticDto::warning(
            "missing-armours",
            "Armours.xml is missing; armour-driven LOBOT filters will use zero values",
            Some(ARMOURS_XML.into()),
        ));
        return Ok(HashMap::new());
    };
    let document = parse_document(&text, &source)?;
    Ok(document
        .descendants()
        .filter(|node| node.has_tag_name("ARMOUR"))
        .filter_map(|node| {
            Some((
                child_i32(node, "uiIndex")?.max(0) as usize,
                ArmourStats {
                    class: child_i32(node, "ubArmourClass").unwrap_or(-1),
                    protection: child_i32(node, "ubProtection").unwrap_or(0),
                    coverage: child_i32(node, "ubCoverage").unwrap_or(0),
                },
            ))
        })
        .collect())
}

fn parse_load_bearing(
    vfs: &Vfs,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> Result<HashMap<usize, LoadBearingStats>, String> {
    let Ok((text, source)) = vfs.read_text(LOAD_BEARING_XML) else {
        diagnostics.push(DiagnosticDto::warning(
            "missing-load-bearing-equipment",
            "LoadBearingEquipment.xml is missing; LBE inventory slots will have no choices",
            Some(LOAD_BEARING_XML.into()),
        ));
        return Ok(HashMap::new());
    };
    let document = parse_document(&text, &source)?;
    Ok(document
        .descendants()
        .filter(|node| node.has_tag_name("LOADBEARINGEQUIPMENT"))
        .filter_map(|node| {
            Some((
                child_i32(node, "lbeIndex")?.max(0) as usize,
                LoadBearingStats {
                    class: child_i32(node, "lbeClass").unwrap_or(0),
                    combo: child_i32(node, "lbeCombo").unwrap_or(0),
                },
            ))
        })
        .collect())
}

fn parse_attachment_links(
    vfs: &Vfs,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> HashMap<u16, Vec<u16>> {
    let Ok((text, source)) = vfs.read_text(ATTACHMENTS_XML) else {
        diagnostics.push(DiagnosticDto::warning(
            "missing-attachments",
            "Attachments.xml is missing; armour attachment selectors will have no choices",
            Some(ATTACHMENTS_XML.into()),
        ));
        return HashMap::new();
    };
    let Ok(document) = Document::parse(&text) else {
        diagnostics.push(DiagnosticDto::error(
            "invalid-attachments",
            "Attachments.xml could not be parsed",
            Some(source.display().to_string()),
        ));
        return HashMap::new();
    };
    let mut links: HashMap<u16, Vec<u16>> = HashMap::new();
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("ATTACHMENT"))
    {
        let (Some(attachment), Some(host)) = (
            child_u16(node, "attachmentIndex"),
            child_u16(node, "itemIndex"),
        ) else {
            continue;
        };
        links.entry(host).or_default().push(attachment);
    }
    for attachments in links.values_mut() {
        attachments.sort_unstable();
        attachments.dedup();
    }
    links
}

fn parse_engine_settings(vfs: &Vfs) -> EngineSettings {
    let mut settings = EngineSettings::default();
    if let Ok((text, _)) = vfs.read_text(JA2_OPTIONS_INI) {
        if let Some(value) =
            ini_value(&text, "CAMO_KIT_USABLE_AREA").and_then(|value| value.parse::<i32>().ok())
        {
            settings.camo_kit_area = value.clamp(0, 100);
        }
    }
    if let Ok((text, _)) = vfs.read_text(ITEM_SETTINGS_INI) {
        if let Some(value) = ini_value(&text, "CAMO_LBE_OVER_VEST_MODIFIER")
            .and_then(|value| value.parse::<f32>().ok())
        {
            settings.camo_lbe_over_vest = value.clamp(0.0, 1.0);
        }
        if let Some(value) = ini_value(&text, "CAMO_LBE_OVER_PANTS_MODIFIER")
            .and_then(|value| value.parse::<f32>().ok())
        {
            settings.camo_lbe_over_pants = value.clamp(0.0, 1.0);
        }
    }
    settings
}

fn ini_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|raw| {
        let line = raw.split_once(';').map_or(raw, |(value, _)| value).trim();
        let (candidate, value) = line.split_once('=')?;
        candidate
            .trim()
            .eq_ignore_ascii_case(key)
            .then(|| value.trim())
    })
}

fn compatible_slots(
    item: &Item,
    armours: &HashMap<usize, ArmourStats>,
    load_bearing: &HashMap<usize, LoadBearingStats>,
) -> Vec<String> {
    const IC_BLADE: i32 = 0x0000_0004;
    const IC_THROWING_KNIFE: i32 = 0x0000_0008;
    const IC_GRENADE: i32 = 0x0000_0100;
    const IC_AMMO: i32 = 0x0000_0400;
    const IC_ARMOUR: i32 = 0x0000_0800;
    const IC_FACE: i32 = 0x0000_8000;
    const IC_MONEY: i32 = 0x2000_0000;

    if item.id == 0 {
        return Vec::new();
    }

    // JA2 permits ordinary objects in either hand. The remaining slots have
    // explicit class checks in ValidItemForSlot().
    let mut slots = vec!["HANDPOS", "SECONDHANDPOS"];
    match item.item_class {
        IC_ARMOUR => match armours.get(&item.class_index).map(|armour| armour.class) {
            Some(0) => slots.push("HELMETPOS"),
            Some(1) => slots.push("VESTPOS"),
            Some(2) => slots.push("LEGPOS"),
            _ => {}
        },
        IC_FACE => {
            slots.push("HEAD1POS");
            slots.push("HEAD2POS");
        }
        IC_LBEGEAR => match load_bearing.get(&item.class_index).map(|lbe| lbe.class) {
            Some(1) => {
                slots.push("LTHIGHPOCKPOS");
                slots.push("RTHIGHPOCKPOS");
            }
            Some(2) => slots.push("VESTPOCKPOS"),
            Some(3) => slots.push("CPACKPOCKPOS"),
            Some(4) => slots.push("BPACKPOCKPOS"),
            _ => {}
        },
        _ => {}
    }

    if item.item_class & (IC_AMMO | IC_GRENADE | IC_MONEY) == 0 {
        slots.push("GUNSLINGPOCKPOS");
    }
    if item.item_class & (IC_BLADE | IC_THROWING_KNIFE) != 0 {
        slots.push("KNIFEPOCKPOS");
    }

    slots.into_iter().map(str::to_string).collect()
}

fn parse_palette_replacements(
    vfs: &Vfs,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> PaletteReplacementDb {
    let result = (|| {
        let (bytes, _) = vfs.read(SOLDIER_PALETTES)?;
        let mut cursor = 0usize;
        let range_count = read_u32(&bytes, &mut cursor)? as usize;
        let _replacement_counts = read_slice(&bytes, &mut cursor, range_count)?;
        let mut ranges = Vec::with_capacity(range_count);
        for _ in 0..range_count {
            let range = read_slice(&bytes, &mut cursor, 2)?;
            ranges.push((range[0], range[1]));
        }
        let replacement_count = read_u32(&bytes, &mut cursor)? as usize;
        let mut replacements = HashMap::new();
        for _ in 0..replacement_count {
            let palette_type = read_slice(&bytes, &mut cursor, 1)?[0] as usize;
            let id_bytes = read_slice(&bytes, &mut cursor, 30)?;
            let id_end = id_bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(id_bytes.len());
            let id = String::from_utf8_lossy(&id_bytes[..id_end])
                .trim()
                .to_ascii_uppercase();
            let color_count = read_slice(&bytes, &mut cursor, 1)?[0] as usize;
            let colors = read_slice(&bytes, &mut cursor, color_count * 3)?.to_vec();
            let (start, end) = *ranges
                .get(palette_type)
                .ok_or_else(|| format!("JA2PAL.DAT references unknown range {palette_type}"))?;
            if color_count != end.saturating_sub(start) as usize + 1 {
                return Err(format!(
                    "JA2PAL.DAT replacement {id} has {color_count} colours for range {start}–{end}"
                ));
            }
            replacements.insert(id, PaletteReplacement { start, colors });
        }
        Ok::<_, String>(PaletteReplacementDb { replacements })
    })();

    match result {
        Ok(database) => database,
        Err(error) => {
            diagnostics.push(DiagnosticDto::warning(
                "soldier-palette-data",
                format!("Profile palette replacements are unavailable: {error}"),
                Some(SOLDIER_PALETTES.into()),
            ));
            PaletteReplacementDb::default()
        }
    }
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let value = read_slice(bytes, cursor, 4)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_slice<'a>(bytes: &'a [u8], cursor: &mut usize, length: usize) -> Result<&'a [u8], String> {
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| "JA2PAL.DAT offset overflow".to_string())?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| "JA2PAL.DAT ends unexpectedly".to_string())?;
    *cursor = end;
    Ok(value)
}

fn validate_configuration(
    vfs: &Vfs,
    layers: &[LayerDef],
    palettes: &HashMap<String, PaletteDef>,
    surfaces: &HashMap<String, SurfaceDef>,
    filters: &HashMap<String, FilterDef>,
    body_types: &[BodyType],
    diagnostics: &mut Vec<DiagnosticDto>,
) {
    let layer_names: HashSet<_> = layers.iter().map(|layer| layer.name.as_str()).collect();
    let mut checked_assets = HashSet::new();
    for surface in surfaces.values() {
        if checked_assets.insert(surface.file.to_lowercase()) && !vfs.exists(&surface.file) {
            diagnostics.push(DiagnosticDto::error(
                "missing-sti",
                format!(
                    "Surface {} points to a missing STI: {}",
                    surface.name, surface.file
                ),
                Some(surface.file.clone()),
            ));
        }
    }
    for palette in palettes.values() {
        if !vfs.exists(&palette.file) {
            diagnostics.push(DiagnosticDto::error(
                "missing-palette-file",
                format!(
                    "Palette {} points to a missing file: {}",
                    palette.name, palette.file
                ),
                Some(palette.file.clone()),
            ));
        }
    }
    for body_type in body_types {
        if body_type
            .filter
            .as_ref()
            .is_some_and(|name| !filters.contains_key(name))
        {
            diagnostics.push(DiagnosticDto::error(
                "unknown-body-filter",
                format!("{} uses an unknown filter", body_type.label),
                Some(BODY_TYPES_XML.into()),
            ));
        }
        for (layer, props) in &body_type.layer_props {
            if !layer_names.contains(layer.as_str()) {
                diagnostics.push(DiagnosticDto::error(
                    "unknown-layer",
                    format!("{} references unknown layer {layer}", body_type.label),
                    Some(BODY_TYPES_XML.into()),
                ));
            }
            for prop in props {
                if prop
                    .filter
                    .as_ref()
                    .is_some_and(|name| !filters.contains_key(name))
                {
                    diagnostics.push(DiagnosticDto::error(
                        "unknown-layer-filter",
                        format!(
                            "{} layer {layer} uses unknown filter {}",
                            body_type.label,
                            prop.filter.as_deref().unwrap_or_default()
                        ),
                        Some(BODY_TYPES_XML.into()),
                    ));
                }
                if prop
                    .palette
                    .as_ref()
                    .is_some_and(|name| !palettes.contains_key(name))
                {
                    diagnostics.push(DiagnosticDto::error(
                        "unknown-palette",
                        format!(
                            "{} layer {layer} uses unknown palette {}",
                            body_type.label,
                            prop.palette.as_deref().unwrap_or_default()
                        ),
                        Some(BODY_TYPES_XML.into()),
                    ));
                }
                for mapping in &prop.surfaces {
                    if !surfaces.contains_key(&mapping.surface) {
                        diagnostics.push(DiagnosticDto::error(
                            "unknown-surface",
                            format!(
                                "{} layer {layer} references unknown surface {}",
                                body_type.label, mapping.surface
                            ),
                            Some(BODY_TYPES_XML.into()),
                        ));
                    }
                    if mapping.animation_state.is_some() {
                        diagnostics.push(DiagnosticDto::info(
                            "animation-state-mapping",
                            format!(
                                "{} uses animstate mapping {} (physical-surface previews remain available)",
                                mapping.surface,
                                mapping.animation_state.as_deref().unwrap_or_default()
                            ),
                            Some(BODY_TYPES_XML.into()),
                        ));
                    }
                }
            }
        }
    }
}

fn parse_document<'a>(text: &'a str, source: &std::path::Path) -> Result<Document<'a>, String> {
    Document::parse(text).map_err(|error| format!("Could not parse {}: {error}", source.display()))
}

fn parse_bool(value: Option<&str>) -> Option<bool> {
    value.map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

fn child_text(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.children()
        .find(|child| child.has_tag_name(name))
        .and_then(|child| child.text())
        .map(|value| value.trim().to_string())
}

fn child_i32(node: Node<'_, '_>, name: &str) -> Option<i32> {
    child_text(node, name)?.parse().ok()
}

fn child_u16(node: Node<'_, '_>, name: &str) -> Option<u16> {
    child_text(node, name)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::model::PreviewRequest;

    fn integration_data_roots() -> Option<(PathBuf, PathBuf)> {
        let install = std::env::var_os("LOBOT_TEST_INSTALL").map(PathBuf::from)?;
        let data = install.join("Data");
        let data_113 = install.join("Data-1.13");
        (data.is_dir() && data_113.is_dir()).then_some((data, data_113))
    }

    #[test]
    fn loads_and_renders_configured_real_install_when_available() {
        let Some((data, data_113)) = integration_data_roots() else {
            return;
        };
        let mut workspace = Workspace::load(vec![
            data.to_string_lossy().into_owned(),
            data_113.to_string_lossy().into_owned(),
        ])
        .expect("real 1.13 data should load");
        let summary = workspace.summary();
        assert!(summary.characters.len() > 200);
        assert!(summary.items.len() > 1000);
        assert!(summary.surfaces > 1000);
        let empty_item = summary
            .items
            .iter()
            .find(|item| item.id == 0)
            .expect("empty item");
        assert_eq!(empty_item.lbe_class, None);
        assert_eq!(empty_item.lbe_combo, None);
        let glock = summary
            .items
            .iter()
            .find(|item| item.id == 1)
            .expect("Glock item");
        assert!(glock.compatible_slots.iter().any(|slot| slot == "HANDPOS"));
        assert!(glock
            .compatible_slots
            .iter()
            .any(|slot| slot == "GUNSLINGPOCKPOS"));
        assert!(!glock
            .compatible_slots
            .iter()
            .any(|slot| slot == "HELMETPOS"));
        assert!(summary
            .items
            .iter()
            .any(|item| item.compatible_slots.iter().any(|slot| slot == "HELMETPOS")));
        for slot in [
            "VESTPOCKPOS",
            "LTHIGHPOCKPOS",
            "RTHIGHPOCKPOS",
            "CPACKPOCKPOS",
            "BPACKPOCKPOS",
        ] {
            assert!(
                summary.items.iter().any(|item| item
                    .compatible_slots
                    .iter()
                    .any(|candidate| candidate == slot)),
                "real 1.13 data should offer compatible items for {slot}"
            );
        }

        let mut request = PreviewRequest {
            character_id: 0,
            inventory: HashMap::from([("HANDPOS".into(), 1)]),
            attachments: HashMap::new(),
            scenario: Default::default(),
            animation: String::new(),
            direction: 2,
            frame: 0,
        };
        let context = workspace.preview_context(&request).expect("Barry context");
        let standing = context
            .animations
            .iter()
            .find(|animation| animation.id == "STANDING")
            .expect("standing animation");
        assert_eq!(standing.resolved_surface, "RGMPISTOLBREATH");
        assert_eq!(standing.variant, "handgun substitution");
        assert!(!context.profile_palette.skin.is_empty());
        request.animation = standing.id.clone();
        let preview = workspace
            .render_preview(&request)
            .expect("standing preview");
        assert_eq!(preview.animation_state, "STANDING");
        assert_eq!(preview.resolved_surface, "RGMPISTOLBREATH");
        assert!(preview.png_data_url.is_some());
        assert!(preview
            .layers
            .iter()
            .any(|layer| layer.status == "rendered"));
        assert!(preview
            .layers
            .iter()
            .any(|layer| layer.layer == "gun" && layer.status == "rendered"));

        request.character_id = 1;
        let blood_preview = workspace
            .render_preview(&request)
            .expect("Blood standing preview");
        assert_ne!(
            preview.png_data_url, blood_preview.png_data_url,
            "profile palette skinning should change the composite"
        );

        request.character_id = 0;
        request.inventory = HashMap::from([("VESTPOS".into(), 809)]);
        request.animation = "STANDING".into();
        let ghillie_context = workspace
            .preview_context(&request)
            .expect("ghillie context");
        assert_eq!(ghillie_context.camouflage.total[0], 50);
        let ghillie_preview = workspace.render_preview(&request).expect("ghillie preview");
        assert!(ghillie_preview.png_data_url.is_some());
        if workspace.vfs.exists("ANIMS/camo.COL") {
            assert!(!ghillie_preview
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "camouflage-palette-fallback"));
        } else if workspace.vfs.exists("ANIMS/forest.col") {
            assert!(ghillie_preview
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "camouflage-palette-fallback"));
            assert!(!ghillie_preview
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "camouflage-palette"));
        }
        assert_eq!(workspace.palette_replacements.replacements.len(), 28);
    }

    #[test]
    fn loads_configured_mod_overlay_when_available() {
        let Some((data, data_113)) = integration_data_roots() else {
            return;
        };
        let Some(overlay) = std::env::var_os("LOBOT_TEST_OVERLAY").map(PathBuf::from) else {
            return;
        };
        if !overlay.is_dir() {
            return;
        }
        let mut workspace = Workspace::load(vec![
            data.to_string_lossy().into_owned(),
            data_113.to_string_lossy().into_owned(),
            overlay.to_string_lossy().into_owned(),
        ])
        .expect("configured overlay should remain inspectable");
        let summary = workspace.summary();
        println!(
            "mod overlay: {} surfaces, {} filters, {} findings",
            summary.surfaces, summary.filters, summary.warning_count
        );
        for diagnostic in summary
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == "error")
            .take(8)
        {
            println!("{}: {}", diagnostic.code, diagnostic.message);
        }
        assert!(summary.items.len() > 1000);
        assert_eq!(summary.roots.len(), 3);

        let request = PreviewRequest {
            character_id: 1,
            inventory: HashMap::from([("HANDPOS".into(), 1)]),
            attachments: HashMap::new(),
            scenario: Default::default(),
            animation: "STANDING".into(),
            direction: 2,
            frame: 0,
        };
        let context = workspace.preview_context(&request).expect("Blood context");
        let standing = context
            .animations
            .iter()
            .find(|animation| animation.id == "STANDING")
            .expect("standing action");
        assert_eq!(standing.resolved_surface, "RGMPISTOLBREATH");
        let preview = workspace
            .render_preview(&request)
            .expect("AIMNAS Glock preview");
        let gun_surface = preview
            .layers
            .iter()
            .find(|layer| layer.layer == "gun" && layer.status == "rendered")
            .and_then(|layer| layer.surface.as_deref())
            .expect("Glock layer should render");
        assert!(
            gun_surface.contains("PISTOL"),
            "expected a pistol surface, got {gun_surface}"
        );
        let audit = workspace
            .audit_workspace(&request)
            .expect("AIMNAS completeness audit");
        assert!(audit.animations_checked > 20);
        assert!(audit.surfaces_checked > 100);
    }
}
