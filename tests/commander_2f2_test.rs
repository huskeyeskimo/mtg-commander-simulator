//! CR 903.9a choices after actual graveyard/exile arrivals.
use std::sync::Arc;
use mtg_gto::{
    action::{legal_actions, Action, canonical::{canonicalize, resolve}},
    card::{CardDef, CardType, ZoneType},
    game::{CardDatabase, CommanderDesignation, GameState, Phase},
    info_set::InformationSet, rules,
};

const COMMANDER: u64 = 998_201;
const HIDDEN: u64 = 998_202;

fn fixture(players: usize) -> GameState {
    let mut db = CardDatabase::new();
    db.insert(CardDef { id: COMMANDER, name: "Commander".into(), card_types: vec![CardType::Creature],
        power: Some(2), toughness: Some(2), ..Default::default() });
    db.insert(CardDef { id: HIDDEN, name: "Hidden".into(), card_types: vec![CardType::Creature],
        power: Some(1), toughness: Some(1), ..Default::default() });
    let mut state = GameState::new_commander(players);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    state
}
fn add(state: &mut GameState, owner: usize, role: u8) -> u64 {
    let id = state.create_card_in_zone(COMMANDER, owner, ZoneType::Battlefield).unwrap();
    state.designate_commander(id, role).unwrap();
    id
}
fn choice(owner: usize, role: u8, to_command: bool) -> Action {
    Action::ChooseCommanderSba { designation: CommanderDesignation { owner, role }, to_command }
}
fn arrival(state: &mut GameState, id: u64, zone: ZoneType) {
    state.move_object(id, ZoneType::Battlefield, zone);
    assert!(state.objects[&id].zone_change_count > 0);
}
fn variants(state: &GameState) -> Vec<GameState> {
    let mut snapshot = state.clone();
    snapshot.restore(state.snapshot()).unwrap();
    let mut all = vec![state.clone(), snapshot,
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap(),
        bincode::deserialize(&bincode::serialize(state).unwrap()).unwrap()];
    for variant in &mut all { variant.card_db = state.card_db.clone(); }
    all
}

#[test]
fn arrival_choice_decline_consumes_and_new_arrival_reopens() {
    for zone in [ZoneType::Graveyard, ZoneType::Exile] {
        let mut state = fixture(2);
        let id = add(&mut state, 0, 0);
        arrival(&mut state, id, zone);
        assert!(state.players[0].command_zone.is_empty());
        rules::check_state_based_actions(&mut state);
        assert_eq!(state.priority_player, 0);
        assert!(legal_actions(&state).contains(&choice(0, 0, false)));
        assert!(legal_actions(&state).contains(&choice(0, 0, true)));
        let before = bincode::serialize(&state).unwrap();
        rules::apply_action(&mut state, &Action::PassPriority);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
        rules::apply_action(&mut state, &choice(0, 0, false));
        assert!(state.pending_commander_sba.is_none());
        assert!(match zone { ZoneType::Graveyard => state.players[0].graveyard.contains(&id),
            ZoneType::Exile => state.players[0].exile.contains(&id), _ => false });
        rules::check_state_based_actions(&mut state);
        assert!(state.pending_commander_sba.is_none());
        state.move_object(id, zone, ZoneType::Battlefield);
        arrival(&mut state, id, zone);
        rules::check_state_based_actions(&mut state);
        assert!(state.pending_commander_sba.is_some());
        rules::apply_action(&mut state, &choice(0, 0, true));
        assert!(state.players[0].command_zone.contains(&id));
        assert!(state.pending_commander_sba.is_none());
        rules::check_state_based_actions(&mut state);
        assert!(state.pending_commander_sba.is_none());
    }
}

#[test]
fn stale_or_noop_arrivals_do_not_offer() {
    let mut state = fixture(2);
    let id = add(&mut state, 0, 0);
    state.move_object(id, ZoneType::Battlefield, ZoneType::Battlefield);
    assert!(state.commander_arrivals.is_empty());
    arrival(&mut state, id, ZoneType::Exile);
    state.move_object(id, ZoneType::Exile, ZoneType::Hand);
    assert!(state.commander_arrivals.is_empty());
    rules::check_state_based_actions(&mut state);
    assert!(state.pending_commander_sba.is_none());
    state.move_object(id, ZoneType::Hand, ZoneType::Exile);
    rules::check_state_based_actions(&mut state);
    let stale = state.pending_commander_sba.as_ref().unwrap().offers[0].object;
    let mut bad = state.clone();
    bad.pending_commander_sba.as_mut().unwrap().offers[0].object.generation = stale.generation - 1;
    assert!(bad.validate_commander_choices().is_err());
    assert!(legal_actions(&bad).is_empty());
    assert!(bad.restore(bad.snapshot()).is_err());
}

