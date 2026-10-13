use std::{path::Path, sync::Arc};

use mtg_gto::{
    action::{canonical::{canonicalize, resolve as reconstruct}, legal_actions, Action},
    card::{sample, ZoneType},
    deck_loader::{self, certification_issue, stable_fingerprint},
    events::{GameEvent, Zone},
    game::{GameState, Phase, StackEntry, StackSource, Target, TriggerContext},
    info_set::InformationSet,
    replacement::{MovementDestination, ReplacementAction, ReplacementEffect, ReplacementEventKind},
    rules,
};

fn game(players: usize) -> (GameState, u64) {
    let mut state = GameState::new_commander(players);
    state.card_db = Some(Arc::new(sample::build_sample_db()));
    state.phase = Phase::PreCombatMain;
    let source = state.create_card_in_zone(sample::ids::GOBLIN_MATRON, 0, ZoneType::Battlefield).unwrap();
    state.drain_events();
    (state, source)
}

fn stack_trigger(state: &mut GameState, source: u64, controller: usize) {
    let definition = state.card_db().get(sample::ids::GOBLIN_MATRON).unwrap();
    let effect = definition.triggered_abilities[0].effect.clone();
    let id = state.new_stack_id();
    state.stack.push(StackEntry {
        id,
        controller,
        source: StackSource::TriggeredAbility {
            source_id: source,
            ability_index: 0,
            context: Box::new(TriggerContext {
                source_card_id: sample::ids::GOBLIN_MATRON,
                source_generation: state.objects[&source].zone_change_count,
                effect,
                cast_spell: None,
                zone_transition: None,
            }),
        },
        targets: Vec::<Target>::new(),
        target_generations: vec![],
    });
}

fn resolve_trigger(state: &mut GameState) {
    for _ in 0..state.players.len() { rules::apply_action(state, &Action::PassPriority); }
}

fn search_action(state: &GameState, object: u64) -> Action {
    legal_actions(state).into_iter().find(|action| matches!(action,
        Action::ChooseGoblinMatronCard { object: exact, .. } if exact.id == object)).unwrap()
}

fn commander_answer(state: &GameState, to_command: bool) -> Action {
    legal_actions(state).into_iter().find(|action| matches!(action,
        Action::ChooseCommanderMovement { to_command: answer, .. } if *answer == to_command)).unwrap()
}

fn replacement(state: &mut GameState, source: u64, destination: MovementDestination,
    action: ReplacementAction, self_replacement: bool) {
    state.replacement_effects.push(ReplacementEffect {
        source_id: source,
        controller: state.objects[&source].controller,
        applies_to: ReplacementEventKind::WouldMoveTo {
            destination,
            source_generation: state.objects[&source].zone_change_count,
        },
        action,
        is_self_replacement: self_replacement,
        description: "Matron correction fixture".into(),
    });
}

fn restored_variants(state: &GameState) -> Vec<GameState> {
    let mut snapshot = state.clone();
    snapshot.restore(state.snapshot()).unwrap();
    let mut variants = vec![
        state.clone(),
        snapshot,
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap(),
        bincode::deserialize(&bincode::serialize(state).unwrap()).unwrap(),
    ];
    for variant in &mut variants { variant.card_db = state.card_db.clone(); }
    variants
}

#[test]
fn actual_goblin_matron_definition_is_authored_and_catalog_certified() {
    let resolved = deck_loader::resolve_file(Path::new("decks/goblin_storm.txt")).unwrap();
    let id = resolved.db.find_by_name("Goblin Matron").unwrap();
    assert_eq!(id, sample::ids::GOBLIN_MATRON);
    let definition = resolved.db.get(id).unwrap();
    assert_eq!(definition.triggered_abilities.len(), 1);
    assert_eq!(definition.triggered_abilities[0].effect, mtg_gto::card::Effect::GoblinMatronSearch);
    let entry = resolved.catalog.as_ref().unwrap().cards.iter().find(|card| card.name == "Goblin Matron").unwrap();
    let fingerprint = stable_fingerprint(&serde_json::to_vec(definition).unwrap());
    eprintln!("Goblin Matron behavior fingerprint: {fingerprint}");
    assert_eq!(entry.support.behavior_fingerprint.as_deref(), Some(fingerprint.as_str()));
    assert_eq!(certification_issue(entry, definition), None);
}

