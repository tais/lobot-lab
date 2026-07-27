use std::{
    cmp::{max, min},
    collections::{HashMap, HashSet},
};

use base64::{engine::general_purpose::STANDARD, Engine};

use crate::{
    animation::{self, ResolvedAnimation},
    filter,
    loader::Workspace,
    model::{
        AuditDto, AuditFindingDto, BodyType, CamouflageDto, Character, DiagnosticDto, LayerDef,
        PreviewDto, PreviewLayerDto, PreviewRequest, SoldierState, SurfaceDef,
    },
    sti::{self, StiSubImage},
};

#[derive(Clone)]
struct SelectedSurface {
    layer: LayerDef,
    surface: SurfaceDef,
    filter: Option<String>,
    palette: Option<String>,
    render_shadows: bool,
}

#[derive(Clone)]
struct Drawable {
    image: StiSubImage,
    palette: Vec<u8>,
    alpha_mask: Option<StiSubImage>,
    render_shadows: bool,
    report_index: usize,
}

#[derive(Clone)]
pub(crate) struct ResolvedLayer {
    pub layer: LayerDef,
    pub surface_name: Option<String>,
    pub surface: Option<SurfaceDef>,
    pub filter: Option<String>,
    pub palette: Option<String>,
    pub should_render: bool,
    pub render_shadows: bool,
    pub detail: Option<String>,
}

