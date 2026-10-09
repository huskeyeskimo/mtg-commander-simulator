use mtg_gto::{
    action::{legal_actions, Action},
    card::{CardDef, CardType, KeywordAbility, Subtype, ZoneType},
    game::{CardDatabase, GameState, Phase},
    layers::{AffectedObjects, StaticAbility},
    mana::ManaCost,
    rules,
};
use std::sync::Arc;

fn fixture() -> (GameState, u64, u64, u64) {
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
        name: "Synthetic equipment".into(),
        card_types: vec![CardType::Artifact],
        subtypes: vec![Subtype("Equipment".into())],
        equip_cost: Some(ManaCost::new(1, 0, 0, 0, 0, 0)),
        static_abilities: vec![StaticAbility::Anthem {
            power: 1,
            toughness: 0,
            affected: AffectedObjects::AttachedTo,
        }],
        ..Default::default()
    });
    let mut s = GameState::new(2);
    s.card_db = Some(Arc::new(db));
    s.phase = Phase::PreCombatMain;
    let e = s.create_card_in_zone(2, 0, ZoneType::Battlefield);
    let a = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let b = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
    s.players[0].mana_pool.colorless = 5;
    (s, e, a, b)
}
fn equip(s: &mut GameState, e: u64, c: u64) {
    rules::apply_action(
        s,
        &Action::Equip {
            equipment_id: e,
            target_id: c,
        },
    );
}
fn resolve(s: &mut GameState) {
    for _ in 0..s.players.len() {
        rules::apply_action(s, &Action::PassPriority);
    }
}
fn established(s: &mut GameState, e: u64, c: u64) {
    let p = s
        .prepare_attach(
            s.exact_object(e).unwrap(),
            s.exact_object(c).unwrap(),
            mtg_gto::card::AttachmentContext::Established,
        )
        .unwrap();
    s.commit_attach(p).unwrap();
}
#[test]
fn activation_is_pending_without_attachment_or_bonus() {
    let (mut s, e, a, _) = fixture();
    let ts = s.next_timestamp;
    equip(&mut s, e, a);
    assert_eq!(s.stack.len(), 1);
    assert!(s.objects[&e].attachment_link().is_none());
    assert_eq!(s.effective_power(a), 2);
    assert_eq!(s.next_timestamp, ts);
    assert_eq!(s.players[0].mana_pool.colorless, 4);
    assert_eq!(s.priority_player, 0);
    resolve(&mut s);
    assert_eq!(s.effective_power(a), 3);
    assert_eq!(s.players[0].mana_pool.colorless, 4);
}
#[test]
fn pending_reattachment_preserves_a() {
    let (mut s, e, a, b) = fixture();
    established(&mut s, e, a);
    equip(&mut s, e, b);
    assert_eq!(
        s.attachment_target(s.exact_object(e).unwrap()),
        s.exact_object(a)
    );
    assert_eq!(s.effective_power(a), 3);
    resolve(&mut s);
    assert_eq!(s.effective_power(a), 2);
    assert_eq!(s.effective_power(b), 3);
}
#[test]
fn wrong_phase_activation_is_byte_identical() {
    let (mut s, e, a, _) = fixture();
    s.phase = Phase::Upkeep;
    let before = bincode::serialize(&s).unwrap();
    equip(&mut s, e, a);
    assert_eq!(bincode::serialize(&s).unwrap(), before);
}
#[test]
fn shroud_activation_is_byte_identical_and_not_generated() {
    let (mut s, e, a, _) = fixture();
    s.objects
        .get_mut(&a)
        .unwrap()
        .temp_keywords
        .push(KeywordAbility::Shroud);
    s.invalidate_characteristics_cache();
    let before = bincode::serialize(&s).unwrap();
    equip(&mut s, e, a);
    assert_eq!(bincode::serialize(&s).unwrap(), before);
    assert!(!legal_actions(&s).contains(&Action::Equip {
        equipment_id: e,
        target_id: a
    }));
}
#[test]
fn target_blink_defeats_pending_resolution() {
    let (mut s, e, a, _) = fixture();
    equip(&mut s, e, a);
    assert_eq!(s.stack.len(), 1);
    s.move_object(a, ZoneType::Battlefield, ZoneType::Exile);
    s.move_object(a, ZoneType::Exile, ZoneType::Battlefield);
    resolve(&mut s);
    assert!(s.objects[&e].attachment_link().is_none());
    assert_eq!(s.effective_power(a), 2);
}
#[test]
fn source_blink_defeats_pending_resolution() {
    let (mut s, e, a, _) = fixture();
    equip(&mut s, e, a);
    assert_eq!(s.stack.len(), 1);
    s.move_object(e, ZoneType::Battlefield, ZoneType::Exile);
    s.move_object(e, ZoneType::Exile, ZoneType::Battlefield);
    resolve(&mut s);
    assert!(s.objects[&e].attachment_link().is_none());
}
#[test]
fn reattachment_refreshes_timestamp_but_same_target_does_not() {
    let (mut s, e, a, b) = fixture();
    established(&mut s, e, a);
    let old = s.objects[&e].attachment_link().unwrap().timestamp;
    equip(&mut s, e, b);
    resolve(&mut s);
    let new = s.objects[&e].attachment_link().unwrap().timestamp;
    assert!(new > old);
    equip(&mut s, e, b);
    resolve(&mut s);
    assert_eq!(s.objects[&e].attachment_link().unwrap().timestamp, new);
}

fn effect(
    s: &mut GameState,
    source: u64,
    target: u64,
    modification: mtg_gto::layers::LayerModification,
) {
    let timestamp = s.new_timestamp();
    s.continuous_effects
        .push(mtg_gto::layers::ContinuousEffect {
            source_id: source,
            controller: 0,
            timestamp,
            duration: mtg_gto::layers::Duration::Permanent,
            affected: AffectedObjects::Specific(target),
            modification,
        });
    s.invalidate_characteristics_cache();
}
fn variants(s: &GameState) -> Vec<GameState> {
    let mut snap = s.clone();
    snap.restore(s.snapshot()).unwrap();
    let mut result = vec![
        s.clone(),
        snap,
        serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap(),
        bincode::deserialize(&bincode::serialize(s).unwrap()).unwrap(),
    ];
    for candidate in &mut result {
        candidate.card_db = s.card_db.clone();
    }
    result
}
fn info(s: &GameState) -> mtg_gto::info_set::InformationSet {
    mtg_gto::info_set::InformationSet::from_view(&s.visible_state(0), s.card_db()).unwrap()
}

