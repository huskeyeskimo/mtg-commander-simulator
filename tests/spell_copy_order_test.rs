//! Terminal spell-copy ordering, independent of any card or Zada trigger.
use std::sync::Arc;

use mtg_gto::action::{canonical::{canonicalize, resolve}, legal_actions, Action};
use mtg_gto::card::{CardDef, CardType, Effect, KeywordAbility, ManaAbility, TargetSpec, TriggerCondition, TriggeredAbility, ZoneType};
use mtg_gto::game::{CardDatabase, GameState, PendingTrigger, PendingTutor, Phase, PreparedSpellCopy, StackSource, Target, TriggerContext};
use mtg_gto::info_set::{BucketedAbstraction, CardAwareBucketedAbstraction, InfoSetAbstraction, InformationSet};
use mtg_gto::mana::{Color, ManaCost};
use mtg_gto::rules::{self, begin_terminal_copy_batch, copy_spell_snapshot, copy_stack_spell,
    prepare_spell_copy, snapshot_stack_spell, CopyBatchOutcome, CopyError, CopyTargetPolicy};

const DRAW: u64 = 981001;
const HASTE_DRAW: u64 = 981002;
const CREATURE: u64 = 981003;

fn game() -> GameState {
    let mut db = CardDatabase::new();
    db.insert(CardDef { id: DRAW, name: "Draw one".into(), card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::DrawCards { count: 1 }), ..Default::default() });
    db.insert(CardDef { id: HASTE_DRAW, name: "Haste and draw".into(), card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Multiple(vec![Effect::GainKeywordUntilEOT {
            keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature },
            Effect::DrawCards { count: 1 }])), ..Default::default() });
    db.insert(CardDef { id: CREATURE, name: "Creature".into(), card_types: vec![CardType::Creature],
        power: Some(1), toughness: Some(1), ..Default::default() });
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    for _ in 0..16 { state.create_card_in_zone(CREATURE, 0, ZoneType::Library); }
    state
}

fn source(state: &mut GameState, card: u64, targets: Vec<Target>) -> u64 {
    let object = state.create_card_in_zone(card, 0, ZoneType::Hand);
    rules::apply_action(state, &Action::CastSpell { object_id: object, targets });
    state.stack.last().unwrap().id
}

fn prepared(state: &GameState, stack_id: u64, controller: usize, policy: CopyTargetPolicy) -> PreparedSpellCopy {
    let snapshot = snapshot_stack_spell(state, stack_id).unwrap();
    prepare_spell_copy(state, &snapshot, controller, policy).unwrap()
}

fn begin(state: &mut GameState, items: Vec<PreparedSpellCopy>) -> CopyBatchOutcome {
    begin_terminal_copy_batch(state, items, true).unwrap()
}

fn pass_twice(state: &mut GameState) {
    rules::apply_action(state, &Action::PassPriority);
    rules::apply_action(state, &Action::PassPriority);
}

#[test]
fn zero_one_and_two_copy_batches() {
    let mut state = game();
    let original = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, original, 0, CopyTargetPolicy::Preserve);
    let base_len = state.stack.len();
    assert_eq!(begin(&mut state, vec![]), CopyBatchOutcome::Empty);
    assert!(state.pending_copy_order.is_none());
    assert_eq!(state.stack.len(), base_len);
    let first = begin(&mut state, vec![item.clone()]);
    assert!(matches!(first, CopyBatchOutcome::Committed(ids) if ids.len() == 1));
    assert!(state.pending_copy_order.is_none());
    assert_eq!(state.stack.len(), base_len + 1);
    let prior = state.stack.len();
    assert_eq!(begin(&mut state, vec![item.clone(), item]), CopyBatchOutcome::Pending);
    assert_eq!(state.stack.len(), prior); // no partial insertion
    assert_eq!(legal_actions(&state), vec![Action::ChooseNextCopy { item_index: 0 },
        Action::ChooseNextCopy { item_index: 1 }]);
}

