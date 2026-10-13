mod wire;
mod commander;
pub use commander::{CommanderDesignation, CommanderRecord, PublicCommander};
mod movement_choice;
pub use movement_choice::{LibraryPlacement, MovementReplacementCandidate, MovementProposal, MovementDecision, MovementDecisionRecord, PendingMovement, PendingChittering, BoundedMovementInstruction};
mod commander_choice;
pub use commander_choice::{CommanderArrival, CommanderSbaChoice, PublicCommanderSbaChoice};
mod refresh;
pub(crate) use refresh::{PreparedContinuousRefresh, RefreshProjection};
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
    /// A paid Equip activation. Endpoint definitions remain public after departure.
    EquipAbility {
        source: crate::card::ExactObjectRef,
        source_card_id: crate::card::CardId,
        target_card_id: crate::card::CardId,
    },
    /// Physical ordinary hand-cast enchant-creature spell. Owns endpoint facts.
    AuraSpell {
        source: crate::card::ExactObjectRef,
        source_card_id: CardId,
        source_owner: PlayerIndex,
        target: crate::card::ExactObjectRef,
        target_card_id: CardId,
        target_owner: PlayerIndex,
        target_controller: PlayerIndex,
        target_is_token: bool,
        restriction: crate::card::EnchantRestriction,
    },
    /// A spell without a physical card on the stack. Owns its spell data.
    SpellCopy { definition: Box<CardDef> },
    /// Closed printed ability; owns historical public endpoint facts.
    ReclamationAbility { source: crate::card::ExactObjectRef, source_owner: PlayerIndex, target_card_id: CardId },
}

