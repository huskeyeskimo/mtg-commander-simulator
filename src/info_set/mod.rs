//! Phase 1B.1 — Information Set Module
//!
//! An information set represents everything a player can observe about the
//! game state. Two game positions that look identical from a player's
//! perspective belong to the same information set — the player cannot
//! distinguish between them and must play the same mixed strategy in both.
//!
//! This module converts `PlayerView` (the observation API from Phase 0.1)
//! into a hashable `InformationSet` suitable for keying MCCFR regret tables.
//!
//! # Design constraints
//!
//! - Reads **only** from `PlayerView`, never from raw `GameState` fields.
//! - Deterministic: same observable game position always produces the same hash.
//! - The hash is a `u64` computed via a stable, order-independent scheme
//!   for sets (battlefield, graveyards) and order-dependent for sequences
//!   (hand, stack).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::card::CardInstance;
use crate::game::{CardDatabase, CopyTargetDescription, PlayerView, StackEntry, StackSource, TriggerOrderResume};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PendingCopyInfo {
    pub controller: usize,
    pub items: Vec<PreparedCopyInfo>,
    pub selected_order: Vec<usize>,
    pub resolving_entry: Option<StackInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PreparedCopyInfo {
    pub definition: String,
    pub controller: usize,
    pub targets: Vec<CopyTargetDescription>,
}

/// A compact representation of everything a player can observe.
///
/// Two `InformationSet` values are equal iff the player cannot distinguish
/// the underlying game states. MCCFR regret tables key on `info_set_hash()`.
#[derive(Debug, Clone)]
pub struct InformationSet {
    /// Current game phase.
    pub phase: u8,
    /// Whose turn it is.
    pub active_player: usize,
    /// Current turn number (capped for hashing stability).
    pub turn_number: u32,
    /// Who has priority.
    pub priority_player: usize,

    /// Viewing player's life total.
    pub my_life: i32,
    /// Opponent's life total.
    pub opp_life: i32,

    /// Cards in our hand, represented as sorted CardIds.
    pub my_hand: Vec<u64>,
    /// Observable transient nonbattlefield tokens, distinct from cards.
    pub visible_zone_tokens: Vec<(u8, usize, u64)>,
    /// Opponent's hand size (contents unknown).
    pub opp_hand_size: usize,
    /// Opponent's library size (contents unknown).
    pub opp_library_size: usize,

    /// Permanents on the battlefield, represented as sorted (controller, card_id, tapped, power, toughness) tuples.
    pub battlefield: Vec<PermanentInfo>,

    /// Stack entries, in stack order (top = last element).
    pub stack_entries: Vec<StackInfo>,
    /// Cast relationships retained by triggers waiting for APNAP ordering.
    pub pending_cast_spells: Vec<Option<CastSpellInfo>>,
    /// Public historical subjects for pending transition occurrences. This
    /// multiset excludes runtime ObjectIds and absolute generations.
    pub pending_zone_triggers: Vec<crate::rules::transitions::ZoneOccurrenceInfo>,
    /// Complete exact retained projection, not only refinement colors/labels.
    pub zone_normalization: Vec<u8>,
    pub trigger_order_resume: Option<TriggerOrderResume>,
    pub cleanup_needs_repeat: bool,
    pub cleanup_discard_in_progress: bool,
    pub pending_copy_order: Option<PendingCopyInfo>,
    pub pending_failed_draws: Vec<usize>,
    pub sba_preparation_failed: bool,

    /// Our graveyard as sorted CardIds.
    pub my_graveyard: Vec<u64>,
    /// Opponent's graveyard as sorted CardIds.
    pub opp_graveyard: Vec<u64>,

    /// Our exile zone as sorted CardIds.
    pub my_exile: Vec<u64>,
    /// Opponent's exile zone as sorted CardIds.
    pub opp_exile: Vec<u64>,

    /// Remaining land plays this turn.
    pub my_land_plays_remaining: u32,
    /// Available mana per color (W, U, B, R, G, colorless).
    pub my_mana: [u32; 6],

    // --- Commander fields ---
    /// Cards in our command zone, represented as sorted CardIds.
    pub my_command_zone: Vec<u64>,
    /// Cards in opponent's command zone, represented as sorted CardIds.
    pub opp_command_zone: Vec<u64>,
    /// Commander tax (number of times commander has been cast from command zone).
    pub my_commander_tax: u32,

    /// Number of times we've mulliganed (relevant during Mulligan phase).
    pub my_mulligan_count: u32,
}

/// Observable information about a permanent on the battlefield.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PermanentInfo {
    pub controller: usize,
    pub card_id: u64,
    pub tapped: bool,
    pub damage_marked: u32,
    pub summoning_sick: bool,
    pub plus_counters: i32,
    pub minus_counters: i32,
}

/// Observable information about a stack entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StackInfo {
    pub controller: usize,
    pub source_card_id: u64,
    pub is_spell_copy: bool,
    pub target_summary: Vec<u64>, // hashed target descriptions
    /// Historical cast relationship, distinct from this ability's targets.
    pub cast_spell: Option<CastSpellInfo>,
    pub zone_occurrence: Option<crate::rules::transitions::ZoneOccurrenceInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CastSpellInfo {
    pub card_id: u64,
    pub controller: usize,
    pub live_stack_position: Option<usize>,
    pub target_summary: Vec<u64>,
}

