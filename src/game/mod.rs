use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use crate::card::{CardDef, CardId, CardInstance, ObjectId, ZoneType};

// Re-export CardDatabase from its new home in card::database for backward compatibility.
pub use crate::card::CardDatabase;
use crate::combo::ComboRegistry;
use crate::events::GameEvent;
use crate::layers::{ComputedCharacteristics, ContinuousEffect};
use crate::mana::ManaPool;
use crate::replacement::{ReplacementEffect, ReplacementEventKind, ReplacementAction};

// ---------------------------------------------------------------------------
// Game format
// ---------------------------------------------------------------------------

/// Which format rules the game uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameFormat {
    /// Standard 60-card constructed, 20 life.
    Standard,
    /// 1v1 Commander: 100-card singleton, 40 life, commander zone, commander
    /// tax, 21 commander damage.
    Commander,
}

// ---------------------------------------------------------------------------
// Characteristics cache (Fix 1 + Fix 3)
// ---------------------------------------------------------------------------

/// Interior of the transient characteristics cache.
#[derive(Debug, Default)]
struct CharacteristicsCacheInner {
    entries: HashMap<ObjectId, ComputedCharacteristics>,
    /// Lazily-built set for O(1) battlefield membership tests.
    battlefield_set: Option<HashSet<ObjectId>>,
}

/// Transient cache for [`compute_characteristics`](crate::layers::compute_characteristics) results.
///
/// Uses `Mutex` for interior mutability so the read-only query methods
/// (`effective_power`, `has_keyword`, etc.) can populate the cache through
/// shared `&self` references. `Mutex` (rather than `RefCell`) is required
/// because `GameState` must be `Sync` for rayon parallel iteration.
/// Each clone gets its own empty cache, so contention never occurs.
///
/// Per the Snapshot Contract (Phase 0.3), this cache is:
/// - **NOT serialized** (`#[serde(skip)]`) — it's derived state
/// - **NOT cloned** — `Clone` produces an empty cache (cheap `GameState::clone()`)
/// - **Invalidated** whenever canonical state that affects characteristics changes
pub struct CharacteristicsCache(Mutex<CharacteristicsCacheInner>);

impl Clone for CharacteristicsCache {
    fn clone(&self) -> Self {
        // Per Snapshot Contract: derived/cached fields reset on clone.
        CharacteristicsCache(Mutex::new(CharacteristicsCacheInner::default()))
    }
}

impl Default for CharacteristicsCache {
    fn default() -> Self {
        CharacteristicsCache(Mutex::new(CharacteristicsCacheInner::default()))
    }
}

impl std::fmt::Debug for CharacteristicsCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CharacteristicsCache")
            .field("entries", &self.0.lock().map(|c| c.entries.len()).unwrap_or(0))
            .finish()
    }
}

/// Index into the players array (0 or 1 for a two-player game).
pub type PlayerIndex = usize;

/// Represents the phase/step within a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Phase {
    // Pre-game
    Mulligan,

    // Beginning phase
    Untap,
    Upkeep,
    Draw,

    // Pre-combat main phase
    PreCombatMain,

    // Combat phase
    BeginningOfCombat,
    DeclareAttackers,
    DeclareBlockers,
    FirstStrikeDamage,
    CombatDamage,
    EndOfCombat,

    // Post-combat main phase
    PostCombatMain,

    // Ending phase
    EndStep,
    Cleanup,
}

impl Phase {
    /// Ordered list of all phases in a turn.
    pub const TURN_ORDER: [Phase; 13] = [
        Phase::Untap,
        Phase::Upkeep,
        Phase::Draw,
        Phase::PreCombatMain,
        Phase::BeginningOfCombat,
        Phase::DeclareAttackers,
        Phase::DeclareBlockers,
        Phase::FirstStrikeDamage,
        Phase::CombatDamage,
        Phase::EndOfCombat,
        Phase::PostCombatMain,
        Phase::EndStep,
        Phase::Cleanup,
    ];

    /// Can a player normally cast sorcery-speed spells here?
    pub fn is_main_phase(&self) -> bool {
        matches!(self, Phase::PreCombatMain | Phase::PostCombatMain)
    }

    /// Is this a combat step where damage is dealt?
    pub fn is_damage_step(&self) -> bool {
        matches!(self, Phase::FirstStrikeDamage | Phase::CombatDamage)
    }
}

/// Identity of one occurrence on the stack, separate from physical card identity.
pub type StackId = u64;

/// A stack entry — a spell or ability waiting to resolve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackEntry {
    pub id: StackId,
    pub source: StackSource,
    pub controller: PlayerIndex,
    pub targets: Vec<Target>,
    /// Incarnation of each object target when selected (CR 400.7).
    #[serde(default)]
    pub target_generations: Vec<Option<u32>>,
}

/// What put this entry on the stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StackSource {
    /// A card being cast (the object ID of the card).
    Spell(ObjectId),
    /// An activated ability from a permanent.
    ActivatedAbility {
        source_id: ObjectId,
        ability_index: usize,
    },
    /// A triggered ability.
    TriggeredAbility {
        source_id: ObjectId,
        ability_index: usize,
        context: Box<TriggerContext>,
    },
    /// A spell without a physical card on the stack. Owns its spell data.
    SpellCopy { definition: Box<CardDef> },
}

impl StackSource {
    pub fn is_spell(&self) -> bool {
        matches!(self, Self::Spell(_) | Self::SpellCopy { .. })
    }
}

/// A resolved target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    Player(PlayerIndex),
    Object(ObjectId),
    /// A spell occurrence on the stack, not its physical card.
    StackEntry(StackId),
}

/// Per-player state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub life: i32,
    pub mana_pool: ManaPool,
    pub land_plays_remaining: u32,
    pub has_drawn_for_turn: bool,
    pub has_lost: bool,
    pub has_won: bool,

    // Zones (each is a list of ObjectIds).
    pub library: Vec<ObjectId>,
    pub hand: Vec<ObjectId>,
    pub graveyard: Vec<ObjectId>,
    pub exile: Vec<ObjectId>,

    // ---- Commander fields ----
    /// The command zone (Commander format only). Contains ObjectIds of cards
    /// currently in the command zone (typically the commander).
    pub command_zone: Vec<ObjectId>,
    /// The CardId of this player's commander (None in non-Commander formats).
    /// Used during setup to identify which card is the commander.
    pub commander_card_id: Option<CardId>,
    /// The ObjectId of this player's commander instance (set during game setup).
    /// Used at runtime for identity checks — more robust than card_def_id
    /// matching since two cards can share a CardId (e.g., clone effects).
    pub commander_object_id: Option<ObjectId>,
    /// How many times the commander has been cast from the command zone.
    /// Each additional cast costs {2} more (the "commander tax").
    pub commander_tax: u32,
    /// Partner commander fields (CR 702.124): a second commander with Partner.
    /// Combined color identity is used for deck validation.
    #[serde(default)]
    pub partner_commander_card_id: Option<CardId>,
    #[serde(default)]
    pub partner_commander_object_id: Option<ObjectId>,
    /// Separate tax for the partner commander.
    #[serde(default)]
    pub partner_commander_tax: u32,
    /// Commander damage received from each opponent's commander, indexed by
    /// opponent PlayerIndex. In 1v1 this is a single-element vec.
    /// A player loses if any entry reaches 21.
    pub commander_damage_received: Vec<i32>,

    /// Poison counters on this player (10+ = lose the game via SBA).
    #[serde(default)]
    pub poison_counters: u32,

    // ---- Mulligan fields ----
    /// Number of times this player has mulliganed (London Mulligan).
    /// After keeping, the player puts this many cards on the bottom of their library.
    pub mulligan_count: u32,
    /// Whether this player has decided to keep their hand.
    pub mulligan_decided: bool,

    /// Cards that tutors can search for (configured from the decklist).
    /// When non-empty, tutor effects present a choice restricted to this set.
    /// MCCFR learns which target is optimal in each game state.
    #[serde(default)]
    pub tutor_targets: Vec<CardId>,
}

