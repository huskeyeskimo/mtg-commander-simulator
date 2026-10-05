pub mod catalog;
pub mod database;
pub mod effects;
pub mod keywords;
pub mod sample;

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::layers::StaticAbility;
use crate::mana::{Color, ManaCost};

// Re-export from submodules so external code can use `crate::card::KeywordAbility` etc.
pub use database::CardDatabase;
pub use effects::{DynamicContext, DynamicValue, Effect, TargetSpec, TokenDef};
pub use keywords::KeywordAbility;

/// Unique identifier for a card definition (template).
pub type CardId = u64;

/// Unique identifier for a specific card instance in a game.
/// Two copies of "Lightning Bolt" share a CardId but have different ObjectId.
pub type ObjectId = u64;

/// Top-level card types in MTG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CardType {
    Creature,
    Instant,
    Sorcery,
    Enchantment,
    Artifact,
    Planeswalker,
    Land,
    /// Kindred (formerly tribal) is a card type, not a creature subtype.
    /// Appended to preserve the existing bincode discriminants.
    Kindred,
}

/// Creature subtypes (a small representative set; extensible).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Subtype(pub String);

/// Supertypes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Supertype {
    Basic,
    Legendary,
    Snow,
}

/// What kind of mana a land can produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManaAbility {
    /// Tap to add one mana of the given color.
    TapForColor(Color),
    /// Tap to add one colorless mana.
    TapForColorless,
    /// Tap to add one mana of any color (e.g., City of Brass).
    TapForAny,
    /// Tap for one of several colors (e.g., dual lands).
    TapForChoice(Vec<Color>),
    /// Tap to add N colorless mana (e.g., Sol Ring → 2, Basalt Monolith → 3).
    TapForColorlessAmount(u32),
    /// Tap to add one mana of any color among legendary creatures and
    /// planeswalkers you control (Mox Amber). Produces nothing if you
    /// control no legendary creature or planeswalker.
    TapForLegendaryColors,
}

/// An activated ability (non-mana).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivatedAbility {
    pub cost: ManaCost,
    pub requires_tap: bool,
    /// Additional cost: sacrifice a permanent as part of activating this ability.
    #[serde(default)]
    pub sacrifice_cost: Option<SacrificeCost>,
    /// Additional cost: pay life (e.g., fetch lands pay 1 life).
    #[serde(default)]
    pub life_cost: u32,
    pub effect: Effect,
    pub description: String,
}

/// A sacrifice cost required to activate an ability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SacrificeCost {
    /// Sacrifice any creature you control (e.g., Nezumi Bone-Reader).
    AnyCreature,
    /// Sacrifice a creature with a specific subtype (e.g., Marrow-Gnawer: "Sacrifice a Rat").
    CreatureWithSubtype(Subtype),
    /// Sacrifice the source permanent itself (e.g., fetch lands).
    SelfSacrifice,
}

/// A loyalty ability on a planeswalker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoyaltyAbility {
    /// Loyalty cost: positive = add counters, negative = remove counters, 0 = no change.
    pub cost: i32,
    pub effect: Effect,
    pub description: String,
}

/// A triggered ability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggeredAbility {
    pub trigger: TriggerCondition,
    pub effect: Effect,
    pub description: String,
}

/// When a triggered ability fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerCondition {
    EntersBattlefield,
    /// "Whenever a creature enters the battlefield" — watcher trigger (e.g., Soul Warden).
    ACreatureEnters,
    LeavesBattlefield,
    Dies,
    /// "Whenever a creature dies" — watcher trigger (e.g., Blood Artist).
    ACreatureDies,
    /// "Whenever a creature you control dies" — filtered watcher trigger.
    ACreatureYouControlDies,
    AttacksAlone,
    Attacks,
    BeginningOfUpkeep,
    /// "At the beginning of combat on your turn" (e.g., Lord Skitter).
    BeginningOfCombat,
    EndOfTurn,
    DealsDamage,
    DealsCombatDamage,
    DealsCombatDamageToPlayer,
    /// Whenever the controller casts any spell (e.g., Tidespout Tyrant).
    YouCastSpell,
    /// Whenever the controller casts a creature spell (e.g., Bontu's Monument).
    YouCastCreatureSpell,
    /// Whenever an opponent casts a noncreature spell (e.g., Mystic Remora, Nezahal).
    OpponentCastsNoncreatureSpell,
    /// Whenever an opponent casts any spell (e.g., Rhystic Study).
    OpponentCastsSpell,
    /// Whenever an opponent draws a card (e.g., Consecrated Sphinx).
    OpponentDrawsCard,
    /// Landfall — "Whenever a land enters the battlefield under your control".
    ALandYouControlEnters,
    /// "Whenever you play a land" (e.g., Flubs, the Fool).
    YouPlayALand,
    /// "Whenever you discard a card" (e.g., Monument to Endurance).
    YouDiscardACard,
    /// Whenever you cast an instant or sorcery targeting only this source.
    YouCastInstantOrSorceryTargetingOnlySelf,
    /// Departure observer predicate for the transition kernel. No authored
    /// card currently relies on this general predicate.
    APermanentLeaves,
}

