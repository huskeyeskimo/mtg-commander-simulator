use mtg_gto::{
    action::{canonical, Action},
    card::{CardDef, CardType, Effect, Subtype, TargetSpec, ZoneType},
    game::{CardDatabase, GameState, Phase},
    info_set::InformationSet,
    layers::{AffectedObjects, StaticAbility},
    mana::ManaCost,
    rules,
    rules::transitions::{transition_batch, ExactObjectRef, MovementKind, TransitionRequest},
};
use std::sync::Arc;
fn fixture() -> (GameState, u64, u64) {
    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: 1,
        name: "Body".into(),
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        ..Default::default()
    });
    db.insert(CardDef {
        id: 2,
        name: "Equipment".into(),
        card_types: vec![CardType::Artifact],
        subtypes: vec![Subtype("Equipment".into())],
        equip_cost: Some(ManaCost::zero()),
        static_abilities: vec![StaticAbility::Anthem {
            power: 1,
            toughness: 0,
            affected: AffectedObjects::AttachedTo,
        }],
        ..Default::default()
    });
    db.insert(CardDef {
        id: 3,
        name: "Aura".into(),
        card_types: vec![CardType::Enchantment],
        subtypes: vec![Subtype("Aura".into())],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::Buff {
            power: 1,
            toughness: 0,
            until_eot: false,
        }),
        ..Default::default()
    });
    let mut s = GameState::new(2);
    s.card_db = Some(Arc::new(db));
    s.phase = Phase::PreCombatMain;
    let e = s.create_card_in_zone(2, 0, ZoneType::Battlefield);
    let c = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
    rules::apply_action(
        &mut s,
        &Action::Equip {
            equipment_id: e,
            target_id: c,
        },
    );
    (s, e, c)
}
fn leave(s: &mut GameState, id: u64) {
    let generation = s.objects[&id].zone_change_count;
    transition_batch(
        s,
        &[TransitionRequest {
            object: ExactObjectRef { id, generation },
            from: ZoneType::Battlefield,
            to: ZoneType::Exile,
            kind: MovementKind::Put,
        }],
    )
    .unwrap();
}
fn blink(s: &mut GameState, id: u64) {
    leave(s, id);
    s.move_object(id, ZoneType::Exile, ZoneType::Battlefield);
    s.refresh_continuous_effects();
}
fn info(s: &GameState) -> InformationSet {
    InformationSet::from_view(&s.visible_state(0), s.card_db()).unwrap()
}
#[test]
fn target_departure_detaches_immediately() {
    let (mut s, e, c) = fixture();
    leave(&mut s, c);
    assert_eq!(
        s.objects[&e].attachment_link().map(|link| link.target.id),
        None
    );
}
#[test]
fn target_blink_does_not_rebind() {
    let (mut s, e, c) = fixture();
    blink(&mut s, c);
    rules::check_state_based_actions(&mut s);
    assert_eq!(
        s.objects[&e].attachment_link().map(|link| link.target.id),
        None
    );
    assert_eq!(s.effective_power(c), 2);
}
#[test]
fn source_blink_does_not_rebind() {
    let (mut s, e, c) = fixture();
    blink(&mut s, e);
    rules::check_state_based_actions(&mut s);
    assert_eq!(
        s.objects[&e].attachment_link().map(|link| link.target.id),
        None
    );
    assert_eq!(s.effective_power(c), 2);
}
#[test]
fn source_departure_clears_reverse() {
    let (mut s, e, c) = fixture();
    leave(&mut s, e);
    assert!(s.attachments_of(s.exact_object(c).unwrap()).is_empty());
}
#[test]
fn pairing_changes_information() {
    let (s, e, _c) = fixture();
    let mut a = s;
    let other = a.create_card_in_zone(1, 0, ZoneType::Battlefield);
    a.objects.get_mut(&other).unwrap().tapped = true;
    let mut b = a.clone();
    let prepared = b
        .prepare_attach(
            b.exact_object(e).unwrap(),
            b.exact_object(other).unwrap(),
            mtg_gto::card::AttachmentContext::ExistingEquip,
        )
        .unwrap();
    b.commit_attach(prepared).unwrap();
    b.invalidate_characteristics_cache();
    assert_ne!(info(&a).hash_value(), info(&b).hash_value());
}
#[test]
fn canonical_target_respects_semantic_identity() {
    let (mut a, e, c) = fixture();
    let other = a.create_card_in_zone(1, 0, ZoneType::Battlefield);
    a.objects.get_mut(&other).unwrap().tapped = true;
    let mut b = a.clone();
    let mut ci = b.objects.remove(&c).unwrap();
    let mut oi = b.objects.remove(&other).unwrap();
    ci.object_id = other;
    oi.object_id = c;
    b.objects.insert(other, ci);
    b.objects.insert(c, oi);
    let prepared = b
        .prepare_attach(
            b.exact_object(e).unwrap(),
            b.exact_object(other).unwrap(),
            mtg_gto::card::AttachmentContext::ExistingEquip,
        )
        .unwrap();
    b.commit_attach(prepared).unwrap();
    b.battlefield.reverse();
    b.invalidate_characteristics_cache();
    let x = canonical::canonicalize(
        &Action::Equip {
            equipment_id: e,
            target_id: c,
        },
        &a,
    )
    .unwrap();
    assert_eq!(
        canonical::resolve(&x, &b, 0).unwrap(),
        Some(Action::Equip {
            equipment_id: e,
            target_id: other
        })
    );
}
#[test]
fn restores_do_not_reconnect_source() {
    let (mut s, e, _) = fixture();
    leave(&mut s, e);
    let mut snap = s.clone();
    snap.restore(s.snapshot()).unwrap();
    let mut j: GameState = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    j.card_db = s.card_db.clone();
    let mut b: GameState = bincode::deserialize(&bincode::serialize(&s).unwrap()).unwrap();
    b.card_db = s.card_db.clone();
    let results: Vec<_> = [s, snap, j, b]
        .into_iter()
        .map(|mut v| {
            v.move_object(e, ZoneType::Exile, ZoneType::Battlefield);
            rules::check_state_based_actions(&mut v);
            v.objects[&e].attachment_link().map(|link| link.target.id)
        })
        .collect();
    assert_eq!(results, vec![None; 4]);
}
#[test]
fn token_settlement_control() {
    let (mut s, e, c) = fixture();
    s.objects.get_mut(&c).unwrap().is_token = true;
    leave(&mut s, c);
    assert!(s.players[0].exile.contains(&c));
    rules::check_state_based_actions(&mut s);
    assert!(!s.objects.contains_key(&c));
    assert_eq!(
        s.objects[&e].attachment_link().map(|link| link.target.id),
        None
    );
}
#[test]
fn existing_target_type_legality() {
    let (mut s, e, c) = fixture();
    s.continuous_effects
        .push(mtg_gto::layers::ContinuousEffect {
            source_id: e,
            controller: 0,
            timestamp: 99,
            duration: mtg_gto::layers::Duration::UntilEndOfTurn,
            affected: AffectedObjects::Specific(c),
            modification: mtg_gto::layers::LayerModification::RemoveType(CardType::Creature),
        });
    s.invalidate_characteristics_cache();
    rules::check_state_based_actions(&mut s);
    assert_eq!(
        s.objects[&e].attachment_link().map(|link| link.target.id),
        None
    );
    let _ = TargetSpec::AnyCreature;
}

