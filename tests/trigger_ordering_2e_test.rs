use std::{collections::HashSet, sync::Arc};
use mtg_gto::{action::{Action, legal_actions, legal_actions_abstracted, canonical::{canonicalize, resolve}},
    card::{CardDef, CardType, Effect, TriggerCondition, TriggeredAbility, ZoneType},
    game::{CardDatabase, GameState, PendingTrigger, Phase, StackSource, Target, TriggerOrderResume},
    info_set::InformationSet, rules};

fn fixture(players: usize, count: usize) -> GameState {
    let mut db = CardDatabase::new();
    db.insert(CardDef { id: 999_501, name: "Occurrence source".into(), card_types: vec![CardType::Creature],
        power: Some(2), toughness: Some(2), triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::LeavesBattlefield, effect: Effect::GainLife { amount: 1 }, description: "Gain one life".into(),
        }], ..Default::default() });
    let mut s = GameState::new(players); s.card_db = Some(Arc::new(db)); s.phase = Phase::PreCombatMain;
    let source = s.create_card_in_zone(999_501, 0, ZoneType::Battlefield).unwrap();
    for _ in 0..count { s.pending_triggers.push(PendingTrigger::from_source(&s, source, 0, 0, vec![]).unwrap()); }
    rules::check_state_based_actions(&mut s); s
}
fn choices(s: &GameState) -> Vec<Action> { legal_actions(s).into_iter().filter(|a| matches!(a, Action::ChooseNextTrigger { .. })).collect() }
fn info(s: &GameState) -> u64 { InformationSet::from_view(&s.visible_state(s.priority_player), s.card_db()).unwrap().hash_value() }
fn variants(s: &GameState) -> Vec<GameState> {
    let mut snap = s.clone(); snap.restore(s.snapshot()).unwrap();
    let mut all = vec![s.clone(), snap, serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap(), bincode::deserialize(&bincode::serialize(s).unwrap()).unwrap()];
    for v in &mut all { v.card_db = s.card_db.clone(); } all
}
fn finish(s: &mut GameState) { while s.trigger_placement.is_some() { let a = choices(s).pop().unwrap(); rules::apply_action(s, &a); } }

#[test]
fn every_occurrence_is_available_at_each_prefix_without_permutations() {
    for n in [2, 6, 7, 10, 20] {
        let mut s = fixture(2, n); let mut total = 0;
        for remaining in (1..=n).rev() {
            let actions = choices(&s); assert_eq!(actions.len(), remaining); total += actions.len();
            assert_eq!(legal_actions(&s), legal_actions_abstracted(&s));
            assert!(!legal_actions(&s).contains(&Action::PassPriority));
            let old_stack = s.stack.len();
            let a = actions.last().unwrap(); // deliberately reverse collection order
            let key = canonicalize(a, &s).unwrap(); assert_eq!(resolve(&key, &s, 0).unwrap(), Some(a.clone()));
            let keys: HashSet<_> = actions.iter().map(|a| canonicalize(a, &s).unwrap()).collect();
            assert_eq!(keys.len(), remaining, "duplicate-looking occurrences remain distinct");
            rules::apply_action(&mut s, a); assert_eq!(s.stack.len(), old_stack + 1);
            assert!(s.validate_trigger_placement().is_ok());
        }
        assert_eq!(total, n*(n+1)/2); assert!(s.pending_triggers.is_empty());
        assert!(s.trigger_order_resume.is_none()); assert!(legal_actions(&s).contains(&Action::PassPriority));
    }
}

#[test]
fn three_and_four_player_apnap_preserves_controller_boundaries_and_resume() {
    for players in [3,4] {
        let mut s = fixture(players, 0); s.active_player = players-1; s.priority_player = 1;
        let source = s.battlefield[0];
        for p in (0..players).rev() { for _ in 0..2 { s.pending_triggers.push(PendingTrigger::from_source(&s,source,0,p,vec![]).unwrap()); } }
        rules::check_state_based_actions(&mut s);
        let other = s.pending_triggers.iter().find(|t| t.controller != s.priority_player).unwrap().placement.unwrap();
        let before = bincode::serialize(&s).unwrap();
        rules::apply_action(&mut s, &Action::ChooseNextTrigger { handle: other });
        assert_eq!(before, bincode::serialize(&s).unwrap(), "other controller's valid handle must not mutate state");
        for p in (0..players).map(|i| (i+players-1)%players) {
            for remaining in [2,1] { assert_eq!(s.priority_player,p); assert_eq!(choices(&s).len(),remaining);
                let a = choices(&s).pop().unwrap(); rules::apply_action(&mut s,&a); assert_eq!(s.stack.last().unwrap().controller,p); }
        }
        assert_eq!(s.priority_player,1); assert!(s.trigger_order_resume.is_none());
    }
}