impl InformationSet {
    /// Construct an `InformationSet` from a `PlayerView`.
    ///
    /// This is the sole entry point — MCCFR never reads raw `GameState`.
    pub fn from_view(view: &PlayerView, _card_db: &CardDatabase) -> Result<Self, crate::simulation::TerminationReason> {
        let normalized = Self::normalize_retained_view(view)?;
        Self::from_view_with_normalization(view, _card_db, &normalized)
    }

    pub fn normalize_retained_view<'a>(view: &PlayerView<'a>) -> Result<crate::public_projection::JointPublicNormalization<'a>, crate::simulation::TerminationReason> {
        crate::public_projection::JointPublicNormalization::from_view(view)
    }

    /// Observations and action coordinates share one immutable witness.
    pub fn from_view_with_normalization(
        view: &PlayerView, _card_db: &CardDatabase,
        joint: &crate::public_projection::JointPublicNormalization,
    ) -> Result<Self, crate::simulation::TerminationReason> {
        let normalized = &joint.retained;
        Ok((|| -> Self {
        let phase = phase_to_u8(view.phase);

        // Hand: sorted CardIds for canonical representation
        let mut my_hand: Vec<u64> = view
            .my_hand
            .iter()
            .filter_map(|&id| view.objects.get(&id).filter(|inst| !inst.is_token).map(|inst| inst.card_def_id))
            .collect();
        my_hand.sort();

        // Battlefield: sorted PermanentInfo for canonical representation
        let mut battlefield: Vec<PermanentInfo> = view
            .battlefield
            .iter()
            .filter_map(|&id| {
                view.objects.get(&id).map(|inst| PermanentInfo {
                    controller: inst.controller,
                    card_id: inst.card_def_id,
                    tapped: inst.tapped,
                    damage_marked: inst.damage_marked,
                    summoning_sick: inst.summoning_sick,
                    plus_counters: inst.plus_counters,
                    minus_counters: inst.minus_counters,
                })
            })
            .collect();
        battlefield.sort();

        // Stack: preserve order (LIFO semantics matter)
        let stack_entries: Vec<StackInfo> = view
            .stack
            .iter()
            .enumerate()
            .map(|(position, entry)| stack_entry_to_info(entry, view,
                normalized.stack_occurrences[position].clone()))
            .collect();
        let pending_cast_spells = view.pending_triggers.iter()
            .map(|trigger| trigger.context.cast_spell.as_ref()
                .map(|spell| cast_spell_to_info(spell, &view.objects, view.stack)))
            .collect();
        let mut pending_zone_triggers: Vec<_> = normalized.pending_occurrences.iter().zip(view.pending_triggers).filter(|(_, trigger)| trigger.context.zone_transition.is_some()).filter_map(|(occurrence, _)| occurrence.clone()).collect();
        pending_zone_triggers.sort_by_cached_key(crate::rules::transitions::typed_bytes);
        let pending_copy_order = view.pending_copy_order.map(|pending| PendingCopyInfo {
            controller: pending.controller(),
            items: pending.items().iter().map(|item| PreparedCopyInfo {
                definition: item.definition_description.clone(),
                controller: item.controller(),
                targets: item.target_descriptions.clone(),
            }).collect(),
            selected_order: pending.selected_order().to_vec(),
            resolving_entry: pending.resolving_entry()
                .map(|entry| stack_entry_to_info(entry, view, joint.resolving_occurrence.clone())),
        });

        // Graveyards: sorted CardIds
        let mut my_graveyard: Vec<u64> = view
            .my_graveyard
            .iter()
            .filter_map(|&id| view.objects.get(&id).filter(|inst| !inst.is_token).map(|inst| inst.card_def_id))
            .collect();
        my_graveyard.sort();

        let mut opp_graveyard: Vec<u64> = view
            .opp_graveyard
            .iter()
            .filter_map(|&id| view.objects.get(&id).filter(|inst| !inst.is_token).map(|inst| inst.card_def_id))
            .collect();
        opp_graveyard.sort();

        // Exile zones: sorted CardIds (public information)
        let mut my_exile: Vec<u64> = view
            .my_exile
            .iter()
            .filter_map(|&id| view.objects.get(&id).filter(|inst| !inst.is_token).map(|inst| inst.card_def_id))
            .collect();
        my_exile.sort();

        let mut opp_exile: Vec<u64> = view
            .opp_exile
            .iter()
            .filter_map(|&id| view.objects.get(&id).filter(|inst| !inst.is_token).map(|inst| inst.card_def_id))
            .collect();
        opp_exile.sort();

        // Mana: per-color breakdown
        let mana = &view.my_mana_pool;
        let my_mana = [
            mana.white,
            mana.blue,
            mana.black,
            mana.red,
            mana.green,
            mana.colorless,
        ];

        // Command zones: sorted CardIds
        let mut my_command_zone: Vec<u64> = view
            .my_command_zone
            .iter()
            .filter_map(|&id| view.objects.get(&id).filter(|inst| !inst.is_token).map(|inst| inst.card_def_id))
            .collect();
        my_command_zone.sort();

        let mut opp_command_zone: Vec<u64> = view
            .opp_command_zone
            .iter()
            .filter_map(|&id| view.objects.get(&id).filter(|inst| !inst.is_token).map(|inst| inst.card_def_id))
            .collect();
        opp_command_zone.sort();

        InformationSet {
            phase,
            active_player: view.active_player,
            turn_number: view.turn_number,
            priority_player: view.priority_player,
            my_life: view.my_life,
            opp_life: view.opp_life,
            my_hand,
            visible_zone_tokens: view.visible_zone_tokens.clone(),
            opp_hand_size: view.opp_hand_size,
            opp_library_size: view.opp_library_size,
            battlefield,
            stack_entries,
            pending_cast_spells,
            pending_zone_triggers,
            zone_normalization: joint.encoding.clone(),
            trigger_order_resume: view.trigger_order_resume,
            cleanup_needs_repeat: view.cleanup_needs_repeat,
            cleanup_discard_in_progress: view.cleanup_discard_in_progress,
            pending_copy_order,
            pending_failed_draws: view.pending_failed_draws.to_vec(),
            sba_preparation_failed: view.sba_preparation_failed,
            my_graveyard,
            opp_graveyard,
            my_exile,
            opp_exile,
            my_land_plays_remaining: view.my_land_plays_remaining,
            my_mana,
            my_command_zone,
            opp_command_zone,
            my_commander_tax: view.my_commander_tax,
            my_mulligan_count: view.my_mulligan_count,
        }

        })())
}

    /// Compute a stable hash for this information set.
    ///
    /// Two identical observable game states always produce the same hash.
    /// Collisions are possible but unlikely for practical info set spaces.
    pub fn hash_value(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.phase.hash(&mut hasher);
        self.active_player.hash(&mut hasher);
        self.turn_number.hash(&mut hasher);
        self.priority_player.hash(&mut hasher);
        self.my_life.hash(&mut hasher);
        self.opp_life.hash(&mut hasher);
        self.my_hand.hash(&mut hasher);
        if !self.visible_zone_tokens.is_empty() { self.visible_zone_tokens.hash(&mut hasher); }
        self.opp_hand_size.hash(&mut hasher);
        self.opp_library_size.hash(&mut hasher);
        self.battlefield.hash(&mut hasher);
        self.stack_entries.hash(&mut hasher);
        self.pending_cast_spells.hash(&mut hasher);
        self.pending_zone_triggers.hash(&mut hasher);
        if !self.zone_normalization.is_empty() { self.zone_normalization.hash(&mut hasher); }
        self.trigger_order_resume.hash(&mut hasher);
        self.cleanup_needs_repeat.hash(&mut hasher);
        self.cleanup_discard_in_progress.hash(&mut hasher);
        self.pending_copy_order.hash(&mut hasher);
        if self.sba_preparation_failed { "prepared_sba_failure".hash(&mut hasher); }
        if !self.pending_failed_draws.is_empty() {
            "pending_failed_draws".hash(&mut hasher);
            self.pending_failed_draws.hash(&mut hasher);
        }
        self.my_graveyard.hash(&mut hasher);
        self.opp_graveyard.hash(&mut hasher);
        self.my_exile.hash(&mut hasher);
        self.opp_exile.hash(&mut hasher);
        self.my_land_plays_remaining.hash(&mut hasher);
        self.my_mana.hash(&mut hasher);
        self.my_command_zone.hash(&mut hasher);
        self.opp_command_zone.hash(&mut hasher);
        self.my_commander_tax.hash(&mut hasher);
        self.my_mulligan_count.hash(&mut hasher);
        hasher.finish()
    }
}

