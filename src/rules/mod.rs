mod commander;
pub(crate) use commander::commander_cast_cost;
mod aura;
mod combat;
mod loss;
mod effects;
mod mana;
pub(crate) mod reclamation;
pub(crate) mod chittering;
pub(crate) mod matron;
mod phases;
mod resolution;
pub(crate) mod movement_choice;
mod spell_copy;
pub mod sba;
mod setup;
mod tokens;
mod triggers;
pub mod transitions;

use rand::Rng;

use crate::action::Action;
use crate::card::{CardType, KeywordAbility, ManaAbility, ObjectId, SacrificeCost, TriggerCondition, ZoneType};
use crate::events::{GameEvent, Zone};
use crate::game::{GameState, Phase, PlayerIndex, StackEntry, StackSource, TriggerOrderResume};
use crate::mana::Color;

// Public API re-exports
pub use mana::{total_cost_reduction, apply_cost_reduction, auto_tap_lands, spell_cost_reduction, total_cost_increase};
pub(crate) use mana::can_pay_cost;
pub use spell_copy::{begin_terminal_copy_batch, copy_spell_snapshot, copy_stack_spell, prepare_spell_copy, snapshot_stack_spell, CopyBatchOutcome, CopyError, CopyTargetPolicy};
pub use sba::check_state_based_actions;
pub use triggers::fire_triggers;
pub use setup::{setup_game, setup_game_seeded, setup_commander_game, setup_commander_game_seeded, setup_commander_game_with_partners, set_tutor_targets, reshuffle_opening_hand, validate_commander_deck, validate_commander_deck_with_partner};
pub(crate) use tokens::create_token_from_combo;

/// Required cleanup discards own the next action until the active hand is legal.
fn cleanup_discard_required(state: &GameState) -> bool {
    state.cleanup_discard_in_progress
        || (state.phase == Phase::Cleanup
            && !state.cleanup_needs_repeat
            && state.card_count(&state.players[state.active_player].hand) > 7)
}

/// A pending trigger-order decision owns the next action. The placement
/// machinery established the resume context; keep its APNAP chooser intact
/// even when an external caller supplies an action directly.
fn mandatory_trigger_order_chooser(state: &GameState) -> Option<PlayerIndex> {
    state.trigger_placement.as_ref().map(|op|op.controller)
}

/// Apply an action to the game state, advancing it.
pub fn apply_action(state: &mut GameState, action: &Action) {
    if state.gameplay_stopped() { return; }
    if matches!(action, Action::ActivateReclamation { .. }) && reclamation::prepare(state, action).is_none() { return; }
    if let Action::ActivateAbility { object_id, ability_index, .. } = action {
        if state.objects.get(object_id).is_some_and(|i| i.card_def_id == crate::card::sample::ids::PHYREXIAN_RECLAMATION
            || state.card_db.as_ref().and_then(|db| db.get(i.card_def_id)).and_then(|d| d.activated_abilities.get(*ability_index))
                .is_some_and(|a| reclamation::uses_selector(&a.effect))) { return; }
    }
    if state.movement_or_hand_pending() && state.card_db.is_none() { return; }
    if reclamation::validate_play(state).is_err() || chittering::validate_play(state).is_err()
        || matron::validate_play(state).is_err() { return; }
    if matches!(action, Action::ChooseChitteringCard { .. }) && !chittering::selection_actions(state).contains(action) { return; }
    if matches!(action, Action::PlaceChitteringTrigger { .. }) && !chittering::placement_actions(state).contains(action) { return; }
    if state.pending_chittering.is_some() && !matches!(action, Action::ChooseChitteringCard { .. } | Action::Concede) { return; }
    if state.pending_matron_search.is_some() && !matches!(action,
        Action::ChooseGoblinMatronCard { .. } | Action::DeclineGoblinMatronSearch { .. } | Action::FailToFindGoblinMatronCard { .. } | Action::Concede) { return; }
    if state.validate_commanders().is_err() || state.validate_commander_choices().is_err() || state.validate_movement_choices().is_err() {
        state.sba_failure = Some(sba::PreparedPassFailure::StateEncoding); return;
    }
    if let Action::CastCommander { object_id, .. } = action {
        if commander_cast_cost(state, *object_id).is_none() { return; }
    }
    if state.validate_trigger_placement().is_err() {
        state.sba_failure = Some(sba::PreparedPassFailure::StateEncoding); return;
    }
    if state.pending_movement.is_some()
        && !matches!(action, Action::ChooseMovementReplacement { .. } | Action::ChooseCommanderMovement { .. } | Action::ChooseLibraryPlacement { .. } | Action::Concede) { return; }
    if state.pending_commander_sba.is_some()
        && !matches!(action, Action::ChooseCommanderSba { .. } | Action::Concede) { return; }
    if state.trigger_placement.is_some()
        && !matches!(action, Action::ChooseNextTrigger { .. } | Action::PlaceChitteringTrigger { .. } | Action::Concede) { return; }
    if let Action::ChooseNextTrigger { handle } = action {
        if state.trigger_placement.as_ref().is_none_or(|op| op.window != handle.window)
            || !state.pending_triggers.iter().any(|t| t.placement == Some(*handle) && t.controller == state.priority_player) { return; }
    }
    if let Action::ChooseTutorTarget { card_id } = action {
        if state.pending_tutor.as_ref().is_some_and(|op| state.players.get(op.controller).is_some_and(|p|
            p.library.iter().any(|id| state.is_commander(*id) && state.objects[id].card_def_id == *card_id)))
            || state.replacement_effects.iter().any(|e| matches!(e.applies_to, crate::replacement::ReplacementEventKind::WouldMoveTo { .. })) {
            aura::record_entry_failure(state, transitions::TransitionError::UnsupportedPath); return;
        }
    }
    if let Err(reason) = aura::preflight_action(state, action) {
        state.sba_failure = Some(reason); return;
    }
    if matches!(action, Action::ChooseTutorTarget { .. })
        && state.pending_tutor.as_ref().is_some_and(|pending|
            pending.controller != state.priority_player || pending.controller >= state.players.len())
    { return; }
    if state.loss_action_in_progress {
        apply_action_at_boundary(state, action);
        return;
    }
    if crate::targeting::validate_pending_equips(state).is_err() {
        state.sba_failure = Some(sba::PreparedPassFailure::StateEncoding);
        return;
    }
    if let Action::Equip { equipment_id, target_id } = action {
        if crate::targeting::validate_equip_activation_structure(state, *equipment_id, *target_id).is_err() {
            state.sba_failure = Some(sba::PreparedPassFailure::StateEncoding);
            return;
        }
    }
    // Existing monotone mutation markers are exact for these handlers. All
    // other proposals retain the full canonical comparison, including absent
    // ability indices and malformed actor indices. No handler validity changes.
    enum Checkpoint {
        Pass(u32, bool),
        Stack(crate::game::StackId),
        Land(PlayerIndex, u32),
        Tap(ObjectId, Option<(bool, u32)>),
    }
    let checkpoint = match action {
        Action::ActivateReclamation { .. } => Some(Checkpoint::Stack(state.next_stack_id)),
        Action::PassPriority if u32::try_from(state.players.len())
            .is_ok_and(|players| players >= 2) =>
            Some(Checkpoint::Pass(state.consecutive_passes, state.pending_tutor.is_some())),
        Action::CastSpell { .. } | Action::CastCommander { .. }
            | Action::CastFromGraveyard { .. } => Some(Checkpoint::Stack(state.next_stack_id)),
        Action::ActivateAbility { object_id, ability_index, .. }
            if state.objects.get(object_id).and_then(|instance|
                state.card_db.as_ref().and_then(|db| db.get(instance.card_def_id)))
                .and_then(|def| def.activated_abilities.get(*ability_index)).is_some() =>
            Some(Checkpoint::Stack(state.next_stack_id)),
        Action::PlayLand { .. } | Action::PlayLandFromGraveyard { .. } =>
            state.players.get(state.priority_player)
                .map(|player| Checkpoint::Land(state.priority_player, player.land_plays_remaining)),
        Action::ActivateManaAbility { object_id, .. } =>
            Some(Checkpoint::Tap(*object_id, state.objects.get(object_id)
                .map(|instance| (instance.tapped, instance.zone_change_count)))),
        _ => None,
    };
    let mut before = Vec::new();
    if checkpoint.is_none() {
        bincode::serialize_into(&mut before, &*state).expect("canonical action state");
    }
    let active_before = state.active_player;
    state.loss_action_in_progress = true;
    apply_action_at_boundary(state, action);
    state.loss_action_in_progress = false;
    let changed = match checkpoint {
        Some(Checkpoint::Pass(passes, tutor)) =>
            (passes, tutor) != (state.consecutive_passes, state.pending_tutor.is_some()),
        Some(Checkpoint::Stack(stack_id)) => stack_id != state.next_stack_id,
        Some(Checkpoint::Land(player, remaining)) =>
            state.players[player].land_plays_remaining != remaining,
        Some(Checkpoint::Tap(object_id, source)) => source != state.objects.get(&object_id)
            .map(|instance| (instance.tapped, instance.zone_change_count)),
        None => {
            // Identical fixed-integer bytes, without serialize's sizing pass.
            let mut after = Vec::with_capacity(before.len());
            bincode::serialize_into(&mut after, &*state).expect("canonical action state");
            before != after
        }
    };
    if changed {
        state.loss_boundary.accepted_actions += 1;
        state.loss_boundary.turns_taken.resize(state.players.len(), 0);
        if state.loss_boundary.turns_taken.iter().all(|turns| *turns == 0) {
            state.loss_boundary.turns_taken[active_before] = 1;
        }
    }
}

