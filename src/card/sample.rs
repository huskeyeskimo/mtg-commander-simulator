//! Sample card definitions for testing — classic MTG cards.

use crate::card::*;
use crate::card::effects::{Condition, PredefinedToken};
use crate::game::CardDatabase;
use crate::layers::{AffectedObjects, StaticAbility};
use crate::mana::{Color, ManaCost};

/// Card IDs for sample cards.
pub mod ids {
    pub use crate::card::catalog::ids::*;
}

pub fn build_sample_db() -> CardDatabase {
    let mut db = CardDatabase::new();

    // === Basic Lands ===
    db.insert(CardDef {
        id: ids::MOUNTAIN,
        name: "Mountain".into(),
        mana_cost: None,
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Basic],
        subtypes: vec![Subtype("Mountain".into())],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![ManaAbility::TapForColor(Color::Red)],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Add {R}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FOREST,
        name: "Forest".into(),
        mana_cost: None,
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Basic],
        subtypes: vec![Subtype("Forest".into())],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Add {G}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::PLAINS,
        name: "Plains".into(),
        mana_cost: None,
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Basic],
        subtypes: vec![Subtype("Plains".into())],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![ManaAbility::TapForColor(Color::White)],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Add {W}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::ISLAND,
        name: "Island".into(),
        mana_cost: None,
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Basic],
        subtypes: vec![Subtype("Island".into())],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![ManaAbility::TapForColor(Color::Blue)],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Add {U}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SWAMP,
        name: "Swamp".into(),
        mana_cost: None,
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Basic],
        subtypes: vec![Subtype("Swamp".into())],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![ManaAbility::TapForColor(Color::Black)],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Add {B}.".into(),
        ..Default::default()
    });

    // === Red Creatures ===
    db.insert(CardDef {
        id: ids::GREY_OGRE,
        name: "Gray Ogre".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Ogre".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::GOBLIN_GUIDE,
        name: "Goblin Guide".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Goblin".into()), Subtype("Scout".into())],
        keywords: vec![KeywordAbility::Haste],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![], // simplified — no "reveal top" trigger
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Haste".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MONASTERY_SWIFTSPEAR,
        name: "Monastery Swiftspear".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Monk".into())],
        keywords: vec![KeywordAbility::Haste],
        power: Some(1),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![], // simplified — no prowess
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Haste, Prowess".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SHIVAN_DRAGON,
        name: "Shivan Dragon".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Dragon".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(5),
        toughness: Some(5),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(0, 0, 0, 0, 1, 0),
            requires_tap: false,
            sacrifice_cost: None, life_cost: 0,
            effect: Effect::Buff {
                power: 1,
                toughness: 0,
                until_eot: true,
            },
            description: "{R}: Shivan Dragon gets +1/+0 until end of turn.".into(),
        }],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flying. {R}: Shivan Dragon gets +1/+0 until end of turn.".into(),
        ..Default::default()
    });

    // === Green Creatures ===
    db.insert(CardDef {
        id: ids::GRIZZLY_BEARS,
        name: "Grizzly Bears".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Bear".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::LLANOWAR_ELVES,
        name: "Llanowar Elves".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Elf".into()), Subtype("Druid".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Add {G}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::ELVISH_MYSTIC,
        name: "Elvish Mystic".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Elf".into()), Subtype("Druid".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Add {G}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::KALONIAN_TUSKER,
        name: "Kalonian Tusker".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Beast".into())],
        keywords: vec![],
        power: Some(3),
        toughness: Some(3),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::LEATHERBACK_BALOTH,
        name: "Leatherback Baloth".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 3)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Beast".into())],
        keywords: vec![],
        power: Some(4),
        toughness: Some(5),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "".into(),
        ..Default::default()
    });

    // === White Creatures ===
    db.insert(CardDef {
        id: ids::SAVANNAH_LIONS,
        name: "Savannah Lions".into(),
        mana_cost: Some(ManaCost::new(0, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Cat".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SERRA_ANGEL,
        name: "Serra Angel".into(),
        mana_cost: Some(ManaCost::new(3, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Angel".into())],
        keywords: vec![KeywordAbility::Flying, KeywordAbility::Vigilance],
        power: Some(4),
        toughness: Some(4),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flying, Vigilance".into(),
        ..Default::default()
    });

    // === Instants / Sorceries ===
    db.insert(CardDef {
        id: ids::LIGHTNING_BOLT,
        name: "Lightning Bolt".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DealDamage {
            amount: 3,
            target: TargetSpec::CreatureOrPlayer,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Lightning Bolt deals 3 damage to any target.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SHOCK,
        name: "Shock".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DealDamage {
            amount: 2,
            target: TargetSpec::CreatureOrPlayer,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Shock deals 2 damage to any target.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::LAVA_SPIKE,
        name: "Lava Spike".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Sorcery],
        supertypes: vec![],
        subtypes: vec![Subtype("Arcane".into())],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DealDamage {
            amount: 3,
            target: TargetSpec::AnyPlayer,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Lava Spike deals 3 damage to target player or planeswalker.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::RIFT_BOLT,
        name: "Rift Bolt".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Sorcery],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DealDamage {
            amount: 3,
            target: TargetSpec::CreatureOrPlayer,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Rift Bolt deals 3 damage to any target.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::GIANT_GROWTH,
        name: "Giant Growth".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::Buff {
            power: 3,
            toughness: 3,
            until_eot: true,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Target creature gets +3/+3 until end of turn.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SWORDS_TO_PLOWSHARES,
        name: "Swords to Plowshares".into(),
        mana_cost: Some(ManaCost::new(0, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DestroyTarget {
            target: TargetSpec::AnyCreature,
        }),
        // Simplified — real StP exiles and gains life
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Exile target creature. Its controller gains life equal to its power.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::COUNTERSPELL,
        name: "Counterspell".into(),
        mana_cost: Some(ManaCost::new(0, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::Counter {
            target: TargetSpec::AnySpell,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Counter target spell.".into(),
        ..Default::default()
    });

    // === Cards with Triggered Abilities ===

    // Elvish Visionary: 1G 1/1 Elf Shaman — ETB draw a card
    db.insert(CardDef {
        id: ids::ELVISH_VISIONARY,
        name: "Elvish Visionary".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Elf".into()), Subtype("Shaman".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::DrawCards { count: 1 },
            description: "When Elvish Visionary enters the battlefield, draw a card.".into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "When Elvish Visionary enters the battlefield, draw a card.".into(),
        ..Default::default()
    });

    // Blade Splicer: 2W 1/1 Human Artificer — ETB create a 3/3 Golem with first strike
    db.insert(CardDef {
        id: ids::BLADE_SPLICER,
        name: "Blade Splicer".into(),
        mana_cost: Some(ManaCost::new(2, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Artificer".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::CreateToken(TokenDef {
                name: "Phyrexian Golem".into(),
                power: 3,
                toughness: 3,
                colors: vec![],
                subtypes: vec![Subtype("Phyrexian".into()), Subtype("Golem".into())],
                keywords: vec![KeywordAbility::FirstStrike],
            }),
            description: "When Blade Splicer enters the battlefield, create a 3/3 colorless Phyrexian Golem artifact creature token with first strike.".into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "When Blade Splicer enters the battlefield, create a 3/3 colorless Phyrexian Golem artifact creature token with first strike.".into(),
        ..Default::default()
    });

    // Siege-Gang Commander: 3RR 2/2 Goblin — ETB create three 1/1 Goblin tokens
    db.insert(CardDef {
        id: ids::SIEGE_GANG_COMMANDER,
        name: "Siege-Gang Commander".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Goblin".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::Multiple(vec![
                Effect::CreateToken(TokenDef {
                    name: "Goblin".into(),
                    power: 1,
                    toughness: 1,
                    colors: vec![Color::Red],
                    subtypes: vec![Subtype("Goblin".into())],
                    keywords: vec![],
                }),
                Effect::CreateToken(TokenDef {
                    name: "Goblin".into(),
                    power: 1,
                    toughness: 1,
                    colors: vec![Color::Red],
                    subtypes: vec![Subtype("Goblin".into())],
                    keywords: vec![],
                }),
                Effect::CreateToken(TokenDef {
                    name: "Goblin".into(),
                    power: 1,
                    toughness: 1,
                    colors: vec![Color::Red],
                    subtypes: vec![Subtype("Goblin".into())],
                    keywords: vec![],
                }),
            ]),
            description: "When Siege-Gang Commander enters the battlefield, create three 1/1 red Goblin creature tokens.".into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "When this creature enters, create three 1/1 red Goblin creature tokens.\n{1}{R}, Sacrifice a Goblin: This creature deals 2 damage to any target.".into(),
        ..Default::default()
    });

    // =====================================================================
    // Phase 1A Test Cards
    // =====================================================================

    // Fiery Conclusion Elemental: 2R 2/2 Elemental
    // "When ~ dies, deal 2 damage to each player."
    // Used to test the SBA recurrence loop (CR 704.3 acceptance criteria).
    db.insert(CardDef {
        id: ids::FIERY_CONCLUSION_ELEMENTAL,
        name: "Fiery Conclusion Elemental".into(),
        mana_cost: Some(ManaCost::new(2, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Elemental".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::Dies,
            effect: Effect::DealDamage {
                amount: 2,
                target: TargetSpec::NoTarget,
            },
            description: "When Fiery Conclusion Elemental dies, it deals 2 damage to each player."
                .into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "When Fiery Conclusion Elemental dies, it deals 2 damage to each player."
            .into(),
        ..Default::default()
    });

    // Pyroclasm Elemental: 2R 3/1 Elemental
    // "When ~ dies, deal 2 damage to each creature."
    // Used to test cascading SBA — its dies trigger can kill other creatures,
    // causing recursive SBAs per CR 704.3.
    db.insert(CardDef {
        id: ids::PYROCLASM_ELEMENTAL,
        name: "Pyroclasm Elemental".into(),
        mana_cost: Some(ManaCost::new(2, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Elemental".into())],
        keywords: vec![],
        power: Some(3),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::Dies,
            effect: Effect::DealDamage {
                amount: 2,
                target: TargetSpec::EachCreature,
            },
            description: "When Pyroclasm Elemental dies, it deals 2 damage to each creature."
                .into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "When Pyroclasm Elemental dies, it deals 2 damage to each creature.".into(),
        ..Default::default()
    });

    // =====================================================================
    // Phase 2A: Expanded Card Pool
    // =====================================================================

    // --- White creatures ---

    // Mother of Runes: W 1/1 Human Cleric — protection granting (simplified)
    db.insert(CardDef {
        id: ids::MOTHER_OF_RUNES,
        name: "Mother of Runes".into(),
        mana_cost: Some(ManaCost::new(0, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Cleric".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Target creature you control gains protection from the color of your choice until end of turn.".into(),
        ..Default::default()
    });

    // Elite Vanguard: W 2/1 Human Soldier
    db.insert(CardDef {
        id: ids::ELITE_VANGUARD,
        name: "Elite Vanguard".into(),
        mana_cost: Some(ManaCost::new(0, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Soldier".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "".into(),
        ..Default::default()
    });

    // White Knight: WW 2/2 Human Knight — First strike, protection from black (simplified)
    db.insert(CardDef {
        id: ids::WHITE_KNIGHT,
        name: "White Knight".into(),
        mana_cost: Some(ManaCost::new(0, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Knight".into())],
        keywords: vec![KeywordAbility::FirstStrike],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "First strike, protection from black".into(),
        ..Default::default()
    });

    // Leonin Skyhunter: WW 2/2 Cat Knight — Flying
    db.insert(CardDef {
        id: ids::LEONIN_SKYHUNTER,
        name: "Leonin Skyhunter".into(),
        mana_cost: Some(ManaCost::new(0, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Cat".into()), Subtype("Knight".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flying".into(),
        ..Default::default()
    });

    // Baneslayer Angel: 3WW 5/5 Angel — Flying, first strike, lifelink
    db.insert(CardDef {
        id: ids::BANESLAYER_ANGEL,
        name: "Baneslayer Angel".into(),
        mana_cost: Some(ManaCost::new(3, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Angel".into())],
        keywords: vec![
            KeywordAbility::Flying,
            KeywordAbility::FirstStrike,
            KeywordAbility::Lifelink,
        ],
        power: Some(5),
        toughness: Some(5),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flying, first strike, lifelink".into(),
        ..Default::default()
    });

    // Thalia, Guardian of Thraben: 1W 2/1 Human Soldier (simplified — no taxing)
    db.insert(CardDef {
        id: ids::THALIA_GUARDIAN,
        name: "Thalia, Guardian of Thraben".into(),
        mana_cost: Some(ManaCost::new(1, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Human".into()), Subtype("Soldier".into())],
        keywords: vec![KeywordAbility::FirstStrike],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "First strike. Noncreature spells cost {1} more to cast.".into(),
        ..Default::default()
    });

    // Brimaz, King of Oreskos: 1WW 3/4 Cat Soldier — Vigilance
    db.insert(CardDef {
        id: ids::BRIMAZ_KING,
        name: "Brimaz, King of Oreskos".into(),
        mana_cost: Some(ManaCost::new(1, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Cat".into()), Subtype("Soldier".into())],
        keywords: vec![KeywordAbility::Vigilance],
        power: Some(3),
        toughness: Some(4),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Vigilance".into(),
        ..Default::default()
    });

    // Soldier of the Pantheon: W 2/1 Human Soldier
    db.insert(CardDef {
        id: ids::SOLDIER_OF_THE_PANTHEON,
        name: "Soldier of the Pantheon".into(),
        mana_cost: Some(ManaCost::new(0, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Soldier".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Protection from multicolored.".into(),
        ..Default::default()
    });

    // Hero of Bladehold: 2WW 3/4 Human Knight — Battle cry (simplified as vanilla)
    db.insert(CardDef {
        id: ids::HERO_OF_BLADEHOLD,
        name: "Hero of Bladehold".into(),
        mana_cost: Some(ManaCost::new(2, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Knight".into())],
        keywords: vec![],
        power: Some(3),
        toughness: Some(4),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Battle cry. Whenever Hero of Bladehold attacks, create two 1/1 white Soldier creature tokens that are tapped and attacking.".into(),
        ..Default::default()
    });

    // Precinct Captain: WW 2/2 Human Soldier — First strike
    db.insert(CardDef {
        id: ids::PRECINCT_CAPTAIN,
        name: "Precinct Captain".into(),
        mana_cost: Some(ManaCost::new(0, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Soldier".into())],
        keywords: vec![KeywordAbility::FirstStrike],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "First strike. Whenever Precinct Captain deals combat damage to a player, create a 1/1 white Soldier creature token.".into(),
        ..Default::default()
    });

    // --- White spells ---

    // Path to Exile: W — Exile target creature
    db.insert(CardDef {
        id: ids::PATH_TO_EXILE,
        name: "Path to Exile".into(),
        mana_cost: Some(ManaCost::new(0, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::ExileTarget {
            target: TargetSpec::AnyCreature,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Exile target creature. Its controller may search their library for a basic land card, put that card onto the battlefield tapped, then shuffle.".into(),
        ..Default::default()
    });

    // Wrath of God: 2WW — Destroy all creatures
    db.insert(CardDef {
        id: ids::WRATH_OF_GOD,
        name: "Wrath of God".into(),
        mana_cost: Some(ManaCost::new(2, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Sorcery],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DestroyAll),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Destroy all creatures. They can't be regenerated.".into(),
        ..Default::default()
    });

    // Day of Judgment: 2WW — Destroy all creatures
    db.insert(CardDef {
        id: ids::DAY_OF_JUDGMENT,
        name: "Day of Judgment".into(),
        mana_cost: Some(ManaCost::new(2, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Sorcery],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DestroyAll),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Destroy all creatures.".into(),
        ..Default::default()
    });

    // Oblivion Ring: 2W — Exile target nonland permanent (simplified as destroy)
    db.insert(CardDef {
        id: ids::OBLIVION_RING,
        name: "Oblivion Ring".into(),
        mana_cost: Some(ManaCost::new(2, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Enchantment],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::ExileTarget {
                target: TargetSpec::AnyNonlandPermanent,
            },
            description:
                "When Oblivion Ring enters the battlefield, exile another target nonland permanent."
                    .into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text:
            "When Oblivion Ring enters the battlefield, exile another target nonland permanent."
                .into(),
        ..Default::default()
    });

    // Disenchant: 1W — Destroy target artifact or enchantment (simplified as destroy nonland permanent)
    db.insert(CardDef {
        id: ids::DISENCHANT,
        name: "Disenchant".into(),
        mana_cost: Some(ManaCost::new(1, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DestroyTarget {
            target: TargetSpec::AnyNonlandPermanent,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Destroy target artifact or enchantment.".into(),
        ..Default::default()
    });

    // --- White enchantments (layered effects) ---

    // Glorious Anthem: 1WW — Creatures you control get +1/+1
    db.insert(CardDef {
        id: ids::GLORIOUS_ANTHEM,
        name: "Glorious Anthem".into(),
        mana_cost: Some(ManaCost::new(1, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Enchantment],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![StaticAbility::Anthem {
            power: 1,
            toughness: 1,
            affected: AffectedObjects::OtherCreaturesControlledBy(0), // placeholder — refreshed at runtime
        }],
        enters_tapped: false,
        oracle_text: "Creatures you control get +1/+1.".into(),
        ..Default::default()
    });

    // Honor of the Pure: 1W — White creatures you control get +1/+1 (simplified as creatures you control)
    db.insert(CardDef {
        id: ids::HONOR_OF_THE_PURE,
        name: "Honor of the Pure".into(),
        mana_cost: Some(ManaCost::new(1, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Enchantment],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![StaticAbility::Anthem {
            power: 1,
            toughness: 1,
            affected: AffectedObjects::CreaturesControlledBy(0),
        }],
        enters_tapped: false,
        oracle_text: "White creatures you control get +1/+1.".into(),
        ..Default::default()
    });

    // Crusade: WW — White creatures get +1/+1 (simplified as all creatures)
    db.insert(CardDef {
        id: ids::CRUSADE,
        name: "Crusade".into(),
        mana_cost: Some(ManaCost::new(0, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Enchantment],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![StaticAbility::Anthem {
            power: 1,
            toughness: 1,
            affected: AffectedObjects::AllCreatures,
        }],
        enters_tapped: false,
        oracle_text: "White creatures get +1/+1.".into(),
        ..Default::default()
    });

    // Humility: 2WW — All creatures lose all abilities and are 1/1
    db.insert(CardDef {
        id: ids::HUMILITY,
        name: "Humility".into(),
        mana_cost: Some(ManaCost::new(2, 2, 0, 0, 0, 0)),
        card_types: vec![CardType::Enchantment],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![
            StaticAbility::RemoveAllAbilities {
                affected: AffectedObjects::AllCreatures,
            },
            StaticAbility::SetPowerToughness {
                power: 1,
                toughness: 1,
                affected: AffectedObjects::AllCreatures,
            },
        ],
        enters_tapped: false,
        oracle_text: "All creatures lose all abilities and have base power and toughness 1/1."
            .into(),
        ..Default::default()
    });

    // --- Blue creatures ---

    // Delver of Secrets: U 1/1 Human Wizard (simplified — no transform)
    db.insert(CardDef {
        id: ids::DELVER_OF_SECRETS,
        name: "Delver of Secrets".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Wizard".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "At the beginning of your upkeep, look at the top card of your library. You may reveal that card. If an instant or sorcery card is revealed this way, transform Delver of Secrets.".into(),
        ..Default::default()
    });

    // Snapcaster Mage: 1U 2/1 Human Wizard — Flash
    db.insert(CardDef {
        id: ids::SNAPCASTER_MAGE,
        name: "Snapcaster Mage".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Wizard".into())],
        keywords: vec![KeywordAbility::Flash],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flash. When Snapcaster Mage enters the battlefield, target instant or sorcery card in your graveyard gains flashback until end of turn.".into(),
        ..Default::default()
    });

    // Vendilion Clique: 1UU 3/1 Faerie Wizard — Flash, flying
    db.insert(CardDef {
        id: ids::VENDILION_CLIQUE,
        name: "Vendilion Clique".into(),
        mana_cost: Some(ManaCost::new(1, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Faerie".into()), Subtype("Wizard".into())],
        keywords: vec![KeywordAbility::Flash, KeywordAbility::Flying],
        power: Some(3),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flash, flying. When Vendilion Clique enters the battlefield, look at target player's hand. You may choose a nonland card from it. If you do, that player reveals the chosen card, puts it on the bottom of their library, then draws a card.".into(),
        ..Default::default()
    });

    // Man-o'-War: 2U 2/2 Jellyfish — ETB bounce a creature
    db.insert(CardDef {
        id: ids::MAN_O_WAR,
        name: "Man-o'-War".into(),
        mana_cost: Some(ManaCost::new(2, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Jellyfish".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::BounceTo {
                zone: ZoneType::Hand,
                target: TargetSpec::AnyCreature,
            },
            description: "When Man-o'-War enters the battlefield, return target creature to its owner's hand.".into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "When Man-o'-War enters the battlefield, return target creature to its owner's hand.".into(),
        ..Default::default()
    });

    // Serendib Efreet: 2U 3/4 Efreet — Flying
    db.insert(CardDef {
        id: ids::SERENDIB_EFREET,
        name: "Serendib Efreet".into(),
        mana_cost: Some(ManaCost::new(2, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Efreet".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(3),
        toughness: Some(4),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flying. At the beginning of your upkeep, Serendib Efreet deals 1 damage to you.".into(),
        ..Default::default()
    });

    // Phantasmal Bear: U 2/2 Bear Illusion
    db.insert(CardDef {
        id: ids::PHANTASMAL_BEAR,
        name: "Phantasmal Bear".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Bear".into()), Subtype("Illusion".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "When Phantasmal Bear becomes the target of a spell or ability, sacrifice it."
            .into(),
        ..Default::default()
    });

    // --- Blue spells ---

    // Mana Leak: 1U — Counter target spell unless its controller pays {3}
    db.insert(CardDef {
        id: ids::MANA_LEAK,
        name: "Mana Leak".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::Counter {
            target: TargetSpec::AnySpell,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Counter target spell unless its controller pays {3}.".into(),
        ..Default::default()
    });

    // Remand: 1U — Counter target spell, draw a card (simplified as counter + draw)
    db.insert(CardDef {
        id: ids::REMAND,
        name: "Remand".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::Multiple(vec![
            Effect::Counter { target: TargetSpec::AnySpell },
            Effect::DrawCards { count: 1 },
        ])),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Counter target spell. If that spell is countered this way, put it into its owner's hand instead of into that player's graveyard. Draw a card.".into(),
        ..Default::default()
    });

    // Expedite: R — Target creature gains haste until end of turn. Draw a card.
    db.insert(CardDef {
        id: ids::EXPEDITE,
        name: "Expedite".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Multiple(vec![
            Effect::GainKeywordUntilEOT {
                keyword: KeywordAbility::Haste,
                target: TargetSpec::AnyCreature,
            },
            Effect::DrawCards { count: 1 },
        ])),
        oracle_text: "Target creature gains haste until end of turn. Draw a card.".into(),
        ..Default::default()
    });

    // Brainstorm: U — Draw 3, then put 2 cards from hand on top (partially simplified: draw 3)
    db.insert(CardDef {
        id: ids::BRAINSTORM,
        name: "Brainstorm".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DrawCards { count: 3 }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Draw three cards, then put two cards from your hand on top of your library in any order.".into(),
        ..Default::default()
    });

    // Ponder: U — Look at top 3, may shuffle, draw (simplified as draw 1)
    db.insert(CardDef {
        id: ids::PONDER,
        name: "Ponder".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Sorcery],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DrawCards { count: 1 }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Look at the top three cards of your library, then put them back in any order. You may shuffle. Draw a card.".into(),
        ..Default::default()
    });

    // Unsummon: U — Return target creature to its owner's hand
    db.insert(CardDef {
        id: ids::UNSUMMON,
        name: "Unsummon".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::BounceTo {
            zone: ZoneType::Hand,
            target: TargetSpec::AnyCreature,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Return target creature to its owner's hand.".into(),
        ..Default::default()
    });

    // --- Black creatures ---

    // Dark Confidant: 1B 2/1 Human Wizard
    db.insert(CardDef {
        id: ids::DARK_CONFIDANT,
        name: "Dark Confidant".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Wizard".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "At the beginning of your upkeep, reveal the top card of your library and put that card into your hand. You lose life equal to its mana value.".into(),
        ..Default::default()
    });

    // Hypnotic Specter: 1BB 2/2 Specter — Flying
    db.insert(CardDef {
        id: ids::HYPNOTIC_SPECTER,
        name: "Hypnotic Specter".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Specter".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flying. Whenever Hypnotic Specter deals damage to an opponent, that player discards a card at random.".into(),
        ..Default::default()
    });

    // Nantuko Shade: BB 2/1 Insect Shade
    db.insert(CardDef {
        id: ids::NANTUKO_SHADE,
        name: "Nantuko Shade".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Insect".into()), Subtype("Shade".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{B}: Nantuko Shade gets +1/+1 until end of turn.".into(),
        ..Default::default()
    });

    // Vampire Nighthawk: 1BB 2/3 Vampire Shaman — Flying, deathtouch, lifelink
    db.insert(CardDef {
        id: ids::VAMPIRE_NIGHTHAWK,
        name: "Vampire Nighthawk".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Vampire".into()), Subtype("Shaman".into())],
        keywords: vec![
            KeywordAbility::Flying,
            KeywordAbility::Deathtouch,
            KeywordAbility::Lifelink,
        ],
        power: Some(2),
        toughness: Some(3),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flying, deathtouch, lifelink".into(),
        ..Default::default()
    });

    // Gatekeeper of Malakir: BB 2/2 Vampire Warrior — Kicker B, ETB sacrifice if kicked (simplified)
    db.insert(CardDef {
        id: ids::GATEKEEPER_OF_MALAKIR,
        name: "Gatekeeper of Malakir".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Vampire".into()), Subtype("Warrior".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Kicker {B}. When Gatekeeper of Malakir enters the battlefield, if it was kicked, target player sacrifices a creature.".into(),
        ..Default::default()
    });

    // Bloodghast: BB 2/1 Vampire Spirit — can't block (simplified)
    db.insert(CardDef {
        id: ids::BLOODGHAST,
        name: "Bloodghast".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Vampire".into()), Subtype("Spirit".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Bloodghast can't block. Landfall — Whenever a land enters the battlefield under your control, you may return Bloodghast from your graveyard to the battlefield.".into(),
        ..Default::default()
    });

    // Geralf's Messenger: BBB 3/2 Zombie — ETB opponent loses 2 life
    db.insert(CardDef {
        id: ids::GERALF_MESSENGER,
        name: "Geralf's Messenger".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 3, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Zombie".into())],
        keywords: vec![],
        power: Some(3),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::LoseLife {
                amount: 2,
                target: TargetSpec::Opponent,
            },
            description: "When Geralf's Messenger enters the battlefield, target opponent loses 2 life.".into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: true,
        oracle_text: "Geralf's Messenger enters the battlefield tapped. When Geralf's Messenger enters the battlefield, target opponent loses 2 life. Undying.".into(),
        ..Default::default()
    });

    // Phyrexian Obliterator: BBBB 5/5 Phyrexian Horror — Trample
    db.insert(CardDef {
        id: ids::PHYREXIAN_OBLITERATOR,
        name: "Phyrexian Obliterator".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 4, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Phyrexian".into()), Subtype("Horror".into())],
        keywords: vec![KeywordAbility::Trample],
        power: Some(5),
        toughness: Some(5),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Trample. Whenever a source deals damage to Phyrexian Obliterator, that source's controller sacrifices that many permanents.".into(),
        ..Default::default()
    });

    // Knight of the Ebon Legion: B 1/2 Vampire Knight
    db.insert(CardDef {
        id: ids::KNIGHT_OF_THE_EBON_LEGION,
        name: "Knight of the Ebon Legion".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Vampire".into()), Subtype("Knight".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text:
            "{2}{B}: Knight of the Ebon Legion gets +3/+3 and gains deathtouch until end of turn."
                .into(),
        ..Default::default()
    });

    // --- Black spells ---

    // Doom Blade: 1B — Destroy target nonblack creature (simplified as destroy creature)
    db.insert(CardDef {
        id: ids::DOOM_BLADE,
        name: "Doom Blade".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DestroyTarget {
            target: TargetSpec::AnyCreature,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Destroy target nonblack creature.".into(),
        ..Default::default()
    });

    // Go for the Throat: 1B — Destroy target nonartifact creature (simplified as destroy creature)
    db.insert(CardDef {
        id: ids::GO_FOR_THE_THROAT,
        name: "Go for the Throat".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DestroyTarget {
            target: TargetSpec::AnyCreature,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Destroy target nonartifact creature.".into(),
        ..Default::default()
    });

    // Thoughtseize: B — Target player discards a card (simplified)
    db.insert(CardDef {
        id: ids::THOUGHTSEIZE,
        name: "Thoughtseize".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Sorcery],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::Multiple(vec![
            Effect::DiscardCards { count: 1, target: TargetSpec::Opponent },
            Effect::LoseLife { amount: 2, target: TargetSpec::Controller },
        ])),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Target player reveals their hand. You choose a nonland card from it. That player discards that card. You lose 2 life.".into(),
        ..Default::default()
    });

    // Hymn to Tourach: BB — Target player discards 2 cards at random
    db.insert(CardDef {
        id: ids::HYMN_TO_TOURACH,
        name: "Hymn to Tourach".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Sorcery],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DiscardCards {
            count: 2,
            target: TargetSpec::Opponent,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Target player discards two cards at random.".into(),
        ..Default::default()
    });

    // Diabolic Edict: 1B — Target player sacrifices a creature
    db.insert(CardDef {
        id: ids::DIABOLIC_EDICT,
        name: "Diabolic Edict".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::SacrificeCreatures {
            count: 1,
            target: TargetSpec::Opponent,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Target player sacrifices a creature.".into(),
        ..Default::default()
    });

    // Tragic Slip: B — Target creature gets -1/-1 until end of turn (simplified)
    db.insert(CardDef {
        id: ids::TRAGIC_SLIP,
        name: "Tragic Slip".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::Debuff {
            power: -1,
            toughness: -1,
            until_eot: true,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Target creature gets -1/-1 until end of turn. Morbid — That creature gets -13/-13 until end of turn instead if a creature died this turn.".into(),
        ..Default::default()
    });

    // --- Red creatures ---

    // Ash Zealot: RR 2/2 Human Warrior — First strike, haste
    db.insert(CardDef {
        id: ids::ASH_ZEALOT,
        name: "Ash Zealot".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Warrior".into())],
        keywords: vec![KeywordAbility::FirstStrike, KeywordAbility::Haste],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "First strike, haste".into(),
        ..Default::default()
    });

    // Ember Hauler: RR 2/2 Goblin — sac: deal 2 damage (simplified as vanilla)
    db.insert(CardDef {
        id: ids::EMBER_HAULER,
        name: "Ember Hauler".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Goblin".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{1}, Sacrifice Ember Hauler: Ember Hauler deals 2 damage to any target."
            .into(),
        ..Default::default()
    });

    // Hellrider: 2RR 3/3 Devil — Haste
    db.insert(CardDef {
        id: ids::HELLRIDER,
        name: "Hellrider".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Devil".into())],
        keywords: vec![KeywordAbility::Haste],
        power: Some(3),
        toughness: Some(3),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Haste. Whenever a creature you control attacks, Hellrider deals 1 damage to the defending player.".into(),
        ..Default::default()
    });

    // Jackal Pup: R 2/1 Hound
    db.insert(CardDef {
        id: ids::JACKAL_PUP,
        name: "Jackal Pup".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Hound".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Whenever Jackal Pup is dealt damage, it deals that much damage to you."
            .into(),
        ..Default::default()
    });

    // Keldon Marauders: 1R 3/3 Human Warrior — Vanishing 2 (simplified)
    db.insert(CardDef {
        id: ids::KELDON_MARAUDERS,
        name: "Keldon Marauders".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Warrior".into())],
        keywords: vec![],
        power: Some(3),
        toughness: Some(3),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::DealDamage {
                amount: 1,
                target: TargetSpec::Opponent,
            },
            description: "When Keldon Marauders enters the battlefield, deal 1 damage to target opponent.".into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Vanishing 2. When Keldon Marauders enters or leaves the battlefield, it deals 1 damage to target player or planeswalker.".into(),
        ..Default::default()
    });

    // Vexing Devil: R 4/3 Devil
    db.insert(CardDef {
        id: ids::VEXING_DEVIL,
        name: "Vexing Devil".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Devil".into())],
        keywords: vec![],
        power: Some(4),
        toughness: Some(3),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "When Vexing Devil enters the battlefield, any opponent may have it deal 4 damage to them. If a player does, sacrifice Vexing Devil.".into(),
        ..Default::default()
    });

    // Eidolon of the Great Revel: RR 2/2 Spirit
    db.insert(CardDef {
        id: ids::EIDOLON_OF_GREAT_REVEL,
        name: "Eidolon of the Great Revel".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Spirit".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Whenever a player casts a spell with mana value 3 or less, Eidolon of the Great Revel deals 2 damage to that player.".into(),
        ..Default::default()
    });

    // Young Pyromancer: 1R 2/1 Human Shaman
    db.insert(CardDef {
        id: ids::YOUNG_PYROMANCER,
        name: "Young Pyromancer".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Shaman".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Whenever you cast an instant or sorcery spell, create a 1/1 red Elemental creature token.".into(),
        ..Default::default()
    });

    // Goblin Chainwhirler: RRR 3/3 Goblin Warrior — First strike, ETB 1 damage to each opponent and each creature/planeswalker they control
    db.insert(CardDef {
        id: ids::GOBLIN_CHAINWHIRLER,
        name: "Goblin Chainwhirler".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 3, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Goblin".into()), Subtype("Warrior".into())],
        keywords: vec![KeywordAbility::FirstStrike],
        power: Some(3),
        toughness: Some(3),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::DealDamage {
                amount: 1,
                target: TargetSpec::EachCreature,
            },
            description: "When Goblin Chainwhirler enters the battlefield, it deals 1 damage to each opponent and each creature and planeswalker they control.".into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "First strike. When Goblin Chainwhirler enters the battlefield, it deals 1 damage to each opponent and each creature and planeswalker they control.".into(),
        ..Default::default()
    });

    // --- Red spells ---

    // Chain Lightning: R — Deal 3 damage to any target
    db.insert(CardDef {
        id: ids::CHAIN_LIGHTNING,
        name: "Chain Lightning".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Sorcery],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DealDamage {
            amount: 3,
            target: TargetSpec::CreatureOrPlayer,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Chain Lightning deals 3 damage to any target.".into(),
        ..Default::default()
    });

    // Searing Blaze: RR — Deal 1 damage to target player and 1 to target creature (simplified as 3 to creature)
    db.insert(CardDef {
        id: ids::SEARING_BLAZE,
        name: "Searing Blaze".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DealDamage {
            amount: 3,
            target: TargetSpec::CreatureOrPlayer,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Searing Blaze deals 1 damage to target player or planeswalker and 1 damage to target creature that player or that planeswalker's controller controls. Landfall — If you had a land enter the battlefield under your control this turn, Searing Blaze deals 3 damage to that player or planeswalker and 3 damage to that creature instead.".into(),
        ..Default::default()
    });

    // Skullcrack: 1R — Deal 3 damage to opponent, can't gain life
    db.insert(CardDef {
        id: ids::SKULLCRACK,
        name: "Skullcrack".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DealDamage {
            amount: 3,
            target: TargetSpec::AnyPlayer,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Players can't gain life this turn. Damage can't be prevented this turn. Skullcrack deals 3 damage to target player or planeswalker.".into(),
        ..Default::default()
    });

    // Flames of the Blood Hand: 2R — Deal 4 to any player
    db.insert(CardDef {
        id: ids::FLAMES_OF_THE_BLOOD_HAND,
        name: "Flames of the Blood Hand".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DealDamage {
            amount: 4,
            target: TargetSpec::AnyPlayer,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flames of the Blood Hand deals 4 damage to target player or planeswalker. The damage can't be prevented, and if a player would gain life this turn, that player gains no life instead.".into(),
        ..Default::default()
    });

    // Searing Blood: RR — Deal 2 damage to target creature, 3 to controller if it dies
    db.insert(CardDef {
        id: ids::SEARING_BLOOD,
        name: "Searing Blood".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DealDamage {
            amount: 2,
            target: TargetSpec::AnyCreature,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Searing Blood deals 2 damage to target creature. When that creature dies this turn, Searing Blood deals 3 damage to the creature's controller.".into(),
        ..Default::default()
    });

    // --- Green creatures ---

    // Tarmogoyf: 1G 0/1 Lhurgoyf (simplified — no graveyard counting CDA)
    db.insert(CardDef {
        id: ids::TARMOGOYF,
        name: "Tarmogoyf".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Lhurgoyf".into())],
        keywords: vec![],
        power: Some(0),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Tarmogoyf's power is equal to the number of card types among cards in all graveyards and its toughness is equal to that number plus 1.".into(),
        ..Default::default()
    });

    // Scavenging Ooze: 1G 2/2 Ooze
    db.insert(CardDef {
        id: ids::SCAVENGING_OOZE,
        name: "Scavenging Ooze".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Ooze".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{G}: Exile target card from a graveyard. If it was a creature card, put a +1/+1 counter on Scavenging Ooze and you gain 1 life.".into(),
        ..Default::default()
    });

    // Strangleroot Geist: GG 2/1 Spirit — Haste, undying (simplified)
    db.insert(CardDef {
        id: ids::STRANGLEROOT_GEIST,
        name: "Strangleroot Geist".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Spirit".into())],
        keywords: vec![KeywordAbility::Haste],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Haste. Undying.".into(),
        ..Default::default()
    });

    // Wild Nacatl: G 1/1 Cat Warrior (simplified — no domain bonus)
    db.insert(CardDef {
        id: ids::WILD_NACATL,
        name: "Wild Nacatl".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Cat".into()), Subtype("Warrior".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Wild Nacatl gets +1/+1 as long as you control a Mountain. Wild Nacatl gets +1/+1 as long as you control a Plains.".into(),
        ..Default::default()
    });

    // Experiment One: G 1/1 Human Ooze
    db.insert(CardDef {
        id: ids::EXPERIMENT_ONE,
        name: "Experiment One".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Human".into()), Subtype("Ooze".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text:
            "Evolve. Remove two +1/+1 counters from Experiment One: Regenerate Experiment One."
                .into(),
        ..Default::default()
    });

    // Dryad Militant: G/W 2/1 Dryad Soldier
    db.insert(CardDef {
        id: ids::DRYAD_MILITANT,
        name: "Dryad Militant".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Dryad".into()), Subtype("Soldier".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "If an instant or sorcery card would be put into a graveyard from anywhere, exile it instead.".into(),
        ..Default::default()
    });

    // Thrun, the Last Troll: 2GG 4/4 Troll Shaman — trample (simplified, no hexproof/regen)
    db.insert(CardDef {
        id: ids::THRUN_LAST_TROLL,
        name: "Thrun, the Last Troll".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Troll".into()), Subtype("Shaman".into())],
        keywords: vec![KeywordAbility::Trample],
        power: Some(4),
        toughness: Some(4),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text:
            "This spell can't be countered. Hexproof. {1}{G}: Regenerate Thrun, the Last Troll."
                .into(),
        ..Default::default()
    });

    // Rancor Beast: 2G 3/2 Beast — Trample
    db.insert(CardDef {
        id: ids::RANCOR_BEAST,
        name: "Rancor Beast".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Beast".into())],
        keywords: vec![KeywordAbility::Trample],
        power: Some(3),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Trample".into(),
        ..Default::default()
    });

    // --- Green spells ---

    // Rancor: G — Enchant creature gets +2/+0 and trample (simplified as buff)
    db.insert(CardDef {
        id: ids::RANCOR,
        name: "Rancor".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Enchantment],
        supertypes: vec![],
        subtypes: vec![Subtype("Aura".into())],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::Buff {
            power: 2,
            toughness: 0,
            until_eot: false,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Enchant creature. Enchanted creature gets +2/+0 and has trample. When Rancor is put into a graveyard from the battlefield, return Rancor to its owner's hand.".into(),
        ..Default::default()
    });

    // Vines of Vastwood: G — Target creature can't be the target of spells, +4/+4 if kicked (simplified as buff)
    db.insert(CardDef {
        id: ids::VINES_OF_VASTWOOD,
        name: "Vines of Vastwood".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::Buff {
            power: 4,
            toughness: 4,
            until_eot: true,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Kicker {G}. Target creature can't be the target of spells or abilities your opponents control this turn. If Vines of Vastwood was kicked, that creature gets +4/+4 until end of turn.".into(),
        ..Default::default()
    });

    // Collected Company: 3G — Look at top 6, put up to 2 creatures with MV 3 or less onto battlefield (simplified as draw 2)
    db.insert(CardDef {
        id: ids::COLLECTED_COMPANY,
        name: "Collected Company".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DrawCards { count: 2 }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Look at the top six cards of your library. Put up to two creature cards with mana value 3 or less from among them onto the battlefield. Put the rest on the bottom of your library in any random order.".into(),
        ..Default::default()
    });

    // Gaea's Anthem: 1GG — Creatures you control get +1/+1
    db.insert(CardDef {
        id: ids::GAEA_ANTHEM,
        name: "Gaea's Anthem".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Enchantment],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![StaticAbility::Anthem {
            power: 1,
            toughness: 1,
            affected: AffectedObjects::OtherCreaturesControlledBy(0),
        }],
        enters_tapped: false,
        oracle_text: "Creatures you control get +1/+1.".into(),
        ..Default::default()
    });

    // --- Artifacts ---

    // Sol Ring: 1 — {T}: Add {C}{C} (simplified as mana ability)
    db.insert(CardDef {
        id: ids::SOL_RING,
        name: "Sol Ring".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![ManaAbility::TapForColorlessAmount(2)],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Add {C}{C}.".into(),
        ..Default::default()
    });

    // Signal Pest: 1 — 0/1 Artifact Creature — Flying
    db.insert(CardDef {
        id: ids::SIGNAL_PEST,
        name: "Signal Pest".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact, CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Pest".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(0),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text:
            "Battle cry. Signal Pest can't be blocked except by creatures with flying or reach."
                .into(),
        ..Default::default()
    });

    // Vault Skirge: 1B/P — 1/1 Artifact Creature — Flying, lifelink
    db.insert(CardDef {
        id: ids::VAULT_SKIRGE,
        name: "Vault Skirge".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Artifact, CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Phyrexian".into()), Subtype("Imp".into())],
        keywords: vec![KeywordAbility::Flying, KeywordAbility::Lifelink],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Flying, lifelink".into(),
        ..Default::default()
    });

    // Cranial Plating: 2 — Equipment artifact (simplified as buff enchantment)
    db.insert(CardDef {
        id: ids::CRANIAL_PLATING,
        name: "Cranial Plating".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![],
        subtypes: vec![Subtype("Equipment".into())],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        equip_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        oracle_text: "Equipped creature gets +1/+0 for each artifact you control. {B}{B}: Attach Cranial Plating to target creature you control. Equip {1}.".into(),
        ..Default::default()
    });

    // Steel Overseer: 2 — 1/1 Artifact Creature
    db.insert(CardDef {
        id: ids::STEEL_OVERSEER,
        name: "Steel Overseer".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact, CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Construct".into())],
        keywords: vec![],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{T}: Put a +1/+1 counter on each artifact creature you control.".into(),
        ..Default::default()
    });

    // --- Multicolor ---

    // Lightning Helix: RW — Deal 3, gain 3 life
    db.insert(CardDef {
        id: ids::LIGHTNING_HELIX,
        name: "Lightning Helix".into(),
        mana_cost: Some(ManaCost::new(0, 1, 0, 0, 1, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::Multiple(vec![
            Effect::DealDamage {
                amount: 3,
                target: TargetSpec::CreatureOrPlayer,
            },
            Effect::GainLife { amount: 3 },
        ])),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Lightning Helix deals 3 damage to any target and you gain 3 life.".into(),
        ..Default::default()
    });

    // Terminate: BR — Destroy target creature
    db.insert(CardDef {
        id: ids::TERMINATE,
        name: "Terminate".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 1, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::DestroyTarget {
            target: TargetSpec::AnyCreature,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Destroy target creature. It can't be regenerated.".into(),
        ..Default::default()
    });

    // Geist of Saint Traft: 1WU 2/2 Spirit Cleric — Hexproof (simplified)
    db.insert(CardDef {
        id: ids::GEIST_OF_SAINT_TRAFT,
        name: "Geist of Saint Traft".into(),
        mana_cost: Some(ManaCost::new(1, 1, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Spirit".into()), Subtype("Cleric".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Hexproof. Whenever Geist of Saint Traft attacks, create a 4/4 white Angel creature token with flying that's tapped and attacking. Exile that token at end of combat.".into(),
        ..Default::default()
    });

    // Fleecemane Lion: GW 3/3 Cat
    db.insert(CardDef {
        id: ids::FLEECEMANE_LION,
        name: "Fleecemane Lion".into(),
        mana_cost: Some(ManaCost::new(0, 1, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Cat".into())],
        keywords: vec![],
        power: Some(3),
        toughness: Some(3),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "{3}{G}{W}: Monstrosity 1. As long as Fleecemane Lion is monstrous, it has hexproof and indestructible.".into(),
        ..Default::default()
    });

    // Tidehollow Sculler: WB 2/2 Zombie — ETB exile a nonland card from opponent's hand (simplified as discard)
    db.insert(CardDef {
        id: ids::TIDEHOLLOW_SCULLER,
        name: "Tidehollow Sculler".into(),
        mana_cost: Some(ManaCost::new(0, 1, 0, 1, 0, 0)),
        card_types: vec![CardType::Artifact, CardType::Creature],
        supertypes: vec![],
        subtypes: vec![Subtype("Zombie".into())],
        keywords: vec![],
        power: Some(2),
        toughness: Some(2),
        mana_abilities: vec![],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::DiscardCards {
                count: 1,
                target: TargetSpec::Opponent,
            },
            description: "When Tidehollow Sculler enters the battlefield, target opponent reveals their hand and you choose a nonland card from it. Exile that card.".into(),
        }],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "When Tidehollow Sculler enters the battlefield, target opponent reveals their hand and you choose a nonland card from it. Exile that card. When Tidehollow Sculler leaves the battlefield, return the exiled card to its owner's hand.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::ZADA_HEDRON_GRINDER,
        name: "Zada, Hedron Grinder".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Goblin".into()), Subtype("Ally".into())],
        power: Some(3),
        toughness: Some(3),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::YouCastInstantOrSorceryTargetingOnlySelf,
            effect: Effect::CopyCastSpellForOtherCreatures,
            description: "Whenever you cast an instant or sorcery spell that targets only Zada, Hedron Grinder, copy that spell for each other creature you control that the spell could target. Each copy targets a different one of those creatures.".into(),
        }],
        enters_tapped: false,
        oracle_text: "Whenever you cast an instant or sorcery spell that targets only Zada, Hedron Grinder, copy that spell for each other creature you control that the spell could target. Each copy targets a different one of those creatures.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::KRENKO_MOB_BOSS,
        name: "Krenko, Mob Boss".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Goblin".into()), Subtype("Warrior".into())],
        power: Some(3),
        toughness: Some(3),
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(),
            requires_tap: true,
            sacrifice_cost: None,
            life_cost: 0,
            effect: Effect::CreateTokens {
                token: PredefinedToken::Goblin.to_token_def(),
                count: DynamicValue::PermanentsWithSubtype("Goblin".into()),
            },
            description: "{T}: Create X 1/1 red Goblin creature tokens, where X is the number of Goblins you control.".into(),
        }],
        enters_tapped: false,
        oracle_text: "{T}: Create X 1/1 red Goblin creature tokens, where X is the number of Goblins you control.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::BRIGHTSTONE_RITUAL,
        name: "Brightstone Ritual".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::AddDynamicMana {
            color: Color::Red,
            count: DynamicValue::AllPermanentsWithSubtype("Goblin".into()),
        }),
        enters_tapped: false,
        oracle_text: "Add {R} for each Goblin on the battlefield.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::GOBLIN_CHIEFTAIN,
        name: "Goblin Chieftain".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 2, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Goblin".into())],
        keywords: vec![KeywordAbility::Haste],
        power: Some(2),
        toughness: Some(2),
        static_abilities: vec![
            StaticAbility::Anthem {
                power: 1,
                toughness: 1,
                affected: AffectedObjects::OtherCreaturesWithSubtypeControlledBySource("Goblin".into()),
            },
            StaticAbility::GrantKeyword {
                keyword: KeywordAbility::Haste,
                affected: AffectedObjects::OtherCreaturesWithSubtypeControlledBySource("Goblin".into()),
            },
        ],
        enters_tapped: false,
        oracle_text: "Haste\nOther Goblin creatures you control get +1/+1 and have haste.".into(),
        ..Default::default()
    });

    // Goblin Storm mana and token spells.
    db.insert(CardDef {
        id: ids::SEETHING_SONG,
        name: "Seething Song".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::AddMana { color: Some(Color::Red), amount: 5 }),
        enters_tapped: false,
        oracle_text: "Add {R}{R}{R}{R}{R}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::BATTLE_HYMN,
        name: "Battle Hymn".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::AddDynamicMana { color: Color::Red, count: DynamicValue::CreaturesControlled }),
        enters_tapped: false,
        oracle_text: "Add {R} for each creature you control.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::DRAGON_FODDER,
        name: "Dragon Fodder".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::CreateTokens {
            token: TokenDef {
                name: "Goblin".into(),
                power: 1,
                toughness: 1,
                colors: vec![Color::Red],
                subtypes: vec![Subtype("Goblin".into())],
                keywords: vec![],
            },
            count: DynamicValue::Fixed(2),
        }),
        enters_tapped: false,
        oracle_text: "Create two 1/1 red Goblin creature tokens.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::KRENKOS_COMMAND,
        name: "Krenko's Command".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::CreateTokens {
            token: TokenDef {
                name: "Goblin".into(),
                power: 1,
                toughness: 1,
                colors: vec![Color::Red],
                subtypes: vec![Subtype("Goblin".into())],
                keywords: vec![],
            },
            count: DynamicValue::Fixed(2),
        }),
        enters_tapped: false,
        oracle_text: "Create two 1/1 red Goblin creature tokens.".into(),
        ..Default::default()
    });

    // Dark Ritual: B — Add BBB
    db.insert(CardDef {
        id: ids::DARK_RITUAL,
        name: "Dark Ritual".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Instant],
        supertypes: vec![],
        subtypes: vec![],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![],
        spell_effect: Some(Effect::AddMana {
            color: Some(Color::Black),
            amount: 3,
        }),
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: false,
        oracle_text: "Add {B}{B}{B}.".into(),
        ..Default::default()
    });

    // Stomping Ground: dual land (simplified as tapped forest)
    db.insert(CardDef {
        id: ids::STOMPING_GROUND,
        name: "Stomping Ground".into(),
        mana_cost: None,
        card_types: vec![CardType::Land],
        supertypes: vec![],
        subtypes: vec![Subtype("Mountain".into()), Subtype("Forest".into())],
        keywords: vec![],
        power: None,
        toughness: None,
        mana_abilities: vec![ManaAbility::TapForColor(Color::Red)],
        spell_effect: None,
        activated_abilities: vec![],
        triggered_abilities: vec![],
        starting_loyalty: None,
        static_abilities: vec![],
        enters_tapped: true,
        oracle_text: "As Stomping Ground enters the battlefield, you may pay 2 life. If you don't, it enters the battlefield tapped. {T}: Add {R} or {G}.".into(),
        ..Default::default()
    });

    // =====================================================================
    // Kinnan, Bonder Prodigy Commander Deck — Card definitions
    // =====================================================================

    // ---- Lands ----

    db.insert(CardDef {
        id: ids::ANCIENT_TOMB,
        name: "Ancient Tomb".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColorlessAmount(2)],
        oracle_text: "{T}: Add {C}{C}. Ancient Tomb deals 2 damage to you.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::BOSEIJU_WHO_ENDURES,
        name: "Boseiju, Who Endures".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        oracle_text: "{T}: Add {G}. Channel — {1}{G}, Discard Boseiju: Destroy target artifact, enchantment, or nonbasic land an opponent controls.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::BREEDING_POOL,
        name: "Breeding Pool".into(),
        card_types: vec![CardType::Land],
        subtypes: vec![Subtype("Forest".into()), Subtype("Island".into())],
        mana_abilities: vec![ManaAbility::TapForChoice(vec![Color::Green, Color::Blue])],
        oracle_text: "({T}: Add {G} or {U}.) As Breeding Pool enters, you may pay 2 life. If you don't, it enters tapped.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::COMMAND_TOWER,
        name: "Command Tower".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "{T}: Add one mana of any color in your commander's color identity.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FLOODED_STRAND,
        name: "Flooded Strand".into(),
        card_types: vec![CardType::Land],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(),
            requires_tap: true,
            sacrifice_cost: Some(SacrificeCost::SelfSacrifice), life_cost: 1,
            effect: Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![Subtype("Plains".into()), Subtype("Island".into())] },
            description: "{T}, Pay 1 life, Sacrifice Flooded Strand: Search your library for a Plains or Island card, put it onto the battlefield, then shuffle.".into(),
        }],
        oracle_text: "{T}, Pay 1 life, Sacrifice Flooded Strand: Search your library for a Plains or Island card, put it onto the battlefield, then shuffle.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::GAEAS_CRADLE,
        name: "Gaea's Cradle".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        oracle_text: "{T}: Add {G} for each creature you control.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::GEMSTONE_CAVERNS,
        name: "Gemstone Caverns".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "If Gemstone Caverns is in your opening hand and you're not the starting player, you may begin the game with it on the battlefield with a luck counter on it. If you do, exile a card from your hand. {T}: Add {C}. If Gemstone Caverns has a luck counter on it, instead add one mana of any color.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::INVENTORS_FAIR,
        name: "Inventors' Fair".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForColorless],
        oracle_text: "At the beginning of your upkeep, if you control three or more artifacts, you gain 1 life. {T}: Add {C}. {4}, {T}, Sacrifice Inventors' Fair: Search your library for an artifact card, reveal it, put it into your hand, then shuffle. Activate only if you control three or more artifacts.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MINAMO_SCHOOL,
        name: "Minamo, School at Water's Edge".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Blue)],
        oracle_text: "{T}: Add {U}. {U}, {T}: Untap target legendary permanent.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MISTY_RAINFOREST,
        name: "Misty Rainforest".into(),
        card_types: vec![CardType::Land],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(),
            requires_tap: true,
            sacrifice_cost: Some(SacrificeCost::SelfSacrifice), life_cost: 1,
            effect: Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![Subtype("Forest".into()), Subtype("Island".into())] },
            description: "{T}, Pay 1 life, Sacrifice Misty Rainforest: Search your library for a Forest or Island card, put it onto the battlefield, then shuffle.".into(),
        }],
        oracle_text: "{T}, Pay 1 life, Sacrifice Misty Rainforest: Search your library for a Forest or Island card, put it onto the battlefield, then shuffle.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MISTRISE_VILLAGE,
        name: "Mistrise Village".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Blue)],
        oracle_text: "Mistrise Village enters tapped unless you control a Mountain or a Forest. {T}: Add {U}. {U}, {T}: The next spell you cast this turn can't be countered.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::OTAWARA_SOARING_CITY,
        name: "Otawara, Soaring City".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Blue)],
        oracle_text: "{T}: Add {U}. Channel — {3}{U}, Discard Otawara: Return target artifact, creature, or planeswalker to its owner's hand.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SEAT_OF_THE_SYNOD,
        name: "Seat of the Synod".into(),
        card_types: vec![CardType::Artifact, CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Blue)],
        oracle_text: "{T}: Add {U}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SHIFTING_WOODLAND,
        name: "Shifting Woodland".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        oracle_text: "Shifting Woodland enters tapped unless you control two or more other lands. {T}: Add {G}. Delirium — {2}{G}{G}: Shifting Woodland becomes a copy of target permanent card in your graveyard until end of turn.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SNOW_COVERED_FOREST,
        name: "Snow-Covered Forest".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Basic, Supertype::Snow],
        subtypes: vec![Subtype("Forest".into())],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        oracle_text: "{T}: Add {G}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SNOW_COVERED_ISLAND,
        name: "Snow-Covered Island".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Basic, Supertype::Snow],
        subtypes: vec![Subtype("Island".into())],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Blue)],
        oracle_text: "{T}: Add {U}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::TREASURE_VAULT,
        name: "Treasure Vault".into(),
        card_types: vec![CardType::Artifact, CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColorless],
        oracle_text:
            "{T}: Add {C}. {X}{X}, {T}, Sacrifice Treasure Vault: Create X Treasure tokens.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::TREE_OF_TALES,
        name: "Tree of Tales".into(),
        card_types: vec![CardType::Artifact, CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        oracle_text: "{T}: Add {G}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::TROPICAL_ISLAND,
        name: "Tropical Island".into(),
        card_types: vec![CardType::Land],
        subtypes: vec![Subtype("Forest".into()), Subtype("Island".into())],
        mana_abilities: vec![ManaAbility::TapForChoice(vec![Color::Green, Color::Blue])],
        oracle_text: "({T}: Add {G} or {U}.)".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::WATERLOGGED_GROVE,
        name: "Waterlogged Grove".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForChoice(vec![Color::Green, Color::Blue])],
        oracle_text:
            "{T}, Pay 1 life: Add {G} or {U}. {1}, {T}, Sacrifice Waterlogged Grove: Draw a card."
                .into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::WINDSWEPT_HEATH,
        name: "Windswept Heath".into(),
        card_types: vec![CardType::Land],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(),
            requires_tap: true,
            sacrifice_cost: Some(SacrificeCost::SelfSacrifice), life_cost: 1,
            effect: Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![Subtype("Forest".into()), Subtype("Plains".into())] },
            description: "{T}, Pay 1 life, Sacrifice Windswept Heath: Search your library for a Forest or Plains card, put it onto the battlefield, then shuffle.".into(),
        }],
        oracle_text: "{T}, Pay 1 life, Sacrifice Windswept Heath: Search your library for a Forest or Plains card, put it onto the battlefield, then shuffle.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::YAVIMAYA_COAST,
        name: "Yavimaya Coast".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForChoice(vec![Color::Green, Color::Blue])],
        oracle_text: "{T}: Add {C}. {T}: Add {G} or {U}. Yavimaya Coast deals 1 damage to you."
            .into(),
        ..Default::default()
    });

    // ---- Artifacts ----

    db.insert(CardDef {
        id: ids::ARCANE_SIGNET,
        name: "Arcane Signet".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "{T}: Add one mana of any color in your commander's color identity.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::BASALT_MONOLITH,
        name: "Basalt Monolith".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForColorlessAmount(3)],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(3, 0, 0, 0, 0, 0),
            requires_tap: false,
            sacrifice_cost: None, life_cost: 0,
            effect: Effect::UntapTarget { target: TargetSpec::Controller },
            description: "{3}: Untap Basalt Monolith.".into(),
        }],
        oracle_text: "Basalt Monolith doesn't untap during your untap step. {T}: Add {C}{C}{C}. {3}: Untap Basalt Monolith.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::CHROME_MOX,
        name: "Chrome Mox".into(),
        mana_cost: Some(ManaCost::zero()),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "Imprint — When Chrome Mox enters, you may exile a nonartifact, nonland card from your hand. {T}: Add one mana of any of the exiled card's colors.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FELLWAR_STONE,
        name: "Fellwar Stone".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text:
            "{T}: Add one mana of any color that a land an opponent controls could produce.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::GRIM_MONOLITH,
        name: "Grim Monolith".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForColorlessAmount(3)],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(4, 0, 0, 0, 0, 0),
            requires_tap: false,
            sacrifice_cost: None, life_cost: 0,
            effect: Effect::UntapTarget { target: TargetSpec::Controller },
            description: "{4}: Untap Grim Monolith.".into(),
        }],
        oracle_text: "Grim Monolith doesn't untap during your untap step. {T}: Add {C}{C}{C}. {4}: Untap Grim Monolith.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::LOTUS_PETAL,
        name: "Lotus Petal".into(),
        mana_cost: Some(ManaCost::zero()),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "{T}, Sacrifice Lotus Petal: Add one mana of any color.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MANA_VAULT,
        name: "Mana Vault".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForColorlessAmount(3)],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(4, 0, 0, 0, 0, 0),
            requires_tap: false,
            sacrifice_cost: None, life_cost: 0,
            effect: Effect::UntapTarget { target: TargetSpec::Controller },
            description: "{4}: Untap Mana Vault.".into(),
        }],
        oracle_text: "Mana Vault doesn't untap during your untap step. At the beginning of your upkeep, you may pay {4}. If you don't, Mana Vault deals 1 damage to you. {T}: Add {C}{C}{C}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MOX_AMBER,
        name: "Mox Amber".into(),
        mana_cost: Some(ManaCost::zero()),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForLegendaryColors],
        oracle_text: "{T}: Add one mana of any color among legendary creatures and planeswalkers you control.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MOX_DIAMOND,
        name: "Mox Diamond".into(),
        mana_cost: Some(ManaCost::zero()),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "If Mox Diamond would enter, you may discard a land card instead. If you do, put Mox Diamond onto the battlefield. If you don't, put it into its owner's graveyard. {T}: Add one mana of any color.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MOX_OPAL,
        name: "Mox Opal".into(),
        mana_cost: Some(ManaCost::zero()),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "Metalcraft — {T}: Add one mana of any color. Activate only if you control three or more artifacts.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MOONSILVER_KEY,
        name: "Moonsilver Key".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Hand, subtype_filter: vec![] }),
        oracle_text: "{1}, {T}, Sacrifice Moonsilver Key: Search your library for an artifact card with a mana ability or a basic land card, reveal it, put it into your hand, then shuffle.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SIMIC_SIGNET,
        name: "Simic Signet".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForChoice(vec![Color::Green, Color::Blue])],
        oracle_text: "{1}, {T}: Add {G}{U}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SPRINGLEAF_DRUM,
        name: "Springleaf Drum".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "{T}, Tap an untapped creature you control: Add one mana of any color.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::TALISMAN_OF_CURIOSITY,
        name: "Talisman of Curiosity".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForChoice(vec![Color::Green, Color::Blue])],
        oracle_text: "{T}: Add {C}. {T}: Add {G} or {U}. Talisman of Curiosity deals 1 damage to you.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::AGATHAS_SOUL_CAULDRON,
        name: "Agatha's Soul Cauldron".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        oracle_text: "You may spend mana as though it were mana of any color to activate abilities of creatures you control. {T}: Exile target card from a graveyard. When a creature card is exiled this way, put a +1/+1 counter on target creature you control. That creature gains all activated abilities of the exiled card.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::THE_ONE_RING,
        name: "The One Ring".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        keywords: vec![KeywordAbility::Indestructible],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(1, 0, 0, 0, 0, 0),
            requires_tap: true,
            sacrifice_cost: None, life_cost: 0,
            effect: Effect::DrawCards { count: 1 },
            description: "{1}, {T}: Put a burden counter on The One Ring, then draw a card for each burden counter on The One Ring.".into(),
        }],
        oracle_text: "Indestructible. When The One Ring enters, if you cast it, you gain protection from everything until your next turn. At the beginning of your upkeep, you lose 1 life for each burden counter on The One Ring. {T}: Put a burden counter on The One Ring, then draw a card for each burden counter on The One Ring.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MIRAGE_MIRROR,
        name: "Mirage Mirror".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        oracle_text: "{2}: Mirage Mirror becomes a copy of target artifact, creature, enchantment, or land until end of turn.".into(),
        ..Default::default()
    });

    // Walking Ballista: modeled as an artifact with a damage-sink ability.
    // In real MTG it enters with X +1/+1 counters and removes them to deal
    // damage. Here we simplify: {4} pay-to-ping ability represents the loop
    // of "add counter, remove counter to deal 1 damage." The 3-piece combo
    // (Basalt + Kinnan + Ballista) handles the instant-win case.
    db.insert(CardDef {
        id: ids::WALKING_BALLISTA,
        name: "Walking Ballista".into(),
        mana_cost: Some(ManaCost::zero()),
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(4, 0, 0, 0, 0, 0),
            requires_tap: false,
            sacrifice_cost: None, life_cost: 0,
            effect: Effect::DealDamage { amount: 1, target: TargetSpec::Opponent },
            description: "{4}: Walking Ballista deals 1 damage to target opponent.".into(),
        }],
        oracle_text: "Walking Ballista enters with X +1/+1 counters on it. {4}: Put a +1/+1 counter on Walking Ballista. Remove a +1/+1 counter from Walking Ballista: It deals 1 damage to any target.".into(),
        ..Default::default()
    });

    // ---- Creatures ----

    db.insert(CardDef {
        id: ids::KINNAN_BONDER_PRODIGY,
        name: "Kinnan, Bonder Prodigy".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Human".into()), Subtype("Druid".into())],
        power: Some(2),
        toughness: Some(2),
        static_abilities: vec![StaticAbility::ManaFromNonlandBonus],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(5, 0, 1, 0, 0, 1),
            requires_tap: false,
            sacrifice_cost: None, life_cost: 0,
            effect: Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![] },
            description: "{5}{G}{U}: Look at the top five cards of your library. You may put a non-Human creature card from among them onto the battlefield. Put the rest on the bottom of your library in a random order.".into(),
        }],
        oracle_text: "Whenever you tap a nonland permanent for mana, add one mana of any type that permanent produced. {5}{G}{U}: Look at the top five cards of your library. You may put a non-Human creature card from among them onto the battlefield. Put the rest on the bottom in a random order.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::BIRDS_OF_PARADISE,
        name: "Birds of Paradise".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Bird".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(0),
        toughness: Some(1),
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "Flying. {T}: Add one mana of any color.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FYNDHORN_ELVES,
        name: "Fyndhorn Elves".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Elf".into()), Subtype("Druid".into())],
        power: Some(1),
        toughness: Some(1),
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        oracle_text: "{T}: Add {G}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::DELIGHTED_HALFLING,
        name: "Delighted Halfling".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Halfling".into()), Subtype("Citizen".into())],
        power: Some(1),
        toughness: Some(2),
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green), ManaAbility::TapForAny],
        oracle_text: "{T}: Add {G}. {T}: Add one mana of any color. Spend this mana only to cast a legendary spell, and that spell can't be countered.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::BADGERMOLE_CUB,
        name: "Badgermole Cub".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Badger".into()), Subtype("Mole".into())],
        power: Some(2),
        toughness: Some(2),
        oracle_text: "When this creature enters, earthbend 1. Whenever you tap a creature for mana, add an additional {G}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::CLEVER_IMPERSONATOR,
        name: "Clever Impersonator".into(),
        mana_cost: Some(ManaCost::new(2, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Shapeshifter".into())],
        power: Some(0),
        toughness: Some(0),
        oracle_text: "You may have Clever Impersonator enter as a copy of any nonland permanent on the battlefield.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::COLOSSAL_SKYTURTLE,
        name: "Colossal Skyturtle".into(),
        mana_cost: Some(ManaCost::new(5, 0, 1, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Turtle".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(6),
        toughness: Some(5),
        oracle_text: "Flying, ward {2}. Channel — {G}{U}, Discard Colossal Skyturtle: Return target creature to its owner's hand. Channel — {2}{G}, Discard Colossal Skyturtle: Return target permanent card from your graveyard to your hand.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::CONSECRATED_SPHINX,
        name: "Consecrated Sphinx".into(),
        mana_cost: Some(ManaCost::new(4, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Sphinx".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(4),
        toughness: Some(6),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::OpponentDrawsCard,
            effect: Effect::DrawCards { count: 2 },
            description: "Whenever an opponent draws a card, you may draw two cards.".into(),
        }],
        oracle_text: "Flying. Whenever an opponent draws a card, you may draw two cards.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::DRIFT_OF_PHANTASMS,
        name: "Drift of Phantasms".into(),
        mana_cost: Some(ManaCost::new(0, 0, 3, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Spirit".into())],
        keywords: vec![KeywordAbility::Flying, KeywordAbility::Defender],
        power: Some(0),
        toughness: Some(5),
        oracle_text: "Flying, defender. Transmute {1}{U}{U}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::ELVISH_SPIRIT_GUIDE,
        name: "Elvish Spirit Guide".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Elf".into()), Subtype("Spirit".into())],
        power: Some(2),
        toughness: Some(2),
        oracle_text: "Exile Elvish Spirit Guide from your hand: Add {G}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::ENDURANCE,
        name: "Endurance".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Elemental".into()), Subtype("Incarnation".into())],
        keywords: vec![KeywordAbility::Flash, KeywordAbility::Reach],
        power: Some(3),
        toughness: Some(4),
        oracle_text: "Flash. Reach. When Endurance enters, up to one target player shuffles their graveyard into their library.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::ENDURING_VITALITY,
        name: "Enduring Vitality".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Enchantment, CardType::Creature],
        subtypes: vec![Subtype("Elk".into()), Subtype("Glimmer".into())],
        keywords: vec![KeywordAbility::Vigilance],
        power: Some(3),
        toughness: Some(3),
        oracle_text: "Vigilance. Creatures you control have \"{T}: Add one mana of any color.\" When Enduring Vitality dies, if it was a creature, return it to the battlefield under its owner's control. It's an enchantment.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FAERIE_MASTERMIND,
        name: "Faerie Mastermind".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Faerie".into()), Subtype("Rogue".into())],
        keywords: vec![KeywordAbility::Flash, KeywordAbility::Flying],
        power: Some(2),
        toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::OpponentDrawsCard,
            effect: Effect::DrawCards { count: 1 },
            description: "Whenever an opponent draws their second card each turn, you draw a card.".into(),
        }],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(3, 0, 1, 0, 0, 0),
            requires_tap: false,
            sacrifice_cost: None, life_cost: 0,
            effect: Effect::DrawCards { count: 1 },
            description: "{3}{U}: Each player draws a card.".into(),
        }],
        oracle_text: "Flash. Flying. Whenever an opponent draws their second card each turn, you draw a card. {3}{U}: Each player draws a card.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FLESH_DUPLICATE,
        name: "Flesh Duplicate".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Shapeshifter".into())],
        power: Some(0),
        toughness: Some(0),
        oracle_text: "You may have Flesh Duplicate enter as a copy of any creature on the battlefield, except it has vanishing 3.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::HIGH_FAE_TRICKSTER,
        name: "High Fae Trickster".into(),
        mana_cost: Some(ManaCost::new(3, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Faerie".into()), Subtype("Wizard".into())],
        keywords: vec![KeywordAbility::Flash, KeywordAbility::Flying],
        power: Some(4),
        toughness: Some(2),
        oracle_text: "Flash. Flying. You may cast spells as though they had flash.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::HULLBREAKER_HORROR,
        name: "Hullbreaker Horror".into(),
        mana_cost: Some(ManaCost::new(5, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Kraken".into()), Subtype("Horror".into())],
        keywords: vec![KeywordAbility::Flash],
        power: Some(7),
        toughness: Some(8),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::YouCastSpell,
            effect: Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyNonlandPermanent },
            description: "Whenever you cast a spell, choose up to one: return target nonland permanent to its owner's hand; or counter target spell.".into(),
        }],
        oracle_text: "Flash. This spell can't be countered. Whenever you cast a spell, choose up to one — Return target nonland permanent to its owner's hand; or counter target spell.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MOCKINGBIRD,
        name: "Mockingbird".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Bird".into()), Subtype("Bard".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(1),
        toughness: Some(1),
        oracle_text: "Flying. You may have Mockingbird enter as a copy of any creature on the battlefield with mana value less than or equal to the amount of mana spent to cast Mockingbird, except it's a Bird in addition to its other types and it has flying.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::NEZAHAL_PRIMAL_TIDE,
        name: "Nezahal, Primal Tide".into(),
        mana_cost: Some(ManaCost::new(5, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Elder".into()), Subtype("Dinosaur".into())],
        power: Some(7),
        toughness: Some(7),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::OpponentCastsNoncreatureSpell,
            effect: Effect::DrawCards { count: 1 },
            description: "Whenever an opponent casts a noncreature spell, draw a card.".into(),
        }],
        oracle_text: "This spell can't be countered. Whenever an opponent casts a noncreature spell, draw a card. Discard three cards: Exile Nezahal. Return it to the battlefield tapped under its owner's control at the beginning of the next end step.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::NYXBLOOM_ANCIENT,
        name: "Nyxbloom Ancient".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 0, 0, 3)),
        card_types: vec![CardType::Creature, CardType::Enchantment],
        subtypes: vec![Subtype("Elemental".into())],
        keywords: vec![KeywordAbility::Trample],
        power: Some(5),
        toughness: Some(5),
        oracle_text: "Trample. If you tap a permanent for mana, it produces three times as much of that mana instead.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::PHYREXIAN_METAMORPH,
        name: "Phyrexian Metamorph".into(),
        mana_cost: Some(ManaCost::new(3, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Artifact, CardType::Creature],
        subtypes: vec![Subtype("Phyrexian".into()), Subtype("Shapeshifter".into())],
        power: Some(0),
        toughness: Some(0),
        oracle_text: "({U/P} can be paid with either {U} or 2 life.) You may have Phyrexian Metamorph enter as a copy of any artifact or creature on the battlefield, except it's an artifact in addition to its other types.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SEEDBORN_MUSE,
        name: "Seedborn Muse".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Spirit".into())],
        power: Some(2),
        toughness: Some(4),
        oracle_text: "Untap all permanents you control during each other player's untap step."
            .into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::THRASIOS_TRITON_HERO,
        name: "Thrasios, Triton Hero".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Merfolk".into()), Subtype("Wizard".into())],
        power: Some(1),
        toughness: Some(3),
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(4, 0, 0, 0, 0, 0),
            requires_tap: false,
            sacrifice_cost: None, life_cost: 0,
            effect: Effect::DrawCards { count: 1 },
            description: "{4}: Scry 1, then reveal the top card of your library. If it's a land card, put it onto the battlefield tapped. Otherwise, draw a card.".into(),
        }],
        oracle_text: "Partner. {4}: Scry 1, then reveal the top card of your library. If it's a land card, put it onto the battlefield tapped. Otherwise, draw a card.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::TIDESPOUT_TYRANT,
        name: "Tidespout Tyrant".into(),
        mana_cost: Some(ManaCost::new(5, 0, 3, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Djinn".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(5),
        toughness: Some(5),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::YouCastSpell,
            effect: Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent },
            description: "Whenever you cast a spell, return target permanent to its owner's hand.".into(),
        }],
        oracle_text: "Flying. Whenever you cast a spell, return target permanent to its owner's hand.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::TROPHY_MAGE,
        name: "Trophy Mage".into(),
        mana_cost: Some(ManaCost::new(2, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Human".into()), Subtype("Wizard".into())],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::SearchLibrary { destination: ZoneType::Hand, subtype_filter: vec![] },
            description: "When Trophy Mage enters, you may search your library for an artifact card with mana value 3, reveal it, put it into your hand, then shuffle.".into(),
        }],
        oracle_text: "When Trophy Mage enters, you may search your library for an artifact card with mana value 3, reveal it, put it into your hand, then shuffle.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::WAN_SHI_TONG,
        name: "Wan Shi Tong, Librarian".into(),
        mana_cost: Some(ManaCost::new(2, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Bird".into()), Subtype("Spirit".into())],
        keywords: vec![KeywordAbility::Flash, KeywordAbility::Flying, KeywordAbility::Vigilance],
        power: Some(1),
        toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::DrawCards { count: 1 },
            description: "When Wan Shi Tong enters, put X +1/+1 counters on him. Then draw half X cards, rounded down.".into(),
        }],
        oracle_text: "Flash, flying, vigilance. When Wan Shi Tong enters, put X +1/+1 counters on him. Then draw half X cards, rounded down. Whenever an opponent searches their library, put a +1/+1 counter on Wan Shi Tong and draw a card.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::WANDERING_ARCHAIC,
        name: "Wandering Archaic // Explore the Vastlands".into(),
        mana_cost: Some(ManaCost::new(5, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Avatar".into())],
        power: Some(4),
        toughness: Some(4),
        oracle_text: "Whenever an opponent casts an instant or sorcery spell, they may pay {2}. If they don't, you may copy that spell. You may choose new targets for the copy.".into(),
        ..Default::default()
    });

    // ---- Instants ----

    db.insert(CardDef {
        id: ids::AN_OFFER_YOU_CANT_REFUSE,
        name: "An Offer You Can't Refuse".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter {
            target: TargetSpec::AnySpell,
        }),
        oracle_text:
            "Counter target noncreature spell. Its controller creates two Treasure tokens.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::CHORD_OF_CALLING,
        name: "Chord of Calling".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 3)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![] }),
        oracle_text: "Convoke. Search your library for a creature card with mana value X or less, put it onto the battlefield, then shuffle.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::CROP_ROTATION,
        name: "Crop Rotation".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![] }),
        oracle_text: "As an additional cost to cast this spell, sacrifice a land. Search your library for a land card, put that card onto the battlefield, then shuffle.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::CYCLONIC_RIFT,
        name: "Cyclonic Rift".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyNonlandPermanent }),
        oracle_text: "Return target nonland permanent you don't control to its owner's hand. Overload {6}{U} (You may cast this spell for its overload cost. If you do, change 'target' to 'each'.)".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FIERCE_GUARDIANSHIP,
        name: "Fierce Guardianship".into(),
        mana_cost: Some(ManaCost::new(2, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter { target: TargetSpec::AnySpell }),
        oracle_text: "If you control a commander, you may cast this spell without paying its mana cost. Counter target noncreature spell.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FLUSTERSTORM,
        name: "Flusterstorm".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter {
            target: TargetSpec::AnySpell,
        }),
        oracle_text:
            "Counter target instant or sorcery spell unless its controller pays {1}. Storm.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FORCE_OF_NEGATION,
        name: "Force of Negation".into(),
        mana_cost: Some(ManaCost::new(1, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter { target: TargetSpec::AnySpell }),
        oracle_text: "If it's not your turn, you may exile a blue card from your hand rather than pay this spell's mana cost. Counter target noncreature spell. If that spell is countered this way, exile it instead of putting it into its owner's graveyard.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::FORCE_OF_WILL,
        name: "Force of Will".into(),
        mana_cost: Some(ManaCost::new(3, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter { target: TargetSpec::AnySpell }),
        oracle_text: "You may pay 1 life and exile a blue card from your hand rather than pay this spell's mana cost. Counter target spell.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::INTO_THE_FLOOD_MAW,
        name: "Into the Flood Maw".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyCreature }),
        oracle_text: "Gift a tapped Fish. Return target creature an opponent controls to its owner's hand. If the gift was promised, instead return target nonland permanent an opponent controls to its owner's hand.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MENTAL_MISSTEP,
        name: "Mental Misstep".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter {
            target: TargetSpec::AnySpell,
        }),
        oracle_text:
            "({U/P} can be paid with either {U} or 2 life.) Counter target spell with mana value 1."
                .into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MINDBREAK_TRAP,
        name: "Mindbreak Trap".into(),
        mana_cost: Some(ManaCost::new(2, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter { target: TargetSpec::AnySpell }),
        oracle_text: "If an opponent cast three or more spells this turn, you may pay {0} rather than pay this spell's mana cost. Exile any number of target spells.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MYSTICAL_TUTOR,
        name: "Mystical Tutor".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Library, subtype_filter: vec![] }),
        oracle_text: "Search your library for an instant or sorcery card, reveal it, then shuffle and put that card on top.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::NOXIOUS_REVIVAL,
        name: "Noxious Revival".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::ReturnToTopOfLibrary { target: TargetSpec::NoTarget }),
        oracle_text: "({G/P} can be paid with either {G} or 2 life.) Put target card from a graveyard on top of its owner's library.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::PACT_OF_NEGATION,
        name: "Pact of Negation".into(),
        mana_cost: Some(ManaCost::zero()),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter { target: TargetSpec::AnySpell }),
        oracle_text: "Counter target spell. At the beginning of your next upkeep, pay {3}{U}{U}. If you don't, you lose the game.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::SWAN_SONG,
        name: "Swan Song".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter { target: TargetSpec::AnySpell }),
        oracle_text: "Counter target enchantment, instant, or sorcery spell. Its controller creates a 2/2 blue Bird creature token with flying.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::VEIL_OF_SUMMER,
        name: "Veil of Summer".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::DrawCards { count: 1 }),
        oracle_text: "Draw a card if an opponent has cast a blue or black spell this turn. Spells you control can't be countered this turn. You and permanents you control gain hexproof from blue and from black until end of turn.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::WHIR_OF_INVENTION,
        name: "Whir of Invention".into(),
        mana_cost: Some(ManaCost::new(3, 0, 3, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![] }),
        oracle_text: "Improvise. Search your library for an artifact card with mana value X or less, put it onto the battlefield, then shuffle.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::WORLDLY_TUTOR,
        name: "Worldly Tutor".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Library, subtype_filter: vec![] }),
        oracle_text: "Search your library for a creature card, reveal it, then shuffle and put that card on top.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::MUDDLE_THE_MIXTURE,
        name: "Muddle the Mixture".into(),
        mana_cost: Some(ManaCost::new(0, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter {
            target: TargetSpec::AnySpell,
        }),
        oracle_text: "Counter target instant or sorcery spell. Transmute {1}{U}{U}.".into(),
        ..Default::default()
    });

    // ---- Sorceries ----

    db.insert(CardDef {
        id: ids::FINALE_OF_DEVASTATION,
        name: "Finale of Devastation".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![] }),
        oracle_text: "Search your library and/or graveyard for a creature card with mana value X or less and put it onto the battlefield. If you search your library this way, shuffle. If X is 10 or more, creatures you control get +X/+X and gain haste until end of turn.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::GREEN_SUNS_ZENITH,
        name: "Green Sun's Zenith".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![] }),
        oracle_text: "Search your library for a green creature card with mana value X or less, put it onto the battlefield, then shuffle. Shuffle Green Sun's Zenith into its owner's library.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::NATURES_RHYTHM,
        name: "Nature's Rhythm".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Battlefield, subtype_filter: vec![] }),
        oracle_text: "Search your library for a creature card with mana value X or less, put it onto the battlefield, then shuffle. Harmonize {X}{G}{G}{G}{G}.".into(),
        ..Default::default()
    });

    // ---- Enchantments ----

    db.insert(CardDef {
        id: ids::MYSTIC_REMORA,
        name: "Mystic Remora".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::OpponentCastsNoncreatureSpell,
            effect: Effect::DrawCards { count: 1 },
            description: "Whenever an opponent casts a noncreature spell, you may draw a card unless that player pays {4}.".into(),
        }],
        oracle_text: "Cumulative upkeep {1}. Whenever an opponent casts a noncreature spell, you may draw a card unless that player pays {4}.".into(),
        ..Default::default()
    });

    db.insert(CardDef {
        id: ids::RHYSTIC_STUDY,
        name: "Rhystic Study".into(),
        mana_cost: Some(ManaCost::new(2, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::OpponentCastsSpell,
            effect: Effect::DrawCards { count: 1 },
            description: "Whenever an opponent casts a spell, you may draw a card unless that player pays {1}.".into(),
        }],
        oracle_text: "Whenever an opponent casts a spell, you may draw a card unless that player pays {1}.".into(),
        ..Default::default()
    });

    // ---- Planeswalkers ----

    db.insert(CardDef {
        id: ids::TEZZERET_THE_SEEKER,
        name: "Tezzeret the Seeker".into(),
        mana_cost: Some(ManaCost::new(3, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Planeswalker],
        subtypes: vec![Subtype("Tezzeret".into())],
        starting_loyalty: Some(4),
        oracle_text: "+1: Untap up to two target artifacts. -X: Search your library for an artifact card with mana value X or less, put it onto the battlefield, then shuffle. -5: Artifacts you control become artifact creatures with base power and toughness 5/5 until end of turn.".into(),
        ..Default::default()
    });

    // ---- DFC / Battle cards ----

    // Bridgeworks Battle // Tanglespan Bridgeworks (front face: sorcery)
    db.insert(CardDef {
        id: ids::BRIDGEWORKS_BATTLE,
        name: "Bridgeworks Battle // Tanglespan Bridgeworks".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::Multiple(vec![
            Effect::Buff { power: 2, toughness: 2, until_eot: true },
        ])),
        oracle_text: "Target creature you control gets +2/+2 until end of turn. It fights up to one target creature you don't control. // Tanglespan Bridgeworks — Land. As this enters, you may pay 3 life. If you don't, it enters tapped. {T}: Add {G}.".into(),
        ..Default::default()
    });

    // Disciple of Freyalise // Garden of Freyalise (front face: creature)
    db.insert(CardDef {
        id: ids::DISCIPLE_OF_FREYALISE,
        name: "Disciple of Freyalise // Garden of Freyalise".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 3)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Elf".into()), Subtype("Druid".into())],
        power: Some(3),
        toughness: Some(3),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::DrawCards { count: 1 },
            description: "When Disciple of Freyalise enters, you may sacrifice another creature. If you do, you gain X life and draw X cards, where X is that creature's power.".into(),
        }],
        oracle_text: "When Disciple of Freyalise enters, you may sacrifice another creature. If you do, you gain X life and draw X cards, where X is that creature's power. // Garden of Freyalise — Land. As this enters, you may pay 3 life. If you don't, it enters tapped. {T}: Add {G}.".into(),
        ..Default::default()
    });

    // Hydroelectric Specimen // Hydroelectric Laboratory (front face: creature)
    db.insert(CardDef {
        id: ids::HYDROELECTRIC_SPECIMEN,
        name: "Hydroelectric Specimen // Hydroelectric Laboratory".into(),
        mana_cost: Some(ManaCost::new(2, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Weird".into())],
        keywords: vec![KeywordAbility::Flash],
        power: Some(1),
        toughness: Some(4),
        oracle_text: "Flash. When this creature enters, you may change the target of target instant or sorcery spell with a single target to this creature. // Hydroelectric Laboratory — Land. Enters tapped. {T}: Add {U}.".into(),
        ..Default::default()
    });

    // Invasion of Ikoria // Zilortha, Apex of Ikoria (battle front face)
    db.insert(CardDef {
        id: ids::INVASION_OF_IKORIA,
        name: "Invasion of Ikoria // Zilortha, Apex of Ikoria".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::SearchLibrary { destination: ZoneType::Hand, subtype_filter: vec![] }),
        oracle_text: "When Invasion of Ikoria enters, search your library for a non-Human creature card with mana value X or less, reveal it, put it into your hand, then shuffle. // Zilortha, Apex of Ikoria — 7/3 Dinosaur. Trample. Each creature you control with power greater than its toughness assigns combat damage equal to its power rather than its toughness.".into(),
        ..Default::default()
    });

    // Sink into Stupor // Soporific Springs (front face: instant)
    db.insert(CardDef {
        id: ids::SINK_INTO_STUPOR,
        name: "Sink into Stupor // Soporific Springs".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 1, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyNonlandPermanent }),
        oracle_text: "Choose one — Target opponent discards two cards; or return target nonland permanent to its owner's hand. // Soporific Springs — Land. Enters tapped. {T}: Add {U} or {B}.".into(),
        ..Default::default()
    });

    // =====================================================================
    // Ashcoat of the Shadow Swarm Commander Deck
    // =====================================================================

    // --- Commander ---
    // Ashcoat of the Shadow Swarm {3}{B}{B}
    // Legendary Creature — Rat 4/4
    // Whenever Ashcoat attacks, other Rats you control get +2/+2 until EOT.
    // Whenever a Rat you control dies, you may draw a card. If you do, discard a card.
    // {1}{B}, Exile four cards from your graveyard: Return a Rat creature card from
    // your graveyard to the battlefield.
    db.insert(CardDef {
        id: ids::ASHCOAT_OF_THE_SHADOW_SWARM,
        name: "Ashcoat of the Shadow Swarm".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Rat".into())],
        power: Some(4),
        toughness: Some(4),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::Attacks,
                effect: Effect::BuffOtherSubtype {
                    subtype: "Rat".into(),
                    amount: DynamicValue::CreaturesWithSubtype("Rat".into()),
                    until_eot: true,
                },
                description: "Whenever Ashcoat of the Shadow Swarm attacks, other Rats you control get +X/+X until end of turn, where X is the number of Rats you control.".into(),
            },
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureYouControlDies,
                effect: Effect::Multiple(vec![
                    Effect::DrawCards { count: 1 },
                    Effect::DiscardCards { count: 1, target: TargetSpec::Controller },
                ]),
                description: "Whenever a Rat you control dies, you may draw a card. If you do, discard a card.".into(),
            },
        ],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(1, 0, 0, 1, 0, 0),
                requires_tap: false,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::ReturnFromGraveyardToBattlefield { target: TargetSpec::Controller },
                description: "{1}{B}, Exile four cards from your graveyard: Return a Rat creature card from your graveyard to the battlefield.".into(),
            },
        ],
        oracle_text: "Whenever Ashcoat of the Shadow Swarm attacks, other Rats you control get +2/+2 until end of turn. Whenever a Rat you control dies, you may draw a card. If you do, discard a card. {1}{B}, Exile four cards from your graveyard: Return a Rat creature card from your graveyard to the battlefield.".into(),
        ..Default::default()
    });

    // --- Creatures ---

    // Assassin Initiate {B}
    // Creature — Rat Assassin 1/1
    // Deathtouch
    db.insert(CardDef {
        id: ids::ASSASSIN_INITIATE,
        name: "Assassin Initiate".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into()), Subtype("Assassin".into())],
        keywords: vec![KeywordAbility::Deathtouch],
        power: Some(1),
        toughness: Some(1),
        oracle_text: "Deathtouch".into(),
        ..Default::default()
    });

    // Ayara, First of Locthwain {B}{B}{B}
    // Legendary Creature — Elf Noble 2/3
    // Whenever another black creature enters the battlefield under your control,
    // each opponent loses 1 life and you gain 1 life.
    // {T}, Sacrifice another black creature: Draw a card.
    db.insert(CardDef {
        id: ids::AYARA_FIRST_OF_LOCTHWAIN,
        name: "Ayara, First of Locthwain".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 3, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Elf".into()), Subtype("Noble".into())],
        power: Some(2),
        toughness: Some(3),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureEnters,
                effect: Effect::Multiple(vec![
                    Effect::LoseLife { amount: 1, target: TargetSpec::Opponent },
                    Effect::GainLife { amount: 1 },
                ]),
                description: "Whenever another black creature enters the battlefield under your control, each opponent loses 1 life and you gain 1 life.".into(),
            },
        ],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::zero(),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::DrawCards { count: 1 },
                description: "{T}, Sacrifice another black creature: Draw a card.".into(),
            },
        ],
        oracle_text: "Whenever another black creature enters the battlefield under your control, each opponent loses 1 life and you gain 1 life. {T}, Sacrifice another black creature: Draw a card.".into(),
        ..Default::default()
    });

    // Blood Artist {1}{B}
    // Creature — Vampire 0/1
    // Whenever Blood Artist or another creature dies, target opponent loses 1 life
    // and you gain 1 life.
    db.insert(CardDef {
        id: ids::BLOOD_ARTIST,
        name: "Blood Artist".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Vampire".into())],
        power: Some(0),
        toughness: Some(1),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureDies,
                effect: Effect::Multiple(vec![
                    Effect::LoseLife { amount: 1, target: TargetSpec::Opponent },
                    Effect::GainLife { amount: 1 },
                ]),
                description: "Whenever Blood Artist or another creature dies, target opponent loses 1 life and you gain 1 life.".into(),
            },
        ],
        oracle_text: "Whenever Blood Artist or another creature dies, target opponent loses 1 life and you gain 1 life.".into(),
        ..Default::default()
    });

    // Bloodline Pretender {3}
    // Artifact Creature — Shapeshifter 2/2
    // Changeling. Whenever another creature with a shared creature type enters
    // the battlefield under your control, put a +1/+1 counter on Bloodline Pretender.
    db.insert(CardDef {
        id: ids::BLOODLINE_PRETENDER,
        name: "Bloodline Pretender".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact, CardType::Creature],
        subtypes: vec![Subtype("Shapeshifter".into()), Subtype("Rat".into())],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureEnters,
                effect: Effect::PutCounters { count: 1, target: TargetSpec::Controller },
                description: "Whenever another creature with a creature type that's shared with Bloodline Pretender enters the battlefield under your control, put a +1/+1 counter on Bloodline Pretender.".into(),
            },
        ],
        oracle_text: "Changeling. Whenever another creature that shares a creature type with Bloodline Pretender enters the battlefield under your control, put a +1/+1 counter on Bloodline Pretender.".into(),
        ..Default::default()
    });

    // Burglar Rat {1}{B}
    // Creature — Rat 1/1
    // When Burglar Rat enters the battlefield, each opponent discards a card.
    db.insert(CardDef {
        id: ids::BURGLAR_RAT,
        name: "Burglar Rat".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into())],
        power: Some(1),
        toughness: Some(1),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::DiscardCards { count: 1, target: TargetSpec::Opponent },
                description: "When Burglar Rat enters the battlefield, each opponent discards a card.".into(),
            },
        ],
        oracle_text: "When Burglar Rat enters the battlefield, each opponent discards a card.".into(),
        ..Default::default()
    });

    // Changeling Outcast {B}
    // Creature — Shapeshifter 1/1
    // Changeling. Can't be blocked.
    db.insert(CardDef {
        id: ids::CHANGELING_OUTCAST,
        name: "Changeling Outcast".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Shapeshifter".into()), Subtype("Rat".into())],
        keywords: vec![KeywordAbility::CantBlock],
        power: Some(1),
        toughness: Some(1),
        oracle_text: "Changeling. Changeling Outcast can't be blocked.".into(),
        ..Default::default()
    });

    // Chittering Rats {1}{B}{B}
    // Creature — Rat 2/2
    // When Chittering Rats enters the battlefield, target opponent puts a card
    // from their hand on top of their library.
    db.insert(CardDef {
        id: ids::CHITTERING_RATS,
        name: "Chittering Rats".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into())],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::ReturnToTopOfLibrary { target: TargetSpec::Opponent },
                description: "When Chittering Rats enters the battlefield, target opponent puts a card from their hand on top of their library.".into(),
            },
        ],
        oracle_text: "When Chittering Rats enters the battlefield, target opponent puts a card from their hand on top of their library.".into(),
        ..Default::default()
    });

    // Chittering Witch {3}{B}
    // Creature — Human Witch 2/2
    // When Chittering Witch enters the battlefield, create a number of 1/1 black
    // Rat creature tokens equal to the number of opponents you have.
    // {1}{B}, Sacrifice a creature: Target creature gets -2/-2 until end of turn.
    db.insert(CardDef {
        id: ids::CHITTERING_WITCH,
        name: "Chittering Witch".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Human".into()), Subtype("Witch".into())],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::CreateToken(TokenDef {
                    name: "Rat".into(),
                    power: 1,
                    toughness: 1,
                    colors: vec![Color::Black],
                    subtypes: vec![Subtype("Rat".into())],
                    keywords: vec![],
                }),
                description: "When Chittering Witch enters the battlefield, create a number of 1/1 black Rat creature tokens equal to the number of opponents you have.".into(),
            },
        ],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(1, 0, 0, 1, 0, 0),
                requires_tap: false,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::Debuff { power: 2, toughness: 2, until_eot: true },
                description: "{1}{B}, Sacrifice a creature: Target creature gets -2/-2 until end of turn.".into(),
            },
        ],
        oracle_text: "When Chittering Witch enters the battlefield, create a number of 1/1 black Rat creature tokens equal to the number of opponents you have. {1}{B}, Sacrifice a creature: Target creature gets -2/-2 until end of turn.".into(),
        ..Default::default()
    });

    // Crypt Ghast {3}{B}
    // Creature — Spirit 2/2
    // Extort. Whenever you tap a Swamp for mana, add an additional {B}.
    db.insert(CardDef {
        id: ids::CRYPT_GHAST,
        name: "Crypt Ghast".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Spirit".into())],
        power: Some(2),
        toughness: Some(2),
        static_abilities: vec![StaticAbility::ManaFromSwampBonus],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::YouCastSpell,
                effect: Effect::Multiple(vec![
                    Effect::LoseLife { amount: 1, target: TargetSpec::Opponent },
                    Effect::GainLife { amount: 1 },
                ]),
                description: "Extort — Whenever you cast a spell, you may pay {W/B}. If you do, each opponent loses 1 life and you gain that much life.".into(),
            },
        ],
        oracle_text: "Extort. Whenever you tap a Swamp for mana, add an additional {B}.".into(),
        ..Default::default()
    });

    // Falkenrath Noble {3}{B}
    // Creature — Vampire Noble 2/2
    // Flying. Whenever Falkenrath Noble or another creature dies, target opponent
    // loses 1 life and you gain 1 life.
    db.insert(CardDef {
        id: ids::FALKENRATH_NOBLE,
        name: "Falkenrath Noble".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Vampire".into()), Subtype("Noble".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureDies,
                effect: Effect::Multiple(vec![
                    Effect::LoseLife { amount: 1, target: TargetSpec::Opponent },
                    Effect::GainLife { amount: 1 },
                ]),
                description: "Whenever Falkenrath Noble or another creature dies, target opponent loses 1 life and you gain 1 life.".into(),
            },
        ],
        oracle_text: "Flying. Whenever Falkenrath Noble or another creature dies, target opponent loses 1 life and you gain 1 life.".into(),
        ..Default::default()
    });

    // Gnat Miser {B}
    // Creature — Rat Shaman 1/1
    // Each opponent's maximum hand size is reduced by one.
    db.insert(CardDef {
        id: ids::GNAT_MISER,
        name: "Gnat Miser".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into()), Subtype("Shaman".into())],
        power: Some(1),
        toughness: Some(1),
        oracle_text: "Each opponent's maximum hand size is reduced by one.".into(),
        ..Default::default()
    });

    // Ink-Eyes, Servant of Oni {4}{B}{B}
    // Legendary Creature — Rat Ninja 5/4
    // Ninjutsu {3}{B}{B}. Whenever Ink-Eyes deals combat damage to a player, put
    // target creature card from that player's graveyard onto the battlefield under
    // your control. Regenerate {1}{B}.
    db.insert(CardDef {
        id: ids::INK_EYES_SERVANT_OF_ONI,
        name: "Ink-Eyes, Servant of Oni".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Rat".into()), Subtype("Ninja".into())],
        power: Some(5),
        toughness: Some(4),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::DealsCombatDamageToPlayer,
                effect: Effect::ReturnFromGraveyardToBattlefield { target: TargetSpec::Controller },
                description: "Whenever Ink-Eyes, Servant of Oni deals combat damage to a player, put target creature card from that player's graveyard onto the battlefield under your control.".into(),
            },
        ],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(1, 0, 0, 1, 0, 0),
                requires_tap: false,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::Unimplemented("Regenerate Ink-Eyes, Servant of Oni.".into()),
                description: "{1}{B}: Regenerate Ink-Eyes, Servant of Oni.".into(),
            },
        ],
        oracle_text: "Ninjutsu {3}{B}{B}. Whenever Ink-Eyes, Servant of Oni deals combat damage to a player, put target creature card from that player's graveyard onto the battlefield under your control. {1}{B}: Regenerate Ink-Eyes, Servant of Oni.".into(),
        ..Default::default()
    });

    // Karumonix, the Rat King {1}{B}{B}
    // Legendary Creature — Phyrexian Rat 3/3
    // Toxic 1. When Karumonix enters, look at the top five cards of your library.
    // You may reveal any number of Rat cards from among them and put them into your
    // hand. Put the rest on the bottom in a random order. Other Rats you control
    // have toxic 1.
    db.insert(CardDef {
        id: ids::KARUMONIX_THE_RAT_KING,
        name: "Karumonix, the Rat King".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Phyrexian".into()), Subtype("Rat".into())],
        power: Some(3),
        toughness: Some(3),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::DrawCards { count: 2 },
                description: "When Karumonix enters the battlefield, look at the top five cards of your library. Reveal Rat cards and put them into your hand, the rest go on the bottom.".into(),
            },
        ],
        oracle_text: "Toxic 1. When Karumonix, the Rat King enters the battlefield, look at the top five cards of your library. You may reveal any number of Rat cards from among them and put the revealed cards into your hand. Put the rest on the bottom of your library in a random order. Other Rats you control have toxic 1.".into(),
        ..Default::default()
    });

    // Lord Skitter, Sewer King {2}{B}
    // Legendary Creature — Rat Noble 3/3
    // When Lord Skitter enters the battlefield, exile target card from each
    // opponent's graveyard. At the beginning of combat on your turn, create a
    // 1/1 black Rat creature token.
    db.insert(CardDef {
        id: ids::LORD_SKITTER_SEWER_KING,
        name: "Lord Skitter, Sewer King".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Rat".into()), Subtype("Noble".into())],
        power: Some(3),
        toughness: Some(3),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::ExileFromGraveyard { target: TargetSpec::Opponent },
                description: "When Lord Skitter, Sewer King enters the battlefield, exile target card from each opponent's graveyard.".into(),
            },
            TriggeredAbility {
                trigger: TriggerCondition::BeginningOfCombat,
                effect: Effect::CreateToken(TokenDef {
                    name: "Rat".into(),
                    power: 1,
                    toughness: 1,
                    colors: vec![Color::Black],
                    subtypes: vec![Subtype("Rat".into())],
                    keywords: vec![],
                }),
                description: "At the beginning of combat on your turn, create a 1/1 black Rat creature token.".into(),
            },
        ],
        oracle_text: "When Lord Skitter, Sewer King enters the battlefield, exile target card from each opponent's graveyard. At the beginning of combat on your turn, create a 1/1 black Rat creature token.".into(),
        ..Default::default()
    });

    // Marrow-Gnawer {3}{B}{B}
    // Legendary Creature — Rat Rogue 2/3
    // All Rats have fear.
    // {T}, Sacrifice a Rat: Create X 1/1 black Rat creature tokens, where X is
    // the number of Rats you control.
    db.insert(CardDef {
        id: ids::MARROW_GNAWER,
        name: "Marrow-Gnawer".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Rat".into()), Subtype("Rogue".into())],
        power: Some(2),
        toughness: Some(3),
        static_abilities: vec![
            StaticAbility::GrantKeyword {
                keyword: KeywordAbility::Fear,
                affected: AffectedObjects::CreaturesControlledBy(0), // All Rats you control — simplified to all your creatures
            },
        ],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::zero(),
                requires_tap: true,
                sacrifice_cost: Some(SacrificeCost::CreatureWithSubtype(Subtype("Rat".into()))),
                life_cost: 0,
                effect: Effect::CreateTokens {
                    token: TokenDef {
                        name: "Rat".into(),
                        power: 1,
                        toughness: 1,
                        colors: vec![Color::Black],
                        subtypes: vec![Subtype("Rat".into())],
                        keywords: vec![],
                    },
                    count: DynamicValue::CreaturesControlled,
                },
                description: "{T}, Sacrifice a Rat: Create X 1/1 black Rat creature tokens, where X is the number of Rats you control.".into(),
            },
        ],
        oracle_text: "All Rats have fear. {T}, Sacrifice a Rat: Create X 1/1 black Rat creature tokens, where X is the number of Rats you control.".into(),
        ..Default::default()
    });

    // Mikaeus, the Unhallowed {3}{B}{B}{B}
    // Legendary Creature — Zombie Cleric 5/5
    // Intimidate. Whenever a Human deals damage to you, destroy it. Other
    // non-Human creatures you control get +1/+1 and have undying.
    db.insert(CardDef {
        id: ids::MIKAEUS_THE_UNHALLOWED,
        name: "Mikaeus, the Unhallowed".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 3, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Zombie".into()), Subtype("Cleric".into())],
        keywords: vec![KeywordAbility::Intimidate],
        power: Some(5),
        toughness: Some(5),
        static_abilities: vec![
            StaticAbility::Anthem {
                power: 1,
                toughness: 1,
                affected: AffectedObjects::OtherNonHumanCreaturesControlledBy(0),
            },
        ],
        oracle_text: "Intimidate. Whenever a Human deals damage to you, destroy it. Other non-Human creatures you control get +1/+1 and have undying.".into(),
        ..Default::default()
    });

    // Nashi, Moon Sage's Scion {2}{B}{B}
    // Legendary Creature — Rat Ninja 3/2
    // Ninjutsu {3}{B}. Whenever Nashi deals combat damage to a player, exile the
    // top card of each player's library. You may play those cards for as long as
    // they remain exiled, and you may spend mana as though it were mana of any type.
    db.insert(CardDef {
        id: ids::NASHI_MOON_SAGES_SCION,
        name: "Nashi, Moon Sage's Scion".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Rat".into()), Subtype("Ninja".into())],
        power: Some(3),
        toughness: Some(2),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::DealsCombatDamageToPlayer,
                effect: Effect::DrawCards { count: 1 },
                description: "Whenever Nashi, Moon Sage's Scion deals combat damage to a player, exile the top card of each player's library. You may play those cards. (Simplified to draw.)".into(),
            },
        ],
        oracle_text: "Ninjutsu {3}{B}. Whenever Nashi, Moon Sage's Scion deals combat damage to a player, exile the top card of each player's library. For each card exiled this way, you may play that card for as long as it remains exiled, and you may spend mana as though it were mana of any type to cast those spells.".into(),
        ..Default::default()
    });

    // Nezumi Bone-Reader {1}{B}
    // Creature — Rat Shaman 1/1
    // {B}, Sacrifice a creature: Target player discards a card.
    db.insert(CardDef {
        id: ids::NEZUMI_BONE_READER,
        name: "Nezumi Bone-Reader".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into()), Subtype("Shaman".into())],
        power: Some(1),
        toughness: Some(1),
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(0, 0, 0, 1, 0, 0),
                requires_tap: false,
                sacrifice_cost: Some(SacrificeCost::AnyCreature),
                life_cost: 0,
                effect: Effect::DiscardCards { count: 1, target: TargetSpec::Opponent },
                description: "{B}, Sacrifice a creature: Target player discards a card.".into(),
            },
        ],
        oracle_text: "{B}, Sacrifice a creature: Target player discards a card.".into(),
        ..Default::default()
    });

    // Nezumi Cutthroat {1}{B}
    // Creature — Rat Rogue 2/1
    // Fear. Nezumi Cutthroat can't block.
    db.insert(CardDef {
        id: ids::NEZUMI_CUTTHROAT,
        name: "Nezumi Cutthroat".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into()), Subtype("Rogue".into())],
        keywords: vec![KeywordAbility::Fear, KeywordAbility::CantBlock],
        power: Some(2),
        toughness: Some(1),
        oracle_text: "Fear. Nezumi Cutthroat can't block.".into(),
        ..Default::default()
    });

    // Nezumi Graverobber {1}{B}
    // Creature — Rat Rogue 2/1
    // {1}{B}: Exile target card from an opponent's graveyard. If no cards are in
    // that graveyard, flip Nezumi Graverobber.
    db.insert(CardDef {
        id: ids::NEZUMI_GRAVEROBBER,
        name: "Nezumi Graverobber".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into()), Subtype("Rogue".into())],
        power: Some(2),
        toughness: Some(1),
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(1, 0, 0, 1, 0, 0),
                requires_tap: false,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::ExileFromGraveyard { target: TargetSpec::Opponent },
                description: "{1}{B}: Exile target card from an opponent's graveyard. If no cards are in that graveyard, flip Nezumi Graverobber.".into(),
            },
        ],
        oracle_text: "{1}{B}: Exile target card from an opponent's graveyard. If no cards are in that graveyard, flip Nezumi Graverobber.".into(),
        ..Default::default()
    });

    // Nezumi Shortfang {1}{B}
    // Creature — Rat Rogue 1/1
    // {1}{B}, {T}: Target opponent discards a card. Then if that player has no
    // cards in hand, flip Nezumi Shortfang.
    db.insert(CardDef {
        id: ids::NEZUMI_SHORTFANG,
        name: "Nezumi Shortfang".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into()), Subtype("Rogue".into())],
        power: Some(1),
        toughness: Some(1),
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(1, 0, 0, 1, 0, 0),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::DiscardCards { count: 1, target: TargetSpec::Opponent },
                description: "{1}{B}, {T}: Target opponent discards a card. Then if that player has no cards in hand, flip Nezumi Shortfang.".into(),
            },
        ],
        oracle_text: "{1}{B}, {T}: Target opponent discards a card. Then if that player has no cards in hand, flip Nezumi Shortfang.".into(),
        ..Default::default()
    });

    // Nirkana Revenant {4}{B}{B}
    // Creature — Vampire Shade 4/4
    // Whenever you tap a Swamp for mana, add an additional {B}.
    // {B}: Nirkana Revenant gets +1/+1 until end of turn.
    db.insert(CardDef {
        id: ids::NIRKANA_REVENANT,
        name: "Nirkana Revenant".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Vampire".into()), Subtype("Shade".into())],
        power: Some(4),
        toughness: Some(4),
        static_abilities: vec![StaticAbility::ManaFromSwampBonus],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(0, 0, 0, 1, 0, 0),
                requires_tap: false,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::Buff { power: 1, toughness: 1, until_eot: true },
                description: "{B}: Nirkana Revenant gets +1/+1 until end of turn.".into(),
            },
        ],
        oracle_text: "Whenever you tap a Swamp for mana, add an additional {B}. {B}: Nirkana Revenant gets +1/+1 until end of turn.".into(),
        ..Default::default()
    });

    // Ogre Slumlord {3}{B}{B}
    // Creature — Ogre Rogue 3/3
    // Whenever another nontoken creature dies, you may create a 1/1 black Rat
    // creature token. Rats you control have deathtouch.
    db.insert(CardDef {
        id: ids::OGRE_SLUMLORD,
        name: "Ogre Slumlord".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Ogre".into()), Subtype("Rogue".into())],
        power: Some(3),
        toughness: Some(3),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureDies,
                effect: Effect::CreateToken(TokenDef {
                    name: "Rat".into(),
                    power: 1,
                    toughness: 1,
                    colors: vec![Color::Black],
                    subtypes: vec![Subtype("Rat".into())],
                    keywords: vec![],
                }),
                description: "Whenever another nontoken creature dies, you may create a 1/1 black Rat creature token.".into(),
            },
        ],
        static_abilities: vec![
            StaticAbility::GrantKeyword {
                keyword: KeywordAbility::Deathtouch,
                affected: AffectedObjects::OtherCreaturesWithSubtypeControlledBy("Rat".into(), 0),
            },
        ],
        oracle_text: "Whenever another nontoken creature dies, you may create a 1/1 black Rat creature token. Rats you control have deathtouch.".into(),
        ..Default::default()
    });

    // Pack Rat {1}{B}
    // Creature — Rat */*
    // Pack Rat's power and toughness are each equal to the number of Rats you control.
    // {2}{B}, Discard a card: Create a token that's a copy of Pack Rat.
    db.insert(CardDef {
        id: ids::PACK_RAT,
        name: "Pack Rat".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into())],
        power: Some(0),
        toughness: Some(0),
        dynamic_power: Some(DynamicValue::CreaturesControlled),
        dynamic_toughness: Some(DynamicValue::CreaturesControlled),
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(2, 0, 0, 1, 0, 0),
                requires_tap: false,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::CreateToken(TokenDef {
                    name: "Rat".into(),
                    power: 0,
                    toughness: 0,
                    colors: vec![Color::Black],
                    subtypes: vec![Subtype("Rat".into())],
                    keywords: vec![],
                }),
                description: "{2}{B}, Discard a card: Create a token that's a copy of Pack Rat.".into(),
            },
        ],
        oracle_text: "Pack Rat's power and toughness are each equal to the number of Rats you control. {2}{B}, Discard a card: Create a token that's a copy of Pack Rat.".into(),
        ..Default::default()
    });

    // Ratcatcher {4}{B}{B}
    // Creature — Ogre Rogue 4/4
    // Fear. At the beginning of your upkeep, you may search your library for a Rat
    // card, reveal it, put it into your hand, then shuffle.
    db.insert(CardDef {
        id: ids::RATCATCHER,
        name: "Ratcatcher".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Ogre".into()), Subtype("Rogue".into())],
        keywords: vec![KeywordAbility::Fear],
        power: Some(4),
        toughness: Some(4),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::BeginningOfUpkeep,
                effect: Effect::SearchLibrary { destination: ZoneType::Hand, subtype_filter: vec![] },
                description: "At the beginning of your upkeep, you may search your library for a Rat card, reveal it, put it into your hand, then shuffle.".into(),
            },
        ],
        oracle_text: "Fear. At the beginning of your upkeep, you may search your library for a Rat card, reveal it, put it into your hand, then shuffle.".into(),
        ..Default::default()
    });

    // Ravenous Rats {1}{B}
    // Creature — Rat 1/1
    // When Ravenous Rats enters the battlefield, target opponent discards a card.
    db.insert(CardDef {
        id: ids::RAVENOUS_RATS,
        name: "Ravenous Rats".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into())],
        power: Some(1),
        toughness: Some(1),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::DiscardCards { count: 1, target: TargetSpec::Opponent },
                description: "When Ravenous Rats enters the battlefield, target opponent discards a card.".into(),
            },
        ],
        oracle_text: "When Ravenous Rats enters the battlefield, target opponent discards a card.".into(),
        ..Default::default()
    });

    // Refurbished Familiar {2}{B}
    // Creature — Rat 2/1
    // Flying. When Refurbished Familiar enters the battlefield, if an artifact or
    // creature was put into your graveyard from the battlefield this turn, draw a card.
    db.insert(CardDef {
        id: ids::REFURBISHED_FAMILIAR,
        name: "Refurbished Familiar".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into())],
        keywords: vec![KeywordAbility::Flying],
        power: Some(2),
        toughness: Some(1),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::DrawCards { count: 1 },
                description: "When Refurbished Familiar enters the battlefield, if an artifact or creature was put into your graveyard from the battlefield this turn, draw a card.".into(),
            },
        ],
        oracle_text: "Flying. When Refurbished Familiar enters the battlefield, if an artifact or creature was put into your graveyard from the battlefield this turn, draw a card.".into(),
        ..Default::default()
    });

    // Roaming Throne {4}
    // Artifact Creature — Golem 4/4
    // Ward {2}. As Roaming Throne enters the battlefield, choose a creature type.
    // If a triggered ability of another creature you control with the chosen type triggers,
    // it triggers an additional time.
    db.insert(CardDef {
        id: ids::ROAMING_THRONE,
        name: "Roaming Throne".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact, CardType::Creature],
        subtypes: vec![Subtype("Golem".into())],
        power: Some(4),
        toughness: Some(4),
        oracle_text: "Ward {2}. As Roaming Throne enters the battlefield, choose a creature type. Roaming Throne is the chosen type in addition to its other types. If a triggered ability of another creature you control of the chosen type triggers, it triggers an additional time.".into(),
        ..Default::default()
    });

    // Skullsnatcher {1}{B}
    // Creature — Rat Ninja 2/1
    // Ninjutsu {B}. Whenever Skullsnatcher deals combat damage to a player, exile
    // up to two target cards from that player's graveyard.
    db.insert(CardDef {
        id: ids::SKULLSNATCHER,
        name: "Skullsnatcher".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into()), Subtype("Ninja".into())],
        power: Some(2),
        toughness: Some(1),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::DealsCombatDamageToPlayer,
                effect: Effect::ExileFromGraveyard { target: TargetSpec::Opponent },
                description: "Whenever Skullsnatcher deals combat damage to a player, exile up to two target cards from that player's graveyard.".into(),
            },
        ],
        oracle_text: "Ninjutsu {B}. Whenever Skullsnatcher deals combat damage to a player, exile up to two target cards from that player's graveyard.".into(),
        ..Default::default()
    });

    // Species Specialist {2}{B}{B}
    // Creature — Human Warrior 2/3
    // As Species Specialist enters the battlefield, choose a creature type.
    // Whenever a creature of the chosen type dies, you may draw a card.
    db.insert(CardDef {
        id: ids::SPECIES_SPECIALIST,
        name: "Species Specialist".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Human".into()), Subtype("Warrior".into())],
        power: Some(2),
        toughness: Some(3),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureDies,
                effect: Effect::DrawCards { count: 1 },
                description: "Whenever a creature of the chosen type dies, you may draw a card.".into(),
            },
        ],
        oracle_text: "As Species Specialist enters the battlefield, choose a creature type. Whenever a creature of the chosen type dies, you may draw a card.".into(),
        ..Default::default()
    });

    // Typhoid Rats {B}
    // Creature — Rat 1/1
    // Deathtouch.
    db.insert(CardDef {
        id: ids::TYPHOID_RATS,
        name: "Typhoid Rats".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into())],
        keywords: vec![KeywordAbility::Deathtouch],
        power: Some(1),
        toughness: Some(1),
        oracle_text: "Deathtouch".into(),
        ..Default::default()
    });

    // Valley Rotcaller {1}{B}
    // Creature — Rat Warlock 2/2
    // At the beginning of combat on your turn, each Rat you control gets +1/+0
    // and gains menace until end of turn.
    db.insert(CardDef {
        id: ids::VALLEY_ROTCALLER,
        name: "Valley Rotcaller".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Rat".into()), Subtype("Warlock".into())],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::BeginningOfCombat,
                effect: Effect::BuffOtherSubtype { subtype: "Rat".into(), amount: DynamicValue::Fixed(1), until_eot: true },
                description: "At the beginning of combat on your turn, each Rat you control gets +1/+0 and gains menace until end of turn.".into(),
            },
        ],
        oracle_text: "At the beginning of combat on your turn, each Rat you control gets +1/+0 and gains menace until end of turn.".into(),
        ..Default::default()
    });

    // Zulaport Cutthroat {1}{B}
    // Creature — Human Rogue Ally 1/1
    // Whenever Zulaport Cutthroat or another creature you control dies, each
    // opponent loses 1 life and you gain 1 life.
    db.insert(CardDef {
        id: ids::ZULAPORT_CUTTHROAT,
        name: "Zulaport Cutthroat".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Human".into()), Subtype("Rogue".into()), Subtype("Ally".into())],
        power: Some(1),
        toughness: Some(1),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureDies,
                effect: Effect::Multiple(vec![
                    Effect::LoseLife { amount: 1, target: TargetSpec::Opponent },
                    Effect::GainLife { amount: 1 },
                ]),
                description: "Whenever Zulaport Cutthroat or another creature you control dies, each opponent loses 1 life and you gain 1 life.".into(),
            },
        ],
        oracle_text: "Whenever Zulaport Cutthroat or another creature you control dies, each opponent loses 1 life and you gain 1 life.".into(),
        ..Default::default()
    });

    // --- Artifacts ---

    // Bontu's Monument {3}
    // Legendary Artifact
    // Black creature spells you cast cost {1} less to cast.
    // Whenever you cast a creature spell, each opponent loses 1 life and you gain 1 life.
    db.insert(CardDef {
        id: ids::BONTUS_MONUMENT,
        name: "Bontu's Monument".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::YouCastCreatureSpell,
                effect: Effect::Multiple(vec![
                    Effect::LoseLife { amount: 1, target: TargetSpec::Opponent },
                    Effect::GainLife { amount: 1 },
                ]),
                description: "Whenever you cast a creature spell, each opponent loses 1 life and you gain 1 life.".into(),
            },
        ],
        cost_reduction: Some(CostReduction {
            generic_reduction: 1,
            applies_to: CostReductionTarget::CreatureSpells,
        }),
        oracle_text: "Black creature spells you cast cost {1} less to cast. Whenever you cast a creature spell, each opponent loses 1 life and you gain 1 life.".into(),
        ..Default::default()
    });

    // Caged Sun {6}
    // Artifact
    // As Caged Sun enters the battlefield, choose a color.
    // Creatures you control of the chosen color get +1/+1.
    // Whenever you tap a land for mana of the chosen color, add one additional mana of that color.
    db.insert(CardDef {
        id: ids::CAGED_SUN,
        name: "Caged Sun".into(),
        mana_cost: Some(ManaCost::new(6, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        static_abilities: vec![
            StaticAbility::Anthem {
                power: 1,
                toughness: 1,
                affected: AffectedObjects::CreaturesControlledBy(0),
            },
        ],
        oracle_text: "As Caged Sun enters the battlefield, choose a color. Creatures you control of the chosen color get +1/+1. Whenever you tap a land for mana of the chosen color, add one additional mana of that color.".into(),
        ..Default::default()
    });

    // Coat of Arms {5}
    // Artifact
    // Each creature gets +1/+1 for each other creature on the battlefield that
    // shares at least one creature type with it.
    db.insert(CardDef {
        id: ids::COAT_OF_ARMS,
        name: "Coat of Arms".into(),
        mana_cost: Some(ManaCost::new(5, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        static_abilities: vec![
            // Approximate: in a tribal deck most creatures share types, so model as
            // a global +2/+2 anthem. The real effect scales with creature count.
            StaticAbility::Anthem {
                power: 2,
                toughness: 2,
                affected: AffectedObjects::AllCreatures,
            },
        ],
        oracle_text: "Each creature gets +1/+1 for each other creature on the battlefield that shares at least one creature type with it.".into(),
        ..Default::default()
    });

    // Cryptolith Fragment {3}
    // Artifact
    // {T}: Add one mana of any color. Each player loses 1 life.
    db.insert(CardDef {
        id: ids::CRYPTOLITH_FRAGMENT,
        name: "Cryptolith Fragment".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "{T}: Add one mana of any color. Each player loses 1 life. // Aurora of Emrakul.".into(),
        ..Default::default()
    });

    // Darksteel Ingot {3}
    // Artifact
    // Indestructible. {T}: Add one mana of any color.
    db.insert(CardDef {
        id: ids::DARKSTEEL_INGOT,
        name: "Darksteel Ingot".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        keywords: vec![KeywordAbility::Indestructible],
        mana_abilities: vec![ManaAbility::TapForAny],
        oracle_text: "Indestructible. {T}: Add one mana of any color.".into(),
        ..Default::default()
    });

    // Door of Destinies {4}
    // Artifact
    // As Door of Destinies enters the battlefield, choose a creature type.
    // Whenever you cast a spell of the chosen creature type, put a charge counter
    // on Door of Destinies. Creatures you control of the chosen type get +1/+1
    // for each charge counter on Door of Destinies.
    db.insert(CardDef {
        id: ids::DOOR_OF_DESTINIES,
        name: "Door of Destinies".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::YouCastSpell,
                effect: Effect::PutCounters { count: 1, target: TargetSpec::Controller },
                description: "Whenever you cast a spell of the chosen creature type, put a charge counter on Door of Destinies.".into(),
            },
        ],
        oracle_text: "As Door of Destinies enters the battlefield, choose a creature type. Whenever you cast a spell of the chosen creature type, put a charge counter on Door of Destinies. Creatures you control of the chosen type get +1/+1 for each charge counter on Door of Destinies.".into(),
        ..Default::default()
    });

    // Herald's Horn {3}
    // Artifact
    // As Herald's Horn enters the battlefield, choose a creature type.
    // Creature spells of the chosen type cost {1} less to cast.
    // At the beginning of your upkeep, reveal the top card. If it's a creature
    // of the chosen type, put it into your hand.
    db.insert(CardDef {
        id: ids::HERALDS_HORN,
        name: "Herald's Horn".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        cost_reduction: Some(CostReduction {
            generic_reduction: 1,
            applies_to: CostReductionTarget::CreatureSpells,
        }),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::BeginningOfUpkeep,
                effect: Effect::DrawCards { count: 1 },
                description: "At the beginning of your upkeep, reveal the top card of your library. If it's a creature of the chosen type, put it into your hand.".into(),
            },
        ],
        oracle_text: "As Herald's Horn enters the battlefield, choose a creature type. Creature spells of the chosen type cost {1} less to cast. At the beginning of your upkeep, reveal the top card of your library. If it's a creature card of the chosen type, put it into your hand. Otherwise, you may put it on the bottom of your library.".into(),
        ..Default::default()
    });

    // Jet Medallion {2}
    // Artifact
    // Black spells you cast cost {1} less to cast.
    db.insert(CardDef {
        id: ids::JET_MEDALLION,
        name: "Jet Medallion".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        cost_reduction: Some(CostReduction {
            generic_reduction: 1,
            applies_to: CostReductionTarget::AllSpells,
        }),
        oracle_text: "Black spells you cast cost {1} less to cast.".into(),
        ..Default::default()
    });

    // Nim Deathmantle {2}
    // Artifact — Equipment
    // Equipped creature gets +2/+2, is black, is a Zombie in addition to its other
    // types, and has intimidate. Whenever a nontoken creature is put into your
    // graveyard from the battlefield, you may pay {4}. If you do, return that card
    // to the battlefield and attach Nim Deathmantle to it. Equip {4}.
    db.insert(CardDef {
        id: ids::NIM_DEATHMANTLE,
        name: "Nim Deathmantle".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        subtypes: vec![Subtype("Equipment".into())],
        static_abilities: vec![
            StaticAbility::Anthem {
                power: 2,
                toughness: 2,
                affected: AffectedObjects::AttachedTo,
            },
            StaticAbility::GrantKeyword {
                keyword: KeywordAbility::Intimidate,
                affected: AffectedObjects::AttachedTo,
            },
        ],
        equip_cost: Some(ManaCost::new(4, 0, 0, 0, 0, 0)),
        oracle_text: "Equipped creature gets +2/+2, is black, is a Zombie in addition to its other types, and has intimidate. Whenever a nontoken creature is put into your graveyard from the battlefield, you may pay {4}. If you do, return that card to the battlefield and attach Nim Deathmantle to it. Equip {4}.".into(),
        ..Default::default()
    });

    // Semblance Anvil {3}
    // Artifact
    // Imprint — When Semblance Anvil enters the battlefield, you may exile a
    // nonland card from your hand. Spells you cast that share a card type with
    // the exiled card cost {2} less to cast.
    db.insert(CardDef {
        id: ids::SEMBLANCE_ANVIL,
        name: "Semblance Anvil".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::Unimplemented("You may exile a nonland card from your hand. Spells you cast that share a card type with the exiled card cost {2} less to cast.".into()),
                description: "When Semblance Anvil enters the battlefield, you may exile a nonland card from your hand.".into(),
            },
        ],
        oracle_text: "Imprint — When Semblance Anvil enters the battlefield, you may exile a nonland card from your hand. Spells you cast that share a card type with the exiled card cost {2} less to cast.".into(),
        ..Default::default()
    });

    // Skullclamp {1}
    // Artifact — Equipment
    // Equipped creature gets +1/-1.
    // Whenever equipped creature dies, draw two cards.
    // Equip {1}.
    db.insert(CardDef {
        id: ids::SKULLCLAMP,
        name: "Skullclamp".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        subtypes: vec![Subtype("Equipment".into())],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureYouControlDies,
                effect: Effect::DrawCards { count: 2 },
                description: "Whenever equipped creature dies, draw two cards.".into(),
            },
        ],
        static_abilities: vec![
            StaticAbility::Anthem {
                power: 1,
                toughness: -1,
                affected: AffectedObjects::AttachedTo,
            },
        ],
        equip_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        oracle_text: "Equipped creature gets +1/-1. Whenever equipped creature dies, draw two cards. Equip {1}.".into(),
        ..Default::default()
    });

    // Strionic Resonator {2}
    // Artifact
    // {2}, {T}: Copy target triggered ability you control. You may choose new
    // targets for the copy.
    db.insert(CardDef {
        id: ids::STRIONIC_RESONATOR,
        name: "Strionic Resonator".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(2, 0, 0, 0, 0, 0),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::Unimplemented("Copy target triggered ability you control. You may choose new targets for the copy.".into()),
                description: "{2}, {T}: Copy target triggered ability you control. You may choose new targets for the copy.".into(),
            },
        ],
        oracle_text: "{2}, {T}: Copy target triggered ability you control. You may choose new targets for the copy.".into(),
        ..Default::default()
    });

    // The Immortal Sun {6}
    // Legendary Artifact
    // Players can't activate planeswalker loyalty abilities.
    // At the beginning of your draw step, draw an additional card.
    // Spells you cast cost {1} less to cast.
    // Creatures you control get +1/+1.
    db.insert(CardDef {
        id: ids::THE_IMMORTAL_SUN,
        name: "The Immortal Sun".into(),
        mana_cost: Some(ManaCost::new(6, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::BeginningOfUpkeep,
                effect: Effect::DrawCards { count: 1 },
                description: "At the beginning of your draw step, draw an additional card.".into(),
            },
        ],
        static_abilities: vec![
            StaticAbility::Anthem {
                power: 1,
                toughness: 1,
                affected: AffectedObjects::CreaturesControlledBy(0),
            },
        ],
        oracle_text: "Players can't activate planeswalker loyalty abilities. At the beginning of your draw step, draw an additional card. Spells you cast cost {1} less to cast. Creatures you control get +1/+1.".into(),
        ..Default::default()
    });

    // Thornbite Staff {2}
    // Artifact — Equipment
    // Equipped creature has "{2}, {T}: This creature deals 1 damage to any target."
    // Whenever a creature dies, untap equipped creature. Equip {4}.
    db.insert(CardDef {
        id: ids::THORNBITE_STAFF,
        name: "Thornbite Staff".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        subtypes: vec![Subtype("Equipment".into())],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureDies,
                effect: Effect::UntapTarget { target: TargetSpec::Controller },
                description: "Whenever a creature dies, untap equipped creature.".into(),
            },
        ],
        equip_cost: Some(ManaCost::new(4, 0, 0, 0, 0, 0)),
        oracle_text: "Equipped creature has \"{2}, {T}: This creature deals 1 damage to any target.\" Whenever a creature dies, untap equipped creature. Equip {4}.".into(),
        ..Default::default()
    });

    // Thran Dynamo {4}
    // Artifact
    // {T}: Add {C}{C}{C}.
    db.insert(CardDef {
        id: ids::THRAN_DYNAMO,
        name: "Thran Dynamo".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForColorlessAmount(3)],
        oracle_text: "{T}: Add {C}{C}{C}.".into(),
        ..Default::default()
    });

    // Throne of the God-Pharaoh {2}
    // Legendary Artifact
    // At the beginning of your end step, each opponent loses life equal to the
    // number of tapped creatures you control.
    db.insert(CardDef {
        id: ids::THRONE_OF_THE_GOD_PHARAOH,
        name: "Throne of the God-Pharaoh".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EndOfTurn,
                effect: Effect::LoseDynamicLife { amount: DynamicValue::TappedCreaturesControlled, target: TargetSpec::Opponent },
                description: "At the beginning of your end step, each opponent loses life equal to the number of tapped creatures you control.".into(),
            },
        ],
        oracle_text: "At the beginning of your end step, each opponent loses life equal to the number of tapped creatures you control.".into(),
        ..Default::default()
    });

    // Urza's Incubator {3}
    // Artifact
    // As Urza's Incubator enters the battlefield, choose a creature type.
    // Creature spells of the chosen type cost {2} less to cast.
    db.insert(CardDef {
        id: ids::URZAS_INCUBATOR,
        name: "Urza's Incubator".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        cost_reduction: Some(CostReduction {
            generic_reduction: 2,
            applies_to: CostReductionTarget::CreatureSpells,
        }),
        oracle_text: "As Urza's Incubator enters the battlefield, choose a creature type. Creature spells of the chosen type cost {2} less to cast.".into(),
        ..Default::default()
    });

    // Vanquisher's Banner {5}
    // Artifact
    // As Vanquisher's Banner enters the battlefield, choose a creature type.
    // Creatures you control of the chosen type get +1/+1.
    // Whenever you cast a creature spell of the chosen type, draw a card.
    db.insert(CardDef {
        id: ids::VANQUISHERS_BANNER,
        name: "Vanquisher's Banner".into(),
        mana_cost: Some(ManaCost::new(5, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        static_abilities: vec![
            StaticAbility::Anthem {
                power: 1,
                toughness: 1,
                affected: AffectedObjects::CreaturesControlledBy(0),
            },
        ],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::YouCastCreatureSpell,
                effect: Effect::DrawCards { count: 1 },
                description: "Whenever you cast a creature spell of the chosen type, draw a card.".into(),
            },
        ],
        oracle_text: "As Vanquisher's Banner enters the battlefield, choose a creature type. Creatures you control of the chosen type get +1/+1. Whenever you cast a creature spell of the chosen type, draw a card.".into(),
        ..Default::default()
    });

    // --- Enchantments ---

    // Black Market {3}{B}{B}
    // Enchantment
    // Whenever a creature dies, put a charge counter on Black Market.
    // At the beginning of your precombat main phase, add {B} for each charge
    // counter on Black Market.
    db.insert(CardDef {
        id: ids::BLACK_MARKET,
        name: "Black Market".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureDies,
                effect: Effect::PutCounters { count: 1, target: TargetSpec::Controller },
                description: "Whenever a creature dies, put a charge counter on Black Market.".into(),
            },
            TriggeredAbility {
                trigger: TriggerCondition::BeginningOfUpkeep,
                effect: Effect::AddMana { color: Some(Color::Black), amount: 1 },
                description: "At the beginning of your precombat main phase, add {B} for each charge counter on Black Market.".into(),
            },
        ],
        oracle_text: "Whenever a creature dies, put a charge counter on Black Market. At the beginning of your precombat main phase, add {B} for each charge counter on Black Market.".into(),
        ..Default::default()
    });

    // Black Market Connections {2}{B}
    // Enchantment
    // At the beginning of your precombat main phase, choose one or more —
    // • Sell Contraband — Create a Treasure token. You lose 1 life.
    // • Buy Information — Draw a card. You lose 2 life.
    // • Hire a Mercenary — Create a 3/2 black Shapeshifter creature token with
    //   changeling. You lose 3 life.
    db.insert(CardDef {
        id: ids::BLACK_MARKET_CONNECTIONS,
        name: "Black Market Connections".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::BeginningOfUpkeep,
                effect: Effect::Multiple(vec![
                    Effect::DrawCards { count: 1 },
                    Effect::LoseLife { amount: 2, target: TargetSpec::Controller },
                ]),
                description: "At the beginning of your precombat main phase, choose one or more — Buy Information: Draw a card, lose 2 life. Sell Contraband: Create a Treasure, lose 1 life. Hire a Mercenary: Create a 3/2 changeling, lose 3 life.".into(),
            },
        ],
        oracle_text: "At the beginning of your precombat main phase, choose one or more — Sell Contraband: Create a Treasure token. You lose 1 life. Buy Information: Draw a card. You lose 2 life. Hire a Mercenary: Create a 3/2 black Shapeshifter creature token with changeling. You lose 3 life.".into(),
        ..Default::default()
    });

    // Dictate of Erebos {3}{B}{B}
    // Enchantment
    // Flash. Whenever a creature you control dies, each opponent sacrifices a creature.
    db.insert(CardDef {
        id: ids::DICTATE_OF_EREBOS,
        name: "Dictate of Erebos".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Enchantment],
        keywords: vec![KeywordAbility::Flash],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureYouControlDies,
                effect: Effect::SacrificeCreatures { count: 1, target: TargetSpec::Opponent },
                description: "Whenever a creature you control dies, each opponent sacrifices a creature.".into(),
            },
        ],
        oracle_text: "Flash. Whenever a creature you control dies, each opponent sacrifices a creature.".into(),
        ..Default::default()
    });

    // Grave Pact {1}{B}{B}{B}
    // Enchantment
    // Whenever a creature you control dies, each other player sacrifices a creature.
    db.insert(CardDef {
        id: ids::GRAVE_PACT,
        name: "Grave Pact".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 3, 0, 0)),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ACreatureYouControlDies,
                effect: Effect::SacrificeCreatures { count: 1, target: TargetSpec::Opponent },
                description: "Whenever a creature you control dies, each other player sacrifices a creature.".into(),
            },
        ],
        oracle_text: "Whenever a creature you control dies, each other player sacrifices a creature.".into(),
        ..Default::default()
    });

    // Phyrexian Arena {1}{B}{B}
    // Enchantment
    // At the beginning of your upkeep, you draw a card and you lose 1 life.
    db.insert(CardDef {
        id: ids::PHYREXIAN_ARENA,
        name: "Phyrexian Arena".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::BeginningOfUpkeep,
                effect: Effect::Multiple(vec![
                    Effect::DrawCards { count: 1 },
                    Effect::LoseLife { amount: 1, target: TargetSpec::Controller },
                ]),
                description: "At the beginning of your upkeep, you draw a card and you lose 1 life.".into(),
            },
        ],
        oracle_text: "At the beginning of your upkeep, you draw a card and you lose 1 life.".into(),
        ..Default::default()
    });

    // Phyrexian Reclamation {B}
    // Enchantment
    // {1}{B}, Pay 2 life: Return target creature card from your graveyard to your hand.
    db.insert(CardDef {
        id: ids::PHYREXIAN_RECLAMATION,
        name: "Phyrexian Reclamation".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Enchantment],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(1, 0, 0, 1, 0, 0),
                requires_tap: false,
                sacrifice_cost: None, life_cost: 2,
                effect: Effect::ReturnFromGraveyardToHand { target: TargetSpec::Controller },
                description: "{1}{B}, Pay 2 life: Return target creature card from your graveyard to your hand.".into(),
            },
        ],
        oracle_text: "{1}{B}, Pay 2 life: Return target creature card from your graveyard to your hand.".into(),
        ..Default::default()
    });

    // Bolas's Citadel {3}{B}{B}{B}
    // Legendary Artifact
    // You may look at the top card of your library at any time.
    // You may play lands and cast spells from the top of your library. Whenever
    // you cast a spell this way, pay life equal to its mana value rather than
    // paying its mana cost. {T}, Sacrifice ten nonland permanents: Each opponent
    // loses 10 life.
    db.insert(CardDef {
        id: ids::BOLASS_CITADEL,
        name: "Bolas's Citadel".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 3, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::zero(),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::LoseLife { amount: 10, target: TargetSpec::Opponent },
                description: "{T}, Sacrifice ten nonland permanents: Each opponent loses 10 life.".into(),
            },
        ],
        oracle_text: "You may look at the top card of your library at any time. You may play lands and cast spells from the top of your library. Whenever you cast a spell this way, pay life equal to its mana value rather than paying its mana cost. {T}, Sacrifice ten nonland permanents: Each opponent loses 10 life.".into(),
        ..Default::default()
    });

    // --- Lands ---

    // Cabal Coffers
    // Land
    // {2}, {T}: Add {B} for each Swamp you control.
    db.insert(CardDef {
        id: ids::CABAL_COFFERS,
        name: "Cabal Coffers".into(),
        card_types: vec![CardType::Land],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(2, 0, 0, 0, 0, 0),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::AddDynamicMana { color: Color::Black, count: DynamicValue::SwampsControlled },
                description: "{2}, {T}: Add {B} for each Swamp you control.".into(),
            },
        ],
        oracle_text: "{2}, {T}: Add {B} for each Swamp you control.".into(),
        ..Default::default()
    });

    // Castle Locthwain
    // Land
    // Castle Locthwain enters the battlefield tapped unless you control a Swamp.
    // {T}: Add {B}.
    // {1}{B}{B}, {T}: Draw a card, then you lose life equal to the number of
    // cards in your hand.
    db.insert(CardDef {
        id: ids::CASTLE_LOCTHWAIN,
        name: "Castle Locthwain".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Black)],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(1, 0, 0, 2, 0, 0),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::Multiple(vec![
                    Effect::DrawCards { count: 1 },
                    Effect::LoseDynamicLife { amount: DynamicValue::CardsInHand, target: TargetSpec::Controller },
                ]),
                description: "{1}{B}{B}, {T}: Draw a card, then you lose life equal to the number of cards in your hand.".into(),
            },
        ],
        oracle_text: "Castle Locthwain enters the battlefield tapped unless you control a Swamp. {T}: Add {B}. {1}{B}{B}, {T}: Draw a card, then you lose life equal to the number of cards in your hand.".into(),
        ..Default::default()
    });

    // Crypt of Agadeem
    // Land
    // Crypt of Agadeem enters the battlefield tapped.
    // {T}: Add {B}.
    // {2}, {T}: Add {B} for each black creature card in your graveyard.
    db.insert(CardDef {
        id: ids::CRYPT_OF_AGADEEM,
        name: "Crypt of Agadeem".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Black)],
        enters_tapped: true,
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(2, 0, 0, 0, 0, 0),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::AddDynamicMana { color: Color::Black, count: DynamicValue::CreaturesInGraveyard },
                description: "{2}, {T}: Add {B} for each black creature card in your graveyard.".into(),
            },
        ],
        oracle_text: "Crypt of Agadeem enters the battlefield tapped. {T}: Add {B}. {2}, {T}: Add {B} for each black creature card in your graveyard.".into(),
        ..Default::default()
    });

    // Mikokoro, Center of the Sea
    // Legendary Land
    // {T}: Add {C}.
    // {2}, {T}: Each player draws a card.
    db.insert(CardDef {
        id: ids::MIKOKORO_CENTER_OF_THE_SEA,
        name: "Mikokoro, Center of the Sea".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForColorless],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(2, 0, 0, 0, 0, 0),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::DrawCards { count: 1 },
                description: "{2}, {T}: Each player draws a card.".into(),
            },
        ],
        oracle_text: "{T}: Add {C}. {2}, {T}: Each player draws a card.".into(),
        ..Default::default()
    });

    // Nykthos, Shrine to Nyx
    // Legendary Land
    // {T}: Add {C}.
    // {2}, {T}: Choose a color. Add an amount of mana of that color equal to
    // your devotion to that color.
    db.insert(CardDef {
        id: ids::NYKTHOS_SHRINE_TO_NYX,
        name: "Nykthos, Shrine to Nyx".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForColorless],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(2, 0, 0, 0, 0, 0),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::AddDynamicMana { color: Color::Black, count: DynamicValue::DevotionTo(Color::Black) },
                description: "{2}, {T}: Choose a color. Add an amount of mana of that color equal to your devotion to that color.".into(),
            },
        ],
        oracle_text: "{T}: Add {C}. {2}, {T}: Choose a color. Add an amount of mana of that color equal to your devotion to that color.".into(),
        ..Default::default()
    });

    // Path of Ancestry
    // Land
    // Path of Ancestry enters the battlefield tapped.
    // {T}: Add one mana of any color in your commander's color identity.
    db.insert(CardDef {
        id: ids::PATH_OF_ANCESTRY,
        name: "Path of Ancestry".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForAny],
        enters_tapped: true,
        oracle_text: "Path of Ancestry enters the battlefield tapped. {T}: Add one mana of any color in your commander's color identity. When that mana is spent to cast a creature spell that shares a creature type with your commander, scry 1.".into(),
        ..Default::default()
    });

    // Swarmyard
    // Land
    // {T}: Add {C}.
    // {T}: Regenerate target Insect, Rat, Spider, or Squirrel.
    db.insert(CardDef {
        id: ids::SWARMYARD,
        name: "Swarmyard".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColorless],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::zero(),
                requires_tap: true,
                sacrifice_cost: None, life_cost: 0,
                effect: Effect::Unimplemented("Regenerate target Insect, Rat, Spider, or Squirrel.".into()),
                description: "{T}: Regenerate target Insect, Rat, Spider, or Squirrel.".into(),
            },
        ],
        oracle_text: "{T}: Add {C}. {T}: Regenerate target Insect, Rat, Spider, or Squirrel.".into(),
        ..Default::default()
    });

    // --- Instants / Sorceries ---

    // Chain Assassination {2}{B}
    // Sorcery
    // Destroy target creature. Draw a card.
    db.insert(CardDef {
        id: ids::CHAIN_ASSASSINATION,
        name: "Chain Assassination".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 1, 0, 0)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::Multiple(vec![
            Effect::DestroyTarget { target: TargetSpec::AnyCreature },
            Effect::DrawCards { count: 1 },
        ])),
        oracle_text: "Destroy target creature. Draw a card.".into(),
        ..Default::default()
    });

    // Living Death {3}{B}{B}
    // Sorcery
    // Each player exiles all creature cards from their graveyard, then sacrifices
    // all creatures they control, then puts all cards they exiled this way onto
    // the battlefield.
    db.insert(CardDef {
        id: ids::LIVING_DEATH,
        name: "Living Death".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 2, 0, 0)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::Unimplemented("Living Death: exile GY creatures, sacrifice all creatures, return exiled to battlefield.".into())),
        oracle_text: "Each player exiles all creature cards from their graveyard, then sacrifices all creatures they control, then puts all cards they exiled this way onto the battlefield.".into(),
        ..Default::default()
    });

    // =========================================================================
    // Flubs, the Fool Commander Deck cards
    // =========================================================================

    // --- Commander ---

    // Flubs, the Fool {G}{U}{R}
    // Legendary Creature — Frog Scout 0/5
    // You may play an additional land on each of your turns.
    // Whenever you play a land or cast a spell, draw a card if you have no cards in hand.
    // Otherwise, discard a card.
    db.insert(CardDef {
        id: ids::FLUBS_THE_FOOL,
        name: "Flubs, the Fool".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 1, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Frog".into()), Subtype("Scout".into())],
        power: Some(0),
        toughness: Some(5),
        static_abilities: vec![
            StaticAbility::ExtraLandDrops { count: 1 },
        ],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::YouPlayALand,
                effect: Effect::Conditional {
                    condition: Condition::HandIsEmpty,
                    if_true: Box::new(Effect::DrawCards { count: 1 }),
                    if_false: Some(Box::new(Effect::DiscardCards { count: 1, target: TargetSpec::Controller })),
                },
                description: "Whenever you play a land, draw a card if you have no cards in hand. Otherwise, discard a card.".into(),
            },
            TriggeredAbility {
                trigger: TriggerCondition::YouCastSpell,
                effect: Effect::Conditional {
                    condition: Condition::HandIsEmpty,
                    if_true: Box::new(Effect::DrawCards { count: 1 }),
                    if_false: Some(Box::new(Effect::DiscardCards { count: 1, target: TargetSpec::Controller })),
                },
                description: "Whenever you cast a spell, draw a card if you have no cards in hand. Otherwise, discard a card.".into(),
            },
        ],
        oracle_text: "You may play an additional land on each of your turns. Whenever you play a land or cast a spell, draw a card if you have no cards in hand. Otherwise, discard a card.".into(),
        ..Default::default()
    });

    // Abundance {2}{G}{G}
    // Enchantment
    // If you would draw a card, you may instead choose land or nonland. Reveal cards until
    // you reveal a card of the chosen kind. Put that card into your hand and the rest on bottom.
    // Implemented: Draw replacement — always chooses "land" (optimal in lands deck).
    db.insert(CardDef {
        id: ids::ABUNDANCE,
        name: "Abundance".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Enchantment],
        static_abilities: vec![StaticAbility::AbundanceReplacement],
        oracle_text: "If you would draw a card, you may instead choose land or nonland. Reveal cards from the top of your library until you reveal a card of the chosen kind. Put that card into your hand and put all other cards revealed this way on the bottom of your library in a random order.".into(),
        ..Default::default()
    });

    // Amphibian Downpour {2}{U}
    // Enchantment — Aura
    // Flash, Storm
    // Enchant creature. Enchanted creature loses all abilities, is a 1/1 blue Frog.
    // Primarily a removal spell — dead in goldfish (no opponent creatures to target).
    // Kept for completeness: if cast, acts as P/T setter + ability remover on enchanted creature.
    db.insert(CardDef {
        id: ids::AMPHIBIAN_DOWNPOUR,
        name: "Amphibian Downpour".into(),
        mana_cost: Some(ManaCost::new(2, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Enchantment],
        subtypes: vec![Subtype("Aura".into())],
        keywords: vec![KeywordAbility::Flash],
        oracle_text: "Flash. Storm. Enchant creature. Enchanted creature loses all abilities and is a blue Frog with base power and toughness 1/1.".into(),
        ..Default::default()
    });

    // Birgi, God of Storytelling {2}{R}
    // Legendary Creature — God 3/3
    // Whenever you cast a spell, add {R}.
    // Boast — {1}: Exile the top card of your library. You may play that card this turn.
    // Boast simplified as activated: {1}: Draw a card (approximates exile-play from top).
    db.insert(CardDef {
        id: ids::BIRGI_GOD_OF_STORYTELLING,
        name: "Birgi, God of Storytelling".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("God".into())],
        power: Some(3),
        toughness: Some(3),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::YouCastSpell,
                effect: Effect::AddMana { color: Some(Color::Red), amount: 1 },
                description: "Whenever you cast a spell, add {R}.".into(),
            },
        ],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(1, 0, 0, 0, 0, 0),
                requires_tap: false,
                sacrifice_cost: None,
                life_cost: 0,
                effect: Effect::DrawCards { count: 1 },
                description: "Boast — {1}: Exile the top card of your library. You may play that card this turn. (Simplified: draw a card.)".into(),
            },
        ],
        oracle_text: "Whenever you cast a spell, add {R}. Boast — {1}: Exile the top card of your library. You may play that card this turn.".into(),
        ..Default::default()
    });

    // Blackblade Reforged {2}
    // Legendary Artifact — Equipment
    // Equipped creature gets +1/+1 for each land you control.
    // Equip legendary creature {3}. Equip {7}.
    db.insert(CardDef {
        id: ids::BLACKBLADE_REFORGED,
        name: "Blackblade Reforged".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Equipment".into())],
        equip_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        oracle_text: "Equipped creature gets +1/+1 for each land you control. Equip legendary creature {3}. Equip {7}.".into(),
        ..Default::default()
    });

    // Bridge of Khazad-dûm (Ensnaring Bridge) {3}
    // Artifact
    // Creatures with power greater than the number of cards in your hand can't attack.
    db.insert(CardDef {
        id: ids::BRIDGE_OF_KHAZAD_DUM,
        name: "Bridge of Khazad-dum".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        static_abilities: vec![StaticAbility::EnsnaringBridge],
        oracle_text: "Creatures with power greater than the number of cards in your hand can't attack.".into(),
        ..Default::default()
    });

    // Bucklebury Ferry (Oboro, Palace in the Clouds)
    // Legendary Land
    // {T}: Add {U}. {1}: Return to owner's hand.
    db.insert(CardDef {
        id: ids::BUCKLEBURY_FERRY,
        name: "Bucklebury Ferry".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Blue)],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(1, 0, 0, 0, 0, 0),
                requires_tap: false,
                sacrifice_cost: None,
                life_cost: 0,
                effect: Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::NoTarget },
                description: "{1}: Return Bucklebury Ferry to its owner's hand.".into(),
            },
        ],
        oracle_text: "{T}: Add {U}. {1}: Return Bucklebury Ferry to its owner's hand.".into(),
        ..Default::default()
    });

    // Case of the Locked Hothouse {3}{G}
    // Enchantment — Case
    // You may play an additional land on each of your turns.
    // To solve — You control seven or more lands.
    // Solved — play lands/creatures/enchantments from top of library.
    // Simplified: Extra land drop enchantment
    db.insert(CardDef {
        id: ids::CASE_OF_THE_LOCKED_HOTHOUSE,
        name: "Case of the Locked Hothouse".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Enchantment],
        subtypes: vec![Subtype("Case".into())],
        static_abilities: vec![
            StaticAbility::ExtraLandDrops { count: 1 },
        ],
        oracle_text: "You may play an additional land on each of your turns. To solve — You control seven or more lands. Solved — You may look at the top card of your library any time, and you may play lands and cast creature and enchantment spells from the top of your library.".into(),
        ..Default::default()
    });

    // Chocobo Racetrack {3}{G}{G}
    // Artifact
    // Landfall — Create a 2/2 green Bird creature token with landfall +1/+0
    // Token defined as full CardDef (ids::CHOCOBO_TOKEN) with its own landfall trigger.
    db.insert(CardDef {
        id: ids::CHOCOBO_RACETRACK,
        name: "Chocobo Racetrack".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Artifact],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::CreateTokenFromDef { card_def_id: ids::CHOCOBO_TOKEN },
                description: "Landfall — Whenever a land you control enters, create a 2/2 green Bird creature token with landfall +1/+0.".into(),
            },
        ],
        oracle_text: "Landfall — Whenever a land you control enters, create a 2/2 green Bird creature token with \"Whenever a land you control enters, this creature gets +1/+0 until end of turn.\"".into(),
        ..Default::default()
    });

    // Codex Shredder {1}
    // Artifact
    // {T}: Target player mills a card.
    // {5}, {T}, Sacrifice: Return target card from your graveyard to your hand.
    db.insert(CardDef {
        id: ids::CODEX_SHREDDER,
        name: "Codex Shredder".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(0, 0, 0, 0, 0, 0),
                requires_tap: true,
                sacrifice_cost: None,
                life_cost: 0,
                effect: Effect::MillCards { count: 1, target: TargetSpec::Controller },
                description: "{T}: Target player mills a card.".into(),
            },
        ],
        oracle_text: "{T}: Target player mills a card. {5}, {T}, Sacrifice Codex Shredder: Return target card from your graveyard to your hand.".into(),
        ..Default::default()
    });

    // Conduit of Worlds {2}{G}{G}
    // Artifact
    // You may play lands from your graveyard.
    // Once each turn, you may cast a permanent from GY (exile a card from GY as extra cost).
    db.insert(CardDef {
        id: ids::CONDUIT_OF_WORLDS,
        name: "Conduit of Worlds".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Artifact],
        static_abilities: vec![StaticAbility::PlayLandsFromGraveyard],
        oracle_text: "You may play lands from your graveyard. Once during each of your turns, you may cast a permanent spell from your graveyard by paying its mana cost and exiling another permanent card from your graveyard rather than paying any additional costs.".into(),
        ..Default::default()
    });

    // Crucible of Worlds {3}
    // Artifact
    // You may play lands from your graveyard.
    db.insert(CardDef {
        id: ids::CRUCIBLE_OF_WORLDS,
        name: "Crucible of Worlds".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        static_abilities: vec![StaticAbility::PlayLandsFromGraveyard],
        oracle_text: "You may play lands from your graveyard.".into(),
        ..Default::default()
    });

    // Dragonback Assault {3}{G}{U}{R}
    // Enchantment
    // ETB: deals 3 damage to each creature and each planeswalker.
    // Landfall — create a 4/4 red Dragon creature token with flying.
    db.insert(CardDef {
        id: ids::DRAGONBACK_ASSAULT,
        name: "Dragonback Assault".into(),
        mana_cost: Some(ManaCost::new(3, 0, 1, 0, 1, 1)),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::DealDamage { amount: 3, target: TargetSpec::EachCreature },
                description: "When Dragonback Assault enters, it deals 3 damage to each creature and each planeswalker.".into(),
            },
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::CreateToken(TokenDef {
                    name: "Dragon".into(),
                    power: 4,
                    toughness: 4,
                    colors: vec![Color::Red],

                    subtypes: vec![Subtype("Dragon".into())],
                    keywords: vec![KeywordAbility::Flying],
                }),
                description: "Landfall — Whenever a land you control enters, create a 4/4 red Dragon creature token with flying.".into(),
            },
        ],
        oracle_text: "When Dragonback Assault enters, it deals 3 damage to each creature and each planeswalker. Landfall — Whenever a land you control enters, create a 4/4 red Dragon creature token with flying.".into(),
        ..Default::default()
    });

    // Druid Class {G}
    // Enchantment — Class
    // Level 1: Whenever a land enters under your control, gain 1 life.
    // Level 2 {1}{G}: You may play an additional land on each of your turns.
    // Level 3 {3}{G}{G}: Target land becomes creature with P/T = lands controlled.
    // Levels 1+2 always active (class leveling not in engine). Level 3 not modeled.
    db.insert(CardDef {
        id: ids::DRUID_CLASS,
        name: "Druid Class".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Enchantment],
        subtypes: vec![Subtype("Class".into())],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::GainLife { amount: 1 },
                description: "Whenever a land enters the battlefield under your control, you gain 1 life.".into(),
            },
        ],
        static_abilities: vec![
            StaticAbility::ExtraLandDrops { count: 1 },
        ],
        oracle_text: "Whenever a land enters the battlefield under your control, you gain 1 life. {1}{G}: Level 2 — You may play an additional land on each of your turns. {3}{G}{G}: Level 3 — When this Class becomes level 3, target land you control becomes a creature with haste and \"This creature's power and toughness are each equal to the number of lands you control.\"".into(),
        ..Default::default()
    });

    // Dryad of the Ilysian Grove {2}{G}
    // Enchantment Creature — Nymph Dryad 2/4
    // You may play an additional land on each of your turns.
    // Lands you control are every basic land type in addition to their other types.
    db.insert(CardDef {
        id: ids::DRYAD_OF_THE_ILYSIAN_GROVE,
        name: "Dryad of the Ilysian Grove".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Enchantment, CardType::Creature],
        subtypes: vec![Subtype("Nymph".into()), Subtype("Dryad".into())],
        power: Some(2),
        toughness: Some(4),
        static_abilities: vec![
            StaticAbility::ExtraLandDrops { count: 1 },
            StaticAbility::LandsAreAllBasicTypes,
        ],
        oracle_text: "You may play an additional land on each of your turns. Lands you control are every basic land type in addition to their other types.".into(),
        ..Default::default()
    });

    // Everflowing Chalice {0}
    // Artifact
    // Multikicker {2}. Enters with a charge counter for each kick.
    // {T}: Add {C} for each charge counter.
    // Simplified: costs {0}, taps for 1 {C}. Multikicker/charge counters not in engine.
    // In goldfish, casting for {0} and getting 1 colorless is a reasonable floor.
    db.insert(CardDef {
        id: ids::EVERFLOWING_CHALICE,
        name: "Everflowing Chalice".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForColorless],
        oracle_text: "Multikicker {2}. Everflowing Chalice enters with a charge counter on it for each time it was kicked. {T}: Add {C} for each charge counter on Everflowing Chalice.".into(),
        ..Default::default()
    });

    // Exploration {G}
    // Enchantment
    // You may play an additional land on each of your turns.
    db.insert(CardDef {
        id: ids::EXPLORATION,
        name: "Exploration".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Enchantment],
        static_abilities: vec![StaticAbility::ExtraLandDrops { count: 1 }],
        oracle_text: "You may play an additional land on each of your turns.".into(),
        ..Default::default()
    });

    // Explore {1}{G}
    // Sorcery
    // Draw a card. You may play an additional land this turn.
    db.insert(CardDef {
        id: ids::EXPLORE_CARD,
        name: "Explore".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::Multiple(vec![
            Effect::DrawCards { count: 1 },
            Effect::ExtraLandDrop,
        ])),
        oracle_text: "Draw a card. You may play an additional land this turn.".into(),
        ..Default::default()
    });

    // Fortune Teller's Talent {U}
    // Enchantment — Class
    // Level 1: You may look at the top card of your library.
    // Level 2 {3}{U}: Play cards from top of library while you've cast a spell this turn.
    // Level 3 {2}{U}: Spells from elsewhere cost {2} less.
    // Class leveling not in engine. Level 1 (look at top) is informational only.
    // Approximation: scry 1 on ETB to represent card selection value.
    db.insert(CardDef {
        id: ids::FORTUNE_TELLERS_TALENT,
        name: "Fortune Teller's Talent".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Enchantment],
        subtypes: vec![Subtype("Class".into())],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::Scry { count: 1 },
                description: "When this enters, scry 1 (approximates library-top knowledge).".into(),
            },
        ],
        oracle_text: "You may look at the top card of your library any time. {3}{U}: Level 2 — As long as you've cast a spell this turn, you may play cards from the top of your library. {2}{U}: Level 3 — Spells you cast from anywhere other than your hand cost {2} less to cast.".into(),
        ..Default::default()
    });

    // Glacierwood Siege {1}{G}{U}
    // Enchantment
    // Choose Temur or Sultai.
    // Temur — Whenever you cast instant/sorcery, mill 4.
    // Sultai — You may play lands from your graveyard.
    // Sultai is always optimal in goldfish (lands from graveyard > mill in a lands deck).
    db.insert(CardDef {
        id: ids::GLACIERWOOD_SIEGE,
        name: "Glacierwood Siege".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 1)),
        card_types: vec![CardType::Enchantment],
        static_abilities: vec![StaticAbility::PlayLandsFromGraveyard],
        oracle_text: "As Glacierwood Siege enters, choose Temur or Sultai. Temur — Whenever you cast an instant or sorcery spell, target player mills four cards. Sultai — You may play lands from your graveyard.".into(),
        ..Default::default()
    });

    // Gustha's Scepter {0}
    // Artifact
    // {T}: Exile a card from your hand face down.
    // {T}: Return a card exiled with Gustha's Scepter to hand.
    // When leaves: exiled cards go to graveyard (handled by move_object linked exile cleanup).
    db.insert(CardDef {
        id: ids::GUSTHAS_SCEPTER,
        name: "Gustha's Scepter".into(),
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(0, 0, 0, 0, 0, 0),
                requires_tap: true,
                sacrifice_cost: None,
                life_cost: 0,
                effect: Effect::ExileFromHandLinked,
                description: "{T}: Exile a card from your hand face down.".into(),
            },
            ActivatedAbility {
                cost: ManaCost::new(0, 0, 0, 0, 0, 0),
                requires_tap: true,
                sacrifice_cost: None,
                life_cost: 0,
                effect: Effect::ReturnLinkedExileToHand,
                description: "{T}: Return a card exiled with Gustha's Scepter to your hand.".into(),
            },
        ],
        oracle_text: "{T}: Exile a card from your hand face down. You may look at it for as long as it remains exiled. {T}: Return a card you own exiled with Gustha's Scepter to your hand.".into(),
        ..Default::default()
    });

    // Lantern of Insight {1}
    // Artifact
    // Each player plays with the top card of their library revealed.
    // {T}, Sacrifice: Target player shuffles.
    db.insert(CardDef {
        id: ids::LANTERN_OF_INSIGHT,
        name: "Lantern of Insight".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        oracle_text: "Each player plays with the top card of their library revealed. {T}, Sacrifice Lantern of Insight: Target player shuffles.".into(),
        ..Default::default()
    });

    // Lightning Greaves {2}
    // Artifact — Equipment
    // Equipped creature has haste and shroud.
    // Equip {0}.
    db.insert(CardDef {
        id: ids::LIGHTNING_GREAVES,
        name: "Lightning Greaves".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        subtypes: vec![Subtype("Equipment".into())],
        static_abilities: vec![
            StaticAbility::GrantKeyword {
                keyword: KeywordAbility::Haste,
                affected: AffectedObjects::AttachedTo,
            },
            StaticAbility::GrantKeyword {
                keyword: KeywordAbility::Shroud,
                affected: AffectedObjects::AttachedTo,
            },
        ],
        equip_cost: Some(ManaCost::new(0, 0, 0, 0, 0, 0)),
        oracle_text: "Equipped creature has haste and shroud. Equip {0}.".into(),
        ..Default::default()
    });

    // Lotus Cobra {1}{G}
    // Creature — Snake 2/1
    // Landfall — Whenever a land enters under your control, add one mana of any color.
    db.insert(CardDef {
        id: ids::LOTUS_COBRA,
        name: "Lotus Cobra".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Snake".into())],
        power: Some(2),
        toughness: Some(1),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::AddManaOfAnyColor { amount: 1 },
                description: "Landfall — Whenever a land enters the battlefield under your control, add one mana of any color.".into(),
            },
        ],
        oracle_text: "Landfall — Whenever a land enters the battlefield under your control, add one mana of any color.".into(),
        ..Default::default()
    });

    // Miku, Lost but Singing (Azusa, Lost but Seeking) {2}{G}
    // Legendary Creature — Human Monk 1/2
    // You may play two additional lands on each of your turns.
    db.insert(CardDef {
        id: ids::MIKU_LOST_BUT_SINGING,
        name: "Miku, Lost but Singing".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Human".into()), Subtype("Monk".into())],
        power: Some(1),
        toughness: Some(2),
        static_abilities: vec![StaticAbility::ExtraLandDrops { count: 2 }],
        oracle_text: "You may play two additional lands on each of your turns.".into(),
        ..Default::default()
    });

    // Monument to Endurance {3}
    // Artifact
    // Whenever you discard a card, choose one that hasn't been chosen this turn:
    // • Draw a card. • Create a Treasure token. • Each opponent loses 3 life.
    // Modal: goldfish AI picks first available mode (draw > treasure > drain).
    // "Hasn't been chosen this turn" restriction not modeled (minor).
    db.insert(CardDef {
        id: ids::MONUMENT_TO_ENDURANCE,
        name: "Monument to Endurance".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::YouDiscardACard,
                effect: Effect::Modal {
                    choices: vec![
                        Effect::DrawCards { count: 1 },
                        Effect::CreatePredefinedToken { token_type: PredefinedToken::Treasure, count: 1 },
                        Effect::EachOpponentLosesLife { amount: 3 },
                    ],
                    choose_count: 1,
                },
                description: "Whenever you discard a card, choose one — • Draw a card. • Create a Treasure token. • Each opponent loses 3 life.".into(),
            },
        ],
        oracle_text: "Whenever you discard a card, choose one that hasn't been chosen this turn — • Draw a card. • Create a Treasure token. • Each opponent loses 3 life.".into(),
        ..Default::default()
    });

    // Mystic Sanctuary
    // Land — Island
    // Mystic Sanctuary enters tapped unless you control three or more other Islands.
    // When Mystic Sanctuary enters, you may put target instant or sorcery from your graveyard
    // on top of your library.
    db.insert(CardDef {
        id: ids::MYSTIC_SANCTUARY,
        name: "Mystic Sanctuary".into(),
        card_types: vec![CardType::Land],
        subtypes: vec![Subtype("Island".into())],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Blue)],
        enters_tapped: true,
        oracle_text: "Mystic Sanctuary enters tapped unless you control three or more other Islands. When Mystic Sanctuary enters, if you control three or more other Islands, you may put target instant or sorcery card from your graveyard on top of your library.".into(),
        ..Default::default()
    });

    // Null Brooch {2}
    // Artifact
    // {2}, Discard your hand, {T}: Counter target noncreature spell.
    // Dead in goldfish but still needs to be in the deck.
    db.insert(CardDef {
        id: ids::NULL_BROOCH,
        name: "Null Brooch".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        oracle_text: "{2}, {T}, Discard your hand: Counter target noncreature spell.".into(),
        ..Default::default()
    });

    // Otherworldly Gaze {U}
    // Instant
    // Surveil 3.
    // Flashback {1}{U}.
    db.insert(CardDef {
        id: ids::OTHERWORLDLY_GAZE,
        name: "Otherworldly Gaze".into(),
        mana_cost: Some(ManaCost::new(0, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Surveil { count: 3 }),
        flashback_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        oracle_text: "Surveil 3. Flashback {1}{U}.".into(),
        ..Default::default()
    });

    // Phial of Galadriel {3}
    // Legendary Artifact
    // If you would draw a card while you have no cards in hand, draw two cards instead.
    // If you would gain life while you have 5 or less life, gain twice that much instead.
    // {T}: Add one mana of any color.
    // Draw doubling implemented via PhialDrawDoubler. Life doubling not modeled.
    db.insert(CardDef {
        id: ids::PHIAL_OF_GALADRIEL,
        name: "Phial of Galadriel".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForAny],
        static_abilities: vec![StaticAbility::PhialDrawDoubler],
        oracle_text: "If you would draw a card while you have no cards in hand, draw two cards instead. If you would gain life while you have 5 or less life, you gain twice that much life instead. {T}: Add one mana of any color.".into(),
        ..Default::default()
    });

    // Prismatic Omen {1}{G}
    // Enchantment
    // Lands you control are every basic land type in addition to their other types.
    db.insert(CardDef {
        id: ids::PRISMATIC_OMEN,
        name: "Prismatic Omen".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Enchantment],
        static_abilities: vec![StaticAbility::LandsAreAllBasicTypes],
        oracle_text: "Lands you control are every basic land type in addition to their other types.".into(),
        ..Default::default()
    });

    // Renfield, Delusional Minion (Eruth, Tormented Prophet) {1}{U}{R}
    // Legendary Creature — Human Wizard 2/4
    // If you would draw a card, exile the top two cards of your library instead.
    // You may play those cards this turn.
    // Implemented: draw replacement that draws 2 cards instead of 1 (approximates
    // the exile-and-play-this-turn effect as doubled card flow).
    db.insert(CardDef {
        id: ids::RENFIELD_DELUSIONAL_MINION,
        name: "Renfield, Delusional Minion".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Human".into()), Subtype("Wizard".into())],
        power: Some(2),
        toughness: Some(4),
        static_abilities: vec![StaticAbility::RenfieldDrawReplacement],
        oracle_text: "If you would draw a card, exile the top two cards of your library instead. You may play those cards this turn.".into(),
        ..Default::default()
    });

    // Sabotender {1}{R}
    // Creature — Plant 2/1
    // Reach
    // Landfall — deals 1 damage to each opponent.
    db.insert(CardDef {
        id: ids::SABOTENDER,
        name: "Sabotender".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 1, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Plant".into())],
        power: Some(2),
        toughness: Some(1),
        keywords: vec![KeywordAbility::Reach],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::EachOpponentLosesLife { amount: 1 },
                description: "Landfall — Whenever a land you control enters, Sabotender deals 1 damage to each opponent.".into(),
            },
        ],
        oracle_text: "Reach. Landfall — Whenever a land you control enters, Sabotender deals 1 damage to each opponent.".into(),
        ..Default::default()
    });

    // Saw It Coming {1}{U}{U}
    // Instant
    // Counter target spell. Foretell {1}{U}.
    db.insert(CardDef {
        id: ids::SAW_IT_COMING,
        name: "Saw It Coming".into(),
        mana_cost: Some(ManaCost::new(1, 0, 2, 0, 0, 0)),
        card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Counter { target: TargetSpec::AnySpell }),
        oracle_text: "Counter target spell. Foretell {1}{U}.".into(),
        ..Default::default()
    });

    // Scute Swarm {2}{G}
    // Creature — Insect 1/1
    // Landfall — Create a 1/1 green Insect creature token.
    // If you control 6+ lands, create a token copy of Scute Swarm instead.
    // Implemented: Conditional checks land count. Copy tokens inherit landfall trigger,
    // enabling exponential growth at 6+ lands.
    db.insert(CardDef {
        id: ids::SCUTE_SWARM,
        name: "Scute Swarm".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Insect".into())],
        power: Some(1),
        toughness: Some(1),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::Conditional {
                    condition: Condition::ControlNOrMore { count: 6, card_type: CardType::Land },
                    if_true: Box::new(Effect::CreateTokenCopyOfSource),
                    if_false: Some(Box::new(Effect::CreateToken(TokenDef {
                        name: "Insect".into(),
                        power: 1,
                        toughness: 1,
                        colors: vec![Color::Green],
                        subtypes: vec![Subtype("Insect".into())],
                        keywords: vec![],
                    }))),
                },
                description: "Landfall — Whenever a land enters the battlefield under your control, create a 1/1 green Insect creature token. If you control six or more lands, create a token that's a copy of Scute Swarm instead.".into(),
            },
        ],
        oracle_text: "Landfall — Whenever a land enters the battlefield under your control, create a 1/1 green Insect creature token. If you control six or more lands, create a token that's a copy of Scute Swarm instead.".into(),
        ..Default::default()
    });

    // Six {1}{G}
    // Legendary Creature — Treefolk 1/4
    // Reach
    // Lands you control have "{T}: Mill a card."
    // Whenever a land card is put into your graveyard, you may exile it.
    // You may play lands exiled with Six.
    // Implemented: {T}: Mill (on Six itself), landfall triggers mill (approximates
    // lands having mill), PlayLandsFromGraveyard (approximates the exile-and-play loop).
    db.insert(CardDef {
        id: ids::SIX,
        name: "Six".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Treefolk".into())],
        power: Some(1),
        toughness: Some(4),
        keywords: vec![KeywordAbility::Reach],
        activated_abilities: vec![
            ActivatedAbility {
                cost: ManaCost::new(0, 0, 0, 0, 0, 0),
                requires_tap: true,
                sacrifice_cost: None,
                life_cost: 0,
                effect: Effect::MillCards { count: 1, target: TargetSpec::Controller },
                description: "{T}: Mill a card (approximates lands having mill).".into(),
            },
        ],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::MillCards { count: 1, target: TargetSpec::Controller },
                description: "Landfall — Mill a card (approximates lands having mill tap ability).".into(),
            },
        ],
        static_abilities: vec![StaticAbility::PlayLandsFromGraveyard],
        oracle_text: "Reach. Lands you control have \"{T}: Mill a card.\" Whenever a land card is put into your graveyard from anywhere, you may exile it. You may play land cards exiled with Six.".into(),
        ..Default::default()
    });

    // Swiftfoot Boots {2}
    // Artifact — Equipment
    // Equipped creature has hexproof and haste.
    // Equip {1}.
    db.insert(CardDef {
        id: ids::SWIFTFOOT_BOOTS,
        name: "Swiftfoot Boots".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Artifact],
        subtypes: vec![Subtype("Equipment".into())],
        static_abilities: vec![
            StaticAbility::GrantKeyword {
                keyword: KeywordAbility::Hexproof,
                affected: AffectedObjects::AttachedTo,
            },
            StaticAbility::GrantKeyword {
                keyword: KeywordAbility::Haste,
                affected: AffectedObjects::AttachedTo,
            },
        ],
        equip_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        oracle_text: "Equipped creature has hexproof and haste. Equip {1}.".into(),
        ..Default::default()
    });

    // Thespian's Stage
    // Land
    // {T}: Add {C}.
    // {2}, {T}: Thespian's Stage becomes a copy of target land, except it has this ability.
    // Copy effect not in engine. Kept as colorless land with tap for {C}.
    // In the Flubs deck, primarily used to copy Valakut for extra damage triggers.
    db.insert(CardDef {
        id: ids::THESPIANS_STAGE,
        name: "Thespian's Stage".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColorless],
        oracle_text: "{T}: Add {C}. {2}, {T}: Thespian's Stage becomes a copy of target land, except it has this ability.".into(),
        ..Default::default()
    });

    // Tifa Lockhart {1}{G}
    // Legendary Creature — Human Monk 1/2
    // Trample
    // Landfall — Double Tifa's power until end of turn.
    db.insert(CardDef {
        id: ids::TIFA_LOCKHART,
        name: "Tifa Lockhart".into(),
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Human".into()), Subtype("Monk".into())],
        power: Some(1),
        toughness: Some(2),
        keywords: vec![KeywordAbility::Trample],
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::DoublePowerUntilEOT { target: TargetSpec::NoTarget },
                description: "Landfall — Whenever a land you control enters, double Tifa Lockhart's power until end of turn.".into(),
            },
        ],
        oracle_text: "Trample. Landfall — Whenever a land you control enters, double Tifa Lockhart's power until end of turn.".into(),
        ..Default::default()
    });

    // Tireless Provisioner {2}{G}
    // Creature — Elf Scout 3/2
    // Landfall — create a Treasure token or a Food token.
    // Simplified: Landfall → create Treasure
    db.insert(CardDef {
        id: ids::TIRELESS_PROVISIONER,
        name: "Tireless Provisioner".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Elf".into()), Subtype("Scout".into())],
        power: Some(3),
        toughness: Some(2),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::CreatePredefinedToken { token_type: PredefinedToken::Treasure, count: 1 },
                description: "Landfall — Whenever a land enters the battlefield under your control, create a Treasure token.".into(),
            },
        ],
        oracle_text: "Landfall — Whenever a land enters the battlefield under your control, create a Treasure token or a Food token.".into(),
        ..Default::default()
    });

    // Valakut, the Molten Pinnacle
    // Land
    // Valakut enters tapped. {T}: Add {R}.
    // Whenever a Mountain enters under your control, if you control 5+ other Mountains,
    // Valakut deals 3 damage to any target.
    // Mountain check: requires 6+ total lands (approximates "5 other Mountains" since
    // Prismatic Omen / Dryad makes all lands Mountains in this deck).
    db.insert(CardDef {
        id: ids::VALAKUT_THE_MOLTEN_PINNACLE,
        name: "Valakut, the Molten Pinnacle".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Red)],
        enters_tapped: true,
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::Conditional {
                    condition: Condition::ControlNOrMore { count: 6, card_type: CardType::Land },
                    if_true: Box::new(Effect::DealDamage { amount: 3, target: TargetSpec::Opponent }),
                    if_false: None,
                },
                description: "Whenever a Mountain enters the battlefield under your control, if you control at least five other Mountains, Valakut deals 3 damage to any target.".into(),
            },
        ],
        oracle_text: "Valakut, the Molten Pinnacle enters tapped. {T}: Add {R}. Whenever a Mountain enters the battlefield under your control, if you control at least five other Mountains, Valakut, the Molten Pinnacle deals 3 damage to any target.".into(),
        ..Default::default()
    });

    // Vesuva
    // Land
    // As Vesuva enters, you may choose a land on the battlefield. If you do, Vesuva enters
    // as a copy of that land.
    // Copy effect not in engine. Enters tapped, taps for colorless.
    // In this deck, primarily used to copy Valakut for extra damage triggers.
    db.insert(CardDef {
        id: ids::VESUVA,
        name: "Vesuva".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColorless],
        enters_tapped: true,
        oracle_text: "As Vesuva enters, you may choose a land on the battlefield. If you do, Vesuva enters as a copy of that land.".into(),
        ..Default::default()
    });

    // Walk-In Closet {2}{G}
    // Enchantment — Room
    // You may play lands from your graveyard.
    // (Forgotten Cellar door: {3}{G}{G} to unlock)
    // Simplified: PlayLandsFromGraveyard enchantment
    db.insert(CardDef {
        id: ids::WALK_IN_CLOSET,
        name: "Walk-In Closet".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Enchantment],
        subtypes: vec![Subtype("Room".into())],
        static_abilities: vec![StaticAbility::PlayLandsFromGraveyard],
        oracle_text: "You may play lands from your graveyard. // Forgotten Cellar {3}{G}{G}: When you unlock this door, you may cast spells from your graveyard this turn.".into(),
        ..Default::default()
    });

    // Wayward Swordtooth {2}{G}
    // Creature — Dinosaur 5/5
    // Ascend. You may play an additional land on each of your turns.
    // Wayward Swordtooth can't attack or block unless you have the city's blessing (10+ perms).
    // Implemented: CantBlock always, but can attack once you control 10+ permanents.
    // Without city's blessing, acts as a wall; with it, becomes a 5/5 attacker.
    db.insert(CardDef {
        id: ids::WAYWARD_SWORDTOOTH,
        name: "Wayward Swordtooth".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 1)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Dinosaur".into())],
        power: Some(5),
        toughness: Some(5),
        keywords: vec![KeywordAbility::CantBlock],
        static_abilities: vec![StaticAbility::ExtraLandDrops { count: 1 }],
        oracle_text: "Ascend. You may play an additional land on each of your turns. Wayward Swordtooth can't attack or block unless you have the city's blessing.".into(),
        ..Default::default()
    });

    // Wonder {3}{U}
    // Creature — Incarnation 2/2
    // Flying
    // As long as Wonder is in your graveyard and you control an Island,
    // creatures you control have flying.
    // Implemented: Flying on the creature + WonderInGraveyard grants flying to all
    // creatures from the graveyard (Island check assumed true with Prismatic Omen/Dryad).
    db.insert(CardDef {
        id: ids::WONDER,
        name: "Wonder".into(),
        mana_cost: Some(ManaCost::new(3, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Incarnation".into())],
        power: Some(2),
        toughness: Some(2),
        keywords: vec![KeywordAbility::Flying],
        static_abilities: vec![StaticAbility::WonderInGraveyard],
        oracle_text: "Flying. As long as Wonder is in your graveyard and you control an Island, creatures you control have flying.".into(),
        ..Default::default()
    });

    // Yavimaya, Cradle of Growth
    // Legendary Land
    // Each land is a Forest in addition to its other land types.
    db.insert(CardDef {
        id: ids::YAVIMAYA_CRADLE_OF_GROWTH,
        name: "Yavimaya, Cradle of Growth".into(),
        card_types: vec![CardType::Land],
        supertypes: vec![Supertype::Legendary],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Green)],
        static_abilities: vec![StaticAbility::AllLandsAreForests],
        oracle_text: "Each land is a Forest in addition to its other land types.".into(),
        ..Default::default()
    });

    // --- Flubs Deck Token Definitions ---
    // These are full CardDefs (not simple TokenDefs) because they need triggered abilities.

    // Chocobo Token 2/2 green Bird with landfall +1/+0
    // Created by Chocobo Racetrack. Has its own landfall trigger.
    db.insert(CardDef {
        id: ids::CHOCOBO_TOKEN,
        name: "Chocobo".into(),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Bird".into())],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![
            TriggeredAbility {
                trigger: TriggerCondition::ALandYouControlEnters,
                effect: Effect::Buff { power: 1, toughness: 0, until_eot: true },
                description: "Whenever a land you control enters, this creature gets +1/+0 until end of turn.".into(),
            },
        ],
        oracle_text: "2/2 green Bird token. Whenever a land you control enters, this creature gets +1/+0 until end of turn.".into(),
        ..Default::default()
    });

    db
}

