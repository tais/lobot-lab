use std::collections::{HashMap, HashSet};

use crate::model::{
    ArmourStats, CompareOp, Criterion, FilterDef, Item, Operation, SoldierState, WeaponStats,
};

pub struct FilterContext<'a> {
    pub filters: &'a HashMap<String, FilterDef>,
    pub items: &'a HashMap<u16, Item>,
    pub weapons: &'a HashMap<u16, WeaponStats>,
    pub armours: &'a HashMap<usize, ArmourStats>,
}

pub fn matches(
    filter_name: Option<&str>,
    soldier: &SoldierState,
    context: &FilterContext<'_>,
) -> Result<bool, String> {
    let Some(name) = filter_name.filter(|name| !name.is_empty()) else {
        return Ok(true);
    };
    let mut stack = HashSet::new();
    match_named(name, soldier, context, &mut stack)
}

fn match_named(
    name: &str,
    soldier: &SoldierState,
    context: &FilterContext<'_>,
    stack: &mut HashSet<String>,
) -> Result<bool, String> {
    let filter = context
        .filters
        .get(name)
        .ok_or_else(|| format!("Unknown filter: {name}"))?;
    if !stack.insert(name.to_string()) {
        return Err(format!("Recursive filter reference: {name}"));
    }

    let mut has_or = false;
    let mut or_result = false;
    for criterion in &filter.criteria {
        let result = match_criterion(criterion, soldier, context, stack)?;
        match criterion.operation {
            Operation::And if !result => {
                stack.remove(name);
                return Ok(false);
            }
            Operation::And => {}
            Operation::Or => {
                has_or = true;
                or_result |= result;
            }
        }
    }
    stack.remove(name);
    Ok(!has_or || or_result)
}

fn match_criterion(
    criterion: &Criterion,
    soldier: &SoldierState,
    context: &FilterContext<'_>,
    stack: &mut HashSet<String>,
) -> Result<bool, String> {
    // Filter::Match returns false for the entire filter before applying the
    // comparison/negation when FACEINDEX has no valid profile.
    if criterion.field == "FACEINDEX" && (soldier.character.id < 1 || soldier.character.id >= 254) {
        return Ok(false);
    }
    if criterion.field == "FILTER" {
        let referenced = criterion
            .values
            .first()
            .ok_or_else(|| "Empty FILTER criterion".to_string())?;
        let result = match_named(referenced, soldier, context, stack)?;
        return Ok(criterion.negate ^ result);
    }

    if matches!(
        criterion.field.as_str(),
        "NAME" | "PROFILENAME" | "NICKNAME"
    ) {
        let actual = match criterion.field.as_str() {
            "NICKNAME" => &soldier.character.nickname,
            _ => &soldier.character.name,
        };
        let result = match criterion.compare {
            CompareOp::Eq => criterion
                .values
                .first()
                .is_some_and(|value| actual == value),
            CompareOp::In => criterion.values.iter().any(|value| actual == value),
            CompareOp::Between | CompareOp::Greater | CompareOp::Less => false,
        };
        return Ok(criterion.negate ^ result);
    }

    let actual = field_value(&criterion.field, soldier, context)?;
    let values: Result<Vec<i32>, String> = criterion
        .values
        .iter()
        .map(|value| parse_value(&criterion.field, value))
        .collect();
    let values = values?;
    let result = match criterion.compare {
        CompareOp::Eq => values.first().is_some_and(|value| actual == *value),
        CompareOp::In => values.contains(&actual),
        CompareOp::Between => {
            values.len() == 2
                && actual >= values[0].min(values[1])
                && actual <= values[0].max(values[1])
        }
        // These intentionally mirror Filter::Match in JA2 1.13. Its sign comparison means
        // XML `gt` matches values below the operand and `lt` values above it.
        CompareOp::Greater => values.first().is_some_and(|value| actual < *value),
        CompareOp::Less => values.first().is_some_and(|value| actual > *value),
    };
    Ok(criterion.negate ^ result)
}

