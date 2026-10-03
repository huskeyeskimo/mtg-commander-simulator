use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{CardDef, CardInstance, CardType, KeywordAbility, ObjectId, Subtype, ZoneType};
use crate::mana::Color;

/// A dynamic value that can be computed at runtime from the game state.
/// Used for creatures with variable power/toughness like Tarmogoyf
/// ("*/1+* where * is the number of card types in all graveyards")
/// or Maro ("*/*, where * is the number of cards in your hand").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DynamicValue {
    /// Number of cards in the controller's hand (e.g., Maro).
    CardsInHand,
    /// Number of creatures the controller controls (e.g., Coat of Arms).
    CreaturesControlled,
    /// Number of card types among all graveyards (e.g., Tarmogoyf).
    CardTypesInGraveyards,
    /// Total power among creatures the controller controls.
    TotalPowerControlled,
    /// Number of Swamps the controller controls (e.g., Cabal Coffers).
    SwampsControlled,
    /// Devotion to a color — count of mana symbols of that color among
    /// permanents the controller controls (e.g., Nykthos, Shrine to Nyx).
    DevotionTo(Color),
    /// Number of creature cards in the controller's graveyard (e.g., Crypt of Agadeem).
    CreaturesInGraveyard,
    /// Number of creatures with a specific subtype the controller controls
    /// (e.g., "number of Rats you control").
    CreaturesWithSubtype(String),
    /// A fixed value (for testing / compatibility).
    Fixed(i32),
    /// Number of lands the controller controls.
    LandsControlled,
    /// Number of charge counters on the source permanent.
    ChargeCountersOnSource,
    /// Number of tapped creatures the controller controls (e.g., Throne of the God-Pharaoh).
    TappedCreaturesControlled,
    /// All permanents of a subtype controlled by the ability controller.
    PermanentsWithSubtype(String),
    /// All battlefield permanents of a subtype, regardless of controller.
    AllPermanentsWithSubtype(String),
}

/// Extra context from the game state for evaluating `DynamicValue` variants
/// that need data beyond the battlefield (hand size, graveyards).
pub struct DynamicContext {
    /// Number of cards in the controller's hand.
    pub hand_size: usize,
    /// All card type sets across all graveyards, flattened for counting
    /// distinct types. Each inner Vec is one card's types.
    pub graveyard_card_types: Vec<Vec<CardType>>,
    /// Number of creature cards in the controller's graveyard.
    pub creatures_in_graveyard: usize,
}

