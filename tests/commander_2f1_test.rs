use std::sync::Arc;
use mtg_gto::{
    action::{legal_actions, Action, canonical::{canonicalize, resolve}},
    card::{CardDef, CardType, CostIncrease, CostIncreaseTarget, CostReduction, CostReductionTarget,
        Effect, KeywordAbility, TargetSpec, ZoneType},
    game::{CardDatabase, CommanderDesignation, GameState, Phase, Target},
    info_set::InformationSet,
    layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification},
    mana::{ManaCost},
    replacement::{ReplacementAction, ReplacementEffect, ReplacementEventKind}, rules,
};

const COMMANDER: u64 = 999_601;
const OTHER: u64 = 999_602;
const COUNTER: u64 = 999_603;
const REDUCER: u64 = 999_604;
const INCREASER: u64 = 999_605;
const BOLT: u64 = 999_606;

fn fixture() -> GameState {
    let mut db = CardDatabase::new();
    for id in [COMMANDER, OTHER] {
        db.insert(CardDef { id, name: format!("Commander fixture {id}"), card_types: vec![CardType::Creature],
            mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 0)), power: Some(3), toughness: Some(5), ..Default::default() });
    }
    db.insert(CardDef { id: COUNTER, name: "Counter".into(), card_types: vec![CardType::Instant],
        mana_cost: Some(ManaCost::zero()), spell_effect: Some(Effect::Counter { target: TargetSpec::AnySpell }), ..Default::default() });
    db.insert(CardDef { id: BOLT, name: "Damage".into(), card_types: vec![CardType::Sorcery],
        mana_cost: Some(ManaCost::zero()), spell_effect: Some(Effect::DealDamage { amount: 3, target: TargetSpec::AnyPlayer }), ..Default::default() });
    db.insert(CardDef { id: REDUCER, name: "Reduction".into(), card_types: vec![CardType::Artifact],
        cost_reduction: Some(CostReduction { generic_reduction: 1, applies_to: CostReductionTarget::CreatureSpells }), ..Default::default() });
    db.insert(CardDef { id: INCREASER, name: "Increase".into(), card_types: vec![CardType::Artifact],
        cost_increase: Some(CostIncrease { generic_increase: 2, applies_to: CostIncreaseTarget::CreatureSpells, affects_controller: false }), ..Default::default() });
    let mut s = GameState::new_commander(2);
    s.card_db = Some(Arc::new(db)); s.phase = Phase::PreCombatMain;
    s
}
fn designated(s: &mut GameState, owner: usize, role: u8, zone: ZoneType) -> u64 {
    let id = s.create_card_in_zone(COMMANDER, owner, zone).unwrap();
    s.designate_commander(id, role).unwrap(); id
}
fn fund(s: &mut GameState, amount: u32) { s.players[0].mana_pool.colorless += amount; }
fn cast(id: u64) -> Action { Action::CastCommander { object_id: id, targets: vec![] } }
fn pass(s: &mut GameState) { for _ in 0..2 { rules::apply_action(s, &Action::PassPriority); } }
fn info(s: &GameState, player: usize) -> (u64, Vec<u8>) {
    let value = InformationSet::from_view(&s.visible_state(player), s.card_db()).unwrap();
    (value.hash_value(), value.zone_normalization)
}
fn variants(s: &GameState) -> Vec<GameState> {
    let mut snap = s.clone(); snap.restore(s.snapshot()).unwrap();
    let mut all = vec![s.clone(), snap, serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap(),
        bincode::deserialize(&bincode::serialize(s).unwrap()).unwrap()];
    for v in &mut all { v.card_db = s.card_db.clone(); }
    all
}
fn combat(s: &mut GameState, attackers: &[u64]) {
    s.phase = Phase::DeclareBlockers; s.active_player = 0; s.priority_player = 1;
    s.combat.attackers = attackers.to_vec();
    rules::apply_action(s, &Action::DeclareBlockers { blocks: vec![] });
    for _ in 0..4 {
        if s.phase == Phase::CombatDamage || s.gameplay_stopped() { break; }
        pass(s);
    }
    assert_eq!(s.phase, Phase::CombatDamage);
}