fn apply_action_at_boundary(state: &mut GameState, action: &Action) {
    // These handlers complete synchronously. Defer their existing internal
    // flush attempts until costs, action consequences and SBA stabilization
    // have all finished. Continuation actions own their separate 2A boundaries.
    let owns_boundary = state.pending_copy_order.is_none() && !state.movement_or_hand_pending()
        && !cleanup_discard_required(state)
        && (mandatory_trigger_order_chooser(state).is_none() || matches!(action, Action::Concede))
        && matches!(action,
            Action::PlayLand { .. } | Action::PlayLandFromGraveyard { .. }
            | Action::CastSpell { .. } | Action::CastCommander { .. }
            | Action::CastFromGraveyard { .. } | Action::ActivateManaAbility { .. }
            | Action::ActivateAbility { .. } | Action::ActivateReclamation { .. } | Action::ActivateLoyalty { .. }
            | Action::Equip { .. } | Action::ActivateMacro { .. }
            | Action::ChooseTutorTarget { .. } | Action::Concede);
    if !owns_boundary {
        apply_action_inner(state, action);
        return;
    }
    let caller_deferred = std::mem::replace(&mut state.trigger_placement_deferred, true);
    let completed = apply_action_inner(state, action);
    finish_complete_action(state, caller_deferred, completed);
}

fn finish_complete_action(state: &mut GameState, caller_deferred: bool, completed: bool) {
    state.trigger_placement_deferred = caller_deferred;
    if completed && !caller_deferred {
        // Costs and action handlers also write hand/graveyard membership and
        // tapped state directly. Evaluate SBAs from the completed action.
        state.invalidate_characteristics_cache();
        sba::check_state_based_actions(state);
        if state.gameplay_stopped() || state.pending_commander_sba.is_some() { return; }
        clear_lost_tutor_choice(state);
        // Loss processing can remove the actor during this settlement. Keep
        // both immediate priority and a later ordering resume on a live player.
        if !state.game_over {
            if state.players[state.priority_player].has_lost {
                state.priority_player = state.next_player(state.priority_player);
            }
            if let Some(TriggerOrderResume::Player(player)) = state.trigger_order_resume {
                if state.players[player].has_lost {
                    state.trigger_order_resume = Some(TriggerOrderResume::Player(state.next_player(player)));
                }
            }
            // Elimination may remove the last pending chooser's occurrences.
            // Complete that existing placement continuation even without an
            // OrderTriggers action to consume it.
            if state.pending_triggers.is_empty() && state.trigger_order_resume.is_some() {
                resume_after_trigger_placement(state);
            }
        }
    }
}

/// Consume a completed placement window's existing continuation once.
fn resume_after_trigger_placement(state: &mut GameState) {
    if state.gameplay_stopped() { return; }
    match state.trigger_order_resume.take().unwrap_or(TriggerOrderResume::AfterResolution) {
        TriggerOrderResume::AfterResolution => restore_priority_after_resolution(state),
        TriggerOrderResume::Player(player) => {
            state.priority_player = if state.players[player].has_lost {
                state.next_player(player)
            } else { player };
        }
        TriggerOrderResume::AfterAttackers => {
            phases::transition_to_phase(state, Phase::DeclareBlockers);
            state.priority_player = state.next_player(state.active_player);
        }
    }
}

#[cfg(test)]
mod settlement_2b3a_boundary_tests {
    use super::*;
    use crate::card::CardDef;
    use crate::game::CardDatabase;
    use crate::layers::{AffectedObjects, StaticAbility};
    use crate::mana::ManaCost;
    use std::sync::Arc;

    #[test]
    fn loss_action_accounting_pass_matches_full_canonical_comparison() {
        for players in [1, 2, 3, 4] {
            for phase in Phase::TURN_ORDER {
                for passes in 0..=players as u32 + 2 {
                    for tutor in [false, true] {
                        for boundary in 0..4 {
                            let mut initial = GameState::new(players);
                            initial.card_db = Some(Arc::new(CardDatabase::new()));
                            initial.phase = phase;
                            initial.consecutive_passes = passes;
                            initial.loss_boundary.accepted_actions = 41;
                            if tutor {
                                initial.pending_tutor = Some(crate::game::PendingTutor {
                                    controller: 0, destination: ZoneType::Hand,
                                    subtype_filter: vec![],
                                });
                            }
                            match boundary {
                                1 => initial.players[0].life = 0,
                                2 => initial.players[0].has_lost = true,
                                3 => { initial.game_over = true; initial.winner = None; }
                                _ => {}
                            }
                            // Reference the original full-byte contract, not
                            // the optimized marker or handler completion bool.
                            let mut reference = initial.clone();
                            if !reference.gameplay_stopped() {
                                let before = bincode::serialize(&reference).unwrap();
                                reference.loss_action_in_progress = true;
                                apply_action_at_boundary(&mut reference, &Action::PassPriority);
                                reference.loss_action_in_progress = false;
                                if before != bincode::serialize(&reference).unwrap() {
                                    reference.loss_boundary.accepted_actions += 1;
                                    if reference.loss_boundary.turns_taken.iter().all(|turns| *turns == 0) {
                                        reference.loss_boundary.turns_taken[initial.active_player] = 1;
                                    }
                                }
                            }
                            let mut actual = initial.clone();
                            apply_action(&mut actual, &Action::PassPriority);
                            assert_eq!(bincode::serialize(&actual).unwrap(),
                                bincode::serialize(&reference).unwrap(),
                                "players={players} phase={phase:?} passes={passes} tutor={tutor} boundary={boundary}");
                            assert_eq!(actual.pending_events, reference.pending_events);
                        }
                    }
                }
            }
        }
    }

    fn canonical_action_reference(state: &mut GameState, action: &Action) {
        if state.gameplay_stopped() { return; }
        if state.loss_action_in_progress {
            apply_action_at_boundary(state, action);
            return;
        }
        let before = bincode::serialize(&*state).unwrap();
        let active = state.active_player;
        state.loss_action_in_progress = true;
        apply_action_at_boundary(state, action);
        state.loss_action_in_progress = false;
        if before != bincode::serialize(&*state).unwrap() {
            state.loss_boundary.accepted_actions += 1;
            state.loss_boundary.turns_taken.resize(state.players.len(), 0);
            if state.loss_boundary.turns_taken.iter().all(|turns| *turns == 0) {
                state.loss_boundary.turns_taken[active] = 1;
            }
        }
    }

