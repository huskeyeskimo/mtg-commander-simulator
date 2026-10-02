//! Macro-actions and combo detection for the MCCFR solver.
//!
//! Infinite combos in MTG (e.g., Basalt Monolith + Kinnan = infinite mana)
//! require hundreds of individual actions to execute step-by-step, which
//! exhausts the solver's action budget and results in draws. This module
//! provides:
//!
//! 1. **Macro-actions**: Pre-defined combo sequences offered as single
//!    `Action::ActivateMacro` choices in the legal action list, letting the
//!    solver shortcut known loops without discovering them action-by-action.
//!
//! 2. **Combo proximity scoring**: Partial credit in the heuristic evaluator
//!    for having combo pieces on the battlefield, even before the full combo
//!    is assembled.
//!
//! # Design
//!
//! Combos are registered in a [`ComboRegistry`] attached to `GameState` via
//! `Arc` (zero-cost clone, same pattern as `card_db`). Each [`ComboDef`]
//! specifies:
//! - Required pieces (card IDs that must be on the battlefield)
//! - Preconditions (e.g., a piece must be untapped)
//! - The resulting effect (e.g., add N colorless mana)
//! - A reward-shaping weight for partial assembly
//!
//! When `legal_actions_with()` detects that all pieces of a registered combo
//! are on the battlefield and preconditions are met, it injects an
//! `Action::ActivateMacro { combo_id }` into the legal action list. The
//! rules engine applies the effect in a single state transition.

use serde::{Deserialize, Serialize};

use crate::card::{CardId, ObjectId};
use crate::game::{GameState, PlayerIndex};
use crate::mana::Color;

/// Amount used to represent "infinite" resources from combo loops.
/// Set to 1,000,000 — large enough that no spell or ability in the game
/// can exhaust it, but small enough to fit comfortably in u32/i32.
pub const INFINITE_AMOUNT: u32 = 1_000_000;

// =========================================================================
// Combo definitions
// =========================================================================

/// A registered combo that can be activated as a single macro-action.
#[derive(Debug, Clone)]
pub struct ComboDef {
    /// Unique identifier for this combo (index in the registry).
    pub id: usize,
    /// Human-readable name (e.g., "Basalt Monolith + Kinnan Infinite Mana").
    pub name: String,
    /// What kind of infinite loop this is (can be multiple).
    pub categories: Vec<ComboCategory>,
    /// Card IDs that must all be on the battlefield under the same controller.
    pub required_pieces: Vec<CardId>,
    /// Additional preconditions beyond having pieces on the battlefield.
    pub preconditions: Vec<ComboPrecondition>,
    /// Effect produced by activating the combo.
    pub effect: ComboEffect,
    /// Reward-shaping weight: how much partial credit to give for having
    /// some (but not all) pieces on the battlefield. Higher = more incentive
    /// to assemble the combo. Typical range: 0.05 to 0.3.
    pub reward_weight: f64,
}

/// Classification of what an infinite combo produces.
///
/// A single combo can belong to multiple categories (e.g., a token loop
/// with Blood Artist is both `InfiniteTokens` and `InfiniteDamage`).
/// Categories drive how `build_combo_effect` maps a discovered combo
/// to a concrete `ComboEffect` for the game engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComboCategory {
    /// Net-positive mana per cycle (e.g., Basalt Monolith + Kinnan).
    /// Effect: adds `INFINITE_AMOUNT` mana. Solver must still spend it.
    InfiniteMana,
    /// Net-positive creature tokens per cycle (e.g., Marrow-Gnawer + Thornbite).
    /// Effect: creates `INFINITE_AMOUNT` tokens. Solver attacks with them.
    InfiniteTokens,
    /// Deals damage each cycle (e.g., Blood Artist in a sacrifice loop).
    /// Effect: deals `INFINITE_AMOUNT` damage — instant win.
    InfiniteDamage,
    /// Gains life each cycle (e.g., Ayara in a token loop).
    /// Effect: gains `INFINITE_AMOUNT` life.
    InfiniteLifeGain,
    /// Draws cards each cycle.
    /// Effect: draws `INFINITE_AMOUNT` cards — risky, can deck out.
    InfiniteDraw,
    // Future categories:
    // InfiniteETB — triggers ETB effects per cycle (subsumes tokens+payoff)
    // InfiniteMill — mills opponent per cycle
    // InfiniteStorm — increments cast count per cycle
}

