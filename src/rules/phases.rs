use crate::card::TriggerCondition;
use crate::events::GameEvent;
use crate::game::{GameState, Phase, PlayerIndex};
use crate::layers::StaticAbility;

/// Handle when priority is passed (may resolve stack or advance phase).
pub(super) fn handle_priority_pass(state: &mut GameState) {
    if state.pending_copy_order.is_some() { return; }
    let num_players = state.players.len() as u32;

    if state.consecutive_passes >= num_players {
        // All players passed in succession
        state.consecutive_passes = 0;

        if !state.stack.is_empty() {
            // Resolve top of stack
            super::resolution::resolve_top_of_stack(state);
        } else {
            // Advance to next phase
            advance_phase(state);
        }
    } else {
        // Pass priority to the next player in clockwise order
        state.priority_player = state.next_player(state.priority_player);
    }
}

/// Each Phase variant represents a step or phase boundary. Mana empties when
/// crossing it (CR 106.4), never merely when passing priority or resolving.
pub(super) fn transition_to_phase(state: &mut GameState, next: Phase) {
    if state.pending_copy_order.is_some() { return; }
    if state.phase != next {
        for player in &mut state.players {
            player.mana_pool.drain();
        }
        state.phase = next;
    }
}

/// Advance to the next phase.
pub(super) fn advance_phase(state: &mut GameState) {
    if state.pending_copy_order.is_some() { return; }
    let current_idx = Phase::TURN_ORDER
        .iter()
        .position(|&p| p == state.phase)
        .unwrap_or(0);

    // Find the next non-skipped phase
    let mut next_idx = current_idx + 1;
    while next_idx < Phase::TURN_ORDER.len() {
        let candidate = Phase::TURN_ORDER[next_idx];
        if state.skip_phases.contains(&candidate) {
            next_idx += 1;
            continue;
        }
        break;
    }

    if next_idx < Phase::TURN_ORDER.len() {
        transition_to_phase(state, Phase::TURN_ORDER[next_idx]);
    } else {
        // End of turn — go to next turn
        next_turn(state);
        return;
    }

    // Execute phase entry actions
    execute_phase_entry(state);
}

/// Public wrapper for phase entry execution.
pub(super) fn execute_phase_entry_public(state: &mut GameState) {
    execute_phase_entry(state);
}

