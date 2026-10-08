//! Synthetic tests for durable triggers and last-known cast-spell information.
use std::sync::Arc;

use mtg_gto::action::{canonical::{canonicalize, resolve}, Action};
use mtg_gto::card::{CardDef, CardType, Effect, KeywordAbility, TargetSpec, TriggerCondition, TriggeredAbility, ZoneType};
use mtg_gto::game::{CardDatabase, GameState, Phase, StackSource, Target};
use mtg_gto::info_set::InformationSet;
use mtg_gto::mana::ManaCost;
use mtg_gto::rules::{self, copy_spell_snapshot, copy_stack_spell, CopyTargetPolicy};

const WATCHER: u64 = 981001;
const SPELL: u64 = 981002;
const CREATURE: u64 = 981003;
const REPLACEMENT: u64 = 981004;
const COUNTER: u64 = 981005;
const COPY_SOURCE: u64 = 981006;
const DIES_SOURCE: u64 = 981007;
const FLASHBACK_SPELL: u64 = 981008;

fn database() -> Arc<CardDatabase> {
    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: WATCHER, name: "Cast watcher".into(), card_types: vec![CardType::Creature],
        power: Some(1), toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::YouCastSpell,
            effect: Effect::GainLife { amount: 3 },
            description: "gain three".into(),
        }], ..Default::default()
    });
    db.insert(CardDef {
        id: SPELL, name: "Synthetic haste and draw".into(),
        card_types: vec![CardType::Instant], mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::Multiple(vec![
            Effect::GainKeywordUntilEOT { keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature },
            Effect::DrawCards { count: 1 },
        ])), ..Default::default()
    });
    db.insert(CardDef {
        id: CREATURE, name: "Target creature".into(),
        card_types: vec![CardType::Creature], power: Some(1), toughness: Some(1),
        ..Default::default()
    });
    db.insert(CardDef {
        id: REPLACEMENT, name: "Changed incarnation".into(),
        card_types: vec![CardType::Creature], power: Some(1), toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::YouCastSpell,
            effect: Effect::GainLife { amount: 99 },
            description: "new instruction".into(),
        }], ..Default::default()
    });
    db.insert(CardDef {
        id: COUNTER, name: "Synthetic counter".into(),
        card_types: vec![CardType::Instant], mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::Counter { target: TargetSpec::AnySpell }),
        ..Default::default()
    });
    db.insert(CardDef {
        id: COPY_SOURCE, name: "Copy source".into(),
        card_types: vec![CardType::Creature], power: Some(1), toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::ALandYouControlEnters,
            effect: Effect::CreateTokenCopyOfSource,
            description: "copy source".into(),
        }], ..Default::default()
    });
    db.insert(CardDef {
        id: DIES_SOURCE, name: "Dies source".into(),
        card_types: vec![CardType::Creature], power: Some(1), toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::Dies,
            effect: Effect::CreateTokenCopyOfSource,
            description: "copy dying source".into(),
        }], ..Default::default()
    });
    db.insert(CardDef {
        id: FLASHBACK_SPELL, name: "Graveyard spell".into(),
        card_types: vec![CardType::Instant], mana_cost: Some(ManaCost::zero()),
        flashback_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::GainLife { amount: 1 }), ..Default::default()
    });
    Arc::new(db)
}

fn setup() -> (GameState, u64, u64) {
    let mut state = GameState::new(2);
    state.card_db = Some(database());
    state.phase = Phase::PreCombatMain;
    state.active_player = 0;
    state.priority_player = 0;
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let creature = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    for _ in 0..3 { state.create_card_in_zone(CREATURE, 0, ZoneType::Library); }
    (state, watcher, creature)
}

fn cast(state: &mut GameState, creature: u64) -> (u64, u64) {
    let card = state.create_card_in_zone(SPELL, 0, ZoneType::Hand);
    rules::apply_action(state, &Action::CastSpell {
        object_id: card, targets: vec![Target::Object(creature)],
    });
    let id = state.stack.iter().find(|entry| matches!(entry.source, StackSource::Spell(_))).unwrap().id;
    (card, id)
}

fn resolve_top(state: &mut GameState) {
    rules::apply_action(state, &Action::PassPriority);
    rules::apply_action(state, &Action::PassPriority);
}