#[test]
fn timing_control_ability_and_payment_rejections_preserve_all_state() {
    use mtg_gto::layers::LayerModification as M;
    for case in 0..12 {
        let (mut s, e, a, _) = fixture();
        match case {
            0 => s.priority_player = 1,
            1 => s.active_player = 1,
            2 => s.phase = Phase::EndStep,
            3 => s.objects.get_mut(&e).unwrap().controller = 1,
            4 => s.objects.get_mut(&a).unwrap().controller = 1,
            5 => effect(&mut s, e, e, M::RemoveAllAbilities),
            6 => effect(&mut s, e, a, M::RemoveType(CardType::Creature)),
            7 => s.players[0].mana_pool.colorless = 0,
            8 => {
                s.pending_tutor = Some(mtg_gto::game::PendingTutor {
                    controller: 0,
                    destination: ZoneType::Hand,
                    subtype_filter: vec![],
                })
            }
            9 => {
                s.move_object(e, ZoneType::Battlefield, ZoneType::Exile);
            }
            10 => {
                equip(&mut s, e, a);
            }
            _ => {
                s.cleanup_discard_in_progress = true;
            }
        }
        s.invalidate_characteristics_cache();
        let before = bincode::serialize(&s).unwrap();
        equip(&mut s, e, a);
        assert_eq!(before, bincode::serialize(&s).unwrap(), "case={case}");
        assert!(
            !legal_actions(&s).contains(&Action::Equip {
                equipment_id: e,
                target_id: a
            }),
            "case={case}"
        );
    }
}
#[test]
fn both_main_phases_tapped_and_sick_equipment_and_friendly_hexproof_are_legal() {
    for phase in [Phase::PreCombatMain, Phase::PostCombatMain] {
        let (mut s, e, a, _) = fixture();
        s.phase = phase;
        s.objects.get_mut(&e).unwrap().tapped = true;
        s.objects
            .get_mut(&a)
            .unwrap()
            .temp_keywords
            .push(KeywordAbility::Hexproof);
        s.invalidate_characteristics_cache();
        assert!(legal_actions(&s).contains(&Action::Equip {
            equipment_id: e,
            target_id: a
        }));
        equip(&mut s, e, a);
        assert_eq!(s.stack.len(), 1);
        resolve(&mut s);
        assert_eq!(s.effective_power(a), 3);
    }
}
#[test]
fn computed_control_and_creature_type_drive_generation_and_activation() {
    use mtg_gto::layers::LayerModification as M;
    let (mut s, e, a, _) = fixture();
    s.objects.get_mut(&e).unwrap().controller = 1;
    s.objects.get_mut(&a).unwrap().controller = 1;
    effect(&mut s, e, e, M::ChangeController(0));
    effect(&mut s, e, a, M::ChangeController(0));
    assert!(legal_actions(&s).contains(&Action::Equip {
        equipment_id: e,
        target_id: a
    }));
    equip(&mut s, e, a);
    resolve(&mut s);
    assert_eq!(s.effective_power(a), 3);
    let (mut s, e, a, _) = fixture();
    // A printed noncreature becomes a represented creature without replacing its definition.
    let t = s.create_card_in_zone(2, 0, ZoneType::Battlefield);
    effect(&mut s, a, t, M::AddType(CardType::Creature));
    effect(&mut s, a, t, M::SetPT(3, 3));
    assert!(legal_actions(&s).contains(&Action::Equip {
        equipment_id: e,
        target_id: t
    }));
    equip(&mut s, e, t);
    resolve(&mut s);
    assert_eq!(s.effective_power(t), 4);
}
#[test]
fn pending_controller_is_fixed_and_ability_loss_does_not_erase_it() {
    use mtg_gto::layers::LayerModification as M;
    let (mut s, e, a, _) = fixture();
    equip(&mut s, e, a);
    effect(&mut s, e, e, M::ChangeController(1));
    effect(&mut s, e, e, M::RemoveAllAbilities);
    assert_eq!(s.stack[0].controller, 0);
    resolve(&mut s);
    assert_eq!(
        s.attachment_target(s.exact_object(e).unwrap()),
        s.exact_object(a)
    );
}
#[test]
fn illegal_target_at_resolution_preserves_existing_attachment_and_payment() {
    use mtg_gto::layers::LayerModification as M;
    for case in 0..4 {
        let (mut s, e, a, b) = fixture();
        established(&mut s, e, a);
        let ts = s.objects[&e].attachment_link().unwrap().timestamp;
        equip(&mut s, e, b);
        match case {
            0 => effect(&mut s, e, b, M::AddKeyword(KeywordAbility::Shroud)),
            1 => effect(&mut s, e, b, M::ChangeController(1)),
            2 => effect(&mut s, e, b, M::RemoveType(CardType::Creature)),
            _ => {
                s.move_object(b, ZoneType::Battlefield, ZoneType::Exile);
            }
        }
        resolve(&mut s);
        assert_eq!(
            s.attachment_target(s.exact_object(e).unwrap()),
            s.exact_object(a)
        );
        assert_eq!(s.objects[&e].attachment_link().unwrap().timestamp, ts);
        assert_eq!(s.players[0].mana_pool.colorless, 4);
        assert!(!s.gameplay_stopped());
    }
}
#[test]
fn animated_equipment_and_self_target_can_activate_but_cannot_attach() {
    use mtg_gto::layers::LayerModification as M;
    for self_target in [false, true] {
        let (mut s, e, a, _) = fixture();
        effect(&mut s, a, e, M::AddType(CardType::Creature));
        effect(&mut s, a, e, M::SetPT(3, 3));
        let target = if self_target { e } else { a };
        assert!(legal_actions(&s).contains(&Action::Equip {
            equipment_id: e,
            target_id: target
        }));
        equip(&mut s, e, target);
        assert_eq!(s.stack.len(), 1);
        assert_eq!(s.players[0].mana_pool.colorless, 4);
        resolve(&mut s);
        assert!(s.objects[&e].attachment_link().is_none());
        assert!(!s.gameplay_stopped());
    }
}
#[test]
fn computed_equipment_subtype_loss_detaches_in_common_sba_and_defeats_pending() {
    use mtg_gto::layers::LayerModification as M;
    let (mut s, e, a, b) = fixture();
    established(&mut s, e, a);
    equip(&mut s, e, b);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 3,
        name: "Plain artifact".into(),
        card_types: vec![CardType::Artifact],
        ..Default::default()
    });
    effect(&mut s, a, e, M::CopyOf(3));
    rules::check_state_based_actions(&mut s);
    assert!(s.objects[&e].attachment_link().is_none());
    resolve(&mut s);
    assert!(s.objects[&e].attachment_link().is_none());
    assert!(!s.gameplay_stopped());
}
#[test]
fn pending_equip_is_excluded_from_spell_counter_and_copy_paths() {
    let (mut s, e, a, _) = fixture();
    equip(&mut s, e, a);
    let id = s.stack[0].id;
    assert!(!s.stack[0].source.is_spell());
    assert!(!mtg_gto::targeting::target_is_legal(
        &s,
        1,
        &mtg_gto::card::TargetSpec::AnySpell,
        &mtg_gto::game::Target::StackEntry(id)
    ));
    assert!(rules::copy_stack_spell(&mut s, id, 0, rules::CopyTargetPolicy::Preserve).is_err());
    assert_eq!(s.stack.len(), 1);
    resolve(&mut s);
    assert_eq!(s.effective_power(a), 3);
}
#[test]
fn pending_exact_endpoints_survive_partial_pass_departure_blink_and_roundtrips() {
    for case in 0..7 {
        let (mut s, e, a, _) = fixture();
        equip(&mut s, e, a);
        assert_eq!(s.stack.len(), 1);
        rules::apply_action(&mut s, &Action::PassPriority);
        assert_eq!(s.consecutive_passes, 1);
        assert_eq!(s.priority_player, 1);
        if case > 0 {
            let id = if case % 2 == 1 { e } else { a };
            s.move_object(id, ZoneType::Battlefield, ZoneType::Exile);
            if case >= 3 {
                s.move_object(id, ZoneType::Exile, ZoneType::Battlefield);
            }
            if case >= 5 {
                s.objects.get_mut(&id).unwrap().is_token = true;
                s.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
                rules::check_state_based_actions(&mut s);
                assert!(!s.objects.contains_key(&id));
            }
        }
        let mut outcomes = vec![];
        for mut restored in variants(&s) {
            assert_eq!(info(&restored).hash_value(), info(&s).hash_value());
            rules::apply_action(&mut restored, &Action::PassPriority);
            assert!(restored.stack.is_empty());
            assert_eq!(restored.priority_player, 0);
            assert!(!restored.gameplay_stopped());
            if let Some(eq) = restored.objects.get(&e) {
                assert_eq!(eq.attachment_link().is_some(), case == 0);
            }
            assert_eq!(restored.players[0].mana_pool.colorless, 4);
            outcomes.push(serde_json::to_value(restored).unwrap());
        }
        assert!(outcomes.iter().all(|x| *x == outcomes[0]), "case={case}");
    }
}
#[test]
fn noncommuting_static_order_refreshes_in_place_and_preserves_independent_effects() {
    use mtg_gto::layers::{ContinuousEffect, Duration, LayerModification as M};
    let (mut s, e, a, b) = fixture();
    let db = Arc::make_mut(s.card_db.as_mut().unwrap());
    let mut eq = db.get(2).unwrap().clone();
    eq.static_abilities = vec![
        StaticAbility::SetPowerToughness {
            power: 4,
            toughness: 4,
            affected: AffectedObjects::AttachedTo,
        },
        StaticAbility::SetPowerToughness {
            power: 6,
            toughness: 6,
            affected: AffectedObjects::AttachedTo,
        },
    ];
    db.insert(eq);
    s.refresh_continuous_effects();
    effect(&mut s, a, b, M::SetPT(9, 9));
    for duration in [Duration::Permanent, Duration::UntilEndOfTurn] {
        let timestamp = s.new_timestamp();
        s.continuous_effects.push(ContinuousEffect {
            source_id: e,
            controller: 0,
            timestamp,
            duration,
            affected: AffectedObjects::AttachedTo,
            modification: M::ModifyPT(0, 0),
        });
    }
    let original = s.continuous_effects.clone();
    equip(&mut s, e, b);
    assert_eq!(s.continuous_effects, original);
    resolve(&mut s);
    assert_eq!(
        s.effective_power(b),
        6,
        "fresh first attachment overrides older competing SetPT; source order retained"
    );
    let first = s.continuous_effects.clone();
    assert_eq!(first.len(), original.len());
    for (before, after) in original.iter().zip(&first) {
        let mut unchanged = after.clone();
        unchanged.timestamp = before.timestamp;
        assert_eq!(unchanged, *before);
        if before.duration != Duration::WhileSourceOnBattlefield || before.source_id != e {
            assert_eq!(before, after);
        }
    }
    assert!(first[0].timestamp < first[1].timestamp);
    effect(&mut s, a, b, M::SetPT(11, 11));
    assert_eq!(s.effective_power(b), 11);
    let before = s.continuous_effects.clone();
    let ts = s.objects[&e].attachment_link().unwrap().timestamp;
    equip(&mut s, e, b);
    resolve(&mut s);
    assert_eq!(s.continuous_effects, before);
    assert_eq!(s.objects[&e].attachment_link().unwrap().timestamp, ts);
    assert_eq!(s.effective_power(b), 11);
    effect(&mut s, b, a, M::SetPT(12, 12));
    equip(&mut s, e, a);
    resolve(&mut s);
    assert_eq!(s.effective_power(a), 6);
    assert_eq!(s.effective_power(b), 11);
}

