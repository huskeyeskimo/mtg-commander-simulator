use mtg_gto::{
    action::{canonical, legal_actions, Action},
    card::{
        CardDef, CardType, Effect, EnchantRestriction, KeywordAbility, ManaAbility, Subtype,
        TriggerCondition, TriggeredAbility, ZoneType,
    },
    game::{CardDatabase, GameState, Phase, StackSource, Target},
    info_set::InformationSet,
    layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification, StaticAbility},
    mana::ManaCost,
    rules,
    rules::transitions::{transition_batch, MovementKind, TransitionError, TransitionRequest},
    simulation::TerminationReason,
};
use std::sync::Arc;
const BODY: u64 = 970101;
const AURA: u64 = 970102;
const LAND: u64 = 970103;
fn fixture() -> (GameState, u64, u64) {
    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: BODY,
        name: "Body".into(),
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        ..Default::default()
    });
    db.insert(CardDef {
        id: AURA,
        name: "Synthetic Aura".into(),
        card_types: vec![CardType::Enchantment],
        subtypes: vec![Subtype("Aura".into())],
        enchant: EnchantRestriction::Creature,
        mana_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        static_abilities: vec![StaticAbility::Anthem {
            power: 1,
            toughness: 1,
            affected: AffectedObjects::AttachedTo,
        }],
        ..Default::default()
    });
    db.insert(CardDef {
        id: LAND,
        name: "Land".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColorless],
        ..Default::default()
    });
    let mut s = GameState::new(2);
    s.card_db = Some(Arc::new(db));
    s.phase = Phase::PreCombatMain;
    let a = s.create_card_in_zone(AURA, 0, ZoneType::Hand).unwrap();
    let t = s
        .create_card_in_zone(BODY, 1, ZoneType::Battlefield)
        .unwrap();
    s.players[0].mana_pool.colorless = 4;
    (s, a, t)
}
fn cast(s: &mut GameState, a: u64, t: u64) {
    rules::apply_action(
        s,
        &Action::CastSpell {
            object_id: a,
            targets: vec![Target::Object(t)],
        },
    );
    assert!(s
        .stack
        .iter()
        .any(|e| matches!(e.source,StackSource::AuraSpell{source,..}if source.id==a)));
}
fn resolve(s: &mut GameState) {
    for _ in 0..s.active_player_count() {
        rules::apply_action(s, &Action::PassPriority);
    }
}
fn change(s: &mut GameState, id: u64, modification: LayerModification) {
    s.continuous_effects.push(ContinuousEffect {
        source_id: id,
        controller: 0,
        timestamp: s.next_timestamp,
        duration: Duration::Permanent,
        affected: AffectedObjects::Specific(id),
        modification,
    });
    s.next_timestamp += 1;
    s.invalidate_characteristics_cache();
}
fn depart(s: &mut GameState, id: u64, to: ZoneType) {
    let exact = s.exact_object(id).unwrap();
    transition_batch(
        s,
        &[TransitionRequest {
            object: exact,
            from: ZoneType::Battlefield,
            to,
            kind: MovementKind::Put,
        }],
    )
    .unwrap();
}
fn failure_only(s: &GameState, before: &GameState) {
    let mut copy = s.clone();
    copy.sba_failure = before.sba_failure.clone();
    assert_eq!(
        bincode::serialize(&copy).unwrap(),
        bincode::serialize(before).unwrap()
    );
}
fn restores(s: &GameState) -> Vec<GameState> {
    let mut json: GameState = serde_json::from_str(&serde_json::to_string(s).unwrap()).unwrap();
    json.card_db = s.card_db.clone();
    let mut bin: GameState = bincode::deserialize(&bincode::serialize(s).unwrap()).unwrap();
    bin.card_db = s.card_db.clone();
    let mut snap = s.clone();
    snap.restore(s.snapshot()).unwrap();
    vec![s.clone(), json, bin, snap]
}
#[test]
fn casting_selects_one_target_pays_once_and_waits_for_priority() {
    let (mut s, a, t) = fixture();
    let casts: Vec<_> = legal_actions(&s)
        .into_iter()
        .filter(|x| matches!(x,Action::CastSpell{object_id,..}if *object_id==a))
        .collect();
    assert_eq!(
        casts,
        vec![Action::CastSpell {
            object_id: a,
            targets: vec![Target::Object(t)]
        }]
    );
    let source = s.exact_object(a).unwrap();
    let target = s.exact_object(t).unwrap();
    cast(&mut s, a, t);
    assert_eq!(s.players[0].mana_pool.colorless, 3);
    assert!(!s.battlefield.contains(&a));
    assert!(s.objects[&a].attachment_link().is_none());
    assert_eq!(s.effective_power(t), 2);
    match s.stack[0].source {
        StackSource::AuraSpell {
            source: exact,
            target: stored,
            ..
        } => {
            assert_eq!(exact.generation, source.generation + 1);
            assert_eq!(stored, target)
        }
        _ => panic!(),
    };
    rules::apply_action(&mut s, &Action::PassPriority);
    assert_eq!(s.stack.len(), 1);
    rules::apply_action(&mut s, &Action::PassPriority);
    assert_eq!(s.players[0].mana_pool.colorless, 3);
    assert_eq!(
        s.attachment_target(s.exact_object(a).unwrap()),
        Some(target)
    );
    assert_eq!(s.effective_power(t), 3);
    assert_eq!(s.priority_player, s.active_player);
}
#[test]
fn red_a_and_g_missing_extra_wrong_target_and_timing_are_atomic() {
    for case in 0..6 {
        let (mut s, a, t) = fixture();
        let targets = match case {
            0 => vec![],
            1 => vec![Target::Object(t), Target::Object(t)],
            2 => vec![Target::Player(1)],
            3 => vec![Target::Object(123456)],
            _ => vec![Target::Object(t)],
        };
        if case == 4 {
            s.phase = Phase::Upkeep;
        }
        if case == 5 {
            s.priority_player = 1;
            s.players[1].hand.push(a);
            s.players[0].hand.clear();
            s.objects.get_mut(&a).unwrap().owner = 1;
        }
        let before = s.clone();
        rules::apply_action(
            &mut s,
            &Action::CastSpell {
                object_id: a,
                targets,
            },
        );
        assert_eq!(
            bincode::serialize(&s).unwrap(),
            bincode::serialize(&before).unwrap()
        );
    }
}
#[test]
fn flash_allows_response_timing() {
    let (mut s, a, t) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap())
        .cards
        .get_mut(&AURA)
        .unwrap()
        .keywords
        .push(KeywordAbility::Flash);
    s.phase = Phase::Upkeep;
    cast(&mut s, a, t);
    resolve(&mut s);
    assert!(s.battlefield.contains(&a));
}
#[test]
fn initial_shroud_control_and_opposing_hexproof_reject_without_payment() {
    for keyword in [KeywordAbility::Shroud, KeywordAbility::Hexproof] {
        let (mut s, a, t) = fixture();
        s.objects.get_mut(&t).unwrap().temp_keywords.push(keyword);
        s.invalidate_characteristics_cache();
        let before = s.clone();
        assert!(!legal_actions(&s)
            .iter()
            .any(|x| matches!(x,Action::CastSpell{object_id,..}if *object_id==a)));
        rules::apply_action(
            &mut s,
            &Action::CastSpell {
                object_id: a,
                targets: vec![Target::Object(t)],
            },
        );
        assert_eq!(
            bincode::serialize(&s).unwrap(),
            bincode::serialize(&before).unwrap()
        );
    }
}
#[test]
fn incomplete_payment_does_not_tap_partial_resources() {
    let (mut s, a, t) = fixture();
    s.players[0].mana_pool.colorless = 0;
    Arc::make_mut(s.card_db.as_mut().unwrap())
        .cards
        .get_mut(&AURA)
        .unwrap()
        .mana_cost = Some(ManaCost::new(2, 0, 0, 0, 0, 0));
    let land = s
        .create_card_in_zone(LAND, 0, ZoneType::Battlefield)
        .unwrap();
    let before = s.clone();
    rules::apply_action(
        &mut s,
        &Action::CastSpell {
            object_id: a,
            targets: vec![Target::Object(t)],
        },
    );
    assert_eq!(
        bincode::serialize(&s).unwrap(),
        bincode::serialize(&before).unwrap()
    );
    assert!(!s.objects[&land].tapped);
}
#[test]
fn auto_tap_payment_is_once() {
    let (mut s, a, t) = fixture();
    s.players[0].mana_pool.colorless = 0;
    let land = s
        .create_card_in_zone(LAND, 0, ZoneType::Battlefield)
        .unwrap();
    cast(&mut s, a, t);
    assert!(s.objects[&land].tapped);
    resolve(&mut s);
    assert_eq!(s.players[0].mana_pool.colorless, 0);
    assert!(s.battlefield.contains(&a));
}
#[test]
fn red_b_c_d_e_f_illegal_resolution_never_enters_or_triggers() {
    for case in 0..5 {
        let (mut s, a, t) = fixture();
        Arc::make_mut(s.card_db.as_mut().unwrap())
            .cards
            .get_mut(&AURA)
            .unwrap()
            .triggered_abilities
            .push(TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::GainLife { amount: 9 },
                description: "ETB".into(),
            });
        cast(&mut s, a, t);
        match case {
            0 => {
                depart(&mut s, t, ZoneType::Exile);
                s.move_object(t, ZoneType::Exile, ZoneType::Battlefield)
            }
            1 => depart(&mut s, t, ZoneType::Graveyard),
            2 => change(
                &mut s,
                t,
                LayerModification::AddKeyword(KeywordAbility::Shroud),
            ),
            3 => change(
                &mut s,
                t,
                LayerModification::AddKeyword(KeywordAbility::Hexproof),
            ),
            _ => change(
                &mut s,
                t,
                LayerModification::SetTypes(vec![CardType::Artifact]),
            ),
        };
        let events = s.pending_events.len();
        resolve(&mut s);
        assert!(s.players[0].graveyard.contains(&a));
        assert!(!s.battlefield.contains(&a));
        assert!(s.objects[&a].attachment_link().is_none());
        assert!(s.stack.is_empty());
        assert_eq!(s.players[0].life, 20);
        assert!(s.sba_failure.is_none());
        assert!(!s.pending_events[events..].iter().any(|e|matches!(e,mtg_gto::events::GameEvent::ZoneChange{object,to:mtg_gto::events::Zone::Battlefield,..}if *object==a)));
    }
}
#[test]
fn controller_changes_use_current_hexproof_and_unrestricted_enchant() {
    for protected in [false, true] {
        let (mut s, a, t) = fixture();
        s.objects.get_mut(&t).unwrap().controller = 0;
        s.invalidate_characteristics_cache();
        if protected {
            change(
                &mut s,
                t,
                LayerModification::AddKeyword(KeywordAbility::Hexproof),
            );
        }
        cast(&mut s, a, t);
        s.objects.get_mut(&t).unwrap().controller = 1;
        s.invalidate_characteristics_cache();
        resolve(&mut s);
        assert_eq!(s.battlefield.contains(&a), !protected);
        if !protected {
            assert_eq!(s.objects[&a].owner, 0);
            assert_eq!(s.objects[&a].controller, 0);
        }
    }
}
#[test]
fn red_i_malformed_pending_shape_is_encoding_and_pass_atomic() {
    for case in 0..13 {
        let (mut s, a, t) = fixture();
        cast(&mut s, a, t);
        match case {
            0 => {
                s.objects.remove(&a);
            }
            1 => s.players[0].hand.push(a),
            2 => s.objects.get_mut(&a).unwrap().zone_change_count += 1,
            3 => s.stack[0].targets.clear(),
            4 => s.stack[0].target_generations.clear(),
            5 => s.stack[0].controller = 2,
            6 => {
                if let StackSource::AuraSpell { source_card_id, .. } = &mut s.stack[0].source {
                    *source_card_id = BODY
                }
            }
            7 => {
                if let StackSource::AuraSpell { restriction, .. } = &mut s.stack[0].source {
                    *restriction = EnchantRestriction::Unsupported
                }
            }
            8 => s.battlefield.push(t),
            9 => s.objects.get_mut(&t).unwrap().zone_change_count = 0,
            10 => {
                s.objects.remove(&t);
            }
            12 => s.card_db = None,
            _ => {
                s.objects.get_mut(&a).unwrap().zone_change_count += 1;
                s.objects.get_mut(&t).unwrap().zone_change_count += 1;
            }
        };
        if case == 9 {
            if let StackSource::AuraSpell { target, .. } = &mut s.stack[0].source {
                target.generation = 5
            }
        }
        let before = s.clone();
        rules::apply_action(&mut s, &Action::PassPriority);
        assert_eq!(
            s.invalid_gameplay_reason(),
            Some(TerminationReason::StateEncoding)
        );
        failure_only(&s, &before);
    }
}
#[test]
fn preparation_capacity_errors_preserve_entire_priority_boundary() {
    for case in 0..6 {
        let (mut s, a, t) = fixture();
        cast(&mut s, a, t);
        rules::apply_action(&mut s, &Action::PassPriority);
        match case {
            0 => s.next_timestamp = u32::MAX,
            1 => s.next_zone_event_group_id = u64::MAX,
            2 => {
                s.objects.get_mut(&a).unwrap().zone_change_count = u32::MAX;
                if let StackSource::AuraSpell { source, .. } = &mut s.stack[0].source {
                    source.generation = u32::MAX
                }
            }
            3 => s.objects.get_mut(&a).unwrap().owner = 2,
            4 => s.objects.get_mut(&t).unwrap().controller = 2,
            _ => {
                for amount in [i32::MAX, 1] {
                    s.replacement_effects
                        .push(mtg_gto::replacement::ReplacementEffect {
                            source_id: t,
                            controller: 0,
                            applies_to:
                                mtg_gto::replacement::ReplacementEventKind::EntersBattlefield,
                            action: mtg_gto::replacement::ReplacementAction::EntersModified {
                                enters_tapped: false,
                                extra_counters: amount,
                            },
                            is_self_replacement: true,
                            description: "overflow".into(),
                        });
                }
            }
        }
        let before = s.clone();
        rules::apply_action(&mut s, &Action::PassPriority);
        assert_eq!(
            s.invalid_gameplay_reason(),
            Some(TerminationReason::StateEncoding)
        );
        failure_only(&s, &before);
    }
}
#[test]
fn unsupported_metadata_and_cast_origins_are_explicit() {
    for case in 0..4 {
        let (mut s, a, t) = fixture();
        let action = match case {
            0 | 1 => {
                Arc::make_mut(s.card_db.as_mut().unwrap())
                    .cards
                    .get_mut(&AURA)
                    .unwrap()
                    .enchant = if case == 0 {
                    EnchantRestriction::Missing
                } else {
                    EnchantRestriction::Unsupported
                };
                Action::CastSpell {
                    object_id: a,
                    targets: vec![Target::Object(t)],
                }
            }
            2 => {
                s.move_object(a, ZoneType::Hand, ZoneType::Graveyard);
                Action::CastFromGraveyard {
                    object_id: a,
                    targets: vec![Target::Object(t)],
                }
            }
            _ => {
                s.move_object(a, ZoneType::Hand, ZoneType::Command);
                Action::CastCommander {
                    object_id: a,
                    targets: vec![Target::Object(t)],
                }
            }
        };
        let before = s.clone();
        rules::apply_action(&mut s, &action);
        assert_eq!(
            s.invalid_gameplay_reason(),
            Some(TerminationReason::PreparedSbaFailure)
        );
        assert_eq!(
            s.sba_failure,
            Some(rules::sba::PreparedPassFailure::Transition(
                TransitionError::UnsupportedPath
            ))
        );
        failure_only(&s, &before);
    }
}
#[test]
fn red_h_computed_enchant_sba_and_protection_after_attachment() {
    for case in 0..7 {
        let (mut s, a, t) = fixture();
        cast(&mut s, a, t);
        resolve(&mut s);
        match case {
            0 => change(
                &mut s,
                t,
                LayerModification::SetTypes(vec![CardType::Artifact]),
            ),
            1 => change(
                &mut s,
                t,
                LayerModification::AddKeyword(KeywordAbility::Shroud),
            ),
            2 => change(
                &mut s,
                t,
                LayerModification::AddKeyword(KeywordAbility::Hexproof),
            ),
            3 => change(
                &mut s,
                a,
                LayerModification::RemoveSubtype(Subtype("Aura".into())),
            ),
            4 => {
                change(&mut s, a, LayerModification::AddType(CardType::Creature));
                change(&mut s, a, LayerModification::SetPT(2, 2));
            }
            5 => {
                change(&mut s, a, LayerModification::RemoveAllAbilities);
                change(
                    &mut s,
                    t,
                    LayerModification::SetTypes(vec![CardType::Artifact]),
                );
            }
            _ => {
                s.objects.get_mut(&t).unwrap().controller = 0;
                s.invalidate_characteristics_cache();
            }
        }
        rules::check_state_based_actions(&mut s);
        assert!(s.sba_failure.is_none());
        match case {
            0 | 4 => assert!(s.players[0].graveyard.contains(&a)),
            3 => {
                assert!(s.battlefield.contains(&a));
                assert!(s.objects[&a].attachment_link().is_none());
            }
            _ => {
                assert!(s.battlefield.contains(&a));
                assert_eq!(
                    s.attachment_target(s.exact_object(a).unwrap()),
                    s.exact_object(t)
                );
            }
        }
    }
}
#[test]
fn own_type_changing_effect_is_not_circular() {
    let (mut s, a, t) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap())
        .cards
        .get_mut(&AURA)
        .unwrap()
        .static_abilities = vec![StaticAbility::AddType {
        card_type: CardType::Creature,
        affected: AffectedObjects::AttachedTo,
    }];
    change(&mut s, t, LayerModification::RemoveType(CardType::Creature));
    change(&mut s, t, LayerModification::AddType(CardType::Creature));
    cast(&mut s, a, t);
    resolve(&mut s);
    s.continuous_effects.retain(|effect| {
        !(effect.source_id == t
            && effect.modification == LayerModification::AddType(CardType::Creature))
    });
    s.invalidate_characteristics_cache();
    rules::check_state_based_actions(&mut s);
    assert!(s.battlefield.contains(&a));
    assert!(s.is_creature(t));
    assert!(s.sba_failure.is_none());
}
#[test]
fn red_j_raw_noncast_creation_and_movement_are_atomic() {
    let (mut s, a, _) = fixture();
    let before = bincode::serialize(&s).unwrap();
    assert_eq!(
        s.create_card_in_zone(AURA, 0, ZoneType::Battlefield),
        Err(TransitionError::UnsupportedPath)
    );
    assert_eq!(bincode::serialize(&s).unwrap(), before);
    assert_eq!(
        s.try_move_object(a, ZoneType::Hand, ZoneType::Battlefield),
        Err(TransitionError::UnsupportedPath)
    );
    assert_eq!(bincode::serialize(&s).unwrap(), before);
    assert_eq!(
        s.try_move_object(a, ZoneType::Graveyard, ZoneType::Battlefield),
        Err(TransitionError::WrongZone(a))
    );
    assert_eq!(bincode::serialize(&s).unwrap(), before);
    let baseline = s.clone();
    s.move_object(a, ZoneType::Hand, ZoneType::Battlefield);
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::PreparedSbaFailure)
    );
    failure_only(&s, &baseline);
}
#[test]
fn ordinary_creation_preserves_identity_zones_and_roundtrips() {
    let (mut s, _, _) = fixture();
    for zone in [
        ZoneType::Hand,
        ZoneType::Library,
        ZoneType::Battlefield,
        ZoneType::Graveyard,
        ZoneType::Exile,
        ZoneType::Command,
    ] {
        let id = s.next_object_id;
        let events = s.pending_events.len();
        let stamp = s.next_timestamp;
        assert_eq!(s.create_card_in_zone(BODY, 1, zone), Ok(id));
        assert_eq!(s.next_object_id, id + 1);
        let inst = &s.objects[&id];
        assert_eq!(
            (
                inst.owner,
                inst.controller,
                inst.zone_change_count,
                inst.is_token
            ),
            (1, 1, 0, false)
        );
        assert_eq!(s.pending_events.len(), events);
        assert_eq!(s.next_timestamp, stamp);
        for copy in restores(&s) {
            assert_eq!(
                serde_json::to_value(&copy).unwrap(),
                serde_json::to_value(&s).unwrap()
            );
        }
    }
}
#[test]
fn multiple_aura_modifiers_and_later_orphan_history() {
    let (mut s, a, t) = fixture();
    let b = s.create_card_in_zone(AURA, 1, ZoneType::Hand).unwrap();
    s.objects.get_mut(&b).unwrap().owner = 0;
    s.players[1].hand.clear();
    s.players[0].hand.push(b);
    cast(&mut s, a, t);
    resolve(&mut s);
    cast(&mut s, b, t);
    resolve(&mut s);
    assert_eq!(s.effective_power(t), 4);
    let links = s.attachments_of(s.exact_object(t).unwrap());
    assert_eq!(links.len(), 2);
    depart(&mut s, t, ZoneType::Graveyard);
    let group = s.next_zone_event_group_id;
    assert!(s.battlefield.contains(&a) && s.battlefield.contains(&b));
    assert!(s.objects[&a].attachment_link().is_none());
    rules::check_state_based_actions(&mut s);
    assert!(s.players[0].graveyard.contains(&a) && s.players[0].graveyard.contains(&b));
    assert!(s.next_zone_event_group_id > group);
}
#[test]
fn simultaneous_departures_capture_pairing_and_token_cessation_is_once() {
    for token in [false, true] {
        let (mut s, a, t) = fixture();
        s.objects.get_mut(&t).unwrap().is_token = token;
        cast(&mut s, a, t);
        resolve(&mut s);
        let subjects = [a, t].map(|id| TransitionRequest {
            object: s.exact_object(id).unwrap(),
            from: ZoneType::Battlefield,
            to: ZoneType::Graveyard,
            kind: MovementKind::Put,
        });
        let batch = transition_batch(&mut s, &subjects).unwrap();
        assert_eq!(batch.transitions.len(), 2);
        for moved in &batch.transitions {
            assert_eq!(moved.before.attachment.links.len(), 1);
        }
        let events = s.pending_events.len();
        rules::check_state_based_actions(&mut s);
        assert_eq!(s.pending_events.len(), events);
        assert_eq!(s.objects.contains_key(&t), !token);
        assert!(s.players[0].graveyard.contains(&a));
    }
}
#[test]
fn pending_and_attached_continuations_roundtrip_exactly() {
    for boundary in 0..6 {
        let (mut s, a, t) = fixture();
        if boundary > 0 {
            cast(&mut s, a, t);
        }
        if boundary == 2 {
            change(
                &mut s,
                t,
                LayerModification::AddKeyword(KeywordAbility::Shroud),
            );
        }
        if boundary == 3 {
            depart(&mut s, t, ZoneType::Exile);
            s.move_object(t, ZoneType::Exile, ZoneType::Battlefield);
        }
        if boundary >= 4 {
            resolve(&mut s);
        }
        if boundary == 5 {
            depart(&mut s, t, ZoneType::Graveyard);
        }
        let mut outcomes = Vec::new();
        for mut copy in restores(&s) {
            if boundary == 0 {
                cast(&mut copy, a, t);
            }
            if !copy.stack.is_empty() {
                resolve(&mut copy);
            }
            rules::check_state_based_actions(&mut copy);
            outcomes.push(serde_json::to_value(&copy).unwrap());
        }
        assert!(outcomes.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
#[test]
fn action_and_pending_projection_preserve_allocation_and_generation_invariance() {
    let (mut left, a, t) = fixture();
    let (mut right, b, u) = fixture();
    right.next_object_id += 40;
    for id in [b, u] {
        right.objects.get_mut(&id).unwrap().zone_change_count = 7;
    }
    for stage in 0..3 {
        if stage == 1 {
            cast(&mut left, a, t);
            cast(&mut right, b, u);
        }
        if stage == 2 {
            resolve(&mut left);
            resolve(&mut right);
        }
        for player in 0..2 {
            let l = InformationSet::from_view(&left.visible_state(player), left.card_db()).unwrap();
            let r =
                InformationSet::from_view(&right.visible_state(player), right.card_db()).unwrap();
            assert_eq!(l.hash_value(), r.hash_value());
        }
        let actions = legal_actions(&left);
        let mut keys = Vec::new();
        for action in actions {
            let key = canonical::canonicalize(&action, &left).unwrap();
            let rebuilt = canonical::resolve(&key, &right, 0).unwrap().unwrap();
            assert_eq!(canonical::canonicalize(&rebuilt, &right).unwrap(), key);
            keys.push(key);
        }
        assert_eq!(keys.len(), legal_actions(&right).len());
    }
}

#[cfg(feature = "scryfall")]
#[test]
fn importer_accepts_only_one_complete_enchant_creature_clause() {
    use mtg_gto::scryfall::parse_enchant_restriction as parse;
    for text in [
        "Enchant creature",
        "Enchant creature.",
        "Flash.\nEnchant creature.\nEnchanted creature gets +1/+1.",
    ] {
        assert_eq!(parse(text), EnchantRestriction::Creature);
    }
    for text in [
        "Enchant creature you control.",
        "Enchant permanent.",
        "Enchant artifact",
        "Enchant land",
        "Enchant planeswalker",
        "Enchant creature or planeswalker",
        "Enchant creature. Enchant creature.",
        "Enchant creature\nEnchant artifact",
        "Enchant creature with flying",
        "Enchant creature; draw a card",
        "Enchant",
    ] {
        assert_eq!(parse(text), EnchantRestriction::Unsupported, "{text}");
    }
    for text in [
        "",
        "Enchanted creature gets +1/+1.",
        "Target creature gets +1/+1.",
    ] {
        assert_eq!(parse(text), EnchantRestriction::Missing);
    }
}

#[test]
fn metadata_roundtrips_missing_supported_and_unsupported_without_inference() {
    for restriction in [
        EnchantRestriction::Missing,
        EnchantRestriction::Creature,
        EnchantRestriction::Unsupported,
    ] {
        let (s, _, _) = fixture();
        let mut def = s.card_db().get(AURA).unwrap().clone();
        def.enchant = restriction;
        let json: CardDef = serde_json::from_str(&serde_json::to_string(&def).unwrap()).unwrap();
        let bin: CardDef = bincode::deserialize(&bincode::serialize(&def).unwrap()).unwrap();
        assert_eq!(json.enchant, restriction);
        assert_eq!(bin.enchant, restriction);
        assert_eq!(
            mtg_gto::targeting::spell_targeting(&def),
            if restriction == EnchantRestriction::Creature {
                mtg_gto::targeting::SpellTargeting::Single(mtg_gto::card::TargetSpec::AnyCreature)
            } else {
                mtg_gto::targeting::SpellTargeting::Unsupported
            }
        );
    }
}

#[test]
fn noncast_tutor_continuation_and_token_definition_rejection_propagate() {
    for family in 0..3 {
        let (mut s, a, _) = fixture();
        let effect = if family == 2 {
            Effect::CreateTokenFromDef { card_def_id: AURA }
        } else {
            Effect::SearchLibrary {
                destination: ZoneType::Battlefield,
                subtype_filter: vec![],
            }
        };
        let actor = 970110;
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
            id: actor,
            name: "Ingress ability".into(),
            card_types: vec![CardType::Artifact],
            activated_abilities: vec![mtg_gto::card::ActivatedAbility {
                cost: ManaCost::zero(),
                requires_tap: false,
                sacrifice_cost: None,
                life_cost: 0,
                effect,
                description: "ingress".into(),
            }],
            ..Default::default()
        });
        let source = s
            .create_card_in_zone(actor, 0, ZoneType::Battlefield)
            .unwrap();
        if family < 2 {
            s.move_object(a, ZoneType::Hand, ZoneType::Library);
        }
        if family == 1 {
            s.players[0].tutor_targets = vec![AURA];
        }
        rules::apply_action(
            &mut s,
            &Action::ActivateAbility {
                object_id: source,
                ability_index: 0,
                targets: vec![],
            },
        );
        resolve(&mut s);
        if family == 1 {
            assert!(s.pending_tutor.is_some());
            let before = s.clone();
            rules::apply_action(&mut s, &Action::ChooseTutorTarget { card_id: AURA });
            failure_only(&s, &before);
        }
        assert_eq!(
            s.invalid_gameplay_reason(),
            Some(TerminationReason::PreparedSbaFailure)
        );
        assert!(!s.battlefield.contains(&a));
        assert_eq!(s.objects.len(), 3);
        assert_eq!(s.next_object_id, 4);
        assert!(!s.objects.values().any(|i| i.is_token));
    }
}

#[test]
fn successful_entry_etb_replacements_link_and_timestamp_are_coherent() {
    let (mut s, a, t) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap())
        .cards
        .get_mut(&AURA)
        .unwrap()
        .triggered_abilities
        .push(TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::GainLife { amount: 2 },
            description: "entry".into(),
        });
    s.replacement_effects
        .push(mtg_gto::replacement::ReplacementEffect {
            source_id: t,
            controller: 0,
            applies_to: mtg_gto::replacement::ReplacementEventKind::EntersBattlefield,
            action: mtg_gto::replacement::ReplacementAction::EntersModified {
                enters_tapped: true,
                extra_counters: 2,
            },
            is_self_replacement: true,
            description: "entry".into(),
        });
    cast(&mut s, a, t);
    let stamp = s.next_timestamp;
    resolve(&mut s);
    let inst = &s.objects[&a];
    assert!(inst.tapped);
    assert_eq!(inst.plus_counters, 2);
    let link = inst.attachment_link().unwrap();
    assert_eq!(link.timestamp, stamp);
    assert_eq!(link.source_generation, inst.zone_change_count);
    assert_eq!(link.target, s.exact_object(t).unwrap());
    assert_eq!(s.effective_power(t), 3);
    assert_eq!(s.stack.len(), 1);
    match &s.stack[0].source {
        StackSource::TriggeredAbility { context, .. } => {
            assert_eq!(context.source_generation, link.source_generation)
        }
        _ => panic!(),
    };
    resolve(&mut s);
    assert_eq!(s.players[0].life, 22);
}

