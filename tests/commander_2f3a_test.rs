use std::sync::Arc;
use mtg_gto::{action::Action, card::{CardDef, CardType, Effect, TargetSpec, ZoneType},
    game::{CardDatabase, GameState, Phase, StackEntry, StackSource, Target}, rules};

fn fixture(effect: Effect) -> (GameState, u64) { fixture_for(effect, 3) }
fn fixture_for(effect: Effect, players: usize) -> (GameState, u64) {
    let mut db = CardDatabase::new();
    db.insert(CardDef { id: 998_301, name: "Commander".into(), card_types: vec![CardType::Creature],
        power: Some(3), toughness: Some(3), ..Default::default() });
    db.insert(CardDef { id: 998_302, name: "Movement".into(), card_types: vec![CardType::Instant],
        spell_effect: Some(effect), ..Default::default() });
    let mut s = GameState::new_commander(players);
    s.card_db = Some(Arc::new(db)); s.phase = Phase::PreCombatMain;
    let subject = s.create_card_in_zone(998_301, 0, ZoneType::Battlefield).unwrap();
    s.designate_commander(subject, 0).unwrap();
    s.objects.get_mut(&subject).unwrap().controller = 1;
    let instruction_controller = 2.min(players - 1);
    let spell = s.create_card_in_zone(998_302, instruction_controller, ZoneType::Stack).unwrap();
    s.stack.push(StackEntry { id: 1, source: StackSource::Spell(spell), controller: instruction_controller,
        targets: vec![Target::Object(subject)], target_generations: vec![Some(0)] });
    s.next_stack_id = 2;
    (s, subject)
}
fn reach(s: &mut GameState) { for _ in 0..s.players.len() { rules::apply_action(s, &Action::PassPriority); } }

#[test]
fn commander_hand_movement_suspends_before_zone_change() {
    let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    reach(&mut s);
    assert!(s.battlefield.contains(&id), "903.9b owner choice must precede actual movement");
    assert_eq!(s.objects[&id].zone_change_count, 0);
    assert_eq!(s.priority_player, 0);
}

#[test]
fn commander_library_movement_suspends_before_zone_change() {
    let (mut s, id) = fixture(Effect::ShuffleIntoLibrary { target: TargetSpec::AnyPermanent });
    reach(&mut s);
    assert!(s.battlefield.contains(&id), "903.9b owner choice must precede library placement");
    assert_eq!(s.objects[&id].zone_change_count, 0);
}

use mtg_gto::{action::{legal_actions, canonical::{canonicalize, resolve}},
    game::MovementReplacementCandidate,
    info_set::InformationSet,
    replacement::{MovementDestination, ReplacementAction, ReplacementEffect, ReplacementEventKind}};

fn register(s: &mut GameState, destination: MovementDestination, action: ReplacementAction) -> u64 {
    let source = s.create_card_in_zone(998_301, 2, ZoneType::Battlefield).unwrap();
    s.replacement_effects.push(ReplacementEffect { source_id: source, controller: 2,
        applies_to: ReplacementEventKind::WouldMoveTo { destination, source_generation: 0 },
        action, is_self_replacement: false, description: "bounded replacement".into() });
    source
}
fn commander_answer(s: &mut GameState, answer: bool) -> Action {
    let a = legal_actions(s).into_iter().find(|a| matches!(a,
        Action::ChooseCommanderMovement { to_command, .. } if *to_command == answer)).unwrap();
    rules::apply_action(s, &a); a
}
fn select_commander(s: &mut GameState) {
    let a = legal_actions(s).into_iter().find(|a| matches!(a, Action::ChooseMovementReplacement {
        candidate: MovementReplacementCandidate::Commander(_), .. })).unwrap();
    rules::apply_action(s, &a);
}
fn variants(s: &GameState) -> Vec<GameState> {
    let mut snap = s.clone(); snap.restore(s.snapshot()).unwrap();
    let mut variants = vec![s.clone(), snap, serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap(),
        bincode::deserialize(&bincode::serialize(s).unwrap()).unwrap()];
    for v in &mut variants { v.card_db = s.card_db.clone(); }
    variants
}
fn information(s: &GameState, viewer: usize) -> InformationSet {
    InformationSet::from_view(&s.visible_state(viewer), s.card_db()).unwrap()
}

#[test]
fn owner_answers_accept_decline_and_persistence_canonical_roundtrips() {
    for effect in [Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent },
        Effect::ShuffleIntoLibrary { target: TargetSpec::AnyPermanent }] {
        let destination = if matches!(effect, Effect::BounceTo { .. }) { ZoneType::Hand } else { ZoneType::Library };
        let (mut s, id) = fixture(effect);
        reach(&mut s);
        assert_eq!(s.priority_player, 0);
        let pending = s.pending_movement.as_ref().unwrap();
        assert!(pending.resolving_entry.is_some());
        assert_eq!(pending.history.len(), 1, "owned automatic Commander selection");
        for mut restored in variants(&s) {
            assert_eq!(information(&restored, 0).hash_value(), information(&s, 0).hash_value());
            for action in legal_actions(&restored) {
                let canonical = canonicalize(&action, &restored).unwrap();
                assert_eq!(resolve(&canonical, &restored, 0).unwrap(), Some(action));
            }
            for accept in [true, false] {
                let mut branch = restored.clone();
                commander_answer(&mut branch, accept);
                assert!(branch.pending_movement.is_none());
                assert_eq!(branch.objects[&id].zone_change_count, 1);
                let expected = if accept { &branch.players[0].command_zone } else if destination == ZoneType::Hand {
                    &branch.players[0].hand } else { &branch.players[0].library };
                assert!(expected.contains(&id));
                assert!(branch.players[2].graveyard.len() == 1, "physical spell completes exactly once");
                assert_eq!(branch.priority_player, branch.active_player);
            }
            restored.drain_events();
        }
    }
}

#[test]
fn commander_decline_reconsiders_and_actual_repeat_application() {
    for accept_first in [false, true] {
        let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
        let source = register(&mut s, if accept_first { MovementDestination::Command } else { MovementDestination::Hand },
            ReplacementAction::RedirectToZone(ZoneType::Library));
        reach(&mut s);
        if !accept_first { assert_eq!(s.priority_player, 1); select_commander(&mut s); }
        assert_eq!(s.priority_player, 0, "ordering chooser cannot answer for owner");
        let old = commander_answer(&mut s, accept_first);
        assert!(s.pending_movement.is_some(), "903.9b must reopen after proposal changes");
        assert_eq!(s.priority_player, 0);
        let p = &s.pending_movement.as_ref().unwrap().proposals[0];
        assert_eq!(p.proposed, ZoneType::Library);
        assert_eq!(p.consumed.len(), 1);
        assert!(matches!(p.consumed[0], MovementReplacementCandidate::Ordinary { source: x, .. } if x.id == source));
        assert!(s.battlefield.contains(&id));
        assert_eq!(s.objects[&id].zone_change_count, 0);
        let before = bincode::serialize(&s).unwrap();
        rules::apply_action(&mut s, &old);
        assert_eq!(before, bincode::serialize(&s).unwrap(), "replayed stage cannot answer new occurrence");
        for mut restored in variants(&s) {
            commander_answer(&mut restored, true);
            assert!(restored.players[0].command_zone.contains(&id));
            assert_eq!(restored.objects[&id].zone_change_count, 1);
        }
    }
}

#[test]
fn complete_catalog_new_applicability_order_and_ordinary_consumption() {
    let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    register(&mut s, MovementDestination::Hand, ReplacementAction::RedirectToZone(ZoneType::Library));
    register(&mut s, MovementDestination::Library, ReplacementAction::RedirectToZone(ZoneType::Hand));
    reach(&mut s);
    assert_eq!(s.pending_movement.as_ref().unwrap().catalog.len(), 2);
    select_commander(&mut s);
    commander_answer(&mut s, false);
    assert_eq!(s.priority_player, 1);
    assert_eq!(s.pending_movement.as_ref().unwrap().proposals[0].proposed, ZoneType::Library);
    // Library's newly applicable ordinary competes with Commander.
    let newly = legal_actions(&s).into_iter().find(|a| matches!(a, Action::ChooseMovementReplacement {
        candidate: MovementReplacementCandidate::Ordinary { predicate: ReplacementEventKind::WouldMoveTo { destination: MovementDestination::Library, .. }, .. }, .. })).unwrap();
    rules::apply_action(&mut s, &newly);
    assert_eq!(s.priority_player, 0);
    assert_eq!(s.pending_movement.as_ref().unwrap().proposals[0].consumed.len(), 2);
    commander_answer(&mut s, false);
    assert!(s.players[0].hand.contains(&id));
    assert_eq!(s.objects[&id].zone_change_count, 1);
}

