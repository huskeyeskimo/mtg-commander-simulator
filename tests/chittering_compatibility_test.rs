use std::sync::Arc;
use mtg_gto::{action::{legal_actions, Action}, card::{sample, TriggerCondition, ZoneType}, game::{GameState, Phase, StackEntry, StackSource, Target, TriggerContext}, rules};

fn fixture(players: usize) -> (GameState, u64, Vec<u64>) { fixture_allocated(players,1) }
fn fixture_allocated(players: usize, allocation: u64) -> (GameState, u64, Vec<u64>) {
    let mut s = GameState::new_commander(players); s.next_object_id=allocation; s.next_stack_id=allocation; s.card_db = Some(Arc::new(sample::build_sample_db())); s.phase = Phase::PreCombatMain;
    let source = s.create_card_in_zone(sample::ids::CHITTERING_RATS, 0, ZoneType::Battlefield).unwrap();
    let first = s.create_card_in_zone(sample::ids::SWAMP, 1, ZoneType::Hand).unwrap();
    let second = s.create_card_in_zone(sample::ids::SOL_RING, 1, ZoneType::Hand).unwrap();
    s.create_card_in_zone(sample::ids::SWAMP, 1, ZoneType::Library).unwrap(); s.drain_events();
    (s, source, vec![first, second])
}
fn force_targeted_trigger(s: &mut GameState, source: u64, opponent: usize) {
    let effect = s.card_db().get(sample::ids::CHITTERING_RATS).unwrap().triggered_abilities[0].effect.clone();
    let id = s.new_stack_id();
    s.stack.push(StackEntry {id, controller: 0, source: StackSource::TriggeredAbility {source_id: source, ability_index: 0,
        context: Box::new(TriggerContext {source_card_id: sample::ids::CHITTERING_RATS, source_generation: s.objects[&source].zone_change_count,
            effect, cast_spell: None, zone_transition: None})}, targets: vec![Target::Player(opponent)], target_generations: vec![None]});
}
fn resolve(s: &mut GameState) {for _ in 0..s.players.len() {rules::apply_action(s, &Action::PassPriority);}}
#[test]
fn singleton_chittering_trigger_requires_an_explicit_opponent_at_stack_placement() {
    let (mut s, source, _) = fixture(3);
    let _ = rules::fire_triggers(&mut s, TriggerCondition::EntersBattlefield, Some(source));
    assert!(s.trigger_placement.is_some(), "target selection must survive singleton auto-placement");
    assert!(s.stack.is_empty()); assert_eq!(s.priority_player, 0);
    let targets: Vec<_> = legal_actions(&s).into_iter().filter(|a| serde_json::to_value(a).unwrap().get("PlaceChitteringTrigger").is_some()).collect();
    assert_eq!(targets.len(), 2);
    let action = targets.into_iter().find(|a| serde_json::to_value(a).unwrap()["PlaceChitteringTrigger"]["opponent"] == 1).unwrap();
    rules::apply_action(&mut s, &action);
    assert!(s.trigger_placement.is_none()); assert_eq!(s.stack.len(), 1);
    assert_eq!(s.stack[0].targets, vec![Target::Player(1)]); assert_eq!(s.stack[0].target_generations, vec![None]);
}
#[test]
fn targeted_chittering_resolution_waits_for_the_opponents_private_hand_selection() {
    let (mut s, source, cards) = fixture(2); force_targeted_trigger(&mut s, source, 1);
    resolve(&mut s);
    assert!(!s.gameplay_stopped(), "the closed instruction must reach its mandatory private selection");
    assert_eq!(s.priority_player, 1); assert!(cards.iter().all(|id| s.players[1].hand.contains(id)));
    let choices: Vec<_> = legal_actions(&s).into_iter().filter(|a| serde_json::to_value(a).unwrap().get("ChooseChitteringCard").is_some()).collect();
    assert_eq!(choices.len(), 2); assert!(!legal_actions(&s).contains(&Action::PassPriority));
    let before = bincode::serialize(&s).unwrap(); rules::apply_action(&mut s, &Action::PassPriority); assert_eq!(bincode::serialize(&s).unwrap(), before);
    rules::apply_action(&mut s, &choices[0]);
    assert!(!s.gameplay_stopped()); assert_eq!(s.players[1].hand.len(), 1); assert_eq!(s.players[1].library.len(), 2);
    assert!(cards.contains(&s.players[1].library[0])); assert_eq!(s.objects[&s.players[1].library[0]].zone_change_count, 1);
    assert!(s.stack.is_empty()); assert_eq!(s.priority_player, s.active_player);
}

use mtg_gto::{action::canonical::{canonicalize, resolve as reconstruct}, card::{Effect, ExactObjectRef},
    game::MovementReplacementCandidate as Candidate, info_set::InformationSet,
    replacement::{MovementDestination as Destination, ReplacementAction, ReplacementEffect, ReplacementEventKind}};
