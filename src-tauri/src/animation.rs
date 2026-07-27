use std::{collections::HashMap, sync::OnceLock};

use serde::Deserialize;

use crate::model::{Item, SoldierState};

const IC_GUN: i32 = 0x0000_0002;
const IC_LAUNCHER: i32 = 0x0000_0010;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WeaponMode {
    None,
    Handgun,
    DualHandguns,
    LongGun,
    RocketLauncher,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AnimationRecord {
    pub id: String,
    pub label: String,
    pub base: HashMap<String, String>,
    pub item: HashMap<String, String>,
    #[serde(default, rename = "waterTwoHanded")]
    pub water_two_handed: HashMap<String, String>,
    #[serde(default, rename = "waterOther")]
    pub water_other: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct ResolvedAnimation {
    pub state: String,
    pub label: String,
    pub group: String,
    pub surface: String,
    pub variant: String,
}

pub fn catalog() -> &'static [AnimationRecord] {
    static CATALOG: OnceLock<Vec<AnimationRecord>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str(include_str!("../animation-catalog.json"))
                .expect("embedded animation catalog must be valid")
        })
        .as_slice()
}

pub fn physical_surface_file(surface: &str) -> Option<&'static str> {
    static SURFACES: OnceLock<HashMap<String, String>> = OnceLock::new();
    SURFACES
        .get_or_init(|| {
            serde_json::from_str(include_str!("../physical-surface-catalog.json"))
                .expect("embedded physical surface catalog must be valid")
        })
        .get(surface)
        .map(String::as_str)
}

pub fn resolve(
    record: &AnimationRecord,
    soldier: &SoldierState,
    items: &HashMap<u16, Item>,
) -> Option<ResolvedAnimation> {
    let body = soldier.character.body_type_name();
    let base = record.base.get(body)?;
    let mode = weapon_mode(soldier, items);
    if !available_for_mode(&record.id, mode) {
        return None;
    }
    let one_handed_firearm = matches!(mode, WeaponMode::Handgun | WeaponMode::DualHandguns);
    let two_handed_firearm = mode == WeaponMode::LongGun;
    let dual_handguns = mode == WeaponMode::DualHandguns;

    let mut surface = big_merc_substitution(base, soldier);
    let mut variant = if surface != *base {
        "big-merc stance substitution"
    } else {
        "base surface"
    };

    if soldier.scenario.in_water {
        let water = if two_handed_firearm {
            record.water_two_handed.get(body)
        } else {
            record.water_other.get(body)
        };
        if let Some(water) = water {
            surface = water.clone();
            variant = if two_handed_firearm {
                "mid-water two-handed substitution"
            } else {
                "mid-water unarmed / handgun substitution"
            };
        }
        return Some(ResolvedAnimation {
            state: record.id.clone(),
            label: action_label(&record.id, &record.label),
            group: group_for(&record.id).into(),
            surface,
            variant: variant.into(),
        });
    }

    let mut adjusted_for_item = false;
    let (selected_surface, selected_variant) = if record.id == "STANDING" && dual_handguns {
        adjusted_for_item = true;
        (
            dual_breath(body).unwrap_or(surface.as_str()).to_string(),
            "dual-handgun substitution",
        )
    } else if dual_handguns && is_dual_state(&record.id) {
        (surface.clone(), "dual-handgun state")
    } else if one_handed_firearm {
        let substituted = if record.id == "RUNNING" {
            catalog()
                .iter()
                .find(|candidate| candidate.id == "RUNNING_W_PISTOL")
                .and_then(|candidate| candidate.item.get(body))
                .or_else(|| record.item.get(body))
        } else {
            record.item.get(body)
        };
        match substituted {
            Some(surface) => {
                adjusted_for_item = true;
                (surface.clone(), "handgun substitution")
            }
            None => (surface.clone(), "base surface (no handgun variant)"),
        }
    } else if two_handed_firearm {
        (surface.clone(), "rifle / two-handed base")
    } else if record.id == "STANDING" {
        adjusted_for_item = true;
        (
            nothing_breath(body).unwrap_or(surface.as_str()).to_string(),
            "unarmed / non-firearm substitution",
        )
    } else {
        match record.item.get(body) {
            Some(surface) => {
                adjusted_for_item = true;
                (surface.clone(), "unarmed / non-firearm substitution")
            }
            None => (surface.clone(), variant),
        }
    };
    surface = selected_surface;
    variant = selected_variant;

    if soldier.scenario.injured && record.id == "WALKING" {
        surface = injured_walking(body, adjusted_for_item)
            .unwrap_or(surface.as_str())
            .to_string();
        variant = if adjusted_for_item {
            "injured unarmed / handgun walking substitution"
        } else {
            "injured rifle walking substitution"
        };
    }

    Some(ResolvedAnimation {
        state: record.id.clone(),
        label: action_label(&record.id, &record.label),
        group: group_for(&record.id).into(),
        surface,
        variant: variant.into(),
    })
}