#[test]
fn bottom_to_top_choice_reverses_resolution_and_allocates_fresh_ids() {
    for first_choice in [0, 1] {
        let mut state = game();
        let original = source(&mut state, DRAW, vec![]);
        let snapshot = snapshot_stack_spell(&state, original).unwrap();
        let mut second_snapshot = snapshot.clone();
        second_snapshot.definition.name = "Other draw".into();
        let first = prepare_spell_copy(&state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
        let second = prepare_spell_copy(&state, &second_snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
        let next = state.next_stack_id;
        begin(&mut state, vec![first, second]);
        rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: first_choice });
        assert!(state.pending_copy_order.is_none());
        assert_eq!(state.stack.iter().map(|e| e.id).collect::<Vec<_>>(), vec![original, next, next + 1]);
        let names: Vec<_> = state.stack[1..].iter().map(|entry| match &entry.source {
            StackSource::SpellCopy { definition } => definition.name.as_str(), _ => panic!() }).collect();
        assert_eq!(names, if first_choice == 0 { vec!["Draw one", "Other draw"] }
            else { vec!["Other draw", "Draw one"] });
        let library = state.players[0].library.len();
        pass_twice(&mut state);
        assert_eq!(state.players[0].library.len(), library - 1);
        assert_eq!(state.stack.len(), 2);
        assert_eq!(state.next_stack_id, next + 2);
    }
}

#[test]
fn invalid_choices_and_commit_preflight_do_not_mutate() {
    let mut state = game();
    let id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, id, 0, CopyTargetPolicy::Preserve);
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(begin_terminal_copy_batch(&mut state, vec![item.clone(), item.clone()], false),
        Err(CopyError::NonTerminalBatch));
    let other = prepared(&state, id, 1, CopyTargetPolicy::Preserve);
    assert_eq!(begin_terminal_copy_batch(&mut state, vec![item.clone(), other], true), Err(CopyError::MixedControllers));
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    state.next_stack_id = u64::MAX - 1;
    let overflow = serde_json::to_value(&state).unwrap();
    assert_eq!(begin_terminal_copy_batch(&mut state, vec![item.clone(), item.clone()], true),
        Err(CopyError::StackIdExhausted));
    assert_eq!(serde_json::to_value(&state).unwrap(), overflow);
    state.next_stack_id = id + 1;
    begin(&mut state, vec![item.clone(), item.clone(), item]);
    let pending = serde_json::to_value(&state).unwrap();
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 9 });
    assert_eq!(serde_json::to_value(&state).unwrap(), pending);
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    let selected = serde_json::to_value(&state).unwrap();
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    assert_eq!(serde_json::to_value(&state).unwrap(), selected);
    state.next_stack_id += 1; // corrupted commitment state must not partly insert
    let corrupted = serde_json::to_value(&state).unwrap();
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 1 });
    assert_eq!(serde_json::to_value(&state).unwrap(), corrupted);

    let mut malformed = selected;
    malformed["pending_copy_order"]["selected_order"] = serde_json::json!([0, 0]);
    let mut malformed_state: GameState = serde_json::from_value(malformed).unwrap();
    let before = serde_json::to_value(&malformed_state).unwrap();
    rules::apply_action(&mut malformed_state, &Action::ChooseNextCopy { item_index: 1 });
    assert_eq!(serde_json::to_value(&malformed_state).unwrap(), before);
}

#[test]
fn mandatory_resolution_lock_blocks_all_bypass_paths() {
    let mut state = game();
    let id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, id, 0, CopyTargetPolicy::Preserve);
    begin(&mut state, vec![item.clone(), item]);
    state.players[0].life = 0; // SBA would end the game if processed now
    let before = serde_json::to_value(&state).unwrap();
    for action in [Action::PassPriority, Action::EndTurn, Action::Concede,
        Action::CastSpell { object_id: 999, targets: vec![] }] {
        rules::apply_action(&mut state, &action);
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
    rules::check_state_based_actions(&mut state);
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    assert!(!state.game_over);
    assert_eq!(state.stack.len(), 1);
    rules::fast_forward_goldfish_turn_until_copy_choice(&mut state, 0);
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    assert!(state.game_over); // SBA resumes only after complete insertion
    assert_eq!(state.next_stack_id, id + 3);
}