fn field_value(
    field: &str,
    soldier: &SoldierState,
    context: &FilterContext<'_>,
) -> Result<i32, String> {
    let inventory_value = |slot: &str| soldier.inventory.get(slot).copied().unwrap_or(0) as i32;
    let hand = soldier.inventory.get("HANDPOS").copied().unwrap_or(0);
    let left = soldier.inventory.get("SECONDHANDPOS").copied().unwrap_or(0);
    let item = |id| context.items.get(&id);
    let armour = |slot: &str| {
        soldier
            .inventory
            .get(slot)
            .and_then(|id| context.items.get(id))
            .and_then(|item| context.armours.get(&item.class_index))
    };

    let value = match field {
        "HELMETPOS" | "VESTPOS" | "LEGPOS" | "HEAD1POS" | "HEAD2POS" | "HANDPOS"
        | "SECONDHANDPOS" | "VESTPOCKPOS" | "LTHIGHPOCKPOS" | "RTHIGHPOCKPOS" | "CPACKPOCKPOS"
        | "BPACKPOCKPOS" | "GUNSLINGPOCKPOS" | "KNIFEPOCKPOS" => inventory_value(field),
        "HELMETPOSATTACHMENT0" => attachment_value(soldier, "HELMETPOS", 0),
        "HELMETPOSATTACHMENT1" => attachment_value(soldier, "HELMETPOS", 1),
        "HELMETPOSATTACHMENT2" => attachment_value(soldier, "HELMETPOS", 2),
        "HELMETPOSATTACHMENT3" => attachment_value(soldier, "HELMETPOS", 3),
        "LEGPOSATTACHMENT0" => attachment_value(soldier, "LEGPOS", 0),
        "LEGPOSATTACHMENT1" => attachment_value(soldier, "LEGPOS", 1),
        "LEGPOSATTACHMENT2" => attachment_value(soldier, "LEGPOS", 2),
        "LEGPOSATTACHMENT3" => attachment_value(soldier, "LEGPOS", 3),
        "VESTPOSATTACHMENT0" => attachment_value(soldier, "VESTPOS", 0),
        "VESTPOSATTACHMENT1" => attachment_value(soldier, "VESTPOS", 1),
        "VESTPOSATTACHMENT2" => attachment_value(soldier, "VESTPOS", 2),
        "VESTPOSATTACHMENT3" => attachment_value(soldier, "VESTPOS", 3),
        "SEX" => soldier.character.sex,
        "MERC_TYPE" => merc_type(soldier.character.profile_type),
        "SOLDIER_CLASS" => soldier.scenario.soldier_class,
        "CIVILIANGROUP" => soldier.scenario.civilian_group,
        "TEAM" => soldier.scenario.team,
        "CAMO" => soldier.scenario.camo,
        "URBANCAMO" => soldier.scenario.urban_camo,
        "DESERTCAMO" => soldier.scenario.desert_camo,
        "SNOWCAMO" => soldier.scenario.snow_camo,
        "BODYTYPE" => soldier.character.body_type,
        "EXPLEVEL" => soldier.character.exp_level,
        "STRENGTH" => soldier.character.strength,
        "LEADERSHIP" => soldier.character.leadership,
        "WISDOM" => soldier.character.wisdom,
        "SKILLTRAIT1" | "SKILLTRAIT2" => 0,
        "FACEINDEX" => soldier.character.face_index,
        "WEAPON_IN_HAND" => item(hand)
            .is_some_and(|item| item.item_class & (0x02 | 0x04 | 0x08 | 0x10 | 0x20 | 0x40) != 0)
            as i32,
        "WEAPON_CLASS" => context
            .weapons
            .get(&hand)
            .map(|weapon| weapon.class)
            .unwrap_or(0),
        "LEFT_WEAPON_CLASS" => context
            .weapons
            .get(&left)
            .map(|weapon| weapon.class)
            .unwrap_or(0),
        "WEAPON_TYPE" => context
            .weapons
            .get(&hand)
            .map(|weapon| weapon.kind)
            .unwrap_or(0),
        "LEFT_WEAPON_TYPE" => context
            .weapons
            .get(&left)
            .map(|weapon| weapon.kind)
            .unwrap_or(0),
        "CALIBRE" => context
            .weapons
            .get(&hand)
            .map(|weapon| weapon.calibre)
            .unwrap_or(0),
        "WEAPON_TWOHANDED" => item(hand).is_some_and(|item| item.two_handed) as i32,
        "LEFT_WEAPON_TWOHANDED" => item(left).is_some_and(|item| item.two_handed) as i32,
        "VEST_AMOR_PROTECTION" => armour("VESTPOS").map(|value| value.protection).unwrap_or(0),
        "VEST_AMOR_COVERAGE" => armour("VESTPOS").map(|value| value.coverage).unwrap_or(0),
        "HELMET_AMOR_PROTECTION" => armour("HELMETPOS")
            .map(|value| value.protection)
            .unwrap_or(0),
        "HELMET_AMOR_COVERAGE" => armour("HELMETPOS").map(|value| value.coverage).unwrap_or(0),
        "WEARING_BACKPACK" => (inventory_value("BPACKPOCKPOS") != 0) as i32,
        _ => return Err(format!("Unsupported filter criterion: {field}")),
    };
    Ok(value)
}

fn attachment_value(soldier: &SoldierState, slot: &str, index: usize) -> i32 {
    soldier
        .attachments
        .get(slot)
        .and_then(|items| items.get(index))
        .copied()
        .unwrap_or(0) as i32
}

fn merc_type(profile_type: i32) -> i32 {
    match profile_type {
        1 => 1,     // AIM
        2 => 2,     // MERC
        3 | 4 => 3, // RPC/NPC
        5 => 6,     // vehicle
        6 => 0,     // IMP
        _ => 3,
    }
}

