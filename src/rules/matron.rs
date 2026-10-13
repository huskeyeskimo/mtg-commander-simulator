//! Bounded Goblin Matron private search and handoff to movement replacement.

use rand::seq::SliceRandom;

use crate::{
    action::Action,
    card::{Effect, ExactObjectRef, sample::ids::GOBLIN_MATRON},
    events::GameEvent,
    game::{BoundedMovementInstruction, GameState, PendingMatronSearch, PendingMovement,
        StackEntry, StackSource, TriggerOrderResume},
};

fn entry_contract(state: &GameState, entry: &StackEntry) -> Result<ExactObjectRef, &'static str> {
    let StackSource::TriggeredAbility { source_id, ability_index, context } = &entry.source else {
        return Err("missing Goblin Matron trigger");
    };
    if *ability_index != 0 || context.source_card_id != GOBLIN_MATRON
        || context.effect != Effect::GoblinMatronSearch || context.cast_spell.is_some()
        || context.zone_transition.is_some() || entry.controller >= state.players.len()
        || !entry.targets.is_empty() || !entry.target_generations.is_empty()
        || *source_id >= state.next_object_id
    {
        return Err("invalid Goblin Matron captured contract");
    }
    let instance = state.objects.get(source_id).ok_or("missing Goblin Matron source")?;
    if instance.object_id != *source_id || instance.card_def_id != GOBLIN_MATRON
        || instance.zone_change_count < context.source_generation
    {
        return Err("invalid Goblin Matron source incarnation");
    }
    Ok(ExactObjectRef { id: *source_id, generation: context.source_generation })
}

/// Validate the trigger entry owned by either private-search stage and return
/// the exact source only while the originally captured incarnation survives.
pub(crate) fn validate_resolving_entry(
    state: &GameState,
    entry: &StackEntry,
    controller: usize,
) -> Result<Option<ExactObjectRef>, &'static str> {
    let captured = entry_contract(state, entry)?;
    if entry.controller != controller || entry.id >= state.next_stack_id
        || state.stack.iter().any(|other| other.id == entry.id)
        || !matches!(state.trigger_order_resume, Some(TriggerOrderResume::AfterResolution))
        || !state.trigger_placement_deferred
    {
        return Err("unowned Goblin Matron resolving entry");
    }
    Ok((state.exact_object(captured.id) == Some(captured)).then_some(captured))
}

pub(crate) fn preflight_resolution(
    state: &GameState,
    entry: &StackEntry,
) -> Result<(), super::transitions::TransitionError> {
    entry_contract(state, entry).map_err(|_| super::transitions::TransitionError::UnsupportedPath)?;
    if entry.id >= state.next_stack_id || state.stack.last().map(|top| top.id) != Some(entry.id)
        || state.stack.iter().filter(|other| other.id == entry.id).count() != 1
        || state.pending_matron_search.is_some() || state.pending_tutor.is_some()
        || state.pending_movement.is_some() || state.pending_chittering.is_some()
        || state.pending_copy_order.is_some() || state.pending_commander_sba.is_some()
        || state.trigger_placement.is_some()
    {
        return Err(super::transitions::TransitionError::UnsupportedPath);
    }
    state.next_movement_window.checked_add(1)
        .ok_or(super::transitions::TransitionError::CapacityExhausted)?;
    Ok(())
}

pub(crate) fn validate_child(state: &GameState, op: &PendingMovement) -> Result<(), &'static str> {
    if !matches!(op.instruction, BoundedMovementInstruction::GoblinMatronSearch)
        || !op.resolution_owned
    {
        return Err("unowned Goblin Matron movement");
    }
    let entry = op.resolving_entry.as_deref().ok_or("missing Goblin Matron movement entry")?;
    let retained = validate_resolving_entry(state, entry, op.instruction_controller)?;
    if op.source != retained.map(|source| source.id) || op.resolving_source != retained {
        return Err("invalid retained Goblin Matron incarnation");
    }
    Ok(())
}

pub(crate) fn begin(state: &mut GameState, controller: usize) {
    if controller >= state.players.len() || state.pending_matron_search.is_some()
        || state.pending_tutor.is_some() || state.pending_movement.is_some()
    {
        super::aura::record_entry_failure(state, super::transitions::TransitionError::UnsupportedPath);
        return;
    }
    let Some(next_window) = state.next_movement_window.checked_add(1) else {
        super::aura::record_entry_failure(state, super::transitions::TransitionError::CapacityExhausted);
        return;
    };
    state.pending_matron_search = Some(PendingMatronSearch {
        version: 1,
        window: state.next_movement_window,
        active_player: state.active_player,
        controller,
        resume_priority: state.priority_player,
        resolving_entry: None,
    });
    state.next_movement_window = next_window;
    state.priority_player = controller;
}

pub(crate) fn attach_entry(state: &mut GameState, entry: StackEntry) -> bool {
    let Some(op) = state.pending_matron_search.as_ref() else { return false; };
    if op.resolving_entry.is_some()
        || validate_resolving_entry(state, &entry, op.controller).is_err()
    {
        return false;
    }
    let op = state.pending_matron_search.as_mut().unwrap();
    op.resolving_entry = Some(Box::new(entry));
    true
}

fn eligible(state: &GameState, id: u64) -> bool {
    let Some(op) = &state.pending_matron_search else { return false; };
    state.players.get(op.controller).is_some_and(|player| player.library.contains(&id))
        && state.is_card(id)
        && state.objects.get(&id).and_then(|instance| state.card_db.as_ref()
            .and_then(|db| db.get(instance.card_def_id)))
            .is_some_and(|definition| definition.subtypes.iter().any(|subtype| subtype.0 == "Goblin"))
}