/// Build a minimal 15-card mono-red burn deck for MCCFR training.
///
/// Phase 1B training scenario: Mountains + Lightning Bolts.
/// Games end in 3-5 turns. Reachable info sets fit in memory (<100K entries).
pub fn mini_red_burn() -> Vec<CardId> {
    let mut deck = Vec::new();
    // 8 Mountains
    for _ in 0..8 {
        deck.push(ids::MOUNTAIN);
    }
    // 4 Lightning Bolt
    for _ in 0..4 {
        deck.push(ids::LIGHTNING_BOLT);
    }
    // 3 Shock (additional burn)
    for _ in 0..3 {
        deck.push(ids::SHOCK);
    }
    assert_eq!(deck.len(), 15);
    deck
}

/// Build a minimal 15-card mono-red creature deck for MCCFR training.
///
/// Phase 1B training scenario: Mountains + Grey Ogres.
/// Simpler creature-based strategy against the burn deck.
pub fn mini_red_creatures() -> Vec<CardId> {
    let mut deck = Vec::new();
    // 8 Mountains
    for _ in 0..8 {
        deck.push(ids::MOUNTAIN);
    }
    // 4 Grey Ogre (2/2 for 2R)
    for _ in 0..4 {
        deck.push(ids::GREY_OGRE);
    }
    // 3 Goblin Guide (2/2 haste for R)
    for _ in 0..3 {
        deck.push(ids::GOBLIN_GUIDE);
    }
    assert_eq!(deck.len(), 15);
    deck
}

