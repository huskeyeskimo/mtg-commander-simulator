use std::sync::Arc;
use mtg_gto::{action::{legal_actions, Action}, card::{sample, CardDef, CardType, ZoneType},
    game::{GameState, Phase, Target}, rules};

const CREATURE: u64 = 999_601;
fn fixture(with_target: bool) -> (GameState, u64, Option<u64>) {
    let mut db = sample::build_sample_db();
    db.insert(CardDef { id: CREATURE, name: "Graveyard creature".into(), card_types: vec![CardType::Creature],
        power: Some(2), toughness: Some(2), ..Default::default() });
    let mut state = GameState::new_commander(2); state.card_db = Some(Arc::new(db)); state.phase = Phase::PreCombatMain;
    let commander = state.create_card_in_zone(sample::ids::ASHCOAT_OF_THE_SHADOW_SWARM, 0, ZoneType::Command).unwrap();
    state.designate_commander(commander, 0).unwrap();
    let source = state.create_card_in_zone(sample::ids::PHYREXIAN_RECLAMATION, 0, ZoneType::Battlefield).unwrap();
    let target = with_target.then(|| state.create_card_in_zone(CREATURE, 0, ZoneType::Graveyard).unwrap());
    if let Some(target) = target {state.objects.get_mut(&target).unwrap().controller = 1;}
    state.players[0].mana_pool.black = 1; state.players[0].mana_pool.colorless = 1; state.drain_events();
    (state, source, target)
}
fn activations(state: &GameState, source: u64) -> Vec<Action> {
    legal_actions(state).into_iter().filter(|action| {
        if matches!(action, Action::ActivateAbility { object_id, .. } if *object_id == source) {return true;}
        serde_json::to_value(action).unwrap().get("ActivateReclamation")
            .and_then(|a|a.get("source")).and_then(|s|s.get("id")).and_then(|id|id.as_u64()) == Some(source)
    }).collect()
}
fn selected_target(action: &Action) -> Option<u64> {
    if let Action::ActivateAbility { targets, .. } = action {
        return match targets.as_slice() {[Target::Object(id)]=>Some(*id),_=>None};
    }
    serde_json::to_value(action).unwrap().get("ActivateReclamation")
        .and_then(|a|a.get("target")).and_then(|s|s.get("id")).and_then(|id|id.as_u64())
}
fn resolve(state: &mut GameState) {for _ in 0..state.players.len() {rules::apply_action(state, &Action::PassPriority);}}

#[test]
fn reclamation_selects_own_graveyard_creature_pays_printed_cost_and_returns_it() {
    let (mut state, source, target) = fixture(true); let target = target.unwrap();
    let actions = activations(&state, source); assert_eq!(actions.len(), 1);
    assert_eq!(selected_target(&actions[0]), Some(target), "printed ability must select its own graveyard creature");
    rules::apply_action(&mut state, &actions[0]);
    assert_eq!(state.players[0].life, 38); assert_eq!(state.players[0].mana_pool.total(), 0);
    assert_eq!(state.stack.len(), 1); assert!(state.players[0].graveyard.contains(&target));
    resolve(&mut state); assert!(!state.gameplay_stopped());
    assert!(state.players[0].hand.contains(&target)); assert!(!state.players[0].graveyard.contains(&target));
    assert_eq!(state.objects[&target].zone_change_count, 1); assert!(state.stack.is_empty());
}

#[test]
fn reclamation_is_not_activatable_with_an_empty_graveyard() {
    let (state, source, _) = fixture(false);
    assert!(activations(&state, source).is_empty(), "required target must exist before activation");
}

#[test]
fn targetless_or_excess_legacy_activation_cannot_pay_any_cost() {
    let (base, source, target) = fixture(true);
    for targets in [vec![], vec![Target::Object(target.unwrap()), Target::Object(target.unwrap())]] {
        let mut state = base.clone(); let before = bincode::serialize(&state).unwrap();
        rules::apply_action(&mut state, &Action::ActivateAbility { object_id: source, ability_index: 0, targets });
        assert_eq!(bincode::serialize(&state).unwrap(), before, "illegal target shape must reject before mana/life or stack mutation");
        assert!(state.pending_events.is_empty());
    }
}

