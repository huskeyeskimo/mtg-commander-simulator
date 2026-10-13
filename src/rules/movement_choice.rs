//! Phase 3A: finite movement leaves and flat audited instruction envelopes.
use crate::{action::Action, card::{Effect, ExactObjectRef, KeywordAbility, ObjectId, ZoneType},
    game::{GameState, LibraryPlacement, MovementDecision, MovementDecisionRecord,
        MovementProposal, BoundedMovementInstruction, MovementReplacementCandidate as Candidate, PendingMovement, PlayerIndex, Target},
    replacement::{ReplacementAction, ReplacementEventKind},
    rules::transitions::{self, MovementKind, TransitionError, TransitionRequest}};

pub(crate) fn movement_leaf(effect: &Effect) -> bool {
    matches!(effect, Effect::ChitteringRats | Effect::BounceTo { .. } | Effect::BounceAllNonlandOpponents
        | Effect::ShuffleIntoLibrary { .. } | Effect::PutOnBottomOfLibrary { .. }
        | Effect::ReturnToTopOfLibrary { .. } | Effect::ReturnFromGraveyardToHand { .. }
        | Effect::ReturnLinkedExileToHand | Effect::DestroyTarget { .. } | Effect::DestroyAll
        | Effect::SacrificeCreatures { .. } | Effect::EachOpponentSacrifices { .. })
}
fn draw_leaf(effect: &Effect) -> bool { matches!(effect, Effect::DrawCards { .. }) }
pub(crate) fn private_draw_operation(op: &PendingMovement) -> bool {
    matches!(op.instruction, BoundedMovementInstruction::Draw { .. })
}
fn draw_envelope(effect: &Effect) -> bool {
    match effect {
        Effect::DrawCards { .. } => true,
        Effect::Multiple(children) => !children.is_empty() &&
            (children.iter().all(draw_leaf) || matches!(children.as_slice(),
                [Effect::GainKeywordUntilEOT { keyword: KeywordAbility::Haste,
                    target: crate::card::TargetSpec::AnyCreature }, Effect::DrawCards { .. }])),
        _ => false,
    }
}
pub(crate) fn preflight_draw_envelope(state: &GameState, effect: &Effect, controller: PlayerIndex)
    -> Result<(), TransitionError> {
    if !draw_envelope(effect) { return Ok(()); }
    let total = match effect {
        Effect::DrawCards { count } => *count as usize,
        Effect::Multiple(children) => children.iter().filter_map(|e| match e {
            Effect::DrawCards { count } => Some(*count as usize), _ => None,
        }).try_fold(0usize, |sum, count| sum.checked_add(count).ok_or(TransitionError::CapacityExhausted))?,
        _ => 0,
    };
    preflight_draw_sequence(state, controller, total)
}
pub(crate) fn preflight_draw_sequence(state: &GameState, player: PlayerIndex, count: usize)
    -> Result<(), TransitionError> {
    let library = state.players.get(player).ok_or(TransitionError::InvalidPlayer(0))?;
    let commanders: Vec<_> = library.library.iter().filter(|id| state.is_card(**id)).take(count)
        .filter(|id| state.is_commander(**id)).copied().collect();
    if commanders.is_empty() { return Ok(()); }
    state.card_db.as_ref().ok_or(TransitionError::MissingDatabase)?;
    state.validate_commanders().map_err(|_| TransitionError::UnsupportedPath)?;
    if state.replacement_effects.iter().any(|e| matches!(e.applies_to,
        ReplacementEventKind::WouldMoveTo { .. } | ReplacementEventKind::CardDraw))
        || [crate::layers::StaticAbility::RenfieldDrawReplacement,
            crate::layers::StaticAbility::AbundanceReplacement,
            crate::layers::StaticAbility::PhialDrawDoubler].iter()
            .any(|ability| super::has_static_ability_on_battlefield(state, player, ability)) {
        return Err(TransitionError::UnsupportedPath);
    }
    let candidates = u64::try_from(commanders.len()).map_err(|_| TransitionError::CapacityExhausted)?;
    state.next_movement_window.checked_add(candidates).ok_or(TransitionError::CapacityExhausted)?;
    state.next_zone_event_group_id.checked_add(candidates).ok_or(TransitionError::CapacityExhausted)?;
    for id in commanders {
        let instance = state.objects.get(&id).ok_or(TransitionError::MissingObject(id))?;
        if instance.owner != player || instance.zone_change_count == u32::MAX
            || state.card_db().get(instance.card_def_id).is_none() {
            return Err(TransitionError::UnsupportedPath);
        }
    }
    Ok(())
}
fn contains_draw(effect: &Effect) -> bool {
    match effect {
        Effect::DrawCards { .. } => true,
        Effect::Multiple(children) | Effect::Modal { choices: children, .. } => children.iter().any(contains_draw),
        Effect::Conditional { if_true, if_false, .. } => contains_draw(if_true) || if_false.as_ref().is_some_and(|e| contains_draw(e)),
        Effect::ForEach { effect, .. } => contains_draw(effect),
        _ => false,
    }
}
fn synchronous(effect: &Effect) -> bool {
    matches!(effect, Effect::GainLife { .. } | Effect::LoseLife { .. }
        | Effect::AddMana { .. } | Effect::UntapTarget { .. })
}
fn contains_movement(effect: &Effect) -> bool {
    movement_leaf(effect) || match effect {
        Effect::Multiple(children) => children.iter().any(contains_movement),
        Effect::Modal { choices, .. } => choices.iter().any(contains_movement),
        Effect::Conditional { if_true, if_false, .. } => contains_movement(if_true) || if_false.as_ref().is_some_and(|e| contains_movement(e)),
        Effect::ForEach { effect, .. } => contains_movement(effect),
        _ => false,
    }
}
pub(crate) fn admission(state: &GameState, effect: &Effect) -> Result<bool, TransitionError> {
    if super::chittering::contains(effect) {
        if *effect != Effect::ChitteringRats || state.stack.last().is_none_or(|e| super::chittering::entry_opponent(e, state.players.len()).is_err()) {
            return Err(TransitionError::UnsupportedPath);
        }
        catalog(state, false)?;
        return Ok(true);
    }
    if super::reclamation::uses_selector(effect) && (effect != &super::reclamation::EFFECT
        || state.stack.last().is_none_or(|e| !matches!(e.source, crate::game::StackSource::ReclamationAbility { .. }))) {
        return Err(TransitionError::UnsupportedPath);
    }
    let relevant = !state.commanders.is_empty() || state.replacement_effects.iter()
        .any(|e| matches!(e.applies_to, ReplacementEventKind::WouldMoveTo { .. }));
    if !relevant && !super::reclamation::uses_selector(effect) { return Ok(false); }
    if contains_draw(effect) && state.players.iter().any(|player|
        player.library.iter().any(|id| state.is_commander(*id))) {
        if !draw_envelope(effect) { return Err(TransitionError::UnsupportedPath); }
        if state.card_db.is_none() || state.validate_commanders().is_err()
            || state.replacement_effects.iter().any(|e| matches!(e.applies_to,
                ReplacementEventKind::WouldMoveTo { .. } | ReplacementEventKind::CardDraw))
            || state.players.iter().enumerate().any(|(player, p)| p.library.iter().any(|id| state.is_commander(*id))
                && (super::has_static_ability_on_battlefield(state, player, &crate::layers::StaticAbility::RenfieldDrawReplacement)
                    || super::has_static_ability_on_battlefield(state, player, &crate::layers::StaticAbility::AbundanceReplacement)
                    || super::has_static_ability_on_battlefield(state, player, &crate::layers::StaticAbility::PhialDrawDoubler))) {
            return Err(TransitionError::UnsupportedPath);
        }
        return Ok(true);
    }
    if synchronous_destroy_draw(state, effect) {
        validate_synchronous_destroy_draw(state)?;
        return Ok(false);
    }
    // Draw/tutor execution is Phase 3B. Reject its structural envelope before
    // an earlier instruction can run if an actual movement choice is possible.
    fn unsupported_continuation(effect: &Effect) -> bool {
        match effect {
            Effect::DrawCards { .. } | Effect::DrawThenDiscard { .. } | Effect::SearchLibrary { .. }
                => true,
            Effect::Multiple(children) => children.iter().any(unsupported_continuation),
            Effect::Modal { choices, .. } => choices.iter().any(unsupported_continuation),
            Effect::Conditional { if_true, if_false, .. } => unsupported_continuation(if_true) || if_false.as_ref().is_some_and(|e| unsupported_continuation(e)),
            Effect::ForEach { effect, .. } => unsupported_continuation(effect),
            _ => false,
        }
    }
    if unsupported_continuation(effect) && (!state.commanders.is_empty()
        && state.players.iter().any(|player| player.library.iter().any(|id| state.is_commander(*id)))
        || state.replacement_effects.iter().any(|e| matches!(e.applies_to, ReplacementEventKind::WouldMoveTo { .. }))
) {
        return Err(TransitionError::UnsupportedPath);
    }
    if !contains_movement(effect) { return Ok(false); }
    match effect {
        Effect::Multiple(children) if children.iter().all(|e| movement_leaf(e) || synchronous(e)) => {},
        e if movement_leaf(e) => {},
        _ => return Err(TransitionError::UnsupportedPath),
    }
    fn invalid_leaf(effect: &Effect) -> bool {
        use crate::card::TargetSpec;
        let battlefield_recipient = |target: &TargetSpec| matches!(target,
            TargetSpec::AnyCreature | TargetSpec::AnyPermanent | TargetSpec::AnyNonlandPermanent
            | TargetSpec::CreatureOrPlaneswalker | TargetSpec::EachCreature | TargetSpec::NoTarget);
        let graveyard_recipient = |target: &TargetSpec| matches!(target,
            TargetSpec::AnyCreature | TargetSpec::AnyPermanent | TargetSpec::AnyNonlandPermanent | TargetSpec::NoTarget);
        match effect {
            Effect::BounceTo { zone, target } => matches!(zone, ZoneType::Battlefield | ZoneType::Stack) || !battlefield_recipient(target),
            Effect::ShuffleIntoLibrary { target } | Effect::PutOnBottomOfLibrary { target } | Effect::DestroyTarget { target } => !battlefield_recipient(target),
            Effect::ReturnToTopOfLibrary { target } => !graveyard_recipient(target),
            Effect::ReturnFromGraveyardToHand { target } => !graveyard_recipient(target) && *target != TargetSpec::CreatureCardInOwnGraveyard,
            Effect::SacrificeCreatures { target, .. } => !matches!(target, TargetSpec::AnyPlayer | TargetSpec::Opponent | TargetSpec::Controller | TargetSpec::NoTarget),
            Effect::Multiple(children) => children.iter().any(invalid_leaf), _ => false,
        }
    }
    if invalid_leaf(effect) { return Err(TransitionError::UnsupportedPath); }
    let catalog = catalog(state, contains_death(effect))?;
    // CR 400.10 and 400.8 exceptions are outside this departure envelope.
    // Compute complete destination closure before offers, including initially
    // inapplicable redirects. Hand/Library same-zone outcomes are represented
    // as no movement; Command/Exile same-zone exceptions reject admission.
    let destinations: Vec<_> = catalog.iter().filter_map(|c| match c {
        Candidate::Ordinary { action: ReplacementAction::RedirectToZone(zone), .. } => Some(*zone), _ => None }).collect();
    fn source_zones(effect: &Effect) -> Vec<ZoneType> {
        match effect {
            Effect::ReturnLinkedExileToHand => vec![ZoneType::Exile],
            Effect::ReturnToTopOfLibrary { .. } | Effect::ReturnFromGraveyardToHand { .. } => vec![ZoneType::Graveyard],
            Effect::Multiple(children) => children.iter().flat_map(source_zones).collect(),
            _ => vec![ZoneType::Battlefield],
        }
    }
    if source_zones(effect).iter().any(|from| matches!(from, ZoneType::Exile | ZoneType::Command) && destinations.contains(from)) {
        return Err(TransitionError::UnsupportedPath);
    }
    Ok(true)
}