/// What executing the combo produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComboEffect {
    /// Add N colorless mana to the controller's pool.
    AddColorlessMana(u32),
    /// Add N mana of a specific color.
    AddColoredMana(Color, u32),
    /// Deal N damage to target opponent.
    DealDamageToOpponent(u32),
    /// Gain N life.
    GainLife(u32),
    /// Draw N cards.
    DrawCards(u32),
    /// Create N creature tokens (1/1 vanilla tokens for combat).
    CreateTokens(u32),
    /// Multiple effects applied in sequence.
    Multiple(Vec<ComboEffect>),
}

/// A precondition that must be satisfied beyond having all pieces on the battlefield.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComboPrecondition {
    /// A specific piece must be untapped (e.g., the mana rock that taps as
    /// part of the combo).
    PieceUntapped(CardId),
}

// =========================================================================
// Combo registry
// =========================================================================

/// Collection of known combos that the solver can activate as macro-actions.
#[derive(Debug, Clone, Default)]
pub struct ComboRegistry {
    pub combos: Vec<ComboDef>,
}

impl ComboRegistry {
    pub fn new() -> Self {
        ComboRegistry { combos: Vec::new() }
    }

    /// Register a combo. The combo's `id` is set to its index in the registry.
    pub fn register(&mut self, mut combo: ComboDef) -> usize {
        let id = self.combos.len();
        combo.id = id;
        self.combos.push(combo);
        id
    }

    /// Get a combo definition by ID.
    pub fn get(&self, id: usize) -> Option<&ComboDef> {
        self.combos.get(id)
    }
}

// =========================================================================
// Combo categories helper
// =========================================================================

/// Classify a combo's categories from its per-cycle outputs.
pub fn categorize(
    net_mana: u32,
    net_colored_mana: u32,
    net_creatures: u32,
    damage: u32,
    life: u32,
    draw: u32,
) -> Vec<ComboCategory> {
    let mut cats = Vec::new();
    if net_mana > 0 || net_colored_mana > 0 {
        cats.push(ComboCategory::InfiniteMana);
    }
    if net_creatures > 0 {
        cats.push(ComboCategory::InfiniteTokens);
    }
    if damage > 0 {
        cats.push(ComboCategory::InfiniteDamage);
    }
    if life > 0 {
        cats.push(ComboCategory::InfiniteLifeGain);
    }
    if draw > 0 {
        cats.push(ComboCategory::InfiniteDraw);
    }
    cats
}

/// Build a `ComboEffect` from a set of categories.
///
/// Mapping rules:
/// - `InfiniteDamage` → `DealDamageToOpponent(INFINITE_AMOUNT)` (instant win)
/// - `InfiniteMana` → `AddColorlessMana(INFINITE_AMOUNT)` (solver spends it)
/// - `InfiniteTokens` → `CreateTokens(INFINITE_AMOUNT)` (solver attacks)
/// - `InfiniteLifeGain` → `GainLife(INFINITE_AMOUNT)` (defensive)
/// - `InfiniteDraw` → `DrawCards(INFINITE_AMOUNT)` (risky)
///
/// When a combo has `InfiniteTokens` but NOT `InfiniteDamage`, we produce
/// tokens rather than assuming damage — the solver decides how to use them.
///
/// When `InfiniteDamage` is present, we skip `CreateTokens` and `GainLife`
/// since the opponent is already at negative 1M life — creating game objects
/// or gaining life is wasted work.
pub fn effect_from_categories(categories: &[ComboCategory]) -> ComboEffect {
    let has_infinite_damage = categories.contains(&ComboCategory::InfiniteDamage);
    let mut effects = Vec::new();

    for cat in categories {
        match cat {
            ComboCategory::InfiniteMana => {
                effects.push(ComboEffect::AddColorlessMana(INFINITE_AMOUNT));
            }
            ComboCategory::InfiniteTokens => {
                // Skip token creation when we already deal infinite damage —
                // creating 1M game objects is wasted work when opponent is dead
                if !has_infinite_damage {
                    effects.push(ComboEffect::CreateTokens(INFINITE_AMOUNT));
                }
            }
            ComboCategory::InfiniteDamage => {
                effects.push(ComboEffect::DealDamageToOpponent(INFINITE_AMOUNT));
            }
            ComboCategory::InfiniteLifeGain => {
                // Skip life gain when we already deal infinite damage
                if !has_infinite_damage {
                    effects.push(ComboEffect::GainLife(INFINITE_AMOUNT));
                }
            }
            ComboCategory::InfiniteDraw => {
                effects.push(ComboEffect::DrawCards(INFINITE_AMOUNT));
            }
        }
    }

    match effects.len() {
        0 => ComboEffect::AddColorlessMana(0),
        1 => effects.into_iter().next().unwrap(),
        _ => ComboEffect::Multiple(effects),
    }
}