    #[test]
    fn loss_action_accounting_markers_and_counted_helper_match_canonical_reference() {
        use crate::card::{ActivatedAbility, Effect};
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 995111, name: "Accounting spell".into(),
            card_types: vec![CardType::Sorcery], mana_cost: Some(ManaCost::zero()),
            spell_effect: Some(Effect::GainLife { amount: 1 }), ..Default::default() });
        db.insert(CardDef { id: 995112, name: "Unpaid accounting spell".into(),
            card_types: vec![CardType::Sorcery], mana_cost: Some(ManaCost::new(100, 0, 0, 0, 0, 0)),
            ..Default::default() });
        db.insert(CardDef { id: 995113, name: "Accounting land".into(),
            card_types: vec![CardType::Land], mana_abilities: vec![ManaAbility::TapForColorless],
            ..Default::default() });
        db.insert(CardDef { id: 995114, name: "Departing accounting source".into(),
            card_types: vec![CardType::Creature], power: Some(1), toughness: Some(0),
            mana_abilities: vec![ManaAbility::TapForColorlessAmount(0)], ..Default::default() });
        db.insert(CardDef { id: 995115, name: "Accounting activated source".into(),
            card_types: vec![CardType::Artifact], activated_abilities: vec![ActivatedAbility {
                cost: ManaCost::zero(), requires_tap: true,
                sacrifice_cost: Some(SacrificeCost::SelfSacrifice), life_cost: 1,
                effect: Effect::GainLife { amount: 1 }, description: "accounting control".into(),
            }], ..Default::default() });
        for players in [2, 3, 4] {
            let mut base = GameState::new(players);
            base.card_db = Some(Arc::new(db.clone()));
            base.phase = Phase::PreCombatMain;
            base.loss_boundary.accepted_actions = 41;
            for player in 0..players {
                for _ in 0..3 { base.create_card_in_zone(995113, player, ZoneType::Library).unwrap(); }
            }
            let hand = base.create_card_in_zone(995111, 0, ZoneType::Hand).unwrap();
            let grave = base.create_card_in_zone(995111, 0, ZoneType::Graveyard).unwrap();
            let command = base.create_card_in_zone(995111, 0, ZoneType::Command).unwrap();
            let unpaid = base.create_card_in_zone(995112, 0, ZoneType::Hand).unwrap();
            let land = base.create_card_in_zone(995113, 0, ZoneType::Hand).unwrap();
            let grave_land = base.create_card_in_zone(995113, 0, ZoneType::Graveyard).unwrap();
            let mana_source = base.create_card_in_zone(995113, 0, ZoneType::Battlefield).unwrap();
            let departing = base.create_card_in_zone(995114, 0, ZoneType::Battlefield).unwrap();
            base.objects.get_mut(&departing).unwrap().summoning_sick = false;
            let ability = base.create_card_in_zone(995115, 0, ZoneType::Battlefield).unwrap();
            let actions = [
                Action::CastSpell { object_id: hand, targets: vec![] },
                Action::CastSpell { object_id: unpaid, targets: vec![] },
                Action::CastSpell { object_id: grave, targets: vec![] },
                Action::CastCommander { object_id: command, targets: vec![] },
                Action::CastFromGraveyard { object_id: grave, targets: vec![] },
                Action::PlayLand { object_id: land },
                Action::PlayLandFromGraveyard { object_id: grave_land },
                Action::ActivateManaAbility { object_id: mana_source, ability_index: 0 },
                Action::ActivateManaAbility { object_id: mana_source, ability_index: 99 },
                Action::ActivateManaAbility { object_id: departing, ability_index: 0 },
                Action::ActivateManaAbility { object_id: u64::MAX, ability_index: 0 },
                Action::ActivateAbility { object_id: ability, ability_index: 0, targets: vec![] },
                Action::ActivateAbility { object_id: ability, ability_index: 99, targets: vec![] },
            ];
            for scenario in 0..12 {
                for passes in [0, 1] {
                    let mut initial = base.clone();
                    initial.consecutive_passes = passes;
                    match scenario {
                        1 => initial.players[0].life = 0,
                        2 => initial.players[0].poison_counters = 10,
                        3 => initial.loss_boundary.pending_failed_draws.push(0),
                        4 => initial.players[0].has_lost = true,
                        5 => initial.game_over = true,
                        6 => {
                            initial.trigger_order_resume = Some(TriggerOrderResume::AfterResolution);
                            initial.pending_triggers = (0..2).map(|index| crate::game::PendingTrigger { placement: None,
                                source_id: mana_source, ability_index: index, controller: 0, targets: vec![],
                                context: crate::game::TriggerContext { source_card_id: 995113,
                                    source_generation: 0, effect: Effect::GainLife { amount: 1 },
                                    cast_spell: None, zone_transition: None },
                            }).collect();
                            let _ = triggers::flush_triggers(&mut initial);
                        }
                        7 => { initial.phase = Phase::Cleanup; initial.cleanup_discard_in_progress = true; }
                        8 => { for id in [mana_source, departing, ability] {
                            initial.objects.get_mut(&id).unwrap().tapped = true;
                        } }
                        9 => initial.trigger_placement_deferred = true,
                        10 => initial.loss_action_in_progress = true,
                        11 => initial.priority_player = 1,
                        _ => {}
                    }
                    let mut snapshot = initial.clone(); snapshot.restore(initial.snapshot()).unwrap();
                    let mut restored = [initial.clone(), snapshot,
                        serde_json::from_slice::<GameState>(&serde_json::to_vec(&initial).unwrap()).unwrap(),
                        bincode::deserialize::<GameState>(&bincode::serialize(&initial).unwrap()).unwrap()];
                    for (roundtrip, candidate) in restored.iter_mut().enumerate() {
                        candidate.card_db = initial.card_db.clone();
                        for action in &actions {
                            let mut reference = candidate.clone();
                            canonical_action_reference(&mut reference, action);
                            let mut actual = candidate.clone();
                            apply_action(&mut actual, action);
                            assert_eq!(bincode::serialize(&actual).unwrap(), bincode::serialize(&reference).unwrap(),
                                "players={players} scenario={scenario} passes={passes} restore={roundtrip} action={action:?}");
                            assert_eq!(actual.pending_events, reference.pending_events);
                            let mut counted_reference = candidate.clone();
                            let expected = if counted_reference.unsupported_continuing_elimination() {
                                Err(crate::simulation::TerminationReason::UnsupportedContinuingElimination)
                            } else if counted_reference.gameplay_stopped() { Ok(false) } else {
                                let before = bincode::serialize(&counted_reference).unwrap();
                                canonical_action_reference(&mut counted_reference, action);
                                if counted_reference.unsupported_continuing_elimination() {
                                    Err(crate::simulation::TerminationReason::UnsupportedContinuingElimination)
                                } else { Ok(before != bincode::serialize(&counted_reference).unwrap()) }
                            };
                            let mut counted = candidate.clone();
                            let result = crate::simulation::apply_counted_action(&mut counted, action, std::slice::from_ref(action));
                            assert_eq!(result, expected);
                            assert_eq!(bincode::serialize(&counted).unwrap(), bincode::serialize(&counted_reference).unwrap());
                            assert_eq!(counted.pending_events, counted_reference.pending_events);
                        }
                    }
                }
            }
        }
    }

    fn fixture() -> (GameState, Action, ObjectId) {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 995101, name: "Boundary victim".into(),
            card_types: vec![CardType::Creature], power: Some(1), toughness: Some(1),
            ..Default::default() });
        db.insert(CardDef { id: 995102, name: "Boundary equipment".into(),
            card_types: vec![CardType::Artifact], subtypes: vec![crate::card::Subtype("Equipment".into())], equip_cost: Some(ManaCost::zero()),
            static_abilities: vec![StaticAbility::Anthem { power: 0, toughness: -1,
                affected: AffectedObjects::AttachedTo }], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::PreCombatMain;
        let equipment_id = state.create_card_in_zone(995102, 0, ZoneType::Battlefield).unwrap();
        let target_id = state.create_card_in_zone(995101, 0, ZoneType::Battlefield).unwrap();
        let victim = state.create_card_in_zone(995101, 0, ZoneType::Battlefield).unwrap();
        state.objects.get_mut(&victim).unwrap().temp_toughness_mod = -1;
        state.invalidate_characteristics_cache();
        (state, Action::Equip { equipment_id, target_id }, victim)
    }

    #[test]
    fn restored_private_post_action_seam_finishes_once() {
        let (mut state, action, victim) = fixture();
        state.trigger_placement_deferred = true;
        assert!(apply_action_inner(&mut state, &action));
        assert!(state.battlefield.contains(&victim));
        let mut snapshot = state.clone();
        snapshot.restore(state.snapshot()).unwrap();
        let mut variants = [state.clone(), snapshot,
            serde_json::from_slice::<GameState>(&serde_json::to_vec(&state).unwrap()).unwrap(),
            bincode::deserialize::<GameState>(&bincode::serialize(&state).unwrap()).unwrap()];
        let mut outcomes = Vec::new();
        for candidate in &mut variants {
            candidate.card_db = state.card_db.clone();
            // Resume this private complete-action seam explicitly; no new
            // persisted production continuation or between-pass state exists.
            finish_complete_action(candidate, false, true);
            assert!(candidate.players[0].graveyard.contains(&victim));
            let before = bincode::serialize(candidate).unwrap();
            sba::check_state_based_actions(candidate);
            assert_eq!(before, bincode::serialize(candidate).unwrap());
            outcomes.push(serde_json::to_value(&*candidate).unwrap());
        }
        assert!(outcomes.iter().all(|outcome| *outcome == outcomes[0]));
    }

    #[test]
    fn enclosing_atomic_operation_retains_deferral_and_owns_settlement() {
        let (mut state, action, victim) = fixture();
        state.trigger_placement_deferred = true;
        apply_action(&mut state, &action);
        assert!(state.trigger_placement_deferred);
        assert!(state.battlefield.contains(&victim));
        finish_complete_action(&mut state, false, true);
        assert!(state.players[0].graveyard.contains(&victim));
    }
}

