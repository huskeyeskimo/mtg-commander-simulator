use std::collections::HashSet;
use std::sync::Arc;

use mtg_gto::action::canonical::{canonicalize, resolve};
use mtg_gto::action::{legal_actions, Action};
use mtg_gto::card::sample::{self, ids};
use mtg_gto::card::{CardDef, CardType, Effect, KeywordAbility, Subtype, Supertype,
    TargetSpec, TriggerCondition, ZoneType};
use mtg_gto::game::{GameState, Phase, StackSource, Target};
use mtg_gto::info_set::InformationSet;
use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
use mtg_gto::mana::ManaCost;
use mtg_gto::rules::{self, CopyTargetPolicy};

const HASTE_DRAW: u64 = 990_001;
const HASTE_DRAW_SORCERY: u64 = 990_002;
const DRAW: u64 = 990_003;
const TARGETED_ENCHANTMENT: u64 = 990_004;
const CREATURE: u64 = 990_005;
const ARTIFACT: u64 = 990_006;
const COUNTER: u64 = 990_007;

fn fixture_spell(id: u64, card_type: CardType, effect: Effect) -> CardDef {
    CardDef { id, name: format!("Synthetic spell {id}"),
        card_types: vec![card_type], mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(effect), ..Default::default() }
}

fn haste_draw() -> Effect {
    Effect::Multiple(vec![
        Effect::GainKeywordUntilEOT {
            keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature,
        },
        Effect::DrawCards { count: 1 },
    ])
}

fn game() -> (GameState, u64) { game_with_id_offset(0) }

fn game_with_id_offset(id_offset: usize) -> (GameState, u64) {
    let mut db = sample::build_sample_db();
    db.insert(fixture_spell(HASTE_DRAW, CardType::Instant, haste_draw()));
    db.insert(fixture_spell(HASTE_DRAW_SORCERY, CardType::Sorcery, haste_draw()));
    db.insert(fixture_spell(DRAW, CardType::Instant, Effect::DrawCards { count: 1 }));
    db.insert(fixture_spell(TARGETED_ENCHANTMENT, CardType::Enchantment, haste_draw()));
    db.insert(fixture_spell(COUNTER, CardType::Instant,
        Effect::Counter { target: TargetSpec::AnySpell }));
    db.insert(CardDef { id: CREATURE, name: "Same-name creature".into(),
        card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
        ..Default::default() });
    db.insert(CardDef { id: ARTIFACT, name: "Layered artifact".into(),
        card_types: vec![CardType::Artifact], ..Default::default() });
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;
    for _ in 0..12 { state.create_card_in_zone(ids::MOUNTAIN, 0, ZoneType::Library); }
    for _ in 0..id_offset { state.new_object_id(); state.new_stack_id(); }
    let zada = state.create_card_in_zone(ids::ZADA_HEDRON_GRINDER, 0, ZoneType::Battlefield);
    (state, zada)
}

fn cast(state: &mut GameState, card_id: u64, target: Option<Target>) -> u64 {
    cast_as(state, 0, card_id, target)
}

fn cast_as(state: &mut GameState, player: usize, card_id: u64, target: Option<Target>) -> u64 {
    state.priority_player = player;
    let spell = state.create_card_in_zone(card_id, player, ZoneType::Hand);
    let action = Action::CastSpell { object_id: spell, targets: target.into_iter().collect() };
    assert!(legal_actions(state).contains(&action), "fixture cast should be legal: {action:?}");
    rules::apply_action(state, &action);
    spell
}

fn pass_twice(state: &mut GameState) {
    rules::apply_action(state, &Action::PassPriority);
    rules::apply_action(state, &Action::PassPriority);
}

fn trigger_count(state: &GameState) -> usize {
    state.stack.iter().filter(|entry| matches!(entry.source,
        StackSource::TriggeredAbility { source_id, .. }
            if state.objects.get(&source_id).is_some_and(|inst| inst.card_def_id == ids::ZADA_HEDRON_GRINDER))).count()
        + state.pending_triggers.iter().filter(|trigger| trigger.context.source_card_id == ids::ZADA_HEDRON_GRINDER).count()
}

fn copy_targets(state: &GameState) -> Vec<u64> {
    state.stack.iter().filter_map(|entry| {
        if !matches!(entry.source, StackSource::SpellCopy { .. }) { return None; }
        match entry.targets.as_slice() { [Target::Object(id)] => Some(*id), _ => None }
    }).collect()
}