/// Build a mono-red aggro decklist (burn).
pub fn red_aggro_deck() -> Vec<CardId> {
    let mut deck = Vec::new();

    // 20 Mountains
    for _ in 0..20 {
        deck.push(ids::MOUNTAIN);
    }

    // 4 Goblin Guide
    for _ in 0..4 {
        deck.push(ids::GOBLIN_GUIDE);
    }

    // 4 Monastery Swiftspear
    for _ in 0..4 {
        deck.push(ids::MONASTERY_SWIFTSPEAR);
    }

    // 4 Lightning Bolt
    for _ in 0..4 {
        deck.push(ids::LIGHTNING_BOLT);
    }

    // 4 Lava Spike
    for _ in 0..4 {
        deck.push(ids::LAVA_SPIKE);
    }

    // 4 Shock
    for _ in 0..4 {
        deck.push(ids::SHOCK);
    }

    // 4 Rift Bolt
    for _ in 0..4 {
        deck.push(ids::RIFT_BOLT);
    }

    // 4 Grey Ogre (filler 3-drop creature)
    for _ in 0..4 {
        deck.push(ids::GREY_OGRE);
    }

    // 12 more Mountains to hit 60
    for _ in 0..12 {
        deck.push(ids::MOUNTAIN);
    }

    assert_eq!(deck.len(), 60);
    deck
}