fn apply_action_inner(state: &mut GameState, action: &Action) -> bool {
    if state.pending_chittering.is_some() {
        if matches!(action, Action::Concede) { loss::adjudicate(state, Some(state.priority_player)); return true; }
        return chittering::choose_card(state, action);
    }
    if state.pending_movement.is_some() {
        if matches!(action, Action::Concede) { loss::adjudicate(state, Some(state.priority_player)); return true; }
        return movement_choice::choose(state, action);
    }
    if state.pending_matron_search.is_some() {
        if matches!(action, Action::Concede) { loss::adjudicate(state, Some(state.priority_player)); return true; }
        return matron::choose(state, action);
    }
    if matches!(action, Action::ChooseMovementReplacement { .. } | Action::ChooseCommanderMovement { .. } | Action::ChooseLibraryPlacement { .. }) { return false; }
    if state.pending_commander_sba.is_some() {
        return match action {
            Action::ChooseCommanderSba { designation, to_command } => {
                if !sba::choose_commander_sba(state, *designation, *to_command) { return false; }
                if state.pending_commander_sba.is_none() && !state.gameplay_stopped()
                    && state.trigger_order_resume.is_some() {
                    if !state.pending_triggers.is_empty() && !triggers::flush_triggers(state) { return true; }
                    resume_after_trigger_placement(state);
                }
                true
            }
            Action::Concede => { loss::adjudicate(state, Some(state.priority_player)); true }
            _ => false,
        };
    }
    if matches!(action, Action::ChooseCommanderSba { .. }) { return false; }
    if state.pending_copy_order.is_some() || state.movement_or_hand_pending() {
        if let Action::ChooseNextCopy { item_index } = action {
            let _ = spell_copy::choose_next_copy(state, *item_index);
        }
        return false;
    }
    if matches!(action, Action::ChooseNextCopy { .. } | Action::ChooseChitteringCard { .. }
        | Action::ChooseGoblinMatronCard { .. } | Action::DeclineGoblinMatronSearch { .. } | Action::FailToFindGoblinMatronCard { .. }) { return false; }
    if cleanup_discard_required(state) {
        let Action::Discard { object_id } = action else { return false; };
        if state.phase != Phase::Cleanup
            || state.cleanup_needs_repeat
            || state.priority_player != state.active_player
            || state.card_count(&state.players[state.active_player].hand) <= 7
            || (!state.players[state.active_player].hand.contains(object_id) || !state.is_card(*object_id))
        {
            return false;
        }
    }
    if let Some(chooser) = mandatory_trigger_order_chooser(state) {
        let valid_kind = matches!(action, Action::ChooseNextTrigger { .. } | Action::PlaceChitteringTrigger { .. } | Action::Concede);
        if state.priority_player != chooser || !valid_kind { return false; }
    }
    // Validate supplied targets before costs, zone changes, or cast events.
    let cast = match action {
        Action::CastSpell { object_id, targets } => Some((*object_id, targets, ZoneType::Hand)),
        Action::CastFromGraveyard { object_id, targets } => Some((*object_id, targets, ZoneType::Graveyard)),
        Action::CastCommander { object_id, targets } => Some((*object_id, targets, ZoneType::Command)),
        _ => None,
    };
    if let Some((id, targets, zone)) = cast {
        let player = state.priority_player;
        let zone_cards = match zone {
            ZoneType::Hand => &state.players[player].hand,
            ZoneType::Graveyard => &state.players[player].graveyard,
            _ => &state.players[player].command_zone,
        };
        if !zone_cards.contains(&id) || !state.is_card(id) { return false; }
        let Some(def) = state.objects.get(&id).and_then(|inst| state.card_db().get(inst.card_def_id)) else { return false; };
        if def.is_aura() {
            if zone != ZoneType::Hand || def.enchant != crate::card::EnchantRestriction::Creature {
                aura::record_entry_failure(state, transitions::TransitionError::UnsupportedPath); return false;
            }
            if !crate::targeting::aura_cast_context(state, id) { return false; }
        }
        if !crate::targeting::valid_spell_targets(state, player, def, targets) { return false; }
    }
    match action {
        Action::ChooseNextCopy { .. } | Action::ChooseCommanderSba { .. } | Action::ChooseMovementReplacement { .. } | Action::ChooseCommanderMovement { .. } | Action::ChooseLibraryPlacement { .. }
        | Action::ChooseGoblinMatronCard { .. } | Action::DeclineGoblinMatronSearch { .. } | Action::FailToFindGoblinMatronCard { .. } => unreachable!(),
        Action::ActivateReclamation { .. } => reclamation::activate(state, action),
        Action::PassPriority => {
            // If there's a pending tutor, passing means "fail to find" —
            // clear it and return without advancing priority normally.
            if state.pending_tutor.is_some() {
                state.pending_tutor = None;
                return false;
            }

            if state.phase == Phase::Cleanup
                && !state.cleanup_needs_repeat
                && state.card_count(&state.players[state.active_player].hand) > 7
            {
                debug_assert!(
                    false,
                    "PassPriority during cleanup discard is illegal; choose a Discard action."
                );
                return false;
            }
            state.consecutive_passes += 1;
            phases::handle_priority_pass(state);
        }

        Action::Discard { object_id } => {
            if state.phase != Phase::Cleanup {
                return false;
            }
            if state.cleanup_needs_repeat {
                return false;
            }
            if state.priority_player != state.active_player {
                return false;
            }
            let player = state.active_player;
            if !state.players[player].hand.contains(object_id) || !state.is_card(*object_id) {
                return false;
            }
            if state.card_count(&state.players[player].hand) <= 7 {
                return false;
            }

            state.cleanup_discard_in_progress = true;

            state.move_object(*object_id, ZoneType::Hand, ZoneType::Graveyard);
            state.consecutive_passes = 0;
            state.priority_player = player;

            // Fire discard triggers (e.g., Monument to Endurance)
            triggers::check_triggers(state, TriggerCondition::YouDiscardACard, None);

            if state.card_count(&state.players[player].hand) <= 7 {
                state.cleanup_discard_in_progress = false;
                phases::finalize_cleanup(state);
            }
        }

        Action::PlayLand { object_id } => {
            let obj_id = *object_id;
            if !state.is_card(obj_id) { return false; }
            if let Err(error) = state.preflight_noncast_entry(obj_id, ZoneType::Battlefield) {
                aura::record_entry_failure(state, error); return false;
            }
            let player = state.priority_player;
            state.players[player].land_plays_remaining -= 1;
            state.move_object(obj_id, ZoneType::Hand, ZoneType::Battlefield);
            // Lands enter untapped by default (we'd check for "enters tapped" later)
            if let Some(inst) = state.objects.get_mut(&obj_id) {
                inst.tapped = false;
                inst.summoning_sick = false; // lands don't have summoning sickness
            }
            state.refresh_continuous_effects();
            if state.gameplay_stopped() { return false; }
            // Fire ETB triggers on the land itself (e.g., Mystic Sanctuary)
            triggers::check_triggers(state, TriggerCondition::EntersBattlefield, Some(obj_id));
            // Fire landfall triggers on all permanents
            triggers::check_triggers(state, TriggerCondition::ALandYouControlEnters, None);
            // Fire "whenever you play a land" triggers
            triggers::check_triggers(state, TriggerCondition::YouPlayALand, None);
            state.consecutive_passes = 0;
        }

        Action::PlayLandFromGraveyard { object_id } => {
            let obj_id = *object_id;
            if !state.is_card(obj_id) { return false; }
            if let Err(error) = state.preflight_noncast_entry(obj_id, ZoneType::Battlefield) {
                aura::record_entry_failure(state, error); return false;
            }
            let player = state.priority_player;
            state.players[player].land_plays_remaining -= 1;
            state.move_object(obj_id, ZoneType::Graveyard, ZoneType::Battlefield);
            if let Some(inst) = state.objects.get_mut(&obj_id) {
                inst.tapped = false;
                inst.summoning_sick = false;
            }
            state.refresh_continuous_effects();
            if state.gameplay_stopped() { return false; }
            // Fire ETB triggers on the land itself
            triggers::check_triggers(state, TriggerCondition::EntersBattlefield, Some(obj_id));
            // Fire landfall triggers on all permanents
            triggers::check_triggers(state, TriggerCondition::ALandYouControlEnters, None);
            // Fire "whenever you play a land" triggers
            triggers::check_triggers(state, TriggerCondition::YouPlayALand, None);
            state.consecutive_passes = 0;
        }

        Action::CastSpell { object_id, targets } => {
            let obj_id = *object_id;
            let player = state.priority_player;
            let db = state.card_db();
            let inst = &state.objects[&obj_id];
            let def = db.get(inst.card_def_id).unwrap().clone();
            let is_creature = def.is_creature();

            // Pay mana cost (with cost reduction, spell keywords, and tax effects)
            let card_def_id = {
                state.objects[&obj_id].card_def_id
            };
            if let Some(ref cost) = def.mana_cost {
                let reduction = mana::total_cost_reduction(state, player, is_creature);
                let spell_reduction = mana::spell_cost_reduction(state, player, card_def_id);
                let tax = mana::total_cost_increase(state, player, is_creature);
                let mut reduced_cost = mana::apply_cost_reduction(cost, reduction + spell_reduction);
                reduced_cost.generic += tax;
                // First, auto-tap lands to generate mana if pool is insufficient
                if !mana::pay_cost(state, player, &reduced_cost, None) {
                    return false;
                }
            }

            // Move to stack
            let stack_id = state.new_stack_id();
            state.stack.push(StackEntry {
                id: stack_id,
                source: if def.is_aura() {
                    let crate::game::Target::Object(target) = targets[0] else { unreachable!() };
                    let inst = &state.objects[&obj_id];
                    let target_inst = &state.objects[&target];
                    StackSource::AuraSpell { source: crate::card::ExactObjectRef { id: obj_id, generation: inst.zone_change_count + 1 },
                        source_card_id: inst.card_def_id, source_owner: inst.owner,
                        target: state.exact_object(target).unwrap(), target_card_id: target_inst.card_def_id,
                        target_owner: target_inst.owner, target_controller: state.get_characteristics(target).unwrap().controller,
                        target_is_token: target_inst.is_token, restriction: def.enchant }
                } else { StackSource::Spell(obj_id) },
                controller: player,
                targets: targets.clone(),
                target_generations: crate::targeting::target_generations(state, targets),
            });
            // Casting is a zone change even though stack placement is explicit.
            state.objects.get_mut(&obj_id).unwrap().zone_change_count += 1;
            // Remove from hand (but don't put in a zone yet — it's on the stack)
            state.players[player].hand.retain(|&id| id != obj_id);

            state.emit_event(GameEvent::SpellCast {
                object: obj_id,
                controller: player,
            });
            state.emit_event(GameEvent::ZoneChange {
                object: obj_id,
                from: Zone::Hand,
                to: Zone::Stack,
            });

            // Fire spell-cast triggers (YouCastSpell, OpponentCastsSpell, etc.)
            triggers::fire_spell_cast_triggers(state, player, is_creature, stack_id);

            state.consecutive_passes = 0;
        }

        Action::ActivateManaAbility {
            object_id,
            ability_index,
        } => {
            let obj_id = *object_id;
            let idx = *ability_index;
            let player = state.priority_player;

            if !state.can_pay_tap_cost(obj_id) {
                return false;
            }

            // Read mana ability and source properties in one borrow scope
            let (ma, source_is_nonland, source_is_swamp) = {
                let db = state.card_db();
                let inst = &state.objects[&obj_id];
                let def = db.get(inst.card_def_id).unwrap();
                (
                    def.mana_abilities.get(idx).cloned(),
                    !def.card_types.contains(&CardType::Land),
                    def.subtypes.iter().any(|s| s.0 == "Swamp"),
                )
            };

            if let Some(ma) = ma {
                match &ma {
                    ManaAbility::TapForColor(color) => {
                        state.players[player].mana_pool.add_color(*color, 1);
                    }
                    ManaAbility::TapForColorless => {
                        state.players[player].mana_pool.colorless += 1;
                    }
                    ManaAbility::TapForAny => {
                        state.players[player].mana_pool.colorless += 1;
                    }
                    ManaAbility::TapForLegendaryColors => {
                        // Mox Amber: add one mana of any color among
                        // legendary creatures/planeswalkers you control.
                        let leg_colors = mana::legendary_colors(state, player);
                        if let Some(&c) = leg_colors.first() {
                            state.players[player].mana_pool.add_color(c, 1);
                        }
                        // If no legendary creature/planeswalker, produces nothing
                    }
                    ManaAbility::TapForChoice(colors) => {
                        if let Some(&color) = colors.first() {
                            state.players[player].mana_pool.add_color(color, 1);
                        }
                    }
                    ManaAbility::TapForColorlessAmount(n) => {
                        state.players[player].mana_pool.colorless += n;
                    }
                }

                // Check for ManaFromNonlandBonus (e.g., Kinnan, Bonder Prodigy)
                if source_is_nonland {
                    let bonus = triggers::mana_from_nonland_bonus_count(state, player);
                    if bonus > 0 {
                        state.players[player].mana_pool.colorless += bonus;
                    }
                }

                // Check for ManaFromSwampBonus (e.g., Nirkana Revenant, Crypt Ghast)
                if source_is_swamp {
                    let bonus = triggers::mana_from_swamp_bonus_count(state, player);
                    if bonus > 0 {
                        state.players[player].mana_pool.add_color(Color::Black, bonus);
                    }
                }
            }

            // Tap the permanent
            if let Some(inst) = state.objects.get_mut(&obj_id) {
                inst.tapped = true;
            }
        }

        Action::ActivateAbility {
            object_id,
            ability_index,
            targets,
        } => {
            let obj_id = *object_id;
            let idx = *ability_index;
            let player = state.priority_player;

            // Clone ability info to avoid borrow conflict
            let ability = {
                let db = state.card_db();
                let inst = &state.objects[&obj_id];
                let def = db.get(inst.card_def_id).unwrap();
                def.activated_abilities.get(idx).cloned()
            };

            if let Some(ability) = ability {
                if ability.requires_tap && !state.can_pay_tap_cost(obj_id) {
                    return false;
                }
                if !state.is_commander(obj_id) && matches!(ability.sacrifice_cost, Some(SacrificeCost::SelfSacrifice)) {
                    if let Some(object) = state.exact_object(obj_id) {
                        let nominal = transitions::TransitionRequest { object, from: ZoneType::Battlefield, to: ZoneType::Graveyard,
                            kind: transitions::MovementKind::Sacrifice { player } };
                        let actual = transitions::TransitionRequest { to: if state.is_creature(obj_id) { state.death_replacement_zone(obj_id) } else { ZoneType::Graveyard }, ..nominal };
                        if movement_choice::requires_choice(state, &nominal) || movement_choice::requires_choice(state, &actual) {
                            aura::record_entry_failure(state, transitions::TransitionError::UnsupportedPath); return false;
                        }
                    }
                }
                let self_sacrifice = if state.is_commander(obj_id)
                    && matches!(ability.sacrifice_cost, Some(SacrificeCost::SelfSacrifice)) {
                    let Some(object) = state.exact_object(obj_id) else { return false; };
                    let selected = transitions::ResolvedSacrificeSubject { player, object };
                    if let Err(error) = transitions::preflight_permanent_sacrifice(state, &[selected]) {
                        state.sba_failure = Some(sba::PreparedPassFailure::Transition(error));
                        return false;
                    }
                    Some(selected)
                } else { None };
                // Pay mana cost
                if !mana::pay_cost(state, player, &ability.cost,
                    ability.requires_tap.then_some(obj_id)) {
                    return false;
                }

                // Pay life cost (e.g., fetch lands pay 1 life)
                if ability.life_cost > 0 {
                    state.players[player].life -= ability.life_cost as i32;
                }

                // Tap if required
                if ability.requires_tap {
                    if let Some(inst) = state.objects.get_mut(&obj_id) {
                        inst.tapped = true;
                    }
                }

                // Pay sacrifice cost
                match &ability.sacrifice_cost {
                    Some(SacrificeCost::SelfSacrifice) => {
                        if let Some(selected) = self_sacrifice {
                            if let Err(error) = transitions::sacrifice_permanents_batch(state, &[selected]) {
                                state.sba_failure = Some(sba::PreparedPassFailure::Transition(error));
                            }
                        } else {
                            state.move_object(obj_id, ZoneType::Battlefield, ZoneType::Graveyard);
                        }
                        if state.gameplay_stopped() { return false; }
                    }
                    Some(SacrificeCost::AnyCreature) | Some(SacrificeCost::CreatureWithSubtype(_)) => {
                        // Sacrifice another creature — for now, this is handled
                        // by the combo discovery engine. Full implementation would
                        // require choosing a sacrifice target from legal_actions.
                    }
                    None => {}
                }

                let stack_id = state.new_stack_id();
                state.stack.push(StackEntry {
                    id: stack_id,
                    source: StackSource::ActivatedAbility {
                        source_id: obj_id,
                        ability_index: idx,
                    },
                    controller: player,
                    targets: targets.clone(),
                    target_generations: crate::targeting::target_generations(state, targets),
                });
            }
            state.consecutive_passes = 0;
        }

        Action::DeclareAttackers { attackers } => {
            state.combat.clear();
            state.combat.attackers = attackers.clone();

            // Collect which attackers need tapping (those without vigilance)
            let to_tap: Vec<ObjectId> = attackers
                .iter()
                .filter(|&&id| !state.has_keyword(id, KeywordAbility::Vigilance))
                .copied()
                .collect();
            for id in to_tap {
                if let Some(inst) = state.objects.get_mut(&id) {
                    inst.tapped = true;
                }
            }

            state.consecutive_passes = 0;
            if attackers.is_empty() {
                // No attackers — skip combat entirely
                phases::transition_to_phase(state, Phase::EndOfCombat);
                state.priority_player = state.active_player;
            } else {
                // Exalted: if exactly one creature attacks, each permanent with
                // Exalted gives it +1/+1 until EOT.
                if attackers.len() == 1 {
                    triggers::apply_exalted(state, attackers[0]);
                }

                // Annihilator: for each attacker with Annihilator N, the defending
                // player sacrifices N permanents (simplified: random selection).
                triggers::apply_annihilator(state, attackers);
                if state.gameplay_stopped() { return false; }

                // Batch-check attack triggers for all attackers before flushing
                for &attacker_id in attackers {
                    triggers::check_triggers(state, TriggerCondition::Attacks, Some(attacker_id));
                }
                state.trigger_order_resume = Some(TriggerOrderResume::AfterAttackers);
                if !state.trigger_placement_deferred {
                    state.invalidate_characteristics_cache();
                    sba::check_state_based_actions(state);
                }
                if state.gameplay_stopped() { return true; }
                let flushed = triggers::flush_triggers(state);

                if flushed {
                    state.trigger_order_resume = None;
                    phases::transition_to_phase(state, Phase::DeclareBlockers);
                    state.priority_player = state.next_player(state.active_player);
                }
            }
        }

        Action::DeclareBlockers { blocks } => {
            state.combat.blockers.clear();
            state.combat.attacker_blockers.clear();

            for &(blocker, attacker) in blocks {
                state.combat.blockers.insert(blocker, attacker);
                state
                    .combat
                    .attacker_blockers
                    .entry(attacker)
                    .or_default()
                    .push(blocker);
            }

            // Advance to combat damage (skip first strike if not applicable)
            state.consecutive_passes = 0;
            phases::transition_to_phase(state, Phase::FirstStrikeDamage);
            // Execute the first strike damage step entry (which may skip to CombatDamage)
            phases::execute_phase_entry_public(state);
        }

        Action::OrderDamageAssignment {
            attacker,
            assignment,
        } => {
            state
                .combat
                .damage_assignment
                .insert(*attacker, assignment.clone());
            state.consecutive_passes = 0;
        }

        Action::ChooseNextTrigger { handle } => triggers::choose_next_trigger(state, *handle),
        Action::PlaceChitteringTrigger { handle, opponent } => triggers::place_chittering_trigger(state, *handle, *opponent),
        Action::ChooseChitteringCard { .. } => unreachable!("handled private selection"),
        // Legacy whole-order actions are no longer accepted. Never silently replay
        // an old policy against the incremental action semantics.
        Action::OrderTriggers { .. } | Action::OrderTriggerOccurrences { .. } => return false,

        Action::CastCommander { object_id, targets } => {
            let obj_id = *object_id;
            let player = state.priority_player;
            let db = state.card_db();
            let inst = &state.objects[&obj_id];
            let def = db.get(inst.card_def_id).unwrap().clone();
            let is_creature = def.is_creature();

            let Some(cost) = commander_cast_cost(state, obj_id) else { return false; };
            if !mana::pay_cost(state, player, &cost, None) { return false; }
            let record = state.commanders.iter_mut().find(|c| c.object == obj_id).expect("admitted designation");
            record.casts_by_player[player] += 1;

            // Move to stack
            let stack_id = state.new_stack_id();
            state.stack.push(StackEntry {
                id: stack_id,
                source: StackSource::Spell(obj_id),
                controller: player,
                targets: targets.clone(),
                target_generations: crate::targeting::target_generations(state, targets),
            });
            // Remove from command zone
            state.objects.get_mut(&obj_id).unwrap().zone_change_count += 1;
            state.players[player].command_zone.retain(|&id| id != obj_id);

            state.emit_event(GameEvent::SpellCast {
                object: obj_id,
                controller: player,
            });
            state.emit_event(GameEvent::ZoneChange {
                object: obj_id,
                from: Zone::Command,
                to: Zone::Stack,
            });

            // Fire spell-cast triggers (YouCastSpell, OpponentCastsSpell, etc.)
            triggers::fire_spell_cast_triggers(state, player, is_creature, stack_id);

            state.consecutive_passes = 0;
        }

        Action::Equip { equipment_id, target_id } => {
            let player = state.priority_player;
            if crate::targeting::validate_equip_activation_structure(state, *equipment_id, *target_id).is_err() {
                state.sba_failure = Some(sba::PreparedPassFailure::StateEncoding);
                return false;
            }
            let Some(cost) = crate::targeting::equip_activation_cost(state, *equipment_id, *target_id) else { return false; };
            let source = state.exact_object(*equipment_id).unwrap();
            let target = state.exact_object(*target_id).unwrap();
            let source_card_id = state.objects[equipment_id].card_def_id;
            let target_card_id = state.objects[target_id].card_def_id;
            if !mana::pay_cost(state, player, &cost, None) { return false; }
            let id = state.new_stack_id();
            state.stack.push(StackEntry { id, controller: player,
                source: StackSource::EquipAbility { source, source_card_id, target_card_id },
                targets: vec![crate::game::Target::Object(target.id)],
                target_generations: vec![Some(target.generation)],
            });
            state.consecutive_passes = 0;
        }

        Action::ActivateLoyalty { object_id, ability_index } => {
            let obj_id = *object_id;
            let ab_idx = *ability_index;

            // Read the loyalty ability info
            let (loyalty_cost, _effect, controller) = {
                let db = state.card_db();
                let inst = &state.objects[&obj_id];
                let def = db.get(inst.card_def_id).unwrap();
                let la = &def.loyalty_abilities[ab_idx];
                (la.cost, la.effect.clone(), inst.controller)
            };

            // Adjust loyalty counters
            if loyalty_cost >= 0 {
                // +N: add counters
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    inst.loyalty_counters += loyalty_cost as u32;
                    inst.loyalty_activated_this_turn = true;
                }
            } else {
                // -N: remove counters
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    inst.loyalty_counters = inst.loyalty_counters.saturating_sub((-loyalty_cost) as u32);
                    inst.loyalty_activated_this_turn = true;
                }
            }

            // Put the ability on the stack
            let stack_id = state.new_stack_id();
            state.stack.push(crate::game::StackEntry {
                id: stack_id,
                source: crate::game::StackSource::ActivatedAbility {
                    source_id: obj_id,
                    ability_index: ab_idx,
                },
                controller,
                targets: Vec::new(),
                target_generations: Vec::new(),
            });
            state.consecutive_passes = 0;
        }

        Action::CastFromGraveyard { object_id, targets } => {
            let obj_id = *object_id;
            let player = state.priority_player;
            let (def, is_flashback, escape_exile_count) = {
                let db = state.card_db();
                let inst = &state.objects[&obj_id];
                let def = db.get(inst.card_def_id).unwrap().clone();
                let is_flashback = def.flashback_cost.is_some();
                let escape_count = def.escape_exile_count;
                (def, is_flashback, escape_count)
            };
            let is_creature = def.is_creature();

            // Pay the appropriate cost
            if is_flashback {
                if let Some(ref fb_cost) = def.flashback_cost {
                    let reduction = mana::total_cost_reduction(state, player, is_creature);
                    let reduced_cost = mana::apply_cost_reduction(fb_cost, reduction);
                    if !mana::pay_cost(state, player, &reduced_cost, None) {
                        return false;
                    }
                }
            } else if let Some(exile_count) = escape_exile_count {
                if state.players[player].graveyard.iter().filter(|&&id| id != obj_id && state.is_card(id)).count() < exile_count as usize {
                    return false;
                }
                // Escape: pay regular mana cost + exile N cards from graveyard
                if let Some(ref cost) = def.mana_cost {
                    let reduction = mana::total_cost_reduction(state, player, is_creature);
                    let reduced_cost = mana::apply_cost_reduction(cost, reduction);
                    if !mana::pay_cost(state, player, &reduced_cost, None) {
                        return false;
                    }
                }
                // Exile N other cards from graveyard as additional cost
                let mut exiled = 0u32;
                let gy: Vec<ObjectId> = state.players[player].graveyard.clone();
                for &gy_id in &gy {
                    if exiled >= exile_count {
                        break;
                    }
                    if gy_id != obj_id && state.is_card(gy_id) {
                        state.move_object(gy_id, ZoneType::Graveyard, ZoneType::Exile);
                        exiled += 1;
                    }
                }
            }

            // Move to stack from graveyard
            let stack_id = state.new_stack_id();
            state.stack.push(StackEntry {
                id: stack_id,
                source: StackSource::Spell(obj_id),
                controller: player,
                targets: targets.clone(),
                target_generations: crate::targeting::target_generations(state, targets),
            });
            state.objects.get_mut(&obj_id).unwrap().zone_change_count += 1;
            state.players[player].graveyard.retain(|&id| id != obj_id);

            state.emit_event(GameEvent::SpellCast {
                object: obj_id,
                controller: player,
            });
            state.emit_event(GameEvent::ZoneChange {
                object: obj_id,
                from: Zone::Graveyard,
                to: Zone::Stack,
            });

            triggers::fire_spell_cast_triggers(state, player, is_creature, stack_id);
            state.consecutive_passes = 0;
        }

        Action::ChooseReplacementOrder { ordering } => {
            let _ = ordering;
            state.consecutive_passes = 0;
        }

        Action::MulliganKeep => {
            let player = state.priority_player;
            state.players[player].mulligan_decided = true;
            setup::advance_mulligan(state);
        }

        Action::MulliganMulligan => {
            let player = state.priority_player;
            debug_assert!(
                state.players[player].mulligan_count < crate::action::MAX_MULLIGANS,
                "MulliganMulligan applied but player {} already at max mulligans ({})",
                player,
                state.players[player].mulligan_count,
            );
            // Shuffle hand back into library
            let hand: Vec<crate::card::ObjectId> = state.players[player].hand.drain(..).collect();
            for obj_id in hand {
                state.players[player].library.push(obj_id);
            }
            {
                use rand::seq::SliceRandom;
                let mut rng = rand::thread_rng();
                state.players[player].library.shuffle(&mut rng);
            }
            // Draw 7 new cards
            draw_cards(state, player, 7);
            state.players[player].mulligan_count += 1;
            // Stay on the same player for another keep/mulligan decision
        }

        Action::MulliganBottomCard { object_id } => {
            let player = state.priority_player;
            // Move the card from hand to bottom of library
            if let Some(pos) = state.players[player].hand.iter().position(|&id| id == *object_id) {
                state.players[player].hand.remove(pos);
                state.players[player].library.push(*object_id);
            }
            setup::advance_mulligan(state);
        }

        Action::ChooseTutorTarget { card_id } => {
            if let Some(pending) = state.pending_tutor.take() {
                let player = pending.controller;
                let destination = pending.destination;
                // Find the first instance of this card in the library
                if let Some(pos) = state.players[player]
                    .library
                    .iter()
                    .position(|&obj_id| state.is_card(obj_id) && state.objects[&obj_id].card_def_id == *card_id)
                {
                    let obj_id = state.players[player].library[pos];
                    state.move_object(obj_id, ZoneType::Library, destination);
                }
            }
        }

        Action::Concede => {
            loss::adjudicate(state, Some(state.priority_player));
        }

        Action::ActivateMacro { combo_id } => {
            let player = state.priority_player;
            // Look up the combo from the registry and apply its effect.
            let combo = state
                .combo_registry
                .as_ref()
                .and_then(|reg| reg.get(*combo_id).cloned());
            if let Some(combo) = combo {
                crate::combo::apply_combo_effect(state, player, &combo);
                state.consecutive_passes = 0;
            }
        }

        Action::EndTurn => {
            fast_forward_end_of_turn(state);
        }

    }
    true
}

