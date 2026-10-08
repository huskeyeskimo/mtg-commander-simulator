use std::sync::Arc;

use mtg_gto::action::{canonical::{canonicalize, resolve}, legal_actions, Action};
use mtg_gto::card::effects::Condition;
use mtg_gto::card::{ActivatedAbility, CardDef, CardType, DynamicValue, Effect, KeywordAbility, TargetSpec, TriggerCondition, TriggeredAbility, ZoneType};
use mtg_gto::events::{GameEvent, Zone};
use mtg_gto::game::{CardDatabase, GameState, Phase, Target};
use mtg_gto::info_set::InformationSet;
use mtg_gto::mana::ManaCost;
use mtg_gto::rules::apply_action;

const SOURCE: u64 = 998_100;
const SUBJECT: u64 = 998_101;
const DRAWN: u64 = 998_102;

fn ability_game(effect: Effect) -> (GameState, u64, u64, u64) {
    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: SOURCE,
        name: "Destroy ability".into(),
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(), requires_tap: false, sacrifice_cost: None,
            life_cost: 0, effect, description: "test".into(),
        }],
        ..Default::default()
    });
    db.insert(CardDef { id: SUBJECT, name: "Subject".into(),
        card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
        ..Default::default() });
    db.insert(CardDef { id: DRAWN, name: "Drawn card".into(),
        card_types: vec![CardType::Land], ..Default::default() });
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    let source = state.create_card_in_zone(SOURCE, 0, ZoneType::Battlefield);
    let subject = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let drawn = state.create_card_in_zone(DRAWN, 0, ZoneType::Library);
    (state, source, subject, drawn)
}

fn activate_and_resolve(state: &mut GameState, source: u64, subject: u64) {
    apply_action(state, &Action::ActivateAbility { object_id: source,
        ability_index: 0, targets: vec![Target::Object(subject)] });
    assert_eq!(state.stack.len(), 1);
    apply_action(state, &Action::PassPriority);
    apply_action(state, &Action::PassPriority);
}

#[test]
fn conditional_nested_destroy_preserves_existing_targeted_resolution() {
    let effect = Effect::Conditional {
        condition: Condition::Always,
        if_true: Box::new(Effect::Multiple(vec![
            Effect::DestroyTarget { target: TargetSpec::AnyCreature },
            Effect::DrawCards { count: 1 },
        ])),
        if_false: None,
    };
    let (mut state, source, subject, drawn) = ability_game(effect);
    activate_and_resolve(&mut state, source, subject);
    assert!(state.players[1].graveyard.contains(&subject));
    assert!(state.players[0].hand.contains(&drawn));
    assert!(state.stack.is_empty());
}

#[test]
fn modal_and_foreach_nested_destroy_keep_existing_resolution() {
    let child = || Effect::Multiple(vec![
        Effect::DestroyTarget { target: TargetSpec::AnyCreature },
        Effect::DrawCards { count: 1 },
    ]);
    let effects = [
        Effect::Modal { choices: vec![child()], choose_count: 1 },
        Effect::ForEach { count: DynamicValue::Fixed(1), effect: Box::new(child()) },
    ];
    for effect in effects {
        let (mut state, source, subject, drawn) = ability_game(effect);
        activate_and_resolve(&mut state, source, subject);
        assert!(state.players[1].graveyard.contains(&subject));
        assert!(state.players[0].hand.contains(&drawn));
        assert!(state.stack.is_empty());
    }
}

#[test]
fn compatible_multiple_checks_target_at_entry_and_keeps_later_child() {
    for case in 0..4 {
        let effect = Effect::Multiple(vec![
            Effect::DestroyTarget { target: TargetSpec::AnyCreature },
            Effect::DrawCards { count: 1 },
        ]);
        let (mut state, source, subject, drawn) = ability_game(effect);
        apply_action(&mut state, &Action::ActivateAbility { object_id: source,
            ability_index: 0, targets: vec![Target::Object(subject)] });
        assert_eq!(state.stack.len(), 1);
        match case {
            0 => {},
            1 => {
                state.objects.get_mut(&subject).unwrap().temp_keywords.push(KeywordAbility::Shroud);
                state.invalidate_characteristics_cache();
            }
            2 => {
                state.move_object(subject, ZoneType::Battlefield, ZoneType::Graveyard);
                state.move_object(subject, ZoneType::Graveyard, ZoneType::Battlefield);
            }
            3 => {
                state.objects.get_mut(&subject).unwrap().temp_keywords.push(KeywordAbility::Indestructible);
                state.invalidate_characteristics_cache();
            }
            _ => unreachable!(),
        }
        apply_action(&mut state, &Action::PassPriority);
        apply_action(&mut state, &Action::PassPriority);
        assert_eq!(state.players[1].graveyard.contains(&subject), case == 0);
        assert_eq!(state.players[0].hand.contains(&drawn), case == 0 || case == 3);
        assert!(state.stack.is_empty());
    }
}