#[test]
fn restored_prefix_has_identical_choices_and_completion() {
    let mut s = fixture(2,7); let a=choices(&s).pop().unwrap(); rules::apply_action(&mut s,&a);
    let hash=info(&s); let mut expected=s.clone(); finish(&mut expected);
    for mut v in variants(&s) { assert_eq!(info(&v),hash); assert_eq!(choices(&v),choices(&s)); finish(&mut v);
        assert_eq!(serde_json::to_value(v).unwrap(),serde_json::to_value(&expected).unwrap()); }
}

#[test]
fn stale_and_wrong_controller_actions_are_byte_for_byte_noops() {
    let mut s=fixture(2,2); let old=choices(&s)[0].clone(); rules::apply_action(&mut s,&old);
    let before=bincode::serialize(&s).unwrap(); rules::apply_action(&mut s,&old); assert_eq!(before,bincode::serialize(&s).unwrap());
    for action in [Action::Equip { equipment_id:u64::MAX,target_id:u64::MAX },Action::PassPriority,Action::EndTurn,Action::OrderTriggers { ordering:vec![] }] {
        rules::apply_action(&mut s,&action); assert_eq!(before,bincode::serialize(&s).unwrap()); }
    finish(&mut s); let source=s.battlefield[0];
    for _ in 0..2 { s.pending_triggers.push(PendingTrigger::from_source(&s,source,0,0,vec![]).unwrap()); }
    rules::check_state_based_actions(&mut s); let before=bincode::serialize(&s).unwrap();
    rules::apply_action(&mut s,&old); assert_eq!(before,bincode::serialize(&s).unwrap());
}

#[test]
fn malformed_operations_fail_observation_actions_and_restore() {
    let mut s=fixture(2,3); let a=choices(&s)[0].clone(); rules::apply_action(&mut s,&a);
    for fault in 0..10 { let mut bad=s.clone(); match fault {
        0=>bad.trigger_placement.as_mut().unwrap().version=99,
        1=>bad.trigger_order_resume=Some(TriggerOrderResume::Player(usize::MAX)),
        2=>bad.pending_triggers[0].placement=bad.pending_triggers[1].placement,
        3=>bad.stack.last_mut().unwrap().controller=1,
        4=>bad.trigger_placement.as_mut().unwrap().placed.push(999),
        5=>bad.trigger_placement=None,
        6=>bad.phase=Phase::Mulligan,
        7=>bad.trigger_order_resume=Some(TriggerOrderResume::AfterAttackers),
        8=>if let StackSource::TriggeredAbility { context,.. }=&mut bad.stack.last_mut().unwrap().source {context.effect=Effect::GainLife { amount:99 };},
        _=>bad.pending_triggers[0].controller=1,
    }
    assert!(bad.validate_trigger_placement().is_err()); assert!(legal_actions(&bad).is_empty());
    assert!(InformationSet::from_view(&bad.visible_state(0),bad.card_db()).is_err());
    let mut restored=s.clone(); assert!(restored.restore(bad.snapshot()).is_err());
    rules::apply_action(&mut bad,&Action::PassPriority); assert!(bad.sba_failure.is_some()); }
    let mut legacy=serde_json::to_value(&s).unwrap(); legacy.as_object_mut().unwrap().remove("trigger_placement");
    assert!(serde_json::from_value::<GameState>(legacy).is_err());
    let bytes=bincode::serialize(&s).unwrap();
    assert!(bincode::deserialize::<GameState>(&bytes[4..]).is_err(), "pre-2E untagged positional payload rejected");
    let mut incompatible=bytes; incompatible[..4].copy_from_slice(&0u32.to_le_bytes());
    assert!(bincode::deserialize::<GameState>(&incompatible).is_err());
}

#[test]
fn no_sba_or_priority_between_selections_but_final_selection_settles() {
    let mut s=fixture(2,3); s.players[1].life=0;
    rules::check_state_based_actions(&mut s); assert!(!s.game_over);
    for _ in 0..2 { let a=choices(&s)[0].clone(); rules::apply_action(&mut s,&a); assert!(!s.game_over); }
    let a=choices(&s)[0].clone(); rules::apply_action(&mut s,&a); assert!(s.game_over); assert!(s.players[1].has_lost);
}