/// Check if a player controls a permanent with a given static ability.
fn has_static_ability_on_battlefield(
    state: &GameState,
    player: PlayerIndex,
    target_ability: &crate::layers::StaticAbility,
) -> bool {
    let db = state.card_db();
    state.battlefield.iter().any(|&obj_id| {
        state.objects.get(&obj_id).map_or(false, |inst| {
            if inst.controller != player {
                return false;
            }
            db.get(inst.card_def_id).map_or(false, |def| {
                def.static_abilities.iter().any(|sa| {
                    std::mem::discriminant(sa) == std::mem::discriminant(target_ability)
                })
            })
        })
    })
}

/// Draw cards for a player, applying draw replacement effects.
///
/// Replacement effect priority (only one applies per would-draw):
/// 1. Renfield/Eruth: exile top 2 instead of drawing (simplified as put 2 in hand)
/// 2. Abundance: reveal until land, put in hand, rest on bottom
/// 3. Phial of Galadriel: draw 2 instead of 1 when hand is empty
/// If none apply, normal draw.
pub fn draw_cards(state: &mut GameState, player: PlayerIndex, count: usize) {
    if state.gameplay_stopped() || state.pending_copy_order.is_some() || state.movement_or_hand_pending() || state.pending_commander_sba.is_some() { return; }
    if player >= state.players.len() { return; }
    if count != 0 && state.replacement_effects.iter().any(|e| matches!(e.applies_to, crate::replacement::ReplacementEventKind::WouldMoveTo { .. })) {
        aura::record_entry_failure(state, transitions::TransitionError::UnsupportedPath); return;
    }
    // Check for draw replacement effects on the battlefield
    let has_renfield = has_static_ability_on_battlefield(
        state, player, &crate::layers::StaticAbility::RenfieldDrawReplacement,
    );
    let has_abundance = has_static_ability_on_battlefield(
        state, player, &crate::layers::StaticAbility::AbundanceReplacement,
    );
    let has_phial = has_static_ability_on_battlefield(
        state, player, &crate::layers::StaticAbility::PhialDrawDoubler,
    );

    // These represented replacements may expand or select beyond the nominal
    // prefix. Slice A rejects their complete Commander interaction before drawing.
    let other_draw_replacement = state.replacement_effects.iter().any(|effect| effect.applies_to == crate::replacement::ReplacementEventKind::CardDraw);
    if count != 0 && (has_renfield || has_abundance || has_phial || other_draw_replacement)
        && state.players[player].library.iter().any(|id| state.is_commander(*id)) {
        aura::record_entry_failure(state, transitions::TransitionError::UnsupportedPath); return;
    }
    if state.players[player].library.iter().filter(|id| state.is_card(**id))
        .take(count).any(|id| state.is_commander(*id)) {
        if state.trigger_placement_deferred && state.trigger_order_resume == Some(TriggerOrderResume::AfterResolution) {
            aura::record_entry_failure(state, transitions::TransitionError::UnsupportedPath); return;
        }
        let Ok(count) = u32::try_from(count) else {
            aura::record_entry_failure(state, transitions::TransitionError::CapacityExhausted); return;
        };
        movement_choice::begin_direct_draw(state, player, count, false);
        return;
    }

    for _ in 0..count {
        if state.first_library_card(player).is_none() && !has_renfield && !has_abundance {
            // CR 704: complete the enclosing effect before this condition
            // becomes a loss at settlement. Repeated attempts share one fact.
            if !state.loss_boundary.pending_failed_draws.contains(&player) {
                state.loss_boundary.pending_failed_draws.push(player);
                state.loss_boundary.pending_failed_draws.sort_unstable();
            }
            return;
        }

        let hand_was_empty = state.card_count(&state.players[player].hand) == 0;

        // Determine how many actual cards to put in hand for this single draw.
        // Phial doubles the draw (draw 2 instead of 1) as a true replacement
        // when hand was empty. It does NOT stack with other replacements --
        // only one replacement effect applies per would-draw event.
        if has_renfield {
            // Renfield replacement: exile top 2 to hand instead of drawing 1.
            // Simplified: put 2 cards in hand (in practice they'd be exiled and
            // playable this turn). This is a replacement, so no CardDrawn event.
            for _ in 0..2 {
                if state.first_library_card(player).is_none() {
                    break;
                }
                let index = state.first_library_card(player).expect("drawable card checked");
                let card_id = state.players[player].library.remove(index);
                state.players[player].hand.push(card_id);
                state.emit_event(GameEvent::ZoneChange {
                    object: card_id,
                    from: Zone::Library,
                    to: Zone::Hand,
                });
            }
        } else if has_abundance {
            // Abundance replacement: reveal cards from the top until you find a land,
            // put it in hand, then put the revealed non-land cards on the bottom
            // in any order. This is a draw replacement, so no CardDrawn event.
            let land_idx = {
                let db = state.card_db();
                state.players[player].library.iter().position(|&id| {
                    state.objects.get(&id).filter(|inst| !inst.is_token)
                        .and_then(|inst| db.get(inst.card_def_id))
                        .map_or(false, |def| def.is_land())
                })
            };
            if let Some(idx) = land_idx {
                let card_id = state.players[player].library[idx];
                let revealed: Vec<ObjectId> = state.players[player].library[..idx].iter()
                    .copied().filter(|&id| state.is_card(id)).collect();
                let removed: std::collections::HashSet<_> = revealed.iter().copied()
                    .chain(std::iter::once(card_id)).collect();
                state.players[player].library.retain(|id| !removed.contains(id));
                state.players[player].hand.push(card_id);
                state.players[player].library.extend(revealed);
                state.emit_event(GameEvent::ZoneChange {
                    object: card_id,
                    from: Zone::Library,
                    to: Zone::Hand,
                });
            } else {
                // No lands left: reveal entire library, put all on bottom (no card drawn).
                // Abundance still replaces the draw even if nothing is found.
            }
        } else {
            // Normal draw, possibly doubled by Phial
            let draws = if has_phial && hand_was_empty { 2 } else { 1 };
            for _ in 0..draws {
                if state.first_library_card(player).is_none() {
                    if !state.loss_boundary.pending_failed_draws.contains(&player) {
                        state.loss_boundary.pending_failed_draws.push(player);
                        state.loss_boundary.pending_failed_draws.sort_unstable();
                    }
                    break;
                }
                let index = state.first_library_card(player).expect("drawable card checked");
                let card_id = state.players[player].library.remove(index);
                state.players[player].hand.push(card_id);
                state.emit_event(GameEvent::CardDrawn {
                    player,
                    object: card_id,
                });
                state.emit_event(GameEvent::ZoneChange {
                    object: card_id,
                    from: Zone::Library,
                    to: Zone::Hand,
                });
            }
        }

        // Fire OpponentDrawsCard triggers (e.g. Consecrated Sphinx)
        triggers::fire_card_draw_triggers(state, player);
    }
}