impl Workspace {
    pub(crate) fn resolve_logical_layers(
        &self,
        soldier: &SoldierState,
        body_type: &BodyType,
        resolved_animation: &ResolvedAnimation,
        world_direction: u8,
    ) -> Vec<ResolvedLayer> {
        let world_direction = world_direction.min(7);
        let filter_context = self.filter_context();
        let mut ordered_layers = self.layers.clone();
        ordered_layers.sort_by_key(|layer| {
            (
                layer.z_index[world_direction as usize],
                layer.declaration_order,
            )
        });

        ordered_layers
            .into_iter()
            .map(|layer| {
                let config = body_type.layer_configs.get(&layer.name);
                let should_render = config
                    .and_then(|config| config.render)
                    .unwrap_or(layer.render);
                let render_shadows = config
                    .and_then(|config| config.render_shadows)
                    .unwrap_or(layer.render_shadows);
                let mut chosen = None;
                let mut filter_error = None;
                if let Some(props) = body_type.layer_props.get(&layer.name) {
                    'lookup: for match_state in [true, false] {
                        for prop in props {
                            let mapping = prop.surfaces.iter().find(|mapping| {
                                if match_state {
                                    mapping.animation_state.as_deref()
                                        == Some(resolved_animation.state.as_str())
                                } else {
                                    mapping.animation_surface.as_deref()
                                        == Some(resolved_animation.surface.as_str())
                                }
                            });
                            let Some(mapping) = mapping else {
                                continue;
                            };
                            match filter::matches(
                                prop.filter.as_deref(),
                                soldier,
                                &filter_context,
                            ) {
                                Ok(true) => {
                                    chosen = Some((mapping, prop));
                                    break 'lookup;
                                }
                                Ok(false) => {}
                                Err(error) => filter_error = Some(error),
                            }
                        }
                    }
                }

                let Some((mapping, prop)) = chosen else {
                    return ResolvedLayer {
                        layer,
                        surface_name: None,
                        surface: None,
                        filter: None,
                        palette: None,
                        should_render,
                        render_shadows,
                        detail: filter_error.or_else(|| {
                            Some(format!(
                                "No mapping for state {} or resolved surface {} with this inventory",
                                resolved_animation.state, resolved_animation.surface
                            ))
                        }),
                    };
                };
                ResolvedLayer {
                    surface_name: Some(mapping.surface.clone()),
                    surface: self.surfaces.get(&mapping.surface).cloned(),
                    filter: prop.filter.clone(),
                    palette: prop.palette.clone(),
                    should_render,
                    render_shadows,
                    detail: (!self.surfaces.contains_key(&mapping.surface)).then(|| {
                        format!(
                            "LogicalBodyType references undefined surface {}",
                            mapping.surface
                        )
                    }),
                    layer,
                }
            })
            .collect()
    }

    pub fn render_preview(&mut self, request: &PreviewRequest) -> Result<PreviewDto, String> {
        let soldier = self.soldier_for(request)?;
        let body_type = self.find_body_type(&soldier)?.clone();
        let resolved_animation =
            animation::resolve_by_state(&request.animation, &soldier, &self.items).unwrap_or_else(
                || ResolvedAnimation {
                    state: request.animation.clone(),
                    label: request.animation.clone(),
                    group: "Physical surfaces".into(),
                    surface: request.animation.clone(),
                    variant: "direct physical surface".into(),
                },
            );
        let world_direction = request.direction.min(7);
        let resolved_layers =
            self.resolve_logical_layers(&soldier, &body_type, &resolved_animation, world_direction);
        let mut reports = Vec::with_capacity(resolved_layers.len());
        let mut selected = Vec::new();
        let mut preview_diagnostics = Vec::new();

        for resolved in resolved_layers {
            let z_index = resolved.layer.z_index[world_direction as usize];
            let Some(surface) = resolved.surface else {
                let missing_surface = resolved.surface_name.is_some();
                reports.push(PreviewLayerDto {
                    layer: resolved.layer.name,
                    z_index,
                    surface: resolved.surface_name,
                    file: None,
                    filter: resolved.filter,
                    palette: resolved.palette,
                    sprite_direction: None,
                    image_index: None,
                    status: if missing_surface {
                        "missing-surface"
                    } else {
                        "unmatched"
                    }
                    .into(),
                    detail: resolved.detail,
                });
                continue;
            };
            let report_index = reports.len();
            reports.push(PreviewLayerDto {
                layer: resolved.layer.name.clone(),
                z_index,
                surface: Some(surface.name.clone()),
                file: Some(surface.file.clone()),
                filter: resolved.filter.clone(),
                palette: resolved.palette.clone(),
                sprite_direction: None,
                image_index: None,
                status: if resolved.should_render {
                    "rendered"
                } else {
                    "hidden"
                }
                .into(),
                detail: if resolved.should_render {
                    Some(match &resolved.filter {
                        Some(filter) => format!(
                            "First matching LayerProp: {filter} · {}",
                            resolved_animation.variant
                        ),
                        None => format!(
                            "Unfiltered fallback LayerProp · {}",
                            resolved_animation.variant
                        ),
                    })
                } else {
                    Some("Layer rendering is disabled by its configuration".into())
                },
            });
            if resolved.should_render {
                selected.push((
                    SelectedSurface {
                        layer: resolved.layer,
                        surface,
                        filter: resolved.filter,
                        palette: resolved.palette,
                        render_shadows: resolved.render_shadows,
                    },
                    report_index,
                ));
            }
        }

        let mut drawables = Vec::new();
        let mut first_image_index = None;
        let mut first_sprite_direction = None;
        let camouflage = self.camouflage_state(&soldier);
        let raw_animation_palette = matches!(
            resolved_animation.state.as_str(),
            "CHARIOTS_OF_FIRE" | "BODYEXPLODING" | "CRYO_DEATH" | "CRYO_DEATH_CROUCHED"
        );
        let palette_surface = if raw_animation_palette {
            Some(resolved_animation.surface.clone())
        } else {
            animation::resolve_by_state("STANDING", &soldier, &self.items)
                .map(|standing| standing.surface)
        };
        let engine_base_palette = palette_surface
            .as_deref()
            .and_then(animation::physical_surface_file)
            .and_then(|path| self.load_sti(path).ok())
            .map(|image| image.palette);
        let current_engine_palette = animation::physical_surface_file(&resolved_animation.surface)
            .and_then(|path| self.load_sti(path).ok())
            .map(|image| image.palette);
        let mut use_profile_layer_fallback = false;
        let mut shared_default_palette = if camouflage.palette == "profile" {
            engine_base_palette.clone().map(|palette| {
                if raw_animation_palette {
                    palette
                } else {
                    self.apply_profile_palette(palette, &soldier.character)
                }
            })
        } else {
            let requested_palette = camouflage.palette.as_str();
            let mut effective_palette = requested_palette;
            let mut used_woodland_fallback = false;
            let palette_result = match self.vfs.read(requested_palette) {
                Ok(result) => Ok(result),
                Err(error) if requested_palette.eq_ignore_ascii_case("ANIMS/camo.COL") => {
                    match self.vfs.read("ANIMS/forest.col") {
                        Ok(result) => {
                            effective_palette = "ANIMS/forest.col";
                            used_woodland_fallback = true;
                            Ok(result)
                        }
                        Err(_) => Err(error),
                    }
                }
                Err(error) => Err(error),
            };
            match palette_result {
                Ok((bytes, _)) if decode_col_palette(&bytes).is_some() => {
                    if used_woodland_fallback {
                        preview_diagnostics.push(DiagnosticDto::warning(
                            "camouflage-palette-fallback",
                            format!(
                                "{requested_palette} is missing; using {effective_palette} as the woodland compatibility palette"
                            ),
                            Some(effective_palette.to_string()),
                        ));
                    }
                    decode_col_palette(&bytes)
                }
                Ok((bytes, source)) => {
                    preview_diagnostics.push(DiagnosticDto::error(
                        "camouflage-palette",
                        format!(
                            "{} is {} bytes; expected a 776-byte COL or 768 raw palette",
                            effective_palette,
                            bytes.len()
                        ),
                        Some(source.to_string_lossy().into_owned()),
                    ));
                    let fallback = current_engine_palette.clone().or_else(|| {
                        engine_base_palette
                            .clone()
                            .map(|palette| self.apply_profile_palette(palette, &soldier.character))
                    });
                    use_profile_layer_fallback = fallback.is_none();
                    fallback
                }
                Err(error) => {
                    preview_diagnostics.push(DiagnosticDto::error(
                        "camouflage-palette",
                        error,
                        Some(camouflage.palette.clone()),
                    ));
                    let fallback = current_engine_palette.clone().or_else(|| {
                        engine_base_palette
                            .clone()
                            .map(|palette| self.apply_profile_palette(palette, &soldier.character))
                    });
                    use_profile_layer_fallback = fallback.is_none();
                    fallback
                }
            }
        };
        for (selection, report_index) in selected {
            if request.frame >= selection.surface.frames_per_direction {
                reports[report_index].status = "missing-frame".into();
                reports[report_index].detail = Some(format!(
                    "Frame {} exceeds this surface's {} frames per direction",
                    request.frame, selection.surface.frames_per_direction
                ));
                continue;
            }
            let sprite_direction = sprite_direction(world_direction, selection.surface.directions);
            let image_index = request.frame as usize
                + selection.surface.frames_per_direction as usize * sprite_direction as usize;
            first_image_index.get_or_insert(image_index as u32);
            first_sprite_direction.get_or_insert(sprite_direction);
            reports[report_index].sprite_direction = Some(sprite_direction);
            reports[report_index].image_index = Some(image_index as u32);

            let image = match self.load_sti(&selection.surface.file) {
                Ok(image) => image,
                Err(error) => {
                    reports[report_index].status = "missing-file".into();
                    reports[report_index].detail = Some(error.clone());
                    preview_diagnostics.push(DiagnosticDto::error(
                        "sti-decode",
                        error,
                        Some(selection.surface.file.clone()),
                    ));
                    continue;
                }
            };
            if image.subimages.len()
                != selection.surface.directions as usize
                    * selection.surface.frames_per_direction as usize
            {
                preview_diagnostics.push(DiagnosticDto::warning(
                    "sti-frame-count",
                    format!(
                        "{} declares {}×{}={} frames but contains {} subimages",
                        selection.surface.name,
                        selection.surface.directions,
                        selection.surface.frames_per_direction,
                        selection.surface.directions as usize
                            * selection.surface.frames_per_direction as usize,
                        image.subimages.len()
                    ),
                    Some(selection.surface.file.clone()),
                ));
            }
            let Some(subimage) = image.subimages.get(image_index).cloned() else {
                reports[report_index].status = "missing-frame".into();
                reports[report_index].detail = Some(format!(
                    "Calculated STI subimage {image_index} does not exist ({} subimages)",
                    image.subimages.len()
                ));
                continue;
            };
            let palette = match &selection.palette {
                Some(name) => match self.load_palette(name) {
                    Ok(palette) => palette,
                    Err(error) => {
                        preview_diagnostics.push(DiagnosticDto::error(
                            "palette-load",
                            error,
                            Some(name.clone()),
                        ));
                        image.palette.clone()
                    }
                },
                None => {
                    if shared_default_palette.is_none() {
                        let fallback = if raw_animation_palette
                            || (camouflage.palette != "profile" && !use_profile_layer_fallback)
                        {
                            image.palette.clone()
                        } else {
                            self.apply_profile_palette(image.palette.clone(), &soldier.character)
                        };
                        if !selection.layer.name.eq_ignore_ascii_case("shadow") {
                            shared_default_palette = Some(fallback.clone());
                        }
                        if shared_default_palette.is_none() {
                            fallback
                        } else {
                            shared_default_palette.clone().unwrap()
                        }
                    } else {
                        shared_default_palette.clone().unwrap()
                    }
                }
            };
            let alpha_mask = if selection.surface.alpha {
                let alpha_path = alpha_companion_path(&selection.surface.file)?;
                match self.load_sti(&alpha_path) {
                    Ok(alpha) => match alpha.subimages.get(image_index).cloned() {
                        Some(mask) => Some(mask),
                        None => {
                            reports[report_index].status = "missing-alpha-frame".into();
                            preview_diagnostics.push(DiagnosticDto::error(
                                "alpha-frame",
                                format!("{alpha_path} has no companion subimage {image_index}"),
                                Some(alpha_path),
                            ));
                            continue;
                        }
                    },
                    Err(error) => {
                        reports[report_index].status = "missing-alpha-file".into();
                        preview_diagnostics.push(DiagnosticDto::error(
                            "alpha-file",
                            error,
                            Some(alpha_path),
                        ));
                        continue;
                    }
                }
            } else {
                None
            };
            let _ = (&selection.layer, &selection.filter);
            drawables.push(Drawable {
                image: subimage,
                palette,
                alpha_mask,
                render_shadows: selection.render_shadows,
                report_index,
            });
        }

        if drawables.is_empty() {
            return Ok(PreviewDto {
                png_data_url: None,
                width: 0,
                height: 0,
                body_type: body_type.label,
                animation_state: resolved_animation.state,
                resolved_surface: resolved_animation.surface,
                animation_variant: resolved_animation.variant,
                sprite_direction: first_sprite_direction
                    .unwrap_or_else(|| rotate_world_direction(world_direction)),
                image_index: first_image_index.unwrap_or(0),
                layers: reports,
                diagnostics: preview_diagnostics,
            });
        }

        let (png, width, height) = composite(&drawables)?;
        for drawable in &drawables {
            reports[drawable.report_index].status = "rendered".into();
        }
        Ok(PreviewDto {
            png_data_url: Some(format!("data:image/png;base64,{}", STANDARD.encode(png))),
            width,
            height,
            body_type: body_type.label,
            animation_state: resolved_animation.state,
            resolved_surface: resolved_animation.surface,
            animation_variant: resolved_animation.variant,
            sprite_direction: first_sprite_direction
                .unwrap_or_else(|| rotate_world_direction(world_direction)),
            image_index: first_image_index.unwrap_or(0),
            layers: reports,
            diagnostics: preview_diagnostics,
        })
    }

    pub fn audit_workspace(&mut self, request: &PreviewRequest) -> Result<AuditDto, String> {
        const FINDING_LIMIT: usize = 600;
        let soldier = self.soldier_for(request)?;
        let body_type = self.find_body_type(&soldier)?.clone();
        let mut findings = Vec::new();
        let mut issue_count = 0usize;
        let mut animations_checked = 0usize;
        let mut surfaces_checked = 0usize;
        let mut checked_files = HashSet::new();

        for record in animation::catalog() {
            let Some(resolved) = animation::resolve(record, &soldier, &self.items) else {
                continue;
            };
            let mut animation_has_layers = false;
            animations_checked += 1;
            for direction in 0..8 {
                let layers =
                    self.resolve_logical_layers(&soldier, &body_type, &resolved, direction);
                let drawable: Vec<_> = layers
                    .into_iter()
                    .filter(|layer| layer.should_render)
                    .filter(|layer| layer.surface_name.is_some())
                    .collect();
                if drawable.is_empty() {
                    add_finding(
                        &mut findings,
                        &mut issue_count,
                        FINDING_LIMIT,
                        AuditFindingDto {
                            severity: "warning".into(),
                            code: "no-drawable-layers".into(),
                            animation: resolved.state.clone(),
                            direction: Some(direction),
                            layer: None,
                            message: format!(
                                "No visible layer resolves for {} ({})",
                                resolved.label, resolved.surface
                            ),
                        },
                    );
                    continue;
                }
                animation_has_layers = true;
                let mut frame_counts = HashMap::<u16, Vec<String>>::new();
                for layer in drawable {
                    let Some(surface) = layer.surface else {
                        add_finding(
                            &mut findings,
                            &mut issue_count,
                            FINDING_LIMIT,
                            AuditFindingDto {
                                severity: "error".into(),
                                code: "undefined-surface".into(),
                                animation: resolved.state.clone(),
                                direction: Some(direction),
                                layer: Some(layer.layer.name),
                                message: layer.detail.unwrap_or_else(|| {
                                    format!(
                                        "Undefined logical surface {}",
                                        layer.surface_name.unwrap_or_default()
                                    )
                                }),
                            },
                        );
                        continue;
                    };
                    surfaces_checked += 1;
                    frame_counts
                        .entry(surface.frames_per_direction)
                        .or_default()
                        .push(layer.layer.name.clone());
                    let sprite_direction = sprite_direction(direction, surface.directions);
                    let expected_count =
                        surface.directions as usize * surface.frames_per_direction as usize;
                    match self.load_sti(&surface.file) {
                        Err(error) => add_finding(
                            &mut findings,
                            &mut issue_count,
                            FINDING_LIMIT,
                            AuditFindingDto {
                                severity: "error".into(),
                                code: "missing-or-invalid-sti".into(),
                                animation: resolved.state.clone(),
                                direction: Some(direction),
                                layer: Some(layer.layer.name.clone()),
                                message: error,
                            },
                        ),
                        Ok(image) => {
                            if checked_files.insert(surface.file.to_ascii_lowercase())
                                && image.subimages.len() != expected_count
                            {
                                add_finding(
                                    &mut findings,
                                    &mut issue_count,
                                    FINDING_LIMIT,
                                    AuditFindingDto {
                                        severity: "error".into(),
                                        code: "declared-frame-count".into(),
                                        animation: resolved.state.clone(),
                                        direction: Some(direction),
                                        layer: Some(layer.layer.name.clone()),
                                        message: format!(
                                            "{} declares {}×{}={} frames, but its STI has {}",
                                            surface.name,
                                            surface.directions,
                                            surface.frames_per_direction,
                                            expected_count,
                                            image.subimages.len()
                                        ),
                                    },
                                );
                            }
                            let last_index = surface.frames_per_direction.saturating_sub(1)
                                as usize
                                + surface.frames_per_direction as usize * sprite_direction as usize;
                            if image.subimages.get(last_index).is_none() {
                                add_finding(
                                    &mut findings,
                                    &mut issue_count,
                                    FINDING_LIMIT,
                                    AuditFindingDto {
                                        severity: "error".into(),
                                        code: "missing-direction-frames".into(),
                                        animation: resolved.state.clone(),
                                        direction: Some(direction),
                                        layer: Some(layer.layer.name.clone()),
                                        message: format!(
                                            "{} needs subimage {last_index} for this direction, but has {}",
                                            surface.name,
                                            image.subimages.len()
                                        ),
                                    },
                                );
                            }
                        }
                    }
                    if surface.alpha {
                        let alpha_path = alpha_companion_path(&surface.file)?;
                        match self.load_sti(&alpha_path) {
                            Err(error) => add_finding(
                                &mut findings,
                                &mut issue_count,
                                FINDING_LIMIT,
                                AuditFindingDto {
                                    severity: "error".into(),
                                    code: "missing-alpha-sti".into(),
                                    animation: resolved.state.clone(),
                                    direction: Some(direction),
                                    layer: Some(layer.layer.name.clone()),
                                    message: error,
                                },
                            ),
                            Ok(alpha) if alpha.subimages.len() != expected_count => add_finding(
                                &mut findings,
                                &mut issue_count,
                                FINDING_LIMIT,
                                AuditFindingDto {
                                    severity: "error".into(),
                                    code: "alpha-frame-count".into(),
                                    animation: resolved.state.clone(),
                                    direction: Some(direction),
                                    layer: Some(layer.layer.name.clone()),
                                    message: format!(
                                        "{alpha_path} should have {expected_count} frames, but has {}",
                                        alpha.subimages.len()
                                    ),
                                },
                            ),
                            Ok(_) => {}
                        }
                    }
                }
                if frame_counts.len() > 1 {
                    let mut groups: Vec<_> = frame_counts
                        .into_iter()
                        .map(|(frames, layers)| format!("{frames}: {}", layers.join(", ")))
                        .collect();
                    groups.sort();
                    add_finding(
                        &mut findings,
                        &mut issue_count,
                        FINDING_LIMIT,
                        AuditFindingDto {
                            severity: "warning".into(),
                            code: "layer-frame-mismatch".into(),
                            animation: resolved.state.clone(),
                            direction: Some(direction),
                            layer: None,
                            message: format!(
                                "Resolved layers disagree on frames per direction ({})",
                                groups.join(" · ")
                            ),
                        },
                    );
                }
            }
            if !animation_has_layers {
                animations_checked = animations_checked.saturating_sub(1);
            }
        }

        Ok(AuditDto {
            animations_checked,
            surfaces_checked,
            issue_count,
            truncated: issue_count > findings.len(),
            findings,
        })
    }

    fn load_sti(&mut self, path: &str) -> Result<crate::sti::StiImage, String> {
        let key = path.replace('\\', "/").to_lowercase();
        if let Some(cached) = self.sti_cache.get(&key) {
            return cached.clone();
        }
        let result = self
            .vfs
            .read(path)
            .and_then(|(bytes, _)| sti::decode(&bytes).map_err(|error| format!("{path}: {error}")));
        self.sti_cache.insert(key, result.clone());
        result
    }

    fn load_palette(&mut self, name: &str) -> Result<Vec<u8>, String> {
        if let Some(cached) = self.palette_cache.get(name) {
            return cached.clone();
        }
        let result = (|| {
            let palette = self
                .palettes
                .get(name)
                .ok_or_else(|| format!("Unknown palette: {name}"))?;
            let (bytes, _) = self.vfs.read(&palette.file)?;
            if bytes.len() < 768 {
                return Err(format!(
                    "Palette {} is only {} bytes; expected 768 raw ACT/STP bytes",
                    palette.file,
                    bytes.len()
                ));
            }
            Ok(bytes[..768].to_vec())
        })();
        self.palette_cache.insert(name.to_string(), result.clone());
        result
    }

    fn apply_profile_palette(&self, mut palette: Vec<u8>, character: &Character) -> Vec<u8> {
        for replacement_id in [
            &character.hair_palette,
            &character.vest_palette,
            &character.pants_palette,
            &character.skin_palette,
        ] {
            let Some(replacement) = self
                .palette_replacements
                .replacements
                .get(&replacement_id.to_ascii_uppercase())
            else {
                continue;
            };
            let start = replacement.start as usize * 3;
            let end = start + replacement.colors.len();
            if let Some(target) = palette.get_mut(start..end) {
                target.copy_from_slice(&replacement.colors);
            }
        }
        palette
    }

    pub(crate) fn camouflage_state(&self, soldier: &SoldierState) -> CamouflageDto {
        const ARMOUR_SLOTS: [&str; 3] = ["HELMETPOS", "VESTPOS", "LEGPOS"];
        const LBE_SLOTS: [&str; 5] = [
            "VESTPOCKPOS",
            "LTHIGHPOCKPOS",
            "RTHIGHPOCKPOS",
            "CPACKPOCKPOS",
            "BPACKPOCKPOS",
        ];
        const WEAPON_SLOTS: [&str; 3] = ["HANDPOS", "SECONDHANDPOS", "GUNSLINGPOCKPOS"];
        const IC_WEAPON: i32 = 0x0000_003e;

        let bonuses = |item: &crate::model::Item| {
            [
                item.camo_bonus,
                item.urban_camo_bonus,
                item.desert_camo_bonus,
                item.snow_camo_bonus,
            ]
        };
        let item_and_attachments = |slot: &str| {
            let mut value = [0i32; 4];
            if let Some(item) = soldier
                .inventory
                .get(slot)
                .and_then(|id| self.items.get(id))
            {
                add_bonus(&mut value, bonuses(item));
            }
            if let Some(attachments) = soldier.attachments.get(slot) {
                for attachment in attachments {
                    if let Some(item) = self.items.get(attachment) {
                        add_bonus(&mut value, bonuses(item));
                    }
                }
            }
            value
        };

        let mut worn = [0i32; 4];
        for slot in ARMOUR_SLOTS {
            add_bonus(&mut worn, item_and_attachments(slot));
        }
        if soldier.inventory.get("VESTPOCKPOS").copied().unwrap_or(0) != 0 {
            if let Some(vest) = soldier
                .inventory
                .get("VESTPOS")
                .and_then(|id| self.items.get(id))
            {
                subtract_scaled(
                    &mut worn,
                    bonuses(vest),
                    1.0 - self.engine_settings.camo_lbe_over_vest,
                );
            }
        }
        let thighs = ["LTHIGHPOCKPOS", "RTHIGHPOCKPOS"]
            .into_iter()
            .filter(|slot| soldier.inventory.get(*slot).copied().unwrap_or(0) != 0)
            .count() as f32;
        if thighs > 0.0 {
            if let Some(legs) = soldier
                .inventory
                .get("LEGPOS")
                .and_then(|id| self.items.get(id))
            {
                subtract_scaled(
                    &mut worn,
                    bonuses(legs),
                    (1.0 - self.engine_settings.camo_lbe_over_pants) * thighs / 2.0,
                );
            }
        }
        for slot in LBE_SLOTS {
            add_bonus(&mut worn, item_and_attachments(slot));
        }
        for slot in WEAPON_SLOTS {
            let is_weapon = soldier
                .inventory
                .get(slot)
                .and_then(|id| self.items.get(id))
                .is_some_and(|item| item.item_class & IC_WEAPON != 0);
            if is_weapon {
                add_bonus(&mut worn, item_and_attachments(slot));
            }
        }
        let maximum_worn = (100 - self.engine_settings.camo_kit_area).max(0);
        for value in &mut worn {
            *value = (*value).clamp(0, maximum_worn);
        }
        let maximum_applied = self.engine_settings.camo_kit_area.max(0);
        let applied = [
            soldier.scenario.camo.clamp(0, maximum_applied),
            soldier.scenario.urban_camo.clamp(0, maximum_applied),
            soldier.scenario.desert_camo.clamp(0, maximum_applied),
            soldier.scenario.snow_camo.clamp(0, maximum_applied),
        ];
        let total = std::array::from_fn(|index| applied[index] + worn[index]);

        let mut stealth = 0;
        for slot in ARMOUR_SLOTS {
            if let Some(item) = soldier
                .inventory
                .get(slot)
                .and_then(|id| self.items.get(id))
            {
                stealth += item.stealth_bonus;
            }
            if let Some(attachments) = soldier.attachments.get(slot) {
                stealth += attachments
                    .iter()
                    .filter_map(|id| self.items.get(id))
                    .map(|item| item.stealth_bonus)
                    .sum::<i32>();
            }
        }
        stealth = stealth.min(100);
        let palette = if stealth >= 50 {
            "ANIMS/stealth.col".to_string()
        } else if total.iter().sum::<i32>() >= 50 {
            let highest = total.iter().copied().max().unwrap_or(0);
            // This order deliberately preserves the engine's tie priority.
            if total[0] == highest {
                "ANIMS/camo.COL"
            } else if total[1] == highest {
                "ANIMS/urban.col"
            } else if total[2] == highest {
                "ANIMS/desert.col"
            } else {
                "ANIMS/snow.col"
            }
            .to_string()
        } else {
            "profile".to_string()
        };
        CamouflageDto {
            applied_limit: maximum_applied,
            applied,
            worn,
            total,
            stealth,
            palette,
        }
    }
}