fn allocated(offset: u64, reverse: bool) -> (GameState, u64, u64, u64) {
    let (base, _, _, _) = fixture();
    let mut s = GameState::new(2);
    s.card_db = base.card_db;
    s.phase = Phase::PreCombatMain;
    s.next_object_id = offset;
    s.next_stack_id = offset + 50;
    s.players[0].mana_pool.colorless = 5;
    let (e, a, b) = if reverse {
        let b = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
        let a = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
        let e = s.create_card_in_zone(2, 0, ZoneType::Battlefield);
        (e, a, b)
    } else {
        (
            s.create_card_in_zone(2, 0, ZoneType::Battlefield),
            s.create_card_in_zone(1, 0, ZoneType::Battlefield),
            s.create_card_in_zone(1, 0, ZoneType::Battlefield),
        )
    };
    for id in [e, a, b] {
        s.objects.get_mut(&id).unwrap().zone_change_count = offset as u32;
    }
    (s, e, a, b)
}
fn abstractions(s: &GameState) -> [u64; 3] {
    use mtg_gto::info_set::{
        BucketedAbstraction, CardAwareBucketedAbstraction, InfoSetAbstraction,
    };
    let i = info(s);
    [
        i.hash_value(),
        BucketedAbstraction.abstract_info_set(&i),
        CardAwareBucketedAbstraction {
            card_db: s.card_db(),
        }
        .abstract_info_set(&i),
    ]
}
#[test]
fn pending_and_historical_equips_preserve_all_abstractions_and_complete_action_multiplicity() {
    use mtg_gto::{action::canonical, public_projection::JointPublicNormalization};
    for stage in 0..5 {
        let mut results = vec![];
        for (offset, rev) in [(1, false), (100, true), (230, false)] {
            let (mut s, e, a, b) = allocated(offset, rev);
            // Distinguish the selected endpoint semantically while permuting allocation.
            s.objects.get_mut(&a).unwrap().plus_counters = 1;
            s.invalidate_characteristics_cache();
            if stage == 4 {
                established(&mut s, e, b);
            }
            equip(&mut s, e, a);
            assert_eq!(s.stack.len(), 1);
            if stage > 0 {
                s.move_object(a, ZoneType::Battlefield, ZoneType::Exile);
            }
            if stage == 2 {
                s.move_object(a, ZoneType::Exile, ZoneType::Battlefield);
            }
            if stage == 3 {
                s.objects.get_mut(&a).unwrap().is_token = true;
                rules::check_state_based_actions(&mut s);
                assert!(!s.objects.contains_key(&a));
            }
            let n = JointPublicNormalization::for_state(&s, 0).unwrap();
            let actions = legal_actions(&s);
            let mut keys = canonical::canonicalize_actions(&actions, &s, &n).unwrap();
            assert_eq!(keys.len(), actions.len());
            for (action, key) in actions.iter().zip(&keys) {
                assert_eq!(
                    canonical::resolve_with_normalization(key, &s, 0, &n).unwrap(),
                    Some(action.clone())
                );
            }
            keys.sort_by_cached_key(|key| format!("{key:?}"));
            results.push((abstractions(&s), n.encoding.clone(), keys));
        }
        assert!(results.iter().all(|x| *x == results[0]), "stage={stage}");
    }
}
#[test]
fn pending_equip_never_rebinds_hidden_newer_current_source_or_target() {
    for source in [false, true] {
        let (mut s, e, a, _) = fixture();
        equip(&mut s, e, a);
        let id = if source { e } else { a };
        s.move_object(id, ZoneType::Battlefield, ZoneType::Library);
        let baseline = abstractions(&s);
        let encoding = mtg_gto::public_projection::JointPublicNormalization::for_state(&s, 0)
            .unwrap()
            .encoding
            .clone();
        s.objects.get_mut(&id).unwrap().card_def_id = if source { 1 } else { 2 };
        s.objects.get_mut(&id).unwrap().temp_toughness_mod = 37;
        s.invalidate_characteristics_cache();
        assert_eq!(baseline, abstractions(&s));
        assert_eq!(
            encoding,
            mtg_gto::public_projection::JointPublicNormalization::for_state(&s, 0)
                .unwrap()
                .encoding
        );
        resolve(&mut s);
        assert!(s.objects[&e].attachment_link().is_none());
        assert!(!s.gameplay_stopped());
    }
}
#[test]
fn malformed_owned_equip_metadata_routes_state_encoding_without_resolution_mutation() {
    use mtg_gto::{
        game::{StackSource, Target},
        simulation::TerminationReason,
    };
    for case in 0..9 {
        let (mut s, e, a, _) = fixture();
        equip(&mut s, e, a);
        match case {
            0 => s.stack[0].targets.clear(),
            1 => s.stack[0].target_generations.clear(),
            2 => s.stack[0].target_generations[0] = None,
            3 => s.stack[0].controller = 8,
            4 => {
                if let StackSource::EquipAbility { target_card_id, .. } = &mut s.stack[0].source {
                    *target_card_id = 2;
                }
            }
            5 => {
                if let StackSource::EquipAbility { source, .. } = &mut s.stack[0].source {
                    source.generation += 1;
                }
            }
            6 => s.stack[0].targets = vec![Target::Player(0)],
            7 => {
                if let StackSource::EquipAbility { source_card_id, .. } = &mut s.stack[0].source {
                    *source_card_id = 1;
                }
            }
            _ => {
                // Malformed multiple-pending representation, not reachable activation gameplay.
                s.stack.push(s.stack[0].clone());
            }
        }
        let before = bincode::serialize(&s).unwrap();
        assert!(matches!(
            mtg_gto::public_projection::JointPublicNormalization::for_state(&s, 0),
            Err(TerminationReason::StateEncoding)
        ));
        assert_eq!(before, bincode::serialize(&s).unwrap());
        rules::apply_action(&mut s, &Action::PassPriority);
        assert_eq!(
            s.invalid_gameplay_reason(),
            Some(TerminationReason::StateEncoding)
        );
        s.sba_failure = None;
        assert_eq!(before, bincode::serialize(&s).unwrap());
    }
}