#[test]
fn departed_target_does_not_get_attachment_bonus() {
    let (mut s, _, c) = fixture();
    leave(&mut s, c);
    assert_eq!(s.effective_power(c), 2);
}

#[test]
fn composed_effect_attachment_distribution_distinguishes_02_11() {
    let (mut a, e1, c1) = fixture();
    let c2 = a.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let c3 = a.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let c4 = a.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let x1 = a.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let x2 = a.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let e2 = a.create_card_in_zone(2, 0, ZoneType::Battlefield);
    for (source_id, target) in [(x1, c1), (x1, c2), (x2, c3), (x2, c4)] {
        a.continuous_effects
            .push(mtg_gto::layers::ContinuousEffect {
                source_id,
                controller: 0,
                timestamp: 100,
                duration: mtg_gto::layers::Duration::Permanent,
                affected: AffectedObjects::Specific(target),
                modification: mtg_gto::layers::LayerModification::ModifyPT(0, 0),
            });
    }
    rules::apply_action(
        &mut a,
        &Action::Equip {
            equipment_id: e2,
            target_id: c2,
        },
    );
    let mut b = a.clone();
    rules::apply_action(
        &mut b,
        &Action::Equip {
            equipment_id: e2,
            target_id: c3,
        },
    );
    assert_eq!(
        a.objects[&e1].attachment_link().map(|link| link.target.id),
        Some(c1)
    );
    assert_ne!(info(&a).hash_value(), info(&b).hash_value());
}