pub fn resolve_by_state(
    state: &str,
    soldier: &SoldierState,
    items: &HashMap<u16, Item>,
) -> Option<ResolvedAnimation> {
    catalog()
        .iter()
        .find(|record| record.id == state)
        .and_then(|record| resolve(record, soldier, items))
}

pub fn weapon_mode_label(soldier: &SoldierState, items: &HashMap<u16, Item>) -> &'static str {
    match weapon_mode(soldier, items) {
        WeaponMode::None => "Unarmed / utility",
        WeaponMode::Handgun => "Single handgun",
        WeaponMode::DualHandguns => "Dual handguns",
        WeaponMode::LongGun => "Rifle / two-handed",
        WeaponMode::RocketLauncher => "Heavy launcher",
    }
}

fn weapon_mode(soldier: &SoldierState, items: &HashMap<u16, Item>) -> WeaponMode {
    let hand = soldier
        .inventory
        .get("HANDPOS")
        .and_then(|id| items.get(id));
    let off_hand = soldier
        .inventory
        .get("SECONDHANDPOS")
        .and_then(|id| items.get(id));
    let Some(hand) = hand else {
        return WeaponMode::None;
    };
    if hand.rocket_launcher {
        return WeaponMode::RocketLauncher;
    }
    if hand.item_class & IC_GUN != 0 && !hand.two_handed {
        let dual = off_hand.is_some_and(|item| {
            item.item_class & IC_GUN != 0
                && !item.two_handed
                && !item.rocket_launcher
                && !hand.grenade_launcher
                && soldier.scenario.second_hand_usable
                && soldier.scenario.second_hand_loaded
                && (!soldier.scenario.burst
                    || (hand.shots_per_burst > 0 && hand.shots_per_burst == item.shots_per_burst))
        });
        return if dual {
            WeaponMode::DualHandguns
        } else {
            WeaponMode::Handgun
        };
    }
    if hand.item_class & IC_LAUNCHER != 0 && !hand.two_handed {
        return WeaponMode::Handgun;
    }
    if hand.item_class & (IC_GUN | IC_LAUNCHER) != 0 {
        return WeaponMode::LongGun;
    }
    WeaponMode::None
}

fn big_merc_substitution(base: &str, soldier: &SoldierState) -> String {
    if soldier.character.body_type_name() != "BIGMALE" {
        return base.to_string();
    }
    let mut surface = base;
    if soldier.scenario.big_merc_badass {
        surface = match surface {
            "BGMSTANDAIM2" => "BGMSTANDAIM",
            "BGMSIDESTEP_R_RDY" => "BGMSIDESTEP_R_RDY2",
            "BGMWALK_R_RDY" => "BGMWALK_R_RDY2",
            _ => surface,
        };
    }
    match surface {
        "BGMSTANDING" if !soldier.scenario.big_merc_alt => "BGMTHREATENSTAND",
        "BGMWALKING" if !soldier.scenario.big_merc_alt => "BGMWALK2",
        "BGMRUNNING" if !soldier.scenario.big_merc_alt => "BGMRUN2",
        "BGMRAISE" if !soldier.scenario.big_merc_alt => "BGMRAISE2",
        "BGM_HIP_AIM" if soldier.scenario.big_merc_alt => "BGMHIPAIMALT",
        "BGMSTANDAIM2" if soldier.scenario.big_merc_alt && !soldier.scenario.big_merc_badass => {
            "BGMSRAIMALT"
        }
        _ => surface,
    }
    .to_string()
}