/// Convert a Phase enum to a stable u8 for hashing.
fn phase_to_u8(phase: crate::game::Phase) -> u8 {
    use crate::game::Phase;
    match phase {
        Phase::Mulligan => 13,
        Phase::Untap => 0,
        Phase::Upkeep => 1,
        Phase::Draw => 2,
        Phase::PreCombatMain => 3,
        Phase::BeginningOfCombat => 4,
        Phase::DeclareAttackers => 5,
        Phase::DeclareBlockers => 6,
        Phase::FirstStrikeDamage => 7,
        Phase::CombatDamage => 8,
        Phase::EndOfCombat => 9,
        Phase::PostCombatMain => 10,
        Phase::EndStep => 11,
        Phase::Cleanup => 12,
    }
}

/// Convert a `StackEntry` into observable `StackInfo`.
fn stack_entry_to_info(
    entry: &StackEntry,
    view: &PlayerView,
    captured_zone: Option<crate::rules::transitions::ZoneOccurrenceInfo>,
) -> StackInfo {
    let objects = &view.objects;
    let stack = view.stack;
    let source_card_id = match &entry.source {
        StackSource::Spell(obj_id) => objects
            .get(obj_id)
            .map(|inst| inst.card_def_id)
            .unwrap_or(0),
        StackSource::ActivatedAbility { source_id, .. } => objects
            .get(source_id)
            .map(|inst| inst.card_def_id)
            .unwrap_or(0),
        StackSource::TriggeredAbility { context, .. } => context.source_card_id,
        StackSource::SpellCopy { definition } => definition.id,
        StackSource::EquipAbility { source_card_id, .. } => *source_card_id,
    };

    let target_summary = match &entry.source {
        StackSource::EquipAbility { target_card_id, .. } => {
            // The joint witness carries endpoint incidence. This owned summary
            // never falls back to a raw missing ID or a newer current object.
            let mut h = DefaultHasher::new();
            4u8.hash(&mut h);
            target_card_id.hash(&mut h);
            vec![h.finish()]
        }
        _ => summarize_targets(&entry.targets, objects, stack),
    };
    let cast_spell = match &entry.source {
        StackSource::TriggeredAbility { context, .. } => context.cast_spell.as_ref()
            .map(|spell| cast_spell_to_info(spell, objects, stack)),
        _ => None,
    };
    let zone_occurrence = captured_zone;

    StackInfo {
        controller: entry.controller,
        source_card_id,
        is_spell_copy: matches!(entry.source, StackSource::SpellCopy { .. }),
        target_summary,
        cast_spell,
        zone_occurrence,
    }
}