fn add_finding(
    findings: &mut Vec<AuditFindingDto>,
    issue_count: &mut usize,
    limit: usize,
    finding: AuditFindingDto,
) {
    *issue_count += 1;
    if findings.len() < limit {
        findings.push(finding);
    }
}

fn add_bonus(target: &mut [i32; 4], value: [i32; 4]) {
    for index in 0..4 {
        target[index] += value[index];
    }
}

fn decode_col_palette(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() >= 776 {
        // CreateSGPPaletteFromCOLFile skips the eight-byte COL header.
        Some(bytes[8..776].to_vec())
    } else if bytes.len() >= 768 {
        Some(bytes[..768].to_vec())
    } else {
        None
    }
}

fn subtract_scaled(target: &mut [i32; 4], value: [i32; 4], scale: f32) {
    for index in 0..4 {
        target[index] -= (value[index] as f32 * scale) as i32;
    }
}

fn alpha_companion_path(path: &str) -> Result<String, String> {
    let Some(dot) = path.rfind('.') else {
        return Err(format!("Alpha surface filename has no extension: {path}"));
    };
    let mut result = path.to_string();
    result.insert_str(dot, "_A");
    Ok(result)
}

fn composite(drawables: &[Drawable]) -> Result<(Vec<u8>, u32, u32), String> {
    const PADDING: i32 = 12;
    let mut left = i32::MAX;
    let mut top = i32::MAX;
    let mut right = i32::MIN;
    let mut bottom = i32::MIN;
    for drawable in drawables {
        left = min(left, drawable.image.offset_x as i32);
        top = min(top, drawable.image.offset_y as i32);
        right = max(
            right,
            drawable.image.offset_x as i32 + drawable.image.width as i32,
        );
        bottom = max(
            bottom,
            drawable.image.offset_y as i32 + drawable.image.height as i32,
        );
    }
    let width = (right - left + PADDING * 2).max(1) as u32;
    let height = (bottom - top + PADDING * 2).max(1) as u32;
    if width > 2048 || height > 2048 {
        return Err(format!(
            "Composite bounds are implausibly large: {width}×{height}"
        ));
    }
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for drawable in drawables {
        let origin_x = drawable.image.offset_x as i32 - left + PADDING;
        let origin_y = drawable.image.offset_y as i32 - top + PADDING;
        for source_y in 0..drawable.image.height as usize {
            for source_x in 0..drawable.image.width as usize {
                let source_index = source_y * drawable.image.width as usize + source_x;
                if drawable.image.alpha.get(source_index).copied().unwrap_or(0) == 0 {
                    continue;
                }
                let dest_x = origin_x + source_x as i32;
                let dest_y = origin_y + source_y as i32;
                if dest_x < 0 || dest_y < 0 || dest_x >= width as i32 || dest_y >= height as i32 {
                    continue;
                }
                let dest = (dest_y as usize * width as usize + dest_x as usize) * 4;
                let palette_index = drawable.image.indices[source_index] as usize;
                if palette_index == 254 {
                    if drawable.render_shadows {
                        if rgba[dest + 3] == 0 {
                            rgba[dest..dest + 4].copy_from_slice(&[0, 0, 0, 110]);
                        } else {
                            rgba[dest] /= 2;
                            rgba[dest + 1] /= 2;
                            rgba[dest + 2] /= 2;
                        }
                    }
                    continue;
                }
                let palette_offset = palette_index * 3;
                if palette_offset + 2 >= drawable.palette.len() {
                    continue;
                }
                let source_alpha = if let Some(mask) = &drawable.alpha_mask {
                    let mask_index = {
                        let mask_x =
                            source_x as i32 + drawable.image.offset_x as i32 - mask.offset_x as i32;
                        let mask_y =
                            source_y as i32 + drawable.image.offset_y as i32 - mask.offset_y as i32;
                        (mask_x >= 0
                            && mask_y >= 0
                            && mask_x < mask.width as i32
                            && mask_y < mask.height as i32)
                            .then(|| mask_y as usize * mask.width as usize + mask_x as usize)
                    };
                    mask_index
                        .filter(|index| mask.alpha.get(*index).copied().unwrap_or(0) != 0)
                        .and_then(|index| mask.indices.get(index).copied())
                        .unwrap_or(0)
                } else {
                    255
                };
                let inverse = 255u16 - source_alpha as u16;
                let blend = |source: u8, destination: u8| {
                    ((source as u16 * source_alpha as u16 + destination as u16 * inverse) / 255)
                        as u8
                };
                rgba[dest] = blend(drawable.palette[palette_offset], rgba[dest]);
                rgba[dest + 1] = blend(drawable.palette[palette_offset + 1], rgba[dest + 1]);
                rgba[dest + 2] = blend(drawable.palette[palette_offset + 2], rgba[dest + 2]);
                rgba[dest + 3] =
                    source_alpha.saturating_add(((rgba[dest + 3] as u16 * inverse) / 255) as u8);
            }
        }
    }
    Ok((sti::encode_png_rgba(width, height, &rgba)?, width, height))
}