/// Finish resolution only after state-based actions and all queued triggers
/// have had a chance to establish their mandatory chooser.
pub(super) fn complete_stack_resolution(state: &mut GameState) {
    if state.gameplay_stopped() { return; }
    state.trigger_order_resume = Some(TriggerOrderResume::AfterResolution);
    // Copies and abilities need not move a physical spell to invalidate the
    // cache; their completed effects can still change dynamic characteristics.
    state.invalidate_characteristics_cache();
    sba::check_state_based_actions(state);
    state.trigger_placement_deferred = false;
    if state.gameplay_stopped() || state.pending_commander_sba.is_some() { return; }
    if !state.pending_triggers.is_empty() && !triggers::flush_triggers(state) {
        return;
    }
    state.trigger_order_resume = None;
    restore_priority_after_resolution(state);
}

fn restore_priority_after_resolution(state: &mut GameState) {
    if state.gameplay_stopped() || state.pending_commander_sba.is_some() { return; }
    clear_lost_tutor_choice(state);
    if let Some(tutor) = &state.pending_tutor {
        state.priority_player = tutor.controller;
    } else if state.pending_copy_order.is_none() && !state.movement_or_hand_pending() && state.pending_triggers.is_empty() {
        state.priority_player = if state.players[state.active_player].has_lost {
            state.next_player(state.active_player)
        } else { state.active_player };
    }
}