impl DynamicValue {
    /// Evaluate this dynamic value in context.
    pub fn evaluate<'a, F>(
        &self,
        controller: usize,
        objects: &HashMap<ObjectId, CardInstance>,
        battlefield: &[ObjectId],
        card_db: &'a F,
        ctx: Option<&DynamicContext>,
    ) -> i32
    where
        F: Fn(u64) -> Option<&'a CardDef>,
    {
        match self {
            DynamicValue::CardsInHand => ctx.map(|c| c.hand_size as i32).unwrap_or(0),
            DynamicValue::CreaturesControlled => battlefield
                .iter()
                .filter(|&&id| {
                    if let Some(inst) = objects.get(&id) {
                        if inst.controller != controller {
                            return false;
                        }
                        if let Some(def) = card_db(inst.card_def_id) {
                            return def.is_creature();
                        }
                    }
                    false
                })
                .count() as i32,
            DynamicValue::CardTypesInGraveyards => {
                if let Some(c) = ctx {
                    let mut seen = std::collections::HashSet::new();
                    for types in &c.graveyard_card_types {
                        for ct in types {
                            seen.insert(*ct);
                        }
                    }
                    seen.len() as i32
                } else {
                    0
                }
            }
            DynamicValue::TotalPowerControlled => battlefield
                .iter()
                .filter_map(|&id| {
                    let inst = objects.get(&id)?;
                    if inst.controller != controller {
                        return None;
                    }
                    let def = card_db(inst.card_def_id)?;
                    if def.is_creature() {
                        def.power
                    } else {
                        None
                    }
                })
                .sum(),
            DynamicValue::SwampsControlled => {
                battlefield
                    .iter()
                    .filter(|&&id| {
                        if let Some(inst) = objects.get(&id) {
                            if inst.controller != controller {
                                return false;
                            }
                            if let Some(def) = card_db(inst.card_def_id) {
                                return def.is_land()
                                    && def.subtypes.iter().any(|s| s.0 == "Swamp");
                            }
                        }
                        false
                    })
                    .count() as i32
            }
            DynamicValue::DevotionTo(color) => {
                battlefield
                    .iter()
                    .filter_map(|&id| {
                        let inst = objects.get(&id)?;
                        if inst.controller != controller {
                            return None;
                        }
                        let def = card_db(inst.card_def_id)?;
                        def.mana_cost.as_ref().map(|cost| cost.color_amount(*color))
                    })
                    .sum::<u32>() as i32
            }
            DynamicValue::CreaturesInGraveyard => {
                ctx.map(|c| c.creatures_in_graveyard as i32).unwrap_or(0)
            }
            DynamicValue::CreaturesWithSubtype(subtype_name) => battlefield
                .iter()
                .filter(|&&id| {
                    if let Some(inst) = objects.get(&id) {
                        if inst.controller != controller {
                            return false;
                        }
                        if let Some(def) = card_db(inst.card_def_id) {
                            return def.is_creature()
                                && def.subtypes.iter().any(|s| s.0 == *subtype_name);
                        }
                    }
                    false
                })
                .count() as i32,
            DynamicValue::PermanentsWithSubtype(subtype)
            | DynamicValue::AllPermanentsWithSubtype(subtype) => battlefield.iter()
                .filter(|&&id| objects.get(&id).map_or(false, |inst| {
                    (matches!(self, DynamicValue::AllPermanentsWithSubtype(_))
                        || inst.controller == controller) && card_db(inst.card_def_id)
                        .map_or(false, |def| def.subtypes.iter().any(|s| s.0 == *subtype))
                }))
                .count() as i32,
            DynamicValue::Fixed(val) => *val,
            DynamicValue::LandsControlled => {
                battlefield
                    .iter()
                    .filter(|&&id| {
                        if let Some(inst) = objects.get(&id) {
                            if inst.controller != controller {
                                return false;
                            }
                            if let Some(def) = card_db(inst.card_def_id) {
                                return def.is_land();
                            }
                        }
                        false
                    })
                    .count() as i32
            }
            DynamicValue::ChargeCountersOnSource => {
                // This needs source_id context; return 0 as fallback.
                // Actual evaluation happens in resolve_effect with source context.
                0
            }
            DynamicValue::TappedCreaturesControlled => battlefield
                .iter()
                .filter(|&&id| {
                    if let Some(inst) = objects.get(&id) {
                        if inst.controller != controller || !inst.tapped {
                            return false;
                        }
                        if let Some(def) = card_db(inst.card_def_id) {
                            return def.is_creature();
                        }
                    }
                    false
                })
                .count() as i32,
        }
    }
}