#[test]
fn later_arrivals_use_a_new_window_and_concession_restores() {
    let mut s=fixture(2,2); let old=choices(&s)[0].clone(); let source=s.battlefield[0];
    for _ in 0..2 { s.pending_triggers.push(PendingTrigger::from_source(&s,source,0,1,vec![]).unwrap()); }
    assert_eq!(choices(&s).len(),2); rules::apply_action(&mut s,&old);
    let a=choices(&s)[0].clone(); rules::apply_action(&mut s,&a); assert_eq!(s.priority_player,1);
    let before=bincode::serialize(&s).unwrap(); rules::apply_action(&mut s,&old); assert_eq!(before,bincode::serialize(&s).unwrap());
    rules::apply_action(&mut s,&Action::Concede); assert!(s.game_over);
    for v in variants(&s) { assert!(v.validate_trigger_placement().is_ok()); let _=info(&v); }
}

#[test]
fn captured_targets_and_occurrence_permutations_have_stable_meaning() {
    let mut a=fixture(2,0); let source=a.battlefield[0];
    for player in 0..2 { a.pending_triggers.push(PendingTrigger::from_source(&a,source,0,0,vec![Target::Player(player)]).unwrap()); }
    let mut b=a.clone(); b.pending_triggers.reverse(); rules::check_state_based_actions(&mut a); rules::check_state_based_actions(&mut b);
    assert_eq!(info(&a),info(&b));
    for action in choices(&a) { let key=canonicalize(&action,&a).unwrap(); let translated=resolve(&key,&b,0).unwrap().unwrap();
        let mut x=a.clone(); let mut y=b.clone(); rules::apply_action(&mut x,&action); rules::apply_action(&mut y,&translated);
        assert_eq!(x.stack.last().unwrap().targets,y.stack.last().unwrap().targets); assert_eq!(info(&x),info(&y)); }
}

#[test]
fn historical_departures_survive_token_cessation_and_hidden_changes() {
    use mtg_gto::rules::transitions::{transition_batch,TransitionRequest,ExactObjectRef,MovementKind};
    let mut s=fixture(2,0); let source=s.battlefield[0];
    s.objects.get_mut(&source).unwrap().is_token=true;
    let other=s.create_card_in_zone(999_501,0,ZoneType::Battlefield).unwrap();
    let requests:Vec<_>=[source,other].into_iter().map(|id|TransitionRequest { object:ExactObjectRef { id,generation:0 },from:ZoneType::Battlefield,to:ZoneType::Graveyard,kind:MovementKind::Put }).collect();
    transition_batch(&mut s,&requests).unwrap(); rules::check_state_based_actions(&mut s);
    assert!(!s.objects.contains_key(&source)); assert_eq!(choices(&s).len(),2);
    let mut hidden=s.clone(); hidden.create_card_in_zone(999_501,1,ZoneType::Hand).unwrap();
    let mut hidden2=hidden.clone(); hidden2.objects.get_mut(hidden2.players[1].hand.last().unwrap()).unwrap().card_def_id=123456;
    assert_eq!(info(&hidden),info(&hidden2));
    for action in choices(&s) { let key=canonicalize(&action,&s).unwrap(); assert_eq!(resolve(&key,&s,0).unwrap(),Some(action)); }
    finish(&mut s); assert!(s.stack.iter().all(|e| matches!(e.source,StackSource::TriggeredAbility{..})));
}

#[test]
fn malformed_ordering_disposes_solver_attempt_and_prevents_publication() {
    use mtg_gto::{solver::{RegretTable, mccfr::{run_iteration, McfrConfig}}, simulation::TerminationReason};
    let mut s=fixture(2,3); let a=choices(&s)[0].clone(); rules::apply_action(&mut s,&a);
    s.trigger_placement.as_mut().unwrap().version=99;
    let before=bincode::serialize(&s).unwrap();
    let mut tables=[RegretTable::new(),RegretTable::new()];
    tables[0].get_or_create(42).unwrap().visit_count=1;
    let result=run_iteration(&s,&mut tables,&McfrConfig { max_depth:2,max_actions:10,max_nodes_per_iteration:20 });
    assert_eq!(result,Err(TerminationReason::StateEncoding));
    assert!(tables.iter().all(|table| table.entries().is_err() && table.to_bytes().is_err()));
    assert_eq!(before,bincode::serialize(&s).unwrap());
}