/// Compatibility for the exact synchronous pair; it never owns a draw tail.
/// Leave unrelated states outside Phase 3A's admission boundary on their legacy route.
pub(crate) fn synchronous_destroy_draw(state: &GameState, effect: &Effect) -> bool {
    (!state.commanders.is_empty() || state.replacement_effects.iter()
        .any(|e| matches!(e.applies_to, ReplacementEventKind::WouldMoveTo { .. })))
        && matches!(effect, Effect::Multiple(children) if matches!(children.as_slice(),
            [Effect::DestroyTarget { target: crate::card::TargetSpec::AnyCreature }, Effect::DrawCards { count: 1 }]))
}
fn validate_synchronous_destroy_draw(state: &GameState) -> Result<(), TransitionError> {
    state.card_db.as_ref().ok_or(TransitionError::MissingDatabase)?;
    state.validate_commanders().map_err(|_| TransitionError::UnsupportedPath)?;
    // Hidden Hand and Library residences are indistinguishable to nonowners.
    // Eligibility uses public residence, never a secret top-card inspection.
    if state.commanders.iter().any(|commander| !state.battlefield.contains(&commander.object)
        && !state.players.iter().any(|player| [&player.graveyard, &player.exile, &player.command_zone]
            .iter().any(|zone| zone.contains(&commander.object)))
        && !state.stack.iter().any(|entry| entry.source.physical_object() == Some(commander.object)))
        || state.replacement_effects.iter().any(|effect| matches!(effect.applies_to,
            ReplacementEventKind::WouldDie | ReplacementEventKind::WouldMoveTo { .. } | ReplacementEventKind::CardDraw)) {
        return Err(TransitionError::UnsupportedPath);
    }
    // Destruction and linked followups only move to Graveyard; refresh only
    // prunes replacement registrations. Draw statics may change or expand the
    // draw, but no library can acquire a Commander before deferred settlement.
    Ok(())
}
pub(crate) fn preflight_synchronous_destroy_draw(state: &GameState, effect: &Effect, controller: PlayerIndex,
    targets: &[Target], generations: &[Option<u32>], physical_source: Option<ObjectId>)
    -> Result<Option<ExactObjectRef>, TransitionError> {
    validate_synchronous_destroy_draw(state)?;
    if controller >= state.players.len() { return Err(TransitionError::InvalidPlayer(0)); }
    if let Some(source) = physical_source {
        if state.is_commander(source) { return Err(TransitionError::UnsupportedPath); }
        let instance = state.objects.get(&source).ok_or(TransitionError::MissingObject(source))?;
        if instance.zone_change_count == u32::MAX { return Err(TransitionError::GenerationExhausted(source)); }
    }
    // The exact pair has the existing sole AnyCreature contract. An illegal
    // or stale target fizzles the whole entry, including its untargeted draw.
    if !crate::targeting::valid_transition_ability_targets(state, controller, effect, targets, generations) {
        return Ok(None);
    }
    let (Target::Object(id), Some(generation)) = (&targets[0], generations[0]) else {
        return Err(TransitionError::UnsupportedPath);
    };
    let object = ExactObjectRef { id: *id, generation };
    transitions::preflight_destroy_batch(state, &[object])?;
    Ok(Some(object))
}