fn lethal_fixture(token: bool, observers: bool) -> (GameState, u64, u64) {
    use mtg_gto::card::{Effect, TriggerCondition, TriggeredAbility};
    let (mut s, e, a, b) = fixture();
    s.move_object(b, ZoneType::Battlefield, ZoneType::Exile);
    let db = Arc::make_mut(s.card_db.as_mut().unwrap());
    let mut eq = db.get(2).unwrap().clone();
    eq.static_abilities = vec![StaticAbility::Anthem {
        power: 1,
        toughness: -1,
        affected: AffectedObjects::AttachedTo,
    }];
    eq.triggered_abilities = vec![TriggeredAbility {
        trigger: TriggerCondition::EquippedCreatureDies,
        effect: Effect::DrawCards { count: 1 },
        description: "synthetic equipped death".into(),
    }];
    db.insert(eq);
    let mut body = db.get(1).unwrap().clone();
    body.power = Some(1);
    body.toughness = Some(1);
    db.insert(body);
    db.insert(CardDef {
        id: 3,
        name: "Death observer".into(),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::ACreatureDies,
            effect: Effect::GainLife { amount: 1 },
            description: "watch".into(),
        }],
        ..Default::default()
    });
    if token {
        s.objects.get_mut(&a).unwrap().is_token = true;
    }
    for _ in 0..4 {
        s.create_card_in_zone(1, 0, ZoneType::Library);
    }
    if observers {
        s.create_card_in_zone(3, 0, ZoneType::Battlefield);
        s.create_card_in_zone(3, 1, ZoneType::Battlefield);
        s.create_card_in_zone(3, 1, ZoneType::Battlefield);
    }
    s.continuous_effects.retain(|effect| {
        effect.source_id != e
            || effect.duration != mtg_gto::layers::Duration::WhileSourceOnBattlefield
    });
    s.refresh_continuous_effects();
    s.invalidate_characteristics_cache();
    (s, e, a)
}
#[test]
fn resolution_death_captures_exact_history_detaches_and_draws_once_after_token_cessation() {
    use mtg_gto::game::StackSource;
    for token in [false, true] {
        let (mut s, e, a) = lethal_fixture(token, false);
        equip(&mut s, e, a);
        assert!(s.battlefield.contains(&a));
        assert_eq!(s.effective_toughness(a), 1);
        assert_eq!(s.stack.len(), 1);
        assert!(s.pending_triggers.is_empty());
        for mut pending in variants(&s) {
            resolve(&mut pending);
            assert!(!pending.battlefield.contains(&a));
            assert!(pending.objects[&e].attachment_link().is_none());
            assert_eq!(pending.stack.len(), 1);
            assert_eq!(pending.priority_player, 0);
            let StackSource::TriggeredAbility { context, .. } = &pending.stack[0].source else {
                panic!("ordinary death trigger")
            };
            let zone = context.zone_transition.as_ref().unwrap();
            assert_eq!(zone.subject.before.object.id, a);
            assert_eq!(zone.source_before.attachment.links.len(), 1);
            assert_eq!(zone.source_before.attachment.links[0].source.id, e);
            assert_eq!(zone.source_before.attachment.links[0].target.id, a);
            assert_eq!(pending.objects.contains_key(&a), !token);
            for mut after in variants(&pending) {
                let before = serde_json::to_value(&after).unwrap();
                rules::check_state_based_actions(&mut after);
                assert_eq!(before, serde_json::to_value(&after).unwrap());
                let hand = after.players[0].hand.len();
                resolve(&mut after);
                assert_eq!(after.players[0].hand.len(), hand + 1);
                assert!(after.stack.is_empty());
                assert!(!after.gameplay_stopped());
            }
        }
    }
}
#[test]
fn resolution_apnap_mandatory_order_roundtrips_then_restores_priority() {
    let (mut s, e, a) = lethal_fixture(true, true);
    equip(&mut s, e, a);
    resolve(&mut s);
    assert_eq!(s.pending_triggers.len(), 4);
    assert_eq!(s.priority_player, 0);
    assert!(s.stack.is_empty());
    let chooser = legal_actions(&s)
        .into_iter()
        .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    let before = bincode::serialize(&s).unwrap();
    equip(&mut s, e, a);
    assert_eq!(before, bincode::serialize(&s).unwrap());
    for mut restored in variants(&s) {
        rules::apply_action(&mut restored, &chooser);
        assert_eq!(restored.stack.len(), 2);
        assert_eq!(restored.priority_player, 1);
        let next = legal_actions(&restored)
            .into_iter()
            .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
            .unwrap();
        rules::apply_action(&mut restored, &next);
        assert_eq!(restored.stack.len(), 4);
        assert_eq!(restored.priority_player, 0);
        assert!(restored.pending_triggers.is_empty());
        assert_eq!(
            restored
                .stack
                .iter()
                .map(|x| x.controller)
                .collect::<Vec<_>>(),
            vec![0, 0, 1, 1]
        );
        for _ in 0..4 {
            resolve(&mut restored);
        }
        assert_eq!(restored.players[0].hand.len(), 1);
        assert!(restored.stack.is_empty());
    }
}