#[test]
fn designation_survives_control_characteristics_and_new_incarnations() {
    let mut s = fixture();
    let c = designated(&mut s, 0, 0, ZoneType::Battlefield);
    let copy = s.create_card_in_zone(COMMANDER, 0, ZoneType::Battlefield).unwrap();
    for (object, modification) in [(c, LayerModification::CopyOf(OTHER)), (copy, LayerModification::CopyOf(COMMANDER)),
        (c, LayerModification::ChangeController(1))] {
        s.continuous_effects.push(ContinuousEffect { source_id: object, controller: 0, timestamp: 1,
            duration: Duration::Permanent, affected: AffectedObjects::Specific(object), modification });
    }
    s.invalidate_characteristics_cache();
    assert!(s.is_commander(c)); assert!(!s.is_commander(copy));
    assert_eq!(mtg_gto::layers::compute_characteristics(c, &s.continuous_effects, &s.objects, &s.battlefield.iter().copied().collect(), s.card_db()).unwrap().controller, 1);
    let generation = s.objects[&c].zone_change_count;
    s.move_object(c, ZoneType::Battlefield, ZoneType::Hand);
    assert!(s.is_commander(c));
    s.move_object(c, ZoneType::Hand, ZoneType::Battlefield);
    assert!(s.objects[&c].zone_change_count > generation);
    assert!(s.is_commander(c)); assert!(!s.is_commander(copy));
    assert_eq!(s.commander(0, 0).unwrap().initial_card, COMMANDER);
    for restored in variants(&s) { assert!(restored.is_commander(c)); assert!(!restored.is_commander(copy)); }
}

#[test]
fn shared_admission_tax_increases_reductions_and_partner_independence() {
    let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Command);
    let p = designated(&mut s, 0, 1, ZoneType::Command);
    s.create_card_in_zone(REDUCER, 0, ZoneType::Battlefield).unwrap();
    s.create_card_in_zone(INCREASER, 1, ZoneType::Battlefield).unwrap();
    fund(&mut s, 20);
    assert!(legal_actions(&s).contains(&cast(c)));
    rules::apply_action(&mut s, &cast(c));
    assert_eq!(s.players[0].mana_pool.total(), 17); // 2 + 2 - 1
    assert_eq!(s.commander_casts(0, 0, 0), 1);
    assert_eq!(s.commander_casts(0, 1, 0), 0);
    pass(&mut s);
    s.move_object(c, ZoneType::Battlefield, ZoneType::Command);
    rules::apply_action(&mut s, &cast(c));
    assert_eq!(s.players[0].mana_pool.total(), 12); // 2 + tax 2 + increase 2 - reduction 1
    assert_eq!(s.commander_casts(0, 0, 0), 2);
    pass(&mut s);
    rules::apply_action(&mut s, &cast(p));
    assert_eq!(s.players[0].mana_pool.total(), 9);
    assert_eq!(s.commander_casts(0, 1, 0), 1);
    assert_eq!(s.commander_casts(0, 0, 1), 0);
}

#[test]
fn flash_and_rejected_casts_preserve_state() {
    for mode in 0..7 {
        let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Command); fund(&mut s, 20);
        let proposed = match mode {
            0 => { s.players[0].mana_pool = mtg_gto::mana::ManaPool::empty(); cast(c) }
            1 => { s.phase = Phase::Upkeep; cast(c) }
            2 => { s.priority_player = 1; cast(c) }
            3 => { let id = s.create_card_in_zone(COMMANDER, 0, ZoneType::Command).unwrap(); cast(id) }
            4 => { s.next_stack_id = u64::MAX; cast(c) }
            5 => { s.objects.get_mut(&c).unwrap().zone_change_count = u32::MAX; cast(c) }
            _ => Action::CastCommander { object_id: c, targets: vec![Target::Player(99)] },
        };
        let before = serde_json::to_value(&s).unwrap();
        assert!(!legal_actions(&s).contains(&proposed));
        rules::apply_action(&mut s, &proposed);
        assert_eq!(serde_json::to_value(&s).unwrap(), before, "mode {mode}");
    }
    let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Command); fund(&mut s, 2);
    Arc::make_mut(s.card_db.as_mut().unwrap()).cards.get_mut(&COMMANDER).unwrap().keywords.push(KeywordAbility::Flash);
    s.phase = Phase::Upkeep; s.active_player = 1;
    assert!(legal_actions(&s).contains(&cast(c)));
    rules::apply_action(&mut s, &cast(c));
    assert_eq!(s.commander_casts(0, 0, 0), 1);
    assert_eq!(s.stack.len(), 1);
}