#[test]
fn cast_observers_snapshot_owned_aura_facts_and_spell_copy_stays_unsupported() {
    let (mut s, a, t) = fixture();
    let watcher = 970120;
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: watcher,
        name: "Cast observer".into(),
        card_types: vec![CardType::Creature],
        power: Some(1),
        toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::YouCastSpell,
            effect: Effect::GainLife { amount: 1 },
            description: "cast".into(),
        }],
        ..Default::default()
    });
    s.create_card_in_zone(watcher, 0, ZoneType::Battlefield)
        .unwrap();
    cast(&mut s, a, t);
    assert_eq!(s.stack.len(), 2);
    let spell = s.stack[0].id;
    let snapshot = rules::snapshot_stack_spell(&s, spell).unwrap();
    assert_eq!(snapshot.definition.enchant, EnchantRestriction::Creature);
    depart(&mut s, t, ZoneType::Exile);
    s.move_object(t, ZoneType::Exile, ZoneType::Hand);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 970121,
        name: "Hidden newer definition".into(),
        card_types: vec![CardType::Artifact],
        ..Default::default()
    });
    s.objects.get_mut(&t).unwrap().card_def_id = 970121;
    let later = rules::snapshot_stack_spell(&s, spell).unwrap();
    assert_eq!(
        later.historical_object_targets,
        snapshot.historical_object_targets
    );
    assert_eq!(
        rules::copy_stack_spell(&mut s, spell, 0, rules::CopyTargetPolicy::Preserve),
        Err(rules::CopyError::UnsupportedSpell)
    );
    resolve(&mut s);
    assert_eq!(s.players[0].life, 21);
    resolve(&mut s);
    assert!(s.players[0].graveyard.contains(&a));
}