/// Execute actions when entering a new phase.
pub(super) fn execute_phase_entry(state: &mut GameState) {
    if state.pending_copy_order.is_some() { return; }
    let active = state.active_player;

    match state.phase {
        Phase::Mulligan => {
            // Mulligan phase is handled by apply_action (MulliganKeep/Mulligan/BottomCard).
            // Priority is set by advance_mulligan.
            state.priority_player = active;
        }

        Phase::Untap => {
            // Untap all permanents controlled by active player
            let permanents = state.permanents_controlled_by(active);
            for obj_id in permanents {
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    inst.tapped = false;
                    inst.summoning_sick = false;
                }
            }
            // Reset land plays (base 1 + extra from static abilities)
            let extra = count_extra_land_drops(state, active);
            state.players[active].land_plays_remaining = 1 + extra;
            // Drain mana pools
            for p in &mut state.players {
                p.mana_pool.drain();
            }
            // No priority in untap step — advance immediately
            advance_phase(state);
        }

        Phase::Draw => {
            // Active player draws a card (skip on turn 1 for first player in standard rules)
            if !(state.turn_number == 1 && active == 0) {
                super::draw_cards(state, active, 1);
            }
            // Priority is given after draw
            state.priority_player = active;
        }

        Phase::Upkeep => {
            // Fire beginning-of-upkeep triggers.
            state.priority_player = active;
            let flushed = super::triggers::fire_triggers(state, TriggerCondition::BeginningOfUpkeep, None);
            if flushed {
                state.priority_player = active;
            }
        }

        Phase::PreCombatMain | Phase::PostCombatMain => {
            state.priority_player = active;
        }

        Phase::BeginningOfCombat => {
            state.priority_player = active;
            state.combat.clear();
            // Fire "at the beginning of combat on your turn" triggers
            let flushed = super::triggers::fire_triggers(state, TriggerCondition::BeginningOfCombat, None);
            if flushed {
                state.priority_player = active;
            }
        }

        Phase::DeclareAttackers => {
            state.priority_player = active;
        }

        Phase::DeclareBlockers => {
            // Defending player gets priority to declare blockers
            state.priority_player = state.next_player(active);
        }

        Phase::FirstStrikeDamage => {
            // Check if any attacker or blocker has first strike / double strike
            let has_first_strike = super::combat::has_first_strike_creatures(state);
            if has_first_strike {
                super::combat::resolve_combat_damage(state, true);
                state.trigger_order_resume = Some(crate::game::TriggerOrderResume::AfterResolution);
                super::sba::check_state_based_actions(state);
                if state.pending_triggers.is_empty() {
                    state.trigger_order_resume = None;
                    state.priority_player = active;
                }
            } else {
                // Skip first strike damage step
                advance_phase(state);
            }
        }

        Phase::CombatDamage => {
            super::combat::resolve_combat_damage(state, false);
            state.trigger_order_resume = Some(crate::game::TriggerOrderResume::AfterResolution);
            super::sba::check_state_based_actions(state);
            if state.pending_triggers.is_empty() {
                state.trigger_order_resume = None;
                state.priority_player = active;
            }
        }

        Phase::EndOfCombat => {
            state.combat.clear();
            state.priority_player = active;
        }

        Phase::EndStep => {
            // Fire end-of-turn triggers.
            state.priority_player = active;
            let flushed = super::triggers::fire_triggers(state, TriggerCondition::EndOfTurn, None);
            if flushed {
                state.priority_player = active;
            }
        }

        Phase::Cleanup => {
            // Discard down to max hand size (7)
            let hand_size = state.players[active].hand.len();
            if hand_size > 7 {
                state.priority_player = active;
                state.consecutive_passes = 0;
                return;
            }
            finalize_cleanup(state);
        }
    }
}

pub(super) fn finalize_cleanup(state: &mut GameState) {
    if state.pending_copy_order.is_some() { return; }
    // Remove end-of-turn continuous effects (layer engine)
    state.cleanup_eot_effects();
    // Remove legacy EoT effects on instances
    for &obj_id in &state.battlefield.clone() {
        if let Some(inst) = state.objects.get_mut(&obj_id) {
            inst.cleanup_eot();
        }
    }
    // Advance to next turn (no priority in cleanup normally)
    advance_phase(state);
}

/// Move to the next turn.
fn next_turn(state: &mut GameState) {
    // Clear skip_phases from the ending turn
    state.skip_phases.clear();

    // Check for extra turns (Phase 3A)
    if let Some(extra_turn_player) = state.extra_turns.pop_front() {
        state.active_player = extra_turn_player;
    } else {
        state.active_player = state.next_player(state.active_player);
    }

    state.priority_player = state.active_player;
    state.turn_number += 1;
    transition_to_phase(state, Phase::TURN_ORDER[0]); // Untap
    state.consecutive_passes = 0;
    state.spells_cast_this_turn = 0;

    state.emit_event(GameEvent::TurnStarted {
        active_player: state.active_player,
        turn_number: state.turn_number,
    });

    // Drain mana pools
    for p in &mut state.players {
        p.mana_pool.drain();
    }

    execute_phase_entry(state);
}

/// Count extra land drops granted by permanents on the battlefield.
fn count_extra_land_drops(state: &GameState, player: PlayerIndex) -> u32 {
    let db = state.card_db();
    let mut extra = 0u32;
    for &obj_id in &state.battlefield {
        let inst = &state.objects[&obj_id];
        if inst.controller != player {
            continue;
        }
        if let Some(def) = db.get(inst.card_def_id) {
            for sa in &def.static_abilities {
                if let StaticAbility::ExtraLandDrops { count } = sa {
                    extra += count;
                }
            }
        }
    }
    extra
}