#[test]
fn successful_search_is_exact_revealed_moved_and_shuffled_once() {
    let (mut state, source) = game(2);
    let first = state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
    let second = state.create_card_in_zone(sample::ids::GOBLIN_CHIEFTAIN, 0, ZoneType::Library).unwrap();
    let mountain = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library).unwrap();
    stack_trigger(&mut state, source, 0);
    resolve_trigger(&mut state);
    assert!(state.pending_matron_search.is_some());
    assert_eq!(state.priority_player, 0);
    let actions = legal_actions(&state);
    assert!(actions.iter().any(|action| matches!(action, Action::DeclineGoblinMatronSearch { .. })));
    assert!(actions.iter().any(|action| matches!(action, Action::FailToFindGoblinMatronCard { .. })));
    assert!(actions.iter().any(|action| matches!(action, Action::ChooseGoblinMatronCard { object, .. } if object.id == first)));
    assert!(actions.iter().any(|action| matches!(action, Action::ChooseGoblinMatronCard { object, .. } if object.id == second)));
    assert!(!actions.iter().any(|action| matches!(action, Action::ChooseGoblinMatronCard { object, .. } if object.id == mountain)));
    assert!(!actions.contains(&Action::PassPriority), "the search is not a priority window");
    let selected = search_action(&state, second);
    let canonical = canonicalize(&selected, &state).unwrap();
    assert_eq!(reconstruct(&canonical, &state, 0), Ok(Some(selected.clone())));
    rules::apply_action(&mut state, &selected);
    assert!(state.pending_matron_search.is_none());
    assert!(state.pending_movement.is_none());
    assert!(state.players[0].hand.contains(&second));
    assert!(!state.players[0].library.contains(&second));
    assert_eq!(state.priority_player, state.active_player);
    for restored in restored_variants(&state) {
        let opponent = restored.visible_state(1);
        assert!(opponent.objects.contains_key(&second), "revealed hand card remains public");
        assert!(opponent.visible_locations.contains(&(second, ZoneType::Hand, 0)));
        assert!(!opponent.objects.contains_key(&first), "unselected library card remains hidden");
    }
    let events = state.drain_events();
    assert_eq!(events.iter().filter(|event| matches!(event,
        GameEvent::CardRevealed { player: 0, object } if *object == second)).count(), 1);
    assert_eq!(events.iter().filter(|event| matches!(event,
        GameEvent::ZoneChange { object, from: Zone::Library, to: Zone::Hand } if *object == second)).count(), 1);
    assert_eq!(events.iter().filter(|event| matches!(event,
        GameEvent::LibraryShuffled { player: 0 })).count(), 1);
}

#[test]
fn actual_etb_trigger_reaches_the_owned_search_and_settles() {
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(sample::build_sample_db()));
    state.phase = Phase::PreCombatMain;
    state.players[0].mana_pool.red = 1;
    state.players[0].mana_pool.colorless = 2;
    let matron = state.create_card_in_zone(sample::ids::GOBLIN_MATRON, 0, ZoneType::Hand).unwrap();
    let goblin = state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
    state.drain_events();

    let cast = Action::CastSpell { object_id: matron, targets: vec![] };
    assert!(legal_actions(&state).contains(&cast));
    rules::apply_action(&mut state, &cast);
    resolve_trigger(&mut state);
    assert!(state.battlefield.contains(&matron));
    assert!(state.stack.iter().any(|entry| matches!(&entry.source,
        StackSource::TriggeredAbility { source_id, context, .. }
            if *source_id == matron && context.source_card_id == sample::ids::GOBLIN_MATRON)));
    resolve_trigger(&mut state);
    assert!(state.pending_matron_search.is_some());
    let selection = search_action(&state, goblin);
    rules::apply_action(&mut state, &selection);
    assert!(state.players[0].hand.contains(&goblin));
    assert!(state.pending_matron_search.is_none());
    assert!(!state.gameplay_stopped());
}

