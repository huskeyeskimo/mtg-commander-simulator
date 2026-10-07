//! Milestone 2C: token residence is movement; later cessation is nonmovement.
use mtg_gto::{
    card::{CardDef, CardType, DynamicValue, Effect, TriggerCondition, TriggeredAbility, ZoneType},
    game::{CardDatabase, GameState, Phase},
    rules::{
        self,
        transitions::{transition_batch, ExactObjectRef, MovementKind, TransitionRequest},
    },
};
use std::sync::Arc;
fn fixture() -> GameState {
    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: 1,
        name: "Subject".into(),
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::Dies,
            effect: Effect::GainLife { amount: 1 },
            description: "death".into(),
        }],
        ..Default::default()
    });
    db.insert(CardDef {
        id: 2,
        name: "Hand CDA".into(),
        card_types: vec![CardType::Creature],
        power: Some(1),
        dynamic_toughness: Some(DynamicValue::CardsInHand),
        ..Default::default()
    });
    let mut s = GameState::new(2);
    s.card_db = Some(Arc::new(db));
    s.phase = Phase::PreCombatMain;
    s
}
fn token(s: &mut GameState, zone: ZoneType) -> u64 {
    let id = s.create_card_in_zone(1, 0, zone);
    s.objects.get_mut(&id).unwrap().is_token = true;
    id
}
fn depart(
    s: &mut GameState,
    id: u64,
    to: ZoneType,
) -> mtg_gto::rules::transitions::CommittedTransitionBatch {
    transition_batch(
        s,
        &[TransitionRequest {
            object: ExactObjectRef {
                id,
                generation: s.objects[&id].zone_change_count,
            },
            from: ZoneType::Battlefield,
            to,
            kind: MovementKind::Put,
        }],
    )
    .unwrap()
}
#[test]
fn kernel_residence_then_cessation_preserves_evidence() {
    let mut s = fixture();
    let id = token(&mut s, ZoneType::Battlefield);
    let batch = depart(&mut s, id, ZoneType::Graveyard);
    assert!(s.players[0].graveyard.contains(&id));
    assert_eq!(s.objects[&id].zone_change_count, 1);
    assert_eq!(batch.transitions[0].after.generation, 1);
    assert!(batch.transitions[0].creature_died());
    assert_eq!(s.pending_triggers.len(), 1);
    let events = s.pending_events.len();
    let group = s.next_zone_event_group_id;
    rules::check_state_based_actions(&mut s);
    assert!(!s.objects.contains_key(&id));
    assert!(!s.players[0].graveyard.contains(&id));
    assert_eq!(
        s.pending_events
            .iter()
            .filter(|event| matches!(event, mtg_gto::events::GameEvent::ZoneChange { .. }))
            .count(),
        events
    );
    assert_eq!(s.next_zone_event_group_id, group);
    assert_eq!(s.stack.len(), 1);
}
#[test]
fn raw_departure_resides_without_claiming_owned_death() {
    let mut s = fixture();
    let id = token(&mut s, ZoneType::Battlefield);
    s.move_object(id, ZoneType::Battlefield, ZoneType::Hand);
    assert!(s.players[0].hand.contains(&id));
    assert!(s.pending_triggers.is_empty());
    rules::check_state_based_actions(&mut s);
    assert!(!s.objects.contains_key(&id));
}
#[test]
fn nonbattlefield_token_cannot_move_again() {
    for zone in [
        ZoneType::Hand,
        ZoneType::Graveyard,
        ZoneType::Exile,
        ZoneType::Library,
        ZoneType::Command,
    ] {
        let mut s = fixture();
        let id = token(&mut s, zone);
        let before = serde_json::to_value(&s).unwrap();
        s.move_object(id, zone, ZoneType::Battlefield);
        assert_eq!(serde_json::to_value(&s).unwrap(), before);
        rules::check_state_based_actions(&mut s);
        assert!(!s.objects.contains_key(&id));
    }
}
#[test]
fn library_token_is_not_a_drawable_card() {
    let mut s = fixture();
    let id = token(&mut s, ZoneType::Library);
    rules::draw_cards(&mut s, 0, 1);
    assert!(s.players[0].library.contains(&id));
    assert!(s.players[0].hand.is_empty());
    assert_eq!(s.loss_boundary.pending_failed_draws, vec![0]);
    assert!(s.pending_events.is_empty());
}
#[test]
fn linked_token_cannot_move_on_source_departure() {
    let mut s = fixture();
    let source = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let id = token(&mut s, ZoneType::Exile);
    s.objects.get_mut(&id).unwrap().exiled_by = Some(source);
    depart(&mut s, source, ZoneType::Graveyard);
    assert!(s.players[0].exile.contains(&id));
    assert_eq!(s.objects[&id].zone_change_count, 0);
    assert_eq!(s.objects[&id].exiled_by, Some(source));
    assert_eq!(s.pending_events.len(), 1);
}
#[test]
fn hand_token_is_not_a_card_for_dynamic_values() {
    let mut s = fixture();
    token(&mut s, ZoneType::Hand);
    let id = s.create_card_in_zone(2, 0, ZoneType::Battlefield);
    assert_eq!(s.effective_toughness(id), 0);
}