#[test]
fn simultaneous_apnap_all_decisions_before_any_commit() {
    for players in [3, 4] {
        let (mut s, first) = fixture_for(Effect::BounceAllNonlandOpponents, players);
        s.active_player = 1;
        let mut ids = vec![first];
        for owner in 1..players {
            let id = s.create_card_in_zone(998_301, owner, ZoneType::Battlefield).unwrap();
            s.designate_commander(id, 0).unwrap();
            s.objects.get_mut(&id).unwrap().controller = 0;
            ids.push(id);
        }
        s.stack.last_mut().unwrap().targets.clear(); s.stack.last_mut().unwrap().target_generations.clear();
        for _ in 0..players { rules::apply_action(&mut s, &Action::PassPriority); }
        let order: Vec<_> = (0..players).map(|rank| (rank + 1) % players).collect();
        for (step, owner) in order.into_iter().enumerate() {
            assert_eq!(s.priority_player, owner);
            assert!(ids.iter().all(|id| s.battlefield.contains(id)));
            assert!(ids.iter().all(|id| s.objects[id].zone_change_count == 0));
            commander_answer(&mut s, step % 2 == 0);
        }
        assert!(s.pending_movement.is_none());
        assert!(ids.iter().all(|id| s.objects[id].zone_change_count == 1));
    }
}

#[test]
fn invalid_choice_capacity_and_malformed_history_preserve_pending() {
    let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    reach(&mut s);
    let action = legal_actions(&s)[0].clone();
    let before = bincode::serialize(&s).unwrap();
    rules::apply_action(&mut s, &Action::PassPriority);
    assert_eq!(before, bincode::serialize(&s).unwrap());
    let mut exhausted = s.clone(); exhausted.next_zone_event_group_id = u64::MAX;
    let before = bincode::serialize(&exhausted).unwrap();
    rules::apply_action(&mut exhausted, &action);
    assert_eq!(before, bincode::serialize(&exhausted).unwrap());
    assert!(exhausted.battlefield.contains(&id));
    for fault in 0..4 {
        let mut bad = s.clone();
        let p = bad.pending_movement.as_mut().unwrap();
        match fault { 0 => p.proposals[0].request.object.generation = 1,
            1 => p.history[0].chooser = 2, 2 => p.history[0].resulting_revision = 3,
            _ => p.resolving_entry = None }
        assert!(bad.validate_movement_choices().is_err());
        assert!(legal_actions(&bad).is_empty());
        assert!(serde_json::from_slice::<GameState>(&serde_json::to_vec(&bad).unwrap()).is_err());
        assert!(bincode::deserialize::<GameState>(&bincode::serialize(&bad).unwrap()).is_err());
        let snap = bad.snapshot(); assert!(bad.restore(snap).is_err());
    }
}

#[test]
fn unsupported_shape_rejects_before_prefix_and_stack_pop() {
    for unsupported in [Effect::DrawCards { count: 1 }, Effect::Multiple(vec![Effect::GainLife { amount: 3 }])] {
        let effect = Effect::Multiple(vec![Effect::GainLife { amount: 5 }, unsupported,
            Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent }]);
        let (mut s, id) = fixture(effect);
        let source = s.stack[0].source.physical_object().unwrap();
        let life = s.players[2].life; let stack = bincode::serialize(&s.stack).unwrap();
        reach(&mut s);
        assert!(s.sba_failure.is_some());
        assert_eq!(s.players[2].life, life);
        assert_eq!(bincode::serialize(&s.stack).unwrap(), stack);
        assert!(s.battlefield.contains(&id));
        assert_eq!(s.objects[&source].zone_change_count, 0);
        assert!(s.pending_movement.is_none());
    }
}

#[test]
fn stale_replacement_source_and_generation_exhaustion_reject_atomically() {
    for stale in [false, true] {
        let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
        if stale {
            let source = register(&mut s, MovementDestination::Command, ReplacementAction::RedirectToZone(ZoneType::Library));
            s.objects.get_mut(&source).unwrap().zone_change_count = 1;
        } else {
            s.objects.get_mut(&id).unwrap().zone_change_count = u32::MAX;
            s.stack[0].target_generations[0] = Some(u32::MAX);
        }
        reach(&mut s);
        assert!(s.sba_failure.is_some());
        assert!(s.battlefield.contains(&id));
        assert!(s.pending_movement.is_none());
        assert!(s.players[0].hand.is_empty());
    }
}

// Independently specified by the Astra read-only reviewer, using the real stack path.
#[test]
fn astra_stack_movement_is_a_mandatory_owner_window_not_priority() {
    for effect in [Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent },
        Effect::ShuffleIntoLibrary { target: TargetSpec::AnyPermanent }] {
        let (mut s, id) = fixture(effect);
        let spell = s.stack.last().unwrap().source.physical_object().unwrap();
        let subject_generation = s.objects[&id].zone_change_count;
        let spell_generation = s.objects[&spell].zone_change_count;
        let group = s.next_zone_event_group_id;
        s.drain_events(); reach(&mut s);
        assert!(!s.gameplay_stopped());
        assert!(s.battlefield.contains(&id), "owner must answer before movement commits");
        assert_eq!(s.objects[&id].zone_change_count, subject_generation);
        assert_eq!(s.objects[&spell].zone_change_count, spell_generation);
        assert!(!s.players[2].graveyard.contains(&spell));
        assert_eq!(s.next_zone_event_group_id, group);
        assert!(s.drain_events().iter().all(|event| !matches!(event,
            mtg_gto::events::GameEvent::ZoneChange { object, .. } if *object == id || *object == spell)));
        assert_eq!(s.priority_player, 0);
        let actions = legal_actions(&s);
        assert_eq!(actions.iter().filter(|a| !matches!(a, Action::Concede)).count(), 2);
        assert!(!actions.contains(&Action::PassPriority));
        let before = bincode::serialize(&s).unwrap();
        rules::apply_action(&mut s, &Action::PassPriority);
        assert_eq!(bincode::serialize(&s).unwrap(), before);
    }
}
fn astra_redirect(s: &mut GameState, destination: MovementDestination, to: ZoneType) {
    register(s, destination, ReplacementAction::RedirectToZone(to));
}
fn astra_answer(s: &mut GameState, answer: bool) -> Action { commander_answer(s, answer) }
#[test]
fn astra_three_roles_and_both_repeated_commander_paths() {
    use mtg_gto::game::{MovementDecision, MovementReplacementCandidate as C};
    for first_accept in [false, true] {
        let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
        astra_redirect(&mut s, MovementDestination::Hand, ZoneType::Library);
        astra_redirect(&mut s, MovementDestination::Command, ZoneType::Library);
        s.drain_events(); reach(&mut s);
        assert_eq!(s.priority_player, 1);
        let select = legal_actions(&s).into_iter().find(|a| matches!(a,
            Action::ChooseMovementReplacement { candidate: C::Commander(_), .. })).unwrap();
        assert!(!legal_actions(&s).iter().any(|a| matches!(a, Action::ChooseCommanderMovement { .. })));
        rules::apply_action(&mut s, &select);
        assert_eq!(s.priority_player, 0);
        let old_answer = astra_answer(&mut s, first_accept);
        assert_eq!(s.priority_player, 0);
        assert_eq!(s.pending_movement.as_ref().unwrap().proposals[0].proposed, ZoneType::Library);
        assert_eq!(s.objects[&id].zone_change_count, 0);
        assert!(s.battlefield.contains(&id));
        assert!(!s.drain_events().iter().any(|e| matches!(e,
            mtg_gto::events::GameEvent::ZoneChange { object, .. } if *object == id)));
        let before = bincode::serialize(&s).unwrap();
        rules::apply_action(&mut s, &old_answer);
        assert_eq!(bincode::serialize(&s).unwrap(), before);
        if !first_accept {
            astra_answer(&mut s, true);
            assert!(s.pending_movement.is_some());
            assert_eq!(s.pending_movement.as_ref().unwrap().proposals[0].proposed, ZoneType::Library);
        }
        let history = &s.pending_movement.as_ref().unwrap().history;
        assert_eq!(history.iter().filter(|h| matches!(h.decision, MovementDecision::Commander(true))).count(), 1);
        let ordinary: Vec<_> = history.iter().filter_map(|h| match &h.decision {
            MovementDecision::Select(c @ C::Ordinary { .. }) => Some(c), _ => None }).collect();
        let unique: std::collections::HashSet<_> = ordinary.iter().copied().collect();
        assert_eq!(ordinary.len(), unique.len());
        astra_answer(&mut s, true);
        assert!(s.pending_movement.is_none());
        assert!(s.players[0].command_zone.contains(&id));
        assert_eq!(s.objects[&id].zone_change_count, 1);
        assert_eq!(s.drain_events().iter().filter(|e| matches!(e,
            mtg_gto::events::GameEvent::ZoneChange { object, .. } if *object == id)).count(), 1);
    }
}