fn contains_death(effect: &Effect) -> bool {
    matches!(effect, Effect::DestroyTarget { .. } | Effect::DestroyAll | Effect::SacrificeCreatures { .. } | Effect::EachOpponentSacrifices { .. })
        || match effect { Effect::Multiple(children) => children.iter().any(contains_death), _ => false }
}
pub(crate) fn catalog(state: &GameState, death: bool) -> Result<Vec<Candidate>, TransitionError> {
    let mut catalog = Vec::new();
    for e in &state.replacement_effects {
        if !matches!(e.applies_to, ReplacementEventKind::WouldMoveTo { .. })
            && !(death && e.applies_to == ReplacementEventKind::WouldDie) { continue; }
        if e.applies_to == ReplacementEventKind::WouldDie && !e.is_self_replacement {
            return Err(TransitionError::UnsupportedPath);
        }
        let source = ExactObjectRef { id: e.source_id, generation: match e.applies_to { ReplacementEventKind::WouldMoveTo { source_generation, .. } => source_generation, _ => state.exact_object(e.source_id).map(|s| s.generation).unwrap_or(0) } };
        if state.exact_object(source.id) != Some(source) || !state.battlefield.contains(&source.id) {
            return Err(TransitionError::StaleIncarnation(source.id));
        }
        let definition = state.objects[&source.id].card_def_id;
        if state.card_db.as_ref().is_some_and(|db| db.get(definition).is_none()) { return Err(TransitionError::MissingDefinition(definition)); }
        if e.controller >= state.players.len() { return Err(TransitionError::InvalidPlayer(source.id)); }
        if !matches!(e.action, ReplacementAction::RedirectToZone(ZoneType::Hand | ZoneType::Library
            | ZoneType::Command | ZoneType::Graveyard | ZoneType::Exile)
            | ReplacementAction::Prevent) { return Err(TransitionError::UnsupportedPath); }

        let occurrence = catalog.iter().filter(|candidate| matches!(candidate,
            Candidate::Ordinary { source: s, controller, predicate, action, self_replacement, .. }
            if *s == source && *controller == e.controller && *predicate == e.applies_to
                && *action == e.action && *self_replacement == e.is_self_replacement)).count();
        catalog.push(Candidate::Ordinary { source, controller: e.controller, predicate: e.applies_to.clone(),
            action: e.action.clone(), self_replacement: e.is_self_replacement, occurrence });
    }
    Ok(catalog)
}

pub(crate) fn execute(state: &mut GameState, effects: Vec<BoundedMovementInstruction>, controller: PlayerIndex,
    targets: &[Target], generations: &[Option<u32>], source: Option<ObjectId>, earlier_completed: bool, start_index: usize) {
    let mut previous_completed = earlier_completed;
    for index in start_index..effects.len() {
        let instruction = &effects[index];
        let effect = instruction.effect();
        if let Effect::DrawCards { count } = effect {
            execute_draws(state, count, &effects, index, controller, targets, generations, source, false);
            return;
        } else if matches!(instruction, BoundedMovementInstruction::ExpediteHaste) {
            super::effects::resolve_synchronous_movement_companion(state, &effect, controller, targets, generations, source);
            if state.gameplay_stopped() { return; }
            previous_completed = true;
        } else if movement_leaf(&effect) {
            match prepare_operation(state, &effect, controller, targets, generations, source, true, previous_completed) {
                Ok(mut op) => {
                    let Some(next_window) = state.next_movement_window.checked_add(1) else { super::aura::record_entry_failure(state, TransitionError::CapacityExhausted); return; };
                    op.envelope = effects.clone(); op.instruction_index = index;
                    normalize(&mut op, state.players.len());
                    if let Some(chooser) = op.chooser(state.players.len()) {
                        state.priority_player = chooser;
                        state.pending_movement = Some(op);
                        state.next_movement_window = next_window;
                        return;
                    }
                    if let Err(error) = commit(state, &op) { super::aura::record_entry_failure(state, error); return; }
                    execute(state, effects, controller, targets, generations, source, true, index + 1);
                    return;
                }
                Err(error) => { super::aura::record_entry_failure(state, error); return; }
            }
        } else {
            if let Err(error) = preflight_companion(state, &effect, controller, targets) {
                super::aura::record_entry_failure(state, error); return;
            }
            super::effects::resolve_synchronous_movement_companion(state, &effect, controller, targets, generations, source);
            if state.gameplay_stopped() { return; }
            previous_completed = true;
        }
    }
}

pub(crate) fn begin_direct_draw(state: &mut GameState, player: PlayerIndex, count: u32, turn_draw: bool) {
    if let Err(error) = preflight_draw_sequence(state, player, count as usize) {
        super::aura::record_entry_failure(state, error); return;
    }
    let prior_deferred = state.trigger_placement_deferred;
    let choice_expected = state.players[player].library.iter().filter(|id| state.is_card(**id))
        .take(count as usize).any(|id| state.is_commander(*id));
    if choice_expected && !turn_draw { state.trigger_placement_deferred = true; }
    let envelope = vec![BoundedMovementInstruction::Draw { count }];
    execute_draws(state, count, &envelope, 0, player, &[], &[], None, turn_draw);
    if choice_expected && !turn_draw {
        if let Some(pending) = state.pending_movement.as_mut() {
            pending.draw_resume_deferred = prior_deferred;
        } else {
            state.trigger_placement_deferred = prior_deferred;
        }
    }
}

fn execute_draws(state: &mut GameState, count: u32, envelope: &[BoundedMovementInstruction], index: usize,
    controller: PlayerIndex, targets: &[Target], generations: &[Option<u32>], source: Option<ObjectId>, turn_draw: bool) {
    for completed in 0..count {
        if state.gameplay_stopped() { return; }
        let top = state.first_library_card(controller).map(|i| state.players[controller].library[i]);
        if let Some(id) = top.filter(|id| state.is_commander(*id)) {
            let effect = envelope[index].effect();
            let mut op = match prepare_operation(state, &effect, controller, targets, generations, source, true, false) {
                Ok(op) => op, Err(error) => { super::aura::record_entry_failure(state, error); return; }
            };
            if op.proposals.len() != 1 || op.proposals[0].request.object.id != id {
                super::aura::record_entry_failure(state, TransitionError::UnsupportedPath); return;
            }
            let Some(window) = state.next_movement_window.checked_add(1) else {
                super::aura::record_entry_failure(state, TransitionError::CapacityExhausted); return;
            };
            op.envelope = envelope.to_vec(); op.instruction_index = index;
            op.instruction = envelope[index].clone(); op.draws_remaining = count - completed - 1;
            let BoundedMovementInstruction::Draw { count: original_count } = &op.instruction else { unreachable!() };
            let completed_in_instruction = *original_count - op.draws_remaining - 1;
            op.draw_start_library_count = state.card_count(&state.players[controller].library)
                + completed_in_instruction as usize;
            op.turn_draw = turn_draw;
            normalize(&mut op, state.players.len());
            let Some(chooser) = op.chooser(state.players.len()) else {
                super::aura::record_entry_failure(state, TransitionError::UnsupportedPath); return;
            };
            state.priority_player = chooser;
            state.pending_movement = Some(op);
            state.next_movement_window = window;
            return;
        }
        // Existing ordinary and failed-draw behavior remains authoritative for
        // each non-Commander individual draw.
        super::draw_cards(state, controller, 1);
        if state.sba_failure.is_some() { return; }
    }
    if !turn_draw {
        execute(state, envelope.to_vec(), controller, targets, generations, source, true, index + 1);
    }
}