#[test]
fn apnap_owners_freeze_then_commit_simultaneously() {
    for players in [3, 4] {
        let mut state = fixture(players);
        state.active_player = 1;
        state.priority_player = 2;
        let ids: Vec<_> = (0..players).map(|owner| add(&mut state, owner, 0)).collect();
        for (owner, &id) in ids.iter().enumerate() {
            arrival(&mut state, id, if owner % 2 == 0 { ZoneType::Graveyard } else { ZoneType::Exile });
        }
        let initial_generations: Vec<_> = ids.iter().map(|id| state.objects[id].zone_change_count).collect();
        rules::check_state_based_actions(&mut state);
        assert_eq!(state.priority_player, 1);
        let order: Vec<_> = (0..players).map(|rank| (rank + 1) % players).collect();
        for (step, &owner) in order.iter().enumerate() {
            assert_eq!(state.priority_player, owner);
            assert!(state.pending_commander_sba.is_some());
            assert!(state.players.iter().all(|seat| seat.command_zone.is_empty()));
            assert!(ids.iter().enumerate().all(|(i, id)|
                state.objects[id].zone_change_count == initial_generations[i]));
            rules::apply_action(&mut state, &choice(owner, 0, step % 2 == 0));
        }
        assert!(state.pending_commander_sba.is_none());
        assert_eq!(state.priority_player, 2);
        for (step, &owner) in order.iter().enumerate() {
            assert_eq!(state.players[owner].command_zone.contains(&ids[owner]), step % 2 == 0);
        }
    }
}

#[test]
fn partner_choices_can_be_answered_in_either_order() {
    let mut state = fixture(2);
    let a = add(&mut state, 0, 0);
    let b = add(&mut state, 0, 1);
    arrival(&mut state, a, ZoneType::Graveyard);
    arrival(&mut state, b, ZoneType::Exile);
    rules::check_state_based_actions(&mut state);
    rules::apply_action(&mut state, &choice(0, 1, false));
    assert_eq!(state.priority_player, 0);
    assert!(state.pending_commander_sba.is_some());
    rules::apply_action(&mut state, &choice(0, 0, true));
    assert!(state.players[0].command_zone.contains(&a));
    assert!(state.players[0].exile.contains(&b));
}

#[test]
fn canonical_and_restoration_while_choice_pending() {
    let mut state = fixture(2);
    let id = add(&mut state, 0, 0);
    arrival(&mut state, id, ZoneType::Graveyard);
    rules::check_state_based_actions(&mut state);
    let action = choice(0, 0, true);
    let canonical = canonicalize(&action, &state).unwrap();
    let expected_info = InformationSet::from_view(&state.visible_state(0), state.card_db()).unwrap().hash_value();
    let view_json = serde_json::to_string(&state.visible_state(0).pending_commander_sba).unwrap();
    assert!(!view_json.contains("generation"));
    assert!(!view_json.contains("object"));
    for mut variant in variants(&state) {
        assert_eq!(InformationSet::from_view(&variant.visible_state(0), variant.card_db()).unwrap().hash_value(), expected_info);
        assert_eq!(resolve(&canonical, &variant, 0).unwrap(), Some(action.clone()));
        rules::apply_action(&mut variant, &action);
        assert!(variant.players[0].command_zone.contains(&id));
    }
}

