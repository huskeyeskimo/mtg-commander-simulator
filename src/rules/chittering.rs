//! Closed printed ETB: public opponent target, private hand choice, one movement.
use crate::{action::Action, card::{Effect, ExactObjectRef, sample::ids::CHITTERING_RATS, ZoneType},
    game::{BoundedMovementInstruction, GameState, PendingChittering, PendingMovement, PendingTrigger, StackEntry, StackSource, Target},
    rules::transitions::TransitionError};

pub(crate) fn contains(effect: &Effect) -> bool {
    match effect {
        Effect::ChitteringRats => true,
        Effect::Multiple(es) | Effect::Modal { choices: es, .. } => es.iter().any(contains),
        Effect::Conditional { if_true, if_false, .. } => contains(if_true) || if_false.as_ref().is_some_and(|e| contains(e)),
        Effect::ForEach { effect, .. } => contains(effect), _ => false,
    }
}
pub(crate) fn trigger(trigger: &PendingTrigger) -> bool {
    trigger.ability_index == 0 && trigger.context.source_card_id == CHITTERING_RATS && trigger.context.effect == Effect::ChitteringRats
        && trigger.context.cast_spell.is_none() && trigger.context.zone_transition.is_none()
}
pub(crate) fn opponent_legal(state: &GameState, controller: usize, opponent: usize) -> bool {
    controller < state.players.len() && crate::targeting::target_is_legal(state, controller,
        &crate::card::TargetSpec::Opponent, &Target::Player(opponent))
}
pub(crate) fn entry_opponent(entry: &StackEntry, players: usize) -> Result<usize, &'static str> {
    let StackSource::TriggeredAbility { ability_index, context, .. } = &entry.source else { return Err("missing Chittering trigger"); };
    if *ability_index != 0 || context.source_card_id != CHITTERING_RATS || context.effect != Effect::ChitteringRats
        || context.cast_spell.is_some() || context.zone_transition.is_some() || entry.controller >= players { return Err("invalid Chittering captured contract"); }
    match (entry.targets.as_slice(), entry.target_generations.as_slice()) {
        ([Target::Player(opponent)], [None]) if *opponent < players && *opponent != entry.controller => Ok(*opponent),
        _ => Err("invalid Chittering opponent target"),
    }
}
pub(crate) fn private_operation(op: &PendingMovement) -> bool { matches!(op.instruction, BoundedMovementInstruction::ChitteringHandTop) }
pub(crate) fn resolving_entry(state: &GameState) -> Option<&StackEntry> {
    state.pending_chittering.as_ref().map(|p| p.resolving_entry.as_ref()).or_else(||
        state.pending_movement.as_ref().filter(|op| private_operation(op)).and_then(|op| op.resolving_entry.as_deref()))
}
pub(crate) fn public_wait(state: &GameState) -> Option<(u32, usize, usize)> {
    let entry = resolving_entry(state)?; Some((1, entry.controller, entry_opponent(entry, state.players.len()).ok()?))
}
fn hand_card(state: &GameState, owner: usize, card: ExactObjectRef) -> bool {
    state.exact_object(card.id) == Some(card) && super::reclamation::resident_count(state, card.id) == 1
        && state.players.get(owner).is_some_and(|p| p.hand.contains(&card.id))
        && state.objects.get(&card.id).is_some_and(|i| i.object_id == card.id && !i.is_token && i.owner == owner)
}
pub(crate) fn validate_child(state: &GameState, op: &PendingMovement) -> Result<(), &'static str> {
    let entry = op.resolving_entry.as_deref().ok_or("unowned private movement")?;
    let opponent = entry_opponent(entry, state.players.len())?;
    let ([Target::Object(id)], [Some(generation)]) = (op.targets.as_slice(), op.target_generations.as_slice()) else { return Err("invalid private movement selection"); };
    let card = ExactObjectRef { id: *id, generation: *generation };
    if !private_operation(op) || !op.resolution_owned || op.source.is_some() || op.resolving_source.is_some()
        || op.envelope.as_slice() != [BoundedMovementInstruction::ChitteringHandTop] || op.instruction_index != 0
        || op.instruction_controller != entry.controller || (!state.game_over && !opponent_legal(state, entry.controller, opponent))
        || !matches!(state.trigger_order_resume, Some(crate::game::TriggerOrderResume::AfterResolution)) || !state.trigger_placement_deferred
        || !hand_card(state, opponent, card)
        || op.proposals.len() != 1 || op.proposals[0].request.object != card || op.proposals[0].owner != opponent
        || op.proposals[0].request.from != ZoneType::Hand || op.proposals[0].request.to != ZoneType::Library
        || op.chooser(state.players.len()).is_some_and(|p| p != opponent) { return Err("invalid closed private movement"); }
    Ok(())
}
pub(crate) fn validate_structure(state: &GameState) -> Result<(), &'static str> {
    let context = |id: u64, index: usize, c: &crate::game::TriggerContext| -> Result<(), &'static str> {
        if c.source_card_id == CHITTERING_RATS || contains(&c.effect) {
            if index != 0 || c.source_card_id != CHITTERING_RATS || c.effect != Effect::ChitteringRats
                || c.cast_spell.is_some() || c.zone_transition.is_some() || id >= state.next_object_id
                || state.objects.get(&id).is_some_and(|i| i.object_id != id || i.zone_change_count < c.source_generation
                    || i.zone_change_count == c.source_generation && i.card_def_id != CHITTERING_RATS) { return Err("invalid historical Chittering context"); }
        }
        Ok(())
    };
    for t in &state.pending_triggers {
        context(t.source_id, t.ability_index, &t.context)?;
        if trigger(t) && (t.controller >= state.players.len() || (!state.game_over && state.players[t.controller].has_lost)) { return Err("invalid Chittering pending controller"); }
    }
    for e in state.stack.iter().chain(state.resolving_entry()) {
        if let StackSource::TriggeredAbility { source_id, ability_index, context: c } = &e.source { context(*source_id, *ability_index, c)?; }
    }
    if let Some(op) = &state.pending_chittering {
        let opponent = entry_opponent(&op.resolving_entry, state.players.len())?;
        if op.version != 1 || op.opponent != opponent
            || (!state.game_over && !opponent_legal(state, op.resolving_entry.controller, opponent))
            || !state.trigger_placement_deferred || state.priority_player != opponent
            || op.resume_priority >= state.players.len() || state.pending_movement.is_some() || state.pending_copy_order.is_some()
            || state.pending_commander_sba.is_some() || state.trigger_placement.is_some() || state.pending_tutor.is_some()
            || op.resolving_entry.id >= state.next_stack_id || state.stack.iter().any(|e| e.id == op.resolving_entry.id)
            || !matches!(state.trigger_order_resume, Some(crate::game::TriggerOrderResume::AfterResolution))
            || !state.players[opponent].hand.iter().any(|id| state.is_card(*id)) { return Err("invalid private selection ownership"); }
    }
    if let Some(op) = &state.pending_movement { if private_operation(op) { validate_child(state, op)?; } }
    let mut ids = std::collections::HashSet::new();
    let active = state.pending_triggers.iter().any(trigger) || resolving_entry(state).is_some()
        || state.stack.iter().any(|e| matches!(&e.source, StackSource::TriggeredAbility { context, .. } if contains(&context.effect)));
    if active && state.stack.iter().chain(state.resolving_entry()).any(|e| e.id >= state.next_stack_id || !ids.insert(e.id)) { return Err("colliding Chittering stack occurrence"); }
    for entry in state.stack.iter().chain(state.resolving_entry()) {
        if matches!(&entry.source, StackSource::TriggeredAbility { context, .. } if context.source_card_id == CHITTERING_RATS && contains(&context.effect)) {
            entry_opponent(entry, state.players.len())?;

        }
    }
    for t in &state.pending_triggers { if trigger(t) && !t.targets.is_empty() { return Err("preselected Chittering pending target"); } }
    Ok(())
}
pub(crate) fn validate_play(state: &GameState) -> Result<(), &'static str> {
    validate_structure(state)?;
    let active = public_wait(state).is_some() || state.pending_triggers.iter().any(trigger)
        || state.stack.iter().any(|e| matches!(&e.source, StackSource::TriggeredAbility { context, .. } if contains(&context.effect)));
    if active {
        let db = state.card_db.as_ref().ok_or("Chittering play needs database")?;
        if db.get(CHITTERING_RATS).and_then(|d| d.triggered_abilities.first()).is_none_or(|a|
            a.trigger != crate::card::TriggerCondition::EntersBattlefield || a.effect != Effect::ChitteringRats) { return Err("missing Chittering historical definition"); }
    }
    Ok(())
}
pub(crate) fn selection_actions(state: &GameState) -> Vec<Action> {
    if validate_play(state).is_err() || state.gameplay_stopped() { return Vec::new(); }
    let Some(op) = &state.pending_chittering else { return Vec::new(); };
    state.players[op.opponent].hand.iter().filter_map(|&id| state.exact_object(id).filter(|&c| hand_card(state, op.opponent, c)))
        .map(|card| Action::ChooseChitteringCard { window: op.resolving_entry.id, card }).collect()
}
pub(crate) fn preflight_resolution(state: &GameState, entry: &StackEntry) -> Result<(), TransitionError> {
    let opponent = entry_opponent(entry, state.players.len()).map_err(|_| TransitionError::UnsupportedPath)?;
    if !opponent_legal(state, entry.controller, opponent) || state.players[opponent].hand.iter().all(|id| !state.is_card(*id)) { return Ok(()); }
    super::movement_choice::catalog(state, false)?;
    state.next_movement_window.checked_add(1).ok_or(TransitionError::CapacityExhausted)?;
    state.next_zone_event_group_id.checked_add(1).ok_or(TransitionError::GroupIdExhausted)?;
    Ok(())
}
pub(crate) fn begin(state: &mut GameState, entry: StackEntry) {
    let opponent = entry_opponent(&entry, state.players.len()).expect("prevalidated Chittering entry");
    if !opponent_legal(state, entry.controller, opponent) || state.players[opponent].hand.iter().all(|id| !state.is_card(*id)) {
        super::complete_stack_resolution(state); return;
    }
    let resume_priority = state.priority_player;
    state.pending_chittering = Some(PendingChittering { version: 1, opponent, resume_priority, resolving_entry: Box::new(entry) });
    state.priority_player = opponent;
}
pub(crate) fn choose_card(state: &mut GameState, action: &Action) -> bool {
    if !selection_actions(state).contains(action) { return false; }
    let Action::ChooseChitteringCard { card, .. } = action else { return false; };
    let current = state.pending_chittering.as_ref().unwrap().clone();
    let mut op = match super::movement_choice::prepare_operation(state, &Effect::ChitteringRats, current.resolving_entry.controller,
        &[Target::Object(card.id)], &[Some(card.generation)], None, true, false) {
        Ok(op) => op, Err(error) => { super::aura::record_entry_failure(state, error); return false; }
    };
    op.resume_priority = current.resume_priority; op.resolution_owned = true; op.resolving_entry = Some(current.resolving_entry);
    super::movement_choice::normalize(&mut op, state.players.len());
    if let Some(chooser) = op.chooser(state.players.len()) {
        let Some(next) = state.next_movement_window.checked_add(1) else { super::aura::record_entry_failure(state, TransitionError::CapacityExhausted); return false; };
        state.pending_chittering = None; state.pending_movement = Some(op); state.next_movement_window = next; state.priority_player = chooser;
    } else {
        if let Err(error) = super::movement_choice::commit(state, &op) { super::aura::record_entry_failure(state, error); return false; }
        state.pending_chittering = None; state.priority_player = op.resume_priority;
        super::complete_stack_resolution(state);
    }
    true
}