fn astra_private_library_fixture() -> (GameState, [u64; 3]) {
    let (mut s, first) = fixture(Effect::BounceAllNonlandOpponents);
    s.commanders.clear();
    s.stack.last_mut().unwrap().targets.clear(); s.stack.last_mut().unwrap().target_generations.clear();
    for definition in [998_303, 998_304] {
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
            id: definition, name: format!("Private mover {definition}"), card_types: vec![CardType::Creature],
            power: Some(2), toughness: Some(2), ..Default::default() });
    }
    let second = s.create_card_in_zone(998_303, 0, ZoneType::Battlefield).unwrap();
    let third = s.create_card_in_zone(998_304, 0, ZoneType::Battlefield).unwrap();
    s.objects.get_mut(&second).unwrap().controller = 1; s.objects.get_mut(&third).unwrap().controller = 1;
    astra_redirect(&mut s, MovementDestination::Hand, ZoneType::Library);
    reach(&mut s);
    assert_eq!(s.priority_player, 0); assert_eq!(legal_actions(&s).iter().filter(|a| !matches!(a, Action::Concede)).count(), 3);
    (s, [first, second, third])
}
fn astra_place(s: &mut GameState, id: u64) {
    let action = legal_actions(s).into_iter().find(|a| matches!(a,
        Action::ChooseLibraryPlacement { object, .. } if object.id == id)).unwrap();
    rules::apply_action(s, &action);
}
#[test]
fn astra_library_next_history_is_private_until_and_after_commit() {
    let (s, ids) = astra_private_library_fixture();
    let mut left = s.clone(); let mut right = s;
    astra_place(&mut left, ids[0]); astra_place(&mut right, ids[1]);
    for state in [&left, &right] {
        assert!(ids.iter().all(|id| state.battlefield.contains(id)));
        assert!(ids.iter().all(|id| state.objects[id].zone_change_count == 0));
        assert!(state.players[0].library.is_empty());
    }
    assert_ne!(information(&left, 0).hash_value(), information(&right, 0).hash_value());
    for observer in [1, 2] {
        assert_eq!(information(&left, observer).zone_normalization, information(&right, observer).zone_normalization);
        assert_eq!(information(&left, observer).hash_value(), information(&right, observer).hash_value());
    }
    astra_place(&mut left, ids[1]); astra_place(&mut left, ids[2]);
    astra_place(&mut right, ids[0]); astra_place(&mut right, ids[2]);
    assert_eq!(left.players[0].library, ids.to_vec());
    assert_eq!(right.players[0].library, vec![ids[1], ids[0], ids[2]]);
    for observer in [1, 2] { assert_eq!(information(&left, observer).hash_value(), information(&right, observer).hash_value()); }
}

#[test]
fn astra_pending_json_rejects_semantically_inconsistent_owned_state() {
    let (s, _) = astra_private_library_fixture();
    assert!(s.validate_movement_choices().is_ok());
    let original = serde_json::to_value(&s).unwrap();
    for fault in 0..6 {
        let mut value = original.clone();
        match fault {
            0 => value["replacement_effects"][0]["applies_to"]["WouldMoveTo"]["source_generation"] = serde_json::json!(19),
            1 => value["pending_movement"]["resolving_entry"] = serde_json::Value::Null,
            2 => value["pending_movement"]["history"][0]["resulting_revision"] = serde_json::json!(99),
            3 => { let history = value["pending_movement"]["history"].as_array_mut().unwrap(); let repeated = history[0].clone(); history.push(repeated); },
            4 => value["pending_movement"]["proposals"][0]["placement"] = serde_json::json!("Top"),
            _ => value["pending_movement"]["proposals"][0]["affected_controller"] = serde_json::json!(2),
        }
        assert!(serde_json::from_value::<GameState>(value).is_err(), "malformed owned pending state {fault} accepted");
    }
    for fault in 0..3 {
        let mut bad = s.clone();
        let op = bad.pending_movement.as_mut().unwrap();
        match fault { 0 => { op.proposals.pop(); }, 1 => op.proposals[0].is_creature = false,
            _ => op.proposals.push(op.proposals[0].clone()) }
        assert!(bad.validate_movement_choices().is_err());
        assert!(legal_actions(&bad).is_empty());
    }
}

#[test]
fn astra_capacity_failure_keeps_completed_prefix_and_valid_history() {
    let (mut s, id) = fixture(Effect::Multiple(vec![Effect::GainLife { amount: 7 },
        Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent }]));
    reach(&mut s);
    assert_eq!(s.players[2].life, 47); assert!(s.pending_movement.is_some());
    s.next_zone_event_group_id = u64::MAX;
    let action = legal_actions(&s).into_iter().find(|a| matches!(a,
        Action::ChooseCommanderMovement { to_command: false, .. })).unwrap();
    let before = bincode::serialize(&s).unwrap(); s.drain_events();
    rules::apply_action(&mut s, &action);
    assert_eq!(bincode::serialize(&s).unwrap(), before);
    assert_eq!(s.players[2].life, 47); assert!(s.battlefield.contains(&id));
    assert_eq!(s.objects[&id].zone_change_count, 0); assert!(s.drain_events().is_empty());
    s.next_zone_event_group_id = 0;
    rules::apply_action(&mut s, &action);
    assert!(s.pending_movement.is_none()); assert!(s.players[0].hand.contains(&id));
    assert_eq!(s.players[2].life, 47);
}

#[test]
fn six_seven_and_larger_choice_sets_keep_duplicate_occurrences_and_reconstruct() {
    for ordinary_count in [5, 6, 8] {
        let (mut s, _) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
        let source = register(&mut s, MovementDestination::Hand, ReplacementAction::RedirectToZone(ZoneType::Library));
        let base = s.replacement_effects[0].clone();
        for _ in 1..ordinary_count { s.replacement_effects.push(base.clone()); }
        reach(&mut s);
        let actions = legal_actions(&s);
        assert_eq!(actions.iter().filter(|a| !matches!(a, Action::Concede)).count(), ordinary_count + 1);
        let mut canonical = std::collections::HashSet::new();
        for a in &actions {
            let c = canonicalize(a, &s).unwrap();
            assert_eq!(resolve(&c, &s, 1).unwrap(), Some(a.clone())); canonical.insert(c);
        }
        assert_eq!(canonical.len(), actions.len());
        let op = s.pending_movement.as_ref().unwrap();
        assert_eq!(op.catalog.len(), ordinary_count);
        for occurrence in 0..ordinary_count { assert!(op.catalog.iter().any(|c| matches!(c,
            MovementReplacementCandidate::Ordinary { source: x, occurrence: n, .. } if x.id == source && *n == occurrence))); }
        let mut reordered = s.clone(); reordered.replacement_effects.reverse();
        reordered.pending_movement.as_mut().unwrap().catalog.reverse();
        assert_eq!(information(&s, 1).hash_value(), information(&reordered, 1).hash_value());
        let keys: std::collections::HashSet<_> = legal_actions(&reordered).iter().map(|a| canonicalize(a, &reordered).unwrap()).collect();
        assert_eq!(canonical, keys);
    }
}