fn cast_spell_to_info(
    spell: &crate::game::CastSpellSnapshot,
    objects: &std::collections::HashMap<crate::card::ObjectId, &CardInstance>,
    stack: &[StackEntry],
) -> CastSpellInfo {
    CastSpellInfo {
        card_id: spell.definition.id,
        controller: spell.controller,
        live_stack_position: stack.iter().position(|entry| entry.id == spell.stack_id),
        target_summary: spell.targets.iter().enumerate().map(|(index, target)| {
            match target {
                crate::game::Target::Object(_) => {
                    let mut h = DefaultHasher::new();
                    1u8.hash(&mut h);
                    spell.historical_object_targets.get(index).hash(&mut h);
                    spell.target_generations.get(index).hash(&mut h);
                    h.finish()
                }
                _ => summarize_targets(std::slice::from_ref(target), objects, stack)[0],
            }
        }).collect(),
    }
}

fn summarize_targets(
    targets: &[crate::game::Target],
    objects: &std::collections::HashMap<crate::card::ObjectId, &CardInstance>,
    stack: &[StackEntry],
) -> Vec<u64> {
    targets.iter().map(|t| {
            let mut h = DefaultHasher::new();
            match t {
                crate::game::Target::StackEntry(id) => {
                    2u8.hash(&mut h);
                    stack.iter().position(|entry| entry.id == *id).hash(&mut h);
                }
                crate::game::Target::Player(idx) => {
                    0u8.hash(&mut h);
                    idx.hash(&mut h);
                }
                crate::game::Target::Object(obj_id) => {
                    // TODO: Preserve canonical permanent occurrence/incarnation relationships
                    // before relying heavily on solver strategy quality for targeted-spell states.
                    1u8.hash(&mut h);
                    if let Some(inst) = objects.get(obj_id) {
                        inst.card_def_id.hash(&mut h);
                        inst.controller.hash(&mut h);
                    } else {
                        obj_id.hash(&mut h);
                    }
                }
            }
            h.finish()
    }).collect()
}

// =========================================================================
// Phase 2B.1 — Information Set Abstraction
// =========================================================================

/// Trait for abstracting information sets to reduce the state space.
///
/// MCCFR training on realistic decks requires collapsing similar game states
/// into equivalence classes. An `InfoSetAbstraction` maps a detailed
/// `InformationSet` to a coarser hash, trading accuracy for memory/convergence.
///
/// # Design
///
/// - `IdentityAbstraction` — no-op passthrough (full info set, Phase 1B behavior)
/// - `BucketedAbstraction` — life bucketing, board aggregation, hand categorization,
///   turn bucketing (Phase 2B default for scaling to 60-card decks)
pub trait InfoSetAbstraction: Send + Sync {
    /// Map an information set to an abstract hash.
    fn abstract_info_set(&self, info_set: &InformationSet) -> u64;

    /// Name for logging/debugging.
    fn name(&self) -> &str;
}

/// Identity abstraction: uses the full information set hash (no compression).
///
/// This is equivalent to Phase 1B behavior — every distinct observable
/// game state gets its own regret table entry. Only feasible for small
/// game trees (mini decks).
pub struct IdentityAbstraction;

impl InfoSetAbstraction for IdentityAbstraction {
    fn abstract_info_set(&self, info_set: &InformationSet) -> u64 {
        info_set.hash_value()
    }

    fn name(&self) -> &str {
        "Identity"
    }
}

/// Bucketed abstraction for scaling MCCFR to realistic decks.
///
/// Reduces the information set space by bucketizing continuous values:
/// - **Life**: 10 buckets covering 0 through 41+ (supports both Standard
///   20-life and Commander 40-life formats)
/// - **Board**: permanent count per controller (no power/toughness — that
///   requires card_db; see `CardAwareBucketedAbstraction` for richer stats)
/// - **Hand**: hand size and opponent hand size (no role classification
///   without card_db; see `CardAwareBucketedAbstraction`)
/// - **Turn**: {early 0-3, mid 4-6, late 7+} (3 buckets)
/// - **Commander**: command zone occupancy and commander tax
///
/// This abstraction operates without a `CardDatabase` reference, making it
/// suitable for use in contexts where only the `InformationSet` is available.
pub struct BucketedAbstraction;

impl BucketedAbstraction {
    /// Bucket a life total into categories.
    ///
    /// Extended to 10 buckets to support Commander format (40 starting life).
    /// Standard format uses buckets 0-4; Commander uses the full range.
    fn life_bucket(life: i32) -> u8 {
        match life {
            i32::MIN..=0 => 0,
            1..=5 => 1,
            6..=10 => 2,
            11..=15 => 3,
            16..=20 => 4,
            21..=25 => 5,
            26..=30 => 6,
            31..=35 => 7,
            36..=40 => 8,
            _ => 9, // 41+
        }
    }

    /// Bucket a turn number into 3 categories.
    fn turn_bucket(turn: u32) -> u8 {
        match turn {
            0..=3 => 0,  // early
            4..=6 => 1,  // mid
            _ => 2,      // late
        }
    }
}