#[test]
fn no_cast_consequences_and_preserved_generation_after_source_leaves() {
    let mut state = game();
    let creature = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let original_id = source(&mut state, HASTE_DRAW, vec![Target::Object(creature)]);
    let snapshot = snapshot_stack_spell(&state, original_id).unwrap();
    let generations = snapshot.target_generations.clone();
    let original_object = match state.stack[0].source { StackSource::Spell(id) => id, _ => panic!() };
    state.move_object(original_object, ZoneType::Stack, ZoneType::Hand);
    state.move_object(creature, ZoneType::Battlefield, ZoneType::Hand);
    state.move_object(creature, ZoneType::Hand, ZoneType::Battlefield);
    let a = prepare_spell_copy(&state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
    let b = a.clone();
    let cast_count = state.spells_cast_this_turn;
    let object_count = state.objects.len();
    let events = state.drain_events();
    begin(&mut state, vec![a, b]);
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    assert_eq!(state.spells_cast_this_turn, cast_count);
    assert_eq!(state.objects.len(), object_count);
    assert!(state.drain_events().is_empty());
    assert_eq!(state.stack[0].target_generations, generations);
    assert_eq!(state.stack[1].target_generations, generations);
    assert!(!events.is_empty());
    let library = state.players[0].library.len();
    pass_twice(&mut state);
    pass_twice(&mut state);
    assert_eq!(state.players[0].library.len(), library);
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
}

#[test]
fn pending_state_survives_clone_snapshot_json_bincode() {
    let mut state = game();
    let id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, id, 0, CopyTargetPolicy::Preserve);
    begin(&mut state, vec![item.clone(), item.clone(), item]);
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 2 });
    let snap = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let bin = bincode::serialize(&state).unwrap();
    let mut restored = vec![state.clone(), serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&bin).unwrap()];
    state.restore(snap);
    restored.push(state);
    for mut candidate in restored {
        if candidate.card_db.is_none() { candidate.card_db = game().card_db; }
        assert_eq!(candidate.pending_copy_order.as_ref().unwrap().selected_order(), &[2]);
        let before = candidate.next_stack_id;
        rules::apply_action(&mut candidate, &Action::ChooseNextCopy { item_index: 0 });
        assert!(candidate.pending_copy_order.is_none());
        assert_eq!(candidate.stack.len(), 4);
        assert_eq!(candidate.next_stack_id, before + 3);
    }
}

#[test]
fn canonical_and_information_set_ignore_runtime_ids_but_track_choices_and_targets() {
    fn targeted_state(extra_ids: usize, swap: bool) -> GameState {
        let mut state = game();
        for _ in 0..extra_ids {
            state.new_object_id();
            state.new_stack_id();
        }
        let left = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let right = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let source_id = source(&mut state, HASTE_DRAW, vec![Target::Object(left)]);
        let left_copy = prepared(&state, source_id, 0, CopyTargetPolicy::Replace(vec![Target::Object(left)]));
        let right_copy = prepared(&state, source_id, 0, CopyTargetPolicy::Replace(vec![Target::Object(right)]));
        let third = prepared(&state, source_id, 0, CopyTargetPolicy::Replace(vec![Target::Object(left)]));
        if swap { begin(&mut state, vec![right_copy, left_copy, third]); }
        else { begin(&mut state, vec![left_copy, right_copy, third]); }
        state
    }
    let mut state = targeted_state(0, false);
    let action = Action::ChooseNextCopy { item_index: 1 };
    let canonical = canonicalize(&action, &state);
    assert_eq!(resolve(&canonical, &state, 0), Some(action.clone()));
    let equivalent = targeted_state(5, false);
    let info = InformationSet::from_view(&state.visible_state(0), state.card_db());
    let equivalent_info = InformationSet::from_view(&equivalent.visible_state(0), state.card_db());
    assert_eq!(info.pending_copy_order, equivalent_info.pending_copy_order);
    assert_eq!(info.hash_value(), equivalent_info.hash_value());
    assert_eq!(resolve(&canonical, &equivalent, 0), Some(action));
    let different = targeted_state(0, true);
    let different_info = InformationSet::from_view(&different.visible_state(0), different.card_db());
    assert_ne!(info.pending_copy_order, different_info.pending_copy_order);
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    let prefix_info = InformationSet::from_view(&state.visible_state(0), state.card_db());
    assert_ne!(info.pending_copy_order, prefix_info.pending_copy_order);
}