pub(crate) fn prepare_operation(state: &GameState, effect: &Effect, controller: PlayerIndex,
    targets: &[Target], generations: &[Option<u32>], source: Option<ObjectId>, preflight: bool, allow_stale: bool) -> Result<PendingMovement, TransitionError> {
    use crate::targeting::EffectRecipients;
    let recipients = match crate::targeting::effect_recipients(effect) {
        EffectRecipients::Declared(crate::card::TargetSpec::Controller) => Some(vec![Target::Player(controller)]),
        EffectRecipients::Declared(crate::card::TargetSpec::EachCreature) => Some(state.battlefield.iter().copied()
            .filter(|&id| state.is_creature(id)).map(Target::Object).collect()),
        EffectRecipients::Declared(crate::card::TargetSpec::NoTarget) | EffectRecipients::Independent => Some(Vec::new()),
        _ => None,
    };
    let actual_targets = recipients.as_deref().unwrap_or(targets);
    let actual_generations = recipients.as_ref().map(|t| crate::targeting::target_generations(state, t));
    let actual_generations = actual_generations.as_deref().unwrap_or(generations);
    let mut requests = Vec::new();
    let mut placement = LibraryPlacement::Bottom;
    let mut selected: Vec<(ExactObjectRef, ZoneType, ZoneType, MovementKind)> = Vec::new();
    let target_subjects = |from, to, kind| -> Result<Vec<_>, TransitionError> {
        actual_targets.iter().enumerate().filter_map(|(i, target)| match target {
            Target::Object(id) => Some(actual_generations.get(i).copied().flatten()
                .map(|generation| (ExactObjectRef { id: *id, generation }, from, to, kind))
                .ok_or(TransitionError::StaleIncarnation(*id))),
            _ => None,
        }).collect()
    };
    match effect {
        Effect::ChitteringRats => { placement = LibraryPlacement::Top; selected = target_subjects(ZoneType::Hand, ZoneType::Library, MovementKind::Put)?; },
        Effect::BounceTo { zone, .. } => selected = target_subjects(ZoneType::Battlefield, *zone, MovementKind::Put)?,
        Effect::ReturnLinkedExileToHand => {
            let Some(source) = source else { return Err(TransitionError::UnsupportedPath); };
            selected = target_subjects(ZoneType::Exile, ZoneType::Hand, MovementKind::Put)?;
            selected.retain(|(object, ..)| state.objects.get(&object.id).is_some_and(|inst| inst.exiled_by == Some(source) && !inst.is_token));
        },
        Effect::DestroyTarget { .. } => selected = target_subjects(ZoneType::Battlefield, ZoneType::Graveyard, MovementKind::Destroy)?,
        Effect::ShuffleIntoLibrary { .. } => {
            placement = LibraryPlacement::Shuffle;
            selected = target_subjects(ZoneType::Battlefield, ZoneType::Library, MovementKind::Put)?;
        }
        Effect::PutOnBottomOfLibrary { .. } => selected = target_subjects(ZoneType::Battlefield, ZoneType::Library, MovementKind::Put)?,
        Effect::ReturnToTopOfLibrary { .. } => {
            placement = LibraryPlacement::Top;
            selected = target_subjects(ZoneType::Graveyard, ZoneType::Library, MovementKind::Put)?;
        }
        Effect::ReturnFromGraveyardToHand { .. } => selected = target_subjects(ZoneType::Graveyard, ZoneType::Hand, MovementKind::Put)?,
        Effect::DestroyAll | Effect::BounceAllNonlandOpponents => {
            for &id in &state.battlefield {
                let relevant = if matches!(effect, Effect::DestroyAll) { state.is_creature(id) }
                    else { state.get_characteristics(id).is_some_and(|c| c.controller != controller
                        && !c.card_types.contains(&crate::card::CardType::Land)) };
                if relevant { selected.push((state.exact_object(id).unwrap(), ZoneType::Battlefield,
                    if matches!(effect, Effect::DestroyAll) { ZoneType::Graveyard } else { ZoneType::Hand },
                    if matches!(effect, Effect::DestroyAll) { MovementKind::Destroy } else { MovementKind::Put })); }
            }
        }
        Effect::SacrificeCreatures { count, .. } | Effect::EachOpponentSacrifices { count } => {
            let mut players: Vec<_> = if matches!(effect, Effect::EachOpponentSacrifices { .. }) {
                state.opponents(controller)
            } else { actual_targets.iter().filter_map(|t| match t { Target::Player(p) => Some(*p), _ => None }).collect() };
            players.sort_unstable(); players.dedup();
            for player in players {
                if player >= state.players.len() { return Err(TransitionError::InvalidPlayer(0)); }
                let mut creatures: Vec<_> = state.battlefield.iter().copied().filter(|&id|
                    state.is_creature(id) && state.get_characteristics(id).is_some_and(|c| c.controller == player)).collect();
                creatures.sort_by_key(|&id| state.effective_power(id));
                for id in creatures.into_iter().take(*count as usize) { selected.push((state.exact_object(id).unwrap(),
                    ZoneType::Battlefield, ZoneType::Graveyard, MovementKind::Sacrifice { player })); }
            }
        }
        Effect::DrawCards { .. } => {
            let index = state.first_library_card(controller).ok_or(TransitionError::UnsupportedPath)?;
            let id = state.players[controller].library[index];
            selected.push((state.exact_object(id).ok_or(TransitionError::MissingObject(id))?,
                ZoneType::Library, ZoneType::Hand, MovementKind::Draw { player: controller }));
        }
        _ => return Err(TransitionError::UnsupportedPath),
    }
    let mut shuffle_owners: Vec<_> = if placement == LibraryPlacement::Shuffle {
        selected.iter().filter_map(|(object, ..)| state.objects.get(&object.id).map(|i| i.owner)).collect()
    } else { Vec::new() };
    shuffle_owners.sort_unstable(); shuffle_owners.dedup();
    let mut seen = std::collections::HashSet::new();
    for (object, from, to, kind) in selected {
        if !seen.insert(object.id) { return Err(TransitionError::DuplicateSubject(object.id)); }
        let inst = state.objects.get(&object.id).ok_or(TransitionError::MissingObject(object.id))?;
        if state.card_db.as_ref().is_some_and(|db| db.get(inst.card_def_id).is_none()) {
            return Err(TransitionError::MissingDefinition(inst.card_def_id));
        }
        if state.exact_object(object.id) != Some(object) {
            if allow_stale && state.exact_object(object.id).is_none_or(|current| current.generation > object.generation) { continue; }
            return Err(TransitionError::StaleIncarnation(object.id));
        }
        if !resident(state, object, from) { return Err(TransitionError::WrongZone(object.id)); }
        if inst.owner >= state.players.len() || inst.controller >= state.players.len() { return Err(TransitionError::InvalidPlayer(object.id)); }
        if from == to || matches!(to, ZoneType::Battlefield | ZoneType::Stack) { return Err(TransitionError::UnsupportedPath); }
        if inst.is_token && from != ZoneType::Battlefield { continue; }
        if kind == MovementKind::Destroy && state.has_keyword(object.id, KeywordAbility::Indestructible) { continue; }
        requests.push(MovementProposal { request: TransitionRequest { object, from, to, kind },
            owner: inst.owner, affected_controller: if from == ZoneType::Battlefield { state.get_characteristics(object.id).map(|c| c.controller).unwrap_or(inst.controller) } else { inst.controller },
            commander: state.commander_for_object(object.id).map(|c| c.designation),
            is_creature: state.is_creature(object.id), is_token: inst.is_token, canceled: false,
            placement, proposed: to, consumed: Vec::new(), declined_revision: None, revision: 0, commander_selected: false });
    }
    let op = PendingMovement { window: state.next_movement_window, version: 1, active_player: state.active_player, instruction_controller: controller,
        resume_priority: state.priority_player, proposals: requests, catalog: catalog(state, contains_death(effect))?, history: Vec::new(),
        library_order: Vec::new(), shuffle_owners, envelope: vec![BoundedMovementInstruction::from_effect(effect).ok_or(TransitionError::UnsupportedPath)?], instruction_index: 0, targets: targets.to_vec(), target_generations: generations.to_vec(),
        source, instruction: BoundedMovementInstruction::from_effect(effect).ok_or(TransitionError::UnsupportedPath)?,
        resolving_entry: None, resolving_source: None, resolution_owned: false, draws_remaining: 0,
        draw_start_library_count: 0, turn_draw: false, draw_resume_deferred: false };
    // Nominal movement capacity and generation are checked before any offer.
    if preflight {
        let requests: Vec<_> = op.proposals.iter().map(|p| p.request).collect();
        if *effect == Effect::ChitteringRats { transitions::preflight_chittering_batch(state, &requests)?; }
        else if matches!(effect, Effect::DrawCards { .. }) { transitions::preflight_draw_batch(state, &requests)?; }
        else { transitions::preflight_movement_batch(state, &requests)?; }
    }
    Ok(op)
}