/// Effects that abilities and spells can produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    DealDamage {
        amount: u32,
        target: TargetSpec,
    },
    GainLife {
        amount: u32,
    },
    LoseLife {
        amount: u32,
        target: TargetSpec,
    },
    DrawCards {
        count: u32,
    },
    DestroyTarget {
        target: TargetSpec,
    },
    /// Exile target (e.g., Swords to Plowshares, Path to Exile).
    ExileTarget {
        target: TargetSpec,
    },
    /// Destroy all creatures (e.g., Wrath of God, Day of Judgment).
    DestroyAll,
    BounceTo {
        zone: ZoneType,
        target: TargetSpec,
    },
    Buff {
        power: i32,
        toughness: i32,
        until_eot: bool,
    },
    /// Debuff: target creature gets -N/-N until end of turn.
    Debuff {
        power: i32,
        toughness: i32,
        until_eot: bool,
    },
    DiscardCards {
        count: u32,
        target: TargetSpec,
    },
    CreateToken(TokenDef),
    /// Create N tokens where N is determined by a dynamic value at runtime.
    CreateTokens {
        token: TokenDef,
        count: DynamicValue,
    },
    Counter {
        target: TargetSpec,
    },
    /// Put +1/+1 counters on target creature.
    PutCounters {
        count: i32,
        target: TargetSpec,
    },
    /// Each player mills N cards.
    MillCards {
        count: u32,
        target: TargetSpec,
    },
    /// Each player sacrifices N creatures.
    SacrificeCreatures {
        count: u32,
        target: TargetSpec,
    },
    /// Prevent all combat damage this turn.
    PreventCombatDamage,
    /// Add mana to the controller's mana pool.
    AddMana {
        color: Option<Color>,
        amount: u32,
    },
    /// Add mana where the amount is determined dynamically at runtime.
    AddDynamicMana {
        color: Color,
        count: DynamicValue,
    },
    /// Lose life where the amount is determined dynamically.
    LoseDynamicLife {
        amount: DynamicValue,
        target: TargetSpec,
    },
    /// Take an extra turn after this one (e.g., Time Walk, Temporal Manipulation).
    ExtraTurn,
    /// Skip a phase of the controller's next turn (e.g., Stasis skipping untap).
    SkipPhase(crate::game::Phase),
    Multiple(Vec<Effect>),
    /// Search the controller's library and put a card into the destination zone.
    SearchLibrary {
        destination: ZoneType,
        subtype_filter: Vec<Subtype>,
    },
    /// Bounce all nonland permanents opponents control (e.g., Cyclonic Rift overload).
    BounceAllNonlandOpponents,
    /// Put a card from a graveyard on top of its owner's library.
    ReturnToTopOfLibrary {
        target: TargetSpec,
    },
    /// Untap target permanent.
    UntapTarget {
        target: TargetSpec,
    },
    // --- Zone manipulation effects ---

    /// Return target card from graveyard to battlefield (e.g., Reanimate, Animate Dead).
    ReturnFromGraveyardToBattlefield {
        target: TargetSpec,
    },
    /// Return target card from graveyard to hand (e.g., Regrowth, Eternal Witness).
    ReturnFromGraveyardToHand {
        target: TargetSpec,
    },
    /// Exile target card from a graveyard (e.g., Bojuka Bog, Tormod's Crypt).
    ExileFromGraveyard {
        target: TargetSpec,
    },
    /// Exile a card from controller's hand, linked to the source permanent
    /// (e.g., Gustha's Scepter). The exiled card's `exiled_by` is set to the source.
    ExileFromHandLinked,
    /// Return a card exiled with the source permanent to its owner's hand
    /// (e.g., Gustha's Scepter second ability).
    ReturnLinkedExileToHand,
    /// Shuffle target(s) into their owner's library.
    ShuffleIntoLibrary {
        target: TargetSpec,
    },
    /// Put a card on the bottom of its owner's library (e.g., Terminus, Hinder).
    PutOnBottomOfLibrary {
        target: TargetSpec,
    },

    // --- Creature/permanent manipulation ---

    /// Target creature gains a keyword ability until end of turn.
    GainKeywordUntilEOT {
        keyword: KeywordAbility,
        target: TargetSpec,
    },
    /// Set target creature's base power and toughness (e.g., Turn to Frog, Humility).
    SetPowerToughness {
        power: i32,
        toughness: i32,
        until_eot: bool,
        target: TargetSpec,
    },
    /// Gain control of target permanent until end of turn (e.g., Act of Treason).
    GainControlUntilEOT {
        target: TargetSpec,
    },
    /// Target creature fights another target creature (e.g., Prey Upon, Domri Rade).
    Fight {
        target: TargetSpec,
    },
    /// Tap target permanent (e.g., Frost Breath, Icy Manipulator).
    TapTarget {
        target: TargetSpec,
    },

    // --- Player-targeted effects ---

    /// Each opponent loses N life (e.g., Blood Artist, Gray Merchant of Asphodel).
    EachOpponentLosesLife {
        amount: u32,
    },
    /// Each opponent discards N cards (e.g., Sire of Insanity, Bottomless Pit).
    EachOpponentDiscards {
        count: u32,
    },
    /// Each opponent sacrifices N creatures (e.g., Fleshbag Marauder, Dictate of Erebos).
    EachOpponentSacrifices {
        count: u32,
    },
    /// Target player draws N cards then discards M cards (e.g., Faithless Looting).
    DrawThenDiscard {
        draw: u32,
        discard: u32,
        target: TargetSpec,
    },
    /// Gain life equal to a dynamic value (e.g., Gray Merchant drains for devotion).
    GainDynamicLife {
        amount: DynamicValue,
    },

    // --- Conditional/modal effects ---

    /// Choose one (or more) from a list of effects.
    Modal {
        choices: Vec<Effect>,
        choose_count: u32,
    },
    /// Execute an effect only if a condition is true; otherwise execute the else branch.
    Conditional {
        condition: Condition,
        if_true: Box<Effect>,
        if_false: Option<Box<Effect>>,
    },
    /// Repeat an effect for each of a variable (e.g., "for each creature you control").
    ForEach {
        count: DynamicValue,
        effect: Box<Effect>,
    },

    // --- Predefined token shortcuts ---

    /// Create a predefined token type (Treasure, Food, Clue, Blood, etc.).
    CreatePredefinedToken {
        token_type: PredefinedToken,
        count: u32,
    },

    // --- Scry / library manipulation ---

    /// Scry N — look at top N cards, put any on bottom in any order, rest on top.
    Scry {
        count: u32,
    },

    /// Proliferate — for each permanent/player with a counter, add one more of that type.
    Proliferate,

    /// Buff other creatures with a specific subtype you control by +X/+X
    /// until end of turn, where X is a dynamic value.
    /// (e.g., Ashcoat: "other Rats you control get +X/+X where X = rats you control")
    BuffOtherSubtype {
        subtype: String,
        amount: DynamicValue,
        until_eot: bool,
    },

    /// Grant an extra land play for this turn (e.g., Explore sorcery).
    ExtraLandDrop,

    /// Surveil N — look at top N cards, put any into graveyard, rest on top.
    Surveil {
        count: u32,
    },

    /// Add mana of any color (e.g., Lotus Cobra landfall).
    AddManaOfAnyColor {
        amount: u32,
    },

    /// Double the power of the source/attached creature until end of turn.
    DoublePowerUntilEOT {
        target: TargetSpec,
    },

    /// Deal dynamic damage (e.g., Valakut deals 3 per mountain).
    DealDynamicDamage {
        amount: DynamicValue,
        target: TargetSpec,
    },

    /// Create a token that is a copy of the source permanent (e.g., Scute Swarm at 6+ lands).
    /// The token inherits the source's card_def_id and all its abilities.
    CreateTokenCopyOfSource,

    /// Create a token from a specific CardDef in the database (e.g., Chocobo token with
    /// landfall trigger). The CardDef must already be registered.
    CreateTokenFromDef {
        card_def_id: u64,
    },

    /// For effects we haven't modeled yet — described textually.
    Unimplemented(String),

    /// Unit-test fixture for exercising terminal copy ordering through the
    /// real recursive resolver; absent from production card definitions.
    #[cfg(test)]
    TestCopyBatch { copies: usize },

    /// Copy a captured cast spell for each other currently legal creature.
    /// Requires a cast-trigger snapshot and must be terminal on resolution.
    CopyCastSpellForOtherCreatures,
}