/// Check if an effect is removal (damage or destroy).
fn is_removal_effect(effect: &crate::card::Effect) -> bool {
    use crate::card::Effect;
    match effect {
        Effect::DealDamage { .. } => true,
        Effect::DestroyTarget { .. } => true,
        Effect::Multiple(effects) => effects.iter().any(is_removal_effect),
        _ => false,
    }
}

impl InfoSetAbstraction for BucketedAbstraction {
    fn abstract_info_set(&self, info_set: &InformationSet) -> u64 {
        let mut hasher = DefaultHasher::new();

        // Phase (exact — only 13 values)
        info_set.phase.hash(&mut hasher);

        // Active player (exact — only 2 values)
        info_set.active_player.hash(&mut hasher);

        // Turn bucket (3 values)
        Self::turn_bucket(info_set.turn_number).hash(&mut hasher);

        // Priority player (exact)
        info_set.priority_player.hash(&mut hasher);

        // Life buckets (5 values each)
        Self::life_bucket(info_set.my_life).hash(&mut hasher);
        Self::life_bucket(info_set.opp_life).hash(&mut hasher);

        // Hand categorization (lands, cheap, expensive, removal counts)
        // We don't have card_db in the abstract context, so we classify
        // by the raw card IDs in the info set.
        let hand_size = info_set.my_hand.len() as u8;
        hand_size.hash(&mut hasher);
        // Opponent hand size bucket
        let opp_hand_bucket = (info_set.opp_hand_size as u8).min(7);
        opp_hand_bucket.hash(&mut hasher);

        // Board state: aggregate creature count and total power by controller
        // Use PermanentInfo directly — count per controller
        let mut my_creatures: u32 = 0;
        let mut opp_creatures: u32 = 0;
        for perm in &info_set.battlefield {
            if perm.controller == info_set.priority_player {
                my_creatures += 1;
            } else {
                opp_creatures += 1;
            }
        }
        my_creatures.hash(&mut hasher);
        opp_creatures.hash(&mut hasher);

        // Stack: just hash whether stack is empty or has items
        let stack_nonempty = !info_set.stack_entries.is_empty();
        stack_nonempty.hash(&mut hasher);
        if !info_set.zone_normalization.is_empty() { info_set.zone_normalization.hash(&mut hasher); }
        if !info_set.pending_zone_triggers.is_empty() {
            info_set.pending_zone_triggers.hash(&mut hasher);
        }
        if info_set.stack_entries.iter().any(|entry| entry.zone_occurrence.is_some()) {
            for entry in &info_set.stack_entries {
                entry.zone_occurrence.hash(&mut hasher);
            }
        }
        info_set.pending_copy_order.hash(&mut hasher);
        if info_set.sba_preparation_failed { "prepared_sba_failure".hash(&mut hasher); }
        if !info_set.pending_failed_draws.is_empty() {
            "pending_failed_draws".hash(&mut hasher);
            info_set.pending_failed_draws.hash(&mut hasher);
        }
        info_set.trigger_order_resume.hash(&mut hasher);
        info_set.cleanup_needs_repeat.hash(&mut hasher);
        info_set.cleanup_discard_in_progress.hash(&mut hasher);

        // Mana availability: total mana (bucketed)
        let total_mana: u32 = info_set.my_mana.iter().sum();
        let mana_bucket = total_mana.min(10);
        mana_bucket.hash(&mut hasher);

        // Land plays remaining
        info_set.my_land_plays_remaining.hash(&mut hasher);

        // Commander: command zone occupancy and tax
        let my_cmd_count = info_set.my_command_zone.len() as u8;
        let opp_cmd_count = info_set.opp_command_zone.len() as u8;
        my_cmd_count.hash(&mut hasher);
        opp_cmd_count.hash(&mut hasher);
        info_set.my_commander_tax.hash(&mut hasher);

        // Mulligan count (distinguishes 1st mulligan from 2nd, etc.)
        info_set.my_mulligan_count.hash(&mut hasher);

        hasher.finish()
    }

    fn name(&self) -> &str {
        "Bucketed"
    }
}

/// Extended bucketed abstraction that uses the card database for richer
/// classification (hand roles, board power/toughness aggregates).
///
/// Unlike `BucketedAbstraction`, this requires a `CardDatabase` reference
/// and can classify hand cards by role (land, cheap, expensive, removal)
/// and aggregate board stats (total power, toughness, creature count).
pub struct CardAwareBucketedAbstraction<'a> {
    pub card_db: &'a crate::game::CardDatabase,
}

impl<'a> CardAwareBucketedAbstraction<'a> {
    /// Compute aggregate board stats for one side.
    /// Returns (total_power, total_toughness, creature_count).
    fn board_stats(battlefield: &[PermanentInfo], controller: usize, card_db: &crate::game::CardDatabase) -> (i32, i32, u32) {
        let mut total_power: i32 = 0;
        let mut total_toughness: i32 = 0;
        let mut creature_count: u32 = 0;

        for perm in battlefield {
            if perm.controller == controller {
                if let Some(def) = card_db.get(perm.card_id) {
                    if def.is_creature() {
                        let p = def.power.unwrap_or(0) + perm.plus_counters - perm.minus_counters;
                        let t = def.toughness.unwrap_or(0) + perm.plus_counters - perm.minus_counters;
                        total_power += p;
                        total_toughness += t;
                        creature_count += 1;
                    }
                }
            }
        }

        (total_power, total_toughness, creature_count)
    }