#[test]
fn malformed_choice_rejected_and_failed_action_unchanged() {
    let mut state = fixture(2);
    let id = add(&mut state, 1, 0);
    arrival(&mut state, id, ZoneType::Exile);
    rules::check_state_based_actions(&mut state);
    let before = bincode::serialize(&state).unwrap();
    rules::apply_action(&mut state, &choice(0, 0, true));
    assert_eq!(bincode::serialize(&state).unwrap(), before);
    for fault in 0..5 {
        let mut bad = state.clone();
        match fault {
            0 => bad.pending_commander_sba.as_mut().unwrap().offers[0].designation.owner = 0,
            1 => bad.pending_commander_sba.as_mut().unwrap().answers[0] = Some(true),
            2 => bad.pending_commander_sba.as_mut().unwrap().version = 99,
            3 => bad.pending_commander_sba.as_mut().unwrap().offers[0].zone = ZoneType::Graveyard,
            _ => bad.priority_player = 0,
        }
        assert!(bad.validate_commander_choices().is_err(), "fault {fault}");
        assert!(bad.restore(bad.snapshot()).is_err());
    }
}

#[test]
fn replacement_to_exile_preserves_no_death_and_prevention_offers_nothing() {
    use mtg_gto::replacement::{ReplacementAction, ReplacementEffect, ReplacementEventKind};
    for action in [ReplacementAction::RedirectToZone(ZoneType::Exile), ReplacementAction::Prevent] {
        let mut state = fixture(2);
        let id = add(&mut state, 0, 0);
        state.objects.get_mut(&id).unwrap().temp_toughness_mod = -2;
        state.replacement_effects.push(ReplacementEffect { source_id: id, controller: 0,
            applies_to: ReplacementEventKind::WouldDie, action: action.clone(),
            is_self_replacement: true, description: "fixture".into() });
        rules::check_state_based_actions(&mut state);
        match action {
            ReplacementAction::Prevent => {
                assert!(state.battlefield.contains(&id));
                assert!(state.pending_commander_sba.is_none());
                assert!(state.commander_arrivals.is_empty());
            }
            _ => {
                assert!(state.players[0].exile.contains(&id));
                assert!(state.pending_commander_sba.is_some());
                assert!(state.pending_triggers.iter().all(|trigger| trigger.context.zone_transition.as_ref()
                    .is_none_or(|context| !context.subject.creature_died())));
            }
        }
    }
}

#[test]
fn death_keywords_wait_for_choice_and_use_original_history() {
    use mtg_gto::card::KeywordAbility;
    let mut state = fixture(2);
    let id = add(&mut state, 0, 0);
    state.objects.get_mut(&id).unwrap().temp_keywords = vec![KeywordAbility::Undying, KeywordAbility::Persist];
    state.objects.get_mut(&id).unwrap().temp_toughness_mod = -2;
    rules::check_state_based_actions(&mut state);
    assert!(state.players[0].graveyard.contains(&id));
    assert!(state.pending_commander_sba.is_some());
    assert_eq!(state.pending_triggers.len(), 2);
    assert!(state.stack.is_empty());
    assert!(state.pending_triggers.iter().all(|trigger| trigger.context.zone_transition.as_ref()
        .is_some_and(|context| context.subject.creature_died()
            && context.subject.destination.zone == ZoneType::Graveyard)));
    rules::apply_action(&mut state, &choice(0, 0, true));
    assert!(state.players[0].command_zone.contains(&id));
    assert!(state.pending_triggers.iter().all(|trigger| trigger.context.zone_transition.as_ref()
        .is_some_and(|context| context.subject.creature_died()))
        || !state.stack.is_empty());
}

#[test]
fn hidden_order_does_not_affect_pending_public_choice() {
    let mut state = fixture(2);
    let commander = add(&mut state, 0, 0);
    let a = state.create_card_in_zone(HIDDEN, 1, ZoneType::Hand).unwrap();
    let b = state.create_card_in_zone(HIDDEN, 1, ZoneType::Hand).unwrap();
    arrival(&mut state, commander, ZoneType::Exile);
    rules::check_state_based_actions(&mut state);
    let mut swapped = state.clone();
    swapped.players[1].hand.reverse();
    assert_eq!(swapped.players[1].hand, vec![b, a]);
    let original = InformationSet::from_view(&state.visible_state(0), state.card_db()).unwrap();
    let changed = InformationSet::from_view(&swapped.visible_state(0), swapped.card_db()).unwrap();
    assert_eq!(original.hash_value(), changed.hash_value());
    assert_eq!(canonicalize(&choice(0, 0, false), &state).unwrap(),
        canonicalize(&choice(0, 0, false), &swapped).unwrap());
}