impl PlayerState {
    pub fn new() -> Self {
        PlayerState {
            life: 20,
            mana_pool: ManaPool::empty(),
            land_plays_remaining: 1,
            has_drawn_for_turn: false,
            has_lost: false,
            has_won: false,
            library: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            exile: Vec::new(),
            command_zone: Vec::new(),
            commander_card_id: None,
            commander_object_id: None,
            commander_tax: 0,
            partner_commander_card_id: None,
            partner_commander_object_id: None,
            partner_commander_tax: 0,
            commander_damage_received: Vec::new(),
            poison_counters: 0,
            mulligan_count: 0,
            mulligan_decided: false,
            tutor_targets: Vec::new(),
        }
    }

    /// Create a new PlayerState with commander-specific starting life.
    pub fn new_commander(starting_life: i32) -> Self {
        let mut state = Self::new();
        state.life = starting_life;
        state
    }
}

/// Combat state tracked during the combat phase.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CombatState {
    /// ObjectIds of creatures declared as attackers.
    pub attackers: Vec<ObjectId>,
    /// Map from blocker ObjectId -> the attacker ObjectId it's blocking.
    pub blockers: HashMap<ObjectId, ObjectId>,
    /// Map from attacker ObjectId -> list of blocker ObjectIds (derived from above).
    pub attacker_blockers: HashMap<ObjectId, Vec<ObjectId>>,
    /// Damage assignment for blocked creatures (attacker -> [(blocker, damage)]).
    pub damage_assignment: HashMap<ObjectId, Vec<(ObjectId, u32)>>,
}

impl CombatState {
    pub fn clear(&mut self) {
        self.attackers.clear();
        self.blockers.clear();
        self.attacker_blockers.clear();
        self.damage_assignment.clear();
    }
}

/// The complete game state — everything needed to determine legal actions and advance the game.
/// This must be cheaply cloneable for MCTS/CFR tree search.
///
/// # Snapshot Contract (Phase 0.3)
///
/// Defines what gets cloned vs. shared vs. reconstructed when `GameState::clone()` is called.
/// This contract ensures MCCFR traversal (which clones millions of states) stays fast while
/// the rules engine can freely add derived/cached fields without breaking the solver.
///
/// ## Always cloned (canonical state)
/// These fields define the unique game position. Two `GameState`s with identical values
/// for all canonical fields represent the same game state:
/// - `objects` — all card instances and their game-specific state
/// - `players` — life, zones (library, hand, graveyard, exile), mana, flags
/// - `battlefield`, `stack` — shared zones
/// - `combat` — attacker/blocker/damage assignment
/// - `pending_triggers` — triggers waiting to be placed on the stack
/// - All scalar fields: `active_player`, `phase`, `priority_player`, `turn_number`,
///   `consecutive_passes`, `next_object_id`, `next_stack_id`, `game_over`, `winner`
///
/// ## Shared via Arc (immutable reference data)
/// - `card_db` — card definitions are immutable after game setup; shared O(1) via `Arc`
///
/// ## Reconstructed after clone (derived / cached state)
/// Future fields that are derivable from canonical state must NOT be part of `Clone`:
/// - Event bus state (Phase 1A) — transient; not part of game state
/// - Continuous effects caches (Phase 2A) — recomputed from canonical state on demand
/// - Dirty flags / memoization caches — local optimization, not state
///
/// **Rule**: Any field added to `GameState` that is derivable from other fields must be
/// marked `#[serde(skip)]` and excluded from equality/hashing. The canonical game state
/// is the minimal set of fields needed to reconstruct the full state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    /// Card definitions (shared, immutable). Wrapped in `Arc` so that
    /// `GameState::clone()` is O(1) for the DB — critical for MCTS/CFR search.
    #[serde(skip)]
    pub card_db: Option<Arc<CardDatabase>>,

    /// Registered combos for macro-action injection (shared, immutable).
    /// When present, `legal_actions_with()` detects available combos and
    /// injects `Action::ActivateMacro` into the legal action list. Shared
    /// via `Arc` for zero-cost clone, same pattern as `card_db`.
    #[serde(skip)]
    pub combo_registry: Option<Arc<ComboRegistry>>,

    /// The game format (Standard or Commander). Determines starting life,
    /// deck construction rules, and whether commander-specific rules apply.
    pub format: GameFormat,

    /// All card instances in the game, keyed by ObjectId.
    pub objects: HashMap<ObjectId, CardInstance>,

    /// The battlefield zone (shared — both players' permanents).
    pub battlefield: Vec<ObjectId>,

    /// The stack.
    pub stack: Vec<StackEntry>,

    /// Player states.
    pub players: Vec<PlayerState>,

    /// Whose turn is it?
    pub active_player: PlayerIndex,

    /// Current phase/step.
    pub phase: Phase,

    /// Which player has priority.
    pub priority_player: PlayerIndex,

    /// Turn number (starts at 1).
    pub turn_number: u32,

    /// Whether both players have passed priority in succession (stack resolves or phase advances).
    pub consecutive_passes: u32,

    /// Combat state (only meaningful during combat).
    pub combat: CombatState,

    /// Next object ID to assign.
    pub next_object_id: ObjectId,

    /// Next stack ID.
    pub next_stack_id: StackId,

    /// Pending triggers waiting to be put on the stack.
    /// These accumulate during rule processing and are placed on the stack
    /// in APNAP order (active player's triggers first) before priority is given.
    pub pending_triggers: Vec<PendingTrigger>,

    /// Where priority resumes after a mandatory trigger-order choice.
    #[serde(default)]
    pub trigger_order_resume: Option<TriggerOrderResume>,

    /// Cleanup had trigger or SBA activity and must repeat after priority passes.
    #[serde(default)]
    pub cleanup_needs_repeat: bool,

    /// The active player is still completing required discards in this cleanup step.
    #[serde(default)]
    pub cleanup_discard_in_progress: bool,

    /// Mandatory choice inside a still-resolving terminal copy batch.
    pub pending_copy_order: Option<PendingCopyOrder>,

    /// Synchronous stack resolution is between trigger detection and the
    /// settlement/placement boundary. A suspended copy order is persisted by
    /// `pending_copy_order`; this transient flag is only needed while code is
    /// actively resolving an entry on the current call stack.
    #[serde(skip)]
    pub(crate) trigger_placement_deferred: bool,

    /// Transient structural position in recursive effect resolution. A terminal
    /// copy batch cannot be started from a child with instructions after it.
    #[serde(skip)]
    pub(crate) effect_terminal_position: Option<bool>,

    /// Active continuous effects on the battlefield (Phase 2A.1).
    ///
    /// Continuous effects from static abilities are regenerated when the
    /// battlefield changes. Effects from resolved spells (until end of turn,
    /// permanent) are tracked here explicitly. The layer engine uses this
    /// list to compute characteristics on demand.
    pub continuous_effects: Vec<ContinuousEffect>,

    /// Next timestamp for continuous effect ordering (CR 613.7).
    pub next_timestamp: u32,

    /// Active replacement effects (Phase 2A.3).
    /// Replacement effects modify or replace events as they happen (CR 614).
    /// Self-replacement effects are applied automatically; competing player-choice
    /// replacements are surfaced as Action::ChooseReplacementOrder.
    pub replacement_effects: Vec<ReplacementEffect>,

    /// Pending tutor — when a SearchLibrary effect resolves and the player
    /// has tutor_targets configured, this is set to indicate the player must
    /// choose a card from their library. `legal_actions` generates
    /// `ChooseTutorTarget` actions until this is resolved.
    pub pending_tutor: Option<PendingTutor>,

    /// Extra turns queue (Phase 3A). When a player takes an extra turn,
    /// they're added to this queue. After the current turn's Cleanup,
    /// if this queue is non-empty, the next turn's active player is
    /// shifted to the player at the front of the queue.
    pub extra_turns: VecDeque<PlayerIndex>,

    /// Number of spells cast this turn (for Storm count).
    /// Reset at the beginning of each turn.
    pub spells_cast_this_turn: u32,

    /// Phases to skip for the current turn (Phase 3A).
    /// When an effect says "skip your draw step" or "skip your combat phase",
    /// the relevant phase is added here. `advance_phase()` checks this set.
    pub skip_phases: HashSet<Phase>,

    /// Game over flag.
    pub game_over: bool,

    /// Winner (if game is over). None = draw.
    pub winner: Option<PlayerIndex>,

    /// Transient event accumulator (Phase 1A.2).
    ///
    /// Events emitted during `apply_action()` and rule processing are
    /// collected here. External code (event bus, test harness) can drain
    /// this vec after each action to process events.
    ///
    /// This field is:
    /// - **NOT serialized** (`serde(skip)`) — events are transient
    /// - **NOT part of the canonical game state** — two states with
    ///   different pending_events but identical canonical fields represent
    ///   the same game position
    /// - **Cheap to clone** — should be empty between actions; any events
    ///   present at clone time are copied but this is O(0) in practice
    #[serde(skip)]
    pub pending_events: Vec<GameEvent>,

    /// Transient cache for `compute_characteristics` results.
    /// Avoids redundant recomputation (~90× per combat step) by caching
    /// the layer-engine output and a `HashSet` for O(1) battlefield membership.
    /// Reset on clone, not serialized, invalidated on canonical-state mutation.
    #[serde(skip)]
    pub characteristics_cache: CharacteristicsCache,
}