/// Build a mono-green stompy decklist.
pub fn green_stompy_deck() -> Vec<CardId> {
    let mut deck = Vec::new();

    // 20 Forests
    for _ in 0..20 {
        deck.push(ids::FOREST);
    }

    // 4 Llanowar Elves
    for _ in 0..4 {
        deck.push(ids::LLANOWAR_ELVES);
    }

    // 4 Elvish Mystic
    for _ in 0..4 {
        deck.push(ids::ELVISH_MYSTIC);
    }

    // 4 Kalonian Tusker
    for _ in 0..4 {
        deck.push(ids::KALONIAN_TUSKER);
    }

    // 4 Grizzly Bears
    for _ in 0..4 {
        deck.push(ids::GRIZZLY_BEARS);
    }

    // 4 Leatherback Baloth
    for _ in 0..4 {
        deck.push(ids::LEATHERBACK_BALOTH);
    }

    // 4 Giant Growth
    for _ in 0..4 {
        deck.push(ids::GIANT_GROWTH);
    }

    // 8 more Forests + 8 more Bears to hit 60
    for _ in 0..8 {
        deck.push(ids::FOREST);
    }
    for _ in 0..8 {
        deck.push(ids::GRIZZLY_BEARS);
    }

    assert_eq!(deck.len(), 60);
    deck
}