pub(crate) fn resident(state: &GameState, object: ExactObjectRef, from: ZoneType) -> bool {
    if state.exact_object(object.id) != Some(object) { return false; }
    let Some(inst) = state.objects.get(&object.id) else { return false; };
    let Some(player) = state.players.get(inst.owner) else { return false; };
    match from { ZoneType::Battlefield => state.battlefield.contains(&object.id), ZoneType::Hand => player.hand.contains(&object.id),
        ZoneType::Library => player.library.contains(&object.id), ZoneType::Graveyard => player.graveyard.contains(&object.id),
        ZoneType::Exile => player.exile.contains(&object.id), ZoneType::Command => player.command_zone.contains(&object.id), _ => false }
}

fn apply_decision(op: &mut PendingMovement, object: ExactObjectRef, chooser: usize, decision: MovementDecision,
    players: usize) -> Result<(), &'static str> {
    if op.chooser(players) != Some(chooser) { return Err("invalid movement chooser"); }
    let index = op.proposals.iter().position(|p| p.request.object == object).ok_or("invalid movement subject")?;
    let candidates = op.candidates(&op.proposals[index]);
    let p = &mut op.proposals[index];
    match &decision {
        MovementDecision::Select(candidate) => {
            if PendingMovement::replacement_chooser(p) != chooser || !candidates.contains(candidate) { return Err("invalid replacement selection"); }
            match candidate {
                Candidate::Commander(designation) if Some(*designation) == p.commander => p.commander_selected = true,
                Candidate::Ordinary { action, .. } => {
                    p.consumed.push(candidate.clone());
                    p.proposed = match action { ReplacementAction::RedirectToZone(zone) => *zone,
                        ReplacementAction::Prevent => { p.canceled = true; p.request.from }, _ => return Err("unsupported replacement action") };
                    p.revision += 1;
                }
                _ => return Err("invalid commander candidate"),
            }
        }
        MovementDecision::Commander(answer) => {
            if !p.commander_selected || p.owner != chooser { return Err("invalid commander answer"); }
            p.commander_selected = false;
            if *answer { p.proposed = ZoneType::Command; p.revision += 1; }
            else { p.declined_revision = Some(p.revision); }
        }
        MovementDecision::LibraryNext => {
            if op.proposals.iter().any(|p| p.commander_selected || !op.candidates(p).is_empty()) { return Err("premature library placement"); }
            let p = &op.proposals[index];
            if p.is_token || p.canceled || p.owner != chooser || p.proposed != ZoneType::Library || p.placement == LibraryPlacement::Shuffle
                || op.library_order.contains(&object) { return Err("invalid library placement"); }
            op.library_order.push(object);
        }
    }
    let p = &op.proposals[index];
    op.history.push(MovementDecisionRecord { object, chooser, automatic: false, decision,
        resulting_destination: p.proposed, resulting_revision: p.revision });
    Ok(())
}

pub(crate) fn normalize(op: &mut PendingMovement, players: usize) {
    loop {
        let Some(chooser) = op.chooser(players) else { return; };
        let single = op.proposals.iter().find_map(|p| {
            let candidates = op.candidates(p);
            (!p.commander_selected && PendingMovement::replacement_chooser(p) == chooser && candidates.len() == 1)
                .then(|| (p.request.object, candidates[0].clone()))
        });
        let Some((object, candidate)) = single else { return; };
        if apply_decision(op, object, chooser, MovementDecision::Select(candidate), players).is_err() { return; }
        if let Some(record) = op.history.last_mut() { record.automatic = true; }
    }
}

pub(crate) fn actions(state: &GameState) -> Vec<Action> {
    let Some(op) = &state.pending_movement else { return Vec::new(); };
    let chooser = state.priority_player;
    let mut actions = Vec::new();
    for p in &op.proposals {
        let object = p.request.object;
        if p.commander_selected && p.owner == chooser {
            for to_command in [true, false] { actions.push(Action::ChooseCommanderMovement { window: op.window, stage: op.history.len(), object, to_command }); }
        } else if PendingMovement::replacement_chooser(p) == chooser {
            for candidate in op.candidates(p) { actions.push(Action::ChooseMovementReplacement { window: op.window, stage: op.history.len(), object, candidate }); }
        }
    }
    if actions.is_empty() {
        for p in &op.proposals {
            if !p.is_token && !p.canceled && p.owner == chooser && p.proposed == ZoneType::Library && p.placement != LibraryPlacement::Shuffle
                && !op.library_order.contains(&p.request.object) {
                actions.push(Action::ChooseLibraryPlacement { window: op.window, stage: op.history.len(), object: p.request.object });
            }
        }
    }
    actions
}