#[test]
fn large_batch_is_linear_and_automated_strategy_completes_it() {
    use mtg_gto::strategy::{GoldfishStrategy, GreedyStrategy, Strategy};
    let mut state = game();
    let id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, id, 0, CopyTargetPolicy::Preserve);
    begin(&mut state, vec![item; 12]);
    assert_eq!(legal_actions(&state).len(), 12);
    let strategy = GreedyStrategy;
    while state.pending_copy_order.is_some() {
        let action = strategy.choose_action(&state, 0);
        assert!(matches!(action, Action::ChooseNextCopy { .. }));
        rules::apply_action(&mut state, &action);
    }
    assert_eq!(state.stack.len(), 13);

    let mut goldfish = game();
    let id = source(&mut goldfish, DRAW, vec![]);
    let item = prepared(&goldfish, id, 0, CopyTargetPolicy::Preserve);
    begin(&mut goldfish, vec![item.clone(), item]);
    let action = GoldfishStrategy.choose_action(&goldfish, 0);
    assert_eq!(action, Action::ChooseNextCopy { item_index: 0 });
    rules::apply_action(&mut goldfish, &action);
    assert!(goldfish.pending_copy_order.is_none());
}

#[test]
fn fast_forward_pauses_for_human_on_opponents_turn_and_auto_mode_completes() {
    let mut state = game();
    let id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, id, 0, CopyTargetPolicy::Preserve);
    state.active_player = 1;
    begin(&mut state, vec![item.clone(), item]);
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(rules::fast_forward_goldfish_turn_until_copy_choice(&mut state, 0), 0);
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    let mut automated = state.clone();
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    assert_eq!(state.priority_player, 1); // active player receives priority after resolution
    let actions = rules::fast_forward_goldfish_turn(&mut automated);
    assert!(actions > 0);
    assert!(automated.pending_copy_order.is_none());
}

#[test]
fn deferred_triggers_flush_only_after_complete_batch_commit() {
    let mut state = game();
    let id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, id, 0, CopyTargetPolicy::Preserve);
    begin(&mut state, vec![item.clone(), item]);
    state.pending_triggers.push(PendingTrigger {
        source_id: 987654, ability_index: 0, controller: 0, targets: vec![],
        context: TriggerContext { source_card_id: CREATURE, source_generation: 0,
            effect: Effect::GainLife { amount: 1 }, cast_spell: None },
    });
    let stack_len = state.stack.len();
    rules::check_state_based_actions(&mut state);
    assert_eq!(state.stack.len(), stack_len);
    assert_eq!(state.pending_triggers.len(), 1);
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    assert!(state.pending_triggers.is_empty());
    assert_eq!(state.stack.len(), stack_len + 3);
    assert!(matches!(state.stack.last().unwrap().source, StackSource::TriggeredAbility { .. }));
}