// =========================================================================
// Combo detection
// =========================================================================

/// Check which registered combos are currently available for a player.
///
/// Returns a list of combo IDs that can be activated (all pieces present
/// on the battlefield under the player's control and all preconditions met).
pub fn detect_available_combos(
    state: &GameState,
    player: PlayerIndex,
    registry: &ComboRegistry,
) -> Vec<usize> {
    let mut available = Vec::new();

    let controlled: Vec<(ObjectId, CardId)> = state
        .battlefield
        .iter()
        .filter_map(|&obj_id| {
            let inst = state.objects.get(&obj_id)?;
            if inst.controller == player {
                Some((obj_id, inst.card_def_id))
            } else {
                None
            }
        })
        .collect();

    for combo in &registry.combos {
        // Check all required pieces are present
        let mut all_pieces_present = true;
        for &required_card_id in &combo.required_pieces {
            if !controlled.iter().any(|&(_, cid)| cid == required_card_id) {
                all_pieces_present = false;
                break;
            }
        }
        if !all_pieces_present {
            continue;
        }

        // Check preconditions
        let mut preconditions_met = true;
        for precond in &combo.preconditions {
            match precond {
                ComboPrecondition::PieceUntapped(card_id) => {
                    let untapped = controlled.iter().any(|&(obj_id, cid)| {
                        cid == *card_id && !state.objects[&obj_id].tapped
                    });
                    if !untapped {
                        preconditions_met = false;
                        break;
                    }
                }
            }
        }
        if !preconditions_met {
            continue;
        }

        available.push(combo.id);
    }

    available
}

// =========================================================================
// Combo proximity for reward shaping
// =========================================================================

/// Compute a combo proximity bonus for the heuristic evaluator.
///
/// For each registered combo, gives partial credit proportional to the
/// fraction of pieces the player has on the battlefield:
///   `bonus += (pieces_present / total_pieces) * combo.reward_weight`
///
/// When all pieces are assembled (but the macro hasn't been activated yet),
/// the full reward_weight is given. This incentivizes the solver to:
/// 1. Collect combo pieces (play Basalt Monolith, keep Kinnan alive)
/// 2. Activate the macro when available
///
/// Returns a value in [0.0, sum_of_all_weights] — typically [0.0, ~0.5].
pub fn combo_proximity_bonus(
    state: &GameState,
    player: PlayerIndex,
    registry: &ComboRegistry,
) -> f64 {
    let controlled_card_ids: Vec<CardId> = state
        .battlefield
        .iter()
        .filter_map(|&obj_id| {
            let inst = state.objects.get(&obj_id)?;
            if inst.controller == player {
                Some(inst.card_def_id)
            } else {
                None
            }
        })
        .collect();

    let mut bonus = 0.0;
    for combo in &registry.combos {
        if combo.required_pieces.is_empty() {
            continue;
        }
        let pieces_present = combo
            .required_pieces
            .iter()
            .filter(|&&cid| controlled_card_ids.contains(&cid))
            .count();
        let fraction = pieces_present as f64 / combo.required_pieces.len() as f64;
        bonus += fraction * combo.reward_weight;
    }
    bonus
}

// =========================================================================
// Applying combo effects
// =========================================================================

/// Apply a combo's effect to the game state.
///
/// Called by `apply_action()` when handling `Action::ActivateMacro`.
/// The combo pieces are tapped as a side effect (the Monolith that
/// produces mana ends up tapped after the infinite loop).
pub fn apply_combo_effect(
    state: &mut GameState,
    player: PlayerIndex,
    combo: &ComboDef,
) {
    if state.pending_copy_order.is_some() { return; }
    // Tap any piece that has a PieceUntapped precondition (it was tapped
    // as part of the combo loop).
    for precond in &combo.preconditions {
        match precond {
            ComboPrecondition::PieceUntapped(card_id) => {
                // Find the untapped instance and tap it
                for &obj_id in &state.battlefield {
                    let inst = &state.objects[&obj_id];
                    if inst.controller == player
                        && inst.card_def_id == *card_id
                        && !inst.tapped
                    {
                        if let Some(inst_mut) = state.objects.get_mut(&obj_id) {
                            inst_mut.tapped = true;
                        }
                        break;
                    }
                }
            }
        }
    }

    // Apply the effect
    apply_effect_recursive(state, player, &combo.effect);
}