/// A trigger that has been queued but not yet placed on the stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTrigger {
    pub source_id: ObjectId,
    pub ability_index: usize,
    pub controller: PlayerIndex,
    pub targets: Vec<Target>,
    /// Captured when the trigger is created, before its source can change.
    pub context: TriggerContext,
}

/// Last-known spell data retained by a cast trigger. The stack ID is historical,
/// not a target and not a requirement that the original remain on the stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastSpellSnapshot {
    pub stack_id: StackId,
    pub definition: Box<CardDef>,
    pub controller: PlayerIndex,
    pub targets: Vec<Target>,
    pub target_generations: Vec<Option<u32>>,
    /// Public identity of each object target at cast time, retained even if
    /// that object later leaves every visible zone. Never a runtime ObjectId.
    pub historical_object_targets: Vec<Option<(crate::card::CardId, PlayerIndex)>>,
}

/// A validated copy with no final stack identity yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedSpellCopy {
    pub(crate) definition: Box<CardDef>,
    pub(crate) controller: PlayerIndex,
    pub(crate) targets: Vec<Target>,
    pub(crate) target_generations: Vec<Option<u32>>,
    pub(crate) target_descriptions: Vec<CopyTargetDescription>,
    pub(crate) definition_description: String,
}

impl PreparedSpellCopy {
    pub fn controller(&self) -> PlayerIndex { self.controller }
    pub fn definition(&self) -> &CardDef { &self.definition }
    pub fn targets(&self) -> &[Target] { &self.targets }
    pub fn target_generations(&self) -> &[Option<u32>] { &self.target_generations }
}

/// Public meaning of a prepared target without a runtime object or stack ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CopyTargetDescription {
    Player(PlayerIndex),
    StackPosition(Option<usize>),
    Permanent {
        card_id: Option<crate::card::CardId>,
        controller: Option<PlayerIndex>,
        observable: Option<ObservablePermanentKey>,
        occurrence: Option<usize>,
        same_incarnation: bool,
    },
}

/// Observable identity used only for a pending copy-order target. Runtime
/// object IDs and zone-change counters are deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObservablePermanentKey {
    pub card_id: crate::card::CardId,
    pub owner: PlayerIndex,
    pub controller: PlayerIndex,
    pub characteristics: String,
    pub tapped: bool,
    pub damage_marked: u32,
    pub summoning_sick: bool,
    pub plus_counters: i32,
    pub minus_counters: i32,
    pub loyalty_counters: u32,
    pub loyalty_activated_this_turn: bool,
    pub is_token: bool,
    pub attached: bool,
    pub attachment_count: usize,
}

/// One controller's mandatory bottom-to-top ordering of a terminal copy batch.
/// The caller must supply the items in a deterministic order across equivalent
/// states; their vector indices are temporary choice identities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingCopyOrder {
    pub(crate) controller: PlayerIndex,
    pub(crate) items: Vec<PreparedSpellCopy>,
    pub(crate) selected_order: Vec<usize>,
    pub(crate) expected_stack_len: usize,
    pub(crate) expected_next_stack_id: StackId,
    pub(crate) resolving_entry: Option<Box<StackEntry>>,
    /// Incarnation of a deferred physical spell when it left the stack.
    #[serde(default)]
    pub(crate) resolving_source_generation: Option<u32>,
}

/// Continuation captured when placing triggers requires controller ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerOrderResume {
    AfterResolution,
    Player(PlayerIndex),
    AfterAttackers,
}

impl PendingCopyOrder {
    pub fn controller(&self) -> PlayerIndex { self.controller }
    pub fn items(&self) -> &[PreparedSpellCopy] { &self.items }
    pub fn selected_order(&self) -> &[usize] { &self.selected_order }
    pub fn resolving_entry(&self) -> Option<&StackEntry> { self.resolving_entry.as_deref() }
}

/// The resolving instruction and public source information owned by a trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerContext {
    pub source_card_id: crate::card::CardId,
    pub source_generation: u32,
    pub effect: crate::card::Effect,
    pub cast_spell: Option<CastSpellSnapshot>,
}

impl PendingTrigger {
    /// Capture a source's instruction now, before the source can leave or change.
    pub fn from_source(
        state: &GameState,
        source_id: ObjectId,
        ability_index: usize,
        controller: PlayerIndex,
        targets: Vec<Target>,
    ) -> Option<Self> {
        let inst = state.objects.get(&source_id)?;
        let ability = state.card_db().get(inst.card_def_id)?.triggered_abilities.get(ability_index)?;
        Some(Self {
            source_id,
            ability_index,
            controller,
            targets,
            context: TriggerContext {
                source_card_id: inst.card_def_id,
                source_generation: inst.zone_change_count,
                effect: ability.effect.clone(),
                cast_spell: None,
            },
        })
    }
}

/// A pending tutor choice — a SearchLibrary effect has resolved and the
/// player must choose which card to find from their library.
///
/// This is surfaced as `Action::ChooseTutorTarget` choices in `legal_actions`,
/// allowing MCCFR to learn the optimal tutor target in each game state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTutor {
    /// Which player is searching their library.
    pub controller: PlayerIndex,
    /// Where the found card goes (Hand, Battlefield, etc.).
    pub destination: crate::card::ZoneType,
    /// When non-empty, only cards with at least one of these subtypes are
    /// valid targets (e.g., fetch lands searching for Forest/Plains).
    pub subtype_filter: Vec<crate::card::Subtype>,
}

// CardDatabase has been moved to crate::card::database and is re-exported above.

// ---------------------------------------------------------------------------
// Phase 3A — GameStateSnapshot for optimized copy/restore
// ---------------------------------------------------------------------------

/// A lightweight snapshot of a GameState for save/restore during deep search.
///
/// Unlike `GameState::clone()`, this captures only the canonical state needed
/// to restore the game position. The `card_db` Arc is shared (not cloned),
/// and derived caches (CharacteristicsCache) are empty on restore.
///
/// Use `GameState::snapshot()` to create and `GameState::restore(snapshot)` to
/// revert. This is cheaper than full clone for MCTS rollback patterns where
/// you save a state, apply several actions, then revert.
#[derive(Clone)]
pub struct GameStateSnapshot {
    format: GameFormat,
    objects: HashMap<ObjectId, CardInstance>,
    battlefield: Vec<ObjectId>,
    stack: Vec<StackEntry>,
    players: Vec<PlayerState>,
    active_player: PlayerIndex,
    phase: Phase,
    priority_player: PlayerIndex,
    turn_number: u32,
    consecutive_passes: u32,
    combat: CombatState,
    next_object_id: ObjectId,
    next_stack_id: StackId,
    pending_triggers: Vec<PendingTrigger>,
    trigger_order_resume: Option<TriggerOrderResume>,
    cleanup_needs_repeat: bool,
    cleanup_discard_in_progress: bool,
    pending_copy_order: Option<PendingCopyOrder>,
    continuous_effects: Vec<ContinuousEffect>,
    next_timestamp: u32,
    replacement_effects: Vec<ReplacementEffect>,
    pending_tutor: Option<PendingTutor>,
    extra_turns: VecDeque<PlayerIndex>,
    spells_cast_this_turn: u32,
    skip_phases: HashSet<Phase>,
    game_over: bool,
    winner: Option<PlayerIndex>,
}