pub(crate) fn choose(state: &mut GameState, action: &Action) -> bool {
    let Some(current) = &state.pending_movement else { return false; };
    if !actions(state).contains(action) { return false; }
    let (object, decision) = match action {
        Action::ChooseMovementReplacement { object, candidate, .. } => (*object, MovementDecision::Select(candidate.clone())),
        Action::ChooseCommanderMovement { object, to_command, .. } => (*object, MovementDecision::Commander(*to_command)),
        Action::ChooseLibraryPlacement { object, .. } => (*object, MovementDecision::LibraryNext), _ => return false,
    };
    let mut next = current.clone();
    if apply_decision(&mut next, object, state.priority_player, decision, state.players.len()).is_err() { return false; }
    normalize(&mut next, state.players.len());
    if let Some(chooser) = next.chooser(state.players.len()) {
        state.pending_movement = Some(next); state.priority_player = chooser; return true;
    }
    // Final proposal and coupled library work must be fully prepared before
    // replacing the owned pending operation or its valid history.
    if commit(state, &next).is_err() { return false; }
    state.pending_movement = None;
    state.priority_player = next.resume_priority;
    let draw = matches!(next.instruction, BoundedMovementInstruction::Draw { .. });
    if draw && next.proposals[0].proposed == ZoneType::Hand {
        super::triggers::fire_card_draw_triggers(state, next.instruction_controller);
    }
    if draw {
        execute_draws(state, next.draws_remaining, &next.envelope, next.instruction_index,
            next.instruction_controller, &next.targets, &next.target_generations, next.source, next.turn_draw);
    } else {
        execute(state, next.envelope.clone(), next.instruction_controller, &next.targets, &next.target_generations, next.source, true, next.instruction_index + 1);
    }
    if let Some(pending) = state.pending_movement.as_mut() {
        pending.resolving_entry = next.resolving_entry;
        pending.resolving_source = next.resolving_source;
        pending.resolution_owned = next.resolution_owned;
        pending.draw_resume_deferred = next.draw_resume_deferred;
    }
    else if state.gameplay_stopped() {
        if let Some(entry) = next.resolving_entry { super::resolution::retain_failed_movement_entry(state, *entry); }
    } else {
        if let Some(entry) = next.resolving_entry { super::resolution::finish_deferred_resolution(state, &entry); super::complete_stack_resolution(state); }
        else if next.turn_draw && draw { super::phases::finish_turn_draw(state, next.draw_resume_deferred); }
        else if draw {
            state.trigger_placement_deferred = next.draw_resume_deferred;
            super::sba::check_state_based_actions(state);
            if !state.gameplay_stopped() && state.pending_commander_sba.is_none() {
                let _ = super::triggers::flush_triggers(state);
            }
        }
    }
    true
}

pub(crate) fn commit(state: &mut GameState, op: &PendingMovement) -> Result<(), TransitionError> {
    commit_with_rng(state, op, &mut rand::thread_rng())
}
fn commit_with_rng<R: rand::Rng + ?Sized>(state: &mut GameState, op: &PendingMovement, rng: &mut R) -> Result<(), TransitionError> {
    let requests: Vec<_> = op.proposals.iter().filter(|p| p.proposed != p.request.from).map(|p|
        TransitionRequest { to: p.proposed, ..p.request }).collect();
    // The preflight kernel prepares refresh, linked departures, events and all
    // generations. Empty/no-op results have no group or arrival occurrence.
    if let Some(source) = op.resolving_source {
        if state.exact_object(source.id) != Some(source) { return Err(TransitionError::StaleIncarnation(source.id)); }
        if source.generation == u32::MAX { return Err(TransitionError::GenerationExhausted(source.id)); }
    }
    let library_completion = prepare_library_completion(op)?;
    if !requests.is_empty() {
        if super::chittering::private_operation(op) {
            super::chittering::validate_child(state, op).map_err(|_| TransitionError::UnsupportedPath)?;
            transitions::chittering_batch(state, &requests, op)?;
        } else if matches!(op.instruction, BoundedMovementInstruction::Draw { .. }) {
            transitions::draw_batch(state, &requests)?;
        } else { transitions::movement_batch(state, &requests)?; }
    }
    if matches!(op.instruction, BoundedMovementInstruction::LinkedHand) {
        for p in &op.proposals { if !p.canceled && p.proposed != p.request.from {
            if let Some(inst) = state.objects.get_mut(&p.request.object.id) { inst.exiled_by = None; }
        } }
    }
    use rand::seq::SliceRandom;
    for (owner, placement, ids) in library_completion {
        let library = &mut state.players[owner].library;
        let ids: Vec<_> = ids.into_iter().map(|id| {
            let index = library.iter().position(|resident| *resident == id).expect("prepared library resident");
            library.remove(index)
        }).collect();
        if placement == LibraryPlacement::Top { library.splice(0..0, ids); } else { library.extend(ids); }
    }
    for &owner in &op.shuffle_owners { state.players[owner].library.shuffle(rng); }
    Ok(())
}

/// Transient coupled placement plan. Owns only final identities and ordering;
/// it is fully checked before the transition kernel mutates any zone.
fn prepare_library_completion(op: &PendingMovement) -> Result<Vec<(usize, LibraryPlacement, Vec<ObjectId>)>, TransitionError> {
    let mut groups = Vec::new();
    let mut owners: Vec<_> = op.proposals.iter().filter(|p| !p.canceled && p.proposed == ZoneType::Library).map(|p| p.owner).collect();
    owners.sort_unstable(); owners.dedup();
    for owner in owners {
        for placement in [LibraryPlacement::Top, LibraryPlacement::Bottom] {
            let mut movers: Vec<_> = op.proposals.iter().filter(|p| !p.canceled && p.owner == owner
                && p.proposed == ZoneType::Library && p.placement == placement).collect();
            let cards: Vec<_> = movers.iter().filter(|p| !p.is_token).collect();
            if cards.len() > 1 && cards.iter().any(|p| !op.library_order.contains(&p.request.object)) {
                return Err(TransitionError::UnsupportedPath);
            }
            movers.sort_by_key(|p| op.library_order.iter().position(|object| *object == p.request.object).unwrap_or(usize::MAX));
            if !movers.is_empty() { groups.push((owner, placement, movers.into_iter().map(|p| p.request.object.id).collect())); }
        }
    }
    Ok(groups)
}