// ==========================================================================
// Commander decks (100-card singleton, except basics)
// ==========================================================================

/// Build a mono-white Commander deck with Brimaz, King of Oreskos as
/// the commander. Uses all available white cards from the pool plus
/// colorless artifacts, padded with Plains to reach 100 cards.
///
/// Returns (deck, commander_id).
pub fn brimaz_commander_deck() -> (Vec<CardId>, CardId) {
    let commander = ids::BRIMAZ_KING;
    let mut deck = Vec::new();

    // Commander (included in the deck list, will be extracted by setup)
    deck.push(commander);

    // White creatures (1 each, singleton)
    deck.push(ids::SAVANNAH_LIONS);
    deck.push(ids::MOTHER_OF_RUNES);
    deck.push(ids::ELITE_VANGUARD);
    deck.push(ids::WHITE_KNIGHT);
    deck.push(ids::LEONIN_SKYHUNTER);
    deck.push(ids::BANESLAYER_ANGEL);
    deck.push(ids::SERRA_ANGEL);
    deck.push(ids::THALIA_GUARDIAN);
    deck.push(ids::SOLDIER_OF_THE_PANTHEON);
    deck.push(ids::HERO_OF_BLADEHOLD);
    deck.push(ids::PRECINCT_CAPTAIN);
    deck.push(ids::BLADE_SPLICER);

    // White spells
    deck.push(ids::SWORDS_TO_PLOWSHARES);
    deck.push(ids::PATH_TO_EXILE);
    deck.push(ids::WRATH_OF_GOD);
    deck.push(ids::DAY_OF_JUDGMENT);
    deck.push(ids::OBLIVION_RING);
    deck.push(ids::DISENCHANT);

    // White enchantments
    deck.push(ids::GLORIOUS_ANTHEM);
    deck.push(ids::HONOR_OF_THE_PURE);
    deck.push(ids::CRUSADE);

    // Colorless artifacts
    deck.push(ids::SOL_RING);
    deck.push(ids::SIGNAL_PEST);
    deck.push(ids::STEEL_OVERSEER);

    // Fill the rest with Plains to reach 100
    let nonland_count = deck.len();
    for _ in 0..(100 - nonland_count) {
        deck.push(ids::PLAINS);
    }

    assert_eq!(deck.len(), 100);
    (deck, commander)
}