// ---------------------------------------------------------------------------
// Phase 0.1 — Observation API: PlayerView
// ---------------------------------------------------------------------------

/// Everything a player can observe — the information set boundary.
/// The rules engine writes to `GameState`; MCCFR reads through this view.
///
/// `PlayerView` exposes only information that the given player is entitled to
/// see under the MTG rules: public zones (battlefield, graveyard, exile, stack),
/// opponent's hand *size* and library *size* (but not contents), and the
/// player's own private hand.
///
/// When the rules engine adds internal fields (event bus, effects manager,
/// continuous effects cache), MCCFR is insulated — only `visible_state()`
/// needs updating.
pub struct PlayerView<'a> {
    // --- Public information (both players can see) ---
    /// Current game phase/step.
    pub phase: Phase,
    /// Whose turn it is.
    pub active_player: PlayerIndex,
    /// Current turn number (starts at 1).
    pub turn_number: u32,
    /// All permanents on the battlefield (both players).
    pub battlefield: &'a [ObjectId],
    /// The stack (spells and abilities waiting to resolve).
    pub stack: &'a [StackEntry],
    /// Combat state (attackers, blockers, damage assignment).
    pub combat: &'a CombatState,
    /// Triggered abilities waiting to be placed on the stack.
    pub pending_triggers: &'a [PendingTrigger],
    pub trigger_order_resume: Option<TriggerOrderResume>,
    pub cleanup_needs_repeat: bool,
    pub cleanup_discard_in_progress: bool,
    pub pending_copy_order: Option<&'a PendingCopyOrder>,
    /// Which player currently has priority.
    pub priority_player: PlayerIndex,

    // --- Per-player public info ---
    /// Viewing player's life total.
    pub my_life: i32,
    /// Opponent's life total.
    pub opp_life: i32,
    /// Viewing player's graveyard.
    pub my_graveyard: &'a [ObjectId],
    /// Opponent's graveyard.
    pub opp_graveyard: &'a [ObjectId],
    /// Viewing player's exile zone.
    pub my_exile: &'a [ObjectId],
    /// Opponent's exile zone.
    pub opp_exile: &'a [ObjectId],
    /// Number of cards in the opponent's hand (contents hidden).
    pub opp_hand_size: usize,
    /// Number of cards in the opponent's library (contents hidden).
    pub opp_library_size: usize,

    // --- Private information (only the viewing player sees) ---
    /// The viewing player's hand (private — hidden from opponent).
    pub my_hand: &'a [ObjectId],

    // --- Mana ---
    /// Viewing player's current mana pool.
    pub my_mana_pool: &'a ManaPool,
    /// Remaining land plays this turn for the viewing player.
    pub my_land_plays_remaining: u32,

    // --- Commander fields (public in Commander format) ---
    /// Cards in the viewing player's command zone.
    pub my_command_zone: &'a [ObjectId],
    /// Cards in the opponent's command zone.
    pub opp_command_zone: &'a [ObjectId],
    /// How many times the viewing player has cast their commander.
    pub my_commander_tax: u32,
    /// The game format (Standard or Commander).
    pub format: GameFormat,

    // --- Mulligan fields ---
    /// How many times the viewing player has mulliganed.
    pub my_mulligan_count: u32,

    // --- Tutor fields ---
    /// Whether there is a pending tutor choice for the viewing player.
    pub pending_tutor: Option<PendingTutor>,

    // --- Object lookup (filtered, read-only) ---
    /// Card instances visible to the viewing player, keyed by ObjectId.
    /// Includes objects on the battlefield, stack, both graveyards, both exile
    /// zones, the viewing player's hand, and pending trigger sources.
    /// Excludes the opponent's hand contents and both libraries (hidden zones).
    pub objects: HashMap<ObjectId, &'a CardInstance>,
    /// Card definitions database (shared, immutable).
    pub card_db: &'a CardDatabase,
}

impl GameState {
    /// Build a `PlayerView` for the given player, exposing only information
    /// that player is entitled to see under the MTG rules.
    ///
    /// The `objects` map is filtered to only include card instances in visible
    /// zones: battlefield, stack, both graveyards, both exile zones, the viewing
    /// player's hand, pending trigger sources, and combat participants.
    /// Opponent hand contents and both libraries are excluded.
    pub fn visible_state(&self, player: PlayerIndex) -> PlayerView<'_> {
        let opp = self.opponent(player);

        // Collect ObjectIds from all visible zones into the filtered objects map.
        let mut visible = HashMap::new();

        // Battlefield — public
        for &id in &self.battlefield {
            if let Some(inst) = self.objects.get(&id) {
                visible.insert(id, inst);
            }
        }
        // Stack — spells/abilities are public
        for entry in &self.stack {
            let source_id = match entry.source {
                StackSource::Spell(id) => Some(id),
                StackSource::ActivatedAbility { source_id, .. } => Some(source_id),
                StackSource::TriggeredAbility { source_id, .. } => Some(source_id),
                StackSource::SpellCopy { .. } => None,
            };
            if let Some(source_id) = source_id {
                if let Some(inst) = self.objects.get(&source_id) {
                    visible.insert(source_id, inst);
                }
            }
        }
        if let Some(entry) = self.pending_copy_order.as_ref().and_then(PendingCopyOrder::resolving_entry) {
            if let StackSource::Spell(source_id) = &entry.source {
                if let Some(inst) = self.objects.get(source_id) { visible.insert(*source_id, inst); }
            }
        }
        // All graveyards — public
        for p in &self.players {
            for &id in &p.graveyard {
                if let Some(inst) = self.objects.get(&id) {
                    visible.insert(id, inst);
                }
            }
        }
        // All exile zones — public
        for p in &self.players {
            for &id in &p.exile {
                if let Some(inst) = self.objects.get(&id) {
                    visible.insert(id, inst);
                }
            }
        }
        // Viewing player's hand — private to this player
        for &id in &self.players[player].hand {
            if let Some(inst) = self.objects.get(&id) {
                visible.insert(id, inst);
            }
        }
        // Pending trigger sources — visible (they reference battlefield permanents)
        for trigger in &self.pending_triggers {
            if let Some(inst) = self.objects.get(&trigger.source_id) {
                visible.insert(trigger.source_id, inst);
            }
        }
        // Command zones — public (both players' commanders are visible)
        for p in &self.players {
            for &id in &p.command_zone {
                if let Some(inst) = self.objects.get(&id) {
                    visible.insert(id, inst);
                }
            }
        }
        // Combat participants — attackers and blockers (both keys and values)
        for &id in &self.combat.attackers {
            if let Some(inst) = self.objects.get(&id) {
                visible.insert(id, inst);
            }
        }
        for (&blocker, &attacker) in &self.combat.blockers {
            if let Some(inst) = self.objects.get(&blocker) {
                visible.insert(blocker, inst);
            }
            if let Some(inst) = self.objects.get(&attacker) {
                visible.insert(attacker, inst);
            }
        }

        PlayerView {
            phase: self.phase,
            active_player: self.active_player,
            turn_number: self.turn_number,
            battlefield: &self.battlefield,
            stack: &self.stack,
            combat: &self.combat,
            pending_triggers: &self.pending_triggers,
            trigger_order_resume: self.trigger_order_resume,
            cleanup_needs_repeat: self.cleanup_needs_repeat,
            cleanup_discard_in_progress: self.cleanup_discard_in_progress,
            pending_copy_order: self.pending_copy_order.as_ref(),
            priority_player: self.priority_player,

            my_life: self.players[player].life,
            opp_life: self.players[opp].life,
            my_graveyard: &self.players[player].graveyard,
            opp_graveyard: &self.players[opp].graveyard,
            my_exile: &self.players[player].exile,
            opp_exile: &self.players[opp].exile,
            opp_hand_size: self.players[opp].hand.len(),
            opp_library_size: self.players[opp].library.len(),

            my_hand: &self.players[player].hand,

            my_mana_pool: &self.players[player].mana_pool,
            my_land_plays_remaining: self.players[player].land_plays_remaining,

            my_command_zone: &self.players[player].command_zone,
            opp_command_zone: &self.players[opp].command_zone,
            my_commander_tax: self.players[player].commander_tax,
            format: self.format,

            my_mulligan_count: self.players[player].mulligan_count,

            pending_tutor: self.pending_tutor.clone(),

            objects: visible,
            card_db: self.card_db(),
        }
    }
}