pub(crate) fn validate_operation(state: &GameState, op: &PendingMovement) -> Result<(), &'static str> {
    if super::chittering::private_operation(op) { super::chittering::validate_child(state, op)?; }
    let drawing = matches!(op.instruction, BoundedMovementInstruction::Draw { .. });
    if drawing {
        let BoundedMovementInstruction::Draw { count } = &op.instruction else { unreachable!() };
        if !state.trigger_placement_deferred || op.instruction_controller >= state.players.len()
            || *count == 0 || op.draws_remaining >= *count || op.proposals.len() != 1
            || Some(op.draw_start_library_count) != state.card_count(&state.players[op.instruction_controller].library)
                .checked_add((*count - op.draws_remaining - 1) as usize)
            || !op.catalog.is_empty() || !op.shuffle_owners.is_empty() || !op.library_order.is_empty()
            || op.proposals[0].owner != op.instruction_controller
            || op.proposals[0].commander.is_none()
            || op.proposals[0].request.object.id != state.first_library_card(op.instruction_controller)
                .and_then(|i| state.players[op.instruction_controller].library.get(i).copied()).unwrap_or(0)
            || op.turn_draw && (op.resolution_owned || op.resolving_entry.is_some()
                || op.source.is_some() || !op.targets.is_empty() || !op.target_generations.is_empty()
                || op.instruction_controller != state.active_player || state.phase != crate::game::Phase::Draw
                || op.envelope.as_slice() != [BoundedMovementInstruction::Draw { count: 1 }]
                || op.draws_remaining != 0)
            || op.resolution_owned && op.draw_resume_deferred
            || !op.turn_draw && op.resolving_entry.is_none() &&
                (op.envelope.as_slice() != [op.instruction.clone()] || op.source.is_some()
                    || !op.targets.is_empty() || !op.target_generations.is_empty()) {
            return Err("invalid pending draw continuation");
        }
    } else if op.draws_remaining != 0 || op.draw_start_library_count != 0 || op.turn_draw || op.draw_resume_deferred {
        return Err("draw continuation on movement operation");
    }
    if op.window >= state.next_movement_window || op.version != 1 || op.active_player != state.active_player || op.active_player >= state.players.len()
        || op.shuffle_owners.iter().any(|owner| *owner >= state.players.len())
        || op.shuffle_owners.windows(2).any(|pair| pair[0] >= pair[1])
        || !matches!(op.instruction, BoundedMovementInstruction::Shuffle { .. }) && !op.shuffle_owners.is_empty()
        || op.instruction_controller >= state.players.len() || op.resume_priority >= state.players.len()
        || state.pending_copy_order.is_some() || state.pending_commander_sba.is_some() || state.trigger_placement.is_some()
        || op.chooser(state.players.len()) != Some(state.priority_player)
        || op.targets.len() != op.target_generations.len()
        || op.resolution_owned != op.resolving_entry.is_some()
        || matches!(state.trigger_order_resume, Some(crate::game::TriggerOrderResume::AfterResolution)) && !op.resolution_owned
        || op.resolving_entry.as_ref().is_some_and(|entry| entry.id >= state.next_stack_id || state.stack.iter().any(|other| other.id == entry.id)
            || entry.controller != op.instruction_controller
            || (!super::chittering::private_operation(op) && (entry.targets != op.targets || entry.target_generations != op.target_generations))
            || entry.source.physical_object() != op.resolving_source.map(|s| s.id))
        || op.resolving_source.is_some_and(|source| state.exact_object(source.id) != Some(source))
        || op.envelope.get(op.instruction_index) != Some(&op.instruction)
        || op.envelope.iter().any(|e| !movement_leaf(&e.effect()) && !synchronous(&e.effect())
            && !matches!(e, BoundedMovementInstruction::Draw { .. } | BoundedMovementInstruction::ExpediteHaste))
        || {
            let envelope_effect = if op.envelope.len() == 1 { op.envelope[0].effect() }
                else { Effect::Multiple(op.envelope.iter().map(|e| e.effect()).collect()) };
            contains_draw(&envelope_effect) && !draw_envelope(&envelope_effect)
        } { return Err("invalid movement operation structure"); }
    if state.card_db.is_some() {
        if let Some(entry) = &op.resolving_entry {
            let effect = match &entry.source {
                crate::game::StackSource::Spell(id) => state.objects.get(id).and_then(|i| state.card_db().get(i.card_def_id)).and_then(|d| d.spell_effect.as_ref()),
                crate::game::StackSource::SpellCopy { definition } => definition.spell_effect.as_ref(),
                crate::game::StackSource::ActivatedAbility { source_id, ability_index } => state.objects.get(source_id).and_then(|i| state.card_db().get(i.card_def_id)).and_then(|d| d.activated_abilities.get(*ability_index).map(|a| &a.effect).or_else(|| d.loyalty_abilities.get(*ability_index).map(|a| &a.effect))),
                crate::game::StackSource::ReclamationAbility { .. } => Some(&super::reclamation::EFFECT),
                crate::game::StackSource::TriggeredAbility { context, .. } => Some(&context.effect),
                _ => None,
            }.ok_or("missing resolving movement envelope")?;
            let leaves = match effect { Effect::Multiple(children) => children.as_slice(), leaf => std::slice::from_ref(leaf) };
            let envelope: Option<Vec<_>> = leaves.iter().map(BoundedMovementInstruction::from_effect).collect();
            if envelope.as_ref() != Some(&op.envelope) { return Err("invalid owned movement envelope"); }
        }
    }
    let expected = match &op.instruction {
        BoundedMovementInstruction::ChitteringHandTop => (ZoneType::Hand, ZoneType::Library, 0u8),
        BoundedMovementInstruction::Bounce { zone, .. } => (ZoneType::Battlefield, *zone, 0u8),
        BoundedMovementInstruction::BounceOpponents => (ZoneType::Battlefield, ZoneType::Hand, 0),
        BoundedMovementInstruction::Shuffle { .. } | BoundedMovementInstruction::Bottom { .. } => (ZoneType::Battlefield, ZoneType::Library, 0),
        BoundedMovementInstruction::GraveTop { .. } => (ZoneType::Graveyard, ZoneType::Library, 0),
        BoundedMovementInstruction::GraveHand { .. } => (ZoneType::Graveyard, ZoneType::Hand, 0),
        BoundedMovementInstruction::LinkedHand => (ZoneType::Exile, ZoneType::Hand, 0),
        BoundedMovementInstruction::Destroy { .. } | BoundedMovementInstruction::DestroyAll => (ZoneType::Battlefield, ZoneType::Graveyard, 1),
        BoundedMovementInstruction::Sacrifice { .. } | BoundedMovementInstruction::OpponentSacrifice { .. } => (ZoneType::Battlefield, ZoneType::Graveyard, 2),
        BoundedMovementInstruction::Draw { .. } => (ZoneType::Library, ZoneType::Hand, 3),
        _ => return Err("invalid reached movement instruction"),
    };
    let mut seen = std::collections::HashSet::new();
    for p in &op.proposals {
        if p.request.from != expected.0 || p.request.to != expected.1
            || !matches!((expected.2, p.request.kind), (0, MovementKind::Put) | (1, MovementKind::Destroy) | (2, MovementKind::Sacrifice { .. }))
                && !(expected.2 == 3 && p.request.kind == MovementKind::Draw { player: op.instruction_controller })
            || !seen.insert(p.request.object.id) || !resident(state, p.request.object, p.request.from)
            || state.objects[&p.request.object.id].owner != p.owner
            || state.objects[&p.request.object.id].is_token != p.is_token
            || p.owner >= state.players.len() || p.affected_controller >= state.players.len()
            || matches!(p.request.kind, MovementKind::Sacrifice { player } if player >= state.players.len())
            || state.commander_for_object(p.request.object.id).map(|c| c.designation) != p.commander
            || p.request.from == p.request.to || matches!(p.request.to, ZoneType::Battlefield | ZoneType::Stack)
            || p.revision > op.catalog.len() * 2 + 1 { return Err("invalid movement proposal"); }
    }
    let current_catalog = catalog(state, contains_death(&op.instruction.effect())).map_err(|_| "invalid movement catalog")?;
    if current_catalog.len() != op.catalog.len() || current_catalog.iter().any(|c| !op.catalog.contains(c)) {
        return Err("changed movement catalog");
    }
    let expected_placement = match op.instruction { BoundedMovementInstruction::Shuffle { .. } => LibraryPlacement::Shuffle,
        BoundedMovementInstruction::GraveTop { .. } | BoundedMovementInstruction::ChitteringHandTop => LibraryPlacement::Top, _ => LibraryPlacement::Bottom };
    if op.proposals.iter().any(|p| p.placement != expected_placement) { return Err("invalid instruction placement"); }
    if state.card_db.is_some() {
        let initial = prepare_operation(state, &op.instruction.effect(), op.instruction_controller,
            &op.targets, &op.target_generations, op.source, false, true).map_err(|_| "invalid reached movement subjects")?;
        let initial_facts = |p: &MovementProposal| (p.request, p.owner, p.affected_controller, p.commander, p.is_creature, p.is_token, p.placement);
        let mut expected: Vec<_> = initial.proposals.iter().map(initial_facts).collect();
        let mut actual: Vec<_> = op.proposals.iter().map(initial_facts).collect();
        expected.sort_by_key(|p| p.0.object.id); actual.sort_by_key(|p| p.0.object.id);
        if expected != actual || initial.shuffle_owners != op.shuffle_owners { return Err("inconsistent complete reached movement subjects"); }
    }
    let mut replay = op.clone(); replay.history.clear(); replay.library_order.clear();
    for p in &mut replay.proposals { p.canceled = false; p.proposed = p.request.to; p.consumed.clear(); p.declined_revision = None;
        p.revision = 0; p.commander_selected = false; }
    for record in &op.history {
        if record.automatic {
            let p = replay.proposals.iter().find(|p| p.request.object == record.object).ok_or("invalid automatic movement")?;
            let candidates = replay.candidates(p);
            if candidates.len() != 1 || !matches!(&record.decision, MovementDecision::Select(c) if c == &candidates[0]) {
                return Err("invalid automatic replacement");
            }
        }
        apply_decision(&mut replay, record.object, record.chooser, record.decision.clone(), state.players.len())?;
        replay.history.last_mut().ok_or("missing movement history")?.automatic = record.automatic;
        if replay.history.last() != Some(record) { return Err("invalid movement history result"); }
    }
    if replay.proposals != op.proposals || replay.library_order != op.library_order { return Err("inconsistent movement history"); }
    Ok(())
}