fn layer(state: &mut GameState, id: u64, modification: LayerModification) {
    let timestamp = state.new_timestamp();
    state.continuous_effects.push(ContinuousEffect {
        source_id: id, controller: 0, timestamp, duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::Specific(id), modification,
    });
    state.invalidate_characteristics_cache();
}

#[test]
fn zada_has_a_hand_authored_trigger() {
    let db = sample::build_sample_db();
    let id = db.find_by_name("Zada, Hedron Grinder").expect("Zada definition");
    assert_eq!(id, ids::ZADA_HEDRON_GRINDER);
    let zada = db.get(id).unwrap();
    assert_eq!(zada.mana_cost, Some(ManaCost::new(3, 0, 0, 0, 1, 0)));
    assert_eq!(zada.card_types, vec![CardType::Creature]);
    assert_eq!(zada.supertypes, vec![Supertype::Legendary]);
    assert_eq!(zada.subtypes, vec![Subtype("Goblin".into()), Subtype("Ally".into())]);
    assert_eq!((zada.power, zada.toughness), (Some(3), Some(3)));
    assert_eq!(zada.triggered_abilities.len(), 1);
    assert_eq!(zada.triggered_abilities[0].trigger,
        TriggerCondition::YouCastInstantOrSorceryTargetingOnlySelf);
    assert_eq!(zada.triggered_abilities[0].effect, Effect::CopyCastSpellForOtherCreatures);
}

#[test]
fn cast_predicate_accepts_instant_and_sorcery_only_when_they_target_zada() {
    for spell_id in [HASTE_DRAW, HASTE_DRAW_SORCERY] {
        let (mut state, zada) = game();
        cast(&mut state, spell_id, Some(Target::Object(zada)));
        assert_eq!(trigger_count(&state), 1);
        assert_eq!(state.stack.len(), 2);
        assert!(matches!(state.stack[0].source, StackSource::Spell(_)));
        assert!(matches!(state.stack[1].source, StackSource::TriggeredAbility { .. }));
    }
    for (spell_id, target_zada) in [(DRAW, false), (HASTE_DRAW, false),
        (TARGETED_ENCHANTMENT, true)] {
        let (mut state, zada) = game();
        let other = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let target = match spell_id {
            DRAW => None,
            _ if target_zada => Some(Target::Object(zada)),
            _ => Some(Target::Object(other)),
        };
        cast(&mut state, spell_id, target);
        assert_eq!(trigger_count(&state), 0, "unexpected trigger for {spell_id}");
    }
}

#[test]
fn copying_a_spell_targeting_zada_does_not_retrigger_or_count_as_cast() {
    let (mut state, zada) = game();
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    let original_id = state.stack[0].id;
    let snapshot = rules::snapshot_stack_spell(&state, original_id).unwrap();
    let cast_count = state.spells_cast_this_turn;
    rules::copy_spell_snapshot(&mut state, &snapshot, 0,
        CopyTargetPolicy::Replace(vec![Target::Object(zada)])).unwrap();
    assert_eq!(trigger_count(&state), 1);
    assert_eq!(state.spells_cast_this_turn, cast_count);
    assert_eq!(state.stack.len(), 3);
}

#[test]
fn zero_candidates_leave_only_the_original_spell() {
    let (mut state, zada) = game();
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    pass_twice(&mut state);
    assert!(state.pending_copy_order.is_none());
    assert_eq!(state.stack.len(), 1);
    assert!(matches!(state.stack[0].source, StackSource::Spell(_)));
}

#[test]
fn nonterminal_copy_instruction_cannot_start_an_ordering_batch() {
    for candidate_count in 0..=2 {
        for nested in [false, true] {
            let (mut state, zada) = game();
            let copy = Effect::CopyCastSpellForOtherCreatures;
            let first = if nested { Effect::Multiple(vec![copy]) } else { copy };
            Arc::make_mut(state.card_db.as_mut().unwrap()).cards
                .get_mut(&ids::ZADA_HEDRON_GRINDER).unwrap().triggered_abilities[0].effect =
                Effect::Multiple(vec![first, Effect::GainLife { amount: 2 }]);
            for _ in 0..candidate_count {
                state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
            }
            let life_before = state.players[0].life;
            cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
            pass_twice(&mut state);
            assert!(state.pending_copy_order.is_none());
            assert!(copy_targets(&state).is_empty());
            assert_eq!(state.players[0].life, life_before);
        }
    }
}