#[test]
fn pending_equip_survives_unrelated_mandatory_continuation_across_all_restores() {
    use mtg_gto::card::{Effect, TriggerCondition, TriggeredAbility};
    let (mut s, e, a, _) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 3,
        name: "Unrelated victim".into(),
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
    equip(&mut s, e, a);
    let pending_id = s.stack[0].id;
    for _ in 0..2 {
        let v = s.create_card_in_zone(3, 0, ZoneType::Battlefield);
        s.objects.get_mut(&v).unwrap().temp_toughness_mod = -1;
    }
    s.invalidate_characteristics_cache();
    rules::check_state_based_actions(&mut s);
    assert_eq!(s.pending_triggers.len(), 2);
    assert_eq!(s.stack.len(), 1);
    assert!(s.objects[&e].attachment_link().is_none());
    for mut candidate in variants(&s) {
        let choice = legal_actions(&candidate)
            .into_iter()
            .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
            .unwrap();
        let before = bincode::serialize(&candidate).unwrap();
        equip(&mut candidate, e, a);
        assert_eq!(before, bincode::serialize(&candidate).unwrap());
        rules::apply_action(&mut candidate, &choice);
        assert_eq!(candidate.stack.len(), 3);
        assert_eq!(candidate.stack[0].id, pending_id);
        resolve(&mut candidate);
        resolve(&mut candidate);
        assert_eq!(candidate.stack.len(), 1);
        assert!(candidate.objects[&e].attachment_link().is_none());
        resolve(&mut candidate);
        assert_eq!(candidate.effective_power(a), 3);
        assert_eq!(candidate.players[0].mana_pool.colorless, 4);
        assert_eq!(candidate.priority_player, 0);
    }
}
#[test]
fn equip_actions_reconstruct_completely_before_activation_across_allocation_and_computed_control() {
    use mtg_gto::{action::canonical, public_projection::JointPublicNormalization};
    let mut sets = vec![];
    for (offset, reverse) in [(1, false), (90, true), (400, false)] {
        let (s, e, _, _) = allocated(offset, reverse);
        let n = JointPublicNormalization::for_state(&s, 0).unwrap();
        let actions = legal_actions(&s);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a,Action::Equip {equipment_id,..} if *equipment_id==e))
                .count(),
            2
        );
        let mut keys = canonical::canonicalize_actions(&actions, &s, &n).unwrap();
        assert_eq!(keys.len(), actions.len());
        for (action, key) in actions.iter().zip(&keys) {
            assert_eq!(
                canonical::resolve_with_normalization(key, &s, 0, &n).unwrap(),
                Some(action.clone())
            );
        }
        keys.sort_by_cached_key(|key| format!("{key:?}"));
        sets.push(keys);
    }
    assert!(sets.iter().all(|set| *set == sets[0]));
}
#[cfg(feature = "tui")]
#[test]
fn tui_pending_display_cache_and_undo_retain_owned_identity() {
    let (mut s, e, a, _) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 3,
        name: "UI response".into(),
        card_types: vec![CardType::Instant],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(mtg_gto::card::Effect::GainLife { amount: 1 }),
        ..Default::default()
    });
    s.create_card_in_zone(3, 0, ZoneType::Hand);
    let db = s.card_db().clone();
    let mut app = mtg_gto::tui::App::new(s, db);
    let action = Action::Equip {
        equipment_id: e,
        target_id: a,
    };
    let idx = app
        .cached_actions
        .iter()
        .position(|x| *x == action)
        .unwrap();
    app.execute_action(idx);
    assert_eq!(app.state.stack.len(), 1);
    assert!(app.state.objects[&e].attachment_link().is_none());
    let display = mtg_gto::tui::format_stack_entry(&app.state, &app.state.stack[0], &app.db);
    assert!(display.contains("Synthetic equipment"));
    assert!(display.contains("Body"));
    assert!(display.starts_with("Equip:"));
    app.undo();
    assert!(app.state.stack.is_empty());
    assert_eq!(app.state.players[0].mana_pool.colorless, 5);
    assert!(app.cached_actions.contains(&action));
    let idx = app
        .cached_actions
        .iter()
        .position(|x| *x == action)
        .unwrap();
    app.execute_action(idx);
    let idx = app
        .cached_actions
        .iter()
        .position(|x| matches!(x, Action::PassPriority))
        .unwrap();
    app.execute_action(idx);
    assert!(app.state.stack.is_empty());
    assert_eq!(app.state.effective_power(a), 3);
    assert_eq!(app.state.players[0].mana_pool.colorless, 4);
}

// Separate bounded diagnostic; same stage measurements and first-32 reconstruction
// methodology as production_performance_diagnostics. No changed thresholds.
#[test]
#[ignore = "bounded pending-Equip production diagnostics; run explicitly with --ignored --nocapture"]
fn pending_equip_performance_diagnostics() {
    use mtg_gto::{
        action::canonical, layers::LayerModification as M,
        public_projection::JointPublicNormalization,
    };
    for (case, n) in [
        ("isolated", 1),
        ("candidates", 8),
        ("identical_token_heavy", 24),
        ("attached_same_target_cycle", 8),
        ("reattachment_effect_relationships", 12),
        ("cyclic_public_incidence", 8),
    ] {
        let (mut s, e, a, b) = fixture();
        if case == "isolated" {
            let db = Arc::make_mut(s.card_db.as_mut().unwrap());
            let mut eq = db.get(2).unwrap().clone();
            eq.static_abilities.clear();
            db.insert(eq);
            s.move_object(b, ZoneType::Battlefield, ZoneType::Exile);
            s.continuous_effects.clear();
        }
        for _ in 2..n {
            let id = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
            if case == "identical_token_heavy" {
                s.objects.get_mut(&id).unwrap().is_token = true;
            }
        }
        if case == "identical_token_heavy" {
            s.objects.get_mut(&a).unwrap().is_token = true;
            s.objects.get_mut(&b).unwrap().is_token = true;
        }
        s.refresh_continuous_effects();
        let target = if case == "attached_same_target_cycle" {
            established(&mut s, e, a);
            a
        } else if case == "reattachment_effect_relationships" || case == "cyclic_public_incidence" {
            established(&mut s, e, a);
            effect(&mut s, b, a, M::ModifyPT(0, 0));
            b
        } else {
            a
        };
        if case == "cyclic_public_incidence" {
            let e2 = s.create_card_in_zone(2, 0, ZoneType::Battlefield);
            established(&mut s, e2, b);
            effect(&mut s, a, b, M::ModifyPT(0, 0));
        }
        equip(&mut s, e, target);
        assert_eq!(s.stack.len(), 1);
        let normalization = JointPublicNormalization::for_state(&s, 0).unwrap();
        let d = &normalization.diagnostics;
        let started = std::time::Instant::now();
        let info = mtg_gto::info_set::InformationSet::from_view_with_normalization(
            &s.visible_state(0),
            s.card_db(),
            &normalization,
        )
        .unwrap();
        std::hint::black_box(info.hash_value());
        let info_ns = started.elapsed().as_nanos();
        let actions = legal_actions(&s);
        let started = std::time::Instant::now();
        let keys = canonical::canonicalize_actions(&actions, &s, &normalization).unwrap();
        let action_ns = started.elapsed().as_nanos();
        let started = std::time::Instant::now();
        for key in keys.iter().take(32) {
            std::hint::black_box(
                canonical::resolve_with_normalization(key, &s, 0, &normalization).unwrap(),
            );
        }
        let reconstruct_ns = started.elapsed().as_nanos();
        println!("{{\"case\":\"{case}\",\"n\":{n},\"vertices\":{},\"edges\":\"{:?}\",\"components\":{},\"rounds\":{},\"cells\":\"{:?}\",\"nodes\":{},\"candidates\":{},\"inventory_ns\":{},\"components_ns\":{},\"tree_ns\":{},\"cyclic_ns\":{},\"witness_ns\":{},\"supplement_ns\":{},\"total_ns\":{},\"info_ns\":{info_ns},\"actions\":{},\"action_ns\":{action_ns},\"reconstruct_first_32_ns\":{reconstruct_ns}}}",d.vertices,d.edges_by_type,normalization.components.len(),d.refinement_rounds,d.unresolved_cell_sizes,d.exact_search_nodes,d.candidate_encodings,d.inventory_nanos,d.component_nanos,d.tree_nanos,d.cyclic_nanos,d.witness_nanos,d.supplement_nanos,d.total_nanos,keys.len());
    }
}