/// Excluded cost/SBA/legacy handoffs must fail at their own preflight boundary.
pub(crate) fn requires_choice(state: &GameState, request: &TransitionRequest) -> bool {
    state.is_commander(request.object.id) && matches!(request.to, ZoneType::Hand | ZoneType::Library)
        || state.replacement_effects.iter().any(|e| match e.applies_to {
            ReplacementEventKind::WouldMoveTo { destination, .. } => destination.zone() == request.to,
            ReplacementEventKind::WouldDie => !e.is_self_replacement && request.to == ZoneType::Graveyard
                && state.is_creature(request.object.id),
            _ => false,
        })
}
fn preflight_companion(state: &GameState, effect: &Effect, controller: usize, targets: &[Target]) -> Result<(), TransitionError> {
    use crate::mana::Color;
    let Some(player) = state.players.get(controller) else { return Err(TransitionError::InvalidPlayer(0)); };
    match effect {
        Effect::GainLife { amount } => { player.life.checked_add(i32::try_from(*amount).map_err(|_| TransitionError::CapacityExhausted)?).ok_or(TransitionError::CapacityExhausted)?; }
        Effect::LoseLife { amount, target } => {
            let amount = i32::try_from(*amount).map_err(|_| TransitionError::CapacityExhausted)?;
            let players: Vec<_> = if *target == crate::card::TargetSpec::Controller { vec![controller] }
                else { targets.iter().filter_map(|t| match t { Target::Player(p) => Some(*p), _ => None }).collect() };
            let mut projected = std::collections::HashMap::new();
            for p in players {
                let life = projected.entry(p).or_insert(state.players.get(p).ok_or(TransitionError::InvalidPlayer(0))?.life);
                *life = life.checked_sub(amount).ok_or(TransitionError::CapacityExhausted)?;
            }
        }
        Effect::AddMana { color, amount } => {
            let pool = &player.mana_pool;
            let occupied = match color { Some(Color::White) => pool.white, Some(Color::Blue) => pool.blue,
                Some(Color::Black) => pool.black, Some(Color::Red) => pool.red, Some(Color::Green) => pool.green, None => pool.colorless };
            occupied.checked_add(*amount).ok_or(TransitionError::CapacityExhausted)?;
            let total = [pool.white, pool.blue, pool.black, pool.red, pool.green, pool.colorless].iter()
                .try_fold(0u32, |sum, &part| sum.checked_add(part)).ok_or(TransitionError::CapacityExhausted)?;
            total.checked_add(*amount).ok_or(TransitionError::CapacityExhausted)?;
        }
        _ => {},
    }
    Ok(())
}

#[cfg(test)]
mod bounded_shuffle_tests {
    use super::*;
    use crate::{card::{CardDef, CardType, TargetSpec}, game::CardDatabase};
    use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
    #[test]
    fn all_redirected_movers_still_shuffle_the_original_owners_library() {
        let mut state = GameState::new_commander(2);
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 998_399, card_types: vec![CardType::Creature], power: Some(1), toughness: Some(1), ..Default::default() });
        state.card_db = Some(std::sync::Arc::new(db));
        let commander = state.create_card_in_zone(998_399, 0, ZoneType::Battlefield).unwrap();
        state.designate_commander(commander, 0).unwrap();
        let cards: Vec<_> = (0..8).map(|_| state.create_card_in_zone(998_399, 0, ZoneType::Library).unwrap()).collect();
        let mut operation = prepare_operation(&state, &Effect::ShuffleIntoLibrary { target: TargetSpec::AnyPermanent }, 1,
            &[Target::Object(commander)], &[Some(0)], None, true, false).unwrap();
        normalize(&mut operation, 2);
        apply_decision(&mut operation, state.exact_object(commander).unwrap(), 0, MovementDecision::Commander(true), 2).unwrap();
        assert_eq!(operation.proposals[0].proposed, ZoneType::Command);
        assert_eq!(operation.proposals[0].placement, LibraryPlacement::Shuffle);
        let mut expected = cards.clone(); expected.shuffle(&mut StdRng::seed_from_u64(0x9039_b));
        assert_ne!(expected, cards, "fixed seed is a nonidentity shuffle control");
        commit_with_rng(&mut state, &operation, &mut StdRng::seed_from_u64(0x9039_b)).unwrap();
        assert!(state.players[0].command_zone.contains(&commander));
        assert_eq!(state.players[0].library, expected);
        assert_eq!(state.objects[&commander].zone_change_count, 1);
    }

    #[test]
    fn now_impossible_named_mover_still_carries_shuffle_obligation() {
        let mut state = GameState::new_commander(2);
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 998_399, card_types: vec![CardType::Creature], power: Some(1), toughness: Some(1), ..Default::default() });
        state.card_db = Some(std::sync::Arc::new(db));
        let target = state.create_card_in_zone(998_399, 0, ZoneType::Battlefield).unwrap();
        let old_generation = state.exact_object(target).unwrap().generation;
        state.move_object(target, ZoneType::Battlefield, ZoneType::Hand);
        let cards: Vec<_> = (0..8).map(|_| state.create_card_in_zone(998_399, 0, ZoneType::Library).unwrap()).collect();
        let operation = prepare_operation(&state, &Effect::ShuffleIntoLibrary { target: TargetSpec::AnyPermanent }, 1,
            &[Target::Object(target)], &[Some(old_generation)], None, true, true).unwrap();
        assert!(operation.proposals.is_empty()); assert_eq!(operation.shuffle_owners, vec![0]);
        let mut expected = cards.clone(); expected.shuffle(&mut StdRng::seed_from_u64(0x9039_b));
        let group = state.next_zone_event_group_id;
        commit_with_rng(&mut state, &operation, &mut StdRng::seed_from_u64(0x9039_b)).unwrap();
        assert_eq!(state.players[0].library, expected); assert!(state.players[0].hand.contains(&target));
        assert_eq!(state.next_zone_event_group_id, group);
    }

    #[test]
    fn direct_draw_restores_its_caller_trigger_deferral() {
        let mut state = GameState::new_commander(2);
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 998_399, card_types: vec![CardType::Creature],
            power: Some(1), toughness: Some(1), ..Default::default() });
        state.card_db = Some(std::sync::Arc::new(db));
        let commander = state.create_card_in_zone(998_399, 0, ZoneType::Library).unwrap();
        state.designate_commander(commander, 0).unwrap();
        state.trigger_placement_deferred = true;
        begin_direct_draw(&mut state, 0, 1, false);
        assert!(state.pending_movement.as_ref().is_some_and(|op| op.draw_resume_deferred));
        let answer = actions(&state).into_iter().find(|action| matches!(action,
            Action::ChooseCommanderMovement { to_command: false, .. })).unwrap();
        assert!(choose(&mut state, &answer));
        assert!(state.players[0].hand.contains(&commander));
        assert!(state.trigger_placement_deferred);
    }
}