#[test]
fn pending_aura_hidden_newer_targets_do_not_supply_owned_public_facts() {
    use mtg_gto::info_set::{
        BucketedAbstraction, CardAwareBucketedAbstraction, InfoSetAbstraction,
    };
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    depart(&mut s, t, ZoneType::Exile);
    s.move_object(t, ZoneType::Exile, ZoneType::Hand);
    let before = InformationSet::from_view(&s.visible_state(0), s.card_db()).unwrap();
    let mut changed = s.clone();
    Arc::make_mut(changed.card_db.as_mut().unwrap()).insert(CardDef {
        id: 970122,
        name: "Other hidden card".into(),
        card_types: vec![CardType::Artifact],
        ..Default::default()
    });
    changed.objects.get_mut(&t).unwrap().card_def_id = 970122;
    changed.objects.get_mut(&t).unwrap().plus_counters = 10;
    changed.invalidate_characteristics_cache();
    let after = InformationSet::from_view(&changed.visible_state(0), changed.card_db()).unwrap();
    assert_eq!(before.hash_value(), after.hash_value());
    assert_eq!(
        BucketedAbstraction.abstract_info_set(&before),
        BucketedAbstraction.abstract_info_set(&after)
    );
    assert_eq!(
        CardAwareBucketedAbstraction {
            card_db: s.card_db()
        }
        .abstract_info_set(&before),
        CardAwareBucketedAbstraction {
            card_db: changed.card_db()
        }
        .abstract_info_set(&after)
    );
}

#[test]
fn solver_consumers_continue_supported_pending_and_dispose_malformed_attempts() {
    use mtg_gto::{
        info_set::BucketedAbstraction,
        solver::{
            mccfr::{run_iteration, McfrConfig},
            mcts::{MctsConfig, MctsStrategy},
            RegretTable,
        },
        strategy::{AbstractedMcfrStrategy, McfrStrategy, Strategy},
    };
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    let mut tables = [RegretTable::new(), RegretTable::new()];
    let config = McfrConfig {
        max_depth: 1,
        max_actions: 20,
        max_nodes_per_iteration: 0,
    };
    run_iteration(&s, &mut tables, &config).unwrap();
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
    for policy in &policies {
        assert!(policy.choose_action(&s, 0).is_ok());
    }
    s.stack[0].target_generations[0] = None;
    assert_eq!(
        run_iteration(&s, &mut tables, &config),
        Err(TerminationReason::StateEncoding)
    );
    for table in &tables {
        assert!(table.num_info_sets().is_err());
        assert!(table.to_bytes().is_err());
    }
    for policy in &policies {
        assert_eq!(
            policy.choose_action(&s, 0),
            Err(TerminationReason::StateEncoding)
        );
    }
}

fn permuted_pair(reverse: bool, offset: u64) -> (GameState, [u64; 2], [u64; 2]) {
    let (base, _, _) = fixture();
    let mut s = GameState::new(2);
    s.card_db = base.card_db;
    Arc::make_mut(s.card_db.as_mut().unwrap())
        .cards
        .get_mut(&AURA)
        .unwrap()
        .keywords
        .push(KeywordAbility::Flash);
    s.phase = Phase::PreCombatMain;
    s.players[0].mana_pool.colorless = 8;
    s.next_object_id = offset;
    let (sources, targets) = if reverse {
        let t1 = s
            .create_card_in_zone(BODY, 1, ZoneType::Battlefield)
            .unwrap();
        let t0 = s
            .create_card_in_zone(BODY, 1, ZoneType::Battlefield)
            .unwrap();
        let a1 = s.create_card_in_zone(AURA, 0, ZoneType::Hand).unwrap();
        let a0 = s.create_card_in_zone(AURA, 0, ZoneType::Hand).unwrap();
        ([a0, a1], [t0, t1])
    } else {
        let a0 = s.create_card_in_zone(AURA, 0, ZoneType::Hand).unwrap();
        let a1 = s.create_card_in_zone(AURA, 0, ZoneType::Hand).unwrap();
        let t0 = s
            .create_card_in_zone(BODY, 1, ZoneType::Battlefield)
            .unwrap();
        let t1 = s
            .create_card_in_zone(BODY, 1, ZoneType::Battlefield)
            .unwrap();
        ([a0, a1], [t0, t1])
    };
    for inst in s.objects.values_mut() {
        inst.zone_change_count = offset as u32;
    }
    (s, sources, targets)
}
fn all_abstractions(s: &GameState, player: usize) -> [u64; 3] {
    use mtg_gto::info_set::{
        BucketedAbstraction, CardAwareBucketedAbstraction, InfoSetAbstraction,
    };
    let info = InformationSet::from_view(&s.visible_state(player), s.card_db()).unwrap();
    [
        info.hash_value(),
        BucketedAbstraction.abstract_info_set(&info),
        CardAwareBucketedAbstraction {
            card_db: s.card_db(),
        }
        .abstract_info_set(&info),
    ]
}
#[test]
fn identical_sources_pending_pairing_live_history_and_complete_actions_survive_raw_id_permutations()
{
    use mtg_gto::public_projection::JointPublicNormalization;
    for relation in [false, true] {
        for stage in 0..6 {
            let mut states = Vec::new();
            for (reverse, offset) in [(false, 1), (true, 101), (false, 307)] {
                let (mut s, sources, targets) = permuted_pair(reverse, offset);
                if relation {
                    change(&mut s, targets[1], LayerModification::ModifyPT(0, 0));
                    // Its public relationship distinguishes an otherwise identical target.
                    s.continuous_effects.last_mut().unwrap().source_id = targets[0];
                } else {
                    s.objects.get_mut(&targets[0]).unwrap().plus_counters = 1;
                }
                s.invalidate_characteristics_cache();
                if stage > 0 {
                    for i in 0..2 {
                        cast(&mut s, sources[i], targets[i]);
                    }
                }
                if stage == 4 {
                    depart(&mut s, targets[0], ZoneType::Exile);
                    s.move_object(targets[0], ZoneType::Exile, ZoneType::Battlefield);
                }
                if matches!(stage, 2 | 3 | 5) {
                    resolve(&mut s);
                    resolve(&mut s);
                }
                if matches!(stage, 3 | 5) {
                    depart(&mut s, targets[0], ZoneType::Graveyard);
                }
                if stage == 5 {
                    rules::check_state_based_actions(&mut s);
                }
                states.push(s);
            }
            let mut encodings = Vec::new();
            let mut key_sets = Vec::new();
            for s in &states {
                let n = JointPublicNormalization::for_state(s, 0).unwrap();
                let actions = legal_actions(s);
                let mut keys = canonical::canonicalize_actions(&actions, s, &n).unwrap();
                assert_eq!(keys.len(), actions.len());
                for (key, action) in keys.iter().zip(&actions) {
                    assert_eq!(
                        canonical::resolve_with_normalization(key, s, 0, &n).unwrap(),
                        Some(action.clone())
                    );
                }
                keys.sort_by_cached_key(|key| format!("{key:?}"));
                encodings.push(n.encoding.clone());
                key_sets.push(keys);
            }
            for player in 0..2 {
                let facts: Vec<_> = states.iter().map(|s| all_abstractions(s, player)).collect();
                assert!(
                    facts.windows(2).all(|p| p[0] == p[1]),
                    "relation={relation} stage={stage} player={player}"
                );
            }
            assert!(
                encodings.windows(2).all(|p| p[0] == p[1]),
                "relation={relation} stage={stage}"
            );
            assert!(
                key_sets.windows(2).all(|p| p[0] == p[1]),
                "relation={relation} stage={stage}"
            );
            for key in &key_sets[0] {
                let mut outcomes = Vec::new();
                for s in &states {
                    let action = canonical::resolve(key, s, 0).unwrap().unwrap();
                    assert_eq!(canonical::canonicalize(&action, s).unwrap(), *key);
                    let mut next = s.clone();
                    rules::apply_action(&mut next, &action);
                    assert!(next.sba_failure.is_none());
                    outcomes.push(
                        JointPublicNormalization::for_state(&next, 0)
                            .unwrap()
                            .encoding
                            .clone(),
                    );
                }
                assert!(outcomes.windows(2).all(|p| p[0] == p[1]));
            }
        }
    }
}
#[cfg(feature = "tui")]
#[test]
fn tui_pending_aura_displays_owned_target_and_rejects_encoding_before_actions() {
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    let text = mtg_gto::tui::format_stack_entry(&s, &s.stack[0], s.card_db());
    assert!(text.contains("Aura spell: Synthetic Aura → Body"));
    assert!(text.contains(&format!("stack #{}", s.stack[0].id)));
    let db = s.card_db().clone();
    let app = mtg_gto::tui::App::new(s.clone(), db.clone());
    assert!(!app.cached_actions.is_empty());
    s.stack[0].target_generations[0] = None;
    let app = mtg_gto::tui::App::new(s, db);
    assert!(app.cached_actions.is_empty());
    assert!(app
        .status_log
        .last()
        .unwrap()
        .contains("INVALID reason=state_encoding"));
}