fn injured_walking(body: &str, adjusted_for_item: bool) -> Option<&'static str> {
    match (body, adjusted_for_item) {
        ("REGMALE" | "STOCKYMALE", true) => Some("RGMHURTWALKINGN"),
        ("BIGMALE", true) => Some("BGMHURTWALKINGN"),
        ("REGFEMALE", true) => Some("RGFHURTWALKINGN"),
        ("REGMALE" | "STOCKYMALE", false) => Some("RGMHURTWALKINGR"),
        ("BIGMALE", false) => Some("BGMHURTWALKINGR"),
        ("REGFEMALE", false) => Some("RGFHURTWALKINGR"),
        _ => None,
    }
}

fn available_for_mode(state: &str, mode: WeaponMode) -> bool {
    let group = group_for(state);
    let dual = is_dual_state(state);
    let pistol_only = matches!(
        state,
        "RUNNING_W_PISTOL"
            | "PISTOL_SHOOT_LOW"
            | "SIDE_STEP_CROUCH_PISTOL"
            | "CROUCHEDMOVE_PISTOL_READY"
    );
    let alternative = state.contains("ALTERNATIVE");
    let heavy_launcher = state.contains("ROCKET") || state.contains("MORTAR");
    let redundant_transition = matches!(state, "RAISE_RIFLE" | "LOWER_RIFLE");

    if group == "Firearms" {
        return match mode {
            WeaponMode::None => false,
            WeaponMode::DualHandguns => dual,
            WeaponMode::Handgun => {
                !dual && !heavy_launcher && !alternative && !redundant_transition
            }
            WeaponMode::LongGun => {
                !dual && !pistol_only && !heavy_launcher && !alternative && !redundant_transition
            }
            WeaponMode::RocketLauncher => heavy_launcher,
        };
    }

    if is_ready_movement(state) {
        return match mode {
            WeaponMode::None | WeaponMode::RocketLauncher => false,
            WeaponMode::DualHandguns => dual,
            WeaponMode::Handgun => {
                !dual
                    && !alternative
                    && !matches!(state, "SIDE_STEP_CROUCH_RIFLE" | "CROUCHEDMOVE_RIFLE_READY")
            }
            WeaponMode::LongGun => !dual && !pistol_only && !alternative,
        };
    }

    // RUNNING uses this record internally to obtain its handgun substitution;
    // it should not appear as a second user-facing Run action.
    state != "RUNNING_W_PISTOL"
}

fn is_dual_state(state: &str) -> bool {
    state.contains("DUAL") || state.contains("DWEL")
}

fn is_ready_movement(state: &str) -> bool {
    contains_any(
        state,
        &[
            "WEAPON_RDY",
            "DUAL_RDY",
            "ALTERNATIVE_RDY",
            "SIDE_STEP_CROUCH_RIFLE",
            "SIDE_STEP_CROUCH_PISTOL",
            "SIDE_STEP_CROUCH_DUAL",
            "CROUCHEDMOVE_RIFLE_READY",
            "CROUCHEDMOVE_PISTOL_READY",
            "CROUCHEDMOVE_DUAL_READY",
        ],
    )
}

fn nothing_breath(body: &str) -> Option<&'static str> {
    match body {
        "REGMALE" | "STOCKYMALE" => Some("RGMNOTHING_STD"),
        "BIGMALE" => Some("BGMNOTHING_STD"),
        "REGFEMALE" => Some("RGFNOTHING_STD"),
        _ => None,
    }
}