fn established(state: &mut GameState, source: u64, target: u64) {
    let prepared = state
        .prepare_attach(
            state.exact_object(source).unwrap(),
            state.exact_object(target).unwrap(),
            mtg_gto::card::AttachmentContext::Established,
        )
        .unwrap();
    state.commit_attach(prepared).unwrap();
}
fn equipped_watcher(
    token: bool,
    source_controller: usize,
    target_controller: usize,
) -> (GameState, u64, u64) {
    let (mut state, equipment, creature) = fixture();
    let mut db = (*state.card_db.as_ref().unwrap().as_ref()).clone();
    let mut source = db.get(2).unwrap().clone();
    source
        .triggered_abilities
        .push(mtg_gto::card::TriggeredAbility {
            trigger: mtg_gto::card::TriggerCondition::EquippedCreatureDies,
            effect: Effect::DrawCards { count: 1 },
            description: "When equipped creature dies, draw one.".into(),
        });
    db.insert(source);
    state.card_db = Some(Arc::new(db));
    state.objects.get_mut(&equipment).unwrap().controller = source_controller;
    state.objects.get_mut(&creature).unwrap().controller = target_controller;
    state.objects.get_mut(&creature).unwrap().is_token = token;
    state.invalidate_characteristics_cache();
    for player in 0..2 {
        for _ in 0..5 {
            state.create_card_in_zone(1, player, ZoneType::Library);
        }
    }
    (state, equipment, creature)
}
fn depart(
    state: &mut GameState,
    ids: &[u64],
    destination: ZoneType,
) -> mtg_gto::rules::transitions::CommittedTransitionBatch {
    let requests: Vec<_> = ids
        .iter()
        .map(|&id| TransitionRequest {
            object: state.exact_object(id).unwrap(),
            from: ZoneType::Battlefield,
            to: destination,
            kind: MovementKind::Put,
        })
        .collect();
    transition_batch(state, &requests).unwrap()
}
#[test]
fn historical_equipped_death_survives_source_departure_and_token_cessation() {
    for token in [false, true] {
        for source_leaves in [false, true] {
            for controller in [0, 1] {
                let (mut state, e, c) = equipped_watcher(token, controller, 1 - controller);
                let before = state.exact_object(c).unwrap();
                let source_before = state.exact_object(e).unwrap();
                let members = if source_leaves { vec![e, c] } else { vec![c] };
                depart(&mut state, &members, ZoneType::Graveyard);
                assert_eq!(state.pending_triggers.len(), 1);
                let trigger = &state.pending_triggers[0];
                assert_eq!(trigger.controller, controller);
                let frozen = trigger.context.zone_transition.as_ref().unwrap();
                assert_eq!(frozen.subject.before.object, before);
                assert!(frozen
                    .source_before
                    .attachment
                    .links
                    .iter()
                    .any(|link| link.source == source_before && link.target == before));
                assert_eq!(state.objects[&e].attachment_link(), None);
                let bytes = mtg_gto::public_projection::JointPublicNormalization::for_state(
                    &state, controller,
                )
                .unwrap()
                .encoding
                .clone();
                rules::check_state_based_actions(&mut state);
                if token {
                    assert!(!state.objects.contains_key(&c));
                }
                assert_eq!(state.stack.len(), 1);
                for restored in [
                    state.clone(),
                    {
                        let mut value: GameState =
                            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                        value.card_db = state.card_db.clone();
                        value
                    },
                    {
                        let mut value: GameState =
                            bincode::deserialize(&bincode::serialize(&state).unwrap()).unwrap();
                        value.card_db = state.card_db.clone();
                        value
                    },
                ] {
                    let normalization =
                        mtg_gto::public_projection::JointPublicNormalization::for_state(
                            &restored, controller,
                        )
                        .unwrap();
                    assert!(normalization
                        .diagnostics
                        .edges_by_type
                        .get(&mtg_gto::public_projection::EdgeKind::HistoricalAttached)
                        .is_some());
                }
                assert!(!bytes.is_empty());
                rules::apply_action(&mut state, &Action::PassPriority);
                rules::apply_action(&mut state, &Action::PassPriority);
                assert_eq!(state.players[controller].hand.len(), 1);
            }
        }
    }
}
#[test]
fn historical_predicate_rejects_unrelated_detached_noncreature_and_non_death() {
    for case in 0..4 {
        let (mut state, e, c) = equipped_watcher(false, 0, 1);
        let victim = if case == 0 {
            state.create_card_in_zone(1, 0, ZoneType::Battlefield)
        } else {
            c
        };
        if case == 1 {
            assert!(state.detach_exact(
                state.exact_object(e).unwrap(),
                state.exact_object(c).unwrap()
            ));
        }
        if case == 2 {
            state
                .continuous_effects
                .push(mtg_gto::layers::ContinuousEffect {
                    source_id: e,
                    controller: 0,
                    timestamp: 900,
                    duration: mtg_gto::layers::Duration::Permanent,
                    affected: AffectedObjects::Specific(c),
                    modification: mtg_gto::layers::LayerModification::RemoveType(
                        CardType::Creature,
                    ),
                });
            state.invalidate_characteristics_cache();
        }
        depart(
            &mut state,
            &[victim],
            if case == 3 {
                ZoneType::Exile
            } else {
                ZoneType::Graveyard
            },
        );
        assert!(state.pending_triggers.is_empty());
    }
}
#[test]
fn simultaneous_attachment_history_is_member_order_invariant() {
    let (mut base, e, c) = equipped_watcher(false, 1, 0);
    let c2 = base.create_card_in_zone(1, 1, ZoneType::Battlefield);
    let e2 = base.create_card_in_zone(2, 0, ZoneType::Battlefield);
    established(&mut base, e2, c2);
    let mut a = base.clone();
    let mut b = base;
    let first = depart(&mut a, &[c, e, e2, c2], ZoneType::Graveyard);
    let second = depart(&mut b, &[c2, e2, e, c], ZoneType::Graveyard);
    assert_eq!(first.transitions.len(), second.transitions.len());
    assert_eq!(a.pending_triggers.len(), 2);
    assert_eq!(b.pending_triggers.len(), 2);
    assert_eq!(info(&a).hash_value(), info(&b).hash_value());
    assert_eq!(
        mtg_gto::public_projection::JointPublicNormalization::for_state(&a, 0)
            .unwrap()
            .encoding
            .clone(),
        mtg_gto::public_projection::JointPublicNormalization::for_state(&b, 0)
            .unwrap()
            .encoding
            .clone()
    );
}
#[test]
fn established_aura_orphans_during_departure_and_moves_in_its_later_group() {
    let (mut state, _, c) = fixture();
    let aura = state.create_card_in_zone(3, 1, ZoneType::Battlefield);
    established(&mut state, aura, c);
    let batch = depart(&mut state, &[c], ZoneType::Graveyard);
    assert_eq!(state.objects[&aura].attachment_link(), None);
    assert!(state.battlefield.contains(&aura));
    assert_eq!(batch.transitions.len(), 1);
    rules::check_state_based_actions(&mut state);
    assert!(state.players[1].graveyard.contains(&aura));
    assert!(state.next_zone_event_group_id > batch.group_id);
    state.move_object(c, ZoneType::Graveyard, ZoneType::Battlefield);
    assert_eq!(state.objects[&aura].attachment_link(), None);
}
#[test]
fn multiple_sources_control_changes_and_shroud_do_not_detach() {
    let (mut state, e, c) = fixture();
    let e2 = state.create_card_in_zone(2, 1, ZoneType::Battlefield);
    let aura1 = state.create_card_in_zone(3, 0, ZoneType::Battlefield);
    let aura2 = state.create_card_in_zone(3, 1, ZoneType::Battlefield);
    for source in [e2, aura1, aura2] {
        established(&mut state, source, c);
    }
    state.objects.get_mut(&c).unwrap().controller = 1;
    state.objects.get_mut(&e).unwrap().controller = 1;
    state
        .objects
        .get_mut(&c)
        .unwrap()
        .temp_keywords
        .push(mtg_gto::card::KeywordAbility::Shroud);
    state.invalidate_characteristics_cache();
    rules::check_state_based_actions(&mut state);
    assert_eq!(
        state.attachments_of(state.exact_object(c).unwrap()).len(),
        4
    );
    assert_eq!(state.effective_power(c), 4);
    let next = state.create_card_in_zone(1, 0, ZoneType::Battlefield);
    established(&mut state, e2, next);
    assert_eq!(
        state.attachments_of(state.exact_object(c).unwrap()).len(),
        3
    );
    assert_eq!(
        state
            .attachments_of(state.exact_object(next).unwrap())
            .len(),
        1
    );
}
#[test]
fn action_only_incidence_merge_split_and_departure_rebuild() {
    use mtg_gto::public_projection::{CoordinateNamespace, JointPublicNormalization};
    let (mut state, e, c) = fixture();
    assert!(state.detach_exact(
        state.exact_object(e).unwrap(),
        state.exact_object(c).unwrap()
    ));
    let first = JointPublicNormalization::for_state(&state, 0).unwrap();
    // A represented AttachedTo effect already relates its source, while the
    // previously unconnected target occupies the action-only namespace.
    assert_eq!(
        first.exact_to_coordinate[&state.exact_object(c).unwrap()].namespace,
        CoordinateNamespace::ActionOnly
    );
    let key = canonical::canonicalize_actions(
        &[Action::Equip {
            equipment_id: e,
            target_id: c,
        }],
        &state,
        &first,
    )
    .unwrap()
    .remove(0);
    let action = canonical::resolve_with_normalization(&key, &state, 0, &first)
        .unwrap()
        .unwrap();
    drop(first);
    rules::apply_action(&mut state, &action);
    let linked = JointPublicNormalization::for_state(&state, 0).unwrap();
    assert_eq!(
        linked.exact_to_coordinate[&state.exact_object(c).unwrap()].namespace,
        CoordinateNamespace::Relational
    );
    let linked_bytes = linked.encoding.clone();
    drop(linked);
    let other = state.create_card_in_zone(1, 0, ZoneType::Battlefield);
    established(&mut state, e, other);
    let changed = JointPublicNormalization::for_state(&state, 0).unwrap();
    assert!(changed
        .exact_to_coordinate
        .contains_key(&state.exact_object(other).unwrap()));
    drop(changed);
    state.detach_exact(
        state.exact_object(e).unwrap(),
        state.exact_object(other).unwrap(),
    );
    assert_ne!(
        linked_bytes,
        JointPublicNormalization::for_state(&state, 0)
            .unwrap()
            .encoding
            .clone()
    );
    established(&mut state, e, c);
    depart(&mut state, &[c], ZoneType::Exile);
    assert_eq!(state.objects[&e].attachment_link(), None);
    JointPublicNormalization::for_state(&state, 0).unwrap();
}
#[test]
fn isolated_facts_do_not_expand_relationship_supplement() {
    use mtg_gto::public_projection::JointPublicNormalization;
    let (mut state, _, _) = fixture();
    let isolated = state.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let before = JointPublicNormalization::for_state(&state, 0)
        .unwrap()
        .encoding
        .clone();
    state.objects.get_mut(&isolated).unwrap().damage_marked = 1;
    state.invalidate_characteristics_cache();
    assert_eq!(
        before,
        JointPublicNormalization::for_state(&state, 0)
            .unwrap()
            .encoding
            .clone()
    );
}
#[test]
fn malformed_projection_is_read_only_and_mutable_entry_only_cleans_legal_orphans() {
    use mtg_gto::{
        card::{AttachmentKind, AttachmentLink},
        public_projection::JointPublicNormalization,
        simulation::TerminationReason,
    };
    let (base, e, c) = fixture();
    for case in 0..6 {
        let mut state = base.clone();
        let mut link = state.objects[&e].attachment_link().unwrap();
        match case {
            0 => link.source_generation += 1,
            1 => link.target.generation += 1,
            2 => {
                state.battlefield.retain(|&id| id != e);
                state.players[0].exile.push(e)
            }
            3 => {
                state.battlefield.retain(|&id| id != c);
                state.players[0].exile.push(c)
            }
            4 => state.battlefield.push(c),
            _ => link.kind = AttachmentKind::Aura,
        };
        state.set_malformed_attachment_fixture(e, Some(link));
        let before = bincode::serialize(&state).unwrap();
        assert!(matches!(
            JointPublicNormalization::for_state(&state, 0),
            Err(TerminationReason::StateEncoding)
        ));
        assert_eq!(before, bincode::serialize(&state).unwrap());
        assert!(!state.game_over);
        assert!(state.validate_attachment_state(true).is_err());
        if matches!(case, 0 | 1 | 5) {
            assert!(state
                .prepare_attach(
                    state.exact_object(e).unwrap(),
                    state.exact_object(c).unwrap(),
                    mtg_gto::card::AttachmentContext::Established
                )
                .is_err());
        }
    }
    let mut orphan = base;
    depart(&mut orphan, &[c], ZoneType::Exile);
    orphan.set_malformed_attachment_fixture(
        e,
        Some(AttachmentLink {
            source_generation: orphan.objects[&e].zone_change_count,
            target: ExactObjectRef {
                id: c,
                generation: 0,
            },
            kind: AttachmentKind::Equipment,
            timestamp: 1,
        }),
    );
    assert!(JointPublicNormalization::for_state(&orphan, 0).is_err());
    orphan.validate_attachment_state(true).unwrap();
    assert_eq!(orphan.objects[&e].attachment_link(), None);
    JointPublicNormalization::for_state(&orphan, 0).unwrap();
}