/// What spells a cost reduction applies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostReductionTarget {
    /// All spells (e.g., Jet Medallion for black spells in mono-black).
    AllSpells,
    /// Only creature spells (e.g., Urza's Incubator, Herald's Horn, Bontu's Monument).
    CreatureSpells,
}

/// Cost reduction provided by a permanent on the battlefield.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostReduction {
    /// How much generic mana to reduce.
    pub generic_reduction: u32,
    /// What spells the reduction applies to.
    pub applies_to: CostReductionTarget,
}

/// What spells a cost increase applies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostIncreaseTarget {
    /// All spells (e.g., Grand Arbiter Augustin IV for opponents).
    AllSpells,
    /// Only noncreature spells (e.g., Thalia, Guardian of Thraben).
    NoncreatureSpells,
    /// Only creature spells.
    CreatureSpells,
}

/// Cost increase imposed by a permanent on opponents' spells (tax effect).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostIncrease {
    /// How much generic mana to add.
    pub generic_increase: u32,
    /// What spells the increase applies to.
    pub applies_to: CostIncreaseTarget,
    /// Whether this applies to the controller's spells (true) or opponents' (false).
    pub affects_controller: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ZoneType {
    Library,
    Hand,
    Battlefield,
    Graveyard,
    Exile,
    Stack,
    Command,
}

/// The card definition — the "template" from which game objects are created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardDef {
    pub id: CardId,
    pub name: String,
    pub mana_cost: Option<ManaCost>,
    /// Explicit base colors for tokens and other cards without colored mana costs.
    #[serde(default)]
    pub colors: Option<Vec<Color>>,
    pub card_types: Vec<CardType>,
    pub supertypes: Vec<Supertype>,
    pub subtypes: Vec<Subtype>,
    pub keywords: Vec<KeywordAbility>,

    // Creature stats (None for non-creatures).
    pub power: Option<i32>,
    pub toughness: Option<i32>,

    // Land abilities.
    pub mana_abilities: Vec<ManaAbility>,

    // Spell/ability effects (for instants, sorceries, and ETB effects).
    pub spell_effect: Option<Effect>,

    // Activated and triggered abilities.
    pub activated_abilities: Vec<ActivatedAbility>,
    pub triggered_abilities: Vec<TriggeredAbility>,

    // Static abilities that generate continuous effects on the battlefield.
    pub static_abilities: Vec<StaticAbility>,

    // Loyalty (planeswalkers).
    pub starting_loyalty: Option<u32>,

    /// Loyalty abilities for planeswalkers (e.g., +1, -2, -7).
    #[serde(default)]
    pub loyalty_abilities: Vec<LoyaltyAbility>,

    // Whether this permanent enters the battlefield tapped.
    pub enters_tapped: bool,

    // Original oracle text for reference.
    pub oracle_text: String,

    /// Dynamic power formula — if set, the creature's base power is computed
    /// at runtime from the game state (e.g., Tarmogoyf, Maro).
    #[serde(default)]
    pub dynamic_power: Option<DynamicValue>,

    /// Dynamic toughness formula — same as dynamic_power but for toughness.
    #[serde(default)]
    pub dynamic_toughness: Option<DynamicValue>,

    /// Cost reduction this permanent provides while on the battlefield.
    #[serde(default)]
    pub cost_reduction: Option<CostReduction>,

    /// Cost increase this permanent imposes while on the battlefield (tax effect).
    #[serde(default)]
    pub cost_increase: Option<CostIncrease>,

    /// Equipment: mana cost to equip to a creature you control (sorcery speed).
    #[serde(default)]
    pub equip_cost: Option<ManaCost>,

    /// Flashback cost — cast from graveyard for this cost, then exile.
    #[serde(default)]
    pub flashback_cost: Option<ManaCost>,

    /// Kicker cost — optional additional cost for an enhanced effect.
    #[serde(default)]
    pub kicker_cost: Option<ManaCost>,

    /// Overload cost — cast for this cost to replace "target" with "each".
    #[serde(default)]
    pub overload_cost: Option<ManaCost>,

    /// Evoke cost — cast for this cost and sacrifice when ETB.
    #[serde(default)]
    pub evoke_cost: Option<ManaCost>,

    /// Escape: number of cards to exile from graveyard (escape cost is the regular mana cost).
    #[serde(default)]
    pub escape_exile_count: Option<u32>,

    /// Annihilator N value (how many permanents the defending player sacrifices).
    #[serde(default)]
    pub annihilator_count: Option<u32>,

    /// For double-faced cards: the CardId of the back face definition.
    /// Both faces are stored as separate CardDef entries in the database.
    #[serde(default)]
    pub back_face_id: Option<CardId>,

    /// For DFC back faces: the CardId of the front face.
    #[serde(default)]
    pub front_face_id: Option<CardId>,
}