#[test]
fn allocation_and_registry_order_do_not_change_action_keys_or_information_sets() {
    fn build(offset: u64, reverse: bool) -> GameState {
        let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
        register(&mut s, MovementDestination::Hand, ReplacementAction::RedirectToZone(ZoneType::Library));
        register(&mut s, MovementDestination::Library, ReplacementAction::RedirectToZone(ZoneType::Command));
        if reverse { s.replacement_effects.reverse(); }
        if offset != 0 {
            let mut objects = std::collections::HashMap::new();
            for (_, mut i) in s.objects.drain() { i.object_id += offset; objects.insert(i.object_id, i); }
            s.objects = objects;
            for object in &mut s.battlefield { *object += offset; }
            for c in &mut s.commanders { c.object += offset; }
            for e in &mut s.replacement_effects { e.source_id += offset; }
            if let StackSource::Spell(source) = &mut s.stack[0].source { *source += offset; }
            s.stack[0].targets = vec![Target::Object(id + offset)];
            s.next_object_id += offset;
        }
        reach(&mut s); s
    }
    let baseline = build(0, false);
    for state in [build(150, false), build(900, true)] {
        assert_eq!(information(&baseline, 1).hash_value(), information(&state, 1).hash_value());
        let keys = |s: &GameState| legal_actions(s).iter().map(|a| canonicalize(a, s).unwrap()).collect::<std::collections::HashSet<_>>();
        assert_eq!(keys(&baseline), keys(&state));
        for action in legal_actions(&state) { assert_eq!(resolve(&canonicalize(&action, &state).unwrap(), &state, 1).unwrap(), Some(action)); }
    }
}

#[test]
fn cancellation_is_terminal_and_has_no_transition_or_generation() {
    let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    register(&mut s, MovementDestination::Hand, ReplacementAction::Prevent);
    reach(&mut s); s.drain_events();
    let prevent = legal_actions(&s).into_iter().find(|a| matches!(a,
        Action::ChooseMovementReplacement { candidate: MovementReplacementCandidate::Ordinary { action: ReplacementAction::Prevent, .. }, .. })).unwrap();
    rules::apply_action(&mut s, &prevent);
    assert!(s.pending_movement.is_none()); assert!(s.battlefield.contains(&id));
    assert_eq!(s.objects[&id].zone_change_count, 0);
    assert!(!s.drain_events().iter().any(|event| matches!(event,
        mtg_gto::events::GameEvent::ZoneChange { object, .. } if *object == id)));
}

#[test]
fn flat_multiple_reaches_current_subjects_and_does_not_rebind_old_targets() {
    let effect = Effect::Multiple(vec![Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent },
        Effect::GainLife { amount: 4 }, Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent }]);
    let (mut s, id) = fixture(effect);
    reach(&mut s); commander_answer(&mut s, false);
    assert!(s.pending_movement.is_none()); assert!(!s.gameplay_stopped());
    assert_eq!(s.objects[&id].zone_change_count, 1); assert_eq!(s.players[2].life, 44);
    assert!(s.players[0].hand.contains(&id));
}

#[test]
fn narrow_death_self_redirect_hands_off_to_commander_replacement() {
    let (mut s, id) = fixture(Effect::DestroyTarget { target: TargetSpec::AnyCreature });
    let source = s.create_card_in_zone(998_301, 2, ZoneType::Battlefield).unwrap();
    s.replacement_effects.push(ReplacementEffect { source_id: source, controller: 2,
        applies_to: ReplacementEventKind::WouldDie, action: ReplacementAction::RedirectToZone(ZoneType::Hand),
        is_self_replacement: true, description: "legacy death redirect".into() });
    reach(&mut s); assert_eq!(s.priority_player, 0); assert!(s.battlefield.contains(&id));
    commander_answer(&mut s, true);
    assert!(s.players[0].command_zone.contains(&id)); assert!(s.players[0].graveyard.is_empty());
    assert_eq!(s.objects[&id].zone_change_count, 1);
}

#[test]
fn library_then_hand_then_command_then_library_decline_commits_only_actual_endpoint() {
    let (mut s, id) = fixture(Effect::PutOnBottomOfLibrary { target: TargetSpec::AnyPermanent });
    astra_redirect(&mut s, MovementDestination::Library, ZoneType::Hand);
    astra_redirect(&mut s, MovementDestination::Command, ZoneType::Library);
    reach(&mut s);
    let redirect = legal_actions(&s).into_iter().find(|a| matches!(a,
        Action::ChooseMovementReplacement { candidate: MovementReplacementCandidate::Ordinary { .. }, .. })).unwrap();
    rules::apply_action(&mut s, &redirect);
    assert_eq!(s.pending_movement.as_ref().unwrap().proposals[0].proposed, ZoneType::Hand);
    commander_answer(&mut s, true);
    assert_eq!(s.pending_movement.as_ref().unwrap().proposals[0].proposed, ZoneType::Library);
    assert_eq!(s.objects[&id].zone_change_count, 0);
    s.drain_events(); commander_answer(&mut s, false);
    assert!(s.players[0].library.contains(&id)); assert!(s.players[0].hand.is_empty());
    assert!(s.players[0].command_zone.is_empty()); assert_eq!(s.objects[&id].zone_change_count, 1);
    assert_eq!(s.drain_events().iter().filter(|e| matches!(e,
        mtg_gto::events::GameEvent::ZoneChange { object, .. } if *object == id)).count(), 1);
}

#[test]
fn redirected_away_library_subjects_are_excluded_and_order_survives_persistence() {
    let (mut s, ids) = astra_private_library_fixture();
    // No object has moved during the owner-controlled placement stage.
    astra_place(&mut s, ids[2]);
    for mut restored in variants(&s) {
        let actions = legal_actions(&restored);
        assert_eq!(actions.iter().filter(|a| !matches!(a, Action::Concede)).count(), 2);
        for a in actions {
            assert_eq!(resolve(&canonicalize(&a, &restored).unwrap(), &restored, 0).unwrap(), Some(a));
        }
        astra_place(&mut restored, ids[0]); astra_place(&mut restored, ids[1]);
        assert_eq!(restored.players[0].library, vec![ids[2], ids[0], ids[1]]);
    }
    let (mut split, first) = fixture(Effect::BounceAllNonlandOpponents);
    split.stack[0].targets.clear(); split.stack[0].target_generations.clear();
    let ordinary = split.create_card_in_zone(998_301, 0, ZoneType::Battlefield).unwrap();
    split.objects.get_mut(&ordinary).unwrap().controller = 1;
    astra_redirect(&mut split, MovementDestination::Hand, ZoneType::Library);
    reach(&mut split); select_commander(&mut split); commander_answer(&mut split, true);
    assert!(split.pending_movement.is_none()); assert!(split.players[0].command_zone.contains(&first));
    assert_eq!(split.players[0].library, vec![ordinary]);
}