/// A completed settlement cannot offer a controller-owned choice to a player
/// who has lost. Retire the choice without selecting or moving a library card.
fn clear_lost_tutor_choice(state: &mut GameState) {
    if state.pending_tutor.as_ref().is_some_and(|tutor| state.players[tutor.controller].has_lost) {
        state.pending_tutor = None;
    }
}

/// Discard random cards from a player's hand.
fn discard_random(state: &mut GameState, player: PlayerIndex, count: usize) {
    let mut rng = rand::thread_rng();
    for _ in 0..count {
        let cards: Vec<_> = state.players[player].hand.iter().copied()
            .filter(|&id| state.is_card(id)).collect();
        if cards.is_empty() { break; }
        let obj_id = cards[rng.gen_range(0..cards.len())];
        apply_action(state, &Action::Discard { object_id: obj_id });
    }
}

/// Advance an automated player's mandatory trigger placement using the same
/// APNAP and ordering actions as normal play. Returns false if the state has
/// no legal ordering action and must be handed back to the caller.
fn auto_place_pending_triggers(state: &mut GameState) -> bool {
    if triggers::flush_triggers(state) {
        return true;
    }
    let order = crate::action::legal_actions(state)
        .into_iter()
        .find(|action| matches!(action,
            Action::PlaceChitteringTrigger { .. } | Action::ChooseNextTrigger { .. } | Action::OrderTriggers { .. } | Action::OrderTriggerOccurrences { .. }));
    if let Some(order) = order {
        apply_action(state, &order);
        true
    } else {
        false
    }
}