#[test]
fn pending_malformed_equips_preserve_consumer_state_and_propagate_encoding_failure() {
    use mtg_gto::{
        action::canonical,
        info_set::BucketedAbstraction,
        simulation::TerminationReason,
        solver::{
            mcts::{MctsConfig, MctsStrategy},
            RegretTable,
        },
        strategy::{AbstractedMcfrStrategy, McfrStrategy, Strategy},
    };
    let (mut s, e, a, _) = fixture();
    equip(&mut s, e, a);
    s.stack[0].target_generations[0] = None;
    let before = bincode::serialize(&s).unwrap();
    let failure = TerminationReason::StateEncoding;
    assert_eq!(
        canonical::canonicalize(&Action::PassPriority, &s),
        Err(failure)
    );
    assert!(matches!(
        mtg_gto::info_set::InformationSet::from_view(&s.visible_state(0), s.card_db()),
        Err(TerminationReason::StateEncoding)
    ));
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
        assert_eq!(policy.choose_action(&s, 0), Err(failure));
    }
    let legal = legal_actions(&s);
    assert_eq!(
        mtg_gto::simulation::apply_counted_action(&mut s, &Action::PassPriority, &legal),
        Err(failure)
    );
    assert_eq!(before, bincode::serialize(&s).unwrap());
    assert!(!s.game_over);
}
#[test]
fn counted_activation_and_responsive_spell_continue_without_repayment() {
    use mtg_gto::{card::Effect, game::Target, simulation::apply_counted_action};
    let (mut s, e, a, _) = fixture();
    let action = Action::Equip {
        equipment_id: e,
        target_id: a,
    };
    let actions = legal_actions(&s);
    assert!(apply_counted_action(&mut s, &action, &actions).unwrap());
    assert_eq!(s.loss_boundary.accepted_actions, 1);
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 3,
        name: "Response".into(),
        card_types: vec![CardType::Instant],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::GainLife { amount: 1 }),
        ..Default::default()
    });
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 4,
        name: "Spell counter".into(),
        card_types: vec![CardType::Instant],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::Counter {
            target: mtg_gto::card::TargetSpec::AnySpell,
        }),
        ..Default::default()
    });
    let counter = s.create_card_in_zone(4, 0, ZoneType::Hand);
    let before = bincode::serialize(&s).unwrap();
    let entry = s.stack[0].id;
    rules::apply_action(
        &mut s,
        &Action::CastSpell {
            object_id: counter,
            targets: vec![Target::StackEntry(entry)],
        },
    );
    assert_eq!(before, bincode::serialize(&s).unwrap());
    let spell = s.create_card_in_zone(3, 0, ZoneType::Hand);
    rules::apply_action(
        &mut s,
        &Action::CastSpell {
            object_id: spell,
            targets: vec![],
        },
    );
    assert_eq!(s.stack.len(), 2);
    rules::apply_action(&mut s, &Action::PassPriority);
    for mut restored in variants(&s) {
        rules::apply_action(&mut restored, &Action::PassPriority);
        assert_eq!(restored.players[0].life, 21);
        assert_eq!(restored.stack.len(), 1);
        assert!(restored.objects[&e].attachment_link().is_none());
        let actions = legal_actions(&restored);
        assert!(apply_counted_action(&mut restored, &Action::PassPriority, &actions).unwrap());
        let actions = legal_actions(&restored);
        assert!(apply_counted_action(&mut restored, &Action::PassPriority, &actions).unwrap());
        assert_eq!(restored.effective_power(a), 3);
        assert_eq!(restored.players[0].mana_pool.colorless, 4);
    }
}
#[test]
fn activation_preflight_preserves_auto_tap_sources_and_successful_cost_is_paid_once() {
    use mtg_gto::card::ManaAbility;
    let (mut s, e, a, _) = fixture();
    Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
        id: 3,
        name: "Mana land".into(),
        card_types: vec![CardType::Land],
        mana_abilities: vec![ManaAbility::TapForColor(mtg_gto::mana::Color::Green)],
        ..Default::default()
    });
    let land = s.create_card_in_zone(3, 0, ZoneType::Battlefield);
    s.players[0].mana_pool.colorless = 0;
    s.objects
        .get_mut(&a)
        .unwrap()
        .temp_keywords
        .push(KeywordAbility::Shroud);
    s.invalidate_characteristics_cache();
    let before = bincode::serialize(&s).unwrap();
    equip(&mut s, e, a);
    assert_eq!(before, bincode::serialize(&s).unwrap());
    assert!(!s.objects[&land].tapped);
    s.objects.get_mut(&a).unwrap().temp_keywords.clear();
    s.invalidate_characteristics_cache();
    equip(&mut s, e, a);
    assert_eq!(s.stack.len(), 1);
    assert!(s.objects[&land].tapped);
    assert_eq!(s.players[0].mana_pool.total(), 0);
    resolve(&mut s);
    assert_eq!(s.effective_power(a), 3);
    assert!(s.objects[&land].tapped);
    assert_eq!(s.players[0].mana_pool.total(), 0);
}
#[test]
fn death_trigger_uses_current_source_controller_not_prior_activation_controller() {
    let (mut s, e, a) = lethal_fixture(false, false);
    s.create_card_in_zone(1, 1, ZoneType::Library);
    equip(&mut s, e, a);
    assert_eq!(s.stack[0].controller, 0);
    effect(
        &mut s,
        e,
        e,
        mtg_gto::layers::LayerModification::ChangeController(1),
    );
    resolve(&mut s);
    assert_eq!(s.stack.len(), 1);
    assert_eq!(s.stack[0].controller, 1);
    resolve(&mut s);
    assert_eq!(s.players[0].hand.len(), 0);
    assert_eq!(s.players[1].hand.len(), 1);
}

#[test]
fn malformed_activation_preflight_preserves_resources_and_accepted_action_accounting() {
    use mtg_gto::simulation::TerminationReason;
    for case in 0..5 {
        let (mut s, e, a, _) = fixture();
        match case {
            0 => s.objects.get_mut(&e).unwrap().object_id += 100,
            1 => s.battlefield.push(a),
            2 => {
                s.objects.remove(&a);
            }
            3 => s.objects.get_mut(&a).unwrap().card_def_id = 999,
            _ => s.next_stack_id = u64::MAX,
        }
        let before = bincode::serialize(&s).unwrap();
        equip(&mut s, e, a);
        assert_eq!(
            s.invalid_gameplay_reason(),
            Some(TerminationReason::StateEncoding)
        );
        assert_eq!(s.loss_boundary.accepted_actions, 0);
        s.sba_failure = None;
        assert_eq!(before, bincode::serialize(&s).unwrap());
    }
}