fn ability_entry(s: &mut GameState, effect: Effect, targets: Vec<Target>) -> u64 {
    use mtg_gto::{card::ActivatedAbility, mana::ManaCost};
    let old_spell = s.stack[0].source.physical_object().unwrap(); s.objects.remove(&old_spell);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 998_305, name: "Movement ability".into(), card_types: vec![CardType::Artifact],
        activated_abilities: vec![ActivatedAbility { cost: ManaCost::zero(), requires_tap: false,
            sacrifice_cost: None, life_cost: 0, effect, description: "represented departure".into() }],
        ..Default::default() });
    let source = s.create_card_in_zone(998_305, 2, ZoneType::Battlefield).unwrap();
    s.stack[0].source = StackSource::ActivatedAbility { source_id: source, ability_index: 0 };
    s.stack[0].target_generations = targets.iter().map(|t| match t { Target::Object(id) => Some(s.objects[id].zone_change_count), _ => None }).collect();
    s.stack[0].targets = targets;
    source
}
#[test]
fn top_placement_uses_owner_order_and_keeps_existing_library_behind_it() {
    let (mut s, first) = fixture(Effect::GainLife { amount: 0 });
    s.move_object(first, ZoneType::Battlefield, ZoneType::Graveyard); s.commander_arrivals.clear();
    let second = s.create_card_in_zone(998_301, 0, ZoneType::Graveyard).unwrap();
    let existing = s.create_card_in_zone(998_301, 0, ZoneType::Library).unwrap(); s.commanders.clear();
    astra_redirect(&mut s, MovementDestination::Command, ZoneType::Hand);
    ability_entry(&mut s, Effect::ReturnToTopOfLibrary { target: TargetSpec::AnyCreature },
        vec![Target::Object(first), Target::Object(second)]);
    reach(&mut s); assert_eq!(s.priority_player, 0); assert_eq!(legal_actions(&s).iter().filter(|a| !matches!(a, Action::Concede)).count(), 2);
    astra_place(&mut s, second); astra_place(&mut s, first);
    assert_eq!(s.players[0].library, vec![second, first, existing]);
    assert_eq!(s.objects[&first].zone_change_count, 2); assert_eq!(s.objects[&second].zone_change_count, 1);
}
#[test]
fn graveyard_hand_redirect_back_is_a_true_noop() {
    let (mut s, id) = fixture(Effect::GainLife { amount: 0 });
    s.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard); s.commander_arrivals.clear(); s.drain_events();
    ability_entry(&mut s, Effect::ReturnFromGraveyardToHand { target: TargetSpec::AnyCreature }, vec![Target::Object(id)]);
    astra_redirect(&mut s, MovementDestination::Hand, ZoneType::Graveyard);
    let generation = s.objects[&id].zone_change_count; let group = s.next_zone_event_group_id;
    reach(&mut s); select_commander(&mut s); commander_answer(&mut s, false);
    assert!(s.pending_movement.is_none()); assert!(s.players[0].graveyard.contains(&id));
    assert_eq!(s.objects[&id].zone_change_count, generation); assert_eq!(s.next_zone_event_group_id, group);
    assert!(!s.drain_events().iter().any(|event| matches!(event,
        mtg_gto::events::GameEvent::ZoneChange { object, .. } if *object == id)));
}
#[test]
fn linked_return_defers_link_clear_and_rejects_exile_samezone_catalog_before_prefix() {
    for same_zone in [false, true] {
        let (mut s, id) = fixture(Effect::GainLife { amount: 0 });
        s.move_object(id, ZoneType::Battlefield, ZoneType::Exile); s.commander_arrivals.clear(); s.drain_events();
        let source = ability_entry(&mut s, Effect::Multiple(vec![Effect::GainLife { amount: 4 }, Effect::ReturnLinkedExileToHand]), vec![Target::Object(id)]);
        s.objects.get_mut(&id).unwrap().exiled_by = Some(source);
        if same_zone { astra_redirect(&mut s, MovementDestination::Hand, ZoneType::Exile); }
        reach(&mut s);
        if same_zone {
            assert!(s.gameplay_stopped()); assert!(s.pending_movement.is_none());
            assert_eq!(s.players[2].life, 40); assert_eq!(s.objects[&id].exiled_by, Some(source));
            assert!(s.players[0].exile.contains(&id)); assert_eq!(s.stack.len(), 1);
        } else {
            assert_eq!(s.players[2].life, 44); assert_eq!(s.objects[&id].exiled_by, Some(source));
            assert!(s.players[0].exile.contains(&id));
            for mut restored in variants(&s) {
                commander_answer(&mut restored, true);
                assert_eq!(restored.objects[&id].exiled_by, None); assert!(restored.players[0].command_zone.contains(&id));
                assert_eq!(restored.objects[&id].zone_change_count, 2);
            }
        }
    }
}
#[test]
fn effective_controller_chooses_order_owner_still_answers_and_apnap_restarts() {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    s.active_player = 0;
    let source = register(&mut s, MovementDestination::Hand, ReplacementAction::RedirectToZone(ZoneType::Library));
    s.continuous_effects.push(ContinuousEffect { source_id: source, controller: 2, timestamp: 1,
        duration: Duration::Permanent, affected: AffectedObjects::Specific(id), modification: LayerModification::ChangeController(2) });
    s.next_timestamp = 2; s.invalidate_characteristics_cache(); reach(&mut s);
    assert_eq!(s.priority_player, 2); select_commander(&mut s);
    assert_eq!(s.priority_player, 0, "created owner choice restarts APNAP");
    commander_answer(&mut s, true); assert!(s.players[0].command_zone.contains(&id));
}
#[test]
fn opponent_bounce_selects_by_effective_controller_in_both_directions() {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    for (stored, effective, included) in [(1, 2, false), (2, 1, true)] {
        let (mut s, id) = fixture(Effect::BounceAllNonlandOpponents);
        s.stack.last_mut().unwrap().targets.clear(); s.stack.last_mut().unwrap().target_generations.clear();
        s.objects.get_mut(&id).unwrap().controller = stored;
        s.continuous_effects.push(ContinuousEffect { source_id: id, controller: 2, timestamp: 1,
            duration: Duration::Permanent, affected: AffectedObjects::Specific(id),
            modification: LayerModification::ChangeController(effective) });
        s.next_timestamp = 2; s.invalidate_characteristics_cache();
        assert_eq!(s.objects[&id].controller, stored);
        assert_eq!(mtg_gto::layers::compute_characteristics(id, &s.continuous_effects, &s.objects,
            &s.battlefield.iter().copied().collect(), s.card_db()).unwrap().controller, effective);
        reach(&mut s);
        assert!(s.battlefield.contains(&id)); assert_eq!(s.objects[&id].zone_change_count, 0);
        if included {
            let op = s.pending_movement.as_ref().expect("opponent-controlled Commander requires owner answer");
            assert_eq!(op.proposals.len(), 1); assert_eq!(op.proposals[0].request.object.id, id);
            assert_eq!(op.proposals[0].affected_controller, effective); assert_eq!(s.priority_player, 0);
            for mut restored in variants(&s) {
                commander_answer(&mut restored, false); assert!(restored.players[0].hand.contains(&id));
                assert_eq!(restored.objects[&id].zone_change_count, 1);
            }
        } else {
            assert!(s.pending_movement.is_none()); assert!(s.stack.is_empty());
            assert!(s.players[0].hand.is_empty());
        }
    }
}
#[test]
fn opponent_bounce_selects_by_effective_land_type_in_both_directions() {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    for base_land in [false, true] {
        let (mut s, id) = fixture(Effect::BounceAllNonlandOpponents);
        s.stack.last_mut().unwrap().targets.clear(); s.stack.last_mut().unwrap().target_generations.clear();
        if base_land {
            let mut definition = s.card_db().get(998_301).unwrap().clone();
            definition.card_types.push(CardType::Land);
            Arc::make_mut(s.card_db.as_mut().unwrap()).insert(definition);
        }
        s.continuous_effects.push(ContinuousEffect { source_id: id, controller: 2, timestamp: 1,
            duration: Duration::Permanent, affected: AffectedObjects::Specific(id),
            modification: if base_land { LayerModification::SetTypes(vec![CardType::Creature]) }
                else { LayerModification::AddType(CardType::Land) } });
        s.next_timestamp = 2; s.invalidate_characteristics_cache();
        assert_eq!(mtg_gto::layers::compute_characteristics(id, &s.continuous_effects, &s.objects,
            &s.battlefield.iter().copied().collect(), s.card_db()).unwrap().card_types.contains(&CardType::Land), !base_land);
        reach(&mut s);
        assert!(s.battlefield.contains(&id)); assert_eq!(s.objects[&id].zone_change_count, 0);
        if base_land {
            assert!(s.pending_movement.is_some()); assert_eq!(s.priority_player, 0);
            commander_answer(&mut s, false); assert!(s.players[0].hand.contains(&id));
        } else {
            assert!(s.pending_movement.is_none()); assert!(s.stack.is_empty());
            assert!(s.players[0].hand.is_empty());
        }
    }
}
#[test]
fn a_concrete_action_from_a_cancelled_operation_cannot_answer_a_later_operation() {
    let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    register(&mut s, MovementDestination::Hand, ReplacementAction::Prevent);
    reach(&mut s);
    let old = legal_actions(&s).into_iter().find(|a| matches!(a,
        Action::ChooseMovementReplacement { candidate: MovementReplacementCandidate::Ordinary { action: ReplacementAction::Prevent, .. }, .. })).unwrap();
    rules::apply_action(&mut s, &old);
    let source = s.create_card_in_zone(998_302, 2, ZoneType::Stack).unwrap();
    s.stack.push(StackEntry { id: 2, source: StackSource::Spell(source), controller: 2,
        targets: vec![Target::Object(id)], target_generations: vec![Some(0)] }); s.next_stack_id = 3;
    reach(&mut s); assert!(s.pending_movement.is_some());
    let before = bincode::serialize(&s).unwrap(); rules::apply_action(&mut s, &old);
    assert_eq!(before, bincode::serialize(&s).unwrap()); assert!(s.battlefield.contains(&id));
}