fn rotate_world_direction(direction: u8) -> u8 {
    // gOneCDirection from TileEngine/Isometric Utils.cpp.
    (direction + 1) % 8
}

fn sprite_direction(world_direction: u8, direction_count: u16) -> u8 {
    let rotated = rotate_world_direction(world_direction);
    match direction_count {
        // With an eight-direction preview, ubHiResDirection is the matching
        // cardinal entry in ubExtDirection and gExtOneCDirection rotates it.
        32 => (world_direction * 4 + 4) % 32,
        4 => rotated / 2,
        1 => 0,
        3 => match world_direction {
            7 => 1, // NORTHWEST
            6 => 0, // WEST
            2 => 2, // EAST
            _ => rotated.min(2),
        },
        2 => [0, 0, 1, 1, 0, 1, 1, 0][world_direction as usize],
        _ => rotated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_mapping_matches_engine_tables() {
        assert_eq!(rotate_world_direction(0), 1);
        assert_eq!(rotate_world_direction(7), 0);
        assert_eq!(sprite_direction(0, 4), 0);
        assert_eq!(sprite_direction(7, 4), 0);
        assert_eq!(sprite_direction(2, 3), 2);
        assert_eq!(sprite_direction(6, 2), 1);
        assert_eq!(sprite_direction(0, 32), 4);
        assert_eq!(sprite_direction(7, 32), 0);
    }

    #[test]
    fn col_palettes_skip_the_engine_header() {
        let mut bytes = vec![99; 8];
        bytes.extend((0..768).map(|value| (value % 251) as u8));
        let palette = decode_col_palette(&bytes).unwrap();
        assert_eq!(palette.len(), 768);
        assert_eq!(palette[0], 0);
        assert_eq!(palette[250], 250);
    }
}
