//! Closed printed Phyrexian Reclamation contract; no generic ability executor.
use crate::{action::Action, card::{sample::ids::PHYREXIAN_RECLAMATION, CardType, Effect, ExactObjectRef, TargetSpec},
    game::{GameState, Phase, StackEntry, StackSource, Target}, mana::ManaCost};

pub(crate) const EFFECT: Effect = Effect::ReturnFromGraveyardToHand { target: TargetSpec::CreatureCardInOwnGraveyard };
fn cost() -> ManaCost { ManaCost::new(1, 0, 0, 1, 0, 0) }

/// Copied printed abilities are outside this bounded contract. Use the same
/// applicability test as the layers engine, rather than guessing from selectors.
pub(crate) fn printed_abilities_available(state: &GameState, id: u64) -> bool {
    state.get_characteristics(id).is_some_and(|c| !c.abilities_removed) && printed_copy_supported(state, id)
}
pub(crate) fn printed_copy_supported(state: &GameState, id: u64) -> bool {
    let Some(db) = state.card_db.as_ref() else { return false; };
    let Some(inst) = state.objects.get(&id) else { return false; };
    let battlefield = state.battlefield.iter().copied().collect();
    !state.continuous_effects.iter().any(|e| matches!(e.modification,
            crate::layers::LayerModification::CopyOf(card) if card != inst.card_def_id)
            && crate::layers::effect_applies_to(e, id, inst, &state.objects, &battlefield, db))
}
/// An unsupported copied static can change mandatory output of other sources.
/// Excluding only that permanent from optional tap candidates is insufficient.
pub(crate) fn copied_mana_bonus_context(state: &GameState, id: u64) -> bool {
    let Some(db) = state.card_db.as_ref() else { return true; };
    let Some(inst) = state.objects.get(&id) else { return true; };
    let Some(chars) = state.get_characteristics(id) else { return true; };
    if chars.controller != state.priority_player || chars.abilities_removed { return false; }
    let bonus = |card| db.get(card).is_none_or(|def| def.static_abilities.iter().any(|a|
        matches!(a, crate::layers::StaticAbility::ManaFromNonlandBonus | crate::layers::StaticAbility::ManaFromSwampBonus)));
    let battlefield = state.battlefield.iter().copied().collect();
    state.continuous_effects.iter().any(|e| matches!(e.modification,
        crate::layers::LayerModification::CopyOf(card) if card != inst.card_def_id && (bonus(inst.card_def_id) || bonus(card)))
        && crate::layers::effect_applies_to(e, id, inst, &state.objects, &battlefield, db))
}
fn context(state: &GameState) -> bool {
    let actor = state.priority_player;
    state.card_db.is_some() && !state.gameplay_stopped() && actor < state.players.len()
        && !state.players[actor].has_lost && !state.players[actor].has_won
        && !state.movement_or_hand_pending() && state.pending_copy_order.is_none()
        && state.pending_commander_sba.is_none() && state.trigger_placement.is_none()
        && state.pending_triggers.is_empty() && state.pending_tutor.is_none() && !super::cleanup_discard_required(state)
        && !matches!(state.phase, Phase::Mulligan | Phase::Untap)
        && !(state.phase == Phase::Cleanup && !state.cleanup_needs_repeat)
        && !(state.phase == Phase::DeclareAttackers && actor == state.active_player && state.combat.attackers.is_empty())
        && !(state.phase == Phase::DeclareBlockers && actor != state.active_player)
        && state.validate_commanders().is_ok() && state.validate_commander_choices().is_ok()
        && state.validate_movement_choices().is_ok() && state.validate_trigger_placement().is_ok()
        && crate::targeting::validate_pending_auras(state).is_ok()
        && crate::targeting::validate_pending_equips(state).is_ok() && validate_play(state).is_ok()
}
pub(crate) fn resident_count(state: &GameState, id: u64) -> usize {
    state.battlefield.iter().filter(|&&other| other == id).count() + state.players.iter().map(|p|
        [&p.hand, &p.library, &p.graveyard, &p.exile, &p.command_zone].iter()
            .map(|zone| zone.iter().filter(|&&other| other == id).count()).sum::<usize>()).sum::<usize>()
        + state.stack.iter().filter(|e| e.source.physical_object() == Some(id)).count()
        + usize::from(state.resolving_entry().is_some_and(|e| e.source.physical_object() == Some(id)))
}
pub(crate) fn target_legal(state: &GameState, actor: usize, target: ExactObjectRef) -> bool {
    let Some(db) = state.card_db.as_ref() else { return false; };
    state.players.get(actor).is_some_and(|p| p.graveyard.contains(&target.id))
        && state.exact_object(target.id) == Some(target) && resident_count(state, target.id) == 1
        && state.objects.get(&target.id).is_some_and(|i| !i.is_token && i.object_id == target.id && i.owner == actor
            && db.get(i.card_def_id).is_some_and(|d| d.card_types.contains(&CardType::Creature)))
}
pub(crate) struct Prepared { payment: super::mana::PaymentPlan, target_card_id: u64, source_owner: usize }
pub(crate) fn prepare(state: &GameState, action: &Action) -> Option<Prepared> {
    let Action::ActivateReclamation { source, target } = action else { return None; };
    if !context(state) || source.id == target.id || state.exact_object(source.id) != Some(*source)
        || !state.battlefield.contains(&source.id) || resident_count(state, source.id) != 1
        || !printed_abilities_available(state, source.id) { return None; }
    let inst = state.objects.get(&source.id)?;
    if inst.is_token || inst.owner >= state.players.len() || inst.object_id != source.id || inst.card_def_id != PHYREXIAN_RECLAMATION
        || state.get_characteristics(source.id)?.controller != state.priority_player { return None; }
    let def = state.card_db.as_ref()?.get(PHYREXIAN_RECLAMATION)?;
    let ability = def.activated_abilities.first()?;
    if def.activated_abilities.len() != 1 || ability.cost != cost() || ability.life_cost != 2
        || ability.requires_tap || ability.sacrifice_cost.is_some() || ability.effect != EFFECT { return None; }
    let actor = state.priority_player;
    if state.players[actor].life < 2 || !target_legal(state, actor, *target) { return None; }
    // Check the same nominal capacities required at the reached Phase 3A leaf.
    // This is not a future resolution plan; responses still revalidate the target.
    state.next_movement_window.checked_add(1)?;
    super::transitions::preflight_movement_batch(state, &[super::transitions::TransitionRequest {
        object: *target, from: crate::card::ZoneType::Graveyard, to: crate::card::ZoneType::Hand,
        kind: super::transitions::MovementKind::Put }]).ok()?;
    state.next_stack_id.checked_add(1)?; state.stack.len().checked_add(1)?;
    let mut stack_ids = std::collections::HashSet::new();
    if state.stack.iter().chain(state.resolving_entry()).any(|e| e.id >= state.next_stack_id || !stack_ids.insert(e.id)) { return None; }
    state.loss_boundary.accepted_actions.checked_add(1)?;
    let payment = super::mana::plan_reclamation(state, actor, &cost())?;
    Some(Prepared { payment, target_card_id: state.objects.get(&target.id)?.card_def_id, source_owner: inst.owner })
}
pub(crate) fn actions(state: &GameState) -> Vec<Action> {
    if !context(state) { return Vec::new(); }
    let mut actions = Vec::new();
    for &id in &state.battlefield {
        if state.objects.get(&id).is_none_or(|i| i.card_def_id != PHYREXIAN_RECLAMATION) { continue; }
        let Some(source) = state.exact_object(id) else { continue; };
        for &id in &state.players[state.priority_player].graveyard {
            let Some(target) = state.exact_object(id) else { continue; };
            let action = Action::ActivateReclamation { source, target };
            if prepare(state, &action).is_some() { actions.push(action); }
        }
    }
    actions
}
pub(crate) fn activate(state: &mut GameState, action: &Action) {
    let Some(prepared) = prepare(state, action) else { return; };
    let Action::ActivateReclamation { source, target } = action else { return; };
    let actor = state.priority_player;
    if !super::mana::commit_payment(state, actor, &cost(), prepared.payment) { return; }
    state.players[actor].life -= 2;
    let id = state.new_stack_id();
    state.stack.push(StackEntry { id, source: StackSource::ReclamationAbility { source: *source,
        source_owner: prepared.source_owner, target_card_id: prepared.target_card_id }, controller: actor,
        targets: vec![Target::Object(target.id)], target_generations: vec![Some(target.generation)] });
    state.consecutive_passes = 0;
}
pub(crate) fn entry_target(entry: &StackEntry) -> Result<ExactObjectRef, &'static str> {
    match (entry.targets.as_slice(), entry.target_generations.as_slice()) {
        ([Target::Object(id)], [Some(generation)]) => Ok(ExactObjectRef { id: *id, generation: *generation }),
        _ => Err("invalid Reclamation target cardinality"),
    }
}
fn validate_entry(state: &GameState, entry: &StackEntry) -> Result<(), &'static str> {
    let StackSource::ReclamationAbility { source, source_owner, target_card_id } = entry.source else { return Ok(()); };
    let target = entry_target(entry)?;
    if entry.id >= state.next_stack_id || entry.controller >= state.players.len() || source_owner >= state.players.len()
        || source.id == target.id { return Err("invalid Reclamation frame"); }
    for (exact, owner, card) in [(source, source_owner, PHYREXIAN_RECLAMATION), (target, entry.controller, target_card_id)] {
        let inst = state.objects.get(&exact.id).ok_or("missing Reclamation endpoint")?;
        if inst.is_token || inst.object_id != exact.id || inst.owner != owner || inst.card_def_id != card || inst.zone_change_count < exact.generation
            || resident_count(state, exact.id) != 1 { return Err("invalid Reclamation endpoint facts"); }
    }
    if let Some(db) = &state.card_db {
        if db.get(PHYREXIAN_RECLAMATION).is_none_or(|d| d.activated_abilities.len() != 1
            || d.activated_abilities.first().is_none_or(|a| a.effect != EFFECT || a.cost != cost()
                || a.life_cost != 2 || a.requires_tap || a.sacrifice_cost.is_some()))
            || db.get(target_card_id).is_none_or(|d| !d.card_types.contains(&CardType::Creature)) {
            return Err("missing Reclamation historical definition");
        }
    }
    Ok(())
}
pub(crate) fn validate_structure(state: &GameState) -> Result<(), &'static str> {
    let mut ids = std::collections::HashSet::new();
    let entries = || state.stack.iter().chain(state.resolving_entry());
    if entries().any(|e| matches!(e.source, StackSource::ReclamationAbility { .. }))
        && entries().any(|e| !ids.insert(e.id) || e.id >= state.next_stack_id) { return Err("duplicate Reclamation stack occurrence"); }
    for entry in entries() {
        if let StackSource::ActivatedAbility { source_id, .. } = entry.source {
            if state.objects.get(&source_id).is_some_and(|i| i.card_def_id == PHYREXIAN_RECLAMATION) { return Err("unowned legacy Reclamation ability"); }
        }
        validate_entry(state, entry)?;
    }
    Ok(())
}
pub(crate) fn validate_play(state: &GameState) -> Result<(), &'static str> {
    validate_structure(state)?;
    if state.card_db.is_none() && state.stack.iter().chain(state.resolving_entry()).any(|e|
        matches!(e.source, StackSource::ReclamationAbility { .. })) { return Err("Reclamation play requires database"); }
    Ok(())
}

/// The new recipient is owned only by the closed frame, never generic abilities.
pub(crate) fn uses_selector(effect: &Effect) -> bool {
    match effect {
        Effect::ReturnFromGraveyardToHand { target: TargetSpec::CreatureCardInOwnGraveyard } => true,
        Effect::Multiple(children) | Effect::Modal { choices: children, .. } => children.iter().any(uses_selector),
        Effect::Conditional { if_true, if_false, .. } => uses_selector(if_true) || if_false.as_ref().is_some_and(|e| uses_selector(e)),
        Effect::ForEach { effect, .. } => uses_selector(effect), _ => false,
    }
}