#[test]
fn exhausted_operation_window_rejects_before_offers_and_owner_reply() {
    let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    s.next_movement_window = u64::MAX;
    reach(&mut s);
    assert!(s.gameplay_stopped()); assert!(s.pending_movement.is_none());
    assert!(s.battlefield.contains(&id)); assert_eq!(s.objects[&id].zone_change_count, 0);
}

#[test]
fn direct_draw_uses_owner_choice_and_tutor_still_rejects_before_mutation() {
    let (mut s, commander) = fixture(Effect::GainLife { amount: 0 });
    s.move_object(commander, ZoneType::Battlefield, ZoneType::Library); s.drain_events();
    let library = s.players[0].library.clone(); let generation = s.objects[&commander].zone_change_count;
    rules::draw_cards(&mut s, 0, 1);
    assert!(!s.gameplay_stopped()); assert!(s.pending_movement.is_some()); assert_eq!(s.players[0].library, library); assert!(s.players[0].hand.is_empty());
    assert_eq!(s.objects[&commander].zone_change_count, generation); assert!(s.drain_events().is_empty());
    commander_answer(&mut s, false);
    assert!(s.pending_movement.is_none()); assert!(s.players[0].hand.contains(&commander));
    let (mut tutor, commander) = fixture(Effect::GainLife { amount: 0 });
    tutor.move_object(commander, ZoneType::Battlefield, ZoneType::Library); tutor.drain_events();
    tutor.priority_player = 0;
    tutor.pending_tutor = Some(mtg_gto::game::PendingTutor { controller: 0, destination: ZoneType::Hand, subtype_filter: vec![] });
    tutor.players[0].tutor_targets = vec![998_301];
    rules::apply_action(&mut tutor, &Action::ChooseTutorTarget { card_id: 998_301 });
    assert!(tutor.gameplay_stopped()); assert!(tutor.pending_tutor.is_some());
    assert_eq!(tutor.players[0].library, vec![commander]); assert!(tutor.players[0].hand.is_empty());
    assert!(tutor.drain_events().is_empty());
}

#[test]
fn excluded_sba_and_cost_handoffs_do_not_move_or_pay_a_prefix() {
    use mtg_gto::{card::{ActivatedAbility, SacrificeCost}, mana::ManaCost};
    let (mut s, id) = fixture(Effect::GainLife { amount: 0 });
    let source = s.create_card_in_zone(998_301, 2, ZoneType::Battlefield).unwrap();
    s.replacement_effects.push(ReplacementEffect { source_id: source, controller: 2,
        applies_to: ReplacementEventKind::WouldDie, action: ReplacementAction::RedirectToZone(ZoneType::Hand),
        is_self_replacement: true, description: "death to hand".into() });
    s.objects.get_mut(&id).unwrap().damage_marked = 3;
    let group = s.next_zone_event_group_id;
    rules::check_state_based_actions(&mut s);
    assert!(s.gameplay_stopped()); assert!(s.battlefield.contains(&id));
    assert_eq!(s.objects[&id].zone_change_count, 0); assert_eq!(s.next_zone_event_group_id, group);
    let (mut cost, id) = fixture(Effect::GainLife { amount: 0 });
    cost.priority_player = 1; cost.objects.get_mut(&id).unwrap().summoning_sick = false;
    let effect_source = cost.create_card_in_zone(998_301, 2, ZoneType::Battlefield).unwrap();
    cost.replacement_effects.push(ReplacementEffect { source_id: effect_source, controller: 2,
        applies_to: ReplacementEventKind::WouldDie, action: ReplacementAction::RedirectToZone(ZoneType::Hand),
        is_self_replacement: true, description: "death to hand".into() });
    let mut definition = cost.card_db().get(998_301).unwrap().clone();
    definition.activated_abilities.push(ActivatedAbility { cost: ManaCost::zero(), requires_tap: true,
        sacrifice_cost: Some(SacrificeCost::SelfSacrifice), life_cost: 3, effect: Effect::GainLife { amount: 8 }, description: "cost seam".into() });
    Arc::make_mut(cost.card_db.as_mut().unwrap()).insert(definition);
    let life = cost.players[1].life; let stack = cost.stack.len();
    rules::apply_action(&mut cost, &Action::ActivateAbility { object_id: id, ability_index: 0, targets: vec![] });
    assert!(cost.gameplay_stopped()); assert!(cost.battlefield.contains(&id));
    assert!(!cost.objects[&id].tapped); assert_eq!(cost.players[1].life, life); assert_eq!(cost.stack.len(), stack);
}

#[test]
fn unrelated_hidden_identities_and_source_generation_are_not_public_coordinates() {
    let (mut s, _) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    register(&mut s, MovementDestination::Hand, ReplacementAction::RedirectToZone(ZoneType::Library));
    let hidden_hand = s.create_card_in_zone(998_301, 2, ZoneType::Hand).unwrap();
    let hidden_library = s.create_card_in_zone(998_301, 0, ZoneType::Library).unwrap();
    let mut hidden = s.clone();
    hidden.objects.get_mut(&hidden_hand).unwrap().card_def_id = 998_302;
    hidden.objects.get_mut(&hidden_library).unwrap().card_def_id = 998_302;
    hidden.objects.get_mut(&hidden_library).unwrap().zone_change_count = 24;
    reach(&mut s); reach(&mut hidden);
    assert_eq!(information(&s, 1).hash_value(), information(&hidden, 1).hash_value());
    let keys = |s: &GameState| legal_actions(s).iter().map(|a| canonicalize(a, s).unwrap()).collect::<std::collections::HashSet<_>>();
    assert_eq!(keys(&s), keys(&hidden));
    let (mut shifted, _) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    register(&mut shifted, MovementDestination::Hand, ReplacementAction::RedirectToZone(ZoneType::Library));
    shifted.create_card_in_zone(998_301, 2, ZoneType::Hand).unwrap(); shifted.create_card_in_zone(998_301, 0, ZoneType::Library).unwrap();
    for object in shifted.objects.values_mut() { object.zone_change_count += 13; }
    shifted.stack[0].target_generations[0] = Some(13);
    if let ReplacementEventKind::WouldMoveTo { source_generation, .. } = &mut shifted.replacement_effects[0].applies_to { *source_generation = 13; }
    reach(&mut shifted);
    assert_eq!(information(&s, 1).hash_value(), information(&shifted, 1).hash_value()); assert_eq!(keys(&s), keys(&shifted));
}

#[test]
fn departed_trigger_source_owned_context_survives_all_persistence_forms() {
    use mtg_gto::{card::{TriggeredAbility, TriggerCondition}, rules::transitions::{transition_batch, TransitionRequest, MovementKind}};
    let (mut s, id) = fixture(Effect::GainLife { amount: 0 });
    let effect = Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent };
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef { id: 998_306, name: "Departing source".into(),
        card_types: vec![CardType::Creature], power: Some(1), toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility { trigger: TriggerCondition::Dies, effect, description: "owned death movement".into() }],
        ..Default::default() });
    let source = s.create_card_in_zone(998_306, 2, ZoneType::Battlefield).unwrap();
    let exact_source = s.exact_object(source).unwrap();
    transition_batch(&mut s, &[TransitionRequest { object: exact_source,
        from: ZoneType::Battlefield, to: ZoneType::Graveyard, kind: MovementKind::Put }]).unwrap();
    let trigger = s.pending_triggers.iter().find(|t| t.source_id == source).unwrap().clone();
    s.pending_triggers.clear();
    let old_spell = s.stack[0].source.physical_object().unwrap(); s.objects.remove(&old_spell);
    s.stack[0].source = StackSource::TriggeredAbility { source_id: source, ability_index: 0, context: Box::new(trigger.context) };
    reach(&mut s); assert_eq!(s.priority_player, 0); assert!(s.pending_movement.as_ref().unwrap().source.is_none());
    let information_before = information(&s, 0).hash_value();
    for mut restored in variants(&s) {
        assert_eq!(information(&restored, 0).hash_value(), information_before);
        assert_eq!(restored.objects[&source].zone_change_count, 1);
        commander_answer(&mut restored, false);
        assert!(restored.players[0].hand.contains(&id)); assert!(restored.players[2].graveyard.contains(&source));
        assert!(!restored.gameplay_stopped()); assert_eq!(restored.objects[&id].zone_change_count, 1);
    }
}