fn parse_value(field: &str, value: &str) -> Result<i32, String> {
    if let Ok(number) = value.trim().parse::<i32>() {
        return Ok(number);
    }
    let name = value.trim().to_ascii_uppercase();
    let names: &[&str] = match field {
        "BODYTYPE" => &[
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
        ],
        "WEAPON_CLASS" | "LEFT_WEAPON_CLASS" => &[
            "NOGUNCLASS",
            "HANDGUNCLASS",
            "SMGCLASS",
            "RIFLECLASS",
            "MGCLASS",
            "SHOTGUNCLASS",
            "MONSTERCLASS",
            "KNIFECLASS",
        ],
        "WEAPON_TYPE" | "LEFT_WEAPON_TYPE" => &[
            "NOT_GUN",
            "GUN_PISTOL",
            "GUN_M_PISTOL",
            "GUN_SMG",
            "GUN_RIFLE",
            "GUN_SN_RIFLE",
            "GUN_AS_RIFLE",
            "GUN_LMG",
            "GUN_SHOTGUN",
        ],
        "SEX" => &["MALE", "FEMALE"],
        "MERC_TYPE" => &[
            "MERC_TYPE__PLAYER_CHARACTER",
            "MERC_TYPE__AIM_MERC",
            "MERC_TYPE__MERC",
            "MERC_TYPE__NPC",
            "MERC_TYPE__EPC",
            "MERC_TYPE__NPC_WITH_UNEXTENDABLE_CONTRACT",
            "MERC_TYPE__VEHICLE",
        ],
        "SOLDIER_CLASS" => &[
            "SOLDIER_CLASS_NONE",
            "SOLDIER_CLASS_ADMINISTRATOR",
            "SOLDIER_CLASS_ELITE",
            "SOLDIER_CLASS_ARMY",
            "SOLDIER_CLASS_GREEN_MILITIA",
            "SOLDIER_CLASS_REG_MILITIA",
            "SOLDIER_CLASS_ELITE_MILITIA",
            "SOLDIER_CLASS_CREATURE",
            "SOLDIER_CLASS_MINER",
            "SOLDIER_CLASS_ZOMBIE",
            "SOLDIER_CLASS_TANK",
            "SOLDIER_CLASS_JEEP",
            "SOLDIER_CLASS_BANDIT",
            "SOLDIER_CLASS_ROBOT",
        ],
        "CIVILIANGROUP" => &[
            "NON_CIV_GROUP",
            "REBEL_CIV_GROUP",
            "KINGPIN_CIV_GROUP",
            "SANMONA_ARMS_GROUP",
            "ANGELS_GROUP",
            "BEGGARS_CIV_GROUP",
            "TOURISTS_CIV_GROUP",
            "ALMA_MILITARY_CIV_GROUP",
            "DOCTORS_CIV_GROUP",
            "COUPLE1_CIV_GROUP",
            "HICKS_CIV_GROUP",
            "WARDEN_CIV_GROUP",
            "JUNKYARD_CIV_GROUP",
            "FACTORY_KIDS_GROUP",
            "QUEENS_CIV_GROUP",
            "UNNAMED_CIV_GROUP_15",
            "UNNAMED_CIV_GROUP_16",
            "UNNAMED_CIV_GROUP_17",
            "UNNAMED_CIV_GROUP_18",
            "UNNAMED_CIV_GROUP_19",
            "ASSASSIN_CIV_GROUP",
            "POW_PRISON_CIV_GROUP",
            "UNNAMED_CIV_GROUP_22",
            "UNNAMED_CIV_GROUP_23",
            "VOLUNTEER_CIV_GROUP",
            "BOUNTYHUNTER_CIV_GROUP",
            "DOWNEDPILOT_CIV_GROUP",
            "SCIENTIST_GROUP",
        ],
        _ => &[],
    };
    names
        .iter()
        .position(|candidate| *candidate == name)
        .map(|index| index as i32)
        .ok_or_else(|| format!("Unknown {field} value: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Character, CompareOp, Criterion, FilterDef, Operation};

    #[test]
    fn filter_requires_all_and_one_or() {
        let character = Character {
            id: 1,
            name: "Fox".into(),
            nickname: "Fox".into(),
            profile_type: 1,
            body_type: 3,
            face_index: 14,
            sex: 1,
            exp_level: 4,
            strength: 55,
            leadership: 40,
            wisdom: 70,
            hair_palette: "BLACKHEAD".into(),
            skin_palette: "PINKSKIN".into(),
            vest_palette: "BLUEVEST".into(),
            pants_palette: "TANPANTS".into(),
        };
        let soldier = SoldierState {
            character,
            inventory: HashMap::new(),
            attachments: HashMap::new(),
            scenario: Default::default(),
        };
        let filter = FilterDef {
            criteria: vec![
                Criterion {
                    field: "BODYTYPE".into(),
                    operation: Operation::And,
                    compare: CompareOp::Eq,
                    negate: false,
                    values: vec!["REGFEMALE".into()],
                },
                Criterion {
                    field: "FACEINDEX".into(),
                    operation: Operation::Or,
                    compare: CompareOp::Eq,
                    negate: false,
                    values: vec!["14".into()],
                },
            ],
        };
        let filters = HashMap::from([("test".into(), filter)]);
        let items = HashMap::new();
        let weapons = HashMap::new();
        let armours = HashMap::new();
        let context = FilterContext {
            filters: &filters,
            items: &items,
            weapons: &weapons,
            armours: &armours,
        };
        assert!(matches(Some("test"), &soldier, &context).unwrap());
    }
}