#[test]
fn aura_etb_and_departure_apnap_order_persists_and_reconstructs_all_choices() {
    use mtg_gto::public_projection::JointPublicNormalization;
    let (mut s, a, t) = fixture();
    let db = Arc::make_mut(s.card_db.as_mut().unwrap());
    let aura = db.cards.get_mut(&AURA).unwrap();
    aura.static_abilities = vec![StaticAbility::Anthem {
        power: 0,
        toughness: -3,
        affected: AffectedObjects::AttachedTo,
    }];
    aura.triggered_abilities = vec![
        TriggeredAbility {
            trigger: TriggerCondition::EntersBattlefield,
            effect: Effect::GainLife { amount: 1 },
            description: "entry".into()
        };
        2
    ];
    db.cards.get_mut(&BODY).unwrap().triggered_abilities = vec![
        TriggeredAbility {
            trigger: TriggerCondition::Dies,
            effect: Effect::GainLife { amount: 1 },
            description: "death".into()
        };
        2
    ];
    cast(&mut s, a, t);
    resolve(&mut s);
    assert!(s.players[0].graveyard.contains(&a));
    assert!(s.players[1].graveyard.contains(&t));
    assert_eq!(s.pending_triggers.len(), 4);
    assert!(s.stack.is_empty());
    assert_eq!(s.priority_player, 0);
    for mut restored in restores(&s) {
        for chooser in 0..2 {
            let n = JointPublicNormalization::for_state(&restored, chooser).unwrap();
            let actions = legal_actions(&restored);
            assert_eq!(restored.priority_player, chooser);
            assert!(actions
                .iter()
                .all(|action| matches!(action, Action::Concede)
                    || if chooser == 0 {
                        matches!(action, Action::OrderTriggers { .. })
                    } else {
                        matches!(action, Action::OrderTriggerOccurrences { .. })
                    }));
            let keys = canonical::canonicalize_actions(&actions, &restored, &n).unwrap();
            assert_eq!(keys.len(), 3);
            for (key, action) in keys.iter().zip(&actions) {
                assert_eq!(
                    canonical::resolve_with_normalization(key, &restored, chooser, &n).unwrap(),
                    Some(action.clone())
                );
                assert_eq!(canonical::canonicalize(action, &restored).unwrap(), *key);
            }
            rules::apply_action(&mut restored, &actions[0]);
        }
        assert_eq!(
            restored
                .stack
                .iter()
                .map(|entry| entry.controller)
                .collect::<Vec<_>>(),
            vec![0, 0, 1, 1]
        );
        assert!(restored.pending_triggers.is_empty());
        assert_eq!(restored.priority_player, 0);
        for _ in 0..4 {
            resolve(&mut restored);
        }
        assert!(restored.stack.is_empty());
        assert_eq!(restored.priority_player, 0);
        assert_eq!(restored.players[0].life, 22);
        assert_eq!(restored.players[1].life, 22);
    }
}

#[test]
fn computed_flash_admission_matches_generation_and_removal_restores_timing() {
    let (mut s, a, t) = fixture();
    s.phase = Phase::Upkeep;
    change(
        &mut s,
        a,
        LayerModification::AddKeyword(KeywordAbility::Flash),
    );
    let action = Action::CastSpell {
        object_id: a,
        targets: vec![Target::Object(t)],
    };
    assert!(legal_actions(&s).contains(&action));
    let mut removed = s.clone();
    change(&mut removed, a, LayerModification::RemoveAllAbilities);
    assert!(!legal_actions(&removed).contains(&action));
    let before = removed.clone();
    rules::apply_action(&mut removed, &action);
    assert_eq!(
        serde_json::to_value(&before).unwrap(),
        serde_json::to_value(&removed).unwrap()
    );
    cast(&mut s, a, t);
    resolve(&mut s);
    assert!(s.battlefield.contains(&a));
}
#[test]
fn animated_aura_death_keyword_noncast_return_rejects_without_mutation() {
    use mtg_gto::rules::transitions::{return_death_keyword, ZoneTriggerContext};
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    resolve(&mut s);
    change(&mut s, a, LayerModification::AddType(CardType::Creature));
    change(&mut s, a, LayerModification::SetPT(1, 1));
    change(
        &mut s,
        a,
        LayerModification::AddKeyword(KeywordAbility::Undying),
    );
    let exact = s.exact_object(a).unwrap();
    let batch = transition_batch(
        &mut s,
        &[TransitionRequest {
            object: exact,
            from: ZoneType::Battlefield,
            to: ZoneType::Graveyard,
            kind: MovementKind::Put,
        }],
    )
    .unwrap();
    let subject = batch.transitions[0].clone();
    let context = ZoneTriggerContext {
        source_before: subject.before.clone(),
        subject,
        source_was_subject: true,
        group_id: batch.group_id,
    };
    let before = s.clone();
    assert_eq!(
        return_death_keyword(&mut s, &context, KeywordAbility::Undying),
        Err(TransitionError::UnsupportedPath)
    );
    assert_eq!(
        serde_json::to_value(&before).unwrap(),
        serde_json::to_value(&s).unwrap()
    );
}

#[test]
fn corrupted_missing_or_unsupported_established_metadata_is_not_ability_removal() {
    for restriction in [EnchantRestriction::Missing, EnchantRestriction::Unsupported] {
        let (mut s, a, t) = fixture();
        cast(&mut s, a, t);
        resolve(&mut s);
        Arc::make_mut(s.card_db.as_mut().unwrap())
            .cards
            .get_mut(&AURA)
            .unwrap()
            .enchant = restriction;
        change(&mut s, a, LayerModification::RemoveAllAbilities);
        let chars = s.visible_state(0).represented_characteristics[&a].clone();
        assert_eq!(
            chars.enchant,
            mtg_gto::card::ComputedEnchant::Restriction(restriction)
        );
        let before = s.clone();
        rules::check_state_based_actions(&mut s);
        assert_eq!(
            s.invalid_gameplay_reason(),
            Some(if restriction == EnchantRestriction::Missing {
                TerminationReason::StateEncoding
            } else {
                TerminationReason::PreparedSbaFailure
            })
        );
        failure_only(&s, &before);
    }
}
#[test]
fn source_effect_departure_causes_later_common_pass_creature_death() {
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    resolve(&mut s);
    change(&mut s, a, LayerModification::AddType(CardType::Creature));
    change(&mut s, a, LayerModification::SetPT(1, 0));
    s.objects.get_mut(&t).unwrap().damage_marked = 2;
    s.invalidate_characteristics_cache();
    let group = s.next_zone_event_group_id;
    rules::check_state_based_actions(&mut s);
    assert!(s.players[0].graveyard.contains(&a));
    assert!(s.players[1].graveyard.contains(&t));
    assert_eq!(s.next_zone_event_group_id, group + 2);
    assert_eq!(s.objects[&a].zone_change_count, 3);
    assert_eq!(s.objects[&t].zone_change_count, 1);
    assert!(s.sba_failure.is_none());
}
#[test]
fn pending_target_generation_exhaustion_and_cast_preflight_failures_are_atomic() {
    let (mut s, a, t) = fixture();
    s.objects.get_mut(&t).unwrap().zone_change_count = u32::MAX;
    cast(&mut s, a, t);
    rules::apply_action(&mut s, &Action::PassPriority);
    let before = s.clone();
    rules::apply_action(&mut s, &Action::PassPriority);
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    failure_only(&s, &before);
    for case in 0..10 {
        let (mut s, a, t) = fixture();
        s.players[0].mana_pool.colorless = 0;
        let land = s
            .create_card_in_zone(LAND, 0, ZoneType::Battlefield)
            .unwrap();
        match case {
            0 => s.objects.get_mut(&a).unwrap().zone_change_count = u32::MAX,
            1 => s.next_stack_id = u64::MAX,
            2 => s.objects.get_mut(&a).unwrap().controller = 2,
            3 => s.objects.get_mut(&a).unwrap().owner = 2,
            4 => s.players[0].hand.push(a),
            5 => s.objects.get_mut(&t).unwrap().object_id += 1,
            6 => s.battlefield.push(t),
            7 => s.objects.get_mut(&a).unwrap().owner = 1,
            8 => change(&mut s, a, LayerModification::ChangeController(2)),
            _ => change(&mut s, t, LayerModification::ChangeController(2)),
        }
        let before = s.clone();
        rules::apply_action(
            &mut s,
            &Action::CastSpell {
                object_id: a,
                targets: vec![Target::Object(t)],
            },
        );
        assert_eq!(
            s.invalid_gameplay_reason(),
            Some(TerminationReason::StateEncoding)
        );
        assert!(!s.objects[&land].tapped);
        failure_only(&s, &before);
    }
}
#[test]
fn attachment_entry_retains_independent_record_timestamp_and_valid_last_timestamp_capacity() {
    let (mut s, a, t) = fixture();
    let effect = ContinuousEffect {
        source_id: a,
        controller: 0,
        timestamp: 4,
        duration: Duration::WhileSourceOnBattlefield,
        affected: AffectedObjects::Specific(t),
        modification: LayerModification::ModifyPT(2, 0),
    };
    // An independent record is distinguished from the definition's +1/+1 template.
    cast(&mut s, a, t);
    s.continuous_effects.push(effect.clone());
    s.next_timestamp = u32::MAX - 1;
    s.invalidate_characteristics_cache();
    resolve(&mut s);
    assert!(s.sba_failure.is_none());
    assert!(s.continuous_effects.contains(&effect));
    assert_eq!(
        s.objects[&a].attachment_link().unwrap().timestamp,
        u32::MAX - 1
    );
    assert_eq!(s.effective_power(t), 5);
}

#[test]
fn pending_aura_survives_unrelated_mandatory_order_and_all_restore_forms() {
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 970150,
        name: "Mandatory victim".into(),
        card_types: vec![CardType::Creature],
        power: Some(1),
        toughness: Some(1),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::Dies,
            effect: Effect::GainLife { amount: 1 },
            description: "unrelated".into(),
        }],
        ..Default::default()
    });
    for _ in 0..2 {
        let victim = s
            .create_card_in_zone(970150, 0, ZoneType::Battlefield)
            .unwrap();
        s.objects.get_mut(&victim).unwrap().temp_toughness_mod = -1;
    }
    s.invalidate_characteristics_cache();
    rules::check_state_based_actions(&mut s);
    assert_eq!(s.pending_triggers.len(), 2);
    assert_eq!(s.stack.len(), 1);
    let pending = s.stack[0].clone();
    for mut copy in restores(&s) {
        let before = copy.clone();
        rules::apply_action(
            &mut copy,
            &Action::CastSpell {
                object_id: a,
                targets: vec![Target::Object(t)],
            },
        );
        assert_eq!(
            serde_json::to_value(&before).unwrap(),
            serde_json::to_value(&copy).unwrap()
        );
        let actions = legal_actions(&copy);
        assert_eq!(actions.len(), 3);
        assert!(actions.iter().all(|action| matches!(
            action,
            Action::OrderTriggerOccurrences { .. } | Action::Concede
        )));
        rules::apply_action(&mut copy, &actions[0]);
        assert_eq!(copy.stack.len(), 3);
        assert_eq!(
            serde_json::to_value(&copy.stack[0]).unwrap(),
            serde_json::to_value(&pending).unwrap()
        );
        resolve(&mut copy);
        resolve(&mut copy);
        assert_eq!(copy.stack.len(), 1);
        assert!(copy.objects[&a].attachment_link().is_none());
        resolve(&mut copy);
        assert_eq!(copy.effective_power(t), 3);
        assert_eq!(copy.players[0].life, 22);
        assert_eq!(copy.priority_player, 0);
        assert_eq!(copy.players[0].mana_pool.colorless, 3);
    }
}
#[test]
fn four_mandatory_occurrences_preserve_complete_keys_and_outcomes_across_raw_id_reversals() {
    use mtg_gto::public_projection::JointPublicNormalization;
    let mut states = Vec::new();
    for (reverse, offset) in [(false, 1), (true, 201), (false, 409)] {
        let (mut s, sources, targets) = permuted_pair(reverse, offset);
        let db = Arc::make_mut(s.card_db.as_mut().unwrap());
        let aura = db.cards.get_mut(&AURA).unwrap();
        aura.static_abilities = vec![StaticAbility::Anthem {
            power: 0,
            toughness: -3,
            affected: AffectedObjects::AttachedTo,
        }];
        aura.triggered_abilities = vec![
            TriggeredAbility {
                trigger: TriggerCondition::EntersBattlefield,
                effect: Effect::GainLife { amount: 1 },
                description: "entry".into()
            };
            2
        ];
        db.cards.get_mut(&BODY).unwrap().triggered_abilities = vec![
            TriggeredAbility {
                trigger: TriggerCondition::Dies,
                effect: Effect::GainLife { amount: 1 },
                description: "death".into()
            };
            2
        ];
        cast(&mut s, sources[0], targets[0]);
        resolve(&mut s);
        assert_eq!(s.pending_triggers.len(), 4);
        states.push(s);
    }
    for chooser in 0..2 {
        let mut encodings = Vec::new();
        let mut keys_by_state = Vec::new();
        for s in &states {
            assert_eq!(s.priority_player, chooser);
            let n = JointPublicNormalization::for_state(s, chooser).unwrap();
            let actions = legal_actions(s);
            let mut keys = canonical::canonicalize_actions(&actions, s, &n).unwrap();
            assert_eq!(keys.len(), 3);
            keys.sort_by_cached_key(|key| format!("{key:?}"));
            encodings.push(n.encoding.clone());
            keys_by_state.push(keys);
        }
        assert!(encodings.windows(2).all(|p| p[0] == p[1]));
        assert!(keys_by_state.windows(2).all(|p| p[0] == p[1]));
        for key in &keys_by_state[0] {
            let mut outcomes = Vec::new();
            for s in &states {
                let action = canonical::resolve(key, s, chooser).unwrap().unwrap();
                assert_eq!(canonical::canonicalize(&action, s).unwrap(), *key);
                let mut next = s.clone();
                rules::apply_action(&mut next, &action);
                outcomes.push(
                    JointPublicNormalization::for_state(&next, 0)
                        .unwrap()
                        .encoding
                        .clone(),
                );
            }
            assert!(outcomes.windows(2).all(|p| p[0] == p[1]));
        }
        for s in &mut states {
            let action = canonical::resolve(
                keys_by_state[0]
                    .iter()
                    .find(|key| !matches!(key, canonical::CanonicalAction::Concede))
                    .unwrap(),
                s,
                chooser,
            )
            .unwrap()
            .unwrap();
            rules::apply_action(s, &action);
        }
    }
    for s in &mut states {
        for _ in 0..4 {
            resolve(s);
        }
    }
    let outcomes: Vec<_> = states
        .iter()
        .map(|s| {
            JointPublicNormalization::for_state(s, 0)
                .unwrap()
                .encoding
                .clone()
        })
        .collect();
    assert!(outcomes.windows(2).all(|p| p[0] == p[1]));
}