/// Build a mono-green Commander deck with Thrun, the Last Troll as
/// the commander. Uses all available green cards plus colorless
/// artifacts, padded with Forests to reach 100 cards.
///
/// Returns (deck, commander_id).
pub fn thrun_commander_deck() -> (Vec<CardId>, CardId) {
    let commander = ids::THRUN_LAST_TROLL;
    let mut deck = Vec::new();

    // Commander
    deck.push(commander);

    // Green creatures
    deck.push(ids::LLANOWAR_ELVES);
    deck.push(ids::ELVISH_MYSTIC);
    deck.push(ids::GRIZZLY_BEARS);
    deck.push(ids::KALONIAN_TUSKER);
    deck.push(ids::LEATHERBACK_BALOTH);
    deck.push(ids::ELVISH_VISIONARY);
    deck.push(ids::TARMOGOYF);
    deck.push(ids::SCAVENGING_OOZE);
    deck.push(ids::STRANGLEROOT_GEIST);
    deck.push(ids::EXPERIMENT_ONE);
    deck.push(ids::DRYAD_MILITANT);
    deck.push(ids::RANCOR_BEAST);

    // Green spells
    deck.push(ids::GIANT_GROWTH);
    deck.push(ids::RANCOR);
    deck.push(ids::VINES_OF_VASTWOOD);
    deck.push(ids::COLLECTED_COMPANY);

    // Green enchantment
    deck.push(ids::GAEA_ANTHEM);

    // Colorless artifacts
    deck.push(ids::SOL_RING);
    deck.push(ids::SIGNAL_PEST);
    deck.push(ids::STEEL_OVERSEER);

    // Fill the rest with Forests to reach 100
    let nonland_count = deck.len();
    for _ in 0..(100 - nonland_count) {
        deck.push(ids::FOREST);
    }

    assert_eq!(deck.len(), 100);
    (deck, commander)
}