#[test]
fn countering_and_other_zone_casts_do_not_rewrite_history() {
    let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Command); fund(&mut s, 10);
    rules::apply_action(&mut s, &cast(c));
    let target = s.stack[0].id;
    let counter = s.create_card_in_zone(COUNTER, 0, ZoneType::Hand).unwrap();
    let action = Action::CastSpell { object_id: counter, targets: vec![Target::StackEntry(target)] };
    assert!(legal_actions(&s).contains(&action)); rules::apply_action(&mut s, &action); pass(&mut s);
    assert!(s.players[0].command_zone.contains(&c)); // Phase 2 automatic-return limitation retained.
    assert_eq!(s.commander_casts(0, 0, 0), 1);
    rules::apply_action(&mut s, &cast(c));
    assert_eq!(s.players[0].mana_pool.total(), 4);
    pass(&mut s);
    s.move_object(c, ZoneType::Battlefield, ZoneType::Hand);
    rules::apply_action(&mut s, &Action::CastSpell { object_id: c, targets: vec![] });
    assert_eq!(s.commander_casts(0, 0, 0), 2);
}

#[test]
fn actual_combat_damage_infect_and_bounded_prevention() {
    for (replacement, expected) in [(None, 3), (Some(ReplacementAction::Prevent), 0),
        (Some(ReplacementAction::ModifyAmount { delta: -1 }), 2)] {
        for infect in [false, true] {
            let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Battlefield);
            if infect { s.objects.get_mut(&c).unwrap().temp_keywords.push(KeywordAbility::Infect); }
            if let Some(action) = replacement.clone() {
                s.replacement_effects.push(ReplacementEffect { source_id: c, controller: 0,
                    applies_to: ReplacementEventKind::DamageDealt, action, is_self_replacement: true, description: "bounded fixture".into() });
            }
            combat(&mut s, &[c]);
            assert_eq!(s.commander(0, 0).unwrap().damage_to_player[1], expected);
            assert_eq!(s.players[1].life, if infect { 40 } else { 40 - expected as i32 });
            assert_eq!(s.players[1].poison_counters, if infect { expected } else { 0 });
        }
    }
}

#[test]
fn partners_do_not_combine_and_twenty_one_uses_actual_combat() {
    let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Battlefield);
    let p = designated(&mut s, 0, 1, ZoneType::Battlefield);
    s.objects.get_mut(&c).unwrap().temp_power_mod = 8;
    s.objects.get_mut(&p).unwrap().temp_power_mod = 7;
    combat(&mut s, &[c, p]);
    assert_eq!(s.commander(0, 0).unwrap().damage_to_player[1], 11);
    assert_eq!(s.commander(0, 1).unwrap().damage_to_player[1], 10);
    assert!(!s.game_over);
    s.players[1].life = 100;
    s.objects.get_mut(&c).unwrap().temp_power_mod = 6; s.invalidate_characteristics_cache();
    combat(&mut s, &[c]); assert_eq!(s.commander(0, 0).unwrap().damage_to_player[1], 20); assert!(!s.game_over);
    s.move_object(c, ZoneType::Battlefield, ZoneType::Hand); s.move_object(c, ZoneType::Hand, ZoneType::Battlefield);
    s.objects.get_mut(&c).unwrap().temp_power_mod = -2; s.invalidate_characteristics_cache();
    combat(&mut s, &[c]); assert_eq!(s.commander(0, 0).unwrap().damage_to_player[1], 21);
    assert_eq!(s.winner, Some(0));
    assert!(s.loss_boundary.terminal.as_ref().unwrap().losses[0].causes.contains(
        &mtg_gto::game::LossCause::CommanderDamage { commander: CommanderDesignation { owner: 0, role: 0 } }));
}

#[test]
fn control_change_keeps_owner_designation_and_noncombat_does_not_count() {
    let mut s = fixture(); let c = designated(&mut s, 1, 0, ZoneType::Battlefield);
    s.objects.get_mut(&c).unwrap().controller = 0;
    combat(&mut s, &[c]);
    assert_eq!(s.commander(1, 0).unwrap().damage_to_player[1], 3);
    let mut s = fixture(); let c = s.create_card_in_zone(BOLT, 0, ZoneType::Command).unwrap();
    s.designate_commander(c, 0).unwrap();
    let action = Action::CastCommander { object_id: c, targets: vec![Target::Player(1)] };
    rules::apply_action(&mut s, &action); pass(&mut s);
    assert_eq!(s.players[1].life, 37);
    assert_eq!(s.commander(0, 0).unwrap().damage_to_player, vec![0, 0]);
}