#[test]
fn pending_token_target_ceases_then_aura_fizzles_using_owned_historical_endpoint() {
    let (mut s, a, t) = fixture();
    s.objects.get_mut(&t).unwrap().is_token = true;
    cast(&mut s, a, t);
    let historical = s.exact_object(t).unwrap();
    depart(&mut s, t, ZoneType::Graveyard);
    rules::check_state_based_actions(&mut s);
    assert!(!s.objects.contains_key(&t));
    assert!(s.sba_failure.is_none());
    for mut copy in restores(&s) {
        match copy.stack[0].source {
            StackSource::AuraSpell {
                target,
                target_is_token,
                ..
            } => {
                assert_eq!(target, historical);
                assert!(target_is_token);
            }
            _ => panic!(),
        }
        assert!(InformationSet::from_view(&copy.visible_state(0), copy.card_db()).is_ok());
        let events = copy.pending_events.len();
        resolve(&mut copy);
        assert!(copy.players[0].graveyard.contains(&a));
        assert!(copy.sba_failure.is_none());
        assert!(!copy.pending_events[events..].iter().any(|event| matches!(event,
            mtg_gto::events::GameEvent::ZoneChange { object, to: mtg_gto::events::Zone::Battlefield, .. } if *object == a)));
    }
}

#[test]
fn pending_and_live_auras_share_target_with_distinct_owners_and_cast_controllers() {
    let (mut s, a, t) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap())
        .cards
        .get_mut(&AURA)
        .unwrap()
        .keywords
        .push(KeywordAbility::Flash);
    let b = s.create_card_in_zone(AURA, 1, ZoneType::Hand).unwrap();
    s.players[1].mana_pool.colorless = 2;
    cast(&mut s, a, t);
    rules::apply_action(&mut s, &Action::PassPriority);
    assert_eq!(s.priority_player, 1);
    cast(&mut s, b, t);
    assert_eq!(
        s.stack.iter().map(|e| e.controller).collect::<Vec<_>>(),
        vec![0, 1]
    );
    s.objects.get_mut(&t).unwrap().controller = 0;
    s.invalidate_characteristics_cache();
    resolve(&mut s);
    resolve(&mut s);
    assert_eq!(s.attachments_of(s.exact_object(t).unwrap()).len(), 2);
    assert_eq!(s.effective_toughness(t), 4);
    assert_eq!((s.objects[&a].owner, s.objects[&a].controller), (0, 0));
    assert_eq!((s.objects[&b].owner, s.objects[&b].controller), (1, 1));
    depart(&mut s, t, ZoneType::Graveyard);
    rules::check_state_based_actions(&mut s);
    assert!(s.players[0].graveyard.contains(&a));
    assert!(s.players[1].graveyard.contains(&b));
    assert!(s.players[1].graveyard.contains(&t));
    assert!(!s.players[0].graveyard.contains(&b));
    assert!(s.sba_failure.is_none());
}
#[test]
fn ordinary_counter_removes_the_exact_physical_aura_without_entry_or_attachment() {
    let (mut s, a, t) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 970151,
        name: "Counter control".into(),
        card_types: vec![CardType::Instant],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::Counter {
            target: mtg_gto::card::TargetSpec::AnySpell,
        }),
        ..Default::default()
    });
    let counter = s.create_card_in_zone(970151, 0, ZoneType::Hand).unwrap();
    cast(&mut s, a, t);
    let stack_id = s.stack[0].id;
    let action = Action::CastSpell {
        object_id: counter,
        targets: vec![Target::StackEntry(stack_id)],
    };
    assert!(legal_actions(&s).contains(&action));
    rules::apply_action(&mut s, &action);
    let events = s.pending_events.len();
    resolve(&mut s);
    assert!(s.stack.is_empty());
    assert!(s.players[0].graveyard.contains(&a));
    assert!(s.objects[&a].attachment_link().is_none());
    assert_eq!(s.effective_power(t), 2);
    assert!(s.sba_failure.is_none());
    assert!(!s.pending_events[events..].iter().any(|event| matches!(event,
        mtg_gto::events::GameEvent::ZoneChange {object,to:mtg_gto::events::Zone::Battlefield,..} if *object==a)));
}

#[test]
fn raw_ingress_distinguishes_malformed_pending_identity_from_valid_unsupported_movement() {
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    let before = s.clone();
    assert_eq!(
        s.try_move_object(a, ZoneType::Stack, ZoneType::Battlefield),
        Err(TransitionError::UnsupportedPath)
    );
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
    s.objects.get_mut(&a).unwrap().zone_change_count += 1;
    let before = s.clone();
    assert_eq!(
        s.try_move_object(a, ZoneType::Stack, ZoneType::Battlefield),
        Err(TransitionError::StaleIncarnation(a))
    );
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}

#[test]
fn attached_entry_keeps_independent_records_even_when_they_match_static_templates() {
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    let independent = ContinuousEffect {
        source_id: a,
        controller: 0,
        timestamp: 4,
        duration: Duration::WhileSourceOnBattlefield,
        affected: AffectedObjects::AttachedTo,
        modification: LayerModification::ModifyPT(1, 1),
    };
    s.continuous_effects.push(independent.clone());
    s.next_timestamp = 10;
    s.invalidate_characteristics_cache();
    assert_eq!(s.effective_power(t), 2);
    resolve(&mut s);
    assert!(s.continuous_effects.contains(&independent));
    assert_eq!(s.effective_power(t), 4);
    assert_eq!(s.objects[&a].attachment_link().unwrap().timestamp, 10);
    assert!(s.sba_failure.is_none());
}

fn entry_observation_fixture(remove_abilities: bool) -> (GameState, u64) {
    let (mut s, a, t) = fixture();
    let def = Arc::make_mut(s.card_db.as_mut().unwrap())
        .cards
        .get_mut(&AURA)
        .unwrap();
    def.triggered_abilities = vec![TriggeredAbility {
        trigger: TriggerCondition::EntersBattlefield,
        effect: Effect::GainLife { amount: 3 },
        description: "entry observer".into(),
    }];
    if remove_abilities {
        def.static_abilities
            .push(StaticAbility::RemoveAllAbilities {
                affected: AffectedObjects::AllPermanents,
            });
    }
    cast(&mut s, a, t);
    change(&mut s, a, LayerModification::ChangeController(1));
    resolve(&mut s);
    (s, a)
}
#[test]
fn entry_observation_uses_refreshed_ability_removal() {
    let (s, a) = entry_observation_fixture(true);
    let chars = mtg_gto::layers::compute_characteristics(
        a,
        &s.continuous_effects,
        &s.objects,
        &s.battlefield.iter().copied().collect(),
        s.card_db(),
    )
    .unwrap();
    assert!(chars.abilities_removed);
    assert!(s.stack.is_empty());
    assert_eq!((s.players[0].life, s.players[1].life), (20, 20));
}
#[test]
fn entry_observation_uses_computed_controller() {
    let (mut s, a) = entry_observation_fixture(false);
    let chars = mtg_gto::layers::compute_characteristics(
        a,
        &s.continuous_effects,
        &s.objects,
        &s.battlefield.iter().copied().collect(),
        s.card_db(),
    )
    .unwrap();
    assert_eq!(chars.controller, 1);
    assert_eq!(s.stack.len(), 1);
    assert_eq!(s.stack[0].controller, 1);
    resolve(&mut s);
    assert_eq!((s.players[0].life, s.players[1].life), (20, 23));
}

#[test]
fn animated_entry_observation_uses_refreshed_watcher_abilities() {
    let (mut s, a, t) = fixture();
    let db = Arc::make_mut(s.card_db.as_mut().unwrap());
    db.cards
        .get_mut(&BODY)
        .unwrap()
        .triggered_abilities
        .push(TriggeredAbility {
            trigger: TriggerCondition::ACreatureEnters,
            effect: Effect::GainLife { amount: 4 },
            description: "creature entry watcher".into(),
        });
    db.cards.get_mut(&AURA).unwrap().static_abilities.extend([
        StaticAbility::AddType {
            card_type: CardType::Creature,
            affected: AffectedObjects::AllPermanents,
        },
        StaticAbility::RemoveAllAbilities {
            affected: AffectedObjects::AllPermanents,
        },
    ]);
    cast(&mut s, a, t);
    resolve(&mut s);
    assert!(s.stack.is_empty());
    assert!(s.players[0].graveyard.contains(&a));
    assert!(s.battlefield.contains(&t));
    assert_eq!((s.players[0].life, s.players[1].life), (20, 20));
}

#[test]
fn entry_activated_invalid_control_record_rejects_before_attachment_or_observation() {
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    s.continuous_effects.push(ContinuousEffect {
        source_id: a,
        controller: 0,
        timestamp: s.next_timestamp,
        duration: Duration::WhileSourceOnBattlefield,
        affected: AffectedObjects::Specific(a),
        modification: LayerModification::ChangeController(2),
    });
    rules::apply_action(&mut s, &Action::PassPriority);
    let before = s.clone();
    rules::apply_action(&mut s, &Action::PassPriority);
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    failure_only(&s, &before);
}

// Permanent reproductions of the independent Astra review findings.
#[test]
fn end_turn_preparation_failure_is_atomic() {
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    s.next_timestamp = u32::MAX;
    let before = s.clone();
    rules::apply_action(&mut s, &Action::EndTurn);
    let failure = s.sba_failure.clone();
    s.sba_failure = before.sba_failure.clone();
    println!(
        "ENDTURN failure={:?}; accepted before={} after={}",
        failure,
        before.loss_coordinates().action_index,
        s.loss_coordinates().action_index
    );
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}
#[test]
fn prowess_capacity_rejected_before_payment() {
    let (mut s, a, t) = fixture();
    let mut d = s.card_db().get(BODY).unwrap().clone();
    d.keywords.push(KeywordAbility::Prowess);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    s.objects.get_mut(&t).unwrap().controller = 0;
    s.invalidate_characteristics_cache();
    s.next_timestamp = u32::MAX;
    let before = s.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rules::apply_action(
            &mut s,
            &Action::CastSpell {
                object_id: a,
                targets: vec![Target::Object(t)],
            },
        )
    }));
    println!(
        "PROWESS panicked={} mana_before={} mana_after={} stack_before={} stack_after={}",
        result.is_err(),
        before.players[0].mana_pool.colorless,
        s.players[0].mana_pool.colorless,
        before.stack.len(),
        s.stack.len()
    );
    assert!(result.is_ok(), "Aura cast panicked after mutation");
    s.sba_failure = before.sba_failure.clone();
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}
#[test]
fn spell_count_capacity_rejected_before_payment() {
    let (mut s, a, t) = fixture();
    s.spells_cast_this_turn = u32::MAX;
    let before = s.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rules::apply_action(
            &mut s,
            &Action::CastSpell {
                object_id: a,
                targets: vec![Target::Object(t)],
            },
        )
    }));
    println!(
        "SPELLCOUNT panicked={} mana_before={} mana_after={} stack_before={} stack_after={}",
        result.is_err(),
        before.players[0].mana_pool.colorless,
        s.players[0].mana_pool.colorless,
        before.stack.len(),
        s.stack.len()
    );
    assert!(result.is_ok(), "Aura cast panicked after mutation");
    s.sba_failure = before.sba_failure.clone();
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}