#[test]
fn simultaneous_deaths_keep_historical_controller_and_owner_choices() {
    use mtg_gto::card::{Effect, TriggerCondition, TriggeredAbility};
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    let mut state = fixture(3);
    Arc::make_mut(state.card_db.as_mut().unwrap()).cards.get_mut(&COMMANDER).unwrap()
        .triggered_abilities = [TriggerCondition::Dies, TriggerCondition::LeavesBattlefield]
        .into_iter().map(|trigger| TriggeredAbility { trigger,
            effect: Effect::GainLife { amount: 1 }, description: "historical".into() }).collect();
    let a = add(&mut state, 0, 0);
    let b = add(&mut state, 2, 0);
    state.active_player = 1;
    state.priority_player = 1;
    state.continuous_effects.push(ContinuousEffect { source_id: b, controller: 2,
        timestamp: 1, duration: Duration::Permanent, affected: AffectedObjects::Specific(b),
        modification: LayerModification::ChangeController(1) });
    state.objects.get_mut(&a).unwrap().temp_toughness_mod = -2;
    state.objects.get_mut(&b).unwrap().temp_toughness_mod = -2;
    state.invalidate_characteristics_cache();
    rules::check_state_based_actions(&mut state);
    assert!(state.players[0].graveyard.contains(&a));
    assert!(state.players[2].graveyard.contains(&b));
    assert_eq!(state.priority_player, 2);
    assert!(state.stack.is_empty());
    assert_eq!(state.pending_triggers.len(), 4);
    let groups: std::collections::HashSet<_> = state.pending_triggers.iter().map(|trigger|
        trigger.context.zone_transition.as_ref().unwrap().group_id).collect();
    assert_eq!(groups.len(), 1);
    for trigger in &state.pending_triggers {
        let history = trigger.context.zone_transition.as_ref().unwrap();
        assert!(history.subject.creature_died());
        assert_eq!(history.subject.before.owner, if history.subject.before.object.id == a { 0 } else { 2 });
        assert_eq!(history.subject.before.controller, if history.subject.before.object.id == a { 0 } else { 1 });
    }
    rules::apply_action(&mut state, &choice(2, 0, true));
    assert!(state.players[2].graveyard.contains(&b));
    rules::apply_action(&mut state, &choice(0, 0, false));
    assert!(state.players[2].command_zone.contains(&b));
    assert!(state.players[0].graveyard.contains(&a));
    assert!(state.pending_commander_sba.is_none());
}

#[test]
fn consumed_opportunity_cannot_be_replayed_by_restoration() {
    let mut state = fixture(2);
    let id = add(&mut state, 0, 0);
    arrival(&mut state, id, ZoneType::Exile);
    rules::check_state_based_actions(&mut state);
    let saved = state.pending_commander_sba.clone().unwrap();
    rules::apply_action(&mut state, &choice(0, 0, false));
    assert_eq!(state.consumed_commander_arrivals.len(), 1);
    let mut replay_arrival = state.clone();
    replay_arrival.commander_arrivals.push(saved.offers[0]);
    assert!(replay_arrival.validate_commander_choices().is_err());
    let mut replay_window = state.clone();
    replay_window.pending_commander_sba = Some(saved);
    assert!(replay_window.validate_commander_choices().is_err());
    assert!(replay_window.restore(replay_window.snapshot()).is_err());
    state.move_object(id, ZoneType::Exile, ZoneType::Battlefield);
    assert!(state.consumed_commander_arrivals.is_empty());
}

#[test]
fn malformed_owners_and_consumed_windows_do_not_panic_in_public_view() {
    let mut state = fixture(2);
    let id = add(&mut state, 1, 0);
    arrival(&mut state, id, ZoneType::Exile);
    rules::check_state_based_actions(&mut state);
    for fault in 0..2 {
        let mut bad = state.clone();
        if fault == 0 { bad.pending_commander_sba.as_mut().unwrap().offers[0].designation.owner = usize::MAX; }
        else { bad.pending_commander_sba.as_mut().unwrap().answers[0] = Some(false); }
        assert!(bad.validate_commander_choices().is_err());
        assert!(bad.visible_state(0).state_encoding_failed);
        assert!(serde_json::from_slice::<GameState>(&serde_json::to_vec(&bad).unwrap()).is_err());
        assert!(bincode::deserialize::<GameState>(&bincode::serialize(&bad).unwrap()).is_err());
    }
}