/// Conditions that can be checked at runtime for conditional effects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    /// Controller controls at least one creature.
    ControlCreatures,
    /// Controller's life is at or above N.
    LifeAtOrAbove(i32),
    /// Controller's life is at or below N.
    LifeAtOrBelow(i32),
    /// It is the controller's turn.
    IsYourTurn,
    /// The source permanent has +1/+1 counters.
    SourceHasCounters,
    /// Controller controls N or more permanents of a type.
    ControlNOrMore {
        count: u32,
        card_type: CardType,
    },
    /// Always true (for testing / default).
    Always,
    /// Controller has no cards in hand (hellbent).
    HandIsEmpty,
    /// Controller controls N or more total permanents (for ascend/city's blessing).
    ControlNOrMorePermanents {
        count: u32,
    },
}

/// Predefined token types used across many cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredefinedToken {
    /// 0/0 artifact — sacrifice, add one mana of any color (simplified: adds colorless).
    Treasure,
    /// 0/0 artifact — sacrifice, gain 3 life.
    Food,
    /// 0/0 artifact — sacrifice, draw a card.
    Clue,
    /// 0/0 artifact — sacrifice, discard a card then draw a card.
    Blood,
    /// 1/1 white Soldier creature.
    Soldier,
    /// 1/1 white Spirit creature with flying.
    Spirit,
    /// 2/2 black Zombie creature.
    Zombie,
    /// 1/1 green Saproling creature.
    Saproling,
    /// 1/1 red Goblin creature.
    Goblin,
    /// 1/1 white Human creature.
    Human,
    /// 3/3 green Beast creature.
    Beast,
}