#[test]
fn legacy_single_copy_apis_still_work() {
    let mut state = game();
    let id = source(&mut state, DRAW, vec![]);
    let snapshot = snapshot_stack_spell(&state, id).unwrap();
    let a = copy_stack_spell(&mut state, id, 0, CopyTargetPolicy::Preserve).unwrap();
    let b = copy_spell_snapshot(&mut state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
    assert_ne!(a, b);
    assert_eq!(state.stack.len(), 3);
}

fn trigger(source_id: u64, controller: usize, ability_index: usize) -> PendingTrigger {
    PendingTrigger { source_id, ability_index, controller, targets: vec![],
        context: TriggerContext { source_card_id: CREATURE, source_generation: 0,
            effect: Effect::GainLife { amount: 1 }, cast_spell: None } }
}

#[test]
fn resolution_grants_active_priority_unless_a_trigger_order_is_mandatory() {
    let mut plain = game();
    source(&mut plain, DRAW, vec![]);
    pass_twice(&mut plain);
    assert_eq!(plain.priority_player, plain.active_player);

    let mut state = game();
    const ETB: u64 = 981100;
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: ETB,
        name: "Two ETBs".into(), card_types: vec![CardType::Creature],
        power: Some(1), toughness: Some(1),
        triggered_abilities: (0..2).map(|i| TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::GainLife { amount: i + 1 }, description: format!("ETB {i}")
        }).collect(), ..Default::default() });
    let object = state.create_card_in_zone(ETB, 1, ZoneType::Hand);
    state.priority_player = 1;
    rules::apply_action(&mut state, &Action::CastSpell { object_id: object, targets: vec![] });
    pass_twice(&mut state);
    assert_eq!(state.pending_triggers.len(), 2);
    assert_eq!(state.priority_player, 1);
    let choices = legal_actions(&state);
    assert!(choices.iter().all(|action| matches!(action, Action::OrderTriggers { .. } | Action::Concede)));
    let order = choices.into_iter().find(|action| matches!(action, Action::OrderTriggers { .. })).unwrap();
    rules::apply_action(&mut state, &order);
    assert!(state.pending_triggers.is_empty());
    assert_eq!(state.priority_player, 0);
    assert_eq!(state.stack.len(), 2);
}

#[test]
fn nonactive_caster_retains_priority_after_ordering_cast_triggers_and_restore() {
    const WATCHER: u64 = 981104;
    let mut state = game();
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: WATCHER,
        name: "Cast watcher".into(), card_types: vec![CardType::Enchantment],
        triggered_abilities: (0..2).map(|i| TriggeredAbility {
            trigger: TriggerCondition::YouCastSpell,
            effect: Effect::GainLife { amount: i + 1 }, description: format!("Cast {i}")
        }).collect(), ..Default::default() });
    state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
    let spell = state.create_card_in_zone(DRAW, 1, ZoneType::Hand);
    state.priority_player = 1;
    rules::apply_action(&mut state, &Action::CastSpell { object_id: spell, targets: vec![] });
    assert_eq!(state.priority_player, 1);
    assert_eq!(state.pending_triggers.len(), 2);
    let snapshot = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let mut restored = vec![state.clone(), serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap()];
    state.restore(snapshot);
    restored.push(state);
    for mut candidate in restored {
        if candidate.card_db.is_none() { candidate.card_db = game().card_db; }
        let order = legal_actions(&candidate).into_iter()
            .find(|action| matches!(action, Action::OrderTriggers { .. })).unwrap();
        rules::apply_action(&mut candidate, &order);
        assert!(candidate.pending_triggers.is_empty());
        assert_eq!(candidate.priority_player, 1);
    }
}