impl GameState {
    pub fn new(num_players: usize) -> Self {
        GameState {
            card_db: None,
            combo_registry: None,
            format: GameFormat::Standard,
            objects: HashMap::new(),
            battlefield: Vec::new(),
            stack: Vec::new(),
            players: (0..num_players).map(|_| PlayerState::new()).collect(),
            active_player: 0,
            phase: Phase::Untap,
            priority_player: 0,
            turn_number: 1,
            consecutive_passes: 0,
            combat: CombatState::default(),
            next_object_id: 1,
            next_stack_id: 1,
            pending_triggers: Vec::new(),
            trigger_order_resume: None,
            cleanup_needs_repeat: false,
            cleanup_discard_in_progress: false,
            pending_copy_order: None,
            trigger_placement_deferred: false,
            effect_terminal_position: None,
            continuous_effects: Vec::new(),
            next_timestamp: 1,
            replacement_effects: Vec::new(),
            pending_tutor: None,
            extra_turns: VecDeque::new(),
            spells_cast_this_turn: 0,
            skip_phases: HashSet::new(),
            game_over: false,
            winner: None,
            pending_events: Vec::new(),
            characteristics_cache: CharacteristicsCache::default(),
        }
    }

    /// Create a new game state configured for Commander format.
    pub fn new_commander(num_players: usize) -> Self {
        let mut state = GameState {
            card_db: None,
            combo_registry: None,
            format: GameFormat::Commander,
            objects: HashMap::new(),
            battlefield: Vec::new(),
            stack: Vec::new(),
            players: (0..num_players)
                .map(|_| PlayerState::new_commander(40))
                .collect(),
            active_player: 0,
            phase: Phase::Untap,
            priority_player: 0,
            turn_number: 1,
            consecutive_passes: 0,
            combat: CombatState::default(),
            next_object_id: 1,
            next_stack_id: 1,
            pending_triggers: Vec::new(),
            trigger_order_resume: None,
            cleanup_needs_repeat: false,
            cleanup_discard_in_progress: false,
            pending_copy_order: None,
            trigger_placement_deferred: false,
            effect_terminal_position: None,
            continuous_effects: Vec::new(),
            next_timestamp: 1,
            replacement_effects: Vec::new(),
            pending_tutor: None,
            extra_turns: VecDeque::new(),
            spells_cast_this_turn: 0,
            skip_phases: HashSet::new(),
            game_over: false,
            winner: None,
            pending_events: Vec::new(),
            characteristics_cache: CharacteristicsCache::default(),
        };
        // Initialize commander damage tracking (each player tracks damage from each opponent)
        for i in 0..num_players {
            state.players[i].commander_damage_received = vec![0; num_players];
        }
        state
    }

    /// Whether this game uses Commander format rules.
    pub fn is_commander_format(&self) -> bool {
        self.format == GameFormat::Commander
    }

    pub fn card_db(&self) -> &CardDatabase {
        self.card_db.as_ref().expect("CardDatabase not set on GameState")
    }

    /// Create a lightweight snapshot of the current game state for later restore.
    /// Cheaper than `clone()` for rollback patterns since it doesn't carry
    /// the Arc<CardDatabase> or characteristics cache.
    pub fn snapshot(&self) -> GameStateSnapshot {
        GameStateSnapshot {
            format: self.format,
            objects: self.objects.clone(),
            battlefield: self.battlefield.clone(),
            stack: self.stack.clone(),
            players: self.players.clone(),
            active_player: self.active_player,
            phase: self.phase,
            priority_player: self.priority_player,
            turn_number: self.turn_number,
            consecutive_passes: self.consecutive_passes,
            combat: self.combat.clone(),
            next_object_id: self.next_object_id,
            next_stack_id: self.next_stack_id,
            pending_triggers: self.pending_triggers.clone(),
            trigger_order_resume: self.trigger_order_resume,
            cleanup_needs_repeat: self.cleanup_needs_repeat,
            cleanup_discard_in_progress: self.cleanup_discard_in_progress,
            pending_copy_order: self.pending_copy_order.clone(),
            continuous_effects: self.continuous_effects.clone(),
            next_timestamp: self.next_timestamp,
            replacement_effects: self.replacement_effects.clone(),
            pending_tutor: self.pending_tutor.clone(),
            extra_turns: self.extra_turns.clone(),
            spells_cast_this_turn: self.spells_cast_this_turn,
            skip_phases: self.skip_phases.clone(),
            game_over: self.game_over,
            winner: self.winner,
        }
    }

    /// Restore game state from a snapshot, keeping the current card_db.
    /// Invalidates all caches.
    pub fn restore(&mut self, snap: GameStateSnapshot) {
        self.format = snap.format;
        self.objects = snap.objects;
        self.battlefield = snap.battlefield;
        self.stack = snap.stack;
        self.players = snap.players;
        self.active_player = snap.active_player;
        self.phase = snap.phase;
        self.priority_player = snap.priority_player;
        self.turn_number = snap.turn_number;
        self.consecutive_passes = snap.consecutive_passes;
        self.combat = snap.combat;
        self.next_object_id = snap.next_object_id;
        self.next_stack_id = snap.next_stack_id;
        self.pending_triggers = snap.pending_triggers;
        self.trigger_order_resume = snap.trigger_order_resume;
        self.cleanup_needs_repeat = snap.cleanup_needs_repeat;
        self.cleanup_discard_in_progress = snap.cleanup_discard_in_progress;
        self.pending_copy_order = snap.pending_copy_order;
        self.trigger_placement_deferred = false;
        self.effect_terminal_position = None;
        self.continuous_effects = snap.continuous_effects;
        self.next_timestamp = snap.next_timestamp;
        self.replacement_effects = snap.replacement_effects;
        self.pending_tutor = snap.pending_tutor;
        self.extra_turns = snap.extra_turns;
        self.spells_cast_this_turn = snap.spells_cast_this_turn;
        self.skip_phases = snap.skip_phases;
        self.game_over = snap.game_over;
        self.winner = snap.winner;
        self.pending_events.clear();
        self.invalidate_characteristics_cache();
    }

    /// Allocate a new unique ObjectId.
    pub fn new_object_id(&mut self) -> ObjectId {
        let id = self.next_object_id;
        self.next_object_id += 1;
        id
    }

    /// Allocate a new stack entry ID.
    pub fn new_stack_id(&mut self) -> StackId {
        let id = self.next_stack_id;
        self.next_stack_id = self.next_stack_id.checked_add(1).expect("StackId exhausted");
        id
    }

    /// Create a card instance and put it in the specified zone.
    pub fn create_card_in_zone(
        &mut self,
        card_id: CardId,
        owner: PlayerIndex,
        zone: ZoneType,
    ) -> ObjectId {
        let obj_id = self.new_object_id();
        let instance = CardInstance::new(obj_id, card_id, owner);
        self.objects.insert(obj_id, instance);

        match zone {
            ZoneType::Library => self.players[owner].library.push(obj_id),
            ZoneType::Hand => self.players[owner].hand.push(obj_id),
            ZoneType::Battlefield => self.battlefield.push(obj_id),
            ZoneType::Graveyard => self.players[owner].graveyard.push(obj_id),
            ZoneType::Exile => self.players[owner].exile.push(obj_id),
            ZoneType::Stack => {} // handled separately
            ZoneType::Command => self.players[owner].command_zone.push(obj_id),
        }
        obj_id
    }