/// The kernel adapter is sealed to an actually owned Chittering resolution.
pub(crate) fn request_context(state: &GameState, request: &super::transitions::TransitionRequest) -> bool {
    if let Some(op) = state.pending_movement.as_ref().filter(|op| private_operation(op)) {
        if op.targets != [Target::Object(request.object.id)] || op.target_generations != [Some(request.object.generation)] { return false; }
    }
    resolving_entry(state).is_some_and(|entry| entry_opponent(entry, state.players.len()).is_ok_and(|owner|
        hand_card(state, owner, request.object) && request.kind == super::transitions::MovementKind::Put
        && request.from == ZoneType::Hand && request.to == ZoneType::Library))
}

pub(crate) fn placement_actions(state: &GameState) -> Vec<Action> {
    if validate_play(state).is_err() || state.gameplay_stopped() || state.validate_trigger_placement().is_err() { return Vec::new(); }
    let Some(op) = &state.trigger_placement else { return Vec::new(); };
    let mut actions = Vec::new();
    for t in &state.pending_triggers {
        if t.controller != op.controller || !trigger(t) { continue; }
        let Some(handle) = t.placement else { continue; };
        for opponent in 0..state.players.len() { if opponent_legal(state, t.controller, opponent) { actions.push(Action::PlaceChitteringTrigger { handle, opponent }); } }
    }
    actions
}

pub(crate) fn committed_context(state: &GameState, requests: &[super::transitions::TransitionRequest], op: &PendingMovement) -> bool {
    if validate_child(state, op).is_err() { return false; }
    let Some(original) = resolving_entry(state) else { return false; };
    if bincode::serialize(original).ok() != op.resolving_entry.as_ref().and_then(|e| bincode::serialize(e.as_ref()).ok()) { return false; }
    if let Some(current) = state.pending_movement.as_ref().filter(|p| private_operation(p)) {
        if current.targets != op.targets || current.target_generations != op.target_generations { return false; }
    }
    let expected: Vec<_> = op.proposals.iter().filter(|p| !p.canceled && p.proposed != p.request.from)
        .map(|p| super::transitions::TransitionRequest {to: p.proposed, ..p.request}).collect();
    expected == requests
}