#[test]
fn search_controller_can_differ_from_the_source_owner() {
    let (mut state, source) = game(2);
    state.objects.get_mut(&source).unwrap().controller = 1;
    let owners_goblin = state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
    let controllers_goblin = state.create_card_in_zone(sample::ids::GOBLIN_CHIEFTAIN, 1, ZoneType::Library).unwrap();
    stack_trigger(&mut state, source, 1);
    resolve_trigger(&mut state);
    assert_eq!(state.priority_player, 1);
    let actions = legal_actions(&state);
    assert!(actions.iter().any(|action| matches!(action,
        Action::ChooseGoblinMatronCard { object, .. } if object.id == controllers_goblin)));
    assert!(!actions.iter().any(|action| matches!(action,
        Action::ChooseGoblinMatronCard { object, .. } if object.id == owners_goblin)));
    let selection = search_action(&state, controllers_goblin);
    rules::apply_action(&mut state, &selection);
    assert!(state.players[1].hand.contains(&controllers_goblin));
    assert!(state.players[0].library.contains(&owners_goblin));
}

#[test]
fn fail_to_find_is_always_legal_and_shuffles_empty_or_nonempty_library() {
    for with_eligible in [false, true] {
        let (mut state, source) = game(2);
        if with_eligible { state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap(); }
        stack_trigger(&mut state, source, 0);
        resolve_trigger(&mut state);
        let action = legal_actions(&state).into_iter().find(|action|
            matches!(action, Action::FailToFindGoblinMatronCard { .. })).unwrap();
        let before = state.players[0].library.clone();
        rules::apply_action(&mut state, &action);
        assert!(state.pending_matron_search.is_none());
        assert_eq!(state.players[0].library.len(), before.len());
        assert!(state.players[0].hand.is_empty());
        assert_eq!(state.drain_events().iter().filter(|event|
            matches!(event, GameEvent::LibraryShuffled { player: 0 })).count(), 1);
    }
}

#[test]
fn optional_search_choices_remain_distinct_through_canonicalization_and_restoration() {
    let (mut state, source) = game(2);
    let goblin = state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
    state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library).unwrap();
    state.create_card_in_zone(sample::ids::FOREST, 0, ZoneType::Library).unwrap();
    stack_trigger(&mut state, source, 0);
    resolve_trigger(&mut state);
    state.drain_events();
    assert!(state.pending_matron_search.is_some());
    assert!(!legal_actions(&state).contains(&Action::PassPriority));

    for mut restored in restored_variants(&state) {
        let actions = legal_actions(&restored);
        let decline = actions.iter().find(|action| matches!(action,
            Action::DeclineGoblinMatronSearch { .. })).unwrap().clone();
        let fail = actions.iter().find(|action| matches!(action,
            Action::FailToFindGoblinMatronCard { .. })).unwrap().clone();
        let find = search_action(&restored, goblin);
        let identities = [canonicalize(&decline, &restored).unwrap(),
            canonicalize(&fail, &restored).unwrap(), canonicalize(&find, &restored).unwrap()];
        assert_ne!(identities[0], identities[1]);
        assert_ne!(identities[0], identities[2]);
        assert_ne!(identities[1], identities[2]);
        for (identity, action) in identities.iter().zip([&decline, &fail, &find]) {
            assert_eq!(reconstruct(identity, &restored, 0), Ok(Some(action.clone())));
        }

        let before = restored.players[0].library.clone();
        rules::apply_action(&mut restored, &decline);
        assert_eq!(restored.players[0].library, before);
        assert!(restored.players[0].hand.is_empty());
        assert!(restored.pending_matron_search.is_none());
        assert!(restored.trigger_order_resume.is_none());
        assert_eq!(restored.priority_player, restored.active_player);
        assert!(!restored.gameplay_stopped());
        assert!(restored.drain_events().iter().all(|event|
            !matches!(event, GameEvent::LibraryShuffled { .. } | GameEvent::CardRevealed { .. })));
    }

    for mut restored in restored_variants(&state) {
        let fail = legal_actions(&restored).into_iter().find(|action| matches!(action,
            Action::FailToFindGoblinMatronCard { .. })).unwrap();
        rules::apply_action(&mut restored, &fail);
        assert!(restored.pending_matron_search.is_none());
        assert!(restored.players[0].hand.is_empty());
        assert_eq!(restored.drain_events().iter().filter(|event|
            matches!(event, GameEvent::LibraryShuffled { player: 0 })).count(), 1);
    }
}