#[test]
fn attacker_trigger_order_still_advances_to_blockers() {
    const ATTACKER: u64 = 981105;
    let mut state = game();
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: ATTACKER,
        name: "Attacker".into(), card_types: vec![CardType::Creature],
        power: Some(1), toughness: Some(1),
        triggered_abilities: (0..2).map(|i| TriggeredAbility {
            trigger: TriggerCondition::Attacks,
            effect: Effect::GainLife { amount: i + 1 }, description: format!("Attack {i}")
        }).collect(), ..Default::default() });
    let attacker = state.create_card_in_zone(ATTACKER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&attacker).unwrap().summoning_sick = false;
    state.phase = Phase::DeclareAttackers;
    rules::apply_action(&mut state, &Action::DeclareAttackers { attackers: vec![attacker] });
    assert_eq!(state.phase, Phase::DeclareAttackers);
    assert_eq!(state.pending_triggers.len(), 2);
    let order = legal_actions(&state).into_iter()
        .find(|action| matches!(action, Action::OrderTriggers { .. })).unwrap();
    rules::apply_action(&mut state, &order);
    assert_eq!(state.phase, Phase::DeclareBlockers);
    assert_eq!(state.priority_player, 1);
    assert!(state.pending_triggers.is_empty());
}

#[test]
fn copy_commit_keeps_nonactive_trigger_order_and_restores_priority_after_choice() {
    let mut state = game();
    let source_id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, source_id, 0, CopyTargetPolicy::Preserve);
    state.pending_triggers.extend([trigger(101, 1, 0), trigger(102, 1, 1)]);
    begin(&mut state, vec![item.clone(), item]);
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    assert_eq!(state.priority_player, 1);
    assert_eq!(state.pending_triggers.len(), 2);
    let order = legal_actions(&state).into_iter().find(|action| matches!(action, Action::OrderTriggers { .. })).unwrap();
    rules::apply_action(&mut state, &order);
    assert!(state.pending_triggers.is_empty());
    assert_eq!(state.priority_player, 0);
    assert_eq!(state.stack.len(), 5);
}

#[test]
fn copy_commit_sba_death_orders_nonactive_triggers_before_priority() {
    let mut state = game();
    const WATCHER: u64 = 981101;
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: WATCHER,
        name: "Watcher".into(), card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![TriggeredAbility { trigger: TriggerCondition::ACreatureDies,
            effect: Effect::GainLife { amount: 1 }, description: "Death".into() }],
        ..Default::default() });
    state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
    state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
    let doomed = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let source_id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, source_id, 0, CopyTargetPolicy::Preserve);
    begin(&mut state, vec![item.clone(), item]);
    state.objects.get_mut(&doomed).unwrap().damage_marked = 1;
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    assert!(!state.battlefield.contains(&doomed));
    assert_eq!(state.pending_triggers.len(), 2);
    assert_eq!(state.priority_player, 1);
    let order = legal_actions(&state).into_iter().find(|action| matches!(action, Action::OrderTriggers { .. })).unwrap();
    rules::apply_action(&mut state, &order);
    assert_eq!(state.priority_player, 0);
}

#[test]
fn direct_mana_and_trigger_helpers_cannot_bypass_copy_order() {
    let mut state = game();
    const LAND: u64 = 981102;
    const WATCHER: u64 = 981103;
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: LAND,
        name: "Red land".into(), card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColor(Color::Red)], ..Default::default() });
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: WATCHER,
        name: "Upkeep watcher".into(), card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![TriggeredAbility { trigger: TriggerCondition::BeginningOfUpkeep,
            effect: Effect::GainLife { amount: 1 }, description: "Upkeep".into() }],
        ..Default::default() });
    state.create_card_in_zone(LAND, 0, ZoneType::Battlefield);
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let source_id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, source_id, 0, CopyTargetPolicy::Preserve);
    begin(&mut state, vec![item.clone(), item]);
    let before = serde_json::to_value(&state).unwrap();
    rules::auto_tap_lands(&mut state, 0, &ManaCost::new(0, 0, 0, 0, 1, 0));
    assert!(!rules::fire_triggers(&mut state, TriggerCondition::BeginningOfUpkeep, None));
    rules::check_state_based_actions(&mut state);
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn direct_combo_helper_cannot_mutate_while_copy_order_is_pending() {
    use mtg_gto::combo::{apply_combo_effect, ComboDef, ComboEffect, ComboPrecondition};
    let mut state = game();
    let piece = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let source_id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, source_id, 0, CopyTargetPolicy::Preserve);
    begin(&mut state, vec![item.clone(), item]);
    let combo = ComboDef { id: 0, name: "Mana loop".into(), categories: vec![],
        required_pieces: vec![CREATURE],
        preconditions: vec![ComboPrecondition::PieceUntapped(CREATURE)],
        effect: ComboEffect::AddColorlessMana(5), reward_weight: 0.0 };
    let before = serde_json::to_value(&state).unwrap();
    apply_combo_effect(&mut state, 0, &combo);
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    assert!(!state.objects[&piece].tapped);
}