#[test]
fn linked_exile_departure_creates_new_graveyard_opportunity() {
    let mut state = fixture(2);
    let id = add(&mut state, 0, 0);
    arrival(&mut state, id, ZoneType::Exile);
    rules::check_state_based_actions(&mut state);
    rules::apply_action(&mut state, &choice(0, 0, false));
    let source = state.create_card_in_zone(HIDDEN, 0, ZoneType::Battlefield).unwrap();
    state.objects.get_mut(&id).unwrap().exiled_by = Some(source);
    state.move_object(source, ZoneType::Battlefield, ZoneType::Graveyard);
    assert!(state.players[0].graveyard.contains(&id));
    assert!(state.consumed_commander_arrivals.is_empty());
    rules::check_state_based_actions(&mut state);
    assert_eq!(state.pending_commander_sba.as_ref().unwrap().offers[0].zone, ZoneType::Graveyard);
}

#[test]
fn surveilled_library_commander_gets_graveyard_choice() {
    use mtg_gto::{card::Effect, mana::ManaCost};
    const SURVEIL: u64 = 998_203;
    let mut state = fixture(2);
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: SURVEIL,
        name: "Surveil".into(), card_types: vec![CardType::Sorcery],
        mana_cost: Some(ManaCost::zero()), spell_effect: Some(Effect::Surveil { count: 1 }),
        ..Default::default() });
    let id = state.create_card_in_zone(COMMANDER, 0, ZoneType::Library).unwrap();
    state.designate_commander(id, 0).unwrap();
    let spell = state.create_card_in_zone(SURVEIL, 0, ZoneType::Hand).unwrap();
    rules::apply_action(&mut state, &Action::CastSpell { object_id: spell, targets: vec![] });
    for _ in 0..2 { rules::apply_action(&mut state, &Action::PassPriority); }
    assert!(state.players[0].graveyard.contains(&id));
    assert!(state.pending_commander_sba.is_some());
}

#[test]
fn terminal_concession_during_choice_is_restorable() {
    let mut state = fixture(2);
    let id = add(&mut state, 0, 0);
    arrival(&mut state, id, ZoneType::Graveyard);
    rules::check_state_based_actions(&mut state);
    rules::apply_action(&mut state, &Action::Concede);
    assert!(state.gameplay_stopped());
    for variant in variants(&state) { assert!(variant.gameplay_stopped()); }
}

#[test]
fn cleanup_decline_without_activity_ends_step_acceptance_repeats() {
    for to_command in [false, true] {
        let mut state = fixture(2);
        state.phase = Phase::Cleanup;
        state.active_player = 0;
        state.priority_player = 0;
        let id = state.create_card_in_zone(COMMANDER, 0, ZoneType::Hand).unwrap();
        state.designate_commander(id, 0).unwrap();
        for _ in 0..7 { state.create_card_in_zone(HIDDEN, 0, ZoneType::Hand).unwrap(); }
        rules::apply_action(&mut state, &Action::Discard { object_id: id });
        assert!(state.pending_commander_sba.is_some());
        assert_eq!(state.pending_commander_sba.as_ref().unwrap().cleanup_resume, Some(false));
        rules::apply_action(&mut state, &choice(0, 0, to_command));
        assert!(state.pending_commander_sba.is_none());
        if to_command {
            assert!(state.players[0].command_zone.contains(&id));
            assert_eq!(state.phase, Phase::Cleanup);
            assert!(state.cleanup_needs_repeat);
        } else {
            assert!(state.players[0].graveyard.contains(&id));
            assert!(state.turn_number > 1);
            assert!(!state.cleanup_needs_repeat);
        }
    }
}