#[test]
fn commander_owner_accepts_hand_or_replaces_with_command_zone() {
    for to_command in [false, true] {
        let (mut state, source) = game(3);
        let zada = state.create_card_in_zone(sample::ids::ZADA_HEDRON_GRINDER, 0, ZoneType::Library).unwrap();
        state.designate_commander(zada, 0).unwrap();
        state.objects.get_mut(&zada).unwrap().controller = 2;
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library).unwrap();
        stack_trigger(&mut state, source, 0);
        resolve_trigger(&mut state);
        let selection = search_action(&state, zada);
        rules::apply_action(&mut state, &selection);
        assert!(state.pending_matron_search.is_none());
        assert!(state.pending_movement.is_some());
        assert_eq!(state.priority_player, 0, "owner chooses despite stored controller");
        assert!(state.players[0].library.contains(&zada), "movement waits for choice");
        let answer = commander_answer(&state, to_command);
        rules::apply_action(&mut state, &answer);
        assert!(state.pending_movement.is_none());
        assert_eq!(state.players[0].hand.contains(&zada), !to_command);
        assert_eq!(state.players[0].command_zone.contains(&zada), to_command);
        assert_eq!(state.drain_events().iter().filter(|event|
            matches!(event, GameEvent::LibraryShuffled { player: 0 })).count(), 1);
    }
}

#[test]
fn stale_duplicate_and_wrong_search_actions_are_atomic() {
    let (mut state, source) = game(2);
    let goblin = state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
    stack_trigger(&mut state, source, 0);
    resolve_trigger(&mut state);
    let action = search_action(&state, goblin);
    let before = bincode::serialize(&state).unwrap();
    let Action::ChooseGoblinMatronCard { window, object } = action.clone() else { unreachable!() };
    rules::apply_action(&mut state, &Action::ChooseGoblinMatronCard { window: window + 1, object });
    assert_eq!(bincode::serialize(&state).unwrap(), before);
    let stale = mtg_gto::card::ExactObjectRef { generation: object.generation + 1, ..object };
    rules::apply_action(&mut state, &Action::ChooseGoblinMatronCard { window, object: stale });
    assert_eq!(bincode::serialize(&state).unwrap(), before);
    rules::apply_action(&mut state, &action);
    let settled = bincode::serialize(&state).unwrap();
    rules::apply_action(&mut state, &action);
    assert_eq!(bincode::serialize(&state).unwrap(), settled);
}

#[test]
fn unsupported_matron_replacements_reject_before_reveal_or_search_consumption() {
    let cases = [
        (ReplacementAction::RedirectToZone(ZoneType::Exile), false),
        (ReplacementAction::Prevent, true),
    ];
    for (action, self_replacement) in cases {
        let (mut state, source) = game(2);
        let goblin = state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
        stack_trigger(&mut state, source, 0);
        resolve_trigger(&mut state);
        replacement(&mut state, source, MovementDestination::Hand, action, self_replacement);
        state.drain_events();
        let selection = search_action(&state, goblin);
        let pending = bincode::serialize(&state.pending_matron_search).unwrap();
        let library = state.players[0].library.clone();
        let hand = state.players[0].hand.clone();
        rules::apply_action(&mut state, &selection);
        assert!(state.gameplay_stopped());
        assert_eq!(bincode::serialize(&state.pending_matron_search).unwrap(), pending);
        assert_eq!(state.players[0].library, library);
        assert_eq!(state.players[0].hand, hand);
        assert!(state.pending_movement.is_none());
        assert!(state.drain_events().iter().all(|event| !matches!(event,
            GameEvent::CardRevealed { .. } | GameEvent::LibraryShuffled { .. }
                | GameEvent::ZoneChange { .. })));
    }

    let (mut state, source) = game(2);
    let goblin = state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
    stack_trigger(&mut state, source, 0);
    resolve_trigger(&mut state);
    replacement(&mut state, source, MovementDestination::Hand,
        ReplacementAction::RedirectToZone(ZoneType::Exile), false);
    replacement(&mut state, source, MovementDestination::Hand,
        ReplacementAction::RedirectToZone(ZoneType::Library), false);
    state.drain_events();
    let pending = bincode::serialize(&state.pending_matron_search).unwrap();
    let selection = search_action(&state, goblin);
    rules::apply_action(&mut state, &selection);
    assert!(state.gameplay_stopped());
    assert_eq!(bincode::serialize(&state.pending_matron_search).unwrap(), pending,
        "competing ordinary replacements retain the owned trigger continuation");
    assert!(state.players[0].library.contains(&goblin));
    assert!(state.drain_events().is_empty());
}

