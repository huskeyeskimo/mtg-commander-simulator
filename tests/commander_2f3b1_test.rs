use std::sync::Arc;

use mtg_gto::{
    action::{canonical::{canonicalize, resolve}, legal_actions, Action},
    card::{ActivatedAbility, CardDef, CardType, Effect, KeywordAbility, TargetSpec, TriggerCondition, TriggeredAbility, ZoneType},
    events::{GameEvent, Zone},
    game::{CardDatabase, GameState, Phase, StackEntry, StackSource, Target},
    info_set::InformationSet,
    mana::ManaCost,
    rules,
};

const COMMANDER: u64 = 998_401;
const ORDINARY: u64 = 998_402;
const DRAW_SPELL: u64 = 998_403;
const HASTE_SPELL: u64 = 998_404;
const CREATURE: u64 = 998_405;
const ORDINARY_TWO: u64 = 998_406;
const WATCHER: u64 = 998_407;
const UNSUPPORTED: u64 = 998_408;
const DRAW_ABILITY: u64 = 998_409;

fn game() -> GameState {
    let mut db = CardDatabase::new();
    db.insert(CardDef { id: COMMANDER, name: "Hidden commander".into(),
        card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2), ..Default::default() });
    db.insert(CardDef { id: ORDINARY, name: "Ordinary".into(), card_types: vec![CardType::Land], ..Default::default() });
    db.insert(CardDef { id: ORDINARY_TWO, name: "Other hidden card".into(), card_types: vec![CardType::Land], ..Default::default() });
    db.insert(CardDef { id: DRAW_SPELL, name: "Draw".into(), card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::DrawCards { count: 3 }), ..Default::default() });
    db.insert(CardDef { id: HASTE_SPELL, name: "Expedite shape".into(), card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Multiple(vec![Effect::GainKeywordUntilEOT {
            keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature },
            Effect::DrawCards { count: 1 }])), ..Default::default() });
    db.insert(CardDef { id: CREATURE, name: "Target".into(), card_types: vec![CardType::Creature],
        power: Some(2), toughness: Some(2), ..Default::default() });
    db.insert(CardDef { id: WATCHER, name: "Draw watcher".into(), card_types: vec![CardType::Creature],
        power: Some(2), toughness: Some(2), triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::OpponentDrawsCard, effect: Effect::GainLife { amount: 1 },
            description: "On opponent draw".into() }], ..Default::default() });
    db.insert(CardDef { id: UNSUPPORTED, name: "Unsupported continuation".into(), card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::Multiple(vec![Effect::GainLife { amount: 3 },
            Effect::DrawCards { count: 1 }])), ..Default::default() });
    db.insert(CardDef { id: DRAW_ABILITY, name: "Draw ability source".into(), card_types: vec![CardType::Artifact],
        activated_abilities: vec![ActivatedAbility { cost: ManaCost::zero(), requires_tap: false,
            sacrifice_cost: None, life_cost: 0, effect: Effect::DrawCards { count: 2 },
            description: "Draw two cards".into() }], ..Default::default() });
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    state
}
fn add_commander(state: &mut GameState) -> u64 {
    let id = state.create_card_in_zone(COMMANDER, 0, ZoneType::Library).unwrap();
    state.designate_commander(id, 0).unwrap();
    id
}
fn stack_spell(state: &mut GameState, definition: u64, targets: Vec<Target>) -> u64 {
    let spell = state.create_card_in_zone(definition, 0, ZoneType::Stack).unwrap();
    let generations = targets.iter().map(|target| match target {
        Target::Object(id) => Some(state.objects[id].zone_change_count), _ => None,
    }).collect();
    state.stack.push(StackEntry { id: 1, source: StackSource::Spell(spell), controller: 0,
        targets, target_generations: generations });
    state.next_stack_id = 2;
    spell
}
fn resolve_stack(state: &mut GameState) {
    rules::apply_action(state, &Action::PassPriority);
    rules::apply_action(state, &Action::PassPriority);
}
fn answer(state: &mut GameState, to_command: bool) -> Action {
    let action = legal_actions(state).into_iter().find(|action| matches!(action,
        Action::ChooseCommanderMovement { to_command: answer, .. } if *answer == to_command)).unwrap();
    rules::apply_action(state, &action);
    action
}
fn variants(state: &GameState) -> Vec<GameState> {
    let mut snapshot = state.clone();
    snapshot.restore(state.snapshot()).unwrap();
    let mut variants = vec![state.clone(), snapshot, serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap(),
        bincode::deserialize(&bincode::serialize(state).unwrap()).unwrap()];
    for variant in &mut variants { variant.card_db = state.card_db.clone(); }
    variants
}