impl StackSource {
    pub fn physical_object(&self) -> Option<ObjectId> {
        match self { Self::Spell(id) => Some(*id), Self::AuraSpell { source, .. } => Some(source.id), _ => None }
    }
    pub fn is_spell(&self) -> bool {
        matches!(self, Self::Spell(_) | Self::AuraSpell { .. } | Self::SpellCopy { .. })
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
#[derive(Debug, Clone, Serialize)]
pub struct GameState {
    #[serde(deserialize_with = "deserialize_game_state_format")]
    state_format: u32,
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
    /// Authoritative designation bindings and public histories.
    pub commanders: Vec<CommanderRecord>,
    pub commander_arrivals: Vec<CommanderArrival>,
    pub consumed_commander_arrivals: Vec<CommanderArrival>,
    pub pending_commander_sba: Option<CommanderSbaChoice>,
    pub pending_movement: Option<PendingMovement>,
    pub pending_chittering: Option<PendingChittering>,
    pub next_movement_window: u64,

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

    /// Internal identity for each committed transition batch. The raw value
    /// never enters canonical actions or information-set hashes.
    pub next_zone_event_group_id: u64,

    /// Pending triggers waiting to be put on the stack.
    /// These accumulate during rule processing and are placed on the stack
    /// in APNAP order (active player's triggers first) before priority is given.
    pub pending_triggers: Vec<PendingTrigger>,

    /// Where priority resumes after a mandatory trigger-order choice.
    #[serde(default)]
    pub trigger_order_resume: Option<TriggerOrderResume>,
    #[serde(deserialize_with = "deserialize_trigger_placement")]
    pub trigger_placement: Option<TriggerPlacement>,
    pub next_trigger_window: u64,

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

    /// Persisted loss facts and termination provenance. Appended to the saved
    /// layout; defaulting supports absent JSON fields, not old binary saves.
    #[serde(default)]
    pub loss_boundary: LossBoundary,

    /// The current synchronous action owns its accepted-action coordinate.
    #[serde(skip)]
    pub(crate) loss_action_in_progress: bool,

    /// Failed common-pass preparation at an external INVALID boundary.
    /// No pass plan is persisted.
    #[serde(default)]
    pub sba_failure: Option<crate::rules::sba::PreparedPassFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossCause {
    LifeTotal,
    Poison,
    CommanderDamage { commander: CommanderDesignation },
    FailedDraw,
    Concession,
    /// A resumed represented loss without an original persisted cause.
    AlreadyLost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LossFact {
    pub player: PlayerIndex,
    pub causes: Vec<LossCause>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LossCoordinates {
    pub global_turn: u32,
    pub active_seat: PlayerIndex,
    /// Actual turns taken by each seat, including extra turns. Index this by
    /// the benchmark pilot; do not infer pilot turns from the global serial.
    pub player_turns: Vec<u32>,
    pub phase: Phase,
    pub action_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalLoss {
    pub losses: Vec<LossFact>,
    pub coordinates: LossCoordinates,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsupportedElimination {
    pub losses: Vec<LossFact>,
    pub coordinates: LossCoordinates,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LossBoundary {
    /// Deduplicated players who actually failed a draw since last settlement.
    pub pending_failed_draws: Vec<PlayerIndex>,
    pub terminal: Option<TerminalLoss>,
    pub unsupported: Option<UnsupportedElimination>,
    pub accepted_actions: u64,
    pub turns_taken: Vec<u32>,
}

/// Runtime action freshness. IDs are local to one persisted placement window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TriggerOccurrenceHandle {
    pub window: u64,
    pub occurrence: u64,
}

/// Versioned ordinary-trigger placement (CR 603.3b, September 25, 2026).
/// Trigger-of-trigger conditions are not represented by this engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerPlacement {
    pub version: u32,
    pub window: u64,
    pub active_player: PlayerIndex,
    pub controller: PlayerIndex,
    pub initial_count: usize,
    pub occurrences: Vec<PendingTrigger>,
    pub placed: Vec<u64>,
    pub initial_stack_len: usize,
    pub initial_stack_id: StackId,
}

/// A trigger that has been queued but not yet placed on the stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTrigger {
    /// Runtime identity in a frozen placement window; never a semantic key.
    #[serde(default)]
    pub placement: Option<TriggerOccurrenceHandle>,
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
    /// Owned facts about the event subject. Legacy trigger paths have none.
    #[serde(default)]
    pub zone_transition: Option<crate::rules::transitions::ZoneTriggerContext>,
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
            placement: None,
            source_id,
            ability_index,
            controller,
            targets,
            context: TriggerContext {
                source_card_id: inst.card_def_id,
                source_generation: inst.zone_change_count,
                effect: ability.effect.clone(),
                cast_spell: None,
                zone_transition: None,
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
    commanders: Vec<CommanderRecord>,
    commander_arrivals: Vec<CommanderArrival>,
    consumed_commander_arrivals: Vec<CommanderArrival>,
    pending_commander_sba: Option<CommanderSbaChoice>,
    pending_movement: Option<PendingMovement>,
    pending_chittering: Option<PendingChittering>,
    next_movement_window: u64,
    active_player: PlayerIndex,
    phase: Phase,
    priority_player: PlayerIndex,
    turn_number: u32,
    consecutive_passes: u32,
    combat: CombatState,
    next_object_id: ObjectId,
    next_stack_id: StackId,
    next_zone_event_group_id: u64,
    pending_triggers: Vec<PendingTrigger>,
    trigger_order_resume: Option<TriggerOrderResume>,
    trigger_placement: Option<TriggerPlacement>,
    next_trigger_window: u64,
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
    loss_boundary: LossBoundary,
    sba_failure: Option<crate::rules::sba::PreparedPassFailure>,
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
    pub(crate) player_count: usize,
    pub(crate) viewer: PlayerIndex,
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
    pub trigger_placement: Option<(u32, usize, usize, usize, usize)>,
    pub cleanup_needs_repeat: bool,
    pub cleanup_discard_in_progress: bool,
    pub pending_copy_order: Option<&'a PendingCopyOrder>,
    pub pending_failed_draws: &'a [PlayerIndex],
    pub sba_preparation_failed: bool,
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
    pub commanders: Vec<PublicCommander>,
    pub pending_commander_sba: Option<PublicCommanderSbaChoice>,
    pub(crate) pending_movement: Option<&'a PendingMovement>,
    pub(crate) pending_chittering: Option<&'a PendingChittering>,
    /// Identical public waiting facts across every private resolution prefix.
    pub chittering_wait: Option<(u32, PlayerIndex, PlayerIndex)>,
    pub(crate) chittering_entry: Option<&'a StackEntry>,
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
    /// Includes current objects in public zones from every seat, physical
    /// spells on the stack, and the viewing player's hand. An ability source
    /// reference grants no additional access to a current hidden instance.
    /// Owned trigger contexts retain historical source information separately.
    pub objects: HashMap<ObjectId, &'a CardInstance>,
    /// Finite admitted public inventories for the joint decision projection.
    pub state_encoding_failed: bool,
    pub visible_locations: Vec<(ObjectId, ZoneType, PlayerIndex)>,
    pub represented_characteristics: HashMap<ObjectId, crate::layers::ComputedCharacteristics>,
    pub commander_objects: HashSet<ObjectId>,
    pub commander_roles: HashMap<ObjectId, (usize, u8, u32)>,
    pub continuous_effects: &'a [crate::layers::ContinuousEffect],
    pub(crate) replacement_effects: &'a [crate::replacement::ReplacementEffect],
    /// Observable nonbattlefield token residents: (zone tag, seat, definition).
    /// No ObjectIds, generations or hidden-zone identity are disclosed.
    pub visible_zone_tokens: Vec<(u8, usize, CardId)>,
    /// Authoritative public zone membership from every seat.
    pub(crate) public_zones: Vec<&'a [ObjectId]>,
    /// Evaluated facts for live sources of owned transition occurrences only.
    pub zone_live_sources: HashMap<ObjectId, crate::rules::transitions::LiveSourceInfo>,
    /// Card definitions database (shared, immutable).
    pub card_db: &'a CardDatabase,
}

impl PlayerView<'_> {
    /// A current incarnation is observable only in a public zone or this
    /// player's own hand. Use authoritative current zone membership rather
    /// than a historical ability reference to confirm that incarnation.
    pub(crate) fn visible_current_generation(&self, id: ObjectId) -> Option<u32> {
        (self.public_zones.iter().any(|zone| zone.contains(&id)) || self.visible_locations.iter().any(|&(object, _, _)| object == id))
            .then(|| self.objects.get(&id).map(|inst| inst.zone_change_count)).flatten()
    }
}

impl GameState {
    /// Build a `PlayerView` for the given player, exposing only information
    /// that player is entitled to see under the MTG rules.
    ///
    /// The `objects` map is filtered to only include card instances in visible
    /// zones: battlefield, physical spells on the stack, all graveyards,
    /// exile and command zones, and the viewing player's hand. Owned pending
    /// and stacked trigger history does not reveal current hidden instances.
    pub fn visible_state(&self, player: PlayerIndex) -> PlayerView<'_> {
        let opp = self.opponent(player);
        let public_zones = std::iter::once(self.battlefield.as_slice())
            .chain(self.players.iter().flat_map(|seat| [
                seat.graveyard.as_slice(), seat.exile.as_slice(), seat.command_zone.as_slice(),
            ]))
            .collect();

        // Collect ObjectIds from all visible zones into the filtered objects map.
        let mut visible = HashMap::new();

        // Battlefield — public
        for &id in &self.battlefield {
            if let Some(inst) = self.objects.get(&id) {
                visible.insert(id, inst);
            }
        }
        // Physical spells occupy a current public zone. Ability entries are
        // public history, but their referenced sources can now be hidden.
        // Such current sources enter this map only through visible zones.
        for entry in &self.stack {
            let source_id = match entry.source {
                StackSource::Spell(id) => Some(id),
                StackSource::AuraSpell { source, .. } => Some(source.id),
                StackSource::ActivatedAbility { .. } | StackSource::TriggeredAbility { .. }
                    | StackSource::EquipAbility { .. } | StackSource::ReclamationAbility { .. } => None,
                StackSource::SpellCopy { .. } => None,
            };
            if let Some(source_id) = source_id {
                if self.players.iter().any(|seat| seat.hand.contains(&source_id) || seat.library.contains(&source_id)) { continue; }
                if let Some(inst) = self.objects.get(&source_id) {
                    visible.insert(source_id, inst);
                }
            }
        }
        if let Some(entry) = self.resolving_entry() {
            if let StackSource::Spell(source_id) = &entry.source {
                if !self.players.iter().any(|seat| seat.hand.contains(source_id) || seat.library.contains(source_id)) {
                    if let Some(inst) = self.objects.get(source_id) { visible.insert(*source_id, inst); }
                }
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
        // Pending sources likewise grant no current-instance access. Their
        // owned contexts remain available through PlayerView.pending_triggers;
        // public-zone and own-hand membership provide all legitimate access.
        // Command zones — public (both players' commanders are visible)
        for p in &self.players {
            for &id in &p.command_zone {
                if let Some(inst) = self.objects.get(&id) {
                    visible.insert(id, inst);
                }
            }
        }
        // Combat references can also outlive battlefield residence. Current
        // participants are already included by visible-zone membership above;
        // retaining a reference must not reveal a departed hidden source.

        // Mechanical per-view reuse: one evaluated current profile per
        // relevant live raw object, regardless of occurrence multiplicity.
        let mut retained_live_ids = std::collections::HashSet::new();
        for context in self.pending_triggers.iter().map(|trigger| &trigger.context)
            .chain(self.stack.iter().filter_map(|entry| match &entry.source {
                StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()),
                _ => None,
            })).chain(self.resolving_entry().and_then(|entry| match &entry.source {
                StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()), _ => None,
            })) {
            if let Some(zone) = &context.zone_transition {
                let exact = zone.source_before.object;
                if context.source_generation == exact.generation
                    && visible.get(&exact.id).is_some_and(|inst| inst.zone_change_count == exact.generation) {
                    retained_live_ids.insert(exact.id);
                }
            }
        }
        let zone_live_sources = retained_live_ids.into_iter().filter_map(|id|
            self.card_db.as_ref().and_then(|_| crate::rules::transitions::capture_live_source(self, id)).map(|info| (id, info))).collect();

        let mut visible_zone_tokens = Vec::new();
        for (seat, zones) in self.players.iter().enumerate() {
            for (tag, ids) in [(0, &zones.hand), (1, &zones.graveyard),
                (2, &zones.exile), (3, &zones.command_zone)] {
                if tag == 0 && seat != player { continue; }
                for &id in ids {
                    if let Some(inst) = self.objects.get(&id).filter(|inst| inst.is_token) {
                        visible_zone_tokens.push((tag, seat, inst.card_def_id));
                    }
                }
            }
        }
        for entry in &self.stack {
            if let Some(id) = entry.source.physical_object() {
                if let Some(inst) = visible.get(&id).filter(|inst| inst.is_token) {
                    visible_zone_tokens.push((4, inst.owner, inst.card_def_id));
                }
            }
        }
        visible_zone_tokens.sort_unstable();
        let mut visible_locations: Vec<_> = self.battlefield.iter().map(|&id| (id, ZoneType::Battlefield, 0)).collect();
        for (seat, zones) in self.players.iter().enumerate() {
            for (zone, ids) in [(ZoneType::Graveyard, &zones.graveyard), (ZoneType::Exile, &zones.exile),
                (ZoneType::Command, &zones.command_zone)] {
                visible_locations.extend(ids.iter().map(|&id| (id, zone, seat)));
            }
        }
        visible_locations.extend(self.players[player].hand.iter().map(|&id| (id, ZoneType::Hand, player)));
        for entry in &self.stack {
            if let Some(id) = entry.source.physical_object() { visible_locations.push((id, ZoneType::Stack, entry.controller)); }
        }
        if let Some(entry) = self.resolving_entry() {
            if let Some(id) = entry.source.physical_object() {
                if visible.contains_key(&id) && !visible_locations.iter().any(|&(current, _, _)| current == id) {
                    visible_locations.push((id, ZoneType::Stack, entry.controller));
                }
            }
        }

        let represented_characteristics = visible.keys().filter_map(|&id|
            self.card_db.as_ref().and_then(|_| self.get_characteristics(id)).map(|chars| (id, chars))).collect();
        let commander_objects = self.commanders.iter().map(|c| c.object)
            .filter(|id| visible.contains_key(id)).collect();
        let commander_roles = self.commanders.iter().filter(|c| visible.contains_key(&c.object))
            .map(|c| (c.object, (c.designation.owner, c.designation.role,
                c.casts_by_player.get(c.designation.owner).copied().unwrap_or(0)))).collect();
        PlayerView {
            player_count: self.players.len(),
            viewer: player,
            phase: self.phase,
            active_player: self.active_player,
            turn_number: self.turn_number,
            battlefield: &self.battlefield,
            stack: &self.stack,
            combat: &self.combat,
            pending_triggers: &self.pending_triggers,
            trigger_order_resume: self.trigger_order_resume,
            trigger_placement: self.trigger_placement.as_ref().map(|p| (p.version, p.active_player, p.controller, p.initial_count, p.placed.len())),
            cleanup_needs_repeat: self.cleanup_needs_repeat,
            cleanup_discard_in_progress: self.cleanup_discard_in_progress,
            pending_copy_order: self.pending_copy_order.as_ref(),
            pending_failed_draws: &self.loss_boundary.pending_failed_draws,
            sba_preparation_failed: self.sba_failure.is_some(),
            priority_player: self.priority_player,

            my_life: self.players[player].life,
            opp_life: self.players[opp].life,
            my_graveyard: &self.players[player].graveyard,
            opp_graveyard: &self.players[opp].graveyard,
            my_exile: &self.players[player].exile,
            opp_exile: &self.players[opp].exile,
            opp_hand_size: self.card_count(&self.players[opp].hand),
            opp_library_size: self.card_count(&self.players[opp].library),

            my_hand: &self.players[player].hand,

            my_mana_pool: &self.players[player].mana_pool,
            my_land_plays_remaining: self.players[player].land_plays_remaining,

            my_command_zone: &self.players[player].command_zone,
            opp_command_zone: &self.players[opp].command_zone,
            my_commander_tax: self.commander_casts(player, 0, player),
            commanders: self.public_commanders(),
            pending_commander_sba: self.pending_commander_sba.as_ref().map(PublicCommanderSbaChoice::from),
            pending_movement: self.pending_movement.as_ref().filter(|op|
                !crate::rules::chittering::private_operation(op) || op.chooser(self.players.len()) == Some(player)),
            pending_chittering: self.pending_chittering.as_ref().filter(|op| op.opponent == player),
            chittering_wait: crate::rules::chittering::public_wait(self),
            chittering_entry: crate::rules::chittering::resolving_entry(self),
            format: self.format,

            my_mulligan_count: self.players[player].mulligan_count,

            pending_tutor: self.pending_tutor.clone(),

            objects: visible,
            state_encoding_failed: self.validate_commanders().is_err() || self.validate_commander_choices().is_err() || self.validate_movement_choices().is_err() || crate::rules::chittering::validate_structure(self).is_err() || crate::rules::reclamation::validate_structure(self).is_err() || self.validate_trigger_placement().is_err() || matches!(self.sba_failure, Some(crate::rules::sba::PreparedPassFailure::StateEncoding)),
            visible_locations,
            represented_characteristics,
            commander_objects,
            commander_roles,
            continuous_effects: &self.continuous_effects,
            replacement_effects: &self.replacement_effects,
            public_zones,
            visible_zone_tokens,
            zone_live_sources,
            // An observation with no current objects requires no definition
            // lookup. Any visible undefined object still fails projection.
            card_db: self.card_db.as_deref().unwrap_or_else(|| {
                static EMPTY: std::sync::OnceLock<CardDatabase> = std::sync::OnceLock::new();
                EMPTY.get_or_init(CardDatabase::new)
            }),
        }
    }
}

impl GameState {
    pub fn new(num_players: usize) -> Self {
        GameState {
            state_format: GAME_STATE_FORMAT,
            card_db: None,
            combo_registry: None,
            format: GameFormat::Standard,
            objects: HashMap::new(),
            battlefield: Vec::new(),
            stack: Vec::new(),
            players: (0..num_players).map(|_| PlayerState::new()).collect(),
            commanders: Vec::new(),
            commander_arrivals: Vec::new(),
            consumed_commander_arrivals: Vec::new(),
            pending_commander_sba: None,
            pending_movement: None,
            pending_chittering: None,
            next_movement_window: 1,
            active_player: 0,
            phase: Phase::Untap,
            priority_player: 0,
            turn_number: 1,
            consecutive_passes: 0,
            combat: CombatState::default(),
            next_object_id: 1,
            next_stack_id: 1,
            next_zone_event_group_id: 0,
            pending_triggers: Vec::new(),
            trigger_order_resume: None,
            trigger_placement: None,
            next_trigger_window: 0,
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
            loss_boundary: LossBoundary { turns_taken: vec![0; num_players], ..Default::default() },
            loss_action_in_progress: false,
            sba_failure: None,
        }
    }

    /// Create a new game state configured for Commander format.
    pub fn new_commander(num_players: usize) -> Self {
        let state = GameState {
            state_format: GAME_STATE_FORMAT,
            card_db: None,
            combo_registry: None,
            format: GameFormat::Commander,
            objects: HashMap::new(),
            battlefield: Vec::new(),
            stack: Vec::new(),
            players: (0..num_players)
                .map(|_| PlayerState::new_commander(40))
                .collect(),
            commanders: Vec::new(),
            commander_arrivals: Vec::new(),
            consumed_commander_arrivals: Vec::new(),
            pending_commander_sba: None,
            pending_movement: None,
            pending_chittering: None,
            next_movement_window: 1,
            active_player: 0,
            phase: Phase::Untap,
            priority_player: 0,
            turn_number: 1,
            consecutive_passes: 0,
            combat: CombatState::default(),
            next_object_id: 1,
            next_stack_id: 1,
            next_zone_event_group_id: 0,
            pending_triggers: Vec::new(),
            trigger_order_resume: None,
            trigger_placement: None,
            next_trigger_window: 0,
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
            loss_boundary: LossBoundary { turns_taken: vec![0; num_players], ..Default::default() },
            loss_action_in_progress: false,
            sba_failure: None,
        };
        state
    }

    /// Continuing elimination is deliberately unsupported until leaving-game
    /// ownership, control, stack and trigger semantics are implemented.
    pub fn unsupported_continuing_elimination(&self) -> bool {
        self.loss_boundary.unsupported.is_some()
            || (!self.game_over && self.active_player_count() >= 2
                && self.players.iter().any(|player| player.has_lost))
    }

    /// Preserve existing INVALID reporting while distinguishing prepared-pass
    /// failure from continuing elimination. Both stop all production consumers.
    pub fn invalid_gameplay_reason(&self) -> Option<crate::simulation::TerminationReason> {
        if self.unsupported_continuing_elimination() {
            Some(crate::simulation::TerminationReason::UnsupportedContinuingElimination)
        } else if matches!(self.sba_failure, Some(crate::rules::sba::PreparedPassFailure::StateEncoding)) {
            Some(crate::simulation::TerminationReason::StateEncoding)
        } else if self.sba_failure.is_some() {
            Some(crate::simulation::TerminationReason::PreparedSbaFailure)
        } else { None }
    }

    pub fn gameplay_stopped(&self) -> bool {
        self.game_over || self.invalid_gameplay_reason().is_some()
    }

    pub fn loss_coordinates(&self) -> LossCoordinates {
        let mut player_turns = self.loss_boundary.turns_taken.clone();
        player_turns.resize(self.players.len(), 0);
        if player_turns.iter().all(|turns| *turns == 0) && self.active_player < player_turns.len() {
            player_turns[self.active_player] = 1;
        }
        LossCoordinates { global_turn: self.turn_number, active_seat: self.active_player,
            player_turns, phase: self.phase,
            action_index: self.loss_boundary.accepted_actions + u64::from(self.loss_action_in_progress) }
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
            commanders: self.commanders.clone(),
            commander_arrivals: self.commander_arrivals.clone(),
            consumed_commander_arrivals: self.consumed_commander_arrivals.clone(),
            pending_commander_sba: self.pending_commander_sba.clone(),
            pending_movement: self.pending_movement.clone(),
            pending_chittering: self.pending_chittering.clone(),
            next_movement_window: self.next_movement_window,
            active_player: self.active_player,
            phase: self.phase,
            priority_player: self.priority_player,
            turn_number: self.turn_number,
            consecutive_passes: self.consecutive_passes,
            combat: self.combat.clone(),
            next_object_id: self.next_object_id,
            next_stack_id: self.next_stack_id,
            next_zone_event_group_id: self.next_zone_event_group_id,
            pending_triggers: self.pending_triggers.clone(),
            trigger_order_resume: self.trigger_order_resume,
            trigger_placement: self.trigger_placement.clone(),
            next_trigger_window: self.next_trigger_window,
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
            loss_boundary: self.loss_boundary.clone(),
            sba_failure: self.sba_failure.clone(),
        }
    }

    /// Restore game state from a snapshot, keeping the current card_db.
    /// Invalidates all caches.
    pub fn restore(&mut self, snap: GameStateSnapshot)  -> Result<(), crate::card::AttachmentError> {
        self.format = snap.format;
        self.objects = snap.objects;
        self.battlefield = snap.battlefield;
        self.stack = snap.stack;
        self.players = snap.players;
        self.commanders = snap.commanders;
        self.commander_arrivals = snap.commander_arrivals;
        self.consumed_commander_arrivals = snap.consumed_commander_arrivals;
        self.pending_commander_sba = snap.pending_commander_sba;
        self.pending_movement = snap.pending_movement;
        self.pending_chittering = snap.pending_chittering;
        self.next_movement_window = snap.next_movement_window;
        self.active_player = snap.active_player;
        self.phase = snap.phase;
        self.priority_player = snap.priority_player;
        self.turn_number = snap.turn_number;
        self.consecutive_passes = snap.consecutive_passes;
        self.combat = snap.combat;
        self.next_object_id = snap.next_object_id;
        self.next_stack_id = snap.next_stack_id;
        self.next_zone_event_group_id = snap.next_zone_event_group_id;
        self.pending_triggers = snap.pending_triggers;
        self.trigger_order_resume = snap.trigger_order_resume;
        self.trigger_placement = snap.trigger_placement;
        self.next_trigger_window = snap.next_trigger_window;
        self.cleanup_needs_repeat = snap.cleanup_needs_repeat;
        self.cleanup_discard_in_progress = snap.cleanup_discard_in_progress;
        self.pending_copy_order = snap.pending_copy_order;
        self.trigger_placement_deferred = crate::rules::chittering::resolving_entry(self).is_some();
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
        self.loss_boundary = snap.loss_boundary;
        self.sba_failure = snap.sba_failure;
        self.loss_action_in_progress = false;
        self.pending_events.clear();
        self.invalidate_characteristics_cache();

        if self.validate_commanders().is_err() || self.validate_commander_choices().is_err() || self.validate_movement_choices().is_err() || crate::rules::chittering::validate_structure(self).is_err() || crate::rules::reclamation::validate_structure(self).is_err() || self.validate_trigger_placement().is_err() {
            self.sba_failure = Some(crate::rules::sba::PreparedPassFailure::StateEncoding);
            return Err(crate::card::AttachmentError::Source);
        }
        self.validate_attachment_state(true)
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
    ) -> Result<ObjectId, crate::rules::transitions::TransitionError> {
        use crate::rules::transitions::TransitionError;
        if owner >= self.players.len() { return Err(TransitionError::InvalidPlayer(self.next_object_id)); }
        if zone == ZoneType::Battlefield {
            let def = self.card_db.as_ref().ok_or(TransitionError::MissingDatabase)?
                .get(card_id).ok_or(TransitionError::MissingDefinition(card_id))?;
            if self.prospective_aura(CardInstance::new(self.next_object_id, card_id, owner), def)? {
                return Err(TransitionError::UnsupportedPath);
            }
        }
        if self.objects.contains_key(&self.next_object_id) { return Err(TransitionError::DuplicateSubject(self.next_object_id)); }
        self.next_object_id.checked_add(1).ok_or(TransitionError::CapacityExhausted)?;
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
        Ok(obj_id)
    }

    /// Move a card instance to its actual destination. Commander
    /// graveyard/exile choices occur at the next SBA check.
    pub fn move_object(
        &mut self,
        obj_id: ObjectId,
        from: ZoneType,
        to: ZoneType,
    ) {
        if let Err(error) = self.try_move_object(obj_id, from, to) {
            self.sba_failure = Some(if error == crate::rules::transitions::TransitionError::UnsupportedPath {
                crate::rules::sba::PreparedPassFailure::Transition(error)
            } else { crate::rules::sba::PreparedPassFailure::StateEncoding });
        }
    }

    /// Fallible bounded admission for noncast Battlefield ingress.
    pub fn try_move_object(&mut self, id: ObjectId, from: ZoneType, to: ZoneType)
        -> Result<(), crate::rules::transitions::TransitionError> {
        self.try_move_object_with_entry_controller(id, from, to, None)
    }

    /// Carry a represented entry controller through admission and successful
    /// movement. Do not modify stored control before admission has succeeded.
    pub(crate) fn try_enter_battlefield(&mut self, id: ObjectId, from: ZoneType, controller: PlayerIndex)
        -> Result<(), crate::rules::transitions::TransitionError> {
        self.try_move_object_with_entry_controller(id, from, ZoneType::Battlefield, Some(controller))
    }

    fn try_move_object_with_entry_controller(&mut self, id: ObjectId, from: ZoneType, to: ZoneType,
        entry_controller: Option<PlayerIndex>) -> Result<(), crate::rules::transitions::TransitionError> {
        if to == ZoneType::Battlefield {
            if let Some(inst) = self.objects.get(&id).filter(|inst|
                self.card_db.as_ref().and_then(|db| db.get(inst.card_def_id)).is_some_and(|def| def.is_aura())) {
                let resident = match from {
                    ZoneType::Battlefield => self.battlefield.contains(&id),
                    ZoneType::Stack => self.stack.iter().any(|entry| entry.source.physical_object() == Some(id)),
                    zone => self.players.get(inst.owner).is_some_and(|player| match zone {
                        ZoneType::Library => player.library.contains(&id),
                        ZoneType::Hand => player.hand.contains(&id),
                        ZoneType::Graveyard => player.graveyard.contains(&id),
                        ZoneType::Exile => player.exile.contains(&id),
                        ZoneType::Command => player.command_zone.contains(&id),
                        _ => unreachable!(),
                    }),
                };
                if !resident { return Err(crate::rules::transitions::TransitionError::WrongZone(id)); }
            }
        }
        if let Some(controller) = entry_controller {
            self.preflight_noncast_entry_for_controller(id, to, controller)?;
        } else {
            self.preflight_noncast_entry(id, to)?;
        }
        let refresh = if from == ZoneType::Battlefield && to != ZoneType::Battlefield
            && self.battlefield.contains(&id) {
            let mut projection = RefreshProjection::new(self);
            for player in &self.players {
                for &linked in &player.exile {
                    if self.is_card(linked) && self.objects.get(&linked).is_some_and(|inst| inst.exiled_by == Some(id)) {
                        projection.depart(self, linked, ZoneType::Graveyard).map_err(|_| crate::rules::transitions::TransitionError::CapacityExhausted)?;
                    }
                }
            }
            projection.depart(self, id, to).map_err(|_| crate::rules::transitions::TransitionError::CapacityExhausted)?;
            Some(projection.refresh(self).map_err(|_| crate::rules::transitions::TransitionError::CapacityExhausted)?)
        } else { None };
        self.move_object_with_policy(id, from, to, true, true, None, refresh);
        if let Some(controller) = entry_controller {
            self.objects.get_mut(&id).expect("validated entry source").controller = controller;
        }
        Ok(())
    }

    pub(crate) fn has_prospective_aura_effect(&self) -> bool {
        self.continuous_effects.iter().any(|effect| match &effect.modification {
            crate::layers::LayerModification::CopyOf(_) => true,
            crate::layers::LayerModification::AddSubtype(subtype) => subtype.0 == "Aura",
            _ => false,
        })
    }

    /// Evaluate the represented future Battlefield incarnation without entry.
    /// Only prospective Aura admission uses this local layer input view.
    pub(crate) fn prospective_aura(&self, inst: CardInstance, def: &CardDef)
        -> Result<bool, crate::rules::transitions::TransitionError> {
        use crate::rules::transitions::TransitionError;
        if def.is_aura() { return Ok(true); }
        if !self.has_prospective_aura_effect() { return Ok(false); }
        if inst.owner >= self.players.len() || inst.controller >= self.players.len() {
            return Err(TransitionError::InvalidPlayer(inst.object_id));
        }
        let db = self.card_db.as_ref().ok_or(TransitionError::MissingDatabase)?;
        let mut definitions = (**db).clone();
        definitions.insert(def.clone());
        let id = inst.object_id;
        let mut objects = self.objects.clone();
        objects.insert(id, inst);
        let mut battlefield: HashSet<_> = self.battlefield.iter().copied().collect();
        battlefield.insert(id);
        let chars = crate::layers::compute_characteristics(id, &self.continuous_effects,
            &objects, &battlefield, &definitions).ok_or(TransitionError::MissingObject(id))?;
        if chars.controller >= self.players.len() { return Err(TransitionError::InvalidPlayer(id)); }
        Ok(chars.card_types.contains(&crate::card::CardType::Enchantment)
            && chars.subtypes.iter().any(|subtype| subtype.0 == "Aura"))
    }

    pub(crate) fn preflight_noncast_entry(&self, id: ObjectId, to: ZoneType)
        -> Result<(), crate::rules::transitions::TransitionError> {
        let controller = self.objects.get(&id).map_or(0, |inst| inst.controller);
        self.preflight_noncast_entry_for_controller(id, to, controller)
    }

    pub(crate) fn preflight_noncast_entry_for_controller(&self, id: ObjectId, to: ZoneType, controller: usize)
        -> Result<(), crate::rules::transitions::TransitionError> {
        use crate::rules::transitions::TransitionError;
        if to != ZoneType::Battlefield { return Ok(()); }
        let inst = self.objects.get(&id).ok_or(TransitionError::MissingObject(id))?;
        let def = self.card_db.as_ref().ok_or(TransitionError::MissingDatabase)?
            .get(inst.card_def_id).ok_or(TransitionError::MissingDefinition(inst.card_def_id))?;
        if inst.object_id != id || inst.owner >= self.players.len() || inst.controller >= self.players.len()
            || controller >= self.players.len() {
            return Err(TransitionError::InvalidPlayer(id));
        }
        let residents = self.battlefield.iter().filter(|&&object| object == id).count()
            + self.players.iter().map(|player| [&player.hand, &player.library, &player.graveyard, &player.exile, &player.command_zone]
                .iter().map(|zone| zone.iter().filter(|&&object| object == id).count()).sum::<usize>()).sum::<usize>()
            + self.stack.iter().filter(|entry| entry.source.physical_object() == Some(id)).count();
        let aura = if def.is_aura() || self.has_prospective_aura_effect() {
            let mut future = inst.clone();
            future.controller = controller;
            future.zone_change_count = future.zone_change_count.checked_add(1).ok_or(TransitionError::GenerationExhausted(id))?;
            self.prospective_aura(future, def)?
        } else { false };
        if aura {
            if residents != 1 { return Err(TransitionError::WrongZone(id)); }
            if self.stack.iter().any(|entry| entry.source.physical_object() == Some(id))
                && crate::targeting::validate_pending_auras(self).is_err() {
                return Err(TransitionError::StaleIncarnation(id));
            }
            if self.players.iter().enumerate().any(|(owner, player)| owner != inst.owner
                && [&player.hand, &player.library, &player.graveyard, &player.exile, &player.command_zone]
                    .iter().any(|zone| zone.contains(&id))) { return Err(TransitionError::WrongZone(id)); }
            return Err(TransitionError::UnsupportedPath);
        }
        Ok(())
    }

    /// Storage only for the synchronously prepared cast Aura adapter.
    pub(crate) fn move_prepared_aura(&mut self, id: ObjectId, to: ZoneType) {
        self.move_object_with_policy(id, ZoneType::Stack, to, false, false, None, None);
    }

    /// Storage adapter for the validated transition kernel. The caller owns
    /// committed notifications and trigger collection; the token remains in
    /// its destination until a following state-based cessation pass.
    pub(crate) fn move_object_for_transition(&mut self, obj_id: ObjectId, from: ZoneType, to: ZoneType, attachments: &crate::card::AttachmentInventory) {
        self.move_object_with_policy(obj_id, from, to, false, false, Some(attachments), None);
    }

    /// CR 111.8: a token outside the battlefield cannot change zones again.
    /// Check both the declared source and current residence before any side effect.
    pub(crate) fn token_movement_prohibited(&self, id: ObjectId, from: ZoneType) -> bool {
        self.objects.get(&id).is_some_and(|inst| inst.is_token
            && (from != ZoneType::Battlefield || !self.battlefield.contains(&id)))
    }

    /// Card-only operations must ignore resident tokens (CR 111.6).
    pub fn is_card(&self, id: ObjectId) -> bool {
        self.objects.get(&id).is_some_and(|inst| !inst.is_token)
    }

    pub fn card_count(&self, zone: &[ObjectId]) -> usize {
        zone.iter().filter(|&&id| self.is_card(id)).count()
    }

    pub(crate) fn first_library_card(&self, player: PlayerIndex) -> Option<usize> {
        self.players[player].library.iter().position(|&id| self.is_card(id))
    }

    fn move_object_with_policy(
        &mut self, obj_id: ObjectId, from: ZoneType, to: ZoneType,
        emit_primary_event: bool, run_linked_followup: bool,
        attachment_inventory: Option<&crate::card::AttachmentInventory>,
        refresh: Option<PreparedContinuousRefresh>,
    ) {
        if self.token_movement_prohibited(obj_id, from) { return; }
        let actual_to = to;
        let was_resident = match from {
            ZoneType::Battlefield => self.battlefield.contains(&obj_id),
            ZoneType::Stack => self.stack.iter().any(|entry| entry.source.physical_object() == Some(obj_id))
                || self.resolving_entry()
                    .is_some_and(|entry| entry.source.physical_object() == Some(obj_id))
                || (self.trigger_placement_deferred
                    && self.trigger_order_resume == Some(TriggerOrderResume::AfterResolution)
                    && self.objects.contains_key(&obj_id) && !self.battlefield.contains(&obj_id)
                    && self.players.iter().all(|p| [&p.library, &p.hand, &p.graveyard, &p.exile, &p.command_zone]
                        .iter().all(|zone| !zone.contains(&obj_id)))),
            zone => self.objects.get(&obj_id).is_some_and(|inst| self.players.get(inst.owner).is_some_and(|p| match zone {
                ZoneType::Library => p.library.contains(&obj_id),
                ZoneType::Hand => p.hand.contains(&obj_id),
                ZoneType::Graveyard => p.graveyard.contains(&obj_id),
                ZoneType::Exile => p.exile.contains(&obj_id),
                ZoneType::Command => p.command_zone.contains(&obj_id),
                _ => false,
            })),
        };

        let departed = if from == ZoneType::Battlefield && actual_to != ZoneType::Battlefield
            && self.battlefield.contains(&obj_id) { self.exact_object(obj_id) } else { None };
        let owned_attachments = if departed.is_some() && attachment_inventory.is_none() {
            Some(self.capture_attachment_inventory())
        } else { None };
        let attachments = attachment_inventory.or(owned_attachments.as_ref());
        self.invalidate_characteristics_cache();

        // Emit zone change event
        if emit_primary_event {
            self.emit_event(GameEvent::ZoneChange {
                object: obj_id,
                from: crate::events::Zone::from(from),
                to: crate::events::Zone::from(actual_to),
            });
        }

        // Increment zone-change counter (CR 400.7)
        if let Some(inst) = self.objects.get_mut(&obj_id) {
            inst.zone_change_count += 1;
        }

        // When a permanent leaves the battlefield, move all cards exiled by it
        // to their owner's graveyard (e.g., Gustha's Scepter, Tidehollow Sculler).
        if from == ZoneType::Battlefield && run_linked_followup {
            self.move_linked_exiles_for_departure(obj_id);
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
            if let Some(spell_id) = e.source.physical_object() {
                spell_id != obj_id
            } else {
                true
            }
        });

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
        self.note_commander_arrival(obj_id, from, actual_to, was_resident);
        if let (Some(departed), Some(attachments)) = (departed, attachments) {
            self.cleanup_attachment_departure(departed, attachments);
            if emit_primary_event { refresh.expect("prepared departure refresh").commit(self); }
        }
    }

    /// Existing linked-exile behavior. Transition batches call this only
    /// after their entire primary event and owned occurrences have committed;
    /// the secondary movement remains a legacy follow-up until 2B.5.
    pub(crate) fn move_linked_exiles_for_departure(&mut self, obj_id: ObjectId) {
        let linked_exiles: Vec<(ObjectId, usize)> = self.players.iter().enumerate()
            .flat_map(|(pi, p)| {
                p.exile.iter()
                    .filter(|&&eid| self.is_card(eid) && self.objects.get(&eid).and_then(|i| i.exiled_by) == Some(obj_id))
                    .map(move |&eid| (eid, pi))
                    .collect::<Vec<_>>()
            })
            .collect();
        self.move_prevalidated_linked_exiles(&linked_exiles);
    }

    /// Consume only the exile occupants captured before a transition batch.
    /// The kernel validates each identity and generation before any movement;
    /// rescanning here would incorrectly treat a new batch member as an old
    /// linked exile.
    pub(crate) fn move_prevalidated_linked_exiles(&mut self, linked_exiles: &[(ObjectId, usize)]) {
        for &(eid, player_idx) in linked_exiles {
            if self.token_movement_prohibited(eid, ZoneType::Exile) { continue; }
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
            self.note_commander_arrival(eid, ZoneType::Exile, ZoneType::Graveyard, true);
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
        match RefreshProjection::new(self).refresh(self) {
            Ok(prepared) => prepared.commit(self),
            Err(error) => self.sba_failure = Some(error),
        }
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
                .map(|p| self.card_count(&p.hand))
                .unwrap_or(0);
            let db = self.card_db();
            let mut graveyard_card_types = Vec::new();
            for player in &self.players {
                for &gid in &player.graveyard {
                    if let Some(gi) = self.objects.get(&gid).filter(|inst| !inst.is_token) {
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

        if amount == 0 { return 0; }
        let mut effective_amount = i64::from(amount);

        // Apply self-replacement effects automatically
        for &idx in &self_replacements {
            if let Some(effect) = self.replacement_effects.get(idx) {
                match &effect.action {
                    ReplacementAction::Prevent => {
                        effective_amount = 0;
                        break;
                    }
                    ReplacementAction::ModifyAmount { delta } => {
                        effective_amount += i64::from(*delta);
                        // CR 614.7a: zero damage leaves no event to increase.
                        if effective_amount <= 0 { return 0; }
                    }
                    _ => {}
                }
            }
        }

        // TODO: handle player-choice replacement effects (ChooseReplacementOrder)
        // For now, apply them in order
        // (CR 614.5: each effect applies only once per event)

        effective_amount.clamp(0, i64::from(u32::MAX)) as u32
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

    /// Both stages belong to the same unfinished movement boundary.
    /// Whether the viewer must wait for another player's private Chittering choice.
    pub fn chittering_waits_for_other(&self, viewer: PlayerIndex) -> bool {
        crate::rules::chittering::public_wait(self).is_some_and(|(_, _, owner)| owner != viewer)
    }

    pub fn movement_or_hand_pending(&self) -> bool { self.pending_movement.is_some() || self.pending_chittering.is_some() }

    pub(crate) fn resolving_entry(&self) -> Option<&StackEntry> {
        self.pending_copy_order.as_ref().and_then(PendingCopyOrder::resolving_entry)
            .or_else(|| self.pending_movement.as_ref().and_then(|op| op.resolving_entry.as_deref()))
            .or_else(|| self.pending_chittering.as_ref().map(|op| op.resolving_entry.as_ref()))
    }

    /// Refresh replacement effects based on the current battlefield.
    /// Removes effects whose source has left the battlefield.
    pub fn refresh_replacement_effects(&mut self) {
        let bf: HashSet<ObjectId> = self.battlefield.iter().copied().collect();
        self.replacement_effects
            .retain(|e| bf.contains(&e.source_id) && match e.applies_to { ReplacementEventKind::WouldMoveTo { source_generation, .. } => self.objects.get(&e.source_id).is_some_and(|inst| inst.zone_change_count == source_generation), _ => true });
    }

    /// Check if the given object is a player's commander.
    /// Designation belongs to a physical card, never its copied characteristics.
    pub fn is_commander(&self, obj_id: ObjectId) -> bool {
        self.commander_for_object(obj_id).is_some()
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

#[cfg(test)]
mod player_view_zone_visibility_tests {
    use super::*;

    #[test]
    fn every_seat_public_zone_is_currently_visible_but_only_own_hand_is_private_visible() {
        let mut state = GameState::new(4);
        state.card_db = Some(Arc::new(CardDatabase::new()));
        // This visibility-only fixture deliberately has no definition for 1.
        // Preserve that malformed metadata independently of gameplay admission.
        let battlefield = state.new_object_id();
        state.objects.insert(battlefield, CardInstance::new(battlefield, 1, 3));
        state.battlefield.push(battlefield);
        let mut public = vec![battlefield];
        let mut hands = Vec::new();
        let mut libraries = Vec::new();
        for owner in 0..4 {
            for zone in [ZoneType::Graveyard, ZoneType::Exile, ZoneType::Command] {
                public.push(state.create_card_in_zone(1, owner, zone).unwrap());
            }
            hands.push(state.create_card_in_zone(1, owner, ZoneType::Hand).unwrap());
            libraries.push(state.create_card_in_zone(1, owner, ZoneType::Library).unwrap());
        }
        for viewer in 0..4 {
            let view = state.visible_state(viewer);
            for &id in &public {
                assert_eq!(view.visible_current_generation(id), Some(0),
                    "viewer {viewer} cannot see public object {id}");
            }
            for owner in 0..4 {
                assert_eq!(view.visible_current_generation(hands[owner]),
                    (viewer == owner).then_some(0));
                assert_eq!(view.visible_current_generation(libraries[owner]), None);
            }
        }
    }
}

impl GameState {
    /// Validate authoritative placement ownership before actions or observations.
    pub fn validate_trigger_placement(&self) -> Result<(), crate::simulation::TerminationReason> {
        use crate::simulation::TerminationReason::StateEncoding;
        let Some(op) = &self.trigger_placement else {
            return if self.pending_triggers.iter().any(|t| t.placement.is_some()) { Err(StateEncoding) } else { Ok(()) };
        };
        if op.version != 1 || op.window >= self.next_trigger_window
            || op.active_player != self.active_player || op.active_player >= self.players.len()
            || op.controller >= self.players.len() || self.priority_player != op.controller
            || (self.players[op.controller].has_lost && !self.gameplay_stopped()) || self.trigger_order_resume.is_none()
            || matches!(self.trigger_order_resume, Some(TriggerOrderResume::Player(p)) if p >= self.players.len() || (self.players[p].has_lost && !self.gameplay_stopped()))
            || matches!(self.trigger_order_resume, Some(TriggerOrderResume::AfterAttackers)) && self.phase != Phase::DeclareAttackers
            || op.occurrences.len() != op.initial_count
            || self.pending_copy_order.is_some() || self.movement_or_hand_pending() || self.cleanup_discard_in_progress
            || self.trigger_placement_deferred || self.phase == Phase::Mulligan || op.initial_count == 0
            || op.placed.len() >= op.initial_count
            || op.initial_stack_len.checked_add(op.placed.len()) != Some(self.stack.len())
            || op.initial_stack_id.checked_add(op.placed.len() as u64) != Some(self.next_stack_id)
        { return Err(StateEncoding); }
        for (index, trigger) in op.occurrences.iter().enumerate() {
            if trigger.placement != Some(TriggerOccurrenceHandle { window: op.window, occurrence: index as u64 })
                || trigger.controller >= self.players.len() || (self.players[trigger.controller].has_lost && !self.gameplay_stopped()) { return Err(StateEncoding); }
        }
        let mut seen: HashSet<u64> = op.placed.iter().copied().collect();
        if seen.len() != op.placed.len() || seen.iter().any(|id| *id >= op.initial_count as u64) { return Err(StateEncoding); }
        let mut chooser = None;
        for t in &self.pending_triggers {
            if t.controller >= self.players.len() { return Err(StateEncoding); }
            if let Some(handle) = t.placement {
                if handle.window != op.window || handle.occurrence >= op.initial_count as u64
                    || !seen.insert(handle.occurrence) || !same_trigger_record(&op.occurrences[handle.occurrence as usize], t)? || (self.players[t.controller].has_lost && !self.gameplay_stopped()) { return Err(StateEncoding); }
                let rank = (t.controller + self.players.len() - op.active_player) % self.players.len();
                if chooser.is_none_or(|(old, _)| rank < old) { chooser = Some((rank, t.controller)); }
            }
        }
        if seen.len() != op.initial_count || chooser.map(|(_,p)|p) != Some(op.controller) { return Err(StateEncoding); }
        let mut previous_rank = 0;
        for (offset, entry) in self.stack[op.initial_stack_len..].iter().enumerate() {
            let trigger = &op.occurrences[op.placed[offset] as usize];
            let rank = (trigger.controller + self.players.len() - op.active_player) % self.players.len();
            let StackSource::TriggeredAbility { source_id, ability_index, context } = &entry.source else { return Err(StateEncoding); };
            if entry.id != op.initial_stack_id + offset as u64 || entry.controller != trigger.controller
                || *source_id != trigger.source_id || *ability_index != trigger.ability_index
                || !same_trigger_record(context.as_ref(), &trigger.context)?
                || (if crate::rules::chittering::trigger(trigger) { crate::rules::chittering::entry_opponent(entry, self.players.len()).is_err() }
                    else { entry.targets != trigger.targets || entry.target_generations != crate::targeting::target_generations(self, &trigger.targets) })
                || rank < previous_rank || rank > chooser.unwrap().0 { return Err(StateEncoding); }
            previous_rank = rank;
        }
        Ok(())
    }
}

fn same_trigger_record<T: Serialize>(a: &T, b: &T) -> Result<bool, crate::simulation::TerminationReason> {
    Ok(bincode::serialize(a).map_err(|_| crate::simulation::TerminationReason::StateEncoding)?
        == bincode::serialize(b).map_err(|_| crate::simulation::TerminationReason::StateEncoding)?)
}

fn deserialize_trigger_placement<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<TriggerPlacement>, D::Error> {
    Option::<TriggerPlacement>::deserialize(d)
}

// The leading tag rejects historical positional bincode before interpreting any
// changed layout. JSON must carry the same explicit version.
const GAME_STATE_FORMAT: u32 = 0x3246_3343;
fn deserialize_game_state_format<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let version = u32::deserialize(d)?;
    if version != GAME_STATE_FORMAT { return Err(serde::de::Error::custom("incompatible GameState Commander format")); }
    Ok(version)
}

impl PlayerView<'_> { pub(crate) fn resolving_entry(&self) -> Option<&StackEntry> { self.pending_copy_order.and_then(|op| op.resolving_entry()).or_else(|| self.pending_movement.and_then(|op| op.resolving_entry.as_deref())).or(self.chittering_entry) } }