fn activation(state: &GameState, source: u64, target: u64) -> Action {
    Action::ActivateReclamation { source: state.exact_object(source).unwrap(), target: state.exact_object(target).unwrap() }
}
fn unchanged(mut state: GameState, action: Action) {
    state.drain_events(); let before = bincode::serialize(&state).unwrap();
    rules::apply_action(&mut state, &action);
    assert_eq!(bincode::serialize(&state).unwrap(), before); assert!(state.pending_events.is_empty());
}
fn layer(state: &mut GameState, id: u64, modification: mtg_gto::layers::LayerModification) {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration};
    state.continuous_effects.push(ContinuousEffect { source_id: id, controller: 0, timestamp: 100,
        duration: Duration::Permanent, affected: AffectedObjects::Specific(id), modification });
    state.invalidate_characteristics_cache();
}
fn roundtrips(state: &GameState) -> Vec<GameState> {
    let mut snapshot = state.clone(); snapshot.restore(state.snapshot()).unwrap();
    let mut json: GameState = serde_json::from_str(&serde_json::to_string(state).unwrap()).unwrap(); json.card_db = state.card_db.clone();
    let mut binary: GameState = bincode::deserialize(&bincode::serialize(state).unwrap()).unwrap(); binary.card_db = state.card_db.clone();
    vec![state.clone(), snapshot, json, binary]
}
#[test]
fn all_duplicate_looking_own_graveyard_cards_have_distinct_canonical_actions() {
    use mtg_gto::action::canonical::{canonicalize, resolve as reconstruct};
    let (mut s, source, first) = fixture(true);
    let second = s.create_card_in_zone(CREATURE, 0, ZoneType::Graveyard).unwrap();
    let other = s.create_card_in_zone(CREATURE, 1, ZoneType::Graveyard).unwrap();
    s.objects.get_mut(&other).unwrap().controller = 0;
    let actions = activations(&s, source); assert_eq!(actions.len(), 2);
    let mut keys = std::collections::HashSet::new();
    for a in actions { let key = canonicalize(&a, &s).unwrap(); assert_eq!(reconstruct(&key, &s, 0).unwrap(), Some(a)); keys.insert(key); }
    assert_eq!(keys.len(), 2);
    for target in [first.unwrap(), second] { assert!(activations(&s, source).iter().any(|a| selected_target(a) == Some(target))); }
    unchanged(s.clone(), activation(&s, source, other));
}
#[test]
fn invalid_zone_type_owner_token_and_exact_references_reject_atomically() {
    let (s, source, target) = fixture(true); let target = target.unwrap();
    for fault in 0..8 {
        let mut bad = s.clone(); let mut a = activation(&bad, source, target);
        match fault {
            0 => {bad.move_object(target, ZoneType::Graveyard, ZoneType::Hand); a = activation(&bad, source, target);},
            1 => {bad.objects.get_mut(&target).unwrap().card_def_id = sample::ids::SWAMP;},
            2 => {bad.players[0].graveyard.retain(|&id| id != target); bad.players[1].graveyard.push(target);},
            3 => bad.objects.get_mut(&target).unwrap().is_token = true,
            4 => if let Action::ActivateReclamation { target, .. } = &mut a {target.generation += 1;},
            5 => if let Action::ActivateReclamation { source, .. } = &mut a {source.generation += 1;},
            6 => bad.battlefield.push(source),
            _ => {bad.objects.get_mut(&target).unwrap().owner = 1;},
        }
        assert!(!legal_actions(&bad).contains(&a)); unchanged(bad, a);
    }
}
#[test]
fn insufficient_mana_life_capacity_and_wrong_timing_are_prepayment_noops() {
    let (s, source, target) = fixture(true); let a = activation(&s, source, target.unwrap());
    for fault in 0..11 {
        let mut bad = s.clone();
        match fault {
            0 => bad.players[0].mana_pool.colorless = 0,
            1 => {bad.players[0].mana_pool.black = 0; bad.players[0].mana_pool.colorless = 2;},
            2 => bad.players[0].life = 1,
            3 => bad.next_stack_id = u64::MAX,
            4 => bad.loss_boundary.accepted_actions = u64::MAX,
            5 => bad.priority_player = 1,
            6 => bad.phase = Phase::Untap,
            7 => bad.phase = Phase::Mulligan,
            8 => bad.phase = Phase::Cleanup,
            9 => bad.phase = Phase::DeclareAttackers,
            _ => {let mut entry = mtg_gto::game::StackEntry { id: bad.next_stack_id,
                source: mtg_gto::game::StackSource::ActivatedAbility {source_id: source, ability_index: 0}, controller: 0,
                targets: vec![], target_generations: vec![] }; entry.id = bad.next_stack_id; bad.stack.push(entry);},
        }
        assert!(!activations(&bad, source).contains(&a)); unchanged(bad, a.clone());
    }
}
#[test]
fn exactly_two_life_is_payable_then_existing_sba_loses_the_player() {
    let (mut s, source, target) = fixture(true); s.players[0].life = 2;
    let a = activation(&s, source, target.unwrap()); assert!(activations(&s, source).contains(&a));
    rules::apply_action(&mut s, &a);
    assert_eq!(s.players[0].life, 0); assert!(s.players[0].has_lost);
    assert_eq!(s.players[0].mana_pool.total(), 0);
}
#[test]
fn effective_source_control_and_ability_removal_govern_both_enumeration_and_apply() {
    use mtg_gto::layers::LayerModification;
    for acquired in [false, true] {
        let (mut s, source, target) = fixture(true);
        s.objects.get_mut(&source).unwrap().controller = if acquired {1} else {0};
        layer(&mut s, source, LayerModification::ChangeController(if acquired {0} else {1}));
        let a = activation(&s, source, target.unwrap());
        assert_eq!(activations(&s, source).contains(&a), acquired);
        if acquired {rules::apply_action(&mut s, &a); assert_eq!(s.stack.len(), 1);}
        else {unchanged(s, a);}
    }
    let (mut s, source, target) = fixture(true); layer(&mut s, source, LayerModification::RemoveAllAbilities);
    let a = activation(&s, source, target.unwrap()); assert!(activations(&s, source).is_empty()); unchanged(s, a);
}
#[test]
fn source_departure_control_change_and_new_incarnation_do_not_rebind_the_paid_ability() {
    use mtg_gto::layers::LayerModification;
    for fault in 0..5 {
        let (mut s, source, target) = fixture(true); let target = target.unwrap();
        let a = activation(&s, source, target); rules::apply_action(&mut s, &a);
        if fault == 3 {layer(&mut s, source, LayerModification::RemoveAllAbilities);}
        else if fault == 4 {layer(&mut s, source, LayerModification::CopyOf(sample::ids::SWAMP));}
        else if fault == 0 {layer(&mut s, source, LayerModification::ChangeController(1));}
        else {s.move_object(source, ZoneType::Battlefield, ZoneType::Hand);
            if fault == 2 {s.move_object(source, ZoneType::Hand, ZoneType::Battlefield);}}
        for mut restored in roundtrips(&s) {
            assert_eq!(restored.stack[0].controller, 0); resolve(&mut restored);
            assert!(!restored.gameplay_stopped()); assert!(restored.players[0].hand.contains(&target));
            assert_eq!(restored.players[0].life, 38); assert_eq!(restored.players[0].mana_pool.total(), 0);
        }
    }
}
#[test]
fn target_departure_and_return_fizzle_without_refunding_and_two_activations_move_only_once() {
    for returns in [false, true] {
        let (mut s, source, target) = fixture(true); let target = target.unwrap();
        let a = activation(&s, source, target); rules::apply_action(&mut s, &a);
        s.move_object(target, ZoneType::Graveyard, ZoneType::Exile);
        if returns {s.move_object(target, ZoneType::Exile, ZoneType::Graveyard);}
        for mut restored in roundtrips(&s) {resolve(&mut restored); assert!(!restored.gameplay_stopped());
            assert!(restored.stack.is_empty()); assert!(!restored.players[0].hand.contains(&target)); assert_eq!(restored.players[0].life, 38);}
    }
    let (mut s, source, target) = fixture(true); let target = target.unwrap();
    s.players[0].mana_pool.black = 2; s.players[0].mana_pool.colorless = 2;
    let a = activation(&s, source, target); rules::apply_action(&mut s, &a); rules::apply_action(&mut s, &a);
    assert_eq!(s.stack.len(), 2); assert_eq!(s.players[0].life, 36); assert_eq!(s.players[0].mana_pool.total(), 0);
    resolve(&mut s); resolve(&mut s); assert!(s.stack.is_empty()); assert_eq!(s.objects[&target].zone_change_count, 1);
}
#[test]
fn commander_owner_decision_roundtrips_accept_and_decline() {
    use mtg_gto::action::canonical::{canonicalize, resolve as reconstruct};
    for accept in [false, true] {
        let (mut s, source, _) = fixture(false);
        let target = s.players[0].command_zone[0]; s.move_object(target, ZoneType::Command, ZoneType::Graveyard);
        rules::check_state_based_actions(&mut s);
        let decline = legal_actions(&s).into_iter().find(|a| matches!(a, Action::ChooseCommanderSba {to_command: false, ..})).unwrap();
        rules::apply_action(&mut s, &decline);
        let a = activation(&s, source, target); rules::apply_action(&mut s, &a); resolve(&mut s);
        assert!(s.pending_movement.is_some()); assert!(s.players[0].graveyard.contains(&target));
        for mut restored in roundtrips(&s) {
            let answer = legal_actions(&restored).into_iter().find(|a| matches!(a, Action::ChooseCommanderMovement {to_command, ..} if *to_command == accept)).unwrap();
            assert_eq!(reconstruct(&canonicalize(&answer, &restored).unwrap(), &restored, 0).unwrap(), Some(answer.clone()));
            rules::apply_action(&mut restored, &answer); assert!(!restored.gameplay_stopped()); assert!(restored.pending_movement.is_none());
            assert!(if accept {restored.players[0].command_zone.contains(&target)} else {restored.players[0].hand.contains(&target)});
            assert_eq!(restored.players[0].life, 38);
        }
    }
}
#[test]
fn illegal_target_fizzles_before_a_new_unsupported_movement_catalog_is_considered() {
    use mtg_gto::replacement::{ReplacementEffect, ReplacementEventKind, ReplacementAction, MovementDestination};
    let (mut s, source, target) = fixture(true); let target = target.unwrap();
    let a = activation(&s, source, target); rules::apply_action(&mut s, &a);
    s.move_object(target, ZoneType::Graveyard, ZoneType::Exile);
    s.replacement_effects.push(ReplacementEffect {source_id: source, controller: 0,
        applies_to: ReplacementEventKind::WouldMoveTo {destination: MovementDestination::Hand, source_generation: s.objects[&source].zone_change_count},
        action: ReplacementAction::RedirectToZone(ZoneType::Battlefield), is_self_replacement: false, description: "unsupported destination".into()});
    resolve(&mut s); assert!(!s.gameplay_stopped()); assert!(s.stack.is_empty()); assert_eq!(s.players[0].life, 38);
}