#[test]
fn partial_apnap_restores_and_canonical_cannot_select_other_owner() {
    let mut state = fixture(3);
    state.active_player = 1;
    for owner in [0, 1, 2] { let id = add(&mut state, owner, 0); arrival(&mut state, id, ZoneType::Graveyard); }
    rules::check_state_based_actions(&mut state);
    assert_eq!(state.priority_player, 1);
    assert!(canonicalize(&choice(2, 0, true), &state).is_err());
    assert_eq!(resolve(&mtg_gto::action::canonical::CanonicalAction::ChooseCommanderSba {
        designation: CommanderDesignation { owner: 2, role: 0 }, to_command: true }, &state, 1).unwrap(), None);
    rules::apply_action(&mut state, &choice(1, 0, false));
    assert_eq!(state.priority_player, 2);
    let actions = legal_actions(&state);
    assert!(actions.contains(&choice(2, 0, true)));
    assert!(!actions.contains(&choice(0, 0, true)));
    for mut variant in variants(&state) {
        assert_eq!(variant.priority_player, 2);
        rules::apply_action(&mut variant, &choice(2, 0, true));
        assert_eq!(variant.priority_player, 0);
        rules::apply_action(&mut variant, &choice(0, 0, false));
        assert!(variant.pending_commander_sba.is_none());
        assert_eq!(variant.players[2].command_zone.len(), 1);
        assert_eq!(variant.players[1].graveyard.len(), 1);
        assert_eq!(variant.players[0].graveyard.len(), 1);
    }
}

#[test]
fn accepted_commander_move_joins_frozen_ordinary_sba_pass() {
    let mut state = fixture(2);
    let commander = add(&mut state, 0, 0);
    let other = state.create_card_in_zone(HIDDEN, 1, ZoneType::Battlefield).unwrap();
    arrival(&mut state, commander, ZoneType::Graveyard);
    state.objects.get_mut(&other).unwrap().temp_toughness_mod = -1;
    rules::check_state_based_actions(&mut state);
    assert!(state.battlefield.contains(&other));
    assert!(state.players[0].graveyard.contains(&commander));
    assert_eq!(state.next_zone_event_group_id, 0);
    rules::apply_action(&mut state, &choice(0, 0, true));
    assert!(state.players[0].command_zone.contains(&commander));
    assert!(state.players[1].graveyard.contains(&other));
    assert_eq!(state.next_zone_event_group_id, 1);
}

#[test]
fn commander_self_sacrifice_cost_captures_death_history() {
    use mtg_gto::card::{ActivatedAbility, Effect, SacrificeCost, TriggerCondition, TriggeredAbility};
    use mtg_gto::mana::ManaCost;
    let mut state = fixture(2);
    let def = Arc::make_mut(state.card_db.as_mut().unwrap()).cards.get_mut(&COMMANDER).unwrap();
    def.activated_abilities.push(ActivatedAbility { cost: ManaCost::zero(), requires_tap: false,
        sacrifice_cost: Some(SacrificeCost::SelfSacrifice), life_cost: 0,
        effect: Effect::GainLife { amount: 1 }, description: "sacrifice self".into() });
    def.triggered_abilities = [TriggerCondition::Dies, TriggerCondition::LeavesBattlefield]
        .into_iter().map(|trigger| TriggeredAbility { trigger,
            effect: Effect::GainLife { amount: 1 }, description: "leave".into() }).collect();
    let id = add(&mut state, 0, 0);
    rules::apply_action(&mut state, &Action::ActivateAbility { object_id: id, ability_index: 0, targets: vec![] });
    assert!(state.players[0].graveyard.contains(&id));
    assert!(state.pending_commander_sba.is_some());
    assert_eq!(state.pending_triggers.len(), 2);
    assert!(state.pending_triggers.iter().all(|trigger| trigger.context.zone_transition.as_ref()
        .is_some_and(|context| context.subject.creature_died()
            && context.subject.destination.zone == ZoneType::Graveyard)));
}

#[test]
fn annihilator_commander_sacrifice_captures_death_history() {
    use mtg_gto::card::{Effect, TriggerCondition, TriggeredAbility};
    let mut state = fixture(2);
    Arc::make_mut(state.card_db.as_mut().unwrap()).cards.get_mut(&HIDDEN).unwrap().annihilator_count = Some(1);
    Arc::make_mut(state.card_db.as_mut().unwrap()).cards.get_mut(&COMMANDER).unwrap()
        .triggered_abilities.push(TriggeredAbility { trigger: TriggerCondition::Dies,
            effect: Effect::GainLife { amount: 1 }, description: "die".into() });
    let attacker = state.create_card_in_zone(HIDDEN, 0, ZoneType::Battlefield).unwrap();
    state.objects.get_mut(&attacker).unwrap().summoning_sick = false;
    let id = add(&mut state, 1, 0);
    state.phase = Phase::DeclareAttackers;
    rules::apply_action(&mut state, &Action::DeclareAttackers { attackers: vec![attacker] });
    assert!(state.players[1].graveyard.contains(&id));
    assert!(state.pending_commander_sba.is_some());
    assert!(state.pending_triggers.iter().any(|trigger| trigger.context.zone_transition.as_ref()
        .is_some_and(|context| context.subject.creature_died())));
}