fn dual_breath(body: &str) -> Option<&'static str> {
    match body {
        "REGMALE" | "STOCKYMALE" => Some("RGMDBLBREATH"),
        "BIGMALE" => Some("BGMDBLBREATH"),
        "REGFEMALE" => Some("RGFDBLBREATH"),
        _ => None,
    }
}

fn action_label(state: &str, fallback: &str) -> String {
    let stance = if state.contains("PRONE") {
        Some("prone")
    } else if state.contains("CROUCH") {
        Some("crouched")
    } else if state.contains("STAND") || state.contains("STANDING") {
        Some("standing")
    } else {
        None
    };
    let action = if state.contains("UNJAM") {
        if state.contains("LOW") {
            "Clear low weapon jam"
        } else {
            "Clear weapon jam"
        }
    } else if state.contains("BURST") {
        if state.contains("SPREAD") {
            "Spread burst"
        } else if state.contains("LOW") {
            "Low burst"
        } else {
            "Burst fire"
        }
    } else if state.starts_with("READY_") {
        "Ready weapon"
    } else if state.starts_with("AIM_") {
        "Aim"
    } else if state.starts_with("SHOOT_") || state.starts_with("FIRE_") {
        if state.contains("LOW") {
            "Fire low"
        } else {
            "Fire"
        }
    } else if state.starts_with("END_") {
        "Lower weapon"
    } else if is_ready_movement(state) {
        if state.contains("SIDE_STEP") {
            "Side-step with weapon ready"
        } else if state.contains("CROUCHEDMOVE") {
            "Crouch-move with weapon ready"
        } else {
            "Walk with weapon ready"
        }
    } else {
        return fallback.split_whitespace().collect::<Vec<_>>().join(" ");
    };
    match stance {
        Some(stance) => format!("{action} · {stance}"),
        None => action.into(),
    }
}