    /// Move a card instance from one zone to another.
    ///
    /// # Commander redirect (GTO simplification)
    ///
    /// In Commander format, when a commander would move to graveyard or exile
    /// from anywhere, it is redirected to the command zone instead.
    ///
    /// **Note:** As of the 2024 rules update, CR 903.9a makes this a player
    /// choice — the commander's owner may choose to let it go to graveyard/exile
    /// instead. We always redirect to the command zone because in a GTO
    /// (Game Theory Optimal) context, returning the commander to the command
    /// zone is nearly always the dominant strategy: the commander remains
    /// accessible for re-casting, and the marginal value of a commander in
    /// graveyard/exile (e.g., for delve, escape, or reanimate) is rarely worth
    /// giving up command zone access. If future strategies require modeling
    /// this choice, surface it as an `Action::ChooseCommanderZone` decision
    /// point for the solver.
    pub fn move_object(
        &mut self,
        obj_id: ObjectId,
        from: ZoneType,
        to: ZoneType,
    ) {
        // Commander redirect: graveyard/exile -> command zone (see doc above)
        let actual_to = if self.format == GameFormat::Commander
            && (to == ZoneType::Graveyard || to == ZoneType::Exile)
            && self.is_commander(obj_id)
        {
            ZoneType::Command
        } else {
            to
        };

        self.invalidate_characteristics_cache();

        // Emit zone change event
        self.emit_event(GameEvent::ZoneChange {
            object: obj_id,
            from: crate::events::Zone::from(from),
            to: crate::events::Zone::from(actual_to),
        });

        // Increment zone-change counter (CR 400.7)
        if let Some(inst) = self.objects.get_mut(&obj_id) {
            inst.zone_change_count += 1;
        }

        // When a permanent leaves the battlefield, move all cards exiled by it
        // to their owner's graveyard (e.g., Gustha's Scepter, Tidehollow Sculler).
        if from == ZoneType::Battlefield {
            let linked_exiles: Vec<(ObjectId, usize)> = self.players.iter().enumerate()
                .flat_map(|(pi, p)| {
                    p.exile.iter()
                        .filter(|&&eid| self.objects.get(&eid).and_then(|i| i.exiled_by) == Some(obj_id))
                        .map(move |&eid| (eid, pi))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (eid, player_idx) in linked_exiles {
                if let Some(inst) = self.objects.get_mut(&eid) {
                    inst.exiled_by = None;
                    inst.zone_change_count += 1;
                }
                self.emit_event(GameEvent::ZoneChange {
                    object: eid,
                    from: crate::events::Zone::Exile,
                    to: crate::events::Zone::Graveyard,
                });
                self.players[player_idx].exile.retain(|&id| id != eid);
                let owner_idx = self.objects[&eid].owner;
                self.players[owner_idx].graveyard.push(eid);
            }
        }

        // Remove from all zones (brute force but correct)
        let owner = self.objects[&obj_id].owner;
        let controller = self.objects[&obj_id].controller;

        self.players[owner].library.retain(|&id| id != obj_id);
        self.players[owner].hand.retain(|&id| id != obj_id);
        self.players[owner].graveyard.retain(|&id| id != obj_id);
        self.players[owner].exile.retain(|&id| id != obj_id);
        self.players[owner].command_zone.retain(|&id| id != obj_id);
        // Also check controller's zones if different
        if controller != owner {
            self.players[controller].library.retain(|&id| id != obj_id);
            self.players[controller].hand.retain(|&id| id != obj_id);
            self.players[controller].graveyard.retain(|&id| id != obj_id);
            self.players[controller].exile.retain(|&id| id != obj_id);
            self.players[controller].command_zone.retain(|&id| id != obj_id);
        }
        self.battlefield.retain(|&id| id != obj_id);
        self.stack.retain(|e| {
            if let StackSource::Spell(spell_id) = e.source {
                spell_id != obj_id
            } else {
                true
            }
        });

        // CR 111.7: Tokens that leave the battlefield cease to exist.
        // They briefly visit the destination zone then are removed.
        let is_token = self.objects.get(&obj_id).map_or(false, |i| i.is_token);
        if is_token && actual_to != ZoneType::Battlefield {
            // Token ceases to exist — remove it entirely
            self.objects.remove(&obj_id);
            return;
        }

        // Add to destination zone
        match actual_to {
            ZoneType::Library => self.players[owner].library.push(obj_id),
            ZoneType::Hand => self.players[owner].hand.push(obj_id),
            ZoneType::Battlefield => {
                // Reset battlefield state when entering
                let enters_tapped = self.card_db()
                    .get(self.objects[&obj_id].card_def_id)
                    .map_or(false, |d| d.enters_tapped);
                if let Some(inst) = self.objects.get_mut(&obj_id) {
                    inst.tapped = enters_tapped;
                    inst.summoning_sick = true;
                    inst.damage_marked = 0;
                    inst.temp_power_mod = 0;
                    inst.temp_toughness_mod = 0;
                    inst.temp_keywords.clear();
                }
                self.battlefield.push(obj_id);
            }
            ZoneType::Graveyard => self.players[owner].graveyard.push(obj_id),
            ZoneType::Exile => self.players[owner].exile.push(obj_id),
            ZoneType::Stack => {} // handled by cast_spell
            ZoneType::Command => self.players[owner].command_zone.push(obj_id),
        }
    }

    /// Get all permanent ObjectIds on the battlefield controlled by a given player.
    pub fn permanents_controlled_by(&self, player: PlayerIndex) -> Vec<ObjectId> {
        self.battlefield
            .iter()
            .copied()
            .filter(|&id| self.objects[&id].controller == player)
            .collect()
    }

    /// Get all creature ObjectIds on the battlefield controlled by a given player.
    pub fn creatures_controlled_by(&self, player: PlayerIndex) -> Vec<ObjectId> {
        let db = self.card_db();
        self.battlefield
            .iter()
            .copied()
            .filter(|&id| {
                let inst = &self.objects[&id];
                inst.controller == player
                    && db.get(inst.card_def_id).map_or(false, |d| d.is_creature())
            })
            .collect()
    }

    /// Get lands on the battlefield controlled by a given player.
    pub fn lands_controlled_by(&self, player: PlayerIndex) -> Vec<ObjectId> {
        let db = self.card_db();
        self.battlefield
            .iter()
            .copied()
            .filter(|&id| {
                let inst = &self.objects[&id];
                inst.controller == player
                    && db.get(inst.card_def_id).map_or(false, |d| d.is_land())
            })
            .collect()
    }

    /// Get untapped lands controlled by a player.
    pub fn untapped_lands(&self, player: PlayerIndex) -> Vec<ObjectId> {
        self.lands_controlled_by(player)
            .into_iter()
            .filter(|&id| !self.objects[&id].tapped)
            .collect()
    }

    /// Get all untapped permanents with mana abilities controlled by a player
    /// (lands, mana rocks, mana dorks, etc.).
    pub fn untapped_mana_sources(&self, player: PlayerIndex) -> Vec<ObjectId> {
        let db = self.card_db();
        self.battlefield
            .iter()
            .copied()
            .filter(|&id| {
                let inst = &self.objects[&id];
                inst.controller == player
                    && self.can_pay_tap_cost(id)
                    && db
                        .get(inst.card_def_id)
                        .map_or(false, |d| !d.mana_abilities.is_empty())
            })
            .collect()
    }

    /// The opponent of the given player (backward-compatible 2-player convenience).
    /// For multiplayer, returns the first opponent in clockwise order.
    pub fn opponent(&self, player: PlayerIndex) -> PlayerIndex {
        self.opponents(player).into_iter().next().unwrap_or(0)
    }

    /// All opponents of the given player (all non-eliminated players except self).
    pub fn opponents(&self, player: PlayerIndex) -> Vec<PlayerIndex> {
        (0..self.players.len())
            .filter(|&i| i != player && !self.players[i].has_lost)
            .collect()
    }

    /// Next player in clockwise turn order, skipping eliminated players.
    /// Wraps around. Returns `player` if no other active players exist.
    pub fn next_player(&self, player: PlayerIndex) -> PlayerIndex {
        let n = self.players.len();
        for offset in 1..=n {
            let next = (player + offset) % n;
            if !self.players[next].has_lost {
                return next;
            }
        }
        player // fallback: only player left
    }

    /// Number of active (non-eliminated) players.
    pub fn active_player_count(&self) -> usize {
        self.players.iter().filter(|p| !p.has_lost).count()
    }

    /// Emit a game event to the transient event accumulator.
    /// Events are collected during rule processing and can be drained
    /// by external code (event bus, tests) after each action.
    pub fn emit_event(&mut self, event: GameEvent) {
        self.pending_events.push(event);
    }

    /// Drain all pending events, returning them for processing.
    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.pending_events)
    }