#[test]
fn missing_authoritative_target_is_rejected_by_normalization() {
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    s.objects.remove(&t);
    s.battlefield.retain(|id| *id != t);
    let mut control = s.clone();
    rules::apply_action(&mut control, &Action::PassPriority);
    let joint = mtg_gto::public_projection::JointPublicNormalization::for_state(&s, 0);
    println!(
        "NORMALIZATION prepared_error={:?} joint_is_ok={}",
        control.invalid_gameplay_reason(),
        joint.is_ok()
    );
    assert!(
        joint.is_err(),
        "canonical consumer accepted missing non-token target"
    );
}
#[test]
fn mismatched_tutor_rejection_retains_continuation() {
    let (mut s, a, _) = fixture();
    s.move_object(a, ZoneType::Hand, ZoneType::Library);
    s.pending_tutor = Some(mtg_gto::game::PendingTutor {
        controller: 0,
        destination: ZoneType::Battlefield,
        subtype_filter: vec![],
    });
    s.priority_player = 1;
    let before = s.clone();
    rules::apply_action(&mut s, &Action::ChooseTutorTarget { card_id: AURA });
    println!("TUTOR failure={:?} pending_before={} pending_after={} accepted_before={} accepted_after={}",s.sba_failure,before.pending_tutor.is_some(),s.pending_tutor.is_some(),before.loss_coordinates().action_index,s.loss_coordinates().action_index);
    s.sba_failure = before.sba_failure.clone();
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}
#[test]
fn computed_aura_ingress_is_rejected_before_entry() {
    use mtg_gto::layers::{ContinuousEffect, Duration, LayerModification};
    let (mut s, a, _) = fixture();
    let mut def = s.card_db().get(AURA).unwrap().clone();
    def.subtypes.clear();
    def.enchant = EnchantRestriction::Missing;
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(def);
    s.continuous_effects.push(ContinuousEffect {
        source_id: a,
        controller: 0,
        timestamp: 1,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::Specific(a),
        modification: LayerModification::AddSubtype(Subtype("Aura".into())),
    });
    s.next_timestamp = 2;
    s.invalidate_characteristics_cache();
    println!("COMPUTED is_aura={}", s.has_subtype(a, "Aura"));
    let before = s.clone();
    rules::apply_action(
        &mut s,
        &Action::CastSpell {
            object_id: a,
            targets: vec![],
        },
    );
    println!(
        "COMPUTED after_cast mana={} stack={} failure={:?}",
        s.players[0].mana_pool.colorless,
        s.stack.len(),
        s.sba_failure
    );
    for _ in 0..2 {
        rules::apply_action(&mut s, &Action::PassPriority);
    }
    println!("COMPUTED after_resolve battlefield={} failure={:?} entry_event={}",s.battlefield.contains(&a),s.sba_failure,s.pending_events.iter().any(|e|matches!(e,mtg_gto::events::GameEvent::ZoneChange{object,to:mtg_gto::events::Zone::Battlefield,..}if *object==a)));
    s.sba_failure = before.sba_failure.clone();
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}

#[test]
fn commander_fizzle_reports_actual_destination() {
    let (mut s, a, t) = fixture();
    s.format = mtg_gto::game::GameFormat::Commander;
    s.players[0].commander_object_id = Some(a);
    s.players[0].commander_card_id = Some(AURA);
    cast(&mut s, a, t);
    s.objects
        .get_mut(&t)
        .unwrap()
        .temp_keywords
        .push(KeywordAbility::Shroud);
    s.invalidate_characteristics_cache();
    let old = s.pending_events.len();
    for _ in 0..2 {
        rules::apply_action(&mut s, &Action::PassPriority);
    }
    let events = &s.pending_events[old..];
    println!(
        "COMMANDER command={} graveyard={} events={:?}",
        s.players[0].command_zone.contains(&a),
        s.players[0].graveyard.contains(&a),
        events
    );
    assert!(events.iter().any(|e|matches!(e,mtg_gto::events::GameEvent::ZoneChange{object,to:mtg_gto::events::Zone::Command,..}if *object==a)),"event destination differs from storage");
}

fn animated_refresh_fixture() -> (GameState, u64, u64) {
    let (mut s, a, t) = fixture();
    let mut body = s.card_db().get(BODY).unwrap().clone();
    body.static_abilities
        .push(StaticAbility::ExtraLandDrops { count: 1 });
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(body);
    let mut aura = s.card_db().get(AURA).unwrap().clone();
    aura.power = Some(2);
    aura.toughness = Some(2);
    aura.static_abilities = vec![StaticAbility::AddType {
        card_type: CardType::Creature,
        affected: AffectedObjects::AllPermanents,
    }];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(aura);
    cast(&mut s, a, t);
    (s, a, t)
}
#[test]
fn immediate_entry_refresh_exhaustion_preserves_pending_spell() {
    let (mut s, a, _) = animated_refresh_fixture();
    s.next_timestamp = u32::MAX - 1;
    rules::apply_action(&mut s, &Action::PassPriority);
    let before = s.clone();
    rules::apply_action(&mut s, &Action::PassPriority);
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    assert!(!s.battlefield.contains(&a));
    failure_only(&s, &before);
}
#[test]
fn exhausted_detach_pass_preserves_completed_attached_entry() {
    let (mut s, a, t) = animated_refresh_fixture();
    s.next_timestamp = u32::MAX - 2;
    resolve(&mut s);
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    assert!(s.battlefield.contains(&a));
    assert_eq!(
        s.attachment_target(s.exact_object(a).unwrap()),
        s.exact_object(t)
    );
    assert_eq!(s.next_timestamp, u32::MAX);
    assert!(s.pending_events.iter().any(|event| matches!(event,
        mtg_gto::events::GameEvent::ZoneChange { object, to: mtg_gto::events::Zone::Battlefield, .. } if *object == a)));
    assert!(!s.players[0].graveyard.contains(&a));
}
#[test]
fn later_orphan_pass_exhaustion_preserves_completed_detach() {
    let (mut s, a, _) = animated_refresh_fixture();
    s.next_timestamp = u32::MAX - 4;
    resolve(&mut s);
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    assert!(s.battlefield.contains(&a));
    assert!(s.objects[&a].attachment_link().is_none());
    assert_eq!(s.next_timestamp, u32::MAX);
    assert!(!s.players[0].graveyard.contains(&a));
    assert!(!s.pending_events.iter().any(|event| matches!(event,
        mtg_gto::events::GameEvent::ZoneChange { object, from: mtg_gto::events::Zone::Battlefield, .. } if *object == a)));
}
#[test]
fn exact_six_timestamp_capacity_preserves_original_success_order() {
    let (s, a, t) = animated_refresh_fixture();
    for mut restored in restores(&s) {
        restored.next_timestamp = u32::MAX - 6;
        let event_start = restored.pending_events.len();
        resolve(&mut restored);
        assert!(restored.invalid_gameplay_reason().is_none());
        assert_eq!(restored.next_timestamp, u32::MAX);
        assert!(restored.players[0].graveyard.contains(&a));
        assert!(restored.objects[&a].attachment_link().is_none());
        assert!(restored.battlefield.contains(&t));
        let destinations: Vec<_> = restored.pending_events[event_start..]
            .iter()
            .filter_map(|event| match event {
                mtg_gto::events::GameEvent::ZoneChange { object, to, .. } if *object == a => {
                    Some(*to)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            destinations,
            vec![
                mtg_gto::events::Zone::Battlefield,
                mtg_gto::events::Zone::Graveyard
            ]
        );
    }
}
#[test]
fn direct_detach_refresh_exhaustion_is_atomic() {
    let (mut s, a, t) = fixture();
    cast(&mut s, a, t);
    resolve(&mut s);
    let mut body = s.card_db().get(BODY).unwrap().clone();
    body.static_abilities
        .push(StaticAbility::ExtraLandDrops { count: 1 });
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(body);
    s.next_timestamp = u32::MAX;
    let before = s.clone();
    assert!(!s.detach_exact(s.exact_object(a).unwrap(), s.exact_object(t).unwrap()));
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    failure_only(&s, &before);
}
#[test]
fn failed_mixed_pass_retains_movements_counters_equipment_and_aura_links() {
    let (mut s, a, t) = animated_refresh_fixture();
    let mut equipment = CardDef {
        id: 978007,
        card_types: vec![CardType::Artifact],
        subtypes: vec![Subtype("Equipment".into())],
        ..Default::default()
    };
    equipment.power = Some(2);
    equipment.toughness = Some(2);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(equipment);
    let e = s
        .create_card_in_zone(978007, 0, ZoneType::Battlefield)
        .unwrap();
    let attach = s
        .prepare_attach(
            s.exact_object(e).unwrap(),
            s.exact_object(t).unwrap(),
            mtg_gto::card::AttachmentContext::ExistingEquip,
        )
        .unwrap();
    s.commit_attach(attach).unwrap();
    let dead_def = CardDef {
        id: 978008,
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        ..Default::default()
    };
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(dead_def);
    let dead = s
        .create_card_in_zone(978008, 1, ZoneType::Battlefield)
        .unwrap();
    s.objects.get_mut(&dead).unwrap().damage_marked = 2;
    let ceased = s
        .create_card_in_zone(978008, 1, ZoneType::Graveyard)
        .unwrap();
    s.objects.get_mut(&ceased).unwrap().is_token = true;
    s.objects.get_mut(&t).unwrap().plus_counters = 2;
    s.objects.get_mut(&t).unwrap().minus_counters = 1;
    s.next_timestamp = u32::MAX - 4;
    let old_e_link = s.objects[&e].attachment_link();
    resolve(&mut s);
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    assert!(s.battlefield.contains(&dead));
    assert!(!s.players[1].graveyard.contains(&dead));
    assert_eq!(
        (s.objects[&t].plus_counters, s.objects[&t].minus_counters),
        (2, 1)
    );
    assert_eq!(s.objects[&e].attachment_link(), old_e_link);
    assert!(s.objects.contains_key(&ceased));
    assert!(s.players[1].graveyard.contains(&ceased));
    assert!(s.objects[&a].attachment_link().is_some());
    assert_eq!(s.next_timestamp, u32::MAX - 2);
}
#[test]
fn refresh_exhaustion_is_atomic_without_a_fixed_global_reserve() {
    let (mut s, _, t) = fixture();
    let mut body = s.card_db().get(BODY).unwrap().clone();
    body.static_abilities = vec![StaticAbility::ExtraLandDrops { count: 1 }; 7];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(body);
    s.next_timestamp = u32::MAX - 6;
    let before = s.clone();
    s.refresh_continuous_effects();
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    failure_only(&s, &before);
    s.sba_failure = None;
    s.next_timestamp = u32::MAX - 7;
    s.refresh_continuous_effects();
    assert!(s.invalid_gameplay_reason().is_none());
    assert_eq!(s.next_timestamp, u32::MAX);
    assert!(s.battlefield.contains(&t));
}

#[test]
fn computed_aura_noncast_raw_and_tutor_ingress_reject_before_mutation() {
    for family in 0..3 {
        let (mut s, a, _) = fixture();
        let mut def = s.card_db().get(AURA).unwrap().clone();
        def.subtypes.clear();
        def.enchant = EnchantRestriction::Missing;
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(def);
        s.move_object(a, ZoneType::Hand, ZoneType::Library);
        change(
            &mut s,
            a,
            LayerModification::AddSubtype(Subtype("Aura".into())),
        );
        let before = s.clone();
        match family {
            0 => {
                assert_eq!(
                    s.try_move_object(a, ZoneType::Library, ZoneType::Battlefield),
                    Err(TransitionError::UnsupportedPath)
                );
            }
            1 => {
                s.pending_tutor = Some(mtg_gto::game::PendingTutor {
                    controller: 0,
                    destination: ZoneType::Battlefield,
                    subtype_filter: vec![],
                });
                let before = s.clone();
                rules::apply_action(&mut s, &Action::ChooseTutorTarget { card_id: AURA });
                assert_eq!(
                    s.sba_failure,
                    Some(rules::sba::PreparedPassFailure::Transition(
                        TransitionError::UnsupportedPath
                    ))
                );
                failure_only(&s, &before);
                continue;
            }
            _ => {
                // Prospective freshly allocated source uses the same represented layer.
                s.continuous_effects.last_mut().unwrap().affected = AffectedObjects::AllPermanents;
                let before = s.clone();
                assert_eq!(
                    s.create_card_in_zone(AURA, 0, ZoneType::Battlefield),
                    Err(TransitionError::UnsupportedPath)
                );
                failure_only(&s, &before);
                continue;
            }
        }
        failure_only(&s, &before);
    }
}
#[test]
fn newly_computed_pending_aura_rejects_before_stack_pop_or_notifications() {
    let (mut s, a, _) = fixture();
    let mut def = s.card_db().get(AURA).unwrap().clone();
    def.subtypes.clear();
    def.enchant = EnchantRestriction::Missing;
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(def);
    rules::apply_action(
        &mut s,
        &Action::CastSpell {
            object_id: a,
            targets: vec![],
        },
    );
    assert!(matches!(s.stack.last().unwrap().source, StackSource::Spell(id) if id == a));
    change(
        &mut s,
        a,
        LayerModification::AddSubtype(Subtype("Aura".into())),
    );
    rules::apply_action(&mut s, &Action::PassPriority);
    let before = s.clone();
    rules::apply_action(&mut s, &Action::PassPriority);
    assert_eq!(
        s.sba_failure,
        Some(rules::sba::PreparedPassFailure::Transition(
            TransitionError::UnsupportedPath
        ))
    );
    failure_only(&s, &before);
}

#[test]
fn stopped_refresh_boundary_survives_restore_and_rejects_solver_attempts() {
    use mtg_gto::solver::{
        mccfr::{run_iteration, McfrConfig},
        mcts::{MctsConfig, MctsStrategy},
        RegretTable,
    };
    use mtg_gto::strategy::Strategy;
    let (mut s, _, _) = animated_refresh_fixture();
    s.next_timestamp = u32::MAX - 2;
    resolve(&mut s);
    for mut restored in restores(&s) {
        assert_eq!(
            restored.invalid_gameplay_reason(),
            Some(TerminationReason::StateEncoding)
        );
        assert!(legal_actions(&restored).is_empty());
        let before = restored.clone();
        rules::apply_action(&mut restored, &Action::PassPriority);
        failure_only(&restored, &before);
        let mut tables = [RegretTable::new(), RegretTable::new()];
        let config = McfrConfig {
            max_depth: 1,
            max_actions: 20,
            max_nodes_per_iteration: 0,
        };
        assert_eq!(
            run_iteration(&restored, &mut tables, &config),
            Err(TerminationReason::StateEncoding)
        );
        for table in &tables {
            assert!(table.num_info_sets().is_err());
            assert!(table.to_bytes().is_err());
        }
        let strategy = MctsStrategy::new(MctsConfig {
            iterations_per_move: 1,
            ..Default::default()
        });
        assert_eq!(
            strategy.choose_action(&restored, 0),
            Err(TerminationReason::StateEncoding)
        );
    }
}

#[test]
fn aura_cast_extort_arithmetic_is_preflighted_before_payment() {
    let (mut s, a, t) = fixture();
    let mut body = s.card_db().get(BODY).unwrap().clone();
    body.keywords.push(KeywordAbility::Extort);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(body);
    s.objects.get_mut(&t).unwrap().controller = 0;
    s.players[0].life = i32::MAX;
    s.invalidate_characteristics_cache();
    let before = s.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rules::apply_action(
            &mut s,
            &Action::CastSpell {
                object_id: a,
                targets: vec![Target::Object(t)],
            },
        );
    }));
    assert!(outcome.is_ok(), "Aura cast overflowed after payment");
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    failure_only(&s, &before);
}