impl CardDef {
    pub fn base_colors(&self) -> Vec<Color> {
        self.colors.clone().unwrap_or_else(|| self.color_identity())
    }

    pub fn is_creature(&self) -> bool {
        self.card_types.contains(&CardType::Creature)
    }

    pub fn is_land(&self) -> bool {
        self.card_types.contains(&CardType::Land)
    }

    pub fn is_instant(&self) -> bool {
        self.card_types.contains(&CardType::Instant)
    }

    pub fn is_sorcery(&self) -> bool {
        self.card_types.contains(&CardType::Sorcery)
    }

    pub fn has_flash(&self) -> bool {
        self.keywords.contains(&KeywordAbility::Flash)
    }

    pub fn is_basic_land(&self) -> bool {
        self.is_land() && self.supertypes.contains(&Supertype::Basic)
    }

    pub fn is_enchantment(&self) -> bool {
        self.card_types.contains(&CardType::Enchantment)
    }

    pub fn is_artifact(&self) -> bool {
        self.card_types.contains(&CardType::Artifact)
    }

    pub fn is_equipment(&self) -> bool {
        self.subtypes.iter().any(|s| s.0 == "Equipment")
    }

    pub fn is_aura(&self) -> bool {
        self.is_enchantment() && self.subtypes.iter().any(|s| s.0 == "Aura")
    }

    pub fn cmc(&self) -> u32 {
        self.mana_cost.as_ref().map(|c| c.cmc()).unwrap_or(0)
    }

    /// Can this card be cast at instant speed?
    pub fn is_instant_speed(&self) -> bool {
        self.is_instant() || self.has_flash()
    }

    /// Color identity of this card (CR 903.4).
    pub fn color_identity(&self) -> Vec<Color> {
        let mut colors = std::collections::HashSet::new();
        // Mana cost contributes to color identity
        if let Some(ref cost) = self.mana_cost {
            for c in cost.colors() {
                colors.insert(c);
            }
        }
        // Mana abilities contribute to color identity (basic land subtypes, duals)
        for ability in &self.mana_abilities {
            match ability {
                ManaAbility::TapForColor(c) => {
                    colors.insert(*c);
                }
                ManaAbility::TapForChoice(cs) => {
                    for c in cs {
                        colors.insert(*c);
                    }
                }
                _ => {}
            }
        }
        let mut result: Vec<Color> = colors.into_iter().collect();
        result.sort_by_key(|c| *c as u8);
        result
    }
}

impl Default for CardDef {
    fn default() -> Self {
        CardDef {
            id: 0,
            name: String::new(),
            mana_cost: None,
            colors: None,
            card_types: Vec::new(),
            supertypes: Vec::new(),
            subtypes: Vec::new(),
            keywords: Vec::new(),
            power: None,
            toughness: None,
            mana_abilities: Vec::new(),
            spell_effect: None,
            activated_abilities: Vec::new(),
            triggered_abilities: Vec::new(),
            static_abilities: Vec::new(),
            starting_loyalty: None,
            loyalty_abilities: Vec::new(),
            enters_tapped: false,
            oracle_text: String::new(),
            dynamic_power: None,
            dynamic_toughness: None,
            cost_reduction: None,
            cost_increase: None,
            equip_cost: None,
            flashback_cost: None,
            kicker_cost: None,
            overload_cost: None,
            evoke_cost: None,
            escape_exile_count: None,
            annihilator_count: None,
            back_face_id: None,
            front_face_id: None,
        }
    }
}