use mtg_gto::{
    action::{
        canonical::{canonicalize, canonicalize_actions, resolve},
        legal_actions, Action,
    },
    card::{KeywordAbility, TargetSpec},
    game::{StackEntry, StackSource, Target},
    info_set::InformationSet,
};
fn info(s: &GameState, player: usize) -> u64 {
    InformationSet::from_view(&s.visible_state(player), s.card_db()).hash_value()
}
fn restores(s: &GameState) -> Vec<GameState> {
    let mut snapshot = s.clone();
    snapshot.restore(s.snapshot());
    let mut result = vec![
        s.clone(),
        snapshot,
        serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap(),
        bincode::deserialize(&bincode::serialize(s).unwrap()).unwrap(),
    ];
    for state in &mut result {
        state.card_db = s.card_db.clone();
    }
    result
}
fn finish(s: &mut GameState) {
    rules::check_state_based_actions(s);
    for _ in 0..200 {
        if s.stack.is_empty() && s.pending_triggers.is_empty() {
            assert!(!s.gameplay_stopped());
            return;
        }
        if let Some(action) = legal_actions(s).into_iter().find(|a| {
            matches!(
                a,
                Action::OrderTriggerOccurrences { .. } | Action::OrderTriggers { .. }
            )
        }) {
            rules::apply_action(s, &action);
        } else {
            rules::apply_action(s, &Action::PassPriority);
        }
    }
    panic!("token continuation did not settle");
}
#[test]
fn all_actual_destinations_reside_then_cease_without_id_reuse() {
    for zone in [
        ZoneType::Graveyard,
        ZoneType::Exile,
        ZoneType::Hand,
        ZoneType::Library,
    ] {
        let mut s = fixture();
        let id = token(&mut s, ZoneType::Battlefield);
        let group = s.next_zone_event_group_id;
        let b = depart(&mut s, id, zone);
        assert_eq!(b.transitions[0].destination.zone, zone);
        assert_eq!(
            b.transitions[0].creature_died(),
            zone == ZoneType::Graveyard
        );
        assert!(s.objects.contains_key(&id));
        assert_eq!(s.objects[&id].zone_change_count, 1);
        let historical = b.clone();
        finish(&mut s);
        assert!(!s.objects.contains_key(&id));
        assert_eq!(s.next_zone_event_group_id, group + 1);
        assert_eq!(historical.group_id, b.group_id);
        assert_eq!(
            serde_json::to_value(historical.transitions).unwrap(),
            serde_json::to_value(b.transitions).unwrap()
        );
        let fresh = token(&mut s, ZoneType::Battlefield);
        assert!(fresh > id);
        rules::check_state_based_actions(&mut s);
        assert!(s.battlefield.contains(&fresh));
    }
}
#[test]
fn token_and_nontoken_share_one_departure_and_departing_watcher_history() {
    let mut s = fixture();
    let db = Arc::make_mut(s.card_db.as_mut().unwrap());
    db.insert(CardDef {
        id: 3,
        name: "Watcher".into(),
        card_types: vec![CardType::Creature],
        power: Some(1),
        toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::ACreatureDies,
            effect: Effect::GainLife { amount: 2 },
            description: "watch".into(),
        }],
        ..Default::default()
    });
    let t = token(&mut s, ZoneType::Battlefield);
    let real = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let watcher = s.create_card_in_zone(3, 1, ZoneType::Battlefield);
    let requests: Vec<_> = [t, real, watcher]
        .into_iter()
        .map(|id| TransitionRequest {
            object: ExactObjectRef { id, generation: 0 },
            from: ZoneType::Battlefield,
            to: ZoneType::Graveyard,
            kind: MovementKind::Put,
        })
        .collect();
    let batch = transition_batch(&mut s, &requests).unwrap();
    assert_eq!(batch.transitions.len(), 3);
    assert_eq!(
        s.pending_triggers
            .iter()
            .filter(|p| p.source_id == watcher)
            .count(),
        3
    );
    assert_eq!(s.pending_triggers.len(), 5);
    assert!(s.objects.contains_key(&t));
    assert!(s.pending_triggers.iter().all(|p| p
        .context
        .zone_transition
        .as_ref()
        .unwrap()
        .group_id
        == batch.group_id));
    finish(&mut s);
    assert!(!s.objects.contains_key(&t));
    assert!(s.players[0].graveyard.contains(&real));
    assert_eq!(s.players[0].life, 22);
    assert_eq!(s.players[1].life, 26);
}
#[test]
fn keyword_tokens_keep_occurrences_and_defensive_nonreturn_has_no_side_effect() {
    use mtg_gto::rules::transitions::return_death_keyword;
    for keywords in [
        vec![KeywordAbility::Undying],
        vec![KeywordAbility::Persist],
        vec![KeywordAbility::Undying, KeywordAbility::Persist],
    ] {
        let mut s = fixture();
        let id = token(&mut s, ZoneType::Battlefield);
        s.objects.get_mut(&id).unwrap().temp_keywords = keywords.clone();
        depart(&mut s, id, ZoneType::Graveyard);
        assert_eq!(s.pending_triggers.len(), 1 + keywords.len());
        for keyword in &keywords {
            let context = s.pending_triggers.iter().find(|p| matches!(p.context.effect, Effect::ReturnWithDeathKeyword { keyword: k } if k == *keyword)).unwrap().context.zone_transition.clone().unwrap();
            let before = serde_json::to_value(&s).unwrap();
            let events = s.pending_events.len();
            assert!(!return_death_keyword(&mut s, &context, *keyword).unwrap());
            assert_eq!(serde_json::to_value(&s).unwrap(), before);
            assert_eq!(s.pending_events.len(), events);
            assert_eq!(s.objects[&id].zone_change_count, 1);
            assert!(!s.battlefield.contains(&id));
        }
        for mut restored in restores(&s) {
            finish(&mut restored);
            assert!(!restored.objects.contains_key(&id));
            assert_eq!(restored.players[0].life, 21);
        }
    }
}
#[test]
fn transient_visibility_counts_and_token_card_projection_are_distinct() {
    for zone in [
        ZoneType::Hand,
        ZoneType::Library,
        ZoneType::Graveyard,
        ZoneType::Exile,
    ] {
        let mut s = fixture();
        let id = token(&mut s, ZoneType::Battlefield);
        depart(&mut s, id, zone);
        let own = s.visible_state(0);
        let other = s.visible_state(1);
        assert_eq!(own.objects.contains_key(&id), zone != ZoneType::Library);
        assert_eq!(
            other.objects.contains_key(&id),
            matches!(zone, ZoneType::Graveyard | ZoneType::Exile)
        );
        assert_eq!(other.opp_hand_size, 0);
        assert_eq!(other.opp_library_size, 0);
        let token_info = info(&s, 0);
        let mut real = s.clone();
        real.objects.get_mut(&id).unwrap().is_token = false;
        if zone != ZoneType::Library {
            assert_ne!(token_info, info(&real, 0));
        }
        finish(&mut s);
        assert!(!s.visible_state(0).objects.contains_key(&id));
    }
}
#[test]
fn resident_clone_snapshot_json_bincode_continue_equivalently() {
    for zone in [
        ZoneType::Hand,
        ZoneType::Library,
        ZoneType::Graveyard,
        ZoneType::Exile,
    ] {
        let mut s = fixture();
        let id = token(&mut s, ZoneType::Battlefield);
        depart(&mut s, id, zone);
        let mut expected = s.clone();
        finish(&mut expected);
        for mut restored in restores(&s) {
            assert!(restored.objects.contains_key(&id));
            assert_eq!(info(&s, 0), info(&restored, 0));
            finish(&mut restored);
            assert_eq!(
                serde_json::to_value(&restored).unwrap(),
                serde_json::to_value(&expected).unwrap()
            );
            assert_eq!(info(&restored, 0), info(&expected, 0));
        }
    }
}
#[test]
fn terminal_before_cessation_preserves_residence_and_loss_coordinates() {
    for players in [2, 3] {
        let mut s = GameState::new(players);
        s.card_db = fixture().card_db;
        s.phase = Phase::PreCombatMain;
        let id = token(&mut s, ZoneType::Battlefield);
        depart(&mut s, id, ZoneType::Graveyard);
        s.players[1].life = 0;
        rules::check_state_based_actions(&mut s);
        assert!(s.objects.contains_key(&id));
        assert!(s.players[0].graveyard.contains(&id));
        if players == 2 {
            assert!(s.game_over);
            assert_eq!(s.winner, Some(0));
            assert!(s.loss_boundary.terminal.is_some());
        } else {
            assert!(!s.game_over);
            assert!(s.loss_boundary.unsupported.is_some());
        }
        let before = serde_json::to_value(&s).unwrap();
        for mut restored in restores(&s) {
            rules::check_state_based_actions(&mut restored);
            assert_eq!(serde_json::to_value(&restored).unwrap(), before);
            assert!(legal_actions(&restored).is_empty());
        }
    }
}
#[test]
fn synthetic_commander_token_uses_actual_command_destination() {
    let mut s = GameState::new_commander(2);
    s.card_db = fixture().card_db;
    let id = token(&mut s, ZoneType::Battlefield);
    s.players[0].commander_object_id = Some(id);
    let b = depart(&mut s, id, ZoneType::Graveyard);
    assert_eq!(b.transitions[0].destination.zone, ZoneType::Command);
    assert!(!b.transitions[0].creature_died());
    assert!(s.pending_triggers.is_empty());
    assert!(s.players[0].command_zone.contains(&id));
    rules::check_state_based_actions(&mut s);
    assert!(!s.objects.contains_key(&id));
    assert_eq!(s.players[0].commander_object_id, Some(id));
}
#[test]
fn failed_draw_skips_tokens_and_draws_next_real_card_without_moving_token() {
    let mut s = fixture();
    let t = token(&mut s, ZoneType::Library);
    let real = s.create_card_in_zone(1, 0, ZoneType::Library);
    rules::draw_cards(&mut s, 0, 1);
    assert_eq!(s.players[0].hand, vec![real]);
    assert_eq!(s.players[0].library, vec![t]);
    assert!(s.loss_boundary.pending_failed_draws.is_empty());
    rules::draw_cards(&mut s, 0, 1);
    assert_eq!(s.loss_boundary.pending_failed_draws, vec![0]);
    assert_eq!(s.objects[&t].zone_change_count, 0);
}
fn ordering_fixture(count: usize, reverse: bool, prefix: usize) -> GameState {
    let mut s = fixture();
    for _ in 0..prefix {
        let id = token(&mut s, ZoneType::Exile);
        rules::check_state_based_actions(&mut s);
        assert!(!s.objects.contains_key(&id));
    }
    let ids: Vec<_> = (0..count)
        .map(|_| token(&mut s, ZoneType::Battlefield))
        .collect();
    let mut requests: Vec<_> = ids
        .iter()
        .map(|&id| TransitionRequest {
            object: ExactObjectRef { id, generation: 0 },
            from: ZoneType::Battlefield,
            to: ZoneType::Graveyard,
            kind: MovementKind::Put,
        })
        .collect();
    if reverse {
        s.battlefield.reverse();
        requests.reverse();
    }
    transition_batch(&mut s, &requests).unwrap();
    if reverse {
        s.players[0].graveyard.reverse();
        s.pending_triggers.reverse();
    }
    s
}
#[test]
fn six_or_fewer_canonical_orders_survive_residence_and_cessation() {
    for count in [2, 6] {
        let mut a = ordering_fixture(count, false, 0);
        let mut b = ordering_fixture(count, true, 5);
        assert_eq!(info(&a, 0), info(&b, 0));
        rules::check_state_based_actions(&mut a);
        rules::check_state_based_actions(&mut b);
        assert_eq!(info(&a, 0), info(&b, 0));
        assert!(a.objects.is_empty());
        assert!(b.objects.is_empty());
        let actions_a = legal_actions(&a);
        let actions_b = legal_actions(&b);
        let normal_a = InformationSet::normalize_retained_view(&a.visible_state(0));
        let normal_b = InformationSet::normalize_retained_view(&b.visible_state(0));
        let mut ca: Vec<_> = canonicalize_actions(&actions_a, &a, &normal_a)
            .iter()
            .map(|c| serde_json::to_vec(c).unwrap())
            .collect();
        let mut cb: Vec<_> = canonicalize_actions(&actions_b, &b, &normal_b)
            .iter()
            .map(|c| serde_json::to_vec(c).unwrap())
            .collect();
        ca.sort();
        cb.sort();
        assert_eq!(ca, cb);
        for action in actions_a
            .iter()
            .filter(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        {
            let canonical = canonicalize(action, &a);
            let rebuilt = resolve(&canonical, &b, b.priority_player).unwrap();
            assert!(actions_b.contains(&rebuilt));
            assert_eq!(canonicalize(&rebuilt, &b), canonical);
        }
        finish(&mut a);
        finish(&mut b);
        assert_eq!(info(&a, 0), info(&b, 0));
        assert_eq!(a.players[0].life, 20 + count as i32);
    }
}
#[test]
fn more_than_six_preserves_semantic_projection_and_own_fifo_boundary() {
    let mut a = ordering_fixture(8, false, 0);
    let mut b = ordering_fixture(8, true, 5);
    rules::check_state_based_actions(&mut a);
    rules::check_state_based_actions(&mut b);
    assert_eq!(info(&a, 0), info(&b, 0));
    for s in [&mut a, &mut b] {
        let orders: Vec<_> = legal_actions(s)
            .into_iter()
            .filter(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
            .collect();
        assert_eq!(orders.len(), 1);
        let c = canonicalize(&orders[0], s);
        let rebuilt = resolve(&c, s, s.priority_player).unwrap();
        assert_eq!(rebuilt, orders[0]);
        finish(s);
    }
    assert_eq!(info(&a, 0), info(&b, 0));
}
#[test]
fn token_and_real_same_definition_do_not_share_card_action_coordinates() {
    use mtg_gto::mana::ManaCost;
    let mut a = fixture();
    Arc::make_mut(a.card_db.as_mut().unwrap())
        .cards
        .get_mut(&1)
        .unwrap()
        .mana_cost = Some(ManaCost::zero());
    let t = token(&mut a, ZoneType::Hand);
    let card = a.create_card_in_zone(1, 0, ZoneType::Hand);
    let mut b = a.clone();
    b.players[0].hand.reverse();
    let cast = Action::CastSpell {
        object_id: card,
        targets: vec![],
    };
    assert!(legal_actions(&a).contains(&cast));
    assert!(!legal_actions(&a).contains(&Action::CastSpell {
        object_id: t,
        targets: vec![]
    }));
    let c = canonicalize(&cast, &a);
    assert_eq!(resolve(&c, &b, 0), Some(cast.clone()));
    assert!(matches!(
        c,
        mtg_gto::action::canonical::CanonicalAction::CastSpell { hand_index: 0, .. }
    ));
}
#[test]
fn card_only_hand_target_excludes_resident_token() {
    let mut s = fixture();
    let t = token(&mut s, ZoneType::Hand);
    let card = s.create_card_in_zone(1, 0, ZoneType::Hand);
    assert!(!mtg_gto::targeting::target_is_legal(
        &s,
        0,
        &TargetSpec::CardInHand,
        &Target::Object(t)
    ));
    assert!(mtg_gto::targeting::target_is_legal(
        &s,
        0,
        &TargetSpec::CardInHand,
        &Target::Object(card)
    ));
}
#[test]
fn copy_order_suspension_preserves_resident_tokens_until_existing_completion() {
    use mtg_gto::rules::{
        begin_terminal_copy_batch, prepare_spell_copy, snapshot_stack_spell, CopyBatchOutcome,
        CopyTargetPolicy,
    };
    let mut s = fixture();
    let t = token(&mut s, ZoneType::Battlefield);
    depart(&mut s, t, ZoneType::Exile);
    let db = Arc::make_mut(s.card_db.as_mut().unwrap());
    db.insert(CardDef {
        id: 4,
        name: "Copyable".into(),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::GainLife { amount: 1 }),
        ..Default::default()
    });
    let spell = s.create_card_in_zone(4, 0, ZoneType::Stack);
    let stack_id = s.new_stack_id();
    s.stack.push(StackEntry {
        id: stack_id,
        source: StackSource::Spell(spell),
        controller: 0,
        targets: vec![],
        target_generations: vec![],
    });
    let snapshot = snapshot_stack_spell(&s, stack_id).unwrap();
    let item = prepare_spell_copy(&s, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
    assert!(matches!(
        begin_terminal_copy_batch(&mut s, vec![item.clone(), item.clone(), item], true).unwrap(),
        CopyBatchOutcome::Pending
    ));
    for mut restored in restores(&s) {
        rules::check_state_based_actions(&mut restored);
        assert!(restored.objects.contains_key(&t));
        rules::apply_action(&mut restored, &Action::ChooseNextCopy { item_index: 0 });
        assert!(restored.objects.contains_key(&t));
        rules::apply_action(&mut restored, &Action::ChooseNextCopy { item_index: 1 });
        assert!(!restored.objects.contains_key(&t));
        assert_eq!(
            restored
                .stack
                .iter()
                .filter(|entry| matches!(entry.source, StackSource::SpellCopy { .. }))
                .count(),
            3
        );
        finish(&mut restored);
        assert_eq!(restored.players[0].life, 24);
    }
}
#[test]
fn token_creation_during_spell_and_trigger_resolution_stays_on_battlefield() {
    use mtg_gto::mana::ManaCost;
    let mut s = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 4,
        name: "Generic token production".into(),
        card_types: vec![CardType::Sorcery],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::Multiple(vec![
            Effect::CreateTokenFromDef { card_def_id: 1 },
            Effect::CreateTokenFromDef { card_def_id: 1 },
        ])),
        ..Default::default()
    });
    let spell = s.create_card_in_zone(4, 0, ZoneType::Hand);
    rules::apply_action(
        &mut s,
        &Action::CastSpell {
            object_id: spell,
            targets: vec![],
        },
    );
    finish(&mut s);
    assert_eq!(s.battlefield.len(), 2);
    assert!(s.battlefield.iter().all(|id| s.objects[id].is_token));
    let mut triggered = fixture();
    Arc::make_mut(triggered.card_db.as_mut().unwrap())
        .cards
        .get_mut(&1)
        .unwrap()
        .triggered_abilities[0]
        .effect = Effect::CreateTokenFromDef { card_def_id: 1 };
    let source = token(&mut triggered, ZoneType::Battlefield);
    depart(&mut triggered, source, ZoneType::Graveyard);
    finish(&mut triggered);
    assert!(!triggered.objects.contains_key(&source));
    assert_eq!(triggered.battlefield.len(), 1);
    let new = triggered.battlefield[0];
    assert!(new > source);
    assert!(triggered.objects[&new].is_token);
}
#[test]
fn search_choices_ignore_tokens_even_when_definition_matches_real_card() {
    let mut s = fixture();
    let t = token(&mut s, ZoneType::Library);
    let real = s.create_card_in_zone(1, 0, ZoneType::Library);
    s.players[0].tutor_targets = vec![1];
    s.pending_tutor = Some(mtg_gto::game::PendingTutor {
        controller: 0,
        destination: ZoneType::Hand,
        subtype_filter: vec![],
    });
    assert_eq!(
        legal_actions(&s),
        vec![Action::ChooseTutorTarget { card_id: 1 }, Action::Concede]
    );
    rules::apply_action(&mut s, &Action::ChooseTutorTarget { card_id: 1 });
    assert!(s.players[0].hand.contains(&real));
    assert!(!s.players[0].hand.contains(&t));
    let mut only_token = fixture();
    token(&mut only_token, ZoneType::Library);
    only_token.players[0].tutor_targets = vec![1];
    only_token.pending_tutor = Some(mtg_gto::game::PendingTutor {
        controller: 0,
        destination: ZoneType::Hand,
        subtype_filter: vec![],
    });
    assert_eq!(
        legal_actions(&only_token),
        vec![Action::PassPriority, Action::Concede]
    );
}
#[test]
fn token_residents_do_not_pay_card_escape_costs_or_delve_reduction() {
    use mtg_gto::mana::ManaCost;
    let mut s = fixture();
    let t = token(&mut s, ZoneType::Graveyard);
    let db = Arc::make_mut(s.card_db.as_mut().unwrap());
    db.insert(CardDef {
        id: 3,
        name: "Escape control".into(),
        card_types: vec![CardType::Sorcery],
        mana_cost: Some(ManaCost::zero()),
        escape_exile_count: Some(1),
        ..Default::default()
    });
    db.insert(CardDef {
        id: 4,
        name: "Delve count control".into(),
        card_types: vec![CardType::Sorcery],
        mana_cost: Some(ManaCost::zero()),
        keywords: vec![KeywordAbility::Delve],
        ..Default::default()
    });
    let caster = s.create_card_in_zone(3, 0, ZoneType::Graveyard);
    let action = Action::CastFromGraveyard {
        object_id: caster,
        targets: vec![],
    };
    assert!(!legal_actions(&s).contains(&action));
    let before = serde_json::to_value(&s).unwrap();
    rules::apply_action(&mut s, &action);
    assert_eq!(serde_json::to_value(&s).unwrap(), before);
    assert!(s.players[0].graveyard.contains(&t));
    assert_eq!(rules::spell_cost_reduction(&s, 0, 4), 1);
}
#[test]
fn represented_token_prevention_redirection_and_indestructibility_keep_actual_endpoints() {
    use mtg_gto::{
        replacement::{ReplacementAction, ReplacementEffect, ReplacementEventKind},
        rules::transitions::destroy_batch,
    };
    for action in [
        ReplacementAction::Prevent,
        ReplacementAction::RedirectToZone(ZoneType::Exile),
    ] {
        let mut s = fixture();
        let id = token(&mut s, ZoneType::Battlefield);
        s.objects
            .get_mut(&id)
            .unwrap()
            .temp_keywords
            .push(KeywordAbility::Undying);
        s.replacement_effects.push(ReplacementEffect {
            source_id: 0,
            controller: 0,
            applies_to: ReplacementEventKind::WouldDie,
            action: action.clone(),
            is_self_replacement: true,
            description: "token replacement control".into(),
        });
        s.invalidate_characteristics_cache();
        let result = destroy_batch(&mut s, &[ExactObjectRef { id, generation: 0 }]).unwrap();
        if action == ReplacementAction::Prevent {
            assert!(result.is_none());
            assert!(s.battlefield.contains(&id));
            assert!(s.pending_events.is_empty());
        } else {
            let b = result.unwrap();
            assert_eq!(b.transitions[0].destination.zone, ZoneType::Exile);
            assert!(!b.transitions[0].creature_died());
            assert!(s.players[0].exile.contains(&id));
            rules::check_state_based_actions(&mut s);
            assert!(!s.objects.contains_key(&id));
        }
        assert!(s.pending_triggers.is_empty());
        assert!(s.stack.is_empty());
    }
    let mut s = fixture();
    let id = token(&mut s, ZoneType::Battlefield);
    s.objects
        .get_mut(&id)
        .unwrap()
        .temp_keywords
        .push(KeywordAbility::Indestructible);
    s.invalidate_characteristics_cache();
    assert!(
        destroy_batch(&mut s, &[ExactObjectRef { id, generation: 0 }])
            .unwrap()
            .is_none()
    );
    assert!(s.battlefield.contains(&id));
}
#[test]
fn token_cessation_precedes_multicontroller_apnap_ordering() {
    let mut s = GameState::new(3);
    s.card_db = fixture().card_db;
    s.phase = Phase::PreCombatMain;
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 3,
        name: "Multicontroller observer".into(),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::ACreatureDies,
            effect: Effect::GainLife { amount: 1 },
            description: "observe".into(),
        }],
        ..Default::default()
    });
    for p in 0..3 {
        s.create_card_in_zone(3, p, ZoneType::Battlefield);
    }
    let ids: Vec<_> = (0..2)
        .map(|_| token(&mut s, ZoneType::Battlefield))
        .collect();
    for id in &ids {
        s.objects.get_mut(id).unwrap().controller = 1;
    }
    let requests: Vec<_> = ids
        .iter()
        .map(|&id| TransitionRequest {
            object: ExactObjectRef { id, generation: 0 },
            from: ZoneType::Battlefield,
            to: ZoneType::Graveyard,
            kind: MovementKind::Put,
        })
        .collect();
    transition_batch(&mut s, &requests).unwrap();
    assert!(ids.iter().all(|id| s.objects.contains_key(id)));
    assert!(s.stack.is_empty());
    rules::check_state_based_actions(&mut s);
    assert!(ids.iter().all(|id| !s.objects.contains_key(id)));
    assert!(s.stack.is_empty());
    assert_eq!(s.priority_player, 0);
    for player in 0..3 {
        assert_eq!(s.priority_player, player);
        let action = legal_actions(&s)
            .into_iter()
            .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
            .unwrap();
        rules::apply_action(&mut s, &action);
    }
    assert_eq!(
        s.stack
            .iter()
            .map(|entry| entry.controller)
            .collect::<Vec<_>>(),
        vec![0, 0, 1, 1, 1, 1, 2, 2]
    );
    for mut restored in restores(&s) {
        finish(&mut restored);
        assert_eq!(
            restored.players.iter().map(|p| p.life).collect::<Vec<_>>(),
            vec![22, 24, 22]
        );
    }
}