    /// Check if a player has lost.
    pub fn check_player_lost(&self, player: PlayerIndex) -> bool {
        self.players[player].life <= 0
            || self.players[player].has_lost
    }

    /// Allocate a new timestamp for continuous effect ordering.
    pub fn new_timestamp(&mut self) -> u32 {
        let ts = self.next_timestamp;
        self.next_timestamp += 1;
        ts
    }

    /// Refresh continuous effects from static abilities on the battlefield.
    /// This regenerates effects from permanents with static abilities,
    /// preserving any non-static effects (from spells, until-end-of-turn, etc.).
    ///
    /// Uses the two-phase read-write pattern to satisfy the borrow checker:
    /// Phase 1 collects what needs to be added (read-only), Phase 2 mutates.
    pub fn refresh_continuous_effects(&mut self) {
        self.invalidate_characteristics_cache();

        // Remove effects whose source has left the relevant zone
        let bf: HashSet<ObjectId> = self.battlefield.iter().copied().collect();
        let gy: HashSet<ObjectId> = self.players.iter()
            .flat_map(|p| p.graveyard.iter().copied())
            .collect();
        self.continuous_effects.retain(|e| {
            match e.duration {
                crate::layers::Duration::WhileSourceOnBattlefield => {
                    bf.contains(&e.source_id)
                }
                crate::layers::Duration::WhileSourceInGraveyard => {
                    gy.contains(&e.source_id)
                }
                _ => true, // UntilEndOfTurn and Permanent effects persist
            }
        });

        // Phase 1: Read — collect new effects to add
        let existing_static_sources: std::collections::HashSet<ObjectId> = self
            .continuous_effects
            .iter()
            .filter(|e| e.duration == crate::layers::Duration::WhileSourceOnBattlefield)
            .map(|e| e.source_id)
            .collect();

        let mut new_effects = Vec::new();
        let mut ts = self.next_timestamp;
        {
            let db = self.card_db();
            for &obj_id in &self.battlefield {
                if existing_static_sources.contains(&obj_id) {
                    continue;
                }
                let inst = match self.objects.get(&obj_id) {
                    Some(i) => i,
                    None => continue,
                };
                let def = match db.get(inst.card_def_id) {
                    Some(d) => d,
                    None => continue,
                };
                for sa in &def.static_abilities {
                    let generated = sa.to_continuous_effects(obj_id, inst.controller, ts);
                    ts += 1;
                    new_effects.extend(generated);
                }
            }
        }

        // Phase 1b: Check graveyards for WonderInGraveyard static abilities.
        // These generate flying grants while the source is in a graveyard.
        let existing_gy_sources: std::collections::HashSet<ObjectId> = self
            .continuous_effects
            .iter()
            .filter(|e| e.duration == crate::layers::Duration::WhileSourceInGraveyard)
            .map(|e| e.source_id)
            .collect();
        {
            let db = self.card_db();
            for (pi, player) in self.players.iter().enumerate() {
                for &obj_id in &player.graveyard {
                    if existing_gy_sources.contains(&obj_id) {
                        continue;
                    }
                    let inst = match self.objects.get(&obj_id) {
                        Some(i) => i,
                        None => continue,
                    };
                    let def = match db.get(inst.card_def_id) {
                        Some(d) => d,
                        None => continue,
                    };
                    for sa in &def.static_abilities {
                        if matches!(sa, crate::layers::StaticAbility::WonderInGraveyard) {
                            // Check if controller has an Island (or all lands are Islands
                            // via Prismatic Omen / Dryad). Simplified: always grant flying
                            // in this deck since Prismatic Omen makes all lands Islands.
                            new_effects.push(crate::layers::ContinuousEffect {
                                source_id: obj_id,
                                controller: pi,
                                timestamp: ts,
                                duration: crate::layers::Duration::WhileSourceInGraveyard,
                                affected: crate::layers::AffectedObjects::CreaturesControlledBy(pi),
                                modification: crate::layers::LayerModification::AddKeyword(
                                    crate::card::KeywordAbility::Flying,
                                ),
                            });
                            ts += 1;
                        }
                    }
                }
            }
        }

        // Phase 2: Write — apply collected effects
        self.next_timestamp = ts;
        self.continuous_effects.extend(new_effects);
    }

    /// Remove all UntilEndOfTurn continuous effects (called during cleanup).
    pub fn cleanup_eot_effects(&mut self) {
        self.invalidate_characteristics_cache();
        self.continuous_effects
            .retain(|e| e.duration != crate::layers::Duration::UntilEndOfTurn);
    }

    /// Get the computed characteristics for an object, using the transient cache
    /// to avoid redundant recomputation within a single game step.
    ///
    /// On a cache miss the battlefield `HashSet` is built once (amortised across
    /// all objects in the same cache epoch) and passed to the layer engine for
    /// O(1) membership tests.
    pub(crate) fn get_characteristics(&self, obj_id: ObjectId) -> Option<ComputedCharacteristics> {
        let mut cache = self.characteristics_cache.0.lock().unwrap();

        // Fast path: cache hit
        if let Some(cached) = cache.entries.get(&obj_id) {
            return Some(cached.clone());
        }

        // Ensure battlefield HashSet is built (once per cache epoch)
        if cache.battlefield_set.is_none() {
            cache.battlefield_set = Some(self.battlefield.iter().copied().collect());
        }

        // Build DynamicContext for this object's controller (needed for
        // DynamicValue::CardsInHand and CardTypesInGraveyards).
        let dyn_ctx = self.objects.get(&obj_id).map(|inst| {
            let controller = inst.controller;
            let hand_size = self.players.get(controller)
                .map(|p| p.hand.len())
                .unwrap_or(0);
            let db = self.card_db();
            let mut graveyard_card_types = Vec::new();
            for player in &self.players {
                for &gid in &player.graveyard {
                    if let Some(gi) = self.objects.get(&gid) {
                        if let Some(gdef) = db.get(gi.card_def_id) {
                            graveyard_card_types.push(gdef.card_types.clone());
                        }
                    }
                }
            }
            let creatures_in_graveyard = graveyard_card_types.iter()
                .filter(|types| types.contains(&crate::card::CardType::Creature))
                .count();
            crate::card::DynamicContext { hand_size, graveyard_card_types, creatures_in_graveyard }
        });

        // Compute characteristics with the cached battlefield set
        let bf_set = cache.battlefield_set.as_ref().unwrap();
        let result = crate::layers::compute_characteristics_with_ctx(
            obj_id,
            &self.continuous_effects,
            &self.objects,
            bf_set,
            self.card_db(),
            dyn_ctx.as_ref(),
        );

        if let Some(ref chars) = result {
            cache.entries.insert(obj_id, chars.clone());
        }

        result
    }

    /// Invalidate the characteristics cache.
    ///
    /// Must be called whenever canonical state that affects characteristics
    /// changes: battlefield membership, continuous effects, or object
    /// counters/controller.
    pub fn invalidate_characteristics_cache(&self) {
        let mut cache = self.characteristics_cache.0.lock().unwrap();
        cache.entries.clear();
        cache.battlefield_set = None;
    }

    /// Compute the effective power of a creature using the layer engine.
    pub fn effective_power(&self, obj_id: ObjectId) -> i32 {
        self.get_characteristics(obj_id)
            .map(|c| c.power)
            .unwrap_or(0)
    }