#[test]
fn final_child_copy_instruction_still_works() {
    let (mut state, zada) = game();
    Arc::make_mut(state.card_db.as_mut().unwrap()).cards
        .get_mut(&ids::ZADA_HEDRON_GRINDER).unwrap().triggered_abilities[0].effect =
        Effect::Multiple(vec![Effect::GainLife { amount: 2 },
            Effect::CopyCastSpellForOtherCreatures]);
    let other = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let life_before = state.players[0].life;
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    pass_twice(&mut state);
    assert_eq!(state.players[0].life, life_before + 2);
    assert_eq!(copy_targets(&state), vec![other]);
}

#[test]
fn one_candidate_copies_immediately_and_both_spells_resolve_independently() {
    let (mut state, zada) = game();
    let other = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let spell = cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    let hand_before = state.players[0].hand.len();
    pass_twice(&mut state);
    assert!(state.pending_copy_order.is_none());
    assert_eq!(copy_targets(&state), vec![other]);
    assert!(matches!(state.stack[0].source, StackSource::Spell(_)));
    let cast_count = state.spells_cast_this_turn;
    pass_twice(&mut state);
    assert!(state.has_keyword(other, KeywordAbility::Haste));
    assert!(!state.has_keyword(zada, KeywordAbility::Haste));
    assert_eq!(state.players[0].hand.len(), hand_before + 1);
    assert_eq!(state.spells_cast_this_turn, cast_count);
    pass_twice(&mut state);
    assert!(state.has_keyword(zada, KeywordAbility::Haste));
    assert_eq!(state.players[0].hand.len(), hand_before + 2);
    assert_eq!(state.players[0].graveyard.iter().filter(|&&id| id == spell).count(), 1);
}

#[test]
fn current_battlefield_and_layered_target_legality_choose_exact_candidates() {
    let (mut state, zada) = game();
    let existing = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let departing = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let no_longer_creature = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let shrouded = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let own_hexproof = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let layered_creature = state.create_card_in_zone(ARTIFACT, 0, ZoneType::Battlefield);
    let opponent = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    let entered = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let token = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state.objects.get_mut(&token).unwrap().is_token = true;
    state.move_object(departing, ZoneType::Battlefield, ZoneType::Graveyard);
    layer(&mut state, no_longer_creature, LayerModification::RemoveType(CardType::Creature));
    layer(&mut state, layered_creature, LayerModification::AddType(CardType::Creature));
    layer(&mut state, shrouded, LayerModification::AddKeyword(KeywordAbility::Shroud));
    layer(&mut state, own_hexproof, LayerModification::AddKeyword(KeywordAbility::Hexproof));
    pass_twice(&mut state);
    let pending = state.pending_copy_order.as_ref().unwrap();
    let targets: HashSet<_> = pending.items().iter()
        .map(|item| match item.targets() { [Target::Object(id)] => *id, _ => panic!("sole target") })
        .collect();
    assert_eq!(targets, HashSet::from([existing, own_hexproof, layered_creature, entered, token]));
    assert!(!targets.contains(&zada));
    assert!(!targets.contains(&opponent));
    assert_eq!(pending.items().len(), targets.len());
}

#[test]
fn naturally_created_goblin_tokens_each_receive_a_copy() {
    let (mut state, zada) = game();
    state.players[0].mana_pool.red = 2;
    cast(&mut state, ids::DRAGON_FODDER, None);
    pass_twice(&mut state);
    let tokens: HashSet<_> = state.battlefield.iter().copied()
        .filter(|id| state.objects[id].is_token).collect();
    assert_eq!(tokens.len(), 2);
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    pass_twice(&mut state);
    let pending = state.pending_copy_order.as_ref().unwrap();
    let targets: HashSet<_> = pending.items().iter()
        .map(|item| match item.targets() { [Target::Object(id)] => *id, _ => unreachable!() })
        .collect();
    assert_eq!(targets, tokens);
}

#[test]
fn original_target_becoming_illegal_does_not_cancel_the_trigger() {
    let (mut state, zada) = game();
    let other = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    layer(&mut state, zada, LayerModification::AddKeyword(KeywordAbility::Shroud));
    pass_twice(&mut state);
    assert_eq!(copy_targets(&state), vec![other]);
    pass_twice(&mut state);
    assert!(state.has_keyword(other, KeywordAbility::Haste));
    pass_twice(&mut state);
    assert!(!state.has_keyword(zada, KeywordAbility::Haste));
}