pub(crate) fn actions(state: &GameState) -> Vec<Action> {
    let Some(op) = &state.pending_matron_search else { return Vec::new(); };
    if validate_structure(state).is_err() || state.priority_player != op.controller { return Vec::new(); }
    let mut actions: Vec<_> = state.players[op.controller].library.iter().copied()
        .filter(|id| eligible(state, *id))
        .filter_map(|id| state.exact_object(id))
        .map(|object| Action::ChooseGoblinMatronCard { window: op.window, object })
        .collect();
    actions.push(Action::DeclineGoblinMatronSearch { window: op.window });
    actions.push(Action::FailToFindGoblinMatronCard { window: op.window });
    actions.push(Action::Concede);
    actions
}

pub(crate) fn choose(state: &mut GameState, action: &Action) -> bool {
    if !actions(state).contains(action) { return false; }
    match action {
        Action::ChooseGoblinMatronCard { object, .. } => {
            if !eligible(state, object.id) || state.exact_object(object.id) != Some(*object) { return false; }
            match super::movement_choice::begin_matron_movement(state, *object) {
                Ok(()) => true,
                Err(error) => {
                    super::aura::record_entry_failure(state, error);
                    false
                }
            }
        }
        Action::DeclineGoblinMatronSearch { .. } => finish_without_card(state, false),
        Action::FailToFindGoblinMatronCard { .. } => finish_without_card(state, true),
        _ => false,
    }
}

fn finish_without_card(state: &mut GameState, searched: bool) -> bool {
    let Some(op) = state.pending_matron_search.take() else { return false; };
    let Some(entry) = op.resolving_entry else {
        state.pending_matron_search = Some(op);
        return false;
    };
    shuffle_if_searched(&mut state.players[op.controller].library, searched, rand::thread_rng);
    if searched {
        state.emit_event(GameEvent::LibraryShuffled { player: op.controller });
    }
    state.priority_player = op.resume_priority;
    super::resolution::finish_deferred_resolution(state, &entry);
    super::complete_stack_resolution(state);
    true
}

fn shuffle_if_searched<R: rand::Rng>(library: &mut [u64], searched: bool, make_rng: impl FnOnce() -> R) {
    if searched { library.shuffle(&mut make_rng()); }
}

pub(crate) fn validate_structure(state: &GameState) -> Result<(), &'static str> {
    let Some(op) = &state.pending_matron_search else { return Ok(()); };
    if op.version != 1 || op.window >= state.next_movement_window
        || op.active_player != state.active_player || op.active_player >= state.players.len()
        || op.controller >= state.players.len() || op.resume_priority >= state.players.len()
        || state.priority_player != op.controller || !state.trigger_placement_deferred
        || state.pending_movement.is_some() || state.pending_chittering.is_some()
        || state.pending_copy_order.is_some() || state.pending_commander_sba.is_some()
        || state.trigger_placement.is_some() || state.pending_tutor.is_some()
    { return Err("invalid Goblin Matron search structure"); }
    let entry = op.resolving_entry.as_deref().ok_or("missing Goblin Matron resolving entry")?;
    validate_resolving_entry(state, entry, op.controller)?;
    Ok(())
}

pub(crate) fn validate_play(state: &GameState) -> Result<(), &'static str> {
    if state.pending_matron_search.is_some() && state.card_db.is_none() {
        return Err("Goblin Matron database must be attached before play");
    }
    if state.pending_matron_search.is_some() && state.card_db.as_ref().is_some_and(|db|
        db.get(GOBLIN_MATRON).and_then(|definition| definition.triggered_abilities.first())
            .is_none_or(|ability| ability.trigger != crate::card::TriggerCondition::EntersBattlefield
                || ability.effect != Effect::GoblinMatronSearch))
    {
        return Err("missing Goblin Matron historical definition");
    }
    validate_structure(state)
}

pub(crate) fn public_wait(state: &GameState) -> Option<(u32, usize)> {
    state.pending_matron_search.as_ref().map(|op| (op.version, op.controller))
}

pub(crate) fn resolving_entry(state: &GameState) -> Option<&StackEntry> {
    state.pending_matron_search.as_ref()?.resolving_entry.as_deref()
}

pub(crate) fn selected_exact(state: &GameState, object: ExactObjectRef) -> bool {
    eligible(state, object.id) && state.exact_object(object.id) == Some(object)
}

#[cfg(test)]
mod tests {
    use rand::{Error, RngCore};

    use super::shuffle_if_searched;

    #[derive(Default)]
    struct CountingRng { calls: usize }

    impl RngCore for CountingRng {
        fn next_u32(&mut self) -> u32 { self.calls += 1; 0 }
        fn next_u64(&mut self) -> u64 { self.calls += 1; 0 }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            self.calls += 1;
            dest.fill(0);
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }

    #[test]
    fn declining_does_not_consume_shuffle_randomness() {
        let original = [1, 2, 3, 4];
        let mut library = original;
        let mut rng = CountingRng::default();
        let mut rng_created = 0;
        shuffle_if_searched(&mut library, false, || {
            rng_created += 1;
            &mut rng
        });
        assert_eq!(library, original);
        assert_eq!(rng_created, 0);
        assert_eq!(rng.calls, 0);

        shuffle_if_searched(&mut library, true, || {
            rng_created += 1;
            &mut rng
        });
        assert_eq!(rng_created, 1);
        assert!(rng.calls > 0);
    }
}