#[test]
fn raw_source_and_target_departure_and_token_source_cessation_are_single_lifecycle_events() {
    for source_leaves in [false, true] {
        let (mut state, e, c) = fixture();
        let original = state.exact_object(e).unwrap();
        let endpoint = if source_leaves { e } else { c };
        state.move_object(endpoint, ZoneType::Battlefield, ZoneType::Exile);
        assert_eq!(state.objects[&e].attachment_link(), None);
        assert_eq!(state.effective_power(c), 2);
        state.move_object(endpoint, ZoneType::Exile, ZoneType::Battlefield);
        assert_eq!(state.objects[&e].attachment_link(), None);
        if source_leaves {
            assert_ne!(state.exact_object(e).unwrap(), original);
        }
    }
    let (mut state, e, c) = equipped_watcher(false, 1, 0);
    state.objects.get_mut(&e).unwrap().is_token = true;
    depart(&mut state, &[e, c], ZoneType::Graveyard);
    let frozen = serde_json::to_value(&state.pending_triggers[0].context).unwrap();
    let groups = state.next_zone_event_group_id;
    assert_eq!(state.objects[&e].attachment_link(), None);
    rules::check_state_based_actions(&mut state);
    assert!(!state.objects.contains_key(&e));
    assert_eq!(state.next_zone_event_group_id, groups);
    assert_eq!(state.stack.len(), 1);
    let context = match &state.stack[0].source {
        mtg_gto::game::StackSource::TriggeredAbility { context, .. } => context,
        _ => panic!("trigger"),
    };
    assert_eq!(serde_json::to_value(context).unwrap(), frozen);
    let restored: GameState = bincode::deserialize(&bincode::serialize(&state).unwrap()).unwrap();
    let context = match &restored.stack[0].source {
        mtg_gto::game::StackSource::TriggeredAbility { context, .. } => context,
        _ => panic!("trigger"),
    };
    assert_eq!(serde_json::to_value(context).unwrap(), frozen);
}