#[test]
fn source_departure_and_return_use_the_original_incarnation() {
    let (mut state, zada) = game();
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    let original_generation = state.objects[&zada].zone_change_count;
    state.move_object(zada, ZoneType::Battlefield, ZoneType::Graveyard);
    state.move_object(zada, ZoneType::Graveyard, ZoneType::Battlefield);
    assert_ne!(state.objects[&zada].zone_change_count, original_generation);
    pass_twice(&mut state);
    assert_eq!(copy_targets(&state), vec![zada]);
    pass_twice(&mut state);
    assert!(state.has_keyword(zada, KeywordAbility::Haste));
}

#[test]
fn returned_zada_qualifies_only_when_currently_controlled_by_trigger_controller() {
    for returned_controller in [0, 1] {
        let (mut state, zada) = game();
        cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
        let old_generation = state.objects[&zada].zone_change_count;
        state.move_object(zada, ZoneType::Battlefield, ZoneType::Graveyard);
        state.move_object(zada, ZoneType::Graveyard, ZoneType::Battlefield);
        assert_ne!(state.objects[&zada].zone_change_count, old_generation);
        if returned_controller == 1 {
            layer(&mut state, zada, LayerModification::ChangeController(1));
        }
        pass_twice(&mut state);
        assert_eq!(copy_targets(&state), if returned_controller == 0 { vec![zada] } else { vec![] });
    }
}

#[test]
fn departed_zada_and_countered_original_still_copy_from_the_snapshot() {
    let (mut state, zada) = game();
    let other = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let spell = cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    let original_stack_id = state.stack[0].id;
    state.move_object(zada, ZoneType::Battlefield, ZoneType::Graveyard);
    cast(&mut state, COUNTER, Some(Target::StackEntry(original_stack_id)));
    pass_twice(&mut state);
    assert!(state.stack.iter().all(|entry| entry.id != original_stack_id));
    assert_eq!(state.players[0].graveyard.iter().filter(|&&id| id == spell).count(), 1);
    pass_twice(&mut state);
    assert_eq!(copy_targets(&state), vec![other]);
    pass_twice(&mut state);
    assert!(state.has_keyword(other, KeywordAbility::Haste));
}

#[test]
fn control_change_uses_captured_trigger_controller() {
    let (mut state, zada) = game();
    let ours = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let theirs = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    state.objects.get_mut(&zada).unwrap().controller = 1;
    state.invalidate_characteristics_cache();
    pass_twice(&mut state);
    assert_eq!(copy_targets(&state), vec![ours]);
    assert_ne!(copy_targets(&state), vec![theirs]);
}

#[test]
fn layered_controller_owns_cast_trigger_and_later_changes_do_not_reassign_it() {
    let (mut state, zada) = game();
    let theirs = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    layer(&mut state, zada, LayerModification::ChangeController(1));
    assert_eq!(state.objects[&zada].controller, 0);
    cast_as(&mut state, 1, HASTE_DRAW, Some(Target::Object(zada)));
    assert_eq!(trigger_count(&state), 1);
    let entry = state.stack.last().unwrap();
    assert_eq!(entry.controller, 1);
    state.continuous_effects.retain(|effect| effect.modification != LayerModification::ChangeController(1));
    state.invalidate_characteristics_cache();
    assert_eq!(state.stack.last().unwrap().controller, 1);
    pass_twice(&mut state);
    assert_eq!(copy_targets(&state), vec![theirs]);
}

#[test]
fn stored_controller_cannot_trigger_while_layered_control_belongs_to_opponent() {
    let (mut state, zada) = game();
    layer(&mut state, zada, LayerModification::ChangeController(1));
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    assert_eq!(trigger_count(&state), 0);
}

#[test]
fn cast_after_layered_control_expires_uses_current_controller() {
    let (mut state, zada) = game();
    layer(&mut state, zada, LayerModification::ChangeController(1));
    state.continuous_effects.clear();
    state.invalidate_characteristics_cache();
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    assert_eq!(trigger_count(&state), 1);
    assert_eq!(state.stack.last().unwrap().controller, 0);
}