#[test]
fn existing_tutor_choice_rejects_batch_atomically() {
    let mut state = game();
    let source_id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, source_id, 0, CopyTargetPolicy::Preserve);
    state.pending_tutor = Some(PendingTutor { controller: 0, destination: ZoneType::Hand,
        subtype_filter: vec![] });
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(begin_terminal_copy_batch(&mut state, vec![item.clone(), item.clone()], true),
        Err(CopyError::ConflictingPendingChoice));
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn queued_trigger_capacity_is_checked_before_copy_commit() {
    let mut state = game();
    let source_id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, source_id, 0, CopyTargetPolicy::Preserve);
    state.next_stack_id = u64::MAX - 2;
    begin(&mut state, vec![item.clone(), item]);
    state.pending_triggers.push(trigger(101, 0, 0));
    let before = serde_json::to_value(&state).unwrap();
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn one_copy_and_queued_trigger_capacity_is_checked_before_materialization() {
    let mut state = game();
    let source_id = source(&mut state, DRAW, vec![]);
    let item = prepared(&state, source_id, 0, CopyTargetPolicy::Preserve);
    state.next_stack_id = u64::MAX - 1;
    state.pending_triggers.push(trigger(101, 0, 0));
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(begin_terminal_copy_batch(&mut state, vec![item], true),
        Err(CopyError::StackIdExhausted));
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn opposite_copy_orders_reverse_observable_resolution() {
    for first in [0, 1] {
        let mut state = game();
        let source_id = source(&mut state, DRAW, vec![]);
        let snapshot = snapshot_stack_spell(&state, source_id).unwrap();
        let draw = prepare_spell_copy(&state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
        let mut life_snapshot = snapshot;
        life_snapshot.definition.spell_effect = Some(Effect::GainLife { amount: 3 });
        let life = prepare_spell_copy(&state, &life_snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
        begin(&mut state, vec![draw, life]);
        rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: first });
        let hand = state.players[0].hand.len();
        let life_before = state.players[0].life;
        pass_twice(&mut state);
        assert_eq!(state.players[0].hand.len() - hand, if first == 0 { 0 } else { 1 });
        assert_eq!(state.players[0].life - life_before, if first == 0 { 3 } else { 0 });
    }
}

fn pending_target_state(extra_ids: usize, reverse_battlefield: bool,
    targeted_damaged: bool, targeted_tapped: bool) -> GameState {
    let mut state = game();
    for _ in 0..extra_ids { state.new_object_id(); state.new_stack_id(); }
    let selected = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let other = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let damaged = if targeted_damaged { selected } else { other };
    state.objects.get_mut(&damaged).unwrap().damage_marked = 1;
    let tapped = if targeted_tapped { selected } else { other };
    state.objects.get_mut(&tapped).unwrap().tapped = true;
    if reverse_battlefield { state.battlefield.reverse(); }
    let source_id = source(&mut state, HASTE_DRAW, vec![Target::Object(selected)]);
    let item = prepared(&state, source_id, 0, CopyTargetPolicy::Preserve);
    begin(&mut state, vec![item.clone(), item]);
    state
}

fn pending_info(state: &GameState) -> InformationSet {
    InformationSet::from_view(&state.visible_state(0), state.card_db())
}

#[test]
fn pending_target_key_distinguishes_observable_state_without_ids_or_vector_order() {
    let base = pending_target_state(0, false, false, false);
    let damaged = pending_target_state(0, false, true, false);
    let tapped = pending_target_state(0, false, false, true);
    let reversed = pending_target_state(0, true, false, false);
    let shifted = pending_target_state(7, false, false, false);
    let base_info = pending_info(&base);
    assert_ne!(base_info.pending_copy_order, pending_info(&damaged).pending_copy_order);
    assert_ne!(base_info.pending_copy_order, pending_info(&tapped).pending_copy_order);
    assert_eq!(base_info.pending_copy_order, pending_info(&reversed).pending_copy_order);
    assert_eq!(base_info.pending_copy_order, pending_info(&shifted).pending_copy_order);
    assert_eq!(base_info.hash_value(), pending_info(&reversed).hash_value());
    assert_eq!(base_info.hash_value(), pending_info(&shifted).hash_value());
    let descriptions = &base_info.pending_copy_order.as_ref().unwrap().items;
    assert_eq!(descriptions[0].targets, descriptions[1].targets);
}

#[test]
fn indistinguishable_targets_keep_relationships_without_raw_id_rank() {
    fn state_for(selected_second: bool, target_other: bool, extra_ids: usize) -> GameState {
        let mut state = game();
        for _ in 0..extra_ids { state.new_object_id(); state.new_stack_id(); }
        let first = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let second = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let selected = if selected_second { second } else { first };
        let other = if selected_second { first } else { second };
        let source_id = source(&mut state, HASTE_DRAW, vec![Target::Object(selected)]);
        let a = prepared(&state, source_id, 0, CopyTargetPolicy::Replace(vec![Target::Object(selected)]));
        let b = prepared(&state, source_id, 0, CopyTargetPolicy::Replace(vec![Target::Object(
            if target_other { other } else { selected })]));
        state.battlefield.reverse();
        begin(&mut state, vec![a, b]);
        state
    }
    let selected_low = state_for(false, false, 0);
    let selected_high = state_for(true, false, 7);
    let two_different = state_for(false, true, 0);
    assert_eq!(pending_info(&selected_low).pending_copy_order,
        pending_info(&selected_high).pending_copy_order);
    assert_eq!(pending_info(&selected_low).hash_value(),
        pending_info(&selected_high).hash_value());
    assert_ne!(pending_info(&selected_low).pending_copy_order,
        pending_info(&two_different).pending_copy_order);
}

#[test]
fn both_bucketed_abstractions_track_the_mandatory_copy_decision() {
    let mut pending = pending_target_state(0, false, false, false);
    let equivalent = pending_target_state(8, true, false, false);
    let different_target = pending_target_state(0, false, true, false);
    let mut no_pending = pending.clone();
    no_pending.pending_copy_order = None;
    let mut changed = serde_json::to_value(&pending).unwrap();
    changed["pending_copy_order"]["items"][1]["definition_description"] =
        serde_json::json!("another spell definition");
    let mut different_batch: GameState = serde_json::from_value(changed).unwrap();
    different_batch.card_db = pending.card_db.clone();
    let db = pending.card_db().clone();
    let abstractions: Vec<Box<dyn InfoSetAbstraction>> = vec![Box::new(BucketedAbstraction),
        Box::new(CardAwareBucketedAbstraction { card_db: &db })];
    for abstraction in &abstractions {
        let original = abstraction.abstract_info_set(&pending_info(&pending));
        assert_eq!(original, abstraction.abstract_info_set(&pending_info(&equivalent)));
        assert_ne!(original, abstraction.abstract_info_set(&pending_info(&different_target)));
        assert_ne!(original, abstraction.abstract_info_set(&pending_info(&different_batch)));
        assert_ne!(original, abstraction.abstract_info_set(&pending_info(&no_pending)));
        rules::apply_action(&mut pending, &Action::ChooseNextCopy { item_index: 0 });
        assert_ne!(original, abstraction.abstract_info_set(&pending_info(&pending)));
        pending = pending_target_state(0, false, false, false);
    }
}