#[test]
fn matron_window_exhaustion_keeps_the_trigger_and_pre_resolution_state() {
    let (mut state, source) = game(2);
    state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
    stack_trigger(&mut state, source, 0);
    state.next_movement_window = u64::MAX;
    state.drain_events();
    let stack = bincode::serialize(&state.stack).unwrap();
    let players = bincode::serialize(&state.players).unwrap();
    let objects = bincode::serialize(&state.objects).unwrap();
    resolve_trigger(&mut state);
    assert!(state.gameplay_stopped());
    assert_eq!(bincode::serialize(&state.stack).unwrap(), stack);
    assert_eq!(bincode::serialize(&state.players).unwrap(), players);
    assert_eq!(bincode::serialize(&state.objects).unwrap(), objects);
    assert!(state.pending_matron_search.is_none());
    assert!(state.pending_movement.is_none());
    assert!(state.trigger_order_resume.is_none());
    assert!(state.drain_events().is_empty());
}

#[test]
fn private_search_and_both_continuation_stages_roundtrip() {
    let (mut state, source) = game(2);
    let zada = state.create_card_in_zone(sample::ids::ZADA_HEDRON_GRINDER, 0, ZoneType::Library).unwrap();
    state.designate_commander(zada, 0).unwrap();
    let other = state.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
    stack_trigger(&mut state, source, 0);
    resolve_trigger(&mut state);
    assert_eq!(state.visible_state(0).matron_search_wait, state.visible_state(1).matron_search_wait);
    let mut reordered = state.clone();
    reordered.players[0].library.swap(0, 1);
    let left = InformationSet::from_view(&state.visible_state(1), state.card_db()).unwrap();
    let right = InformationSet::from_view(&reordered.visible_state(1), reordered.card_db()).unwrap();
    assert_eq!(left.hash_value(), right.hash_value());
    for mut restored in restored_variants(&state) {
        let action = search_action(&restored, zada);
        let canonical = canonicalize(&action, &restored).unwrap();
        assert_eq!(reconstruct(&canonical, &restored, 0), Ok(Some(action.clone())));
        rules::apply_action(&mut restored, &action);
        assert!(restored.pending_movement.is_some());
        for mut movement in restored_variants(&restored) {
            let answer = commander_answer(&movement, false);
            rules::apply_action(&mut movement, &answer);
            assert!(movement.players[0].hand.contains(&zada));
            assert!(movement.players[0].library.contains(&other));
            assert!(movement.pending_movement.is_none());
        }
    }
}

#[test]
fn snapshot_restore_into_an_advanced_state_rebuilds_matron_deferral_and_validates_public_reveals() {
    let (mut pending, source) = game(2);
    let goblin = pending.create_card_in_zone(sample::ids::GOBLIN_GUIDE, 0, ZoneType::Library).unwrap();
    stack_trigger(&mut pending, source, 0);
    resolve_trigger(&mut pending);
    let snapshot = pending.snapshot();

    let mut advanced = GameState::new_commander(2);
    advanced.card_db = pending.card_db.clone();
    advanced.phase = Phase::Cleanup;
    for _ in 0..4 {
        advanced.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Graveyard).unwrap();
    }
    advanced.restore(snapshot).unwrap();
    assert!(advanced.pending_matron_search.is_some());
    assert!(legal_actions(&advanced).iter().any(|action| matches!(action,
        Action::ChooseGoblinMatronCard { object, .. } if object.id == goblin)));

    let mut settled = pending;
    let selection = search_action(&settled, goblin);
    rules::apply_action(&mut settled, &selection);
    let mut invalid = settled.clone();
    invalid.revealed_hand_cards.push(mtg_gto::card::ExactObjectRef {
        id: goblin,
        generation: invalid.objects[&goblin].zone_change_count + 1,
    });
    let bad_snapshot = invalid.snapshot();
    let mut receiver = GameState::new_commander(2);
    receiver.card_db = settled.card_db.clone();
    assert!(receiver.restore(bad_snapshot).is_err());
}