#[test]
fn solver_policies_and_bounded_mccfr_traversal_continue_pending_equip() {
    use mtg_gto::{
        info_set::BucketedAbstraction,
        solver::{
            mccfr::{run_iteration, McfrConfig},
            mcts::{MctsConfig, MctsStrategy},
            RegretTable,
        },
        strategy::{AbstractedMcfrStrategy, McfrStrategy, Strategy},
    };
    let (mut s, e, a, _) = fixture();
    equip(&mut s, e, a);
    let before = bincode::serialize(&s).unwrap();
    let mut tables = [RegretTable::new(), RegretTable::new()];
    run_iteration(
        &s,
        &mut tables,
        &McfrConfig {
            max_depth: 1,
            max_actions: 20,
            max_nodes_per_iteration: 0,
        },
    )
    .unwrap();
    assert!(tables
        .iter()
        .any(|table| table.num_info_sets().unwrap() > 0));
    assert_eq!(before, bincode::serialize(&s).unwrap());
    let mut malformed = s.clone();
    malformed.stack[0].target_generations[0] = None;
    assert_eq!(
        run_iteration(
            &malformed,
            &mut tables,
            &McfrConfig {
                max_depth: 1,
                max_actions: 20,
                max_nodes_per_iteration: 0
            }
        ),
        Err(mtg_gto::simulation::TerminationReason::StateEncoding)
    );
    for table in &tables {
        assert!(table.num_info_sets().is_err());
        assert!(
            table.to_bytes().is_err(),
            "failed attempt cannot be published"
        );
    }
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
    for priority in [0, 1] {
        for policy in &policies {
            assert_eq!(
                policy.choose_action(&s, priority).unwrap(),
                Action::PassPriority
            );
        }
        rules::apply_action(&mut s, &Action::PassPriority);
    }
    assert!(s.stack.is_empty());
    assert_eq!(s.effective_power(a), 3);
    assert_eq!(s.players[0].mana_pool.colorless, 4);
}

#[test]
fn review_dual_zone_equip_endpoints_reject_before_payment_and_accounting() {
    use mtg_gto::{
        game::{StackEntry, StackSource},
        simulation::TerminationReason,
    };
    for source in [false, true] {
        for seat in 0..2 {
            for zone in 0..6 {
                let (mut s, e, a, _) = fixture();
                let id = if source { e } else { a };
                match zone {
                    0 => s.players[seat].hand.push(id),
                    1 => s.players[seat].library.push(id),
                    2 => s.players[seat].graveyard.push(id),
                    3 => s.players[seat].exile.push(id),
                    4 => s.players[seat].command_zone.push(id),
                    _ => {
                        let stack_id = s.new_stack_id();
                        s.stack.push(StackEntry {
                            id: stack_id,
                            source: StackSource::Spell(id),
                            controller: seat,
                            targets: vec![],
                            target_generations: vec![],
                        });
                    }
                }
                // The actor's visible duplicate was the audit reproduction.
                // Other hidden-zone duplicates need authoritative validation
                // without extending what a public projection may inspect.
                if seat == 0 && zone == 0 {
                    assert!(
                        mtg_gto::public_projection::JointPublicNormalization::for_state(&s, 0)
                            .is_err()
                    );
                }
                let before = bincode::serialize(&s).unwrap();
                equip(&mut s, e, a);
                assert_eq!(
                    s.invalid_gameplay_reason(),
                    Some(TerminationReason::StateEncoding),
                    "source={source} seat={seat} zone={zone}"
                );
                s.sba_failure = None;
                assert_eq!(before, bincode::serialize(&s).unwrap());
            }
        }
    }
}
#[test]
fn review_coherent_departed_equip_endpoints_remain_ordinary_rejection() {
    for source in [false, true] {
        for zone in [
            ZoneType::Hand,
            ZoneType::Library,
            ZoneType::Graveyard,
            ZoneType::Exile,
            ZoneType::Command,
        ] {
            let (mut s, e, a, _) = fixture();
            let id = if source { e } else { a };
            s.move_object(id, ZoneType::Battlefield, zone);
            let before = bincode::serialize(&s).unwrap();
            equip(&mut s, e, a);
            assert_eq!(before, bincode::serialize(&s).unwrap());
            assert!(!s.gameplay_stopped());
        }
    }
}
fn review_owned_fixture(role: u8, conflict: bool) -> GameState {
    use mtg_gto::game::{StackSource, Target};
    let (mut s, e, a, _) = fixture();
    let db = Arc::make_mut(s.card_db.as_mut().unwrap());
    let mut eq = db.get(2).unwrap().clone();
    eq.id = 3;
    eq.name = "Other equipment definition".into();
    db.insert(eq);
    let mut body = db.get(1).unwrap().clone();
    body.id = 4;
    body.name = "Other body definition".into();
    db.insert(body);
    equip(&mut s, e, a);
    let departed = if role == 1 { a } else { e };
    s.move_object(departed, ZoneType::Battlefield, ZoneType::Exile);
    s.objects.remove(&departed);
    s.players[0].exile.retain(|id| *id != departed);
    // Two distinct pending Equip occurrences are synthetic representation coverage,
    // not reachable through ordinary sorcery-speed activations.
    let mut other = s.stack[0].clone();
    other.id = s.new_stack_id();
    if let StackSource::EquipAbility {
        source,
        source_card_id,
        target_card_id,
    } = &mut other.source
    {
        match role {
            0 => {
                if conflict {
                    *source_card_id = 3;
                }
            }
            1 => {
                if conflict {
                    *target_card_id = 4;
                }
            }
            _ => {
                let e2 = s.create_card_in_zone(3, 0, ZoneType::Battlefield);
                *source = s.exact_object(e2).unwrap();
                *source_card_id = 3;
                *target_card_id = if conflict { 3 } else { 2 };
                other.targets = vec![Target::Object(e)];
                other.target_generations = vec![Some(0)];
            }
        }
    }
    s.stack.push(other);
    s
}
#[test]
fn review_conflicting_owned_exact_equip_facts_stop_action_and_sba_without_mutation() {
    use mtg_gto::simulation::TerminationReason;
    for role in 0..3 {
        let s = review_owned_fixture(role, true);
        assert!(mtg_gto::public_projection::JointPublicNormalization::for_state(&s, 0).is_err());
        for mut candidate in variants(&s) {
            let before = bincode::serialize(&candidate).unwrap();
            rules::apply_action(&mut candidate, &Action::PassPriority);
            assert_eq!(
                candidate.invalid_gameplay_reason(),
                Some(TerminationReason::StateEncoding),
                "role={role}"
            );
            candidate.sba_failure = None;
            assert_eq!(before, bincode::serialize(&candidate).unwrap());
        }
        let mut candidate = s.clone();
        let before = bincode::serialize(&candidate).unwrap();
        rules::check_state_based_actions(&mut candidate);
        assert_eq!(
            candidate.invalid_gameplay_reason(),
            Some(TerminationReason::StateEncoding)
        );
        candidate.sba_failure = None;
        assert_eq!(before, bincode::serialize(&candidate).unwrap());
    }
}
#[test]
fn review_shared_owned_exact_equip_history_remains_well_formed_and_stale() {
    for role in 0..3 {
        let s = review_owned_fixture(role, false);
        for mut candidate in variants(&s) {
            let observation = abstractions(&candidate);
            assert_eq!(observation, abstractions(&s));
            resolve(&mut candidate);
            assert_eq!(candidate.stack.len(), 1);
            assert!(!candidate.gameplay_stopped());
            resolve(&mut candidate);
            assert!(candidate.stack.is_empty());
            assert!(!candidate.gameplay_stopped());
            assert_eq!(candidate.players[0].mana_pool.colorless, 4);
        }
    }
}