fn mana_source(s: &mut GameState, owner: usize, color: mtg_gto::mana::Color) -> u64 {
    let id = 999_610 + color as u64;
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {id, name: format!("Mana {color}"), card_types: vec![CardType::Artifact],
        mana_abilities: vec![mtg_gto::card::ManaAbility::TapForColor(color)], ..Default::default()});
    s.create_card_in_zone(id, owner, ZoneType::Battlefield).unwrap()
}
#[test]
fn payment_uses_effective_sources_and_alternatives_without_spending_invalid_sources() {
    use mtg_gto::{layers::LayerModification, mana::{Color, ManaPool}};
    for fault in 0..4 {
        let (mut s, source, target) = fixture(true); let target = target.unwrap();
        s.players[0].mana_pool = ManaPool::empty(); s.players[0].mana_pool.colorless = 1;
        let invalid = mana_source(&mut s, if fault == 0 {1} else {0}, Color::Black);
        match fault {
            0 => layer(&mut s, invalid, LayerModification::ChangeController(0)),
            1 => layer(&mut s, invalid, LayerModification::ChangeController(1)),
            2 => layer(&mut s, invalid, LayerModification::RemoveAllAbilities),
            _ => layer(&mut s, invalid, LayerModification::CopyOf(sample::ids::SWAMP)),
        }
        let a = activation(&s, source, target);
        if fault == 0 {assert!(activations(&s, source).contains(&a)); rules::apply_action(&mut s, &a); assert!(s.objects[&invalid].tapped);}
        else {
            assert!(!activations(&s, source).contains(&a)); unchanged(s.clone(), a.clone());
            let valid = mana_source(&mut s, 0, Color::Black);
            assert!(activations(&s, source).contains(&a)); rules::apply_action(&mut s, &a);
            assert!(s.objects[&valid].tapped); assert!(!s.objects[&invalid].tapped);
        }
        assert_eq!(s.players[0].mana_pool.total(), 0); assert_eq!(s.players[0].life, 38);
    }
}
#[test]
fn nonland_mana_bonus_keeps_produced_color_and_effective_type_controls_bonus() {
    use mtg_gto::{layers::LayerModification, mana::Color};
    for land in [false, true] {
        let (mut s, source, target) = fixture(true); s.players[0].mana_pool.black = 0;
        let producer = mana_source(&mut s, 0, Color::Black);
        s.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield).unwrap();
        if land {layer(&mut s, producer, LayerModification::AddType(CardType::Land));}
        let a = activation(&s, source, target.unwrap()); rules::apply_action(&mut s, &a);
        assert_eq!(s.players[0].mana_pool.black, if land {0} else {1});
        assert_eq!(s.players[0].mana_pool.colorless, 0); assert!(s.objects[&producer].tapped);
    }
}
#[test]
fn legendary_colors_survive_ability_removal_and_follow_effective_control() {
    use mtg_gto::{layers::LayerModification, mana::ManaPool};
    for acquired in [true, false] {
        let (mut s, source, target) = fixture(true); s.players[0].mana_pool = ManaPool::empty(); s.players[0].mana_pool.colorless = 1;
        let legendary = s.create_card_in_zone(sample::ids::ASHCOAT_OF_THE_SHADOW_SWARM, if acquired {1} else {0}, ZoneType::Battlefield).unwrap();
        layer(&mut s, legendary, LayerModification::ChangeController(if acquired {0} else {1}));
        layer(&mut s, legendary, LayerModification::RemoveAllAbilities);
        let amber = s.create_card_in_zone(sample::ids::MOX_AMBER, 0, ZoneType::Battlefield).unwrap();
        let a = activation(&s, source, target.unwrap()); assert_eq!(activations(&s, source).contains(&a), acquired);
        if acquired {rules::apply_action(&mut s, &a); assert!(s.objects[&amber].tapped); assert_eq!(s.players[0].mana_pool.total(), 0);}
        else {unchanged(s, a);}
    }
}
#[test]
fn exact_source_action_cannot_replay_after_leave_and_return() {
    let (mut s, source, target) = fixture(true); let a = activation(&s, source, target.unwrap());
    s.move_object(source, ZoneType::Battlefield, ZoneType::Hand); s.move_object(source, ZoneType::Hand, ZoneType::Battlefield);
    unchanged(s.clone(), a.clone()); assert!(!activations(&s, source).contains(&a));
    assert_eq!(activations(&s, source).len(), 1);
}
#[test]
fn malformed_owned_stack_frames_reject_restore_and_valid_stale_targets_restore() {
    let (mut s, source, target) = fixture(true); let a = activation(&s, source, target.unwrap()); rules::apply_action(&mut s, &a);
    let value = serde_json::to_value(&s).unwrap();
    for fault in 0..9 {
        let mut bad = value.clone();
        match fault {
            0 => bad["stack"][0]["targets"] = serde_json::json!([]),
            1 => bad["stack"][0]["target_generations"] = serde_json::json!([null]),
            2 => bad["stack"][0]["source"]["ReclamationAbility"]["source_owner"] = serde_json::json!(1),
            3 => bad["stack"][0]["source"]["ReclamationAbility"]["target_card_id"] = serde_json::json!(sample::ids::SWAMP),
            4 => bad["stack"][0]["source"]["ReclamationAbility"]["source"]["generation"] = serde_json::json!(99),
            5 => bad["stack"][0]["target_generations"] = serde_json::json!([99]),
            6 => bad["stack"][0]["controller"] = serde_json::json!(1),
            7 => bad["stack"][0]["id"] = serde_json::json!(s.next_stack_id),
            _ => {let entry = bad["stack"][0].clone(); bad["stack"].as_array_mut().unwrap().push(entry);},
        }
        assert!(serde_json::from_value::<GameState>(bad).is_err(), "malformed frame {fault}");
    }
    let mut wrong = s.clone(); wrong.stack[0].targets.clear(); assert!(wrong.restore(wrong.snapshot()).is_err());
    let mut detached: GameState = serde_json::from_value(value).unwrap(); let before = bincode::serialize(&detached).unwrap();
    rules::apply_action(&mut detached, &Action::PassPriority); assert_eq!(bincode::serialize(&detached).unwrap(), before);
    assert!(mtg_gto::action::canonical::canonicalize(&Action::PassPriority, &detached).is_err());
    use mtg_gto::strategy::Strategy;
    assert!(mtg_gto::strategy::GreedyStrategy.choose_action(&detached, 0).is_err());
    detached.card_db = s.card_db.clone(); resolve(&mut detached); assert!(detached.players[0].hand.contains(&target.unwrap()));
}
#[test]
fn canonical_actions_and_information_sets_are_allocation_invariant_and_reconstruct_after_restore() {
    use mtg_gto::{action::canonical::{canonicalize, resolve as reconstruct}, info_set::InformationSet};
    let build = |offset: u64| {let (mut s, source, target) = fixture(true); let target = target.unwrap();
        // All runtime object IDs shift together; no new public objects or facts.
        let mut value = serde_json::to_value(&s).unwrap();
        let ids: Vec<_> = s.objects.keys().copied().collect();
        let mut objects = serde_json::Map::new();
        for id in &ids {let mut object = value["objects"][id.to_string()].clone(); object["object_id"] = serde_json::json!(id + offset); objects.insert((id + offset).to_string(), object);}
        value["objects"] = serde_json::Value::Object(objects);
        for zone in ["battlefield"] {for id in value[zone].as_array_mut().unwrap() {*id = serde_json::json!(id.as_u64().unwrap() + offset);}}
        for p in value["players"].as_array_mut().unwrap() {for zone in ["hand", "library", "graveyard", "exile", "command_zone"] {for id in p[zone].as_array_mut().unwrap() {*id = serde_json::json!(id.as_u64().unwrap() + offset);}}}
        // Commander registry stores exact physical ID separately from designation.
        if offset != 0 {for c in value["commanders"].as_array_mut().unwrap() {c["object"] = serde_json::json!(c["object"].as_u64().unwrap() + offset);}}
        value["next_object_id"] = serde_json::json!(s.next_object_id + offset);
        let mut shifted: GameState = serde_json::from_value(value).unwrap(); shifted.card_db = s.card_db.take(); (shifted, source + offset, target + offset)
    };
    let (base, source, target) = build(0); let (shifted, other_source, other_target) = build(500);
    assert_eq!(InformationSet::from_view(&base.visible_state(0), base.card_db()).unwrap().hash_value(), InformationSet::from_view(&shifted.visible_state(0), shifted.card_db()).unwrap().hash_value());
    let key = canonicalize(&activation(&base, source, target), &base).unwrap();
    assert_eq!(key, canonicalize(&activation(&shifted, other_source, other_target), &shifted).unwrap());
    for restored in roundtrips(&shifted) {assert_eq!(reconstruct(&key, &restored, 0).unwrap(), Some(activation(&restored, other_source, other_target)));}
}
#[test]
fn malformed_source_owner_embedded_endpoint_identity_and_duplicate_mana_source_are_atomic() {
    let (s, source, target) = fixture(true); let target = target.unwrap();
    for fault in 0..5 {
        let mut bad = s.clone(); let a = activation(&bad, source, target);
        match fault {
            0 => bad.objects.get_mut(&source).unwrap().owner = usize::MAX,
            1 => bad.objects.get_mut(&source).unwrap().object_id = target,
            2 => bad.objects.get_mut(&target).unwrap().object_id = source,
            3 => {bad.players[0].mana_pool = mtg_gto::mana::ManaPool::empty();
                let swamp = bad.create_card_in_zone(sample::ids::SWAMP, 0, ZoneType::Battlefield).unwrap(); bad.battlefield.push(swamp);},
            _ => {bad.objects.get_mut(&source).unwrap().card_def_id = sample::ids::SWAMP;},
        }
        assert!(!activations(&bad, source).contains(&a)); unchanged(bad, a);
    }
}
#[test]
fn paid_stack_historical_endpoints_do_not_expose_new_hidden_incarnation_facts() {
    use mtg_gto::info_set::InformationSet;
    let (mut s, source, target) = fixture(true); let target = target.unwrap();
    let a = activation(&s, source, target); rules::apply_action(&mut s, &a);
    s.move_object(source, ZoneType::Battlefield, ZoneType::Hand);
    s.move_object(target, ZoneType::Graveyard, ZoneType::Hand);
    let mut other = s.clone(); other.objects.get_mut(&source).unwrap().damage_marked = 17;
    other.objects.get_mut(&target).unwrap().damage_marked = 25;
    for state in [&s, &other] {let view = state.visible_state(1); assert!(!view.objects.contains_key(&source)); assert!(!view.objects.contains_key(&target));}
    let info = |state: &GameState| InformationSet::from_view(&state.visible_state(1), state.card_db()).unwrap();
    assert_eq!(info(&s).zone_normalization, info(&other).zone_normalization); assert_eq!(info(&s).hash_value(), info(&other).hash_value());
    for mut restored in roundtrips(&s) {resolve(&mut restored); assert!(restored.stack.is_empty()); assert!(!restored.gameplay_stopped());}
}
#[test]
fn unsupported_catalog_for_a_live_target_stops_before_movement_and_preserves_paid_costs() {
    use mtg_gto::replacement::{ReplacementEffect, ReplacementEventKind, ReplacementAction, MovementDestination};
    let (mut s, source, target) = fixture(true); let target = target.unwrap();
    let a = activation(&s, source, target); rules::apply_action(&mut s, &a);
    s.replacement_effects.push(ReplacementEffect {source_id: source, controller: 0,
        applies_to: ReplacementEventKind::WouldMoveTo {destination: MovementDestination::Hand, source_generation: s.objects[&source].zone_change_count},
        action: ReplacementAction::RedirectToZone(ZoneType::Battlefield), is_self_replacement: false, description: "unsupported destination".into()});
    s.drain_events(); resolve(&mut s); assert!(s.gameplay_stopped()); assert_eq!(s.stack.len(), 1);
    assert!(s.players[0].graveyard.contains(&target)); assert_eq!(s.objects[&target].zone_change_count, 0);
    assert_eq!(s.players[0].life, 38); assert_eq!(s.players[0].mana_pool.total(), 0); assert!(s.pending_events.is_empty());
}
#[test]
fn instant_speed_reclamation_responds_to_a_stack_entry_and_greedy_selects_its_public_target() {
    use mtg_gto::strategy::{GreedyStrategy, Strategy};
    let (mut s, source, target) = fixture(true); let target = target.unwrap(); s.phase = Phase::Upkeep;
    let id = s.new_stack_id(); s.stack.push(mtg_gto::game::StackEntry { id,
        source: mtg_gto::game::StackSource::ActivatedAbility {source_id: s.players[0].command_zone[0], ability_index: 99}, controller: 1, targets: vec![], target_generations: vec![] });
    let a = activation(&s, source, target); assert!(activations(&s, source).contains(&a));
    assert!(matches!(GreedyStrategy.choose_action(&s, 0).unwrap(), Action::ActivateReclamation { .. }));
    rules::apply_action(&mut s, &a); assert_eq!(s.stack.len(), 2); resolve(&mut s);
    assert_eq!(s.stack.len(), 1); assert!(s.players[0].hand.contains(&target));
}
#[test]
fn changed_printed_contract_and_copied_reclamation_source_remain_unsupported_without_payment() {
    use mtg_gto::layers::LayerModification;
    for fault in 0..3 {
        let (mut s, source, target) = fixture(true); let a = activation(&s, source, target.unwrap());
        match fault {
            0 => layer(&mut s, source, LayerModification::CopyOf(sample::ids::SWAMP)),
            1 => {let mut def = s.card_db().get(sample::ids::PHYREXIAN_RECLAMATION).unwrap().clone(); def.activated_abilities.clear(); Arc::make_mut(s.card_db.as_mut().unwrap()).insert(def);},
            _ => {let mut def = s.card_db().get(sample::ids::PHYREXIAN_RECLAMATION).unwrap().clone(); def.activated_abilities[0].life_cost = 0; Arc::make_mut(s.card_db.as_mut().unwrap()).insert(def);},
        }
        assert!(!activations(&s, source).contains(&a)); unchanged(s, a);
    }
}
#[test]
fn new_target_selector_does_not_enable_unrelated_graveyard_spells_or_abilities() {
    use mtg_gto::card::{Effect, TargetSpec, ActivatedAbility};
    let (mut s, _, target) = fixture(true); let target = target.unwrap();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {id: 999_620, name: "Other graveyard spell".into(),
        card_types: vec![CardType::Sorcery], spell_effect: Some(Effect::ReturnFromGraveyardToHand {target: TargetSpec::CreatureCardInOwnGraveyard}), ..Default::default()});
    let spell = s.create_card_in_zone(999_620, 0, ZoneType::Hand).unwrap();
    assert!(!legal_actions(&s).iter().any(|a| matches!(a, Action::CastSpell {object_id, ..} if *object_id == spell)));
    // An exact Reclamation action cannot activate a similarly shaped other permanent.
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {id: 999_621, name: "Other graveyard ability".into(), card_types: vec![CardType::Enchantment],
        activated_abilities: vec![ActivatedAbility {cost: mtg_gto::mana::ManaCost::new(1,0,0,1,0,0), requires_tap: false, sacrifice_cost: None,
            life_cost: 2, effect: Effect::ReturnFromGraveyardToHand {target: TargetSpec::CreatureCardInOwnGraveyard}, description: "Other".into()}], ..Default::default()});
    let other = s.create_card_in_zone(999_621, 0, ZoneType::Battlefield).unwrap();
    assert!(!legal_actions(&s).iter().any(|a| matches!(a, Action::ActivateAbility {object_id, ..} if *object_id == other)));
    unchanged(s.clone(), Action::ActivateAbility {object_id: other, ability_index: 0, targets: vec![Target::Object(target)]});
    unchanged(s.clone(), activation(&s, other, target));
}
#[test]
fn exhausted_current_movement_window_generation_and_event_group_reject_before_costs() {
    let (s, source, target) = fixture(true); let target = target.unwrap();
    for fault in 0..3 {
        let mut bad = s.clone();
        match fault {0 => bad.next_movement_window = u64::MAX,
            1 => bad.objects.get_mut(&target).unwrap().zone_change_count = u32::MAX,
            _ => bad.next_zone_event_group_id = u64::MAX,}
        let a = activation(&bad, source, target); assert!(!activations(&bad, source).contains(&a)); unchanged(bad, a);
    }
}
#[test]
fn unrelated_hidden_hand_identities_and_library_order_do_not_change_public_reclamation_keys() {
    use mtg_gto::{action::canonical::canonicalize, info_set::InformationSet};
    let (mut s, source, target) = fixture(true); let target = target.unwrap();
    s.create_card_in_zone(sample::ids::SWAMP, 1, ZoneType::Hand).unwrap();
    s.create_card_in_zone(sample::ids::SOL_RING, 1, ZoneType::Library).unwrap();
    s.create_card_in_zone(sample::ids::SWAMP, 1, ZoneType::Library).unwrap();
    let mut other = s.clone(); other.players[1].library.reverse();
    let hand = other.players[1].hand[0]; other.objects.get_mut(&hand).unwrap().card_def_id = sample::ids::SOL_RING;
    let info = |state: &GameState| InformationSet::from_view(&state.visible_state(0), state.card_db()).unwrap();
    assert_eq!(info(&s).hash_value(), info(&other).hash_value()); assert_eq!(info(&s).zone_normalization, info(&other).zone_normalization);
    assert_eq!(canonicalize(&activation(&s, source, target), &s).unwrap(), canonicalize(&activation(&other, source, target), &other).unwrap());
    assert_eq!(activations(&s, source), activations(&other, source));
}
#[test]
fn three_player_acquired_source_uses_actor_graveyard_and_commander_owner_window() {
    let mut s = GameState::new_commander(3); s.card_db = Some(Arc::new(sample::build_sample_db())); s.phase = Phase::PreCombatMain;
    let source = s.create_card_in_zone(sample::ids::PHYREXIAN_RECLAMATION, 1, ZoneType::Battlefield).unwrap();
    layer(&mut s, source, mtg_gto::layers::LayerModification::ChangeController(0));
    let target = s.create_card_in_zone(sample::ids::ASHCOAT_OF_THE_SHADOW_SWARM, 0, ZoneType::Graveyard).unwrap(); s.designate_commander(target, 0).unwrap();
    s.objects.get_mut(&target).unwrap().controller = 2;
    let other = s.create_card_in_zone(sample::ids::ASHCOAT_OF_THE_SHADOW_SWARM, 2, ZoneType::Graveyard).unwrap(); s.objects.get_mut(&other).unwrap().controller = 0;
    s.players[0].mana_pool.black = 1; s.players[0].mana_pool.colorless = 1;
    let a = activation(&s, source, target); assert_eq!(activations(&s, source), vec![a.clone()]);
    rules::apply_action(&mut s, &a); assert_eq!(s.stack[0].controller, 0); s.move_object(source, ZoneType::Battlefield, ZoneType::Hand);
    resolve(&mut s); assert!(s.pending_movement.is_some()); assert_eq!(s.priority_player, 0);
    let answer = legal_actions(&s).into_iter().find(|a| matches!(a, Action::ChooseCommanderMovement {to_command: false, ..})).unwrap();
    rules::apply_action(&mut s, &answer); assert!(s.players[0].hand.contains(&target)); assert!(s.players[2].graveyard.contains(&other)); assert!(s.stack.is_empty());
}
#[test]
fn required_new_semantic_formats_reject_the_immediate_prior_serialized_contract() {
    let (s, _, _) = fixture(true);
    let value = serde_json::to_value(&s).unwrap(); assert_eq!(value["state_format"], serde_json::json!(0x3246_3343u32));
    let mut old = value; old["state_format"] = serde_json::json!(0x3246_3341u32); assert!(serde_json::from_value::<GameState>(old).is_err());
    let mut bytes = bincode::serialize(&s).unwrap(); bytes[..4].copy_from_slice(&0x3246_3341u32.to_le_bytes()); assert!(bincode::deserialize::<GameState>(&bytes).is_err());
    let mut immediate = serde_json::to_value(&s).unwrap(); immediate["state_format"] = serde_json::json!(0x3246_3342u32); assert!(serde_json::from_value::<GameState>(immediate).is_err());
    let mut immediate = bincode::serialize(&s).unwrap(); immediate[..4].copy_from_slice(&0x3246_3342u32.to_le_bytes()); assert!(bincode::deserialize::<GameState>(&immediate).is_err());
    assert_eq!(mtg_gto::public_projection::PROJECTION_SCHEMA, 7); assert_eq!(mtg_gto::solver::SEMANTIC_FORMAT, "mtg-joint-public-2f3a-v3");
}
#[test]
fn empty_commander_registry_still_owns_the_transaction_and_later_failed_resolution() {
    for fails in [false, true] {
        let (mut s, source, target) = fixture(true); s.commanders.clear(); let target = target.unwrap();
        let a = activation(&s, source, target); rules::apply_action(&mut s, &a);
        if fails {s.next_zone_event_group_id = u64::MAX;}
        resolve(&mut s);
        assert_eq!(s.players[0].life, 38); assert_eq!(s.players[0].mana_pool.total(), 0);
        if fails {assert!(s.gameplay_stopped()); assert_eq!(s.stack.len(), 1);
            assert!(matches!(s.stack[0].source, mtg_gto::game::StackSource::ReclamationAbility { .. }));
            assert!(s.players[0].graveyard.contains(&target)); assert_eq!(s.objects[&target].zone_change_count, 0);}
        else {assert!(!s.gameplay_stopped()); assert!(s.stack.is_empty()); assert!(s.players[0].hand.contains(&target));}
    }
}
#[test]
fn copied_mana_bonus_contexts_reject_atomic_autotap_but_allow_proved_zero_tap_or_inactive_contexts() {
    use mtg_gto::layers::LayerModification;
    for bonus in [sample::ids::KINNAN_BONDER_PRODIGY, sample::ids::NIRKANA_REVENANT] {
    for copy_into in [false, true] {
        for control in 0..6 {
            let (mut s, source, target) = fixture(true); let target = target.unwrap(); s.players[0].mana_pool.black = 0;
            let producer = mana_source(&mut s, 0, mtg_gto::mana::Color::Black);
            if bonus == sample::ids::NIRKANA_REVENANT {
                layer(&mut s, producer, LayerModification::AddType(CardType::Land));
                layer(&mut s, producer, LayerModification::AddSubtype(mtg_gto::card::Subtype("Swamp".into())));
            }
            let copied = s.create_card_in_zone(if copy_into {sample::ids::SOL_RING} else {bonus},
                if control == 3 {1} else {0}, ZoneType::Battlefield).unwrap();
            layer(&mut s, copied, LayerModification::CopyOf(if control == 5 {999_999_999} else if copy_into {bonus} else {sample::ids::SOL_RING}));
            match control {
                1 => layer(&mut s, copied, LayerModification::ChangeController(1)),
                2 => layer(&mut s, copied, LayerModification::RemoveAllAbilities),
                3 => layer(&mut s, copied, LayerModification::ChangeController(0)),
                4 => s.players[0].mana_pool.black = 1, _ => {}
            }
            let a = activation(&s, source, target);
            if control == 0 || control == 3 || control == 5 {assert!(!activations(&s, source).contains(&a)); unchanged(s, a);}
            else {
                assert!(activations(&s, source).contains(&a)); rules::apply_action(&mut s, &a);
                assert_eq!(s.players[0].life, 38); assert_eq!(s.players[0].mana_pool.total(), 0);
                assert_eq!(s.objects[&producer].tapped, control != 4); assert!(!s.objects[&copied].tapped);
            }
        }
    }
    }
}