#[test]
fn ordinary_single_and_turn_draw_continue_after_owner_decision() {
    for to_command in [false, true] {
        let mut direct = game();
        let commander = add_commander(&mut direct);
        rules::draw_cards(&mut direct, 0, 1);
        assert!(direct.pending_movement.is_some());
        assert_eq!(direct.players[0].library, vec![commander]);
        assert_eq!(direct.priority_player, 0);
        answer(&mut direct, to_command);
        assert!(direct.pending_movement.is_none());
        assert_eq!(direct.players[0].hand.contains(&commander), !to_command);
        assert_eq!(direct.players[0].command_zone.contains(&commander), to_command);
        let events = direct.drain_events();
        assert_eq!(events.iter().filter(|event| matches!(event, GameEvent::CardDrawn { object, .. } if *object == commander)).count(), usize::from(!to_command));
        assert_eq!(events.iter().filter(|event| matches!(event, GameEvent::ZoneChange { object, from: Zone::Library, .. } if *object == commander)).count(), 1);

        let mut turn = game();
        turn.phase = Phase::Upkeep;
        turn.turn_number = 2;
        let commander = add_commander(&mut turn);
        resolve_stack(&mut turn);
        assert_eq!(turn.phase, Phase::Draw);
        assert!(turn.pending_movement.is_some());
        assert_eq!(turn.players[0].library, vec![commander]);
        assert!(!legal_actions(&turn).contains(&Action::PassPriority));
        answer(&mut turn, to_command);
        assert!(turn.pending_movement.is_none());
        assert_eq!(turn.priority_player, turn.active_player);
        assert_eq!(turn.players[0].hand.contains(&commander), !to_command);
    }
}

#[test]
fn ordinary_draws_work_without_any_commander_in_library() {
    let mut direct = game();
    for _ in 0..2 { direct.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap(); }
    rules::draw_cards(&mut direct, 0, 2);
    assert!(direct.pending_movement.is_none());
    assert_eq!(direct.players[0].hand.len(), 2);
    assert_eq!(direct.drain_events().iter().filter(|e| matches!(e, GameEvent::CardDrawn { .. })).count(), 2);

    let mut spell_game = game();
    for _ in 0..3 { spell_game.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap(); }
    let spell = stack_spell(&mut spell_game, DRAW_SPELL, vec![]);
    resolve_stack(&mut spell_game);
    assert!(spell_game.pending_movement.is_none());
    assert_eq!(spell_game.players[0].hand.len(), 3);
    assert!(spell_game.players[0].graveyard.contains(&spell));
    assert_eq!(spell_game.drain_events().iter().filter(|e| matches!(e, GameEvent::CardDrawn { .. })).count(), 3);
}

#[test]
fn activated_ability_draw_resumes_after_later_commander_choice() {
    let mut state = game();
    let source = state.create_card_in_zone(DRAW_ABILITY, 0, ZoneType::Battlefield).unwrap();
    state.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap();
    let commander = add_commander(&mut state);
    state.stack.push(StackEntry { id: 1, source: StackSource::ActivatedAbility {
        source_id: source, ability_index: 0 }, controller: 0, targets: vec![], target_generations: vec![] });
    state.next_stack_id = 2;
    resolve_stack(&mut state);
    assert!(state.pending_movement.is_some());
    assert_eq!(state.players[0].hand.len(), 1);
    answer(&mut state, false);
    assert!(state.pending_movement.is_none());
    assert_eq!(state.players[0].hand.len(), 2);
    assert!(state.players[0].hand.contains(&commander));
    assert!(state.battlefield.contains(&source));
    assert!(state.stack.is_empty());
}