#[test]
fn distinct_historical_subjects_from_one_departed_source_reconstruct_non_fifo() {
    use mtg_gto::rules::transitions::{transition_batch, TransitionRequest, ExactObjectRef, MovementKind};
    let mut s=fixture(2,0);
    let mut observer=s.card_db().get(999_501).unwrap().clone(); observer.id=999_502;
    observer.triggered_abilities[0].trigger=TriggerCondition::ACreatureDies;
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(observer);
    let watcher=s.create_card_in_zone(999_502,0,ZoneType::Battlefield).unwrap();
    let first=s.battlefield[0]; let second=s.create_card_in_zone(999_501,0,ZoneType::Battlefield).unwrap();
    // Remove the subjects' self triggers to isolate two histories of one ability.
    Arc::make_mut(s.card_db.as_mut().unwrap()).cards.get_mut(&999_501).unwrap().triggered_abilities.clear();
    for id in [first,second,watcher] {
        transition_batch(&mut s,&[TransitionRequest { object:ExactObjectRef { id,generation:0 },from:ZoneType::Battlefield,
            to:if id==watcher {ZoneType::Exile} else {ZoneType::Graveyard},kind:MovementKind::Put }]).unwrap();
    }
    rules::check_state_based_actions(&mut s); assert_eq!(s.pending_triggers.len(),2);
    let contexts:Vec<_>=s.pending_triggers.iter().map(|t|t.context.zone_transition.as_ref().unwrap()).collect();
    assert_ne!(contexts[0].group_id,contexts[1].group_id); assert_ne!(contexts[0].subject.before.object,contexts[1].subject.before.object);
    let chosen=choices(&s).last().unwrap().clone(); let key=canonicalize(&chosen,&s).unwrap();
    for mut v in variants(&s) {
        v.pending_triggers.reverse(); let action=resolve(&key,&v,0).unwrap().unwrap(); rules::apply_action(&mut v,&action);
        let StackSource::TriggeredAbility { source_id,context,.. }=&v.stack.last().unwrap().source else {panic!()};
        assert_eq!(*source_id,watcher); assert_eq!(context.zone_transition.as_ref().unwrap().subject.before.object.id,second);
        finish(&mut v); assert_eq!(v.stack.len(),2);
    }
}

#[test]
fn allocation_and_window_counters_do_not_enter_semantic_keys_and_policies_accept_choices() {
    use mtg_gto::{strategy::{Strategy,GreedyStrategy,GoldfishStrategy}, solver::mcts::{MctsConfig,MctsStrategy}};
    let mut a=fixture(2,7); let mut b=fixture(2,0);
    // Build the same public source at a different runtime allocation.
    b.objects.clear(); b.battlefield.clear(); b.next_object_id=800;
    let source=b.create_card_in_zone(999_501,0,ZoneType::Battlefield).unwrap(); b.next_trigger_window=90;
    for _ in 0..7 {b.pending_triggers.push(PendingTrigger::from_source(&b,source,0,0,vec![]).unwrap());}
    b.pending_triggers.reverse(); rules::check_state_based_actions(&mut b);
    for _ in 0..7 {
        assert_eq!(info(&a),info(&b));
        let keys=|s:&GameState|choices(s).iter().map(|a|canonicalize(a,s).unwrap()).collect::<HashSet<_>>();
        assert_eq!(keys(&a),keys(&b));
        let action=choices(&a).pop().unwrap(); let key=canonicalize(&action,&a).unwrap(); let mapped=resolve(&key,&b,0).unwrap().unwrap();
        rules::apply_action(&mut a,&action); rules::apply_action(&mut b,&mapped);
    }
    for strategy in [&GreedyStrategy as &dyn Strategy,&GoldfishStrategy] {
        let s=fixture(2,7); assert!(choices(&s).contains(&strategy.choose_action(&s,0).unwrap()));
    }
    let s=fixture(2,7); let mcts=MctsStrategy::new(MctsConfig { iterations_per_move:4,max_tree_depth:2,max_rollout_actions:3,..Default::default() });
    assert!(legal_actions(&s).contains(&mcts.choose_action(&s,0).unwrap()));
}

#[test]
fn allocation_failure_stops_before_consumption_and_unrelated_actions_remain_noops() {
    let mut s=fixture(2,0); let source=s.battlefield[0]; s.next_stack_id=u64::MAX;
    for _ in 0..2 {s.pending_triggers.push(PendingTrigger::from_source(&s,source,0,0,vec![]).unwrap());}
    rules::check_state_based_actions(&mut s); let before=bincode::serialize(&s).unwrap();
    rules::apply_action(&mut s,&Action::Equip { equipment_id:u64::MAX,target_id:u64::MAX });
    assert_eq!(before,bincode::serialize(&s).unwrap());
    let action=choices(&s)[0].clone(); rules::apply_action(&mut s,&action);
    assert!(s.sba_failure.is_some()); assert!(s.stack.is_empty()); assert_eq!(s.pending_triggers.len(),2);
}