#[test]
fn historical_frames_and_live_reattachment_share_only_exact_public_identity() {
    use mtg_gto::public_projection::{EdgeKind, JointPublicNormalization};
    let (mut state, e, c) = equipped_watcher(false, 0, 1);
    let old = state.exact_object(c).unwrap();
    let source = state.exact_object(e).unwrap();
    depart(&mut state, &[c], ZoneType::Graveyard);
    state.move_object(c, ZoneType::Graveyard, ZoneType::Battlefield);
    let newer = state.exact_object(c).unwrap();
    assert_ne!(old, newer);
    established(&mut state, e, c);
    let first = JointPublicNormalization::for_state(&state, 0).unwrap();
    assert_ne!(
        first.exact_to_coordinate[&old],
        first.exact_to_coordinate[&newer]
    );
    assert_eq!(first.diagnostics.edges_by_type[&EdgeKind::LiveAttached], 1);
    assert_eq!(
        first.diagnostics.edges_by_type[&EdgeKind::HistoricalAttached],
        1
    );
    assert!(first.exact_to_coordinate.contains_key(&source));
    drop(first);
    depart(&mut state, &[c], ZoneType::Graveyard);
    assert_eq!(state.pending_triggers.len(), 2);
    let second = JointPublicNormalization::for_state(&state, 0).unwrap();
    assert_eq!(
        second.diagnostics.edges_by_type[&EdgeKind::HistoricalAttached],
        2
    );
    assert_eq!(
        second.diagnostics.edges_by_type[&EdgeKind::OccurrenceSource],
        2
    );
    assert!(second.exact_to_coordinate.contains_key(&source));
    drop(second);
    // A newer hidden current incarnation never supplies an old frame's facts.
    state.move_object(c, ZoneType::Graveyard, ZoneType::Hand);
    let hidden = state.exact_object(c).unwrap();
    let normalization = JointPublicNormalization::for_state(&state, 1).unwrap();
    assert!(!normalization.exact_to_coordinate.contains_key(&hidden));
    assert!(normalization.exact_to_coordinate.contains_key(&old));
    let bytes = normalization.encoding.clone();
    drop(normalization);
    state.objects.get_mut(&c).unwrap().card_def_id = 3;
    state.objects.get_mut(&c).unwrap().zone_change_count += 20;
    state.invalidate_characteristics_cache();
    assert_eq!(
        bytes,
        JointPublicNormalization::for_state(&state, 1)
            .unwrap()
            .encoding
    );
}