#[test]
fn cast_trigger_captures_live_spell_without_making_it_a_target() {
    let (mut state, watcher, creature) = setup();
    let (_, original) = cast(&mut state, creature);
    let trigger = state.stack.last().unwrap();
    assert!(trigger.targets.is_empty());
    let StackSource::TriggeredAbility { context, .. } = &trigger.source else { panic!("missing trigger") };
    assert_eq!(context.source_card_id, WATCHER);
    assert_eq!(context.source_generation, state.objects[&watcher].zone_change_count);
    assert!(matches!(context.effect, Effect::GainLife { amount: 3 }));
    let spell = context.cast_spell.as_ref().unwrap();
    assert_eq!(spell.stack_id, original);
    assert_eq!(spell.definition.id, SPELL);
    assert_eq!(spell.controller, 0);
    assert_eq!(spell.targets, vec![Target::Object(creature)]);
    assert_eq!(spell.target_generations, vec![Some(state.objects[&creature].zone_change_count)]);
}

#[test]
fn original_departure_does_not_prevent_snapshot_copy_or_resolution() {
    let (mut state, _, creature) = setup();
    let (card, original) = cast(&mut state, creature);
    let snapshot = match &state.stack.last().unwrap().source {
        StackSource::TriggeredAbility { context, .. } => context.cast_spell.as_ref().unwrap().clone(),
        _ => panic!("missing trigger"),
    };
    state.move_object(card, ZoneType::Stack, ZoneType::Graveyard);
    state.objects.get_mut(&card).unwrap().card_def_id = REPLACEMENT;
    assert!(!state.stack.iter().any(|entry| entry.id == original));
    assert_eq!(snapshot.definition.id, SPELL);
    state.drain_events();
    let casts = state.spells_cast_this_turn;
    let objects = state.objects.len();
    let next_object = state.next_object_id;
    let library = state.players[0].library.len();
    let copy = copy_spell_snapshot(&mut state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
    assert_ne!(copy, original);
    assert!(matches!(state.stack.last().unwrap().source, StackSource::SpellCopy { .. }));
    assert_eq!(state.objects.len(), objects);
    assert_eq!(state.next_object_id, next_object);
    assert_eq!(state.spells_cast_this_turn, casts);
    assert!(state.drain_events().is_empty());
    resolve_top(&mut state);
    assert!(state.has_keyword(creature, KeywordAbility::Haste));
    assert_eq!(state.players[0].library.len(), library - 1);
    assert_eq!(state.players[0].graveyard.iter().filter(|&&id| id == card).count(), 1);
    assert!(!state.stack.iter().any(|entry| entry.id == copy));
}

#[test]
fn countered_original_still_has_usable_trigger_snapshot() {
    let (mut state, _, creature) = setup();
    let (original_card, original) = cast(&mut state, creature);
    let snapshot = match &state.stack.last().unwrap().source {
        StackSource::TriggeredAbility { context, .. } => context.cast_spell.as_ref().unwrap().clone(),
        _ => panic!("missing trigger"),
    };
    state.priority_player = 1;
    let counter_card = state.create_card_in_zone(COUNTER, 1, ZoneType::Hand);
    rules::apply_action(&mut state, &Action::CastSpell {
        object_id: counter_card, targets: vec![Target::StackEntry(original)],
    });
    resolve_top(&mut state);
    assert!(!state.stack.iter().any(|entry| entry.id == original));
    assert!(state.players[0].graveyard.contains(&original_card));
    let copy = copy_spell_snapshot(&mut state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
    assert_ne!(copy, original);
    resolve_top(&mut state);
    assert!(state.has_keyword(creature, KeywordAbility::Haste));
}

#[test]
fn trigger_survives_source_leaving_or_changing_control() {
    for mode in 0..3 {
        let (mut state, watcher, creature) = setup();
        let _ = cast(&mut state, creature);
        let before = state.players[0].life;
        let other_before = state.players[1].life;
        let generation = state.objects[&watcher].zone_change_count;
        match mode {
            0 => {
                state.objects.get_mut(&watcher).unwrap().is_token = true;
                state.move_object(watcher, ZoneType::Battlefield, ZoneType::Graveyard);
                assert!(state.players[0].graveyard.contains(&watcher));
                rules::check_state_based_actions(&mut state);
                assert!(!state.objects.contains_key(&watcher));
            }
            1 => state.objects.get_mut(&watcher).unwrap().controller = 1,
            _ => {
                state.move_object(watcher, ZoneType::Battlefield, ZoneType::Graveyard);
                state.move_object(watcher, ZoneType::Graveyard, ZoneType::Battlefield);
                let inst = state.objects.get_mut(&watcher).unwrap();
                inst.card_def_id = REPLACEMENT;
                inst.controller = 1;
                assert!(inst.zone_change_count > generation);
            }
        }
        resolve_top(&mut state);
        assert_eq!(state.players[0].life, before + 3, "mode {mode}");
        assert_eq!(state.players[1].life, other_before, "mode {mode}");
    }
}

#[test]
fn queued_trigger_survives_source_leaving_and_ordering() {
    let (mut state, watcher, creature) = setup();
    let mut db = (*state.card_db()).clone();
    let mut def = db.get(WATCHER).unwrap().clone();
    def.triggered_abilities.push(TriggeredAbility {
        trigger: TriggerCondition::YouCastSpell,
        effect: Effect::GainLife { amount: 5 }, description: "gain five".into(),
    });
    db.insert(def);
    state.card_db = Some(Arc::new(db));
    cast(&mut state, creature);
    assert_eq!(state.pending_triggers.len(), 2);
    assert!(state.pending_triggers.iter().all(|t| t.context.cast_spell.is_some()));
    state.objects.get_mut(&watcher).unwrap().is_token = true;
    state.move_object(watcher, ZoneType::Battlefield, ZoneType::Graveyard);
    let pending_info = InformationSet::from_view(&state.visible_state(0), state.card_db()).unwrap();
    assert_eq!(pending_info.pending_cast_spells.len(), 2);
    assert!(pending_info.pending_cast_spells.iter().all(|spell| spell.as_ref().unwrap().card_id == SPELL));
    let saved = state.snapshot();
    let json: GameState = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    let binary: GameState = bincode::deserialize(&bincode::serialize(&state).unwrap()).unwrap();
    for restored in [state.clone(), json, binary] {
        assert_eq!(restored.pending_triggers.len(), 2);
        assert!(restored.pending_triggers.iter().all(|t| t.context.cast_spell.as_ref().unwrap().definition.id == SPELL));
    }
    state.restore(saved).unwrap();
    let before = state.players[0].life;
    let order = Action::OrderTriggers {
        ordering: vec![(watcher, 0), (watcher, 1)],
    };
    let canonical = canonicalize(&order, &state).unwrap();
    assert_eq!(resolve(&canonical, &state, 0).unwrap(), Some(order.clone()));
    rules::apply_action(&mut state, &order);
    assert!(state.pending_triggers.is_empty());
    resolve_top(&mut state);
    resolve_top(&mut state);
    assert_eq!(state.players[0].life, before + 8);
}

#[test]
fn trigger_context_persists_and_information_set_uses_relationship_not_raw_id() {
    let (mut state, _, creature) = setup();
    cast(&mut state, creature);
    let original = state.stack[0].id;
    let expected = match &state.stack[1].source {
        StackSource::TriggeredAbility { context, .. } => context.cast_spell.as_ref().unwrap().clone(),
        _ => panic!("missing trigger"),
    };
    let info = InformationSet::from_view(&state.visible_state(0), state.card_db()).unwrap();
    assert_eq!(info.stack_entries[1].cast_spell.as_ref().unwrap().live_stack_position, Some(0));
    assert!(info.stack_entries[1].target_summary.is_empty());
    let mut equivalent = state.clone();
    equivalent.stack[0].id += 100;
    if let StackSource::TriggeredAbility { context, .. } = &mut equivalent.stack[1].source {
        context.cast_spell.as_mut().unwrap().stack_id += 100;
    }
    equivalent.stack[1].id += 100;
    equivalent.next_stack_id += 100;
    let equivalent_info = InformationSet::from_view(&equivalent.visible_state(0), equivalent.card_db()).unwrap();
    assert_eq!(info.hash_value(), equivalent_info.hash_value());

    let snapshot = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let mut restored = vec![state.clone(), serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap()];
    let physical = match state.stack[0].source { StackSource::Spell(id) => id, _ => unreachable!() };
    state.move_object(physical, ZoneType::Stack, ZoneType::Graveyard);
    state.restore(snapshot).unwrap();
    restored.push(state);
    for mut game in restored {
        game.card_db = Some(database());
        let cast = match &game.stack[1].source {
            StackSource::TriggeredAbility { context, .. } => context.cast_spell.as_ref().unwrap(),
            _ => panic!("missing trigger"),
        };
        assert_eq!(cast.stack_id, original);
        assert_eq!(cast.definition.id, expected.definition.id);
        assert_eq!(cast.targets, expected.targets);
        assert_eq!(cast.target_generations, expected.target_generations);
        assert!(game.new_stack_id() > game.stack[1].id);
    }
}

#[test]
fn live_stack_copy_api_still_works() {
    let (mut state, _, creature) = setup();
    let (_, original) = cast(&mut state, creature);
    let copy = copy_stack_spell(&mut state, original, 0, CopyTargetPolicy::Preserve).unwrap();
    assert!(matches!(state.stack.last().unwrap().source, StackSource::SpellCopy { .. }));
    assert_ne!(copy, original);
}

#[test]
fn copy_of_trigger_source_uses_last_known_source_after_departure() {
    let (mut state, _, _) = setup();
    let source = state.create_card_in_zone(COPY_SOURCE, 0, ZoneType::Battlefield);
    assert!(rules::fire_triggers(&mut state, TriggerCondition::ALandYouControlEnters, Some(source)));
    state.move_object(source, ZoneType::Battlefield, ZoneType::Graveyard);
    resolve_top(&mut state);
    assert!(state.battlefield.iter().any(|id| state.objects[id].card_def_id == COPY_SOURCE));
}

#[test]
fn copy_of_trigger_source_uses_live_or_captured_incarnation() {
    for mode in 0..3 {
        let (mut state, _, _) = setup();
        let source = state.create_card_in_zone(COPY_SOURCE, 0, ZoneType::Battlefield);
        assert!(rules::fire_triggers(&mut state, TriggerCondition::ALandYouControlEnters, Some(source)));
        match mode {
            0 => {}, // original remains live
            1 => {
                state.move_object(source, ZoneType::Battlefield, ZoneType::Graveyard);
                state.move_object(source, ZoneType::Graveyard, ZoneType::Battlefield);
                state.objects.get_mut(&source).unwrap().card_def_id = REPLACEMENT;
            },
            _ => {
                state.objects.get_mut(&source).unwrap().is_token = true;
                state.move_object(source, ZoneType::Battlefield, ZoneType::Graveyard);
                assert!(state.players[0].graveyard.contains(&source));
                rules::check_state_based_actions(&mut state);
                assert!(!state.objects.contains_key(&source));
            },
        }
        let before = state.battlefield.iter().filter(|&&id| state.objects[&id].card_def_id == COPY_SOURCE).count();
        resolve_top(&mut state);
        let after = state.battlefield.iter().filter(|&&id| state.objects[&id].card_def_id == COPY_SOURCE).count();
        assert_eq!(after, before + 1, "mode {mode}");
        assert!(state.battlefield.iter().any(|&id| id != source && state.objects[&id].card_def_id == COPY_SOURCE && state.objects[&id].is_token));
    }
}

#[test]
fn self_dies_captures_pre_move_incarnation_and_uses_last_known_source() {
    for returns in [false, true] {
        let (mut state, _, _) = setup();
        let source = state.create_card_in_zone(DIES_SOURCE, 0, ZoneType::Battlefield);
        let generation = state.objects[&source].zone_change_count;
        state.objects.get_mut(&source).unwrap().damage_marked = 1;
        rules::check_state_based_actions(&mut state);
        let trigger = state.stack.last().expect("self-Dies trigger");
        let StackSource::TriggeredAbility { context, .. } = &trigger.source else { panic!("not a trigger") };
        assert_eq!(context.source_generation, generation);
        assert_eq!(context.source_card_id, DIES_SOURCE);
        assert!(state.players[0].graveyard.contains(&source));
        assert!(state.objects[&source].zone_change_count > generation);
        if returns {
            state.move_object(source, ZoneType::Graveyard, ZoneType::Battlefield);
        }
        state.objects.get_mut(&source).unwrap().card_def_id = REPLACEMENT;
        resolve_top(&mut state);
        assert!(state.battlefield.iter().any(|&id| id != source && state.objects[&id].card_def_id == DIES_SOURCE && state.objects[&id].is_token));
    }
}

#[test]
fn departed_historical_target_summary_ignores_runtime_object_id() {
    fn history(dummy_count: usize, target_def: u64) -> (GameState, u64) {
        let (mut state, _, _) = setup();
        for _ in 0..dummy_count {
            let id = state.create_card_in_zone(CREATURE, 1, ZoneType::Exile);
            state.players[1].exile.retain(|&x| x != id);
            state.objects.remove(&id);
        }
        let target = state.create_card_in_zone(target_def, 1, ZoneType::Battlefield);
        let (spell_card, _) = cast(&mut state, target);
        state.move_object(target, ZoneType::Battlefield, ZoneType::Hand);
        state.move_object(spell_card, ZoneType::Stack, ZoneType::Graveyard);
        (state, target)
    }
    let (first, first_id) = history(0, CREATURE);
    let (second, second_id) = history(3, CREATURE);
    assert_ne!(first_id, second_id);
    let a = InformationSet::from_view(&first.visible_state(0), first.card_db()).unwrap();
    let b = InformationSet::from_view(&second.visible_state(0), second.card_db()).unwrap();
    assert_eq!(a.stack_entries[0].cast_spell.as_ref().unwrap().target_summary,
        b.stack_entries[0].cast_spell.as_ref().unwrap().target_summary);
    assert_eq!(a.hash_value(), b.hash_value());

    let (different, _) = history(0, REPLACEMENT);
    let c = InformationSet::from_view(&different.visible_state(0), different.card_db()).unwrap();
    assert_ne!(a.stack_entries[0].cast_spell.as_ref().unwrap().target_summary,
        c.stack_entries[0].cast_spell.as_ref().unwrap().target_summary);
}

#[test]
fn command_and_graveyard_casts_capture_durable_snapshots() {
    for from_command in [true, false] {
        let (mut state, _, _) = setup();
        let (card, action) = if from_command {
            let card = state.create_card_in_zone(CREATURE, 0, ZoneType::Command);
            state.players[0].commander_object_id = Some(card);
            (card, Action::CastCommander { object_id: card, targets: vec![] })
        } else {
            let card = state.create_card_in_zone(FLASHBACK_SPELL, 0, ZoneType::Graveyard);
            (card, Action::CastFromGraveyard { object_id: card, targets: vec![] })
        };
        rules::apply_action(&mut state, &action);
        let original = state.stack.iter().find(|e| matches!(e.source, StackSource::Spell(id) if id == card)).expect("cast spell");
        let trigger = state.stack.iter().find(|e| matches!(e.source, StackSource::TriggeredAbility { .. })).expect("cast trigger");
        let StackSource::TriggeredAbility { context, .. } = &trigger.source else { unreachable!() };
        let snapshot = context.cast_spell.as_ref().expect("cast snapshot");
        assert_eq!(snapshot.stack_id, original.id);
        assert_eq!(snapshot.definition.id, if from_command { CREATURE } else { FLASHBACK_SPELL });
        assert_eq!(snapshot.controller, 0);
    }
}

#[test]
fn historical_stack_id_survives_restore_without_allocator_collision() {
    let (mut state, _, creature) = setup();
    let (_, original) = cast(&mut state, creature);
    let physical = match state.stack[0].source { StackSource::Spell(id) => id, _ => unreachable!() };
    state.move_object(physical, ZoneType::Stack, ZoneType::Graveyard);
    let snapshot = state.snapshot();
    let json: GameState = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    let binary: GameState = bincode::deserialize(&bincode::serialize(&state).unwrap()).unwrap();
    for mut restored in [json, binary] {
        restored.card_db = Some(database());
        let historical = match &restored.stack[0].source {
            StackSource::TriggeredAbility { context, .. } => context.cast_spell.as_ref().unwrap().stack_id,
            _ => panic!("missing trigger"),
        };
        assert_eq!(historical, original);
        let fresh = restored.new_stack_id();
        assert_ne!(fresh, historical);
        assert!(restored.stack.iter().all(|entry| entry.id != fresh));
    }
    state.restore(snapshot).unwrap();
    assert_ne!(state.new_stack_id(), original);
}

#[test]
fn snapshot_preserve_keeps_stale_target_generation() {
    let (mut state, _, creature) = setup();
    cast(&mut state, creature);
    let snapshot = match &state.stack[1].source {
        StackSource::TriggeredAbility { context, .. } => context.cast_spell.as_ref().unwrap().clone(),
        _ => unreachable!(),
    };
    let old_generation = snapshot.target_generations[0];
    state.move_object(creature, ZoneType::Battlefield, ZoneType::Graveyard);
    state.move_object(creature, ZoneType::Graveyard, ZoneType::Battlefield);
    assert_ne!(Some(state.objects[&creature].zone_change_count), old_generation);
    let copy = copy_spell_snapshot(&mut state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
    let entry = state.stack.iter().find(|entry| entry.id == copy).unwrap();
    assert_eq!(entry.target_generations[0], old_generation);
    assert_eq!(snapshot.target_generations[0], old_generation);
}