impl fmt::Display for CardDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(ref cost) = self.mana_cost {
            write!(f, " {}", cost)?;
        }
        Ok(())
    }
}

/// A card object in the game — an instance of a CardDef with game-specific state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardInstance {
    pub object_id: ObjectId,
    pub card_def_id: CardId,
    pub owner: usize,      // player index
    pub controller: usize, // player index (may differ from owner)

    // Battlefield state
    pub tapped: bool,
    pub summoning_sick: bool,
    pub damage_marked: u32,
    pub plus_counters: i32,  // +1/+1 counters
    pub minus_counters: i32, // -1/-1 counters

    // Temporary effects for the current turn
    pub temp_power_mod: i32,
    pub temp_toughness_mod: i32,
    pub temp_keywords: Vec<KeywordAbility>,

    // Planeswalker loyalty
    pub loyalty_counters: u32,
    /// Whether a loyalty ability has been activated this turn.
    pub loyalty_activated_this_turn: bool,

    // Attached permanents (auras, equipment).
    pub attached_to: Option<ObjectId>,
    pub attachments: Vec<ObjectId>,

    /// Whether this is a token (CR 111.6).
    pub is_token: bool,

    /// Zone-change counter — incremented each time this object changes zones.
    pub zone_change_count: u32,

    /// Tracks which permanent exiled this card (for "exiled with" associations).
    pub exiled_by: Option<ObjectId>,
}

impl CardInstance {
    pub fn new(object_id: ObjectId, card_def_id: CardId, owner: usize) -> Self {
        CardInstance {
            object_id,
            card_def_id,
            owner,
            controller: owner,
            tapped: false,
            summoning_sick: true,
            damage_marked: 0,
            plus_counters: 0,
            minus_counters: 0,
            temp_power_mod: 0,
            temp_toughness_mod: 0,
            temp_keywords: Vec::new(),
            loyalty_counters: 0,
            loyalty_activated_this_turn: false,
            attached_to: None,
            attachments: Vec::new(),
            is_token: false,
            zone_change_count: 0,
            exiled_by: None,
        }
    }

    /// Get effective power given the card definition.
    pub fn effective_power(&self, def: &CardDef) -> i32 {
        def.power.unwrap_or(0) + self.plus_counters - self.minus_counters + self.temp_power_mod
    }

    /// Get effective toughness given the card definition.
    pub fn effective_toughness(&self, def: &CardDef) -> i32 {
        def.toughness.unwrap_or(0) + self.plus_counters - self.minus_counters
            + self.temp_toughness_mod
    }

    /// Remaining toughness after damage.
    pub fn remaining_toughness(&self, def: &CardDef) -> i32 {
        self.effective_toughness(def) - self.damage_marked as i32
    }

    /// Check if this creature has a keyword (base + temporary).
    pub fn has_keyword(&self, def: &CardDef, kw: KeywordAbility) -> bool {
        def.keywords.contains(&kw) || self.temp_keywords.contains(&kw)
    }

    /// Can this creature attack? (not tapped, not sick unless haste, not defender)
    pub fn can_attack(&self, def: &CardDef) -> bool {
        !self.tapped
            && (!self.summoning_sick || self.has_keyword(def, KeywordAbility::Haste))
            && !self.has_keyword(def, KeywordAbility::Defender)
    }

    /// Reset end-of-turn temporary effects.
    pub fn cleanup_eot(&mut self) {
        self.temp_power_mod = 0;
        self.temp_toughness_mod = 0;
        self.temp_keywords.clear();
        self.damage_marked = 0;
        self.loyalty_activated_this_turn = false;
    }
}

/// A deck list: card IDs and quantities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decklist {
    pub name: String,
    pub cards: Vec<DeckEntry>,
    /// Commander cards (for Commander format decks).
    #[serde(default)]
    pub commanders: Vec<DeckEntry>,
    /// Cards that tutors can search for.
    #[serde(default)]
    pub tutor_targets: Vec<CardId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckEntry {
    pub card_id: CardId,
    pub quantity: u32,
}

impl Decklist {
    pub fn total_cards(&self) -> u32 {
        self.cards.iter().map(|e| e.quantity).sum()
    }

    /// Expand into a flat list of CardIds (one per copy).
    pub fn expand(&self) -> Vec<CardId> {
        self.cards
            .iter()
            .flat_map(|e| std::iter::repeat(e.card_id).take(e.quantity as usize))
            .collect()
    }
}