#[test]
fn duplicate_owned_frames_cannot_disagree_about_attachment_inventory() {
    use mtg_gto::public_projection::JointPublicNormalization;
    let (mut state, _, c) = equipped_watcher(false, 0, 1);
    depart(&mut state, &[c], ZoneType::Graveyard);
    let mut duplicate = state.pending_triggers[0].clone();
    duplicate
        .context
        .zone_transition
        .as_mut()
        .unwrap()
        .source_before
        .attachment
        .links
        .clear();
    state.pending_triggers.push(duplicate);
    let before = bincode::serialize(&state).unwrap();
    assert!(JointPublicNormalization::for_state(&state, 0).is_err());
    assert_eq!(before, bincode::serialize(&state).unwrap());
}

#[test]
fn both_isolated_endpoints_move_to_relational_and_component_merges_rebuild() {
    use mtg_gto::public_projection::{CoordinateNamespace, JointPublicNormalization};
    let (mut state, e, c) = fixture();
    state.detach_exact(
        state.exact_object(e).unwrap(),
        state.exact_object(c).unwrap(),
    );
    let mut db = (*state.card_db.as_ref().unwrap().as_ref()).clone();
    let mut equipment = db.get(2).unwrap().clone();
    equipment.static_abilities.clear();
    db.insert(equipment);
    state.card_db = Some(Arc::new(db));
    state.continuous_effects.clear();
    state.refresh_continuous_effects();
    let e2 = state.create_card_in_zone(2, 0, ZoneType::Battlefield);
    let c2 = state.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let initial = JointPublicNormalization::for_state(&state, 0).unwrap();
    for id in [e, c, e2, c2] {
        assert_eq!(
            initial.exact_to_coordinate[&state.exact_object(id).unwrap()].namespace,
            CoordinateNamespace::ActionOnly
        );
    }
    let key = canonical::canonicalize_actions(
        &[Action::Equip {
            equipment_id: e,
            target_id: c,
        }],
        &state,
        &initial,
    )
    .unwrap()
    .remove(0);
    let action = canonical::resolve_with_normalization(&key, &state, 0, &initial)
        .unwrap()
        .unwrap();
    drop(initial);
    rules::apply_action(&mut state, &action);
    established(&mut state, e2, c2);
    let distinct = JointPublicNormalization::for_state(&state, 0).unwrap();
    assert_eq!(
        distinct.components.iter().filter(|c| c.relational).count(),
        2
    );
    drop(distinct);
    established(&mut state, e, c2);
    let shared = JointPublicNormalization::for_state(&state, 0).unwrap();
    assert_eq!(shared.components.iter().filter(|c| c.relational).count(), 1);
    assert_eq!(
        shared.exact_to_coordinate[&state.exact_object(c).unwrap()].namespace,
        CoordinateNamespace::ActionOnly
    );
    drop(shared);
    state.detach_exact(
        state.exact_object(e).unwrap(),
        state.exact_object(c2).unwrap(),
    );
    let detached = JointPublicNormalization::for_state(&state, 0).unwrap();
    assert_eq!(
        detached.exact_to_coordinate[&state.exact_object(e).unwrap()].namespace,
        CoordinateNamespace::ActionOnly
    );
}