#[test]
fn aura_cast_invalid_actor_and_watcher_indices_reject_atomically() {
    for malformed in 0..3 {
        let (mut s, a, t) = fixture();
        match malformed {
            0 => s.priority_player = s.players.len(),
            1 => s.active_player = s.players.len(),
            _ => {
                let watcher = s
                    .create_card_in_zone(BODY, 1, ZoneType::Battlefield)
                    .unwrap();
                s.objects.get_mut(&watcher).unwrap().controller = s.players.len();
            }
        }
        s.invalidate_characteristics_cache();
        let before = s.clone();
        rules::apply_action(
            &mut s,
            &Action::CastSpell {
                object_id: a,
                targets: vec![Target::Object(t)],
            },
        );
        assert_eq!(
            s.invalid_gameplay_reason(),
            Some(TerminationReason::StateEncoding)
        );
        failure_only(&s, &before);
    }
}

// Astra second-review adversarial reproductions and independent boundary control.
#[test]
fn linked_departure_refresh_preserves_actual_graveyard_order() {
    use mtg_gto::layers::Duration;
    let (mut s, _, primary) = fixture();
    let mut d = s.card_db().get(BODY).unwrap().clone();
    d.static_abilities = vec![StaticAbility::WonderInGraveyard];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    let linked = s.create_card_in_zone(BODY, 1, ZoneType::Exile).unwrap();
    s.objects.get_mut(&linked).unwrap().exiled_by = Some(primary);
    let stamp = s.next_timestamp;
    s.move_object(primary, ZoneType::Battlefield, ZoneType::Graveyard);
    let effects: Vec<_> = s
        .continuous_effects
        .iter()
        .filter(|e| e.duration == Duration::WhileSourceInGraveyard)
        .map(|e| (e.source_id, e.timestamp))
        .collect();
    println!(
        "LINKED primary={} linked={} graveyard={:?} effects={:?}",
        primary, linked, s.players[1].graveyard, effects
    );
    assert_eq!(s.players[1].graveyard, vec![linked, primary]);
    assert_eq!(effects, vec![(linked, stamp), (primary, stamp + 1)]);
}

#[test]
fn newly_generated_future_reference_rejects_whole_sba_pass() {
    let (mut s, _, t) = fixture();
    let mut d = s.card_db().get(BODY).unwrap().clone();
    d.static_abilities = vec![StaticAbility::Anthem {
        power: 1,
        toughness: 1,
        affected: AffectedObjects::SpecificIncarnation {
            object_id: t,
            zone_change_count: s.objects[&t].zone_change_count + 1,
        },
    }];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    s.objects.get_mut(&t).unwrap().plus_counters = 1;
    s.objects.get_mut(&t).unwrap().minus_counters = 1;
    let before = s.clone();
    rules::check_state_based_actions(&mut s);
    println!(
        "NEWREF failure={:?} counters={}/{} effects={} timestamp={}",
        s.sba_failure,
        s.objects[&t].plus_counters,
        s.objects[&t].minus_counters,
        s.continuous_effects.len(),
        s.next_timestamp
    );
    assert!(
        s.sba_failure.is_some(),
        "impossible generated dependency accepted"
    );
    s.sba_failure = before.sba_failure.clone();
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}
#[test]
fn self_sacrifice_stops_before_stack_allocation_on_refresh_failure() {
    use mtg_gto::card::{ActivatedAbility, Effect, SacrificeCost};
    let (mut s, _, t) = fixture();
    let mut d = s.card_db().get(BODY).unwrap().clone();
    d.static_abilities = vec![StaticAbility::ExtraLandDrops { count: 1 }];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    let source_def = CardDef {
        id: 971000,
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(),
            requires_tap: false,
            sacrifice_cost: Some(SacrificeCost::SelfSacrifice),
            life_cost: 0,
            effect: Effect::DrawCards { count: 1 },
            description: String::new(),
        }],
        ..Default::default()
    };
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(source_def);
    let source = s
        .create_card_in_zone(971000, 0, ZoneType::Battlefield)
        .unwrap();
    s.next_timestamp = u32::MAX;
    let before = s.clone();
    rules::apply_action(
        &mut s,
        &Action::ActivateAbility {
            object_id: source,
            ability_index: 0,
            targets: vec![],
        },
    );
    println!(
        "SACRIFICE failure={:?} source_on_bf={} target_on_bf={} stack={}",
        s.sba_failure,
        s.battlefield.contains(&source),
        s.battlefield.contains(&t),
        s.stack.len()
    );
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    failure_only(&s, &before);
    assert!(
        s.stack.is_empty(),
        "stack allocation continued after failure"
    );
}
#[test]
fn annihilator_stops_after_first_refresh_failure() {
    let (mut s, _, first) = fixture();
    let d = CardDef {
        id: 971001,
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        annihilator_count: Some(2),
        ..Default::default()
    };
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    let attacker = s
        .create_card_in_zone(971001, 0, ZoneType::Battlefield)
        .unwrap();
    s.objects.get_mut(&attacker).unwrap().summoning_sick = false;
    let d = CardDef {
        id: 971002,
        card_types: vec![CardType::Artifact],
        static_abilities: vec![StaticAbility::ExtraLandDrops { count: 1 }],
        ..Default::default()
    };
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    let second = s
        .create_card_in_zone(971002, 1, ZoneType::Battlefield)
        .unwrap();
    s.phase = Phase::DeclareAttackers;
    s.next_timestamp = u32::MAX;
    rules::apply_action(
        &mut s,
        &Action::DeclareAttackers {
            attackers: vec![attacker],
        },
    );
    println!(
        "ANNIHILATOR failure={:?} first_bf={} second_bf={} graveyard={:?}",
        s.sba_failure,
        s.battlefield.contains(&first),
        s.battlefield.contains(&second),
        s.players[1].graveyard
    );
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    assert!(s.battlefield.contains(&first));
    assert!(s.players[1].graveyard.is_empty());
    assert!(s.trigger_order_resume.is_none());
    assert!(
        s.battlefield.contains(&second),
        "later sacrifice continued after failure"
    );
}

#[test]
fn token_loop_stops_after_first_refresh_failure() {
    use mtg_gto::card::{DynamicValue, Effect, TokenDef};
    let (mut s, _, _) = fixture();
    let mut d = s.card_db().get(BODY).unwrap().clone();
    d.static_abilities = vec![StaticAbility::ExtraLandDrops { count: 1 }];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    let d = CardDef {
        id: 971003,
        card_types: vec![CardType::Sorcery],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::CreateTokens {
            token: TokenDef {
                name: "review token".into(),
                power: 1,
                toughness: 1,
                colors: vec![],
                subtypes: vec![],
                keywords: vec![],
            },
            count: DynamicValue::Fixed(3),
        }),
        ..Default::default()
    };
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    let spell = s.create_card_in_zone(971003, 0, ZoneType::Hand).unwrap();
    rules::apply_action(
        &mut s,
        &Action::CastSpell {
            object_id: spell,
            targets: vec![],
        },
    );
    s.next_timestamp = u32::MAX;
    for _ in 0..2 {
        rules::apply_action(&mut s, &Action::PassPriority);
    }
    let count = s
        .battlefield
        .iter()
        .filter(|id| s.objects[id].is_token)
        .count();
    println!("TOKENS failure={:?} created={}", s.sba_failure, count);
    assert!(s.sba_failure.is_some());
    assert_eq!(count, 1, "tokens allocated after first failed refresh");
}
#[test]
fn independent_six_stamp_and_restore_boundary_matrix() {
    use mtg_gto::game::{StackSource, Target};
    let (mut s, a, t) = fixture();
    let mut d = s.card_db().get(BODY).unwrap().clone();
    d.static_abilities = vec![StaticAbility::ExtraLandDrops { count: 1 }];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    let mut d = s.card_db().get(AURA).unwrap().clone();
    d.power = Some(2);
    d.toughness = Some(2);
    d.static_abilities = vec![StaticAbility::AddType {
        card_type: CardType::Creature,
        affected: AffectedObjects::AllPermanents,
    }];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(d);
    rules::apply_action(
        &mut s,
        &Action::CastSpell {
            object_id: a,
            targets: vec![Target::Object(t)],
        },
    );
    for budget in 0..=6 {
        let mut json: GameState =
            serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
        json.card_db = s.card_db.clone();
        let mut bin: GameState = bincode::deserialize(&bincode::serialize(&s).unwrap()).unwrap();
        bin.card_db = s.card_db.clone();
        let mut snap = s.clone();
        snap.restore(s.snapshot()).unwrap();
        for mut state in [s.clone(), json, bin, snap] {
            state.next_timestamp = u32::MAX - budget;
            for _ in 0..2 {
                rules::apply_action(&mut state, &Action::PassPriority);
            }
            assert_eq!(state.sba_failure.is_none(), budget == 6);
            assert_eq!(
                state
                    .stack
                    .iter()
                    .any(|e| matches!(e.source,StackSource::AuraSpell{source,..}if source.id==a)),
                budget < 2
            );
            assert_eq!(state.battlefield.contains(&a), (2..6).contains(&budget));
            assert_eq!(
                state.objects[&a].attachment_link().is_some(),
                (2..4).contains(&budget)
            );
            assert_eq!(state.players[0].graveyard.contains(&a), budget == 6);
        }
    }
}

#[test]
fn generated_current_and_historical_references_keep_existing_applicability() {
    for exact_generation in [1, 2] {
        let (mut s, _, target) = fixture();
        s.objects.get_mut(&target).unwrap().zone_change_count = 2;
        let mut body = s.card_db().get(BODY).unwrap().clone();
        body.static_abilities = vec![StaticAbility::Anthem {
            power: 1,
            toughness: 1,
            affected: AffectedObjects::SpecificIncarnation {
                object_id: target,
                zone_change_count: exact_generation,
            },
        }];
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(body);
        s.objects.get_mut(&target).unwrap().plus_counters = 1;
        s.objects.get_mut(&target).unwrap().minus_counters = 1;
        s.invalidate_characteristics_cache();
        rules::check_state_based_actions(&mut s);
        assert!(s.sba_failure.is_none());
        assert_eq!(s.next_timestamp, 2);
        assert_eq!(
            (
                s.objects[&target].plus_counters,
                s.objects[&target].minus_counters
            ),
            (0, 0)
        );
        assert_eq!(s.continuous_effects.len(), 1);
        assert_eq!(
            s.continuous_effects[0].affected,
            AffectedObjects::SpecificIncarnation {
                object_id: target,
                zone_change_count: exact_generation
            }
        );
        assert_eq!(
            s.effective_power(target),
            if exact_generation == 2 { 3 } else { 2 }
        );
    }
}
#[test]
fn predefined_token_loop_stops_on_refresh_failure_and_keeps_successful_counts() {
    use mtg_gto::card::effects::PredefinedToken;
    for exhausted in [false, true] {
        let (mut s, _, _) = fixture();
        let mut body = s.card_db().get(BODY).unwrap().clone();
        body.static_abilities = vec![StaticAbility::ExtraLandDrops { count: 1 }];
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(body);
        let def = CardDef {
            id: 979705,
            card_types: vec![CardType::Sorcery],
            mana_cost: Some(ManaCost::zero()),
            spell_effect: Some(Effect::CreatePredefinedToken {
                token_type: PredefinedToken::Soldier,
                count: 3,
            }),
            ..Default::default()
        };
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(def);
        let spell = s.create_card_in_zone(979705, 0, ZoneType::Hand).unwrap();
        rules::apply_action(
            &mut s,
            &Action::CastSpell {
                object_id: spell,
                targets: vec![],
            },
        );
        if exhausted {
            s.next_timestamp = u32::MAX;
        }
        resolve(&mut s);
        assert_eq!(
            s.battlefield
                .iter()
                .filter(|id| s.objects[id].is_token)
                .count(),
            if exhausted { 1 } else { 3 }
        );
        if exhausted {
            assert_eq!(
                s.invalid_gameplay_reason(),
                Some(TerminationReason::StateEncoding)
            );
            assert!(!s.players[0].graveyard.contains(&spell));
        } else {
            assert!(s.sba_failure.is_none());
            assert!(s.players[0].graveyard.contains(&spell));
        }
    }
}
#[test]
fn annihilator_later_failure_keeps_earlier_departure_and_stops_continuation() {
    let (mut s, _, first) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 979706,
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        annihilator_count: Some(2),
        ..Default::default()
    });
    let attacker = s
        .create_card_in_zone(979706, 0, ZoneType::Battlefield)
        .unwrap();
    s.objects.get_mut(&attacker).unwrap().summoning_sick = false;
    let second = s
        .create_card_in_zone(BODY, 1, ZoneType::Battlefield)
        .unwrap();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 979707,
        card_types: vec![CardType::Artifact],
        static_abilities: vec![StaticAbility::ExtraLandDrops { count: 1 }],
        ..Default::default()
    });
    s.create_card_in_zone(979707, 0, ZoneType::Battlefield)
        .unwrap();
    s.phase = Phase::DeclareAttackers;
    s.next_timestamp = u32::MAX - 1;
    let old_resume = s.trigger_order_resume.clone();
    let old_next_stack = s.next_stack_id;
    rules::apply_action(
        &mut s,
        &Action::DeclareAttackers {
            attackers: vec![attacker],
        },
    );
    assert_eq!(
        s.invalid_gameplay_reason(),
        Some(TerminationReason::StateEncoding)
    );
    assert_eq!(s.players[1].graveyard, vec![first]);
    assert!(s.battlefield.contains(&second));
    assert_eq!(s.objects[&first].zone_change_count, 1);
    assert_eq!(s.objects[&second].zone_change_count, 0);
    assert_eq!(s.next_timestamp, u32::MAX);
    assert_eq!(s.trigger_order_resume, old_resume);
    assert_eq!(s.next_stack_id, old_next_stack);
    assert!(s.stack.is_empty());
}