#[test]
fn prior_game_state_format_is_rejected() {
    let state = fixture(2);
    let mut json = serde_json::to_value(&state).unwrap();
    json["state_format"] = serde_json::json!(0x3246_3154u32);
    assert!(serde_json::from_value::<GameState>(json).is_err());
    let mut bytes = bincode::serialize(&state).unwrap();
    bytes[..4].copy_from_slice(&0x3246_3154u32.to_le_bytes());
    assert!(bincode::deserialize::<GameState>(&bytes).is_err());
}

#[test]
fn pending_choice_public_key_retains_resumption_context() {
    let mut state = fixture(2);
    let id = add(&mut state, 0, 0);
    arrival(&mut state, id, ZoneType::Graveyard);
    rules::check_state_based_actions(&mut state);
    let mut other_priority = state.clone();
    other_priority.pending_commander_sba.as_mut().unwrap().resume_priority = 1;
    assert!(other_priority.validate_commander_choices().is_ok());
    let hash = |game: &GameState| InformationSet::from_view(&game.visible_state(0), game.card_db())
        .unwrap().hash_value();
    assert_ne!(hash(&state), hash(&other_priority));
}

#[test]
fn commander_self_sacrifice_refresh_failure_does_not_pay_costs() {
    use mtg_gto::card::{ActivatedAbility, Effect, SacrificeCost};
    use mtg_gto::layers::StaticAbility;
    use mtg_gto::mana::ManaCost;
    let mut state = fixture(2);
    let def = Arc::make_mut(state.card_db.as_mut().unwrap()).cards.get_mut(&COMMANDER).unwrap();
    def.static_abilities.push(StaticAbility::WonderInGraveyard);
    def.activated_abilities.push(ActivatedAbility { cost: ManaCost::new(1, 0, 0, 0, 0, 0),
        requires_tap: true, sacrifice_cost: Some(SacrificeCost::SelfSacrifice), life_cost: 1,
        effect: Effect::GainLife { amount: 1 }, description: "cost".into() });
    let id = add(&mut state, 0, 0);
    state.players[0].mana_pool.colorless = 2;
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    state.next_timestamp = u32::MAX;
    let mut before = state.clone();
    rules::apply_action(&mut state, &Action::ActivateAbility { object_id: id, ability_index: 0, targets: vec![] });
    assert!(state.sba_failure.is_some());
    assert!(state.battlefield.contains(&id));
    assert!(!state.objects[&id].tapped);
    assert_eq!(state.players[0].mana_pool.colorless, 2);
    assert_eq!(state.players[0].life, 40);
    assert!(state.stack.is_empty());
    before.sba_failure = state.sba_failure.clone();
    assert_eq!(serde_json::to_value(&state).unwrap(), serde_json::to_value(&before).unwrap());
}

#[test]
fn annihilator_partner_deaths_are_one_event_and_resume_after_ordering() {
    use mtg_gto::card::{Effect, TriggerCondition, TriggeredAbility};
    let mut state = fixture(2);
    Arc::make_mut(state.card_db.as_mut().unwrap()).cards.get_mut(&HIDDEN).unwrap().annihilator_count = Some(2);
    Arc::make_mut(state.card_db.as_mut().unwrap()).cards.get_mut(&COMMANDER).unwrap()
        .triggered_abilities.push(TriggeredAbility { trigger: TriggerCondition::ACreatureDies,
            effect: Effect::GainLife { amount: 1 }, description: "watch both".into() });
    let attacker = state.create_card_in_zone(HIDDEN, 0, ZoneType::Battlefield).unwrap();
    state.objects.get_mut(&attacker).unwrap().summoning_sick = false;
    let a = add(&mut state, 1, 0);
    let b = add(&mut state, 1, 1);
    state.phase = Phase::DeclareAttackers;
    rules::apply_action(&mut state, &Action::DeclareAttackers { attackers: vec![attacker] });
    assert_eq!(state.pending_triggers.len(), 4);
    let groups: std::collections::HashSet<_> = state.pending_triggers.iter().map(|trigger|
        trigger.context.zone_transition.as_ref().unwrap().group_id).collect();
    assert_eq!(groups.len(), 1);
    assert!(state.pending_commander_sba.is_some());
    assert!(state.trigger_placement.is_none());
    rules::apply_action(&mut state, &choice(1, 0, true));
    assert!(state.players[1].graveyard.contains(&a));
    assert!(state.players[1].graveyard.contains(&b));
    rules::apply_action(&mut state, &choice(1, 1, true));
    assert!(state.players[1].command_zone.contains(&a));
    assert!(state.players[1].command_zone.contains(&b));
    let mut steps = 0;
    while state.trigger_placement.is_some() {
        let action = legal_actions(&state).into_iter()
            .find(|action| matches!(action, Action::ChooseNextTrigger { .. })).unwrap();
        rules::apply_action(&mut state, &action);
        steps += 1;
        assert!(steps <= 4);
    }
    assert!(steps > 0);
    assert_eq!(state.phase, Phase::DeclareBlockers);
    assert_eq!(state.priority_player, 1);
}