fn apply_effect_recursive(
    state: &mut GameState,
    player: PlayerIndex,
    effect: &ComboEffect,
) {
    match effect {
        ComboEffect::AddColorlessMana(amount) => {
            state.players[player].mana_pool.colorless += amount;
        }
        ComboEffect::AddColoredMana(color, amount) => {
            state.players[player].mana_pool.add_color(*color, *amount);
        }
        ComboEffect::DealDamageToOpponent(amount) => {
            let opp = state.opponent(player);
            state.players[opp].life -= *amount as i32;
        }
        ComboEffect::GainLife(amount) => {
            state.players[player].life += *amount as i32;
        }
        ComboEffect::DrawCards(count) => {
            for _ in 0..*count {
                if !state.players[player].library.is_empty() {
                    let card_id = state.players[player].library.remove(0);
                    state.players[player].hand.push(card_id);
                }
            }
        }
        ComboEffect::CreateTokens(count) => {
            // Create vanilla 1/1 creature tokens on the battlefield.
            // The game engine tracks creatures as individual objects, so
            // we cap the actual object count at 100 (enough to represent
            // overwhelming board presence and lethal combat damage) even
            // when the combo produces INFINITE_AMOUNT tokens.
            use crate::card::TokenDef;
            let actual_count = (*count).min(100);
            let token_def = TokenDef {
                name: "Creature Token".into(),
                power: 1,
                toughness: 1,
                colors: vec![],
                subtypes: vec![],
                keywords: vec![],
            };
            for _ in 0..actual_count {
                crate::rules::create_token_from_combo(state, &token_def, player);
            }
        }
        ComboEffect::Multiple(effects) => {
            for sub_effect in effects {
                apply_effect_recursive(state, player, sub_effect);
            }
        }
    }
}