fn choose(s: &mut GameState, id: u64) -> Action {
    let action = legal_actions(s).into_iter().find(|a| matches!(a, Action::ChooseChitteringCard { card, .. } if card.id == id)).unwrap();
    rules::apply_action(s, &action); action
}
fn answer(s: &mut GameState, accept: bool) -> Action {
    let action = legal_actions(s).into_iter().find(|a| matches!(a, Action::ChooseCommanderMovement { to_command, .. } if *to_command == accept)).unwrap();
    rules::apply_action(s, &action); action
}
fn redirect(s: &mut GameState, destination: Destination, to: ZoneType) {
    let source = s.create_card_in_zone(sample::ids::SOL_RING, 0, ZoneType::Battlefield).unwrap();
    s.replacement_effects.push(ReplacementEffect { source_id: source, controller: 0,
        applies_to: ReplacementEventKind::WouldMoveTo { destination, source_generation: 0 },
        action: ReplacementAction::RedirectToZone(to), is_self_replacement: false, description: "bounded redirect".into() });
}
fn pending(players: usize) -> (GameState, u64, Vec<u64>) {
    let (mut s, source, cards) = fixture(players); force_targeted_trigger(&mut s, source, 1); resolve(&mut s); (s, source, cards)
}
fn info(s: &GameState, viewer: usize) -> InformationSet { InformationSet::from_view(&s.visible_state(viewer), s.card_db()).unwrap_or_else(|e| panic!("viewer {viewer}: {e:?}; movement {:?}", s.validate_movement_play())) }
fn branches(s: &GameState) -> Vec<GameState> {
    let mut snapshot = s.clone(); snapshot.restore(s.snapshot()).unwrap();
    let mut variants = vec![s.clone(), snapshot, serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap(), bincode::deserialize(&bincode::serialize(s).unwrap()).unwrap()];
    for state in &mut variants {state.card_db = s.card_db.clone();} variants
}
fn noop(s: &mut GameState, action: &Action) {
    let before = bincode::serialize(s).unwrap(); rules::apply_action(s, action); assert_eq!(bincode::serialize(s).unwrap(), before);
}
#[test]
fn three_and_four_player_target_choices_exclude_controller_and_out_of_range_players() {
    for players in [3,4] {
        let (mut s, source, _) = fixture(players);
        let _ = rules::fire_triggers(&mut s, TriggerCondition::EntersBattlefield, Some(source));
        let legal = legal_actions(&s); assert_eq!(legal.iter().filter(|a| matches!(a, Action::PlaceChitteringTrigger { .. })).count(), players-1);
        let handle = s.pending_triggers[0].placement.unwrap();
        for opponent in [0, players] { noop(&mut s, &Action::PlaceChitteringTrigger { handle, opponent }); }
        for action in legal.into_iter().filter(|a| matches!(a, Action::PlaceChitteringTrigger { .. })) {
            let key = canonicalize(&action, &s).unwrap(); assert_eq!(reconstruct(&key, &s, 0).unwrap(), Some(action));
        }
    }
}
#[test]
fn empty_hand_is_a_legal_target_and_finishes_without_zone_or_draw_events() {
    let (mut s, source, cards) = fixture(3);
    for id in cards {s.try_move_object(id, ZoneType::Hand, ZoneType::Graveyard).unwrap();} s.drain_events();
    let _ = rules::fire_triggers(&mut s, TriggerCondition::EntersBattlefield, Some(source));
    let action = legal_actions(&s).into_iter().find(|a| matches!(a, Action::PlaceChitteringTrigger { opponent: 1, .. })).unwrap(); rules::apply_action(&mut s, &action);
    resolve(&mut s); assert!(s.pending_chittering.is_none()); assert!(s.pending_movement.is_none()); assert!(s.stack.is_empty());
    assert!(!s.gameplay_stopped()); assert!(s.drain_events().iter().all(|e| !matches!(e, mtg_gto::events::GameEvent::ZoneChange { .. } | mtg_gto::events::GameEvent::CardDrawn { .. })));
}
#[test]
fn existing_lost_player_boundary_stops_resolution_and_stale_pending_player_rejects_atomic() {
    let (mut s, source, _) = fixture(3); force_targeted_trigger(&mut s, source, 1); s.players[1].has_lost = true; resolve(&mut s);
    assert_eq!(s.stack.len(),1); assert!(s.pending_chittering.is_none()); assert!(s.gameplay_stopped(), "existing continuing-elimination boundary stops gameplay");
    let (mut s, _, _) = pending(3); let action = legal_actions(&s).into_iter().find(|a| matches!(a, Action::ChooseChitteringCard { .. })).unwrap();
    s.players[1].has_lost = true; noop(&mut s, &action); assert!(serde_json::from_slice::<GameState>(&serde_json::to_vec(&s).unwrap()).is_err());
}
#[test]
fn own_hand_selection_rejects_wrong_card_generation_player_and_window_without_events() {
    let (mut s, _, cards) = pending(3); let other = s.create_card_in_zone(sample::ids::SWAMP, 2, ZoneType::Hand).unwrap(); s.drain_events();
    let window = s.pending_chittering.as_ref().unwrap().resolving_entry.id;
    for action in [Action::ChooseChitteringCard { window, card: ExactObjectRef { id: cards[0], generation: 99 } },
        Action::ChooseChitteringCard { window: window+1, card: s.exact_object(cards[0]).unwrap() },
        Action::ChooseChitteringCard { window, card: s.exact_object(other).unwrap() }, Action::PassPriority] {noop(&mut s, &action);}
    assert!(s.drain_events().is_empty());
}
#[test]
fn distinct_duplicate_cards_have_complete_canonical_orbit_and_exact_once_top_commit() {
    let (mut s, source, mut cards) = fixture(2); cards.push(s.create_card_in_zone(sample::ids::SWAMP, 1, ZoneType::Hand).unwrap());
    force_targeted_trigger(&mut s, source, 1); resolve(&mut s); s.drain_events(); let old_library = s.players[1].library.clone();
    let actions: Vec<_> = legal_actions(&s).into_iter().filter(|a| matches!(a, Action::ChooseChitteringCard { .. })).collect(); assert_eq!(actions.len(), 3);
    for a in &actions {let key = canonicalize(a, &s).unwrap(); let found = reconstruct(&key, &s, 1).unwrap().unwrap(); assert_eq!(canonicalize(&found, &s).unwrap(), key);}
    let old = choose(&mut s, cards[2]); assert_eq!(s.players[1].library, [vec![cards[2]], old_library].concat());
    assert_eq!(s.objects[&cards[2]].zone_change_count, 1); noop(&mut s, &old);
    let events = s.drain_events(); assert_eq!(events.iter().filter(|e| matches!(e, mtg_gto::events::GameEvent::ZoneChange { object, .. } if *object == cards[2])).count(), 1);
    assert!(!events.iter().any(|e| matches!(e, mtg_gto::events::GameEvent::CardDrawn { .. })));
}
#[test]
fn captured_source_leaves_returns_or_token_ceases_without_changing_trigger_contract() {
    for token in [false,true] {
        let (mut s, source, cards) = fixture(2); s.objects.get_mut(&source).unwrap().is_token = token;
        force_targeted_trigger(&mut s, source, 1); s.try_move_object(source, ZoneType::Battlefield, ZoneType::Graveyard).unwrap();
        if token {s.players[0].graveyard.retain(|&id| id != source); s.objects.remove(&source);} else {s.try_move_object(source, ZoneType::Graveyard, ZoneType::Hand).unwrap();}
        resolve(&mut s); assert!(s.pending_chittering.is_some());
        for mut branch in branches(&s) {choose(&mut branch, cards[0]); assert_eq!(branch.players[1].library[0], cards[0]); assert!(!branch.gameplay_stopped());}
    }
}
#[test]
fn commander_accept_decline_are_owner_choices_and_card_stays_in_hand_until_final_commit() {
    for accept in [false,true] {
        let (mut s, source, cards) = fixture(3); s.designate_commander(cards[0], 1).unwrap(); force_targeted_trigger(&mut s, source, 1); resolve(&mut s);
        choose(&mut s, cards[0]); assert_eq!(s.priority_player, 1); assert!(s.pending_chittering.is_none()); assert!(s.pending_movement.is_some());
        assert!(s.players[1].hand.contains(&cards[0])); assert_eq!(s.objects[&cards[0]].zone_change_count, 0);
        for mut state in branches(&s) {answer(&mut state, accept); assert!(state.pending_movement.is_none()); assert_eq!(state.objects[&cards[0]].zone_change_count, 1);
            assert!(if accept {state.players[1].command_zone.contains(&cards[0])} else {state.players[1].library[0] == cards[0]});}
    }
}
#[test]
fn private_wait_is_identical_before_selection_and_through_competing_repeated_commander_prefixes() {
    let (mut s, source, cards) = fixture(4); s.designate_commander(cards[0], 1).unwrap();
    redirect(&mut s, Destination::Library, ZoneType::Hand); redirect(&mut s, Destination::Hand, ZoneType::Library); redirect(&mut s, Destination::Command, ZoneType::Library);
    force_targeted_trigger(&mut s, source, 1); resolve(&mut s);
    let original = s.clone(); let observers: Vec<_> = [0,2,3].map(|p| info(&s,p)).into_iter().collect();
    let check = |s: &GameState| {for (index,p) in [0,2,3].into_iter().enumerate() {assert_eq!(info(s,p).zone_normalization, observers[index].zone_normalization); assert_eq!(info(s,p).hash_value(), observers[index].hash_value());}};
    choose(&mut s, cards[0]); check(&s);
    let select = legal_actions(&s).into_iter().find(|a| matches!(a, Action::ChooseMovementReplacement { candidate: Candidate::Commander(_), .. })).unwrap(); rules::apply_action(&mut s, &select); check(&s);
    let old = answer(&mut s, true); check(&s); assert_eq!(s.pending_movement.as_ref().unwrap().proposals[0].proposed, ZoneType::Library);
    noop(&mut s, &old);
    let select = legal_actions(&s).into_iter().find(|a| matches!(a, Action::ChooseMovementReplacement { candidate: Candidate::Commander(_), .. })).unwrap(); rules::apply_action(&mut s,&select); check(&s);
    answer(&mut s, true); assert!(s.pending_movement.is_none()); assert!(s.players[1].command_zone.contains(&cards[0]));
    assert_ne!(info(&original,1).hash_value(), info(&s,1).hash_value());
}
#[test]
fn ordinary_and_commander_private_subjects_have_the_same_nonowner_wait_and_labels() {
    let (mut s, source, cards) = fixture(3); s.designate_commander(cards[0], 1).unwrap(); redirect(&mut s, Destination::Library, ZoneType::Hand);
    force_targeted_trigger(&mut s, source, 1); resolve(&mut s); let mut left = s.clone(); let mut right = s.clone();
    choose(&mut left, cards[0]); // mandatory competing choice remains
    // Normal card redirected to Hand cancels the actual movement and finishes;
    // compare only the protected unfinished prefix with the original wait.
    for p in [0,2] {assert_eq!(info(&left,p).hash_value(), info(&s,p).hash_value());}
    for a in legal_actions(&left) {assert_eq!(mtg_gto::simulation::format_action_name(&left,&a), "Private hand decision");}
    noop(&mut right, &Action::ChooseChitteringCard { window: 9999, card: s.exact_object(cards[1]).unwrap() });
}
#[test]
fn pending_selection_and_replacement_restore_and_reconstruct_without_database_mutation() {
    let (s, _, cards) = pending(3);
    for mut state in branches(&s) {assert_eq!(info(&state,1).hash_value(), info(&s,1).hash_value());
        for action in legal_actions(&state) {let key = canonicalize(&action,&state).unwrap(); assert_eq!(reconstruct(&key,&state,1).unwrap(),Some(action));}
        choose(&mut state,cards[0]); assert!(state.pending_chittering.is_none());}
    let mut detached: GameState = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    let action = legal_actions(&s).into_iter().find(|a| matches!(a,Action::ChooseChitteringCard { .. })).unwrap(); noop(&mut detached,&action);
    assert!(canonicalize(&action,&detached).is_err());
    assert!(mtg_gto::strategy::Strategy::choose_action(&mtg_gto::strategy::GreedyStrategy,&detached,1).is_err());
}
#[test]
fn malformed_selection_and_captured_context_restore_rejects_owned_contract_corruption() {
    let (s, _, _) = pending(3); let original = serde_json::to_value(&s).unwrap();
    for fault in 0..9 {let mut v = original.clone(); match fault {
        0 => v["pending_chittering"]["opponent"] = serde_json::json!(0),
        1 => v["pending_chittering"]["version"] = serde_json::json!(99),
        2 => v["pending_chittering"]["resolving_entry"]["targets"] = serde_json::json!([]),
        3 => v["pending_chittering"]["resolving_entry"]["source"]["TriggeredAbility"]["ability_index"] = serde_json::json!(1),
        4 => v["pending_chittering"]["resolving_entry"]["source"]["TriggeredAbility"]["context"]["source_card_id"] = serde_json::json!(sample::ids::SWAMP),
        5 => v["pending_chittering"]["resolving_entry"]["source"]["TriggeredAbility"]["context"]["source_generation"] = serde_json::json!(100),
        6 => v["pending_chittering"]["resolving_entry"]["source"]["TriggeredAbility"]["source_id"] = serde_json::json!(999999),
        7 => v["trigger_order_resume"] = serde_json::Value::Null,
        _ => v["priority_player"] = serde_json::json!(0),
    } assert!(serde_json::from_value::<GameState>(v).is_err(),"malformed private frame {fault}");}
}
#[test]
fn source_controller_is_effective_and_suppressed_printed_trigger_does_not_fire() {
    use mtg_gto::layers::{ContinuousEffect, AffectedObjects, LayerModification, Duration};
    for suppressed in [false,true] {
        let (mut s, source, _) = fixture(3);
        s.continuous_effects.push(ContinuousEffect {source_id: source, controller: 0, affected: AffectedObjects::Specific(source),
            modification: if suppressed {LayerModification::RemoveAllAbilities} else {LayerModification::ChangeController(2)},
            duration: Duration::Permanent, timestamp: 1});
        let _ = rules::fire_triggers(&mut s, TriggerCondition::EntersBattlefield, Some(source));
        if suppressed {assert!(s.pending_triggers.is_empty()); assert!(s.stack.is_empty());} else {
            assert_eq!(s.priority_player,2); assert!(legal_actions(&s).iter().all(|a| !matches!(a,Action::PlaceChitteringTrigger {opponent:2,..})));
        }
    }
}
#[test]
fn actual_printed_spell_enters_then_targets_and_finishes_private_top_placement() {
    let (mut s, source, cards) = fixture(2); s.try_move_object(source,ZoneType::Battlefield,ZoneType::Hand).unwrap();
    s.players[0].mana_pool.black=2; s.players[0].mana_pool.colorless=1;
    let cast = Action::CastSpell { object_id: source, targets: vec![] }; assert!(legal_actions(&s).contains(&cast)); rules::apply_action(&mut s,&cast); resolve(&mut s);
    assert!(s.battlefield.contains(&source)); assert!(s.trigger_placement.is_some());
    let action=legal_actions(&s).into_iter().find(|a| matches!(a,Action::PlaceChitteringTrigger {opponent:1,..})).unwrap(); rules::apply_action(&mut s,&action); resolve(&mut s);
    choose(&mut s,cards[1]); assert_eq!(s.players[1].library[0],cards[1]); assert!(!s.gameplay_stopped()); assert_eq!(s.objects[&source].zone_change_count,3);
}
#[test]
fn old_untargeted_trigger_action_and_nested_chittering_effect_reject_before_any_prefix() {
    let (mut s,source,_) = fixture(3); let _=rules::fire_triggers(&mut s,TriggerCondition::EntersBattlefield,Some(source));
    let handle=s.pending_triggers[0].placement.unwrap(); noop(&mut s,&Action::ChooseNextTrigger {handle});
    let (mut s,source,_) = fixture(2); force_targeted_trigger(&mut s,source,1);
    if let StackSource::TriggeredAbility {context,..}=&mut s.stack[0].source {context.effect=Effect::Multiple(vec![Effect::GainLife {amount:7},Effect::ChitteringRats]);}
    let life=s.players[0].life; let stack=bincode::serialize(&s.stack).unwrap(); resolve(&mut s);
    assert_eq!(s.players[0].life,life); assert_eq!(bincode::serialize(&s.stack).unwrap(),stack); assert!(s.pending_chittering.is_none());
    assert!(s.invalid_gameplay_reason().is_some() || legal_actions(&s).is_empty());
}
#[test]
fn mixed_chittering_triggers_preserve_incremental_apnap_and_chosen_target_history() {
    for players in [3,4] {
        let (mut s,source,_) = fixture(players);
        let _=rules::fire_triggers(&mut s,TriggerCondition::EntersBattlefield,Some(source));
        // Add occurrences to a fresh simultaneous window rather than altering
        // an already-owned window's frozen history.
        s.pending_triggers.clear(); s.trigger_placement=None; s.trigger_order_resume=None;
        let template = mtg_gto::game::PendingTrigger {placement:None,source_id:source,ability_index:0,controller:0,targets:vec![],context:TriggerContext {source_card_id:sample::ids::CHITTERING_RATS,source_generation:0,effect:Effect::ChitteringRats,cast_spell:None,zone_transition:None}};
        for controller in (0..players).rev() {let mut trigger=template.clone();trigger.controller=controller;s.pending_triggers.push(trigger);}
        let _=rules::fire_triggers(&mut s,TriggerCondition::EntersBattlefield,Some(source)); // queues another active-player occurrence
        for controller in 0..players {
            while s.trigger_placement.as_ref().is_some_and(|op|op.controller==controller) {
                assert_eq!(s.priority_player,controller);
                let action=legal_actions(&s).into_iter().find(|a|matches!(a,Action::PlaceChitteringTrigger {opponent,..} if *opponent==(controller+1)%players)).unwrap();
                rules::apply_action(&mut s,&action); assert!(s.validate_trigger_placement().is_ok());
                for restored in branches(&s) {assert!(restored.validate_trigger_placement().is_ok());}
            }
        }
        assert!(s.pending_triggers.is_empty()); assert!(s.trigger_placement.is_none());
        assert_eq!(s.stack.iter().map(|e|e.controller).collect::<Vec<_>>(), [vec![0,0],(1..players).collect()].concat());
    }
}
#[test]
fn private_commander_decline_reconsiders_after_each_ordinary_occurrence_once() {
    let (mut s,source,cards)=fixture(3);s.designate_commander(cards[0],1).unwrap();redirect(&mut s,Destination::Library,ZoneType::Hand);redirect(&mut s,Destination::Hand,ZoneType::Library);
    force_targeted_trigger(&mut s,source,1);resolve(&mut s);choose(&mut s,cards[0]);
    let ordinary=legal_actions(&s).into_iter().find(|a|matches!(a,Action::ChooseMovementReplacement {candidate:Candidate::Ordinary {..},..})).unwrap();rules::apply_action(&mut s,&ordinary);
    let select=legal_actions(&s).into_iter().find(|a|matches!(a,Action::ChooseMovementReplacement {candidate:Candidate::Commander(_),..})).unwrap();rules::apply_action(&mut s,&select);
    let old=answer(&mut s,false);assert_eq!(s.pending_movement.as_ref().unwrap().proposals[0].proposed,ZoneType::Library);noop(&mut s,&old);
    let op=s.pending_movement.as_ref().unwrap();assert_eq!(op.proposals[0].consumed.len(),2);assert_eq!(op.proposals[0].revision,2);assert!(s.players[1].hand.contains(&cards[0]));
    answer(&mut s,false);assert_eq!(s.players[1].library[0],cards[0]);assert_eq!(s.objects[&cards[0]].zone_change_count,1);
}
#[test]
fn private_child_malformed_subject_history_owned_entry_and_duplicate_stack_ids_reject_restore() {
    let (mut s,source,cards)=fixture(3);s.designate_commander(cards[0],1).unwrap();force_targeted_trigger(&mut s,source,1);resolve(&mut s);choose(&mut s,cards[0]);
    let original=serde_json::to_value(&s).unwrap();for fault in 0..6 {let mut v=original.clone();match fault {
        0=>v["pending_movement"]["targets"][0]=serde_json::json!({"Object":cards[1]}),
        1=>v["pending_movement"]["history"][0]["resulting_revision"]=serde_json::json!(8),
        2=>v["pending_movement"]["resolving_entry"]=serde_json::Value::Null,
        3=>v["pending_movement"]["resolving_entry"]["targets"][0]=serde_json::json!({"Player":0}),
        4=>v["pending_movement"]["proposals"][0]["owner"]=serde_json::json!(0),
        _=>v["pending_movement"]["resolving_entry"]["id"]=v["next_stack_id"].clone(),
    }assert!(serde_json::from_value::<GameState>(v).is_err(),"child corruption {fault}");}
    let entry=s.pending_movement.as_ref().unwrap().resolving_entry.as_ref().unwrap().as_ref().clone();s.stack.push(entry);assert!(serde_json::from_slice::<GameState>(&serde_json::to_vec(&s).unwrap()).is_err());
}
#[test]
fn mandatory_private_window_defers_lethal_sba_and_does_not_generate_priority_or_draw_events() {
    let (mut s,_,cards)=pending(2);s.players[0].life=0;s.drain_events();let group=s.next_zone_event_group_id;
    rules::check_state_based_actions(&mut s);assert!(!s.players[0].has_lost);assert!(s.pending_chittering.is_some());assert_eq!(s.next_zone_event_group_id,group);assert!(s.drain_events().is_empty());
    choose(&mut s,cards[0]);assert!(s.players[0].has_lost);assert!(s.gameplay_stopped());assert_eq!(s.objects[&cards[0]].zone_change_count,1);
}
#[test]
fn exhausted_commit_capacity_preserves_valid_private_prefix_and_exact_subject_residency() {
    let (mut s,source,cards)=fixture(2);s.designate_commander(cards[0],1).unwrap();force_targeted_trigger(&mut s,source,1);resolve(&mut s);choose(&mut s,cards[0]);
    let action=legal_actions(&s).into_iter().find(|a|matches!(a,Action::ChooseCommanderMovement {to_command:false,..})).unwrap();s.next_zone_event_group_id=u64::MAX;s.drain_events();noop(&mut s,&action);
    assert!(s.players[1].hand.contains(&cards[0]));assert_eq!(s.objects[&cards[0]].zone_change_count,0);assert!(s.drain_events().is_empty());s.next_zone_event_group_id=0;rules::apply_action(&mut s,&action);assert_eq!(s.players[1].library[0],cards[0]);
}
#[test]
fn automated_opponent_hand_choice_progresses_and_human_hand_choice_pauses_fast_forward() {
    let (mut s,source,cards)=fixture(2);s.active_player=1;s.priority_player=1;force_targeted_trigger(&mut s,source,1);resolve(&mut s);
    let count=rules::fast_forward_goldfish_turn_until_copy_choice(&mut s,0);assert!(count>0);assert!(s.pending_chittering.is_none());assert!(s.players[1].library.contains(&cards[0]));
    let (mut s,source,_)=fixture(2);s.active_player=1;s.priority_player=1;
    s.create_card_in_zone(sample::ids::SWAMP,0,ZoneType::Hand).unwrap();force_targeted_trigger(&mut s,source,0);s.stack[0].controller=1;resolve(&mut s);
    let before=bincode::serialize(&s).unwrap();assert_eq!(rules::fast_forward_goldfish_turn_until_copy_choice(&mut s,0),0);assert_eq!(bincode::serialize(&s).unwrap(),before);
}
#[test]
fn allocation_shift_preserves_target_and_private_selection_keys_and_reconstruction() {
    let (mut left,ls,lc)=fixture_allocated(3,1);let (mut right,rs,rc)=fixture_allocated(3,501);
    for (state,source) in [(&mut left,ls),(&mut right,rs)] {let _=rules::fire_triggers(state,TriggerCondition::EntersBattlefield,Some(source));}
    let keys=|s:&GameState| legal_actions(s).iter().map(|a|canonicalize(a,s).unwrap()).collect::<Vec<_>>();
    assert_eq!(info(&left,0).hash_value(),info(&right,0).hash_value());assert_eq!(keys(&left),keys(&right));
    for state in [&mut left,&mut right] {let action=legal_actions(state).into_iter().find(|a|matches!(a,Action::PlaceChitteringTrigger {opponent:1,..})).unwrap();rules::apply_action(state,&action);resolve(state);}
    for viewer in 0..3 {assert_eq!(info(&left,viewer).hash_value(),info(&right,viewer).hash_value());}
    assert_eq!(keys(&left),keys(&right));let action=legal_actions(&left).into_iter().find(|a|matches!(a,Action::ChooseChitteringCard {card,..} if card.id==lc[0])).unwrap();
    let key=canonicalize(&action,&left).unwrap();let mapped=reconstruct(&key,&right,1).unwrap().unwrap();assert!(matches!(mapped,Action::ChooseChitteringCard {card,..} if card.id==rc[0]));
    assert_eq!(reconstruct(&key,&right,0).unwrap(),None);
    assert!(canonicalize(&Action::PassPriority,&left).is_err());assert_eq!(reconstruct(&mtg_gto::action::canonical::CanonicalAction::PassPriority,&left,1).unwrap(),None);
}
#[test]
fn unrelated_hidden_hand_definitions_and_library_order_do_not_change_nonowner_wait() {
    let (mut s,_,_) = pending(3);s.create_card_in_zone(sample::ids::SOL_RING,2,ZoneType::Hand).unwrap();
    s.create_card_in_zone(sample::ids::SOL_RING,1,ZoneType::Library).unwrap();let mut other=s.clone();
    let hidden=other.players[2].hand[0];other.objects.get_mut(&hidden).unwrap().card_def_id=sample::ids::SWAMP;other.players[1].library.reverse();
    for viewer in [0,2] {if viewer==0 {assert_eq!(info(&s,viewer).hash_value(),info(&other,viewer).hash_value());}}
    let keys=|s:&GameState|legal_actions(s).iter().map(|a|canonicalize(a,s).unwrap()).collect::<Vec<_>>();assert_eq!(keys(&s),keys(&other));
}
#[test]
fn commander_and_noncommander_chosen_cards_are_opaque_at_the_same_unfinished_public_wait() {
    let (mut s,source,cards)=fixture(3);s.designate_commander(cards[0],1).unwrap();
    redirect(&mut s,Destination::Library,ZoneType::Command);redirect(&mut s,Destination::Library,ZoneType::Graveyard);
    force_targeted_trigger(&mut s,source,1);resolve(&mut s);let mut commander=s.clone();let mut ordinary=s.clone();choose(&mut commander,cards[0]);choose(&mut ordinary,cards[1]);
    assert!(commander.pending_movement.is_some());assert!(ordinary.pending_movement.is_some());
    assert_ne!(legal_actions(&commander).len(),legal_actions(&ordinary).len());
    for viewer in [0,2] {assert_eq!(info(&s,viewer).zone_normalization,info(&commander,viewer).zone_normalization);assert_eq!(info(&s,viewer).zone_normalization,info(&ordinary,viewer).zone_normalization);assert_eq!(info(&commander,viewer).hash_value(),info(&ordinary,viewer).hash_value());}
    for state in [&commander,&ordinary] {for action in legal_actions(state) {let key=canonicalize(&action,state).unwrap();assert_eq!(reconstruct(&key,state,1).unwrap(),Some(action));}}
    assert_ne!(info(&commander,1).hash_value(),info(&ordinary,1).hash_value());
}
#[test]
fn a_closed_unplaceable_occurrence_is_removed_without_allocating_a_fake_stack_slot() {
    let mut s=GameState::new_commander(1);s.card_db=Some(Arc::new(sample::build_sample_db()));let source=s.create_card_in_zone(sample::ids::CHITTERING_RATS,0,ZoneType::Battlefield).unwrap();
    let next=s.next_stack_id;let _=rules::fire_triggers(&mut s,TriggerCondition::EntersBattlefield,Some(source));assert!(s.pending_triggers.is_empty());assert!(s.trigger_placement.is_none());assert!(s.stack.is_empty());assert_eq!(s.next_stack_id,next);
}
#[test]
fn own_selection_replay_from_an_earlier_cancelled_operation_is_rejected_in_the_next_window() {
    let (mut s,source,cards)=fixture(2);redirect(&mut s,Destination::Library,ZoneType::Hand);force_targeted_trigger(&mut s,source,1);resolve(&mut s);
    let old=choose(&mut s,cards[0]);assert!(s.players[1].hand.contains(&cards[0]));assert_eq!(s.objects[&cards[0]].zone_change_count,0);
    force_targeted_trigger(&mut s,source,1);resolve(&mut s);assert!(s.pending_chittering.is_some());noop(&mut s,&old);
}
#[cfg(feature="tui")]
#[test]
fn foreign_private_parent_and_child_ui_hide_action_count_identity_and_stage() {
    let (mut s,source,cards)=fixture(3);s.designate_commander(cards[0],1).unwrap();force_targeted_trigger(&mut s,source,1);resolve(&mut s);
    let app=mtg_gto::tui::App::new(s.clone(),sample::build_sample_db());assert!(app.cached_actions.is_empty());
    let selected=legal_actions(&s).into_iter().find(|a|matches!(a,Action::ChooseChitteringCard {..})).unwrap();assert_eq!(mtg_gto::tui::format_action_short(&s,&selected,s.card_db()),"Waiting for opponent's private choice");
    choose(&mut s,cards[0]);let app=mtg_gto::tui::App::new(s.clone(),sample::build_sample_db());assert!(app.cached_actions.is_empty());
    for a in legal_actions(&s) {assert_eq!(mtg_gto::tui::format_action_short(&s,&a,s.card_db()),"Waiting for opponent's private choice");}
}
#[test]
fn mixed_ordinary_and_targeted_occurrences_keep_apnap_and_independent_captured_sources() {
    use mtg_gto::card::{CardDef,CardType,TriggeredAbility};
    for players in [3,4] {
        let (mut s,source,_) = fixture(players);
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {id:991771,name:"Ordinary ETB".into(),card_types:vec![CardType::Creature],power:Some(1),toughness:Some(1),
            triggered_abilities:vec![TriggeredAbility {trigger:TriggerCondition::EntersBattlefield,effect:Effect::GainLife {amount:1},description:"ordinary life".into()}],..Default::default()});
        for controller in (0..players).rev() {
            let ordinary=s.create_card_in_zone(991771,controller,ZoneType::Battlefield).unwrap();
            s.pending_triggers.push(mtg_gto::game::PendingTrigger {placement:None,source_id:ordinary,ability_index:0,controller,targets:vec![],context:TriggerContext {source_card_id:991771,source_generation:0,effect:Effect::GainLife {amount:1},cast_spell:None,zone_transition:None}});
            s.pending_triggers.push(mtg_gto::game::PendingTrigger {placement:None,source_id:source,ability_index:0,controller,targets:vec![],context:TriggerContext {source_card_id:sample::ids::CHITTERING_RATS,source_generation:0,effect:Effect::ChitteringRats,cast_spell:None,zone_transition:None}});
        }
        let _=rules::fire_triggers(&mut s,TriggerCondition::EntersBattlefield,Some(source));
        for controller in 0..players {while s.trigger_placement.as_ref().is_some_and(|p|p.controller==controller) {
            let legal=legal_actions(&s);let action=legal.iter().find(|a|matches!(a,Action::ChooseNextTrigger {..})).cloned().unwrap_or_else(||legal.into_iter().find(|a|matches!(a,Action::PlaceChitteringTrigger {opponent,..} if *opponent==(controller+1)%players)).unwrap());
            rules::apply_action(&mut s,&action);assert!(s.validate_trigger_placement().is_ok());for restored in branches(&s) {assert!(restored.validate_trigger_placement().is_ok());}
        }}
        let expected:[Vec<usize>;2]=[vec![0,0,0],(1..players).flat_map(|p|[p,p]).collect()];assert_eq!(s.stack.iter().map(|e|e.controller).collect::<Vec<_>>(),expected.concat());
    }
}
#[test]
fn unsupported_nested_printed_spell_rejects_before_prefix_and_without_losing_stack_source() {
    use mtg_gto::card::{CardDef,CardType};let (mut s,_,_) = fixture(2);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {id:991772,name:"Excluded wrapper".into(),card_types:vec![CardType::Instant],spell_effect:Some(Effect::Multiple(vec![Effect::GainLife {amount:7},Effect::ChitteringRats])),..Default::default()});
    let source=s.create_card_in_zone(991772,0,ZoneType::Stack).unwrap();let id=s.new_stack_id();s.stack.push(StackEntry {id,source:StackSource::Spell(source),controller:0,targets:vec![],target_generations:vec![]});
    let stack=bincode::serialize(&s.stack).unwrap();let life=s.players[0].life;resolve(&mut s);assert!(s.gameplay_stopped());assert_eq!(s.players[0].life,life);assert_eq!(bincode::serialize(&s.stack).unwrap(),stack);assert_eq!(s.objects[&source].zone_change_count,0);
}
#[test]
fn private_selection_and_replacement_can_concede_only_through_existing_terminal_loss() {
    for child in [false,true] {let (mut s,source,cards)=fixture(2);s.designate_commander(cards[0],1).unwrap();force_targeted_trigger(&mut s,source,1);resolve(&mut s);if child {choose(&mut s,cards[0]);}
        rules::apply_action(&mut s,&Action::Concede);assert!(s.game_over);assert_eq!(s.winner,Some(0));assert!(s.players[1].hand.contains(&cards[0]));assert_eq!(s.objects[&cards[0]].zone_change_count,0);
        for restored in branches(&s) {assert!(restored.game_over);assert_eq!(restored.winner,Some(0));}
    }
}