/// Build the Kinnan, Bonder Prodigy Commander deck (100-card singleton).
///
/// Returns (deck, commander_id, tutor_targets).
/// Tutor targets are the priority cards that tutors should search for,
/// ordered by strategic importance (Basalt Monolith first for infinite mana).
pub fn kinnan_commander_deck() -> (Vec<CardId>, CardId, Vec<CardId>) {
    let commander = ids::KINNAN_BONDER_PRODIGY;
    let mut deck = Vec::new();

    // Commander
    deck.push(commander);

    // Lands
    deck.push(ids::ANCIENT_TOMB);
    deck.push(ids::BOSEIJU_WHO_ENDURES);
    deck.push(ids::BREEDING_POOL);
    deck.push(ids::COMMAND_TOWER);
    deck.push(ids::FLOODED_STRAND);
    deck.push(ids::GAEAS_CRADLE);
    deck.push(ids::GEMSTONE_CAVERNS);
    deck.push(ids::INVENTORS_FAIR);
    deck.push(ids::ISLAND);
    deck.push(ids::MINAMO_SCHOOL);
    deck.push(ids::MISTY_RAINFOREST);
    deck.push(ids::MISTRISE_VILLAGE);
    deck.push(ids::OTAWARA_SOARING_CITY);
    deck.push(ids::SEAT_OF_THE_SYNOD);
    deck.push(ids::SHIFTING_WOODLAND);
    deck.push(ids::SNOW_COVERED_FOREST);
    deck.push(ids::SNOW_COVERED_ISLAND);
    deck.push(ids::TREASURE_VAULT);
    deck.push(ids::TREE_OF_TALES);
    deck.push(ids::TROPICAL_ISLAND);
    deck.push(ids::WATERLOGGED_GROVE);
    deck.push(ids::WINDSWEPT_HEATH);
    deck.push(ids::YAVIMAYA_COAST);
    deck.push(ids::FOREST);

    // Mana artifacts
    deck.push(ids::ARCANE_SIGNET);
    deck.push(ids::BASALT_MONOLITH);
    deck.push(ids::CHROME_MOX);
    deck.push(ids::FELLWAR_STONE);
    deck.push(ids::GRIM_MONOLITH);
    deck.push(ids::LOTUS_PETAL);
    deck.push(ids::MANA_VAULT);
    deck.push(ids::MOX_AMBER);
    deck.push(ids::MOX_DIAMOND);
    deck.push(ids::MOX_OPAL);
    deck.push(ids::MOONSILVER_KEY);
    deck.push(ids::SIMIC_SIGNET);
    deck.push(ids::SOL_RING);
    deck.push(ids::SPRINGLEAF_DRUM);
    deck.push(ids::TALISMAN_OF_CURIOSITY);

    // Other artifacts
    deck.push(ids::AGATHAS_SOUL_CAULDRON);
    deck.push(ids::THE_ONE_RING);
    deck.push(ids::MIRAGE_MIRROR);
    deck.push(ids::WALKING_BALLISTA);

    // Creatures
    deck.push(ids::BIRDS_OF_PARADISE);
    deck.push(ids::FYNDHORN_ELVES);
    deck.push(ids::LLANOWAR_ELVES);
    deck.push(ids::ELVISH_MYSTIC);
    deck.push(ids::DELIGHTED_HALFLING);
    deck.push(ids::BADGERMOLE_CUB);
    deck.push(ids::CLEVER_IMPERSONATOR);
    deck.push(ids::COLOSSAL_SKYTURTLE);
    deck.push(ids::CONSECRATED_SPHINX);
    deck.push(ids::DRIFT_OF_PHANTASMS);
    deck.push(ids::ELVISH_SPIRIT_GUIDE);
    deck.push(ids::ENDURANCE);
    deck.push(ids::ENDURING_VITALITY);
    deck.push(ids::FAERIE_MASTERMIND);
    deck.push(ids::FLESH_DUPLICATE);
    deck.push(ids::HIGH_FAE_TRICKSTER);
    deck.push(ids::HULLBREAKER_HORROR);
    deck.push(ids::MOCKINGBIRD);
    deck.push(ids::NEZAHAL_PRIMAL_TIDE);
    deck.push(ids::NYXBLOOM_ANCIENT);
    deck.push(ids::PHYREXIAN_METAMORPH);
    deck.push(ids::SEEDBORN_MUSE);
    deck.push(ids::THRASIOS_TRITON_HERO);
    deck.push(ids::TIDESPOUT_TYRANT);
    deck.push(ids::TROPHY_MAGE);
    deck.push(ids::WAN_SHI_TONG);
    deck.push(ids::WANDERING_ARCHAIC);

    // Instants
    deck.push(ids::AN_OFFER_YOU_CANT_REFUSE);
    deck.push(ids::CHORD_OF_CALLING);
    deck.push(ids::CROP_ROTATION);
    deck.push(ids::CYCLONIC_RIFT);
    deck.push(ids::FIERCE_GUARDIANSHIP);
    deck.push(ids::FLUSTERSTORM);
    deck.push(ids::FORCE_OF_NEGATION);
    deck.push(ids::FORCE_OF_WILL);
    deck.push(ids::INTO_THE_FLOOD_MAW);
    deck.push(ids::MENTAL_MISSTEP);
    deck.push(ids::MINDBREAK_TRAP);
    deck.push(ids::MYSTICAL_TUTOR);
    deck.push(ids::NOXIOUS_REVIVAL);
    deck.push(ids::SWAN_SONG);
    deck.push(ids::VEIL_OF_SUMMER);
    deck.push(ids::WHIR_OF_INVENTION);
    deck.push(ids::WORLDLY_TUTOR);
    deck.push(ids::MUDDLE_THE_MIXTURE);

    // Sorceries
    deck.push(ids::FINALE_OF_DEVASTATION);
    deck.push(ids::GREEN_SUNS_ZENITH);
    deck.push(ids::NATURES_RHYTHM);

    // Enchantments
    deck.push(ids::MYSTIC_REMORA);
    deck.push(ids::RHYSTIC_STUDY);

    // Planeswalker
    deck.push(ids::TEZZERET_THE_SEEKER);

    // DFC / Battle cards
    deck.push(ids::BRIDGEWORKS_BATTLE);
    deck.push(ids::DISCIPLE_OF_FREYALISE);
    deck.push(ids::HYDROELECTRIC_SPECIMEN);
    deck.push(ids::INVASION_OF_IKORIA);
    deck.push(ids::SINK_INTO_STUPOR);

    assert_eq!(deck.len(), 100);

    // Priority tutor targets: combo pieces and high-impact cards.
    // Basalt Monolith is #1 — infinite mana with Kinnan's static ability.
    let tutor_targets = vec![
        ids::BASALT_MONOLITH,      // Infinite mana combo with Kinnan
        ids::WALKING_BALLISTA,     // Wins instantly with infinite mana
        ids::GRIM_MONOLITH,        // Fast mana, doubles with Kinnan
        ids::THRASIOS_TRITON_HERO, // Mana sink to win with infinite mana
        ids::TIDESPOUT_TYRANT,     // Bounce engine, wins with infinite mana
        ids::HULLBREAKER_HORROR,   // Bounce engine, flash
        ids::CONSECRATED_SPHINX,   // Card advantage engine
        ids::NYXBLOOM_ANCIENT,     // Mana tripler
        ids::SEEDBORN_MUSE,        // Untap engine for Kinnan activations
        ids::SOL_RING,             // Best mana rock
        ids::TROPHY_MAGE,          // Tutors for Basalt Monolith (CMC 3)
        ids::THE_ONE_RING,         // Protection + card draw
    ];

    (deck, commander, tutor_targets)
}