#[test]
fn canonical_identity_and_hidden_public_histories() {
    let mut a = fixture(); let ca = designated(&mut a, 0, 0, ZoneType::Command); fund(&mut a, 10);
    let pa = designated(&mut a, 0, 1, ZoneType::Command);
    let mut b = fixture(); b.next_object_id = 900; let cb = designated(&mut b, 0, 0, ZoneType::Command); fund(&mut b, 10);
    let pb = designated(&mut b, 0, 1, ZoneType::Command);
    b.commanders.reverse(); b.players[0].command_zone.reverse();
    for (x, y) in [(ca, cb), (pa, pb)] {
        let canonical = canonicalize(&cast(x), &a).unwrap();
        assert_eq!(canonical, canonicalize(&cast(y), &b).unwrap());
        assert_eq!(resolve(&canonical, &b, 0).unwrap(), Some(cast(y)));
    }
    assert_eq!(info(&a, 0), info(&b, 0));
    a.move_object(ca, ZoneType::Command, ZoneType::Library);
    b.move_object(cb, ZoneType::Command, ZoneType::Library);
    b.objects.get_mut(&cb).unwrap().card_def_id = OTHER;
    b.objects.get_mut(&cb).unwrap().zone_change_count += 4;
    assert_eq!(info(&a, 1), info(&b, 1));
    b.commander_mut(0, 0).unwrap().casts_by_player[0] = 2;
    assert_ne!(info(&a, 1), info(&b, 1));
    b.commander_mut(0, 0).unwrap().casts_by_player[0] = 0;
    b.commander_mut(0, 0).unwrap().damage_to_player[1] = 20;
    assert_ne!(info(&a, 1), info(&b, 1));
}

#[test]
fn persistence_preserves_both_commanders_and_all_ledgers() {
    let mut s = fixture(); designated(&mut s, 0, 0, ZoneType::Command); designated(&mut s, 0, 1, ZoneType::Hand);
    s.commander_mut(0, 0).unwrap().casts_by_player = vec![2, 1];
    s.commander_mut(0, 1).unwrap().damage_to_player = vec![5, 20];
    for v in variants(&s) {
        assert_eq!(v.commanders, s.commanders);
        assert_eq!(info(&v, 1), info(&s, 1));
        assert_eq!(legal_actions(&v), legal_actions(&s));
    }
}

#[test]
fn malformed_bindings_and_ledgers_rejected_by_restore_and_consumers() {
    let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Command);
    assert!(s.designate_commander(c, 1).is_err());
    for mode in 0..8 {
        let mut bad = s.clone();
        match mode {
            0 => bad.commanders[0].object = 99999,
            1 => bad.commanders.push(bad.commanders[0].clone()),
            2 => bad.commanders[0].designation.owner = 1,
            3 => bad.commanders[0].designation.role = 2,
            4 => bad.objects.get_mut(&c).unwrap().is_token = true,
            5 => bad.commanders[0].damage_to_player.pop().map(|_| ()).unwrap(),
            6 => bad.commanders[0].casts_by_player[0] = u32::MAX,
            _ => bad.players[0].command_zone.push(c),
        }
        assert!(bad.validate_commanders().is_err());
        assert!(serde_json::from_slice::<GameState>(&serde_json::to_vec(&bad).unwrap()).is_err());
        assert!(bincode::deserialize::<GameState>(&bincode::serialize(&bad).unwrap()).is_err());
        assert!(s.clone().restore(bad.snapshot()).is_err());
        assert!(legal_actions(&bad).is_empty());
        assert!(InformationSet::from_view(&bad.visible_state(0), bad.card_db()).is_err());
    }
    let mut old = serde_json::to_value(&s).unwrap(); old["state_format"] = serde_json::json!(0x3245_4754u32);
    assert!(serde_json::from_value::<GameState>(old).is_err());
    let mut old = bincode::serialize(&s).unwrap(); old[..4].copy_from_slice(&0x3245_4754u32.to_le_bytes());
    assert!(bincode::deserialize::<GameState>(&old).is_err());
}

#[test]
fn affinity_reduces_printed_cost_and_tax_without_expanding_payments() {
    let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Command);
    Arc::make_mut(s.card_db.as_mut().unwrap()).cards.get_mut(&COMMANDER).unwrap().keywords.push(KeywordAbility::AffinityForArtifacts);
    s.create_card_in_zone(INCREASER, 0, ZoneType::Battlefield).unwrap();
    s.commander_mut(0, 0).unwrap().casts_by_player[0] = 1;
    fund(&mut s, 3);
    assert!(legal_actions(&s).contains(&cast(c)));
    rules::apply_action(&mut s, &cast(c));
    assert_eq!(s.players[0].mana_pool.total(), 0);
    assert_eq!(s.commander_casts(0, 0, 0), 2);
}