#[test]
fn json_and_bincode_reject_tampered_matron_search_and_child_entries() {
    let (mut search, source) = game(2);
    let commander = search.create_card_in_zone(sample::ids::ZADA_HEDRON_GRINDER, 0, ZoneType::Library).unwrap();
    search.designate_commander(commander, 0).unwrap();
    stack_trigger(&mut search, source, 0);
    resolve_trigger(&mut search);

    let mut json = serde_json::to_value(&search).unwrap();
    json["pending_matron_search"]["resolving_entry"]["source"]["TriggeredAbility"]["ability_index"] = serde_json::json!(1);
    assert!(serde_json::from_value::<GameState>(json).is_err());

    let mut binary_search = search.clone();
    let future_generation = binary_search.objects[&source].zone_change_count + 1;
    let entry = binary_search.pending_matron_search.as_mut().unwrap().resolving_entry.as_mut().unwrap();
    let StackSource::TriggeredAbility { context, .. } = &mut entry.source else { unreachable!() };
    context.source_generation = future_generation;
    assert!(bincode::deserialize::<GameState>(&bincode::serialize(&binary_search).unwrap()).is_err());

    let selection = search_action(&search, commander);
    rules::apply_action(&mut search, &selection);
    assert!(search.pending_movement.is_some());
    let mut child_json = serde_json::to_value(&search).unwrap();
    child_json["pending_movement"]["resolving_entry"]["source"]["TriggeredAbility"]["context"]["source_card_id"] =
        serde_json::json!(sample::ids::MOUNTAIN);
    assert!(serde_json::from_value::<GameState>(child_json).is_err());

    let mut binary_child = search.clone();
    binary_child.pending_movement.as_mut().unwrap().resolving_source.as_mut().unwrap().generation += 1;
    assert!(bincode::deserialize::<GameState>(&bincode::serialize(&binary_child).unwrap()).is_err());
}

#[test]
fn malformed_restoration_and_non_goblin_selection_are_rejected() {
    let (mut state, source) = game(2);
    let mountain = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library).unwrap();
    stack_trigger(&mut state, source, 0);
    resolve_trigger(&mut state);
    let before = bincode::serialize(&state).unwrap();
    let window = state.pending_matron_search.as_ref().unwrap().window;
    let object = state.exact_object(mountain).unwrap();
    rules::apply_action(&mut state, &Action::ChooseGoblinMatronCard {
        window,
        object,
    });
    assert_eq!(bincode::serialize(&state).unwrap(), before);
    let mut value = serde_json::to_value(&state).unwrap();
    value["pending_matron_search"]["controller"] = serde_json::json!(9);
    assert!(serde_json::from_value::<GameState>(value).is_err());
}

#[test]
fn nested_or_synthetic_matron_search_rejects_before_an_effect_prefix() {
    const SYNTHETIC: u64 = 997_320;
    let mut database = sample::build_sample_db();
    let effect = mtg_gto::card::Effect::Multiple(vec![
        mtg_gto::card::Effect::GainLife { amount: 5 },
        mtg_gto::card::Effect::GoblinMatronSearch,
    ]);
    database.insert(mtg_gto::card::CardDef {
        id: SYNTHETIC,
        name: "Unsupported nested search".into(),
        card_types: vec![mtg_gto::card::CardType::Creature],
        power: Some(1),
        toughness: Some(1),
        triggered_abilities: vec![mtg_gto::card::TriggeredAbility {
            trigger: mtg_gto::card::TriggerCondition::EntersBattlefield,
            effect: effect.clone(),
            description: "unsupported".into(),
        }],
        ..Default::default()
    });
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(database));
    state.phase = Phase::PreCombatMain;
    let source = state.create_card_in_zone(SYNTHETIC, 0, ZoneType::Battlefield).unwrap();
    let id = state.new_stack_id();
    state.stack.push(StackEntry {
        id,
        controller: 0,
        source: StackSource::TriggeredAbility {
            source_id: source,
            ability_index: 0,
            context: Box::new(TriggerContext {
                source_card_id: SYNTHETIC,
                source_generation: state.objects[&source].zone_change_count,
                effect,
                cast_spell: None,
                zone_transition: None,
            }),
        },
        targets: vec![],
        target_generations: vec![],
    });
    let life = state.players[0].life;
    resolve_trigger(&mut state);
    assert!(state.gameplay_stopped());
    assert_eq!(state.players[0].life, life);
    assert!(state.stack.iter().any(|entry| entry.id == id));
    assert!(state.pending_matron_search.is_none());
}