/// Fast-forward the active player's turn from the current phase to completion.
///
/// Called when a player chooses `Action::EndTurn`. Advances through all
/// remaining phases without calling `legal_actions()`, executing phase
/// entries (triggers, combat damage, SBA) along the way. The stack is
/// auto-resolved and cleanup discard is handled randomly.
///
/// This collapses O(remaining_phases) priority passes into a single action,
/// cutting search depth when the optimal play is "do nothing more this turn."
fn fast_forward_end_of_turn(state: &mut GameState) {
    let turn_player = state.active_player;
    let initial_turn = state.turn_number;
    let mut safety = 0u32;
    const MAX_SAFETY: u32 = 200;

    while state.active_player == turn_player
        && state.turn_number == initial_turn
        && !state.gameplay_stopped()
        && safety < MAX_SAFETY
    {
        safety += 1;

        // EndTurn can be chosen by a human. A mandatory ordering choice ends
        // this shortcut so the normal action loop can present it.
        if state.pending_copy_order.is_some() || state.movement_or_hand_pending() || state.pending_commander_sba.is_some() { break; }

        // Finish the current cleanup discard sequence before its queued
        // triggers can enter a placement window.
        if state.phase == Phase::Cleanup && !state.cleanup_needs_repeat {
            let before = state.players[turn_player].hand.len();
            if before > 7 {
                discard_random(state, turn_player, before - 7);
                if state.players[turn_player].hand.len() == before { break; }
                continue;
            }
        }

        // EndTurn stops for a mandatory order choice by the player.
        if !state.pending_triggers.is_empty() {
            if !triggers::flush_triggers(state) {
                break;
            }
            continue;
        }

        // Auto fail-to-find any pending tutor
        if state.pending_tutor.is_some() {
            state.pending_tutor = None;
            continue;
        }

        // Resolve stack items
        if !state.stack.is_empty() {
            resolution::resolve_top_of_stack(state);
            sba::check_state_based_actions(state);
            continue;
        }

        // Handle phases with mandatory actions
        match state.phase {
            Phase::DeclareAttackers => {
                // Skip combat — declare no attackers
                state.combat.clear();
                state.consecutive_passes = 0;
                phases::advance_phase(state);
            }
            Phase::DeclareBlockers => {
                state.consecutive_passes = 0;
                phases::advance_phase(state);
            }
            Phase::Cleanup => {
                if state.cleanup_needs_repeat {
                    state.consecutive_passes = state.players.len() as u32;
                    phases::handle_priority_pass(state);
                } else {
                    phases::finalize_cleanup(state);
                }
            }
            _ => {
                // Normal phase — pass priority to advance
                state.consecutive_passes += 1;
                phases::handle_priority_pass(state);
            }
        }
    }
}

/// Fast-forward through the goldfish player's entire turn.
///
/// In goldfish mode, the opponent (player 1) never casts spells, attacks,
/// or blocks. This function advances through all phases of their turn
/// without calling `legal_actions()` — a significant performance win since
/// `legal_actions()` is the most expensive function in the game loop.
///
/// Phase entries (untap, draw, upkeep/end-step triggers) are still executed
/// so the pilot's cards that trigger during the opponent's turn work correctly.
/// If triggers put items on the stack, they are auto-resolved (both players
/// pass priority). If the goldfish needs to discard in cleanup, random cards
/// are discarded.
///
/// Returns the number of internal actions taken (for action-count tracking).
pub fn fast_forward_goldfish_turn(state: &mut GameState) -> u32 {
    fast_forward_goldfish_turn_counted(state, None)
}

/// TUI path: pause when the human must order copies during an opponent turn.
pub fn fast_forward_goldfish_turn_until_copy_choice(state: &mut GameState, human: PlayerIndex) -> u32 {
    fast_forward_goldfish_turn_counted(state, Some(human))
}

fn fast_forward_goldfish_turn_counted(state: &mut GameState, pause_for: Option<PlayerIndex>) -> u32 {
    if state.gameplay_stopped() { return 0; }
    if state.validate_trigger_placement().is_err() {
        state.sba_failure = Some(sba::PreparedPassFailure::StateEncoding); return 0;
    }
    if state.validate_commanders().is_err() || state.validate_commander_choices().is_err() || state.validate_movement_play().is_err() || chittering::validate_play(state).is_err() {
        state.sba_failure = Some(sba::PreparedPassFailure::StateEncoding); return 0;
    }
    if state.loss_action_in_progress { return fast_forward_goldfish_turn_inner(state, pause_for); }
    let active_before = state.active_player;
    let had_terminal = state.loss_boundary.terminal.is_some();
    let had_unsupported = state.loss_boundary.unsupported.is_some();
    state.loss_action_in_progress = true;
    let actions = fast_forward_goldfish_turn_inner(state, pause_for);
    state.loss_action_in_progress = false;
    state.loss_boundary.accepted_actions += u64::from(actions);
    if actions > 0 && state.loss_boundary.turns_taken.iter().all(|turns| *turns == 0) {
        state.loss_boundary.turns_taken.resize(state.players.len(), 0);
        state.loss_boundary.turns_taken[active_before] = 1;
    }
    // A terminal/failure seam ends the shortcut on its final counted action.
    // Retain that action's phase/seat/turn provenance, correcting only the
    // accepted index accumulated by this existing shortcut's action budget.
    if !had_terminal {
        if let Some(result) = &mut state.loss_boundary.terminal {
            result.coordinates.action_index = state.loss_boundary.accepted_actions;
        }
    }
    if !had_unsupported {
        if let Some(result) = &mut state.loss_boundary.unsupported {
            result.coordinates.action_index = state.loss_boundary.accepted_actions;
        }
    }
    actions
}

fn fast_forward_goldfish_turn_inner(state: &mut GameState, pause_for: Option<PlayerIndex>) -> u32 {
    let goldfish_player = state.active_player;
    let mut actions = 0u32;
    let mut safety = 0u32;
    const MAX_SAFETY: u32 = 200;

    while state.active_player == goldfish_player && !state.gameplay_stopped() && safety < MAX_SAFETY {
        safety += 1;

        if state.pending_chittering.is_some() {
            if pause_for == Some(state.priority_player) { break; }
            let Some(action) = chittering::selection_actions(state).into_iter().next() else { break; };
            if !chittering::choose_card(state, &action) { break; }
            actions += 1; continue;
        }
        if state.pending_movement.is_some() {
            if pause_for == Some(state.priority_player) { break; }
            let Some(action) = movement_choice::actions(state).into_iter().next() else { break; };
            if !movement_choice::choose(state, &action) { break; }
            actions += 1; continue;
        }
        if let Some(operation) = &state.pending_commander_sba {
            if pause_for == operation.chooser() { break; }
            let Some(designation) = operation.offers.iter().zip(&operation.answers)
                .find_map(|(offer, answer)| (offer.designation.owner == state.priority_player
                    && answer.is_none()).then_some(offer.designation)) else { break; };
            apply_action(state, &Action::ChooseCommanderSba { designation, to_command: true });
            actions += 1;
            continue;
        }

        if let Some(pending) = &state.pending_copy_order {
            if pause_for == Some(pending.controller()) { break; }
            let Some(index) = (0..pending.items().len())
                .find(|index| !pending.selected_order().contains(index)) else { break; };
            let before = pending.selected_order().len();
            apply_action(state, &Action::ChooseNextCopy { item_index: index });
            if state.pending_copy_order.as_ref().is_some_and(|pending| pending.selected_order().len() == before) {
                break;
            }
            actions += 1;
            continue;
        }

        // Pending cleanup-discard triggers are detected but cannot be placed
        // until every required discard in this step has happened.
        if state.phase == Phase::Cleanup && !state.cleanup_needs_repeat {
            let before = state.players[goldfish_player].hand.len();
            if before > 7 {
                discard_random(state, goldfish_player, before - 7);
                if state.players[goldfish_player].hand.len() == before { break; }
                actions += 1;
                continue;
            }
        }

        // Use the ordinary mandatory ordering path, including APNAP.
        if !state.pending_triggers.is_empty() {
            if !triggers::flush_triggers(state) && pause_for == Some(state.priority_player) {
                break;
            }
            if !auto_place_pending_triggers(state) {
                break;
            }
            actions += 1;
            continue;
        }

        // Handle pending tutor — auto fail-to-find (goldfish doesn't search)
        if state.pending_tutor.is_some() {
            state.pending_tutor = None;
            actions += 1;
            continue;
        }

        // Resolve stack items (both players auto-pass)
        if !state.stack.is_empty() {
            resolution::resolve_top_of_stack(state);
            sba::check_state_based_actions(state);
            actions += 1;
            continue;
        }

        // Handle phases that need mandatory actions
        match state.phase {
            Phase::DeclareAttackers if state.priority_player == goldfish_player => {
                // Goldfish never attacks — declare empty attackers and advance
                state.combat.clear();
                state.consecutive_passes = 0;
                phases::advance_phase(state);
                actions += 1;
            }
            Phase::DeclareBlockers if state.priority_player != goldfish_player => {
                // Goldfish as defender never blocks — this shouldn't happen in
                // goldfish mode (pilot is attacking), but handle it gracefully
                state.consecutive_passes = 0;
                phases::advance_phase(state);
                actions += 1;
            }
            Phase::Cleanup => {
                if state.cleanup_needs_repeat {
                    state.consecutive_passes = state.players.len() as u32;
                    phases::handle_priority_pass(state);
                } else {
                    phases::finalize_cleanup(state);
                }
                actions += 1;
            }
            _ => {
                // Normal phase — just pass priority to advance
                state.consecutive_passes += 1;
                phases::handle_priority_pass(state);
                actions += 1;
            }
        }
    }

    actions
}