#[test]
fn three_player_wrath_token_occurrences_continue_identically_after_restores() {
    const WRATH: u64 = 998_200;
    const WATCHER: u64 = 998_201;
    let mut db = CardDatabase::new();
    db.insert(CardDef { id: WRATH, name: "Mass destroy".into(),
        card_types: vec![CardType::Sorcery], mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::DestroyAll), ..Default::default() });
    db.insert(CardDef { id: WATCHER, name: "Death observer".into(),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::ACreatureDies,
            effect: Effect::GainLife { amount: 1 }, description: "death".into(),
        }], ..Default::default() });
    db.insert(CardDef { id: SUBJECT, name: "Creature".into(),
        card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
        ..Default::default() });
    let db = Arc::new(db);
    let mut state = GameState::new(3);
    state.card_db = Some(db.clone());
    state.phase = Phase::PreCombatMain;
    let watchers: Vec<_> = (0..3)
        .map(|player| state.create_card_in_zone(WATCHER, player, ZoneType::Battlefield))
        .collect();
    let creature = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let token = state.create_card_in_zone(SUBJECT, 2, ZoneType::Battlefield);
    state.objects.get_mut(&token).unwrap().is_token = true;
    let spell = state.create_card_in_zone(WRATH, 0, ZoneType::Hand);
    apply_action(&mut state, &Action::CastSpell { object_id: spell, targets: vec![] });
    state.drain_events();
    for _ in 0..3 { apply_action(&mut state, &Action::PassPriority); }
    assert!(state.players[1].graveyard.contains(&creature));
    assert!(!state.objects.contains_key(&token));
    assert_eq!(state.pending_triggers.len(), 6);
    let contexts: Vec<_> = state.pending_triggers.iter()
        .map(|pending| pending.context.zone_transition.as_ref().unwrap()).collect();
    assert!(contexts.iter().all(|context| context.group_id == contexts[0].group_id));
    assert_eq!(contexts.iter().filter(|context| context.subject.before.is_token).count(), 3);
    assert_eq!(state.pending_events.iter().filter(|event| matches!(event,
        GameEvent::ZoneChange { from: Zone::Battlefield, to: Zone::Graveyard, .. })).count(), 2);

    let snapshot = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let cloned = state.clone();
    let mut restored = state.clone();
    restored.pending_triggers.clear();
    restored.restore(snapshot).unwrap();
    let mut variants = vec![state, cloned, restored,
        serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap()];
    let expected_information = InformationSet::from_view(
        &variants[0].visible_state(0), variants[0].card_db()).unwrap().hash_value();
    let initial_life: Vec<_> = variants[0].players.iter().map(|player| player.life).collect();
    let mut expected = None;
    for game in &mut variants {
        game.card_db = Some(db.clone());
        assert_eq!(game.pending_triggers.len(), 6);
        assert_eq!(InformationSet::from_view(&game.visible_state(0), game.card_db()).unwrap().hash_value(),
            expected_information);
        for controller in 0..3 {
            assert_eq!(game.priority_player, controller);
            let choice = legal_actions(game).into_iter()
                .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
                .expect("mandatory owned occurrence ordering");
            let key = canonicalize(&choice, game).unwrap();
            let reconstructed = resolve(&key, game, controller).unwrap().unwrap();
            assert_eq!(canonicalize(&reconstructed, game).unwrap(), key);
            apply_action(game, &reconstructed);
        }
        assert!(game.pending_triggers.is_empty());
        assert_eq!(game.stack.len(), 6);
        assert_eq!(game.stack.iter().map(|entry| entry.controller).collect::<Vec<_>>(),
            vec![0, 0, 1, 1, 2, 2]);
        for _ in 0..18 {
            if game.stack.is_empty() { break; }
            for _ in 0..3 { apply_action(game, &Action::PassPriority); }
        }
        assert!(game.stack.is_empty());
        assert!(game.pending_triggers.is_empty());
        assert!(!game.objects.contains_key(&token));
        assert!(game.players.iter().enumerate().all(|(index, player)|
            player.life == initial_life[index] + 2));
        assert_eq!(game.pending_events.iter().filter(|event| matches!(event,
            GameEvent::AbilityTriggered { source, .. } if watchers.contains(source))).count(), 6);
        let finished = serde_json::to_value(&*game).unwrap();
        if let Some(ref expected) = expected { assert_eq!(&finished, expected); }
        else { expected = Some(finished); }
    }
}

#[test]
fn explicit_destroy_and_later_common_sba_death_keep_separate_contexts() {
    const WATCHER: u64 = 998_300;
    let effect = Effect::DestroyTarget { target: TargetSpec::AnyCreature };
    let (mut state, source, subject, _) = ability_game(effect);
    let mut db = state.card_db().clone();
    db.insert(CardDef { id: WATCHER, name: "Death watcher".into(),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::ACreatureDies,
            effect: Effect::GainLife { amount: 1 }, description: "death".into(),
        }], ..Default::default() });
    state.card_db = Some(Arc::new(db));
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let cascade = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    apply_action(&mut state, &Action::ActivateAbility { object_id: source,
        ability_index: 0, targets: vec![Target::Object(subject)] });
    state.objects.get_mut(&cascade).unwrap().damage_marked = 2;
    apply_action(&mut state, &Action::PassPriority);
    apply_action(&mut state, &Action::PassPriority);
    let occurrences: Vec<_> = state.pending_triggers.iter()
        .filter(|pending| pending.source_id == watcher).collect();
    assert_eq!(occurrences.len(), 2);
    assert!(occurrences.iter().all(|pending| pending.context.zone_transition.is_some()));
    let contexts: Vec<_> = occurrences.iter().map(|p| p.context.zone_transition.as_ref().unwrap()).collect();
    assert_ne!(contexts[0].group_id, contexts[1].group_id);
    assert!(contexts.iter().any(|c| c.subject.before.object.id == subject));
    assert!(contexts.iter().any(|c| c.subject.before.object.id == cascade));
    assert!(contexts.iter().all(|c| c.subject.creature_died()));
    assert!(state.players[1].graveyard.contains(&subject));
    assert!(state.players[1].graveyard.contains(&cascade));
}