#[test]
fn structural_sba_failure_stops_as_state_encoding_without_loss_or_mutation() {
    let (mut state, e, _) = fixture();
    let mut link = state.objects[&e].attachment_link().unwrap();
    link.source_generation += 1;
    state.set_malformed_attachment_fixture(e, Some(link));
    let before = serde_json::to_value(&state).unwrap();
    rules::check_state_based_actions(&mut state);
    assert_eq!(
        state.invalid_gameplay_reason(),
        Some(mtg_gto::simulation::TerminationReason::StateEncoding)
    );
    assert!(!state.game_over);
    assert_eq!(state.objects[&e].attachment_link(), Some(link));
    state.sba_failure = None;
    assert_eq!(before, serde_json::to_value(&state).unwrap());
}

#[test]
fn encoding_failure_reaches_actions_observations_policy_search_and_counted_application() {
    use mtg_gto::{
        info_set::BucketedAbstraction,
        simulation::TerminationReason,
        solver::{
            mcts::{MctsConfig, MctsStrategy},
            RegretTable,
        },
        strategy::{AbstractedMcfrStrategy, McfrStrategy, Strategy},
    };
    let (mut state, e, c) = fixture();
    let key = canonical::canonicalize(
        &Action::Equip {
            equipment_id: e,
            target_id: c,
        },
        &state,
    )
    .unwrap();
    let mut link = state.objects[&e].attachment_link().unwrap();
    link.target.generation += 1;
    state.set_malformed_attachment_fixture(e, Some(link));
    let before = bincode::serialize(&state).unwrap();
    let failure = TerminationReason::StateEncoding;
    assert_eq!(
        canonical::canonicalize(&Action::PassPriority, &state),
        Err(failure)
    );
    assert_eq!(canonical::resolve(&key, &state, 0), Err(failure));
    assert!(
        matches!(InformationSet::from_view(&state.visible_state(0),state.card_db()),Err(reason) if reason==failure)
    );
    let policies: Vec<Box<dyn Strategy>> = vec![
        Box::new(McfrStrategy::new(RegretTable::new())),
        Box::new(AbstractedMcfrStrategy::new(
            RegretTable::new(),
            Box::new(BucketedAbstraction),
        )),
        Box::new(MctsStrategy::new(MctsConfig {
            iterations_per_move: 1,
            ..Default::default()
        })),
    ];
    for policy in policies {
        assert_eq!(policy.choose_action(&state, 0), Err(failure));
    }
    let legal = mtg_gto::action::legal_actions(&state);
    assert_eq!(
        mtg_gto::simulation::apply_counted_action(&mut state, &Action::PassPriority, &legal),
        Err(failure)
    );
    assert_eq!(before, bincode::serialize(&state).unwrap());
    assert!(!state.game_over);
}