#[test]
fn conditional_modal_and_loop_movements_are_rejected_before_any_prefix() {
    use mtg_gto::card::{effects::Condition, DynamicValue};
    let movement = Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent };
    for wrapper in [Effect::Conditional { condition: Condition::HandIsEmpty, if_true: Box::new(movement.clone()), if_false: None },
        Effect::Modal { choices: vec![movement.clone()], choose_count: 1 },
        Effect::ForEach { count: DynamicValue::Fixed(2), effect: Box::new(movement) }] {
        let (mut s, id) = fixture(Effect::Multiple(vec![Effect::GainLife { amount: 11 }, wrapper]));
        let source = s.stack[0].source.physical_object().unwrap(); reach(&mut s);
        assert!(s.gameplay_stopped()); assert_eq!(s.players[2].life, 40); assert_eq!(s.stack.len(), 1);
        assert!(s.battlefield.contains(&id)); assert_eq!(s.objects[&source].zone_change_count, 0);
    }
}

#[test]
fn failed_reached_preparation_retains_completed_prefix_and_cannot_resume_it() {
    let (mut s, id) = fixture(Effect::Multiple(vec![Effect::GainLife { amount: 7 },
        Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent }]));
    s.objects.get_mut(&id).unwrap().zone_change_count = u32::MAX;
    s.stack[0].target_generations = vec![Some(u32::MAX)];
    let source = s.stack[0].source.physical_object().unwrap(); reach(&mut s);
    assert!(s.gameplay_stopped()); assert_eq!(s.players[2].life, 47); assert_eq!(s.stack.len(), 1);
    assert!(s.battlefield.contains(&id)); assert!(s.players[2].graveyard.is_empty());
    assert_eq!(s.objects[&source].zone_change_count, 0); assert!(legal_actions(&s).is_empty());
    for mut restored in variants(&s) {
        let before = bincode::serialize(&restored).unwrap(); rules::apply_action(&mut restored, &Action::PassPriority);
        assert_eq!(bincode::serialize(&restored).unwrap(), before); assert_eq!(restored.players[2].life, 47);
    }
}

#[test]
fn restored_closed_envelope_cannot_insert_or_replay_an_unauthorized_tail() {
    use mtg_gto::game::BoundedMovementInstruction;
    let (mut s, _) = fixture(Effect::Multiple(vec![Effect::GainLife { amount: 7 },
        Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent }, Effect::GainLife { amount: 2 }]));
    reach(&mut s); assert_eq!(s.players[2].life, 47);
    for fault in 0..3 {
        let mut bad = s.clone(); let op = bad.pending_movement.as_mut().unwrap();
        match fault { 0 => op.envelope.push(BoundedMovementInstruction::GainLife { amount: 33 }),
            1 => op.instruction_index = 0, _ => op.envelope.pop().map(|_| ()).unwrap() }
        assert!(bad.validate_movement_choices().is_err()); assert!(legal_actions(&bad).is_empty());
        let before = bad.players[2].life;
        rules::apply_action(&mut bad, &legal_actions(&s)[0]); assert_eq!(bad.players[2].life, before);
    }
    commander_answer(&mut s, false); assert_eq!(s.players[2].life, 49);
}

#[test]
fn a_failed_later_instruction_retains_the_resolving_owner_after_committed_movement() {
    let (mut s, id) = fixture(Effect::Multiple(vec![Effect::GainLife { amount: 7 },
        Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent },
        Effect::AddMana { color: None, amount: u32::MAX }]));
    s.players[2].mana_pool.colorless = 1;
    let source = s.stack[0].source.physical_object().unwrap(); reach(&mut s);
    commander_answer(&mut s, false);
    assert!(s.gameplay_stopped()); assert!(s.pending_movement.is_none()); assert_eq!(s.stack.len(), 1);
    assert!(s.players[0].hand.contains(&id)); assert_eq!(s.objects[&id].zone_change_count, 1);
    assert_eq!(s.players[2].life, 47); assert_eq!(s.players[2].mana_pool.colorless, 1);
    assert_eq!(s.objects[&source].zone_change_count, 0);
    for mut restored in variants(&s) {
        let before = bincode::serialize(&restored).unwrap(); rules::apply_action(&mut restored, &Action::PassPriority);
        assert_eq!(bincode::serialize(&restored).unwrap(), before);
    }
}

#[test]
fn heterogeneous_replacement_revisions_do_not_reveal_private_library_order() {
    let (mut s, first) = fixture(Effect::BounceAllNonlandOpponents);
    s.commanders.clear(); s.stack[0].targets.clear(); s.stack[0].target_generations.clear();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef { id: 998_303, name: "Distinct private mover".into(),
        card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2), ..Default::default() });
    let second = s.create_card_in_zone(998_303, 0, ZoneType::Battlefield).unwrap();
    s.objects.get_mut(&second).unwrap().controller = 1;
    astra_redirect(&mut s, MovementDestination::Hand, ZoneType::Library);
    astra_redirect(&mut s, MovementDestination::Hand, ZoneType::Hand);
    reach(&mut s);
    let choose = |s: &mut GameState, id, to| {
        let action = legal_actions(s).into_iter().find(|a| matches!(a,
            Action::ChooseMovementReplacement { object, candidate: MovementReplacementCandidate::Ordinary { action: ReplacementAction::RedirectToZone(zone), .. }, .. }
                if object.id == id && *zone == to)).unwrap();
        rules::apply_action(s, &action);
    };
    choose(&mut s, first, ZoneType::Library); choose(&mut s, second, ZoneType::Hand);
    assert_eq!(s.priority_player, 0);
    let op = s.pending_movement.as_ref().unwrap();
    assert_eq!(op.proposals.iter().find(|p| p.request.object.id == first).unwrap().revision, 1);
    assert_eq!(op.proposals.iter().find(|p| p.request.object.id == second).unwrap().revision, 2);
    let mut left = s.clone(); let mut right = s;
    astra_place(&mut left, first); astra_place(&mut right, second);
    assert!(left.pending_movement.is_some() && right.pending_movement.is_some());
    assert_ne!(information(&left, 0).hash_value(), information(&right, 0).hash_value());
    for observer in [1, 2] {
        assert_eq!(information(&left, observer).zone_normalization, information(&right, observer).zone_normalization);
        assert_eq!(information(&left, observer).hash_value(), information(&right, observer).hash_value());
    }
}

#[test]
fn movement_strategy_consumers_report_corruption_and_handle_opponent_ownership() {
    use mtg_gto::{strategy::{Strategy, GreedyStrategy, GoldfishStrategy, RandomStrategy}, simulation::TerminationReason};
    let (mut s, _) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent }); reach(&mut s);
    s.pending_movement.as_mut().unwrap().history[0].resulting_revision = 42;
    for strategy in [&GreedyStrategy as &dyn Strategy, &GoldfishStrategy, &RandomStrategy] {
        assert_eq!(strategy.choose_action(&s, s.priority_player), Err(TerminationReason::StateEncoding));
    }
    let (mut opponent, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    opponent.commanders.clear(); opponent.designate_commander(id, 0).unwrap();
    // Rebind owner to a nonactive player using a fresh valid designation.
    opponent.commanders.clear(); opponent.objects.get_mut(&id).unwrap().owner = 1;
    opponent.designate_commander(id, 0).unwrap(); reach(&mut opponent);
    assert_eq!(opponent.priority_player, 1);
    let action = GoldfishStrategy.choose_action(&opponent, 1).unwrap();
    assert!(matches!(action, Action::ChooseCommanderMovement { .. }));
    rules::apply_action(&mut opponent, &action); assert!(opponent.pending_movement.is_none());
}

#[test]
fn concession_uses_existing_terminal_or_unsupported_loss_boundary_without_movement() {
    for players in [2, 3] {
        let (mut s, id) = fixture_for(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent }, players);
        if players == 2 { s.stack[0].controller = 1; s.objects.get_mut(&s.stack[0].source.physical_object().unwrap()).unwrap().owner = 1; }
        reach(&mut s); assert!(legal_actions(&s).contains(&Action::Concede));
        let history = s.pending_movement.as_ref().unwrap().history.clone(); s.drain_events();
        rules::apply_action(&mut s, &Action::Concede);
        assert!(s.gameplay_stopped()); assert!(s.battlefield.contains(&id)); assert_eq!(s.objects[&id].zone_change_count, 0);
        assert_eq!(s.pending_movement.as_ref().unwrap().history, history); assert!(s.drain_events().is_empty());
        if players == 2 { assert!(s.game_over); assert!(s.loss_boundary.terminal.is_some()); }
        else { assert!(!s.game_over); assert!(s.loss_boundary.unsupported.is_some()); }
        for restored in variants(&s) { assert!(restored.validate_movement_choices().is_ok()); assert!(legal_actions(&restored).is_empty()); }
    }
}