#[test]
fn multiple_same_name_creatures_get_distinct_orderable_copies_above_original() {
    for chosen in [0, 1] {
        let (mut state, zada) = game();
        let a = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let b = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let spell = cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
        let original_id = state.stack[0].id;
        let cast_count = state.spells_cast_this_turn;
        pass_twice(&mut state);
        let pending = state.pending_copy_order.as_ref().unwrap();
        assert_eq!(pending.items().len(), 2);
        let first = match pending.items()[chosen].targets() { [Target::Object(id)] => *id, _ => unreachable!() };
        let second = match pending.items()[1 - chosen].targets() { [Target::Object(id)] => *id, _ => unreachable!() };
        assert_eq!(HashSet::from([first, second]), HashSet::from([a, b]));
        assert_ne!(first, second);
        assert_eq!(state.stack.len(), 1);
        assert_eq!(legal_actions(&state), vec![Action::ChooseNextCopy { item_index: 0 },
            Action::ChooseNextCopy { item_index: 1 }]);
        rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: chosen });
        assert_eq!(state.stack.len(), 3);
        assert_eq!(state.stack[0].id, original_id);
        assert_eq!(copy_targets(&state), vec![first, second]);
        assert_eq!(state.spells_cast_this_turn, cast_count);
        pass_twice(&mut state);
        assert!(state.has_keyword(second, KeywordAbility::Haste));
        assert!(!state.has_keyword(first, KeywordAbility::Haste));
        pass_twice(&mut state);
        assert!(state.has_keyword(first, KeywordAbility::Haste));
        assert!(!state.players[0].graveyard.contains(&spell));
    }
}

fn equivalent_pending(extra_ids: usize, reverse_battlefield: bool, distinguish: bool) -> GameState {
    let (mut state, zada) = game_with_id_offset(extra_ids);
    let first = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    if reverse_battlefield { state.battlefield.reverse(); }
    if distinguish { state.objects.get_mut(&first).unwrap().tapped = true; }
    state.invalidate_characteristics_cache();
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    pass_twice(&mut state);
    state
}

#[test]
fn zada_ordering_is_canonical_and_information_set_is_id_independent() {
    for distinguish in [false, true] {
        let mut left = equivalent_pending(0, false, distinguish);
        let right = equivalent_pending(9, true, distinguish);
        let left_info = InformationSet::from_view(&left.visible_state(0), left.card_db());
        let right_info = InformationSet::from_view(&right.visible_state(0), right.card_db());
        assert_eq!(left_info.pending_copy_order, right_info.pending_copy_order);
        assert_eq!(left_info.hash_value(), right_info.hash_value());
        let left_actions: Vec<_> = legal_actions(&left).iter().map(|action| canonicalize(action, &left)).collect();
        let right_actions: Vec<_> = legal_actions(&right).iter().map(|action| canonicalize(action, &right)).collect();
        assert_eq!(left_actions, right_actions);
        for action in &left_actions {
            assert!(resolve(action, &left, 0).is_some());
            assert!(resolve(action, &right, 0).is_some());
        }
        rules::apply_action(&mut left, &Action::ChooseNextCopy { item_index: 0 });
        assert_eq!(copy_targets(&left).len(), 2);
        assert_ne!(copy_targets(&left)[0], copy_targets(&left)[1]);
    }
}

#[test]
fn existing_trigger_condition_binary_tags_remain_stable() {
    for (condition, index) in [
        (TriggerCondition::YouCastSpell, 14u32),
        (TriggerCondition::YouCastCreatureSpell, 15),
        (TriggerCondition::OpponentCastsSpell, 17),
        (TriggerCondition::YouDiscardACard, 21),
        (TriggerCondition::YouCastInstantOrSorceryTargetingOnlySelf, 22),
    ] {
        assert_eq!(bincode::serialize(&condition).unwrap(), index.to_le_bytes());
    }
}

#[test]
fn zada_cast_context_survives_clone_snapshot_json_and_bincode() {
    let (mut state, zada) = game();
    state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    cast(&mut state, HASTE_DRAW, Some(Target::Object(zada)));
    let database = state.card_db.clone();
    let snapshot = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let mut restored = vec![state.clone(), serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap()];
    state.restore(snapshot);
    restored.push(state);
    for mut candidate in restored {
        candidate.card_db = database.clone();
        pass_twice(&mut candidate);
        let pending = candidate.pending_copy_order.as_ref().expect("two Zada copies");
        assert_eq!(pending.items().len(), 2);
        assert!(pending.resolving_entry().is_some());
    }
}