#[test]
fn successful_self_sacrifice_keeps_existing_cost_and_stack_semantics() {
    use mtg_gto::card::{ActivatedAbility, SacrificeCost};
    let (mut s, _, _) = fixture();
    let mut body = s.card_db().get(BODY).unwrap().clone();
    body.static_abilities = vec![StaticAbility::ExtraLandDrops { count: 1 }];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(body);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 979708,
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::new(1, 0, 0, 0, 0, 0),
            requires_tap: true,
            sacrifice_cost: Some(SacrificeCost::SelfSacrifice),
            life_cost: 1,
            effect: Effect::DrawCards { count: 1 },
            description: String::new(),
        }],
        ..Default::default()
    });
    let source = s
        .create_card_in_zone(979708, 0, ZoneType::Battlefield)
        .unwrap();
    let next_stack = s.next_stack_id;
    s.next_timestamp = u32::MAX - 1;
    rules::apply_action(
        &mut s,
        &Action::ActivateAbility {
            object_id: source,
            ability_index: 0,
            targets: vec![],
        },
    );
    assert!(s.sba_failure.is_none());
    assert_eq!(s.next_timestamp, u32::MAX);
    assert_eq!(s.players[0].mana_pool.colorless, 3);
    assert_eq!(s.players[0].life, 19);
    assert!(s.players[0].graveyard.contains(&source));
    assert!(!s.battlefield.contains(&source));
    assert_eq!(s.objects[&source].zone_change_count, 1);
    assert_eq!(s.next_stack_id, next_stack + 1);
    assert_eq!(s.stack.len(), 1);
    assert!(
        matches!(s.stack[0].source, StackSource::ActivatedAbility { source_id, ability_index: 0 } if source_id == source)
    );
}
#[test]
fn successful_annihilator_preserves_member_order_and_refresh_count() {
    let (mut s, _, first) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 979709,
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        annihilator_count: Some(2),
        ..Default::default()
    });
    let attacker = s
        .create_card_in_zone(979709, 0, ZoneType::Battlefield)
        .unwrap();
    s.objects.get_mut(&attacker).unwrap().summoning_sick = false;
    let second = s
        .create_card_in_zone(BODY, 1, ZoneType::Battlefield)
        .unwrap();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 979710,
        card_types: vec![CardType::Artifact],
        static_abilities: vec![StaticAbility::ExtraLandDrops { count: 1 }],
        ..Default::default()
    });
    s.create_card_in_zone(979710, 0, ZoneType::Battlefield)
        .unwrap();
    s.phase = Phase::DeclareAttackers;
    s.next_timestamp = u32::MAX - 3;
    rules::apply_action(
        &mut s,
        &Action::DeclareAttackers {
            attackers: vec![attacker],
        },
    );
    assert!(s.sba_failure.is_none());
    assert_eq!(s.players[1].graveyard, vec![first, second]);
    assert_eq!(s.next_timestamp, u32::MAX);
    assert_eq!(s.phase, Phase::DeclareBlockers);
    assert!(s.trigger_order_resume.is_none());
}


fn ordinary_enchantment_with_stale_controller() -> (GameState, u64, u64) {
    let (mut s, id, effect_source) = fixture();
    let mut def = s.card_db().get(AURA).unwrap().clone();
    def.subtypes.clear();
    def.enchant = EnchantRestriction::Missing;
    def.static_abilities.clear();
    def.triggered_abilities = vec![TriggeredAbility {
        trigger: TriggerCondition::EntersBattlefield,
        effect: Effect::GainLife { amount: 1 },
        description: "ordinary entry control".into(),
    }];
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(def);
    s.move_object(id, ZoneType::Hand, ZoneType::Battlefield);
    s.objects.get_mut(&id).unwrap().controller = 1;
    s.move_object(id, ZoneType::Battlefield, ZoneType::Hand);
    assert_eq!(s.objects[&id].controller, 1);
    (s, id, effect_source)
}
fn aura_subtype_for_controller(s: &mut GameState, source: u64, controller: usize) {
    s.continuous_effects.push(ContinuousEffect {
        source_id: source,
        controller,
        timestamp: s.next_timestamp,
        duration: Duration::Permanent,
        affected: AffectedObjects::PermanentsControlledBy(controller),
        modification: LayerModification::AddSubtype(Subtype("Aura".into())),
    });
    s.next_timestamp += 1;
    s.invalidate_characteristics_cache();
}
#[test]
fn ordinary_cast_entry_admission_uses_cast_controller_after_restore() {
    for represented_aura_effect in [false, true] {
        let (mut s, id, source) = ordinary_enchantment_with_stale_controller();
        if represented_aura_effect { aura_subtype_for_controller(&mut s, source, 1); }
        let generation = s.objects[&id].zone_change_count;
        let action = Action::CastSpell { object_id: id, targets: vec![] };
        assert!(legal_actions(&s).contains(&action));
        rules::apply_action(&mut s, &action);
        assert_eq!(s.stack.len(), 1);
        assert_eq!(s.stack[0].controller, 0);
        assert_eq!(s.players[0].mana_pool.colorless, 3);
        for mut current in restores(&s) {
            resolve(&mut current);
            assert!(current.sba_failure.is_none());
            assert!(current.battlefield.contains(&id));
            assert!(!current.players[0].hand.contains(&id));
            assert!(!current.players[0].graveyard.contains(&id));
            assert_eq!((current.objects[&id].owner, current.objects[&id].controller), (0, 0));
            assert_eq!(current.objects[&id].zone_change_count, generation + 2);
            assert_eq!(current.stack.len(), 1); // The ordinary entry trigger observes successful entry.
            resolve(&mut current);
            assert_eq!(current.players[0].life, 21);
            assert_eq!(current.players[0].mana_pool.colorless, 3);
            assert!(current.stack.is_empty());
            assert!(current.sba_failure.is_none());
        }
    }
}
#[test]
fn ordinary_cast_new_aura_entry_rejects_before_stack_pop_or_controller_change() {
    let (mut s, id, source) = ordinary_enchantment_with_stale_controller();
    rules::apply_action(&mut s, &Action::CastSpell { object_id: id, targets: vec![] });
    aura_subtype_for_controller(&mut s, source, 0);
    rules::apply_action(&mut s, &Action::PassPriority);
    for mut current in restores(&s) {
        let before = current.clone();
        rules::apply_action(&mut current, &Action::PassPriority);
        assert_eq!(current.invalid_gameplay_reason(), Some(TerminationReason::PreparedSbaFailure));
        failure_only(&current, &before);
        assert_eq!(current.stack.len(), 1);
        assert_eq!(current.objects[&id].controller, 1);
        assert!(!current.battlefield.contains(&id));
        assert_eq!(current.players[0].life, 20);
    }
}
#[test]
fn death_keyword_entry_admission_uses_owner_controller_after_restore() {
    use mtg_gto::rules::transitions::{return_death_keyword, ZoneTriggerContext};
    for keyword in [KeywordAbility::Undying, KeywordAbility::Persist] {
        let (mut s, _, source) = fixture();
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
            id: 979711,
            card_types: vec![CardType::Enchantment, CardType::Creature],
            keywords: vec![keyword], power: Some(2), toughness: Some(2),
            ..Default::default()
        });
        let id = s.create_card_in_zone(979711, 0, ZoneType::Battlefield).unwrap();
        s.objects.get_mut(&id).unwrap().controller = 1;
        let exact = s.exact_object(id).unwrap();
        let batch = transition_batch(&mut s, &[TransitionRequest {
            object: exact, from: ZoneType::Battlefield, to: ZoneType::Graveyard, kind: MovementKind::Put,
        }]).unwrap();
        let subject = batch.transitions[0].clone();
        let context = ZoneTriggerContext { source_before: subject.before.clone(), subject,
            source_was_subject: true, group_id: batch.group_id };
        aura_subtype_for_controller(&mut s, source, 1);
        let generation = s.objects[&id].zone_change_count;
        for mut current in restores(&s) {
            assert_eq!(return_death_keyword(&mut current, &context, keyword), Ok(true));
            assert!(current.sba_failure.is_none());
            assert!(current.battlefield.contains(&id));
            assert!(!current.players[0].graveyard.contains(&id));
            assert_eq!((current.objects[&id].owner, current.objects[&id].controller), (0, 0));
            assert_eq!(current.objects[&id].zone_change_count, generation + 1);
            assert_eq!(current.objects[&id].plus_counters, i32::from(keyword == KeywordAbility::Undying));
            assert_eq!(current.objects[&id].minus_counters, i32::from(keyword == KeywordAbility::Persist));
        }
        aura_subtype_for_controller(&mut s, source, 0);
        for mut current in restores(&s) {
            let before = current.clone();
            assert_eq!(return_death_keyword(&mut current, &context, keyword), Err(TransitionError::UnsupportedPath));
            assert_eq!(bincode::serialize(&current).unwrap(), bincode::serialize(&before).unwrap());
        }
    }
}
#[test]
fn ordinary_direct_cast_entry_preserves_physical_source_under_intended_controller() {
    let (mut s, id, source) = ordinary_enchantment_with_stale_controller();
    aura_subtype_for_controller(&mut s, source, 1);
    rules::apply_action(&mut s, &Action::CastSpell { object_id: id, targets: vec![] });
    assert_eq!(s.stack.len(), 1);
    resolve(&mut s);
    assert!(s.sba_failure.is_none());
    assert!(s.battlefield.contains(&id));
    assert_eq!((s.objects[&id].owner, s.objects[&id].controller), (0, 0));
    assert!(!s.players[0].hand.contains(&id));
    assert!(!s.players[0].graveyard.contains(&id));
    assert_eq!(s.stack.len(), 1); // Successful ETB, awaiting ordinary trigger resolution.
}
#[test]
fn raw_entry_admission_preserves_stored_controller_policy() {
    for represented_aura_effect in [false, true] {
        let (mut s, id, source) = ordinary_enchantment_with_stale_controller();
        if represented_aura_effect { aura_subtype_for_controller(&mut s, source, 1); }
        let before = s.clone();
        let result = s.try_move_object(id, ZoneType::Hand, ZoneType::Battlefield);
        if represented_aura_effect {
            assert_eq!(result, Err(TransitionError::UnsupportedPath));
            assert_eq!(bincode::serialize(&s).unwrap(), bincode::serialize(&before).unwrap());
        } else {
            assert_eq!(result, Ok(()));
            assert!(s.battlefield.contains(&id));
            assert_eq!((s.objects[&id].owner, s.objects[&id].controller), (0, 1));
        }
    }
}
#[test]
fn malformed_ordinary_entry_controller_rejects_before_consuming_the_spell() {
    let (mut s, id, _) = ordinary_enchantment_with_stale_controller();
    rules::apply_action(&mut s, &Action::CastSpell { object_id: id, targets: vec![] });
    s.stack[0].controller = s.players.len();
    rules::apply_action(&mut s, &Action::PassPriority);
    for mut current in restores(&s) {
        let before = current.clone();
        rules::apply_action(&mut current, &Action::PassPriority);
        assert_eq!(current.invalid_gameplay_reason(), Some(TerminationReason::StateEncoding));
        failure_only(&current, &before);
        assert_eq!(current.stack.len(), 1);
        assert_eq!(current.objects[&id].controller, 1);
        assert!(!current.battlefield.contains(&id));
        assert_eq!(current.players[0].life, 20);
    }
}