    /// Compute the effective toughness of a creature using the layer engine.
    pub fn effective_toughness(&self, obj_id: ObjectId) -> i32 {
        self.get_characteristics(obj_id)
            .map(|c| c.toughness)
            .unwrap_or(0)
    }

    /// Whether a permanent can pay a tap-symbol cost (CR 302.6).
    pub fn can_pay_tap_cost(&self, obj_id: ObjectId) -> bool {
        self.battlefield.contains(&obj_id) && self.objects.get(&obj_id).map_or(false, |inst| {
            !inst.tapped && (!self.is_creature(obj_id) || !inst.summoning_sick
                || self.has_keyword(obj_id, crate::card::KeywordAbility::Haste))
        })
    }

    /// Evaluate a dynamic effect value against current permanent characteristics.
    pub fn evaluate_dynamic_value(
        &self,
        value: &crate::card::DynamicValue,
        controller: PlayerIndex,
        context: Option<&crate::card::DynamicContext>,
    ) -> i32 {
        if let crate::card::DynamicValue::PermanentsWithSubtype(subtype)
            | crate::card::DynamicValue::AllPermanentsWithSubtype(subtype) = value
        {
            return self.battlefield.iter().filter(|&&id| {
                self.get_characteristics(id).map_or(false, |c| {
                    (matches!(value, crate::card::DynamicValue::AllPermanentsWithSubtype(_))
                        || c.controller == controller) && (c.subtypes.iter().any(|s| s.0 == *subtype)
                        || c.keywords.contains(&crate::card::KeywordAbility::Changeling))
                })
            }).count() as i32;
        }
        value.evaluate(controller, &self.objects, &self.battlefield,
            &|id| self.card_db().get(id), context)
    }

    /// Check if an object has a keyword ability using the layer engine.
    pub fn has_keyword(&self, obj_id: ObjectId, kw: crate::card::KeywordAbility) -> bool {
        self.get_characteristics(obj_id)
            .map(|c| c.keywords.contains(&kw))
            .unwrap_or(false)
    }

    /// Check if a permanent has a given creature subtype, respecting Changeling.
    /// Changelings have every creature type.
    pub fn has_subtype(&self, obj_id: ObjectId, subtype: &str) -> bool {
        if self.has_keyword(obj_id, crate::card::KeywordAbility::Changeling) {
            return true;
        }
        let db = self.card_db();
        self.objects.get(&obj_id).and_then(|inst| {
            db.get(inst.card_def_id).map(|def| {
                def.subtypes.iter().any(|s| s.0 == subtype)
            })
        }).unwrap_or(false)
    }

    /// Apply damage with replacement effects (CR 614).
    ///
    /// Checks for damage-replacement effects before dealing damage.
    /// Self-replacement effects are applied automatically. If multiple
    /// non-self replacements apply, the affected player chooses the order
    /// (surfaced via Action::ChooseReplacementOrder in a future iteration).
    ///
    /// Returns the actual damage dealt after replacements.
    pub fn deal_damage_with_replacement(&mut self, amount: u32, target: &Target) -> u32 {
        let (self_replacements, _player_choice) =
            crate::replacement::find_applicable_replacements(
                &self.replacement_effects,
                &ReplacementEventKind::DamageDealt,
                match target {
                    Target::StackEntry(_) => return 0,
                    Target::Player(p) => *p,
                    Target::Object(id) => self.objects.get(id).map(|i| i.controller).unwrap_or(0),
                },
            );

        let mut effective_amount = amount as i32;

        // Apply self-replacement effects automatically
        for &idx in &self_replacements {
            if let Some(effect) = self.replacement_effects.get(idx) {
                match &effect.action {
                    ReplacementAction::Prevent => {
                        effective_amount = 0;
                    }
                    ReplacementAction::ModifyAmount { delta } => {
                        effective_amount += delta;
                    }
                    _ => {}
                }
            }
        }

        // TODO: handle player-choice replacement effects (ChooseReplacementOrder)
        // For now, apply them in order
        // (CR 614.5: each effect applies only once per event)

        effective_amount.max(0) as u32
    }

    /// Check for death replacement effects (CR 614).
    ///
    /// Returns the zone to send the dying creature to (normally Graveyard,
    /// but can be Exile or other zones if a replacement applies).
    pub fn death_replacement_zone(&self, obj_id: ObjectId) -> ZoneType {
        let controller = self
            .objects
            .get(&obj_id)
            .map(|i| i.controller)
            .unwrap_or(0);

        let (self_replacements, _player_choice) =
            crate::replacement::find_applicable_replacements(
                &self.replacement_effects,
                &ReplacementEventKind::WouldDie,
                controller,
            );

        // Apply self-replacement effects first
        for &idx in &self_replacements {
            if let Some(effect) = self.replacement_effects.get(idx) {
                match &effect.action {
                    ReplacementAction::RedirectToZone(zone) => {
                        return *zone;
                    }
                    ReplacementAction::Prevent => {
                        // Prevented death means the creature stays on the battlefield.
                        // This is a special case — we return Battlefield to signal "don't move".
                        return ZoneType::Battlefield;
                    }
                    _ => {}
                }
            }
        }

        // TODO: handle player-choice replacement effects

        ZoneType::Graveyard
    }

    /// Check for ETB replacement effects and apply them (CR 614).
    ///
    /// Applies modifications like "enters tapped" or "enters with counters".
    pub fn apply_etb_replacements(&mut self, obj_id: ObjectId) {
        let controller = self
            .objects
            .get(&obj_id)
            .map(|i| i.controller)
            .unwrap_or(0);

        let (self_replacements, _player_choice) =
            crate::replacement::find_applicable_replacements(
                &self.replacement_effects,
                &ReplacementEventKind::EntersBattlefield,
                controller,
            );

        let mut counters_changed = false;
        for &idx in &self_replacements {
            if let Some(effect) = self.replacement_effects.get(idx).cloned() {
                match &effect.action {
                    ReplacementAction::EntersModified {
                        enters_tapped,
                        extra_counters,
                    } => {
                        if let Some(inst) = self.objects.get_mut(&obj_id) {
                            if *enters_tapped {
                                inst.tapped = true;
                            }
                            if *extra_counters > 0 {
                                inst.plus_counters += extra_counters;
                                counters_changed = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if counters_changed {
            self.invalidate_characteristics_cache();
        }
    }

    /// Refresh replacement effects based on the current battlefield.
    /// Removes effects whose source has left the battlefield.
    pub fn refresh_replacement_effects(&mut self) {
        let bf: HashSet<ObjectId> = self.battlefield.iter().copied().collect();
        self.replacement_effects
            .retain(|e| bf.contains(&e.source_id));
    }

    /// Check if the given object is a player's commander.
    ///
    /// Checks by object identity (`commander_object_id`) when available,
    /// which is robust against clone effects. Falls back to `card_def_id`
    /// matching for states set up without `commander_object_id`.
    pub fn is_commander(&self, obj_id: ObjectId) -> bool {
        if self.format != GameFormat::Commander {
            return false;
        }
        // Prefer object identity check (primary + partner)
        for player in &self.players {
            if player.commander_object_id == Some(obj_id) {
                return true;
            }
            if player.partner_commander_object_id == Some(obj_id) {
                return true;
            }
        }
        // Fallback: card_def_id match (for backwards compatibility)
        if let Some(inst) = self.objects.get(&obj_id) {
            let owner = inst.owner;
            if self.players[owner].commander_object_id.is_none() {
                return self.players[owner].commander_card_id == Some(inst.card_def_id);
            }
            if self.players[owner].partner_commander_object_id.is_none() {
                if self.players[owner].partner_commander_card_id == Some(inst.card_def_id) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if the given object is in a player's command zone.
    pub fn is_in_command_zone(&self, obj_id: ObjectId) -> bool {
        for player in &self.players {
            if player.command_zone.contains(&obj_id) {
                return true;
            }
        }
        false
    }

    /// Check if an object is a creature using the layer engine.
    pub fn is_creature(&self, obj_id: ObjectId) -> bool {
        self.get_characteristics(obj_id)
            .map(|c| c.card_types.contains(&crate::card::CardType::Creature))
            .unwrap_or(false)
    }
}