#[test]
fn death_keywords_resolve_against_post_choice_destination() {
    use mtg_gto::card::KeywordAbility;
    for keyword in [KeywordAbility::Undying, KeywordAbility::Persist] {
        for to_command in [false, true] {
            let mut state = fixture(2);
            let id = add(&mut state, 0, 0);
            state.objects.get_mut(&id).unwrap().temp_keywords.push(keyword);
            state.objects.get_mut(&id).unwrap().temp_toughness_mod = -2;
            rules::check_state_based_actions(&mut state);
            assert_eq!(state.pending_triggers.len(), 1);
            rules::apply_action(&mut state, &choice(0, 0, to_command));
            assert_eq!(state.stack.len(), 1);
            for _ in 0..2 { rules::apply_action(&mut state, &Action::PassPriority); }
            if to_command {
                assert!(state.players[0].command_zone.contains(&id));
                assert!(!state.battlefield.contains(&id));
            } else {
                assert!(state.battlefield.contains(&id));
                assert!(state.consumed_commander_arrivals.is_empty());
                match keyword {
                    KeywordAbility::Undying => assert_eq!(state.objects[&id].plus_counters, 1),
                    KeywordAbility::Persist => assert_eq!(state.objects[&id].minus_counters, 1),
                    _ => unreachable!(),
                }
            }
        }
    }
}

#[test]
fn nonactive_owner_choice_after_trigger_free_resolution_restores_priority() {
    use mtg_gto::{card::{Effect, TargetSpec}, game::Target, mana::ManaCost};
    const DESTROY: u64 = 998_204;
    let mut state = fixture(2);
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: DESTROY,
        name: "Destroy".into(), card_types: vec![CardType::Sorcery],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::DestroyTarget { target: TargetSpec::AnyCreature }),
        ..Default::default() });
    let commander = add(&mut state, 1, 0);
    let spell = state.create_card_in_zone(DESTROY, 0, ZoneType::Hand).unwrap();
    rules::apply_action(&mut state, &Action::CastSpell { object_id: spell, targets: vec![Target::Object(commander)] });
    for _ in 0..2 { rules::apply_action(&mut state, &Action::PassPriority); }
    assert!(state.players[1].graveyard.contains(&commander));
    assert!(state.pending_commander_sba.is_some());
    assert!(state.pending_triggers.is_empty());
    assert_eq!(state.priority_player, 1);
    rules::apply_action(&mut state, &choice(1, 0, false));
    assert!(state.pending_commander_sba.is_none());
    assert_eq!(state.priority_player, 0);
    assert!(state.stack.is_empty());
}

#[test]
fn continuing_multiplayer_elimination_does_not_transfer_choice() {
    let mut state = fixture(3);
    let id = add(&mut state, 1, 0);
    arrival(&mut state, id, ZoneType::Graveyard);
    rules::check_state_based_actions(&mut state);
    assert_eq!(state.priority_player, 1);
    rules::apply_action(&mut state, &Action::Concede);
    assert!(state.gameplay_stopped());
    assert!(state.loss_boundary.unsupported.is_some());
    assert_eq!(state.pending_commander_sba.as_ref().unwrap().chooser(), Some(1));
    assert!(state.players[1].graveyard.contains(&id));
}