    /// Classify hand cards by role: (lands, cheap_spells <=2 cmc, expensive_spells >2 cmc, removal).
    fn hand_categories(hand: &[u64], card_db: &crate::game::CardDatabase) -> (u8, u8, u8, u8) {
        let mut lands: u8 = 0;
        let mut cheap: u8 = 0;
        let mut expensive: u8 = 0;
        let mut removal: u8 = 0;

        for &card_id in hand {
            if let Some(def) = card_db.get(card_id) {
                if def.is_land() {
                    lands += 1;
                } else {
                    let cmc = def.cmc();
                    if cmc <= 2 {
                        cheap += 1;
                    } else {
                        expensive += 1;
                    }
                    if let Some(ref effect) = def.spell_effect {
                        if is_removal_effect(effect) {
                            removal += 1;
                        }
                    }
                }
            }
        }

        (lands, cheap, expensive, removal)
    }
}

impl<'a> InfoSetAbstraction for CardAwareBucketedAbstraction<'a> {
    fn abstract_info_set(&self, info_set: &InformationSet) -> u64 {
        let mut hasher = DefaultHasher::new();

        // Phase (exact)
        info_set.phase.hash(&mut hasher);
        info_set.active_player.hash(&mut hasher);

        // Turn bucket
        BucketedAbstraction::turn_bucket(info_set.turn_number).hash(&mut hasher);

        // Priority player
        info_set.priority_player.hash(&mut hasher);

        // Life buckets
        BucketedAbstraction::life_bucket(info_set.my_life).hash(&mut hasher);
        BucketedAbstraction::life_bucket(info_set.opp_life).hash(&mut hasher);

        // Hand categorization (using card_db)
        let (lands, cheap, expensive, removal) =
            Self::hand_categories(&info_set.my_hand, self.card_db);
        lands.hash(&mut hasher);
        cheap.hash(&mut hasher);
        expensive.hash(&mut hasher);
        removal.hash(&mut hasher);

        // Opponent hand size bucket
        let opp_hand_bucket = (info_set.opp_hand_size as u8).min(7);
        opp_hand_bucket.hash(&mut hasher);

        // Board state with card_db (per-side aggregate stats)
        let me = info_set.priority_player;
        let opp = 1 - me;
        let (my_power, my_toughness, my_count) =
            Self::board_stats(&info_set.battlefield, me, self.card_db);
        let (opp_power, opp_toughness, opp_count) =
            Self::board_stats(&info_set.battlefield, opp, self.card_db);

        my_count.hash(&mut hasher);
        my_power.hash(&mut hasher);
        my_toughness.hash(&mut hasher);
        opp_count.hash(&mut hasher);
        opp_power.hash(&mut hasher);
        opp_toughness.hash(&mut hasher);

        // Stack size bucket
        let stack_size = info_set.stack_entries.len().min(3) as u8;
        stack_size.hash(&mut hasher);
        if !info_set.zone_normalization.is_empty() { info_set.zone_normalization.hash(&mut hasher); }
        if !info_set.pending_zone_triggers.is_empty() {
            info_set.pending_zone_triggers.hash(&mut hasher);
        }
        if info_set.stack_entries.iter().any(|entry| entry.zone_occurrence.is_some()) {
            for entry in &info_set.stack_entries {
                entry.zone_occurrence.hash(&mut hasher);
            }
        }
        info_set.pending_copy_order.hash(&mut hasher);
        if info_set.sba_preparation_failed { "prepared_sba_failure".hash(&mut hasher); }
        if !info_set.pending_failed_draws.is_empty() {
            "pending_failed_draws".hash(&mut hasher);
            info_set.pending_failed_draws.hash(&mut hasher);
        }
        info_set.trigger_order_resume.hash(&mut hasher);
        info_set.cleanup_needs_repeat.hash(&mut hasher);
        info_set.cleanup_discard_in_progress.hash(&mut hasher);

        // Total mana bucket
        let total_mana: u32 = info_set.my_mana.iter().sum();
        let mana_bucket = total_mana.min(10);
        mana_bucket.hash(&mut hasher);

        // Land plays remaining
        info_set.my_land_plays_remaining.hash(&mut hasher);

        // Commander: command zone occupancy and tax
        let my_cmd_count = info_set.my_command_zone.len() as u8;
        let opp_cmd_count = info_set.opp_command_zone.len() as u8;
        my_cmd_count.hash(&mut hasher);
        opp_cmd_count.hash(&mut hasher);
        info_set.my_commander_tax.hash(&mut hasher);

        // Mulligan count
        info_set.my_mulligan_count.hash(&mut hasher);

        hasher.finish()
    }