impl PredefinedToken {
    /// Convert a predefined token type into a concrete TokenDef.
    pub fn to_token_def(&self) -> TokenDef {
        match self {
            PredefinedToken::Treasure => TokenDef {
                name: "Treasure".to_string(),
                power: 0,
                toughness: 0,
                colors: vec![],
                subtypes: vec![Subtype("Treasure".to_string())],
                keywords: vec![],
            },
            PredefinedToken::Food => TokenDef {
                name: "Food".to_string(),
                power: 0,
                toughness: 0,
                colors: vec![],
                subtypes: vec![Subtype("Food".to_string())],
                keywords: vec![],
            },
            PredefinedToken::Clue => TokenDef {
                name: "Clue".to_string(),
                power: 0,
                toughness: 0,
                colors: vec![],
                subtypes: vec![Subtype("Clue".to_string())],
                keywords: vec![],
            },
            PredefinedToken::Blood => TokenDef {
                name: "Blood".to_string(),
                power: 0,
                toughness: 0,
                colors: vec![],
                subtypes: vec![Subtype("Blood".to_string())],
                keywords: vec![],
            },
            PredefinedToken::Soldier => TokenDef {
                name: "Soldier".to_string(),
                power: 1,
                toughness: 1,
                colors: vec![Color::White],
                subtypes: vec![Subtype("Soldier".to_string())],
                keywords: vec![],
            },
            PredefinedToken::Spirit => TokenDef {
                name: "Spirit".to_string(),
                power: 1,
                toughness: 1,
                colors: vec![Color::White],
                subtypes: vec![Subtype("Spirit".to_string())],
                keywords: vec![KeywordAbility::Flying],
            },
            PredefinedToken::Zombie => TokenDef {
                name: "Zombie".to_string(),
                power: 2,
                toughness: 2,
                colors: vec![Color::Black],
                subtypes: vec![Subtype("Zombie".to_string())],
                keywords: vec![],
            },
            PredefinedToken::Saproling => TokenDef {
                name: "Saproling".to_string(),
                power: 1,
                toughness: 1,
                colors: vec![Color::Green],
                subtypes: vec![Subtype("Saproling".to_string())],
                keywords: vec![],
            },
            PredefinedToken::Goblin => TokenDef {
                name: "Goblin".to_string(),
                power: 1,
                toughness: 1,
                colors: vec![Color::Red],
                subtypes: vec![Subtype("Goblin".to_string())],
                keywords: vec![],
            },
            PredefinedToken::Human => TokenDef {
                name: "Human".to_string(),
                power: 1,
                toughness: 1,
                colors: vec![Color::White],
                subtypes: vec![Subtype("Human".to_string())],
                keywords: vec![],
            },
            PredefinedToken::Beast => TokenDef {
                name: "Beast".to_string(),
                power: 3,
                toughness: 3,
                colors: vec![Color::Green],
                subtypes: vec![Subtype("Beast".to_string())],
                keywords: vec![],
            },
        }
    }
}

/// What a targeting restriction looks like.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetSpec {
    /// Target any creature.
    AnyCreature,
    /// Target any player.
    AnyPlayer,
    /// Target creature or player.
    CreatureOrPlayer,
    /// Target creature or planeswalker.
    CreatureOrPlaneswalker,
    /// Target opponent.
    Opponent,
    /// Target the controller (self).
    Controller,
    /// Target any nonland permanent.
    AnyNonlandPermanent,
    /// Target any permanent.
    AnyPermanent,
    /// Target any spell on the stack.
    AnySpell,
    /// No target (e.g., "each opponent").
    NoTarget,
    /// Each creature on the battlefield (no targeting — affects all).
    EachCreature,
    /// A card in the controller's hand (for effects that choose a hand card).
    CardInHand,
    /// A card in exile that was exiled by the source permanent.
    CardInExileBySource,
}

/// Token creature definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenDef {
    pub name: String,
    pub power: u32,
    pub toughness: u32,
    pub colors: Vec<Color>,
    pub subtypes: Vec<Subtype>,
    pub keywords: Vec<KeywordAbility>,
}