fn group_for(state: &str) -> &'static str {
    if contains_any(
        state,
        &[
            "HIT",
            "DIE",
            "DEATH",
            "DYING",
            "HURT",
            "FALLBACK",
            "FLYBACK",
            "FALLOFF",
            "BODYEXPLOD",
            "CRYO",
        ],
    ) {
        "Hits & death"
    } else if contains_any(
        state,
        &[
            "SHOOT",
            "BURST",
            "AIM_",
            "READY_RIFLE",
            "READY_DUAL",
            "END_RIFLE",
            "END_DUAL",
            "READY_ALTERNATIVE",
            "UNJAM",
            "RAISE_RIFLE",
            "LOWER_RIFLE",
            "ROCKET",
            "MORTAR",
        ],
    ) {
        "Firearms"
    } else if contains_any(
        state,
        &[
            "PUNCH",
            "KICK",
            "STAB",
            "SLICE",
            "KNIFE",
            "THROW",
            "LOB_",
            "CROWBAR",
            "BAYONET",
            "DECAPITATE",
            "SLAP",
            "SWIPE",
            "BITE",
        ],
    ) {
        "Melee & throwing"
    } else if contains_any(
        state,
        &[
            "WALK",
            "RUN",
            "SWAT",
            "CRAWL",
            "SIDE_STEP",
            "CLIMB",
            "JUMP",
            "HOP",
            "ROLL",
            "WATER",
            "SWIM",
            "TRED",
        ],
    ) {
        "Movement"
    } else if contains_any(
        state,
        &[
            "OPEN", "CLOSE", "PICKUP", "DROP", "GIVE", "PASS", "STEAL", "AID", "DOCTOR", "PATIENT",
            "REPAIR", "REFUEL", "REMOTE", "BOMB", "RADIO", "BLOOD", "CUTTING", "LOCK", "CATCH",
            "SWITCH", "ATTACH",
        ],
    ) {
        "Interaction"
    } else {
        "Idle & stance"
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Character;

    fn character() -> Character {
        Character {
            id: 1,
            name: "Test".into(),
            nickname: "Test".into(),
            profile_type: 1,
            body_type: 0,
            face_index: 1,
            sex: 0,
            exp_level: 1,
            strength: 50,
            leadership: 50,
            wisdom: 50,
            hair_palette: "BLACKHEAD".into(),
            skin_palette: "PINKSKIN".into(),
            vest_palette: "BLUEVEST".into(),
            pants_palette: "TANPANTS".into(),
        }
    }

    fn handgun(id: u16) -> Item {
        Item {
            id,
            name: format!("Handgun {id}"),
            item_class: IC_GUN,
            class_index: 0,
            two_handed: false,
            rocket_launcher: false,
            grenade_launcher: false,
            shots_per_burst: 0,
            camo_bonus: 0,
            urban_camo_bonus: 0,
            desert_camo_bonus: 0,
            snow_camo_bonus: 0,
            stealth_bonus: 0,
            camouflage_kit: false,
        }
    }

    #[test]
    fn standing_glock_uses_pistol_breath() {
        let soldier = SoldierState {
            character: character(),
            inventory: HashMap::from([("HANDPOS".into(), 1)]),
            attachments: HashMap::new(),
            scenario: Default::default(),
        };
        let items = HashMap::from([(1, handgun(1))]);
        let resolved = resolve_by_state("STANDING", &soldier, &items).unwrap();
        assert_eq!(resolved.surface, "RGMPISTOLBREATH");
        assert_eq!(
            physical_surface_file(&resolved.surface),
            Some("ANIMS\\S_MERC\\S_P_BRTH.STI")
        );
    }

    #[test]
    fn firearm_actions_follow_dual_handgun_loadout() {
        let mut soldier = SoldierState {
            character: character(),
            inventory: HashMap::from([("HANDPOS".into(), 1), ("SECONDHANDPOS".into(), 2)]),
            attachments: HashMap::new(),
            scenario: Default::default(),
        };
        let items = HashMap::from([(1, handgun(1)), (2, handgun(2))]);

        assert_eq!(weapon_mode_label(&soldier, &items), "Dual handguns");
        assert!(resolve_by_state("READY_RIFLE_STAND", &soldier, &items).is_none());
        let dual = resolve_by_state("READY_DUAL_STAND", &soldier, &items).unwrap();
        assert_eq!(dual.surface, "RGMSTANDDWALAIM");
        assert_eq!(dual.label, "Ready weapon · standing");

        soldier.inventory.insert("SECONDHANDPOS".into(), 0);
        assert_eq!(weapon_mode_label(&soldier, &items), "Single handgun");
        assert!(resolve_by_state("READY_DUAL_STAND", &soldier, &items).is_none());
        let single = resolve_by_state("READY_RIFLE_STAND", &soldier, &items).unwrap();
        assert_eq!(single.surface, "RGMHANDGUN_S_SHOT");
        assert_eq!(single.label, dual.label);
    }

    #[test]
    fn engine_scenario_substitutions_stay_behind_the_action() {
        let mut soldier = SoldierState {
            character: character(),
            inventory: HashMap::from([("HANDPOS".into(), 1)]),
            attachments: HashMap::new(),
            scenario: Default::default(),
        };
        let items = HashMap::from([(1, handgun(1))]);

        soldier.scenario.in_water = true;
        let water = resolve_by_state("STANDING", &soldier, &items).unwrap();
        assert_eq!(water.surface, "RGMWATER_N_STD");
        assert!(water.variant.contains("mid-water"));

        soldier.scenario.in_water = false;
        soldier.scenario.injured = true;
        let injured = resolve_by_state("WALKING", &soldier, &items).unwrap();
        assert_eq!(injured.surface, "RGMHURTWALKINGN");

        soldier.inventory.insert("SECONDHANDPOS".into(), 2);
        soldier.scenario.second_hand_loaded = false;
        let items = HashMap::from([(1, handgun(1)), (2, handgun(2))]);
        assert_eq!(weapon_mode_label(&soldier, &items), "Single handgun");
    }
}