    fn name(&self) -> &str {
        "CardAwareBucketed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::sample;
    use crate::card::ZoneType;
    use crate::game::{GameState, Phase};
    use std::sync::Arc;

    fn setup_test_state() -> GameState {
        let db = sample::build_sample_db();
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        for _ in 0..20 {
            state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
            state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
        }
        state
    }

    #[test]
    fn pending_failed_draw_changes_each_information_set_hash_without_normalizer_changes() {
        let state = setup_test_state();
        let mut pending = state.clone();
        pending.loss_boundary.pending_failed_draws.push(0);
        let plain = InformationSet::from_view(&state.visible_state(0), state.card_db()).unwrap();
        let failed = InformationSet::from_view(&pending.visible_state(0), pending.card_db()).unwrap();
        assert_eq!(plain.zone_normalization, failed.zone_normalization);
        assert_ne!(IdentityAbstraction.abstract_info_set(&plain), IdentityAbstraction.abstract_info_set(&failed));
        assert_ne!(BucketedAbstraction.abstract_info_set(&plain), BucketedAbstraction.abstract_info_set(&failed));
        let aware = CardAwareBucketedAbstraction { card_db: state.card_db() };
        assert_ne!(aware.abstract_info_set(&plain), aware.abstract_info_set(&failed));
    }

    #[test]
    fn test_info_set_from_view_basic() {
        let mut state = setup_test_state();
        state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::PreCombatMain;

        let view = state.visible_state(0);
        let info_set = InformationSet::from_view(&view, state.card_db()).unwrap();

        assert_eq!(info_set.phase, 3); // PreCombatMain
        assert_eq!(info_set.active_player, 0);
        assert_eq!(info_set.my_life, 20);
        assert_eq!(info_set.opp_life, 20);
        assert_eq!(info_set.my_hand.len(), 1);
        assert_eq!(info_set.my_hand[0], sample::ids::LIGHTNING_BOLT);
        assert!(!info_set.battlefield.is_empty());
    }

    #[test]
    fn test_info_set_hash_deterministic() {
        let mut state = setup_test_state();
        state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::PreCombatMain;

        let view1 = state.visible_state(0);
        let info1 = InformationSet::from_view(&view1, state.card_db()).unwrap();

        let view2 = state.visible_state(0);
        let info2 = InformationSet::from_view(&view2, state.card_db()).unwrap();

        assert_eq!(info1.hash_value(), info2.hash_value());
    }

    #[test]
    fn test_info_set_different_hands_different_hash() {
        let mut state1 = setup_test_state();
        state1.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
        state1.active_player = 0;
        state1.priority_player = 0;
        state1.phase = Phase::PreCombatMain;

        let mut state2 = setup_test_state();
        state2.create_card_in_zone(sample::ids::GREY_OGRE, 0, ZoneType::Hand);
        state2.active_player = 0;
        state2.priority_player = 0;
        state2.phase = Phase::PreCombatMain;

        let view1 = state1.visible_state(0);
        let info1 = InformationSet::from_view(&view1, state1.card_db()).unwrap();

        let view2 = state2.visible_state(0);
        let info2 = InformationSet::from_view(&view2, state2.card_db()).unwrap();

        assert_ne!(info1.hash_value(), info2.hash_value());
    }

    #[test]
    fn test_info_set_same_observable_same_hash() {
        // Two game states that look the same from player 0's perspective
        // should produce the same info set hash, even if opponent's hidden
        // state differs.
        let mut state1 = setup_test_state();
        state1.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
        state1.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
        // Opponent has bolt in hand (hidden from player 0)
        state1.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 1, ZoneType::Hand);
        state1.active_player = 0;
        state1.priority_player = 0;
        state1.phase = Phase::PreCombatMain;

        let mut state2 = setup_test_state();
        state2.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
        state2.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
        // Opponent has Grey Ogre in hand instead (hidden from player 0)
        state2.create_card_in_zone(sample::ids::GREY_OGRE, 1, ZoneType::Hand);
        state2.active_player = 0;
        state2.priority_player = 0;
        state2.phase = Phase::PreCombatMain;

        let view1 = state1.visible_state(0);
        let info1 = InformationSet::from_view(&view1, state1.card_db()).unwrap();

        let view2 = state2.visible_state(0);
        let info2 = InformationSet::from_view(&view2, state2.card_db()).unwrap();

        // Same observable state, same hash (opponent hand contents hidden)
        assert_eq!(info1.hash_value(), info2.hash_value());
    }

    // ===================================================================
    // Phase 2B.1 — Abstraction tests
    // ===================================================================

    #[test]
    fn test_identity_abstraction_matches_hash() {
        let mut state = setup_test_state();
        state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::PreCombatMain;

        let view = state.visible_state(0);
        let info_set = InformationSet::from_view(&view, state.card_db()).unwrap();

        let identity = IdentityAbstraction;
        assert_eq!(
            identity.abstract_info_set(&info_set),
            info_set.hash_value(),
            "IdentityAbstraction should return the raw hash"
        );
    }

    #[test]
    fn test_bucketed_abstraction_collapses_similar_life() {
        // Two states with different life totals in the same bucket (16 vs 18)
        // should produce the same abstract hash.
        let mut state1 = setup_test_state();
        state1.active_player = 0;
        state1.priority_player = 0;
        state1.phase = Phase::PreCombatMain;
        state1.players[0].life = 16;

        let mut state2 = setup_test_state();
        state2.active_player = 0;
        state2.priority_player = 0;
        state2.phase = Phase::PreCombatMain;
        state2.players[0].life = 18;

        let view1 = state1.visible_state(0);
        let info1 = InformationSet::from_view(&view1, state1.card_db()).unwrap();

        let view2 = state2.visible_state(0);
        let info2 = InformationSet::from_view(&view2, state2.card_db()).unwrap();

        let bucketed = BucketedAbstraction;
        assert_eq!(
            bucketed.abstract_info_set(&info1),
            bucketed.abstract_info_set(&info2),
            "Life totals 16 and 18 should be in the same bucket"
        );

        // But full hash should differ
        assert_ne!(info1.hash_value(), info2.hash_value());
    }