#[test]
fn sequential_draws_resume_at_exact_card_and_finish_spell_once() {
    for commander_position in [0, 1] {
        for to_command in [false, true] {
            let mut state = game();
            if commander_position == 1 { state.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap(); }
            let commander = add_commander(&mut state);
            state.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap();
            state.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap();
            let spell = stack_spell(&mut state, DRAW_SPELL, vec![]);
            resolve_stack(&mut state);
            let pending = state.pending_movement.as_ref().expect("Commander draw choice");
            assert_eq!(pending.draws_remaining, 2 - commander_position);
            assert_eq!(state.players[0].hand.len(), commander_position as usize);
            assert_eq!(state.stack.len(), 0);
            assert!(!legal_actions(&state).contains(&Action::PassPriority));
            for (variant, mut restored) in variants(&state).into_iter().enumerate() {
                for action in legal_actions(&restored).into_iter().filter(|a| !matches!(a, Action::Concede)) {
                    let canonical = canonicalize(&action, &restored).unwrap();
                    assert_eq!(resolve(&canonical, &restored, 0).unwrap(), Some(action));
                }
                answer(&mut restored, to_command);
                assert!(restored.pending_movement.is_none());
                assert_eq!(restored.players[0].hand.len(), 3 - usize::from(to_command),
                    "position={commander_position} to_command={to_command} remaining={} library={} stopped={}",
                    pending.draws_remaining, restored.players[0].library.len(), restored.gameplay_stopped());
                assert_eq!(restored.players[0].command_zone.contains(&commander), to_command);
                assert_eq!(restored.players[0].graveyard.iter().filter(|&&id| id == spell).count(), 1);
                assert_eq!(restored.priority_player, restored.active_player);
                let events = restored.drain_events();
                if variant == 0 {
                    assert_eq!(events.iter().filter(|event| matches!(event, GameEvent::CardDrawn { player: 0, .. })).count(), 3 - usize::from(to_command));
                }
            }
        }
    }
}

#[test]
fn two_commander_draws_have_distinct_windows_and_each_finishes_before_the_next() {
    let mut state = game();
    let first = add_commander(&mut state);
    let second = state.create_card_in_zone(COMMANDER, 0, ZoneType::Library).unwrap();
    state.designate_commander(second, 1).unwrap();
    state.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap();
    let spell = stack_spell(&mut state, DRAW_SPELL, vec![]);
    resolve_stack(&mut state);
    let stale = legal_actions(&state).into_iter().find(|a| matches!(a, Action::ChooseCommanderMovement { .. })).unwrap();
    answer(&mut state, true);
    assert!(state.players[0].command_zone.contains(&first));
    assert!(state.pending_movement.is_some());
    assert_eq!(state.pending_movement.as_ref().unwrap().proposals[0].request.object.id, second);
    let before = bincode::serialize(&state).unwrap();
    rules::apply_action(&mut state, &stale);
    assert_eq!(bincode::serialize(&state).unwrap(), before);
    answer(&mut state, false);
    assert!(state.pending_movement.is_none());
    assert!(state.players[0].hand.contains(&second));
    assert_eq!(state.players[0].hand.len(), 2);
    assert!(state.players[0].graveyard.contains(&spell));
    let events = state.drain_events();
    assert_eq!(events.iter().filter(|e| matches!(e, GameEvent::CardDrawn { .. })).count(), 2);
}

#[test]
fn draw_triggers_wait_for_the_whole_sequence_even_after_restoration() {
    let mut state = game();
    state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield).unwrap();
    state.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap();
    add_commander(&mut state);
    state.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap();
    stack_spell(&mut state, DRAW_SPELL, vec![]);
    resolve_stack(&mut state);
    assert_eq!(state.pending_triggers.len(), 1);
    assert!(state.pending_movement.is_some());
    assert!(state.stack.is_empty(), "draw trigger cannot enter stack mid-resolution");
    for mut restored in variants(&state) {
        assert_eq!(restored.pending_triggers.len(), 1);
        assert!(restored.stack.is_empty());
        assert!(!legal_actions(&restored).contains(&Action::PassPriority));
        answer(&mut restored, true);
        assert!(restored.pending_movement.is_none());
        assert_eq!(restored.pending_triggers.len() + restored.stack.len(), 2);
        assert_eq!(restored.players[0].hand.len(), 2);
    }
}

#[test]
fn expedite_haste_is_applied_once_before_draw_choice_and_resolution_finishes() {
    let mut state = game();
    let target = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield).unwrap();
    let commander = add_commander(&mut state);
    let spell = stack_spell(&mut state, HASTE_SPELL, vec![Target::Object(target)]);
    resolve_stack(&mut state);
    assert!(state.has_keyword(target, KeywordAbility::Haste));
    assert_eq!(state.continuous_effects.len(), 1);
    assert!(state.pending_movement.is_some());
    assert!(state.players[0].library.contains(&commander));
    answer(&mut state, false);
    assert!(state.pending_movement.is_none());
    assert_eq!(state.continuous_effects.len(), 1);
    assert!(state.players[0].hand.contains(&commander));
    assert!(state.players[0].graveyard.contains(&spell));
}