#[test]
fn spell_copy_is_not_a_cast_and_does_not_acquire_designation() {
    let mut s = fixture(); let c = s.create_card_in_zone(BOLT, 0, ZoneType::Command).unwrap();
    s.designate_commander(c, 0).unwrap();
    rules::apply_action(&mut s, &Action::CastCommander { object_id: c, targets: vec![Target::Player(1)] });
    let stack_id = s.stack[0].id;
    rules::copy_stack_spell(&mut s, stack_id, 0, rules::CopyTargetPolicy::Preserve).unwrap();
    assert_eq!(s.commanders.len(), 1);
    assert_eq!(s.commander_casts(0, 0, 0), 1);
    pass(&mut s); pass(&mut s);
    assert_eq!(s.players[1].life, 34);
    assert_eq!(s.commander_casts(0, 0, 0), 1);
    assert_eq!(s.commander(0, 0).unwrap().damage_to_player, vec![0, 0]);
}

#[test]
fn flash_commander_remains_available_in_supported_cleanup_priority() {
    let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Command); fund(&mut s, 2);
    Arc::make_mut(s.card_db.as_mut().unwrap()).cards.get_mut(&COMMANDER).unwrap().keywords.push(KeywordAbility::Flash);
    s.phase = Phase::Cleanup; s.cleanup_needs_repeat = true;
    assert!(legal_actions(&s).contains(&cast(c)));
    rules::apply_action(&mut s, &cast(c));
    assert_eq!(s.commander_casts(0, 0, 0), 1);
    assert_eq!(s.stack.len(), 1);
}

#[test]
fn replacement_zero_damage_cannot_be_revived_and_capacity_fails_before_results() {
    for deltas in [vec![-3, 10], vec![i32::MAX]] {
        let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Battlefield);
        for &delta in &deltas {
            s.replacement_effects.push(ReplacementEffect { source_id: c, controller: 0,
                applies_to: ReplacementEventKind::DamageDealt, action: ReplacementAction::ModifyAmount { delta },
                is_self_replacement: true, description: "bounded amount".into() });
        }
        combat(&mut s, &[c]);
        assert_eq!(s.players[1].life, 40);
        assert_eq!(s.commander(0, 0).unwrap().damage_to_player[1], 0);
        assert_eq!(s.sba_failure.is_some(), deltas[0] == i32::MAX);
    }
}

#[test]
fn commander_loss_provenance_is_registry_order_independent() {
    let mut s = fixture(); designated(&mut s, 0, 0, ZoneType::Command); designated(&mut s, 0, 1, ZoneType::Command);
    for c in &mut s.commanders { c.damage_to_player[1] = 21; }
    let mut reversed = s.clone(); reversed.commanders.reverse();
    rules::check_state_based_actions(&mut s); rules::check_state_based_actions(&mut reversed);
    assert_eq!(s.loss_boundary.terminal, reversed.loss_boundary.terminal);
}

#[test]
fn partner_setup_does_not_depend_on_library_input_order() {
    let mut s = fixture();
    let mut deck = vec![OTHER, COMMANDER]; deck.extend([REDUCER; 10]);
    rules::setup_commander_game_with_partners(&mut s, &deck, &deck, COMMANDER, Some(OTHER), COMMANDER, Some(OTHER));
    assert_eq!(s.commanders.len(), 4);
    for owner in 0..2 {
        assert_eq!(s.commander(owner, 0).unwrap().initial_card, COMMANDER);
        assert_eq!(s.commander(owner, 1).unwrap().initial_card, OTHER);
    }
    assert!(s.validate_commanders().is_ok());
}

#[test]
fn unsupported_damage_replacement_stops_without_inventing_results() {
    let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Battlefield);
    s.replacement_effects.push(ReplacementEffect { source_id: c, controller: 0,
        applies_to: ReplacementEventKind::DamageDealt, action: ReplacementAction::Prevent,
        is_self_replacement: false, description: "requires ordering support".into() });
    combat(&mut s, &[c]);
    assert_eq!(s.players[1].life, 40);
    assert_eq!(s.commander(0, 0).unwrap().damage_to_player[1], 0);
    assert_eq!(s.sba_failure, Some(rules::sba::PreparedPassFailure::Transition(rules::transitions::TransitionError::UnsupportedPath)));
}

#[test]
fn extreme_tax_and_colored_cost_reject_without_payment_overflow() {
    let mut s = fixture(); let c = designated(&mut s, 0, 0, ZoneType::Command);
    Arc::make_mut(s.card_db.as_mut().unwrap()).cards.get_mut(&COMMANDER).unwrap().mana_cost = Some(ManaCost::new(2, 0, 2, 0, 0, 0));
    s.commander_mut(0, 0).unwrap().casts_by_player[0] = 2_147_483_646;
    let before = serde_json::to_value(&s).unwrap();
    assert!(!legal_actions(&s).contains(&cast(c)));
    rules::apply_action(&mut s, &cast(c));
    assert_eq!(serde_json::to_value(&s).unwrap(), before);
}