    #[test]
    fn test_bucketed_abstraction_separates_different_life_buckets() {
        // Life 5 (bucket 1) vs life 15 (bucket 3) should differ.
        let mut state1 = setup_test_state();
        state1.active_player = 0;
        state1.priority_player = 0;
        state1.phase = Phase::PreCombatMain;
        state1.players[0].life = 5;

        let mut state2 = setup_test_state();
        state2.active_player = 0;
        state2.priority_player = 0;
        state2.phase = Phase::PreCombatMain;
        state2.players[0].life = 15;

        let view1 = state1.visible_state(0);
        let info1 = InformationSet::from_view(&view1, state1.card_db()).unwrap();

        let view2 = state2.visible_state(0);
        let info2 = InformationSet::from_view(&view2, state2.card_db()).unwrap();

        let bucketed = BucketedAbstraction;
        assert_ne!(
            bucketed.abstract_info_set(&info1),
            bucketed.abstract_info_set(&info2),
            "Life totals in different buckets should produce different abstract hashes"
        );
    }

    #[test]
    fn test_bucketed_abstraction_collapses_turn_bucket() {
        // Turn 1 and turn 3 are both "early" — should produce same hash.
        let mut state1 = setup_test_state();
        state1.active_player = 0;
        state1.priority_player = 0;
        state1.phase = Phase::PreCombatMain;
        state1.turn_number = 1;

        let mut state2 = setup_test_state();
        state2.active_player = 0;
        state2.priority_player = 0;
        state2.phase = Phase::PreCombatMain;
        state2.turn_number = 3;

        let view1 = state1.visible_state(0);
        let info1 = InformationSet::from_view(&view1, state1.card_db()).unwrap();

        let view2 = state2.visible_state(0);
        let info2 = InformationSet::from_view(&view2, state2.card_db()).unwrap();

        let bucketed = BucketedAbstraction;
        assert_eq!(
            bucketed.abstract_info_set(&info1),
            bucketed.abstract_info_set(&info2),
            "Turns 1 and 3 should be in the same 'early' bucket"
        );
    }

    #[test]
    fn test_bucketed_fewer_info_sets_than_identity() {
        // BucketedAbstraction should produce fewer unique hashes than
        // IdentityAbstraction for varied game states.
        let identity = IdentityAbstraction;
        let bucketed = BucketedAbstraction;

        let mut identity_hashes = std::collections::HashSet::new();
        let mut bucketed_hashes = std::collections::HashSet::new();

        for life in [5, 10, 15, 16, 17, 18, 19, 20, 25] {
            for turn in [1, 2, 3, 4, 5, 6, 7, 8, 10] {
                let mut state = setup_test_state();
                state.active_player = 0;
                state.priority_player = 0;
                state.phase = Phase::PreCombatMain;
                state.players[0].life = life;
                state.turn_number = turn;

                let view = state.visible_state(0);
                let info_set = InformationSet::from_view(&view, state.card_db()).unwrap();

                identity_hashes.insert(identity.abstract_info_set(&info_set));
                bucketed_hashes.insert(bucketed.abstract_info_set(&info_set));
            }
        }

        assert!(
            bucketed_hashes.len() < identity_hashes.len(),
            "Bucketed abstraction should produce fewer unique hashes ({}) than identity ({})",
            bucketed_hashes.len(),
            identity_hashes.len(),
        );
    }

    #[test]
    fn test_card_aware_bucketed_abstraction() {
        let db = sample::build_sample_db();
        let mut state = setup_test_state();
        state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand);
        state.create_card_in_zone(sample::ids::GREY_OGRE, 0, ZoneType::Battlefield);
        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::PreCombatMain;

        let view = state.visible_state(0);
        let info_set = InformationSet::from_view(&view, state.card_db()).unwrap();

        let card_aware = CardAwareBucketedAbstraction { card_db: &db };
        let hash = card_aware.abstract_info_set(&info_set);
        // Should be deterministic
        assert_eq!(hash, card_aware.abstract_info_set(&info_set));
    }

    #[test]
    fn test_life_bucket_values() {
        assert_eq!(BucketedAbstraction::life_bucket(0), 0);
        assert_eq!(BucketedAbstraction::life_bucket(-5), 0);
        assert_eq!(BucketedAbstraction::life_bucket(1), 1);
        assert_eq!(BucketedAbstraction::life_bucket(5), 1);
        assert_eq!(BucketedAbstraction::life_bucket(6), 2);
        assert_eq!(BucketedAbstraction::life_bucket(10), 2);
        assert_eq!(BucketedAbstraction::life_bucket(11), 3);
        assert_eq!(BucketedAbstraction::life_bucket(15), 3);
        assert_eq!(BucketedAbstraction::life_bucket(16), 4);
        assert_eq!(BucketedAbstraction::life_bucket(20), 4);
        assert_eq!(BucketedAbstraction::life_bucket(21), 5);
        assert_eq!(BucketedAbstraction::life_bucket(25), 5);
        assert_eq!(BucketedAbstraction::life_bucket(30), 6);
        assert_eq!(BucketedAbstraction::life_bucket(35), 7);
        assert_eq!(BucketedAbstraction::life_bucket(40), 8);
        assert_eq!(BucketedAbstraction::life_bucket(100), 9);
    }

    #[test]
    fn test_turn_bucket_values() {
        assert_eq!(BucketedAbstraction::turn_bucket(1), 0);
        assert_eq!(BucketedAbstraction::turn_bucket(3), 0);
        assert_eq!(BucketedAbstraction::turn_bucket(4), 1);
        assert_eq!(BucketedAbstraction::turn_bucket(6), 1);
        assert_eq!(BucketedAbstraction::turn_bucket(7), 2);
        assert_eq!(BucketedAbstraction::turn_bucket(100), 2);
    }
}