/// Register the "Infinite Mana + Walking Ballista = Instant Win" combo.
///
/// This is a 3-piece combo: Basalt Monolith + Kinnan + Walking Ballista.
/// When all three are on the battlefield with Monolith untapped, the player
/// can generate infinite mana AND deal infinite damage via Ballista's ability.
/// This collapses what would otherwise be hundreds of actions (generate mana,
/// activate Ballista 40 times) into a single macro-action.
pub fn register_ballista_win_combo(registry: &mut ComboRegistry) {
    use crate::card::catalog::ids;

    registry.register(ComboDef {
        id: 0, // Assigned by registry
        name: "Basalt Monolith + Kinnan + Walking Ballista Infinite Damage".into(),
        categories: vec![ComboCategory::InfiniteMana, ComboCategory::InfiniteDamage],
        required_pieces: vec![
            ids::BASALT_MONOLITH,
            ids::KINNAN_BONDER_PRODIGY,
            ids::WALKING_BALLISTA,
        ],
        preconditions: vec![ComboPrecondition::PieceUntapped(ids::BASALT_MONOLITH)],
        effect: ComboEffect::Multiple(vec![
            ComboEffect::AddColorlessMana(INFINITE_AMOUNT),
            ComboEffect::DealDamageToOpponent(INFINITE_AMOUNT),
        ]),
        reward_weight: 0.4, // Higher weight — this is an instant win
    });

    // Also register Grim Monolith variant
    registry.register(ComboDef {
        id: 0,
        name: "Grim Monolith + Kinnan + Walking Ballista Infinite Damage".into(),
        categories: vec![ComboCategory::InfiniteMana, ComboCategory::InfiniteDamage],
        required_pieces: vec![
            ids::GRIM_MONOLITH,
            ids::KINNAN_BONDER_PRODIGY,
            ids::WALKING_BALLISTA,
        ],
        preconditions: vec![ComboPrecondition::PieceUntapped(ids::GRIM_MONOLITH)],
        effect: ComboEffect::Multiple(vec![
            ComboEffect::AddColorlessMana(INFINITE_AMOUNT),
            ComboEffect::DealDamageToOpponent(INFINITE_AMOUNT),
        ]),
        reward_weight: 0.4,
    });
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::sample::{self, ids};
    use crate::card::ZoneType;
    use crate::combo_discovery::{discover_and_register, DiscoveryConfig};
    use std::sync::Arc;

    /// Build a test state + registry via combo discovery (Basalt + Kinnan).
    fn setup_state_with_combos() -> (GameState, ComboRegistry) {
        let db = sample::build_sample_db();
        let cards = vec![ids::BASALT_MONOLITH, ids::KINNAN_BONDER_PRODIGY];
        let config = DiscoveryConfig::default();
        let (registry, _) = discover_and_register(&db, &cards, &config);

        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));

        // Libraries so no one decks out
        for _ in 0..20 {
            state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
            state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
        }

        (state, registry)
    }

    #[test]
    fn test_detect_no_combo_pieces() {
        let (state, registry) = setup_state_with_combos();
        let available = detect_available_combos(&state, 0, &registry);
        assert!(available.is_empty(), "No combo pieces = no combos");
    }

    #[test]
    fn test_detect_partial_combo() {
        let (mut state, registry) = setup_state_with_combos();

        // Only Basalt Monolith, no Kinnan
        state.create_card_in_zone(ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);

        let available = detect_available_combos(&state, 0, &registry);
        assert!(available.is_empty(), "Missing Kinnan = combo not available");
    }

    #[test]
    fn test_detect_full_combo() {
        let (mut state, registry) = setup_state_with_combos();

        // Both pieces on battlefield
        state.create_card_in_zone(ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
        state.create_card_in_zone(ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

        // Monolith starts untapped, so combo should be available
        let available = detect_available_combos(&state, 0, &registry);
        assert!(
            available.contains(&0),
            "Basalt Monolith + Kinnan combo should be available"
        );
    }

    #[test]
    fn test_detect_combo_tapped_monolith() {
        let (mut state, registry) = setup_state_with_combos();

        let monolith = state.create_card_in_zone(ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
        state.create_card_in_zone(ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

        // Tap the monolith
        state.objects.get_mut(&monolith).unwrap().tapped = true;

        let available = detect_available_combos(&state, 0, &registry);
        assert!(
            !available.contains(&0),
            "Tapped Monolith = combo not available"
        );
    }

    #[test]
    fn test_detect_combo_wrong_controller() {
        let (mut state, registry) = setup_state_with_combos();

        // Monolith controlled by player 0, Kinnan controlled by player 1
        state.create_card_in_zone(ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
        state.create_card_in_zone(ids::KINNAN_BONDER_PRODIGY, 1, ZoneType::Battlefield);

        let available = detect_available_combos(&state, 0, &registry);
        assert!(
            available.is_empty(),
            "Pieces under different controllers = no combo"
        );
    }

    #[test]
    fn test_combo_proximity_no_pieces() {
        let (state, registry) = setup_state_with_combos();
        let bonus = combo_proximity_bonus(&state, 0, &registry);
        assert!(
            bonus.abs() < 1e-10,
            "No pieces = zero bonus, got {}",
            bonus
        );
    }

    #[test]
    fn test_combo_proximity_partial() {
        let (mut state, registry) = setup_state_with_combos();

        // Only Kinnan (1 of 2 pieces for Basalt Monolith combo)
        state.create_card_in_zone(ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

        let bonus = combo_proximity_bonus(&state, 0, &registry);
        // Kinnan is 1/2 of the Basalt combo, weight 0.2
        // bonus = 0.5 * 0.2 = 0.1
        assert!(
            (bonus - 0.1).abs() < 1e-10,
            "Half pieces = half weight, got {}",
            bonus
        );
    }

    #[test]
    fn test_combo_proximity_full() {
        let (mut state, registry) = setup_state_with_combos();

        // Both pieces for Basalt combo
        state.create_card_in_zone(ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
        state.create_card_in_zone(ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

        let bonus = combo_proximity_bonus(&state, 0, &registry);
        // Basalt combo: 2/2 * 0.2 = 0.2
        assert!(
            (bonus - 0.2).abs() < 1e-10,
            "Full combo = 0.2, got {}",
            bonus
        );
    }

    #[test]
    fn test_apply_combo_effect_adds_mana() {
        let (mut state, registry) = setup_state_with_combos();

        let monolith = state.create_card_in_zone(ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
        state.create_card_in_zone(ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

        let initial_colorless = state.players[0].mana_pool.colorless;
        apply_combo_effect(&mut state, 0, &registry.combos[0]);

        assert_eq!(
            state.players[0].mana_pool.colorless,
            initial_colorless + INFINITE_AMOUNT,
            "Combo should add INFINITE_AMOUNT colorless mana"
        );
        assert!(
            state.objects[&monolith].tapped,
            "Monolith should be tapped after combo activation"
        );
    }

}