#[test]
fn special_draw_expansion_rejects_commander_beyond_the_nominal_prefix() {
    use mtg_gto::layers::StaticAbility;
    for replacement in [StaticAbility::PhialDrawDoubler, StaticAbility::RenfieldDrawReplacement, StaticAbility::AbundanceReplacement] {
        let (mut s, commander) = fixture(Effect::GainLife { amount: 0 });
        let ordinary = s.create_card_in_zone(998_302, 0, ZoneType::Library).unwrap();
        s.move_object(commander, ZoneType::Battlefield, ZoneType::Library); s.drain_events();
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef { id: 998_307, name: "Special draw".into(),
            card_types: vec![CardType::Artifact], static_abilities: vec![replacement], ..Default::default() });
        s.create_card_in_zone(998_307, 0, ZoneType::Battlefield).unwrap();
        let library = s.players[0].library.clone(); assert_eq!(library, vec![ordinary, commander]);
        let generation = s.objects[&commander].zone_change_count;
        rules::draw_cards(&mut s, 0, 1);
        assert!(s.gameplay_stopped()); assert_eq!(s.players[0].library, library); assert!(s.players[0].hand.is_empty());
        assert_eq!(s.objects[&commander].zone_change_count, generation); assert!(s.drain_events().is_empty());
    }
}

#[test]
fn incompatible_graveyard_recipients_reject_before_the_prefix() {
    for recipient in [TargetSpec::EachCreature, TargetSpec::CardInHand, TargetSpec::CardInExileBySource] {
        for movement in [Effect::ReturnFromGraveyardToHand { target: recipient.clone() }, Effect::ReturnToTopOfLibrary { target: recipient.clone() }] {
            let (mut s, id) = fixture(Effect::Multiple(vec![Effect::GainLife { amount: 7 }, movement]));
            s.stack[0].targets.clear(); s.stack[0].target_generations.clear();
            reach(&mut s); assert!(s.gameplay_stopped()); assert_eq!(s.players[2].life, 40);
            assert_eq!(s.stack.len(), 1); assert!(s.battlefield.contains(&id)); assert!(s.pending_movement.is_none());
        }
    }
}

#[test]
fn multiple_self_replacements_take_priority_over_commander_and_ordinary_effects() {
    let (mut s, id) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    for to in [ZoneType::Library, ZoneType::Command] {
        astra_redirect(&mut s, MovementDestination::Hand, to); s.replacement_effects.last_mut().unwrap().is_self_replacement = true;
    }
    astra_redirect(&mut s, MovementDestination::Hand, ZoneType::Exile);
    reach(&mut s); assert_eq!(s.priority_player, 1);
    let choices: Vec<_> = legal_actions(&s).into_iter().filter(|a| !matches!(a, Action::Concede)).collect();
    assert_eq!(choices.len(), 2);
    assert!(choices.iter().all(|a| matches!(a,
        Action::ChooseMovementReplacement { candidate: MovementReplacementCandidate::Ordinary { self_replacement: true, .. }, .. })));
    let choose_library = choices.into_iter().find(|a| matches!(a,
        Action::ChooseMovementReplacement { candidate: MovementReplacementCandidate::Ordinary { action: ReplacementAction::RedirectToZone(ZoneType::Library), .. }, .. })).unwrap();
    rules::apply_action(&mut s, &choose_library); assert_eq!(s.priority_player, 0);
    commander_answer(&mut s, false); assert!(s.players[0].library.contains(&id)); assert!(s.players[0].exile.is_empty());
}

#[test]
fn multiple_library_owners_choose_private_orders_in_apnap_before_any_movement() {
    let (mut s, first) = fixture_for(Effect::BounceAllNonlandOpponents, 4);
    s.commanders.clear(); s.stack[0].targets.clear(); s.stack[0].target_generations.clear(); s.active_player = 1;
    let mut ids = vec![vec![first], vec![], vec![], vec![]];
    for owner in 0..4 {
        for _ in ids[owner].len()..2 {
            let id = s.create_card_in_zone(998_301, owner, ZoneType::Battlefield).unwrap();
            s.objects.get_mut(&id).unwrap().controller = 1; ids[owner].push(id);
        }
    }
    astra_redirect(&mut s, MovementDestination::Hand, ZoneType::Library);
    for _ in 0..4 { rules::apply_action(&mut s, &Action::PassPriority); }
    for owner in [1, 2, 3, 0] {
        assert_eq!(s.priority_player, owner);
        assert!(ids.iter().flatten().all(|id| s.battlefield.contains(id) && s.objects[id].zone_change_count == 0));
        astra_place(&mut s, ids[owner][1]);
        for observer in (0..4).filter(|viewer| *viewer != owner) {
            let mut alternate = s.clone();
            let op = alternate.pending_movement.as_mut().unwrap();
            let last = op.history.last_mut().unwrap(); last.object = op.proposals.iter().find(|p| p.request.object.id == ids[owner][0]).unwrap().request.object;
            *op.library_order.last_mut().unwrap() = last.object;
            assert!(alternate.validate_movement_choices().is_ok());
            assert_eq!(information(&s, observer).hash_value(), information(&alternate, observer).hash_value());
        }
        astra_place(&mut s, ids[owner][0]);
    }
    assert!(s.pending_movement.is_none());
    for owner in 0..4 { assert_eq!(s.players[owner].library, vec![ids[owner][1], ids[owner][0]]); }
}

#[test]
fn missing_replacement_source_definition_fails_actions_observation_and_strategies() {
    use mtg_gto::{strategy::{Strategy, GreedyStrategy, GoldfishStrategy, RandomStrategy}, simulation::TerminationReason};
    let (mut s, _) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    let source = register(&mut s, MovementDestination::Hand, ReplacementAction::RedirectToZone(ZoneType::Library));
    reach(&mut s); s.objects.get_mut(&source).unwrap().card_def_id = 9_999_999;
    assert!(s.validate_movement_choices().is_err()); assert!(legal_actions(&s).is_empty());
    assert!(InformationSet::from_view(&s.visible_state(1), s.card_db()).is_err());
    for strategy in [&GreedyStrategy as &dyn Strategy, &GoldfishStrategy, &RandomStrategy] {
        assert_eq!(strategy.choose_action(&s, s.priority_player), Err(TerminationReason::StateEncoding));
    }
}

#[test]
fn restored_structure_cannot_resume_choices_before_database_attachment() {
    use mtg_gto::{strategy::{Strategy, GreedyStrategy, GoldfishStrategy, RandomStrategy}, simulation::TerminationReason};
    let (mut original, _) = fixture(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent });
    reach(&mut original); let action = legal_actions(&original)[0].clone();
    let canonical = canonicalize(&action, &original).unwrap();
    for mut state in [serde_json::from_slice::<GameState>(&serde_json::to_vec(&original).unwrap()).unwrap(),
        bincode::deserialize::<GameState>(&bincode::serialize(&original).unwrap()).unwrap()] {
        assert!(state.card_db.is_none()); assert!(state.validate_movement_choices().is_ok());
        assert!(state.validate_movement_play().is_err()); assert!(legal_actions(&state).is_empty());
        assert_eq!(canonicalize(&action, &state), Err(TerminationReason::StateEncoding));
        assert_eq!(resolve(&canonical, &state, state.priority_player), Err(TerminationReason::StateEncoding));
        for strategy in [&GreedyStrategy as &dyn Strategy, &GoldfishStrategy, &RandomStrategy] {
            assert_eq!(strategy.choose_action(&state, state.priority_player), Err(TerminationReason::StateEncoding));
        }
        let before = bincode::serialize(&state).unwrap(); rules::apply_action(&mut state, &action);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
        state.card_db = original.card_db.clone(); assert!(state.validate_movement_play().is_ok());
        assert_eq!(legal_actions(&state), legal_actions(&original));
    }
}