/// Build the Ashcoat of the Shadow Swarm Commander deck (100-card singleton).
/// Mono-black Rat tribal.
///
/// Returns (deck, commander_id).
pub fn ashcoat_commander_deck() -> (Vec<CardId>, CardId) {
    let commander = ids::ASHCOAT_OF_THE_SHADOW_SWARM;
    let mut deck = Vec::new();

    // Commander
    deck.push(commander);

    // Creatures (34 total including commander)
    deck.push(ids::ASSASSIN_INITIATE);
    deck.push(ids::AYARA_FIRST_OF_LOCTHWAIN);
    deck.push(ids::BLOOD_ARTIST);
    deck.push(ids::BLOODLINE_PRETENDER);
    deck.push(ids::BURGLAR_RAT);
    deck.push(ids::CHANGELING_OUTCAST);
    deck.push(ids::CHITTERING_RATS);
    deck.push(ids::CHITTERING_WITCH);
    deck.push(ids::CRYPT_GHAST);
    deck.push(ids::FALKENRATH_NOBLE);
    deck.push(ids::GNAT_MISER);
    deck.push(ids::INK_EYES_SERVANT_OF_ONI);
    deck.push(ids::KARUMONIX_THE_RAT_KING);
    deck.push(ids::LORD_SKITTER_SEWER_KING);
    deck.push(ids::MARROW_GNAWER);
    deck.push(ids::MIKAEUS_THE_UNHALLOWED);
    deck.push(ids::NASHI_MOON_SAGES_SCION);
    deck.push(ids::NEZUMI_BONE_READER);
    deck.push(ids::NEZUMI_CUTTHROAT);
    deck.push(ids::NEZUMI_GRAVEROBBER);
    deck.push(ids::NEZUMI_SHORTFANG);
    deck.push(ids::NIRKANA_REVENANT);
    deck.push(ids::OGRE_SLUMLORD);
    deck.push(ids::PACK_RAT);
    deck.push(ids::RATCATCHER);
    deck.push(ids::RAVENOUS_RATS);
    deck.push(ids::REFURBISHED_FAMILIAR);
    deck.push(ids::ROAMING_THRONE);
    deck.push(ids::SKULLSNATCHER);
    deck.push(ids::SPECIES_SPECIALIST);
    deck.push(ids::TYPHOID_RATS);
    deck.push(ids::VALLEY_ROTCALLER);
    deck.push(ids::ZULAPORT_CUTTHROAT);

    // Artifacts (19 total)
    deck.push(ids::BONTUS_MONUMENT);
    deck.push(ids::CAGED_SUN);
    deck.push(ids::COAT_OF_ARMS);
    deck.push(ids::CRYPTOLITH_FRAGMENT);
    deck.push(ids::DARKSTEEL_INGOT);
    deck.push(ids::DOOR_OF_DESTINIES);
    deck.push(ids::HERALDS_HORN);
    deck.push(ids::JET_MEDALLION);
    deck.push(ids::NIM_DEATHMANTLE);
    deck.push(ids::SEMBLANCE_ANVIL);
    deck.push(ids::SKULLCLAMP);
    deck.push(ids::SOL_RING);
    deck.push(ids::STRIONIC_RESONATOR);
    deck.push(ids::THE_IMMORTAL_SUN);
    deck.push(ids::THORNBITE_STAFF);
    deck.push(ids::THRAN_DYNAMO);
    deck.push(ids::THRONE_OF_THE_GOD_PHARAOH);
    deck.push(ids::URZAS_INCUBATOR);
    deck.push(ids::VANQUISHERS_BANNER);

    // Enchantments (7 total)
    deck.push(ids::BLACK_MARKET);
    deck.push(ids::BLACK_MARKET_CONNECTIONS);
    deck.push(ids::DICTATE_OF_EREBOS);
    deck.push(ids::GRAVE_PACT);
    deck.push(ids::PHYREXIAN_ARENA);
    deck.push(ids::PHYREXIAN_RECLAMATION);
    deck.push(ids::BOLASS_CITADEL);

    // Lands (7 nonbasic)
    deck.push(ids::CABAL_COFFERS);
    deck.push(ids::CASTLE_LOCTHWAIN);
    deck.push(ids::CRYPT_OF_AGADEEM);
    deck.push(ids::MIKOKORO_CENTER_OF_THE_SEA);
    deck.push(ids::NYKTHOS_SHRINE_TO_NYX);
    deck.push(ids::PATH_OF_ANCESTRY);
    deck.push(ids::SWARMYARD);

    // Instants / Sorceries (3 total)
    deck.push(ids::DARK_RITUAL);
    deck.push(ids::CHAIN_ASSASSINATION);
    deck.push(ids::LIVING_DEATH);

    // Mana artifacts already in database
    deck.push(ids::LOTUS_PETAL);

    // 29 Swamps
    for _ in 0..29 {
        deck.push(ids::SWAMP);
    }

    assert_eq!(deck.len(), 100);
    (deck, commander)
}

/// Build a Flubs, the Fool Commander deck (Temur Lands).
/// Commander: Flubs, the Fool
pub fn flubs_commander_deck() -> (Vec<CardId>, CardId) {
    let commander = ids::FLUBS_THE_FOOL;
    let mut deck = Vec::new();

    // Creatures (14)
    deck.push(ids::BIRGI_GOD_OF_STORYTELLING);
    deck.push(ids::DRYAD_OF_THE_ILYSIAN_GROVE);
    deck.push(ids::LOTUS_COBRA);
    deck.push(ids::MIKU_LOST_BUT_SINGING);
    deck.push(ids::RENFIELD_DELUSIONAL_MINION);
    deck.push(ids::SABOTENDER);
    deck.push(ids::SCUTE_SWARM);
    deck.push(ids::SIX);
    deck.push(ids::TIFA_LOCKHART);
    deck.push(ids::TIRELESS_PROVISIONER);
    deck.push(ids::WAYWARD_SWORDTOOTH);
    deck.push(ids::WONDER);

    // Enchantments (10)
    deck.push(ids::ABUNDANCE);
    deck.push(ids::AMPHIBIAN_DOWNPOUR);
    deck.push(ids::CASE_OF_THE_LOCKED_HOTHOUSE);
    deck.push(ids::DRAGONBACK_ASSAULT);
    deck.push(ids::DRUID_CLASS);
    deck.push(ids::EXPLORATION);
    deck.push(ids::FORTUNE_TELLERS_TALENT);
    deck.push(ids::GLACIERWOOD_SIEGE);
    deck.push(ids::PRISMATIC_OMEN);
    deck.push(ids::WALK_IN_CLOSET);

    // Artifacts (13)
    deck.push(ids::ARCANE_SIGNET);
    deck.push(ids::BLACKBLADE_REFORGED);
    deck.push(ids::BRIDGE_OF_KHAZAD_DUM);
    deck.push(ids::CHOCOBO_RACETRACK);
    deck.push(ids::CODEX_SHREDDER);
    deck.push(ids::CONDUIT_OF_WORLDS);
    deck.push(ids::CRUCIBLE_OF_WORLDS);
    deck.push(ids::EVERFLOWING_CHALICE);
    deck.push(ids::GUSTHAS_SCEPTER);
    deck.push(ids::LANTERN_OF_INSIGHT);
    deck.push(ids::LIGHTNING_GREAVES);
    deck.push(ids::MONUMENT_TO_ENDURANCE);
    deck.push(ids::NULL_BROOCH);
    deck.push(ids::PHIAL_OF_GALADRIEL);
    deck.push(ids::SWIFTFOOT_BOOTS);

    // Instants / Sorceries (5)
    deck.push(ids::BRAINSTORM);
    deck.push(ids::EXPLORE_CARD);
    deck.push(ids::OTHERWORLDLY_GAZE);
    deck.push(ids::SAW_IT_COMING);

    // Lands (36)
    deck.push(ids::BOSEIJU_WHO_ENDURES);
    deck.push(ids::BUCKLEBURY_FERRY);
    deck.push(ids::COMMAND_TOWER);
    deck.push(ids::MISTRISE_VILLAGE);
    deck.push(ids::MYSTIC_SANCTUARY);
    deck.push(ids::OTAWARA_SOARING_CITY);
    deck.push(ids::SHIFTING_WOODLAND);
    deck.push(ids::THESPIANS_STAGE);
    deck.push(ids::VALAKUT_THE_MOLTEN_PINNACLE);
    deck.push(ids::VESUVA);
    deck.push(ids::YAVIMAYA_CRADLE_OF_GROWTH);

    // 3 Forest
    for _ in 0..3 {
        deck.push(ids::FOREST);
    }
    // 9 Island
    for _ in 0..9 {
        deck.push(ids::ISLAND);
    }
    // 8 Mountain
    for _ in 0..8 {
        deck.push(ids::MOUNTAIN);
    }

    // Pad to 99 with additional basics
    while deck.len() < 99 {
        deck.push(ids::FOREST);
    }

    assert_eq!(deck.len(), 99, "Flubs deck should have 99 cards (commander is separate)");
    (deck, commander)
}