#[test]
fn solver_checkpoint_rejects_old_and_unversioned_semantic_keys_explicitly() {
    use mtg_gto::solver::{
        mccfr::{load_checkpoint, save_checkpoint},
        RegretTable,
    };
    let folder =
        std::env::temp_dir().join(format!("mtg2d1-table-checkpoint-{}", std::process::id()));
    let path = folder.to_str().unwrap();
    let tables = [RegretTable::new(), RegretTable::new()];
    save_checkpoint(&tables, path, 7).unwrap();
    let current = std::fs::read(folder.join("player_0_iter_7.bin")).unwrap();
    assert!(load_checkpoint(path, 7).is_ok());
    for legacy in [
        bincode::serialize(&std::collections::HashMap::<
            u64,
            mtg_gto::solver::InfoSetData,
        >::new())
        .unwrap(),
        {
            let mut bytes = current.clone();
            let index = bytes
                .windows(mtg_gto::solver::SEMANTIC_FORMAT.len())
                .position(|s| s == mtg_gto::solver::SEMANTIC_FORMAT.as_bytes())
                .unwrap();
            bytes[index] = b'X';
            bytes
        },
    ] {
        std::fs::write(folder.join("player_0_iter_7.bin"), legacy).unwrap();
        let error = load_checkpoint(path, 7).unwrap_err();
        assert!(error.contains("incompatible or unversioned solver semantic format"));
        std::fs::write(folder.join("player_0_iter_7.bin"), &current).unwrap();
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn exact_relationship_state_roundtrips_at_live_departed_and_returned_decisions() {
    use mtg_gto::public_projection::JointPublicNormalization;
    let (mut state, e, c) = equipped_watcher(false, 0, 1);
    for stage in 0..4 {
        let expected = JointPublicNormalization::for_state(&state, 0)
            .unwrap()
            .encoding
            .clone();
        let mut snapshot = state.clone();
        snapshot.restore(state.snapshot()).unwrap();
        let mut json: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        json.card_db = state.card_db.clone();
        let mut binary: GameState =
            bincode::deserialize(&bincode::serialize(&state).unwrap()).unwrap();
        binary.card_db = state.card_db.clone();
        for mut restored in [state.clone(), snapshot, json, binary] {
            restored.validate_attachment_state(true).unwrap();
            assert_eq!(
                expected,
                JointPublicNormalization::for_state(&restored, 0)
                    .unwrap()
                    .encoding
            );
            assert_eq!(
                restored.attachment_target(restored.exact_object(e).unwrap()),
                state.attachment_target(state.exact_object(e).unwrap())
            );
            assert_eq!(restored.priority_player, state.priority_player);
        }
        match stage {
            0 => {
                depart(&mut state, &[c], ZoneType::Graveyard);
            }
            1 => {
                state.move_object(c, ZoneType::Graveyard, ZoneType::Battlefield);
                established(&mut state, e, c);
            }
            2 => {
                depart(&mut state, &[e, c], ZoneType::Graveyard);
            }
            _ => {}
        }
    }
}

#[test]
fn suspended_owned_occurrence_uses_joint_frame_and_visible_current_source() {
    use mtg_gto::game::{PendingCopyOrder, StackEntry, StackSource};
    use mtg_gto::public_projection::JointPublicNormalization;
    let (mut state, e, c) = equipped_watcher(false, 0, 1);
    depart(&mut state, &[c], ZoneType::Graveyard);
    let trigger = state.pending_triggers.remove(0);
    let entry = StackEntry {
        id: 77,
        source: StackSource::TriggeredAbility {
            source_id: e,
            ability_index: trigger.ability_index,
            context: Box::new(trigger.context),
        },
        controller: 0,
        targets: vec![],
        target_generations: vec![],
    };
    // Bounded representation fixture for the existing suspended operation;
    // it does not add or execute a copying instruction or new ability.
    let operation:PendingCopyOrder=serde_json::from_value(serde_json::json!({"controller":0,"items":[],"selected_order":[],"expected_stack_len":0,"expected_next_stack_id":state.next_stack_id,"resolving_entry":entry,"resolving_source_generation":null})).unwrap();
    state.pending_copy_order = Some(operation);
    let normalization = JointPublicNormalization::for_state(&state, 0).unwrap();
    let occurrence = normalization.resolving_occurrence.as_ref().unwrap();
    assert!(occurrence.source.as_ref().unwrap().live.is_some());
    assert!(occurrence.decision_coordinate.is_some());
    let info = InformationSet::from_view_with_normalization(
        &state.visible_state(0),
        state.card_db(),
        &normalization,
    )
    .unwrap();
    assert_eq!(
        info.pending_copy_order
            .as_ref()
            .unwrap()
            .resolving_entry
            .as_ref()
            .unwrap()
            .zone_occurrence
            .as_ref(),
        Some(occurrence)
    );
}