fn generation_board(n: usize, equipment: bool) -> GameState {
    let (mut s, e, _, _) = fixture();
    // No static records: this diagnostic isolates action generation, not joint
    // normalization or attachment-effect materialization.
    let db = Arc::make_mut(s.card_db.as_mut().unwrap());
    let mut eq = db.get(2).unwrap().clone();
    eq.static_abilities.clear();
    db.insert(eq);
    s.continuous_effects.clear();
    if !equipment {
        s.move_object(e, ZoneType::Battlefield, ZoneType::Exile);
    }
    for _ in 2..n {
        s.create_card_in_zone(1, 0, ZoneType::Battlefield);
    }
    for id in s.battlefield.clone() {
        if s.objects[&id].card_def_id == 1 {
            s.objects.get_mut(&id).unwrap().is_token = true;
        }
    }
    s.invalidate_characteristics_cache();
    s
}
#[test]
fn review_no_equipment_generation_preserves_complete_action_vector() {
    for n in [64, 128, 256, 512] {
        let s = generation_board(n, false);
        assert_eq!(
            legal_actions(&s),
            vec![Action::PassPriority, Action::EndTurn]
        );
    }
}
#[test]
fn review_mixed_generation_preserves_order_multiplicity_and_computed_eligibility() {
    use mtg_gto::{
        action::canonical, layers::LayerModification as M,
        public_projection::JointPublicNormalization,
    };
    for mode in 0..5 {
        let (mut s, e, a, b) = fixture();
        Arc::make_mut(s.card_db.as_mut().unwrap()).insert(CardDef {
            id: 3,
            name: "Mana artifact".into(),
            card_types: vec![CardType::Artifact],
            mana_abilities: vec![mtg_gto::card::ManaAbility::TapForColorless],
            ..Default::default()
        });
        let e2 = s.create_card_in_zone(2, 1, ZoneType::Battlefield);
        let mana = s.create_card_in_zone(3, 0, ZoneType::Battlefield);
        let c = s.create_card_in_zone(1, 1, ZoneType::Battlefield);
        effect(&mut s, mana, e2, M::ChangeController(0));
        effect(&mut s, mana, c, M::ChangeController(0));
        effect(&mut s, mana, a, M::AddKeyword(KeywordAbility::Hexproof));
        match mode {
            1 => {
                effect(&mut s, mana, e, M::AddType(CardType::Creature));
                effect(&mut s, mana, e, M::SetPT(2, 2));
            }
            2 => effect(&mut s, mana, e, M::RemoveAllAbilities),
            3 => effect(&mut s, mana, b, M::AddKeyword(KeywordAbility::Shroud)),
            4 => effect(&mut s, mana, e, M::ChangeController(1)),
            _ => {}
        }
        let mut expected = vec![Action::PassPriority, Action::EndTurn];
        let sources = if mode == 2 || mode == 4 {
            vec![e2]
        } else {
            vec![e, e2]
        };
        let targets = match mode {
            1 => vec![e, a, b, c],
            3 => vec![a, c],
            _ => vec![a, b, c],
        };
        for equipment_id in sources {
            for &target_id in &targets {
                expected.push(Action::Equip {
                    equipment_id,
                    target_id,
                });
            }
        }
        expected.push(Action::ActivateManaAbility {
            object_id: mana,
            ability_index: 0,
        });
        let actions = legal_actions(&s);
        assert_eq!(actions, expected, "mode={mode}");
        let normalization = JointPublicNormalization::for_state(&s, 0).unwrap();
        let keys = canonical::canonicalize_actions(&actions, &s, &normalization).unwrap();
        assert_eq!(keys.len(), actions.len());
        for (action, key) in actions.iter().zip(&keys) {
            assert_eq!(
                canonical::resolve_with_normalization(key, &s, 0, &normalization).unwrap(),
                Some(action.clone())
            );
        }
        if mode == 1 {
            assert!(actions.contains(&Action::Equip {
                equipment_id: e,
                target_id: e
            }));
        }
    }
}
// Local generation measurement added after review. It deliberately times only
// legal_actions, unlike pending_equip_performance_diagnostics (normalization).
// No normalization algorithm, production benchmark methodology or threshold changes.
#[test]
#[ignore = "bounded local action-generation measurements; run explicitly with --ignored --nocapture"]
fn review_action_generation_diagnostics() {
    for equipment in [false, true] {
        for n in [64, 128, 256, 512] {
            let s = generation_board(n, equipment);
            let start = std::time::Instant::now();
            let actions = legal_actions(&s);
            let elapsed = start.elapsed();
            assert_eq!(actions.len(), if equipment { n + 2 } else { 2 });
            println!(
                "generation equipment={equipment} creatures={n} actions={} elapsed_ms={:.3}",
                actions.len(),
                elapsed.as_secs_f64() * 1000.0
            );
        }
    }
}
#[test]
fn review_pending_endpoint_membership_rejects_before_pass_or_sba_mutation() {
    use mtg_gto::simulation::TerminationReason;
    for source in [false, true] {
        for damage in 0..3 {
            let (mut s, e, a, _) = fixture();
            equip(&mut s, e, a);
            let id = if source { e } else { a };
            match damage {
                0 => s.players[1].hand.push(id),
                1 => s.battlefield.retain(|object| *object != id),
                _ => {
                    s.objects.remove(&id);
                }
            }
            let mut restored = s.clone();
            let restore_result = restored.restore(s.snapshot());
            // A dangling battlefield resident is already rejected by snapshot
            // restoration. Preserve that contract as well as action preflight.
            if damage == 2 {
                assert_eq!(
                    restore_result,
                    Err(mtg_gto::card::AttachmentError::Membership)
                );
            }
            let candidates = vec![
                s.clone(),
                restored,
                serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap(),
                bincode::deserialize(&bincode::serialize(&s).unwrap()).unwrap(),
            ];
            for mut candidate in candidates {
                candidate.card_db = s.card_db.clone();
                let before = bincode::serialize(&candidate).unwrap();
                rules::apply_action(&mut candidate, &Action::PassPriority);
                assert_eq!(
                    candidate.invalid_gameplay_reason(),
                    Some(TerminationReason::StateEncoding)
                );
                candidate.sba_failure = None;
                assert_eq!(bincode::serialize(&candidate).unwrap(), before);
                rules::check_state_based_actions(&mut candidate);
                assert_eq!(
                    candidate.invalid_gameplay_reason(),
                    Some(TerminationReason::StateEncoding)
                );
                candidate.sba_failure = None;
                assert_eq!(bincode::serialize(&candidate).unwrap(), before);
            }
        }
    }
}