#[test]
fn owner_chooses_even_when_library_card_has_opponent_controller_and_invalid_actions_are_atomic() {
    let mut state = game();
    let commander = add_commander(&mut state);
    state.objects.get_mut(&commander).unwrap().controller = 1;
    rules::draw_cards(&mut state, 0, 1);
    assert_eq!(state.priority_player, 0);
    let real = legal_actions(&state).into_iter().find(|a| matches!(a, Action::ChooseCommanderMovement { .. })).unwrap();
    let before = bincode::serialize(&state).unwrap();
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(bincode::serialize(&state).unwrap(), before);
    if let Action::ChooseCommanderMovement { window, stage, object, to_command } = real {
        rules::apply_action(&mut state, &Action::ChooseCommanderMovement { window: window + 1, stage, object, to_command });
        assert_eq!(bincode::serialize(&state).unwrap(), before);
        answer(&mut state, false);
        let settled = bincode::serialize(&state).unwrap();
        rules::apply_action(&mut state, &Action::ChooseCommanderMovement { window, stage, object, to_command });
        assert_eq!(bincode::serialize(&state).unwrap(), settled);
    }
}

#[test]
fn pending_draw_is_private_to_other_player_and_rejects_malformed_restoration() {
    let mut left = game();
    let commander = add_commander(&mut left);
    left.create_card_in_zone(ORDINARY, 0, ZoneType::Library).unwrap();
    left.create_card_in_zone(ORDINARY_TWO, 0, ZoneType::Library).unwrap();
    rules::draw_cards(&mut left, 0, 1);
    let mut right = left.clone();
    right.players[0].library.swap(1, 2);
    let left_info = InformationSet::from_view(&left.visible_state(1), left.card_db()).unwrap();
    let right_info = InformationSet::from_view(&right.visible_state(1), right.card_db()).unwrap();
    assert_eq!(left_info.hash_value(), right_info.hash_value());
    assert_eq!(InformationSet::normalize_retained_view(&left.visible_state(1)).unwrap().encoding,
        InformationSet::normalize_retained_view(&right.visible_state(1)).unwrap().encoding);
    assert_eq!(left.pending_movement.as_ref().unwrap().proposals[0].request.object.id, commander);
    let mut value = serde_json::to_value(&left).unwrap();
    value["pending_movement"]["draws_remaining"] = serde_json::json!(5);
    assert!(serde_json::from_value::<GameState>(value).is_err());
}

#[test]
fn empty_library_after_replaced_draw_records_failure_before_resolution_settlement() {
    let mut state = game();
    let commander = add_commander(&mut state);
    let spell = stack_spell(&mut state, DRAW_SPELL, vec![]);
    resolve_stack(&mut state);
    assert!(state.pending_movement.is_some());
    assert!(!state.game_over);
    answer(&mut state, true);
    assert!(state.pending_movement.is_none());
    assert!(state.players[0].command_zone.contains(&commander));
    assert!(state.loss_boundary.pending_failed_draws.contains(&0) || state.game_over);
    assert!(state.players[0].graveyard.contains(&spell) || state.game_over);
}

#[test]
fn empty_library_without_commander_records_failed_draw_until_next_priority() {
    let mut state = game();
    rules::draw_cards(&mut state, 0, 1);
    assert!(state.loss_boundary.pending_failed_draws.contains(&0));
    assert!(!state.game_over);
    assert!(state.drain_events().is_empty());
    rules::check_state_based_actions(&mut state);
    assert!(state.game_over);
}

#[test]
fn unsupported_draw_tail_rejects_before_earlier_instruction_or_stack_pop() {
    let mut state = game();
    let commander = add_commander(&mut state);
    let spell = stack_spell(&mut state, UNSUPPORTED, vec![]);
    let life = state.players[0].life;
    resolve_stack(&mut state);
    assert!(state.gameplay_stopped());
    assert_eq!(state.players[0].life, life);
    assert_eq!(state.players[0].library, vec![commander]);
    assert!(state.stack.iter().any(|entry| entry.source.physical_object() == Some(spell)));
    assert!(state.pending_movement.is_none());
}
