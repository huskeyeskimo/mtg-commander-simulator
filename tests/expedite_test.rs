use std::sync::Arc;

use mtg_gto::action::{legal_actions, Action};
use mtg_gto::card::sample::{self, ids};
use mtg_gto::card::{ActivatedAbility, CardDef, CardType, Effect, KeywordAbility, TargetSpec, ZoneType};
use mtg_gto::game::{GameState, Phase, StackSource, Target};
use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
use mtg_gto::mana::ManaCost;
use mtg_gto::rules;

fn game() -> GameState {
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(sample::build_sample_db()));
    state.phase = Phase::PreCombatMain;
    state.priority_player = 0;
    state.players[0].mana_pool.red = 10;
    for _ in 0..20 {
        state.create_card_in_zone(ids::MOUNTAIN, 0, ZoneType::Library);
    }
    state
}

fn cast_expedite(state: &mut GameState, target: u64) -> u64 {
    let spell = state.create_card_in_zone(ids::EXPEDITE, 0, ZoneType::Hand);
    let action = Action::CastSpell {
        object_id: spell,
        targets: vec![Target::Object(target)],
    };
    assert!(legal_actions(state).contains(&action));
    rules::apply_action(state, &action);
    spell
}

fn pass_twice(state: &mut GameState) {
    rules::apply_action(state, &Action::PassPriority);
    rules::apply_action(state, &Action::PassPriority);
}

fn pass_until(state: &mut GameState, active_player: usize, phase: Phase) {
    for _ in 0..32 {
        if state.active_player == active_player && state.phase == phase {
            return;
        }
        pass_twice(state);
    }
    panic!("did not reach {phase:?} for player {active_player}");
}

fn keyword_layer(state: &mut GameState, object_id: u64, modification: LayerModification) -> u32 {
    let timestamp = state.new_timestamp();
    state.continuous_effects.push(ContinuousEffect {
        source_id: object_id,
        controller: 0,
        timestamp,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::Specific(object_id),
        modification,
    });
    state.invalidate_characteristics_cache();
    timestamp
}

fn keyword_grants(state: &GameState, object_id: u64, keyword: KeywordAbility) -> Vec<(u32, u32)> {
    state.continuous_effects.iter().filter_map(|effect| {
        match (&effect.affected, &effect.modification) {
            (AffectedObjects::SpecificIncarnation { object_id: id, zone_change_count },
                LayerModification::AddKeyword(kw)) if *id == object_id && *kw == keyword =>
                    Some((effect.timestamp, *zone_change_count)),
            _ => None,
        }
    }).collect()
}

fn cast_flying_spell(state: &mut GameState, target: u64) {
    const FLYING_SPELL: u64 = 990_101;
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef {
        id: FLYING_SPELL,
        name: "Synthetic flying grant".into(),
        card_types: vec![CardType::Instant],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::GainKeywordUntilEOT {
            keyword: KeywordAbility::Flying,
            target: TargetSpec::AnyCreature,
        }),
        ..Default::default()
    });
    let spell = state.create_card_in_zone(FLYING_SPELL, 0, ZoneType::Hand);
    let action = Action::CastSpell { object_id: spell, targets: vec![Target::Object(target)] };
    assert!(legal_actions(state).contains(&action));
    rules::apply_action(state, &action);
    pass_twice(state);
}

#[test]
fn sample_expedite_has_real_card_definition() {
    let db = sample::build_sample_db();
    let expedite = db.get(ids::EXPEDITE).expect("Expedite definition");
    assert_eq!(expedite.name, "Expedite");
    assert_eq!(expedite.mana_cost, Some(ManaCost::new(0, 0, 0, 0, 1, 0)));
    assert_eq!(expedite.card_types, vec![mtg_gto::card::CardType::Instant]);
    assert_eq!(expedite.spell_effect, Some(Effect::Multiple(vec![
        Effect::GainKeywordUntilEOT {
            keyword: KeywordAbility::Haste,
            target: mtg_gto::card::TargetSpec::AnyCreature,
        },
        Effect::DrawCards { count: 1 },
    ])));
}

#[test]
fn expedite_targets_and_resolves_through_normal_priority_flow() {
    let mut state = game();
    let creature = state.create_card_in_zone(ids::KRENKO_MOB_BOSS, 0, ZoneType::Battlefield);
    let spell = cast_expedite(&mut state, creature);
    let hand_before_resolution = state.players[0].hand.len();
    let library_before_resolution = state.players[0].library.len();
    pass_twice(&mut state);

    assert!(state.has_keyword(creature, KeywordAbility::Haste));
    assert_eq!(state.players[0].hand.len(), hand_before_resolution + 1);
    assert_eq!(state.players[0].library.len(), library_before_resolution - 1);
    assert_eq!(state.players[0].graveyard, vec![spell]);
    assert!(state.stack.is_empty());
}

#[test]
fn expedite_enables_krenko_then_haste_expires_at_real_cleanup() {
    let mut state = game();
    // Keep this real-cleanup/haste control within ordinary continuing play.
    // The passive player's draw must not cause an unrelated terminal loss.
    state.create_card_in_zone(ids::MOUNTAIN, 1, ZoneType::Library);
    let krenko = state.create_card_in_zone(ids::KRENKO_MOB_BOSS, 0, ZoneType::Battlefield);
    let activate = Action::ActivateAbility {
        object_id: krenko,
        ability_index: 0,
        targets: vec![],
    };
    assert!(!state.objects[&krenko].tapped);
    assert!(state.objects[&krenko].summoning_sick);
    assert!(!legal_actions(&state).contains(&activate));

    let expedite = cast_expedite(&mut state, krenko);
    pass_twice(&mut state);

    assert!(state.objects[&krenko].summoning_sick);
    assert!(state.has_keyword(krenko, KeywordAbility::Haste));
    assert!(legal_actions(&state).contains(&activate));

    // Advance through the ordinary turn phases to cleanup without tapping
    // Krenko. Player 1 begins the next turn and receives priority first.
    pass_until(&mut state, 0, Phase::EndStep);
    pass_twice(&mut state);
    assert_eq!(state.active_player, 1);
    assert_eq!(state.priority_player, 1);
    assert!(!state.objects[&krenko].tapped);
    assert!(!state.has_keyword(krenko, KeywordAbility::Haste));
    assert!(state.objects[&krenko].summoning_sick);
    assert!(state.players[0].graveyard.contains(&expedite));

    // Tap abilities can be activated during an opponent's turn when their
    // controller has priority. Passing once gives player 0 priority while
    // sickness remains the only restriction on Krenko's ability.
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(state.priority_player, 0);
    assert!(!state.objects[&krenko].tapped);
    assert!(state.objects[&krenko].summoning_sick);
    assert!(!state.has_keyword(krenko, KeywordAbility::Haste));
    assert!(!legal_actions(&state).contains(&activate));

    // Play through player 1's turn using priority passes, then verify on
    // Krenko's controller's next turn: untap clears summoning sickness, so
    // the tap ability is legal even though Expedite's haste has expired.
    rules::apply_action(&mut state, &Action::PassPriority);
    pass_until(&mut state, 0, Phase::PreCombatMain);
    assert_eq!(state.priority_player, 0);
    assert!(!state.objects[&krenko].tapped);
    assert!(!state.objects[&krenko].summoning_sick);
    assert!(!state.has_keyword(krenko, KeywordAbility::Haste));
    assert!(legal_actions(&state).contains(&activate));
}

#[test]
fn zada_expedite_copies_are_orderable_independent_not_casts_and_fizzle_individually() {
    let mut state = game();
    let zada = state.create_card_in_zone(ids::ZADA_HEDRON_GRINDER, 0, ZoneType::Battlefield);
    let first_creature = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    let second_creature = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    let spell = cast_expedite(&mut state, zada);
    let original_stack_id = state.stack[0].id;
    let cast_count = state.spells_cast_this_turn;
    let hand_after_cast = state.players[0].hand.len();
    pass_twice(&mut state); // Resolve Zada's cast trigger.

    let pending = state.pending_copy_order.as_ref().expect("two ordered copies");
    assert_eq!(pending.items().len(), 2);
    assert_eq!(state.stack.len(), 1);
    rules::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
    assert_eq!(state.stack.len(), 3);
    assert_eq!(state.spells_cast_this_turn, cast_count);
    assert_eq!(state.stack[0].id, original_stack_id);
    assert!(matches!(state.stack[1].source, StackSource::SpellCopy { .. }));
    assert!(matches!(state.stack[2].source, StackSource::SpellCopy { .. }));

    // The top copy's target leaves before resolution. Its whole individual
    // spell fizzles, including the otherwise untargeted draw.
    let top_target = match state.stack[2].targets.as_slice() {
        [Target::Object(id)] => *id,
        other => panic!("expected one creature target, got {other:?}"),
    };
    assert!([first_creature, second_creature].contains(&top_target));
    state.move_object(top_target, ZoneType::Battlefield, ZoneType::Graveyard);
    pass_twice(&mut state);
    assert_eq!(state.players[0].hand.len(), hand_after_cast);
    assert!(!state.has_keyword(top_target, KeywordAbility::Haste));

    // The other copy and original resolve normally, each drawing exactly one.
    let surviving_copy_target = match state.stack.last().unwrap().targets.as_slice() {
        [Target::Object(id)] => *id,
        other => panic!("expected one creature target, got {other:?}"),
    };
    pass_twice(&mut state);
    assert!(state.has_keyword(surviving_copy_target, KeywordAbility::Haste));
    assert_eq!(state.players[0].hand.len(), hand_after_cast + 1);
    pass_twice(&mut state);
    assert!(state.has_keyword(zada, KeywordAbility::Haste));
    assert_eq!(state.players[0].hand.len(), hand_after_cast + 2);
    assert_eq!(state.spells_cast_this_turn, cast_count);
    assert!(state.players[0].graveyard.contains(&spell));
}

#[test]
fn expedite_with_target_removed_before_resolution_draws_nothing() {
    let mut state = game();
    let creature = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    let spell = cast_expedite(&mut state, creature);
    let hand_before = state.players[0].hand.len();
    let library_before = state.players[0].library.len();
    state.move_object(creature, ZoneType::Battlefield, ZoneType::Exile);
    pass_twice(&mut state);

    assert_eq!(state.players[0].hand.len(), hand_before);
    assert_eq!(state.players[0].library.len(), library_before);
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
    assert_eq!(state.players[0].graveyard, vec![spell]);
}

#[test]
fn expedite_with_blinked_target_draws_nothing() {
    let mut state = game();
    let creature = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    let generation_before = state.objects[&creature].zone_change_count;
    let spell = cast_expedite(&mut state, creature);
    let hand_before = state.players[0].hand.len();
    let library_before = state.players[0].library.len();
    state.move_object(creature, ZoneType::Battlefield, ZoneType::Exile);
    state.move_object(creature, ZoneType::Exile, ZoneType::Battlefield);
    assert_ne!(state.objects[&creature].zone_change_count, generation_before);
    pass_twice(&mut state);

    assert_eq!(state.players[0].hand.len(), hand_before);
    assert_eq!(state.players[0].library.len(), library_before);
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
    assert_eq!(state.players[0].graveyard, vec![spell]);
}

#[test]
fn expedite_with_new_shroud_before_resolution_draws_nothing() {
    let mut state = game();
    let creature = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    let spell = cast_expedite(&mut state, creature);
    let hand_before = state.players[0].hand.len();
    let library_before = state.players[0].library.len();
    let timestamp = state.new_timestamp();
    state.continuous_effects.push(ContinuousEffect {
        source_id: creature,
        controller: 0,
        timestamp,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::Specific(creature),
        modification: LayerModification::AddKeyword(KeywordAbility::Shroud),
    });
    state.invalidate_characteristics_cache();
    assert!(state.has_keyword(creature, KeywordAbility::Shroud));
    pass_twice(&mut state);

    assert_eq!(state.players[0].hand.len(), hand_before);
    assert_eq!(state.players[0].library.len(), library_before);
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
    assert_eq!(state.players[0].graveyard, vec![spell]);
}

#[test]
fn expedited_haste_obeys_later_removal_and_fresh_regrant() {
    let mut state = game();
    let krenko = state.create_card_in_zone(ids::KRENKO_MOB_BOSS, 0, ZoneType::Battlefield);
    assert!(!state.has_keyword(krenko, KeywordAbility::Haste)); // Warm the cache.

    let first = cast_expedite(&mut state, krenko);
    pass_twice(&mut state);
    assert!(state.has_keyword(krenko, KeywordAbility::Haste));
    assert!(state.players[0].graveyard.contains(&first));
    assert!(state.objects[&krenko].temp_keywords.is_empty());
    let first_grant = keyword_grants(&state, krenko, KeywordAbility::Haste);
    assert_eq!(first_grant.len(), 1);
    assert_eq!(first_grant[0].1, state.objects[&krenko].zone_change_count);
    assert_eq!(state.continuous_effects.iter().find(|e| e.timestamp == first_grant[0].0)
        .unwrap().source_id, first);

    // A second grant while haste is already present still gets its own timestamp.
    cast_expedite(&mut state, krenko);
    pass_twice(&mut state);
    let two_grants = keyword_grants(&state, krenko, KeywordAbility::Haste);
    assert_eq!(two_grants.len(), 2);
    assert!(two_grants[0].0 < two_grants[1].0);
    assert!(state.has_keyword(krenko, KeywordAbility::Haste));

    let removal = keyword_layer(&mut state, krenko,
        LayerModification::RemoveKeyword(KeywordAbility::Haste));
    assert!(two_grants[1].0 < removal);
    assert!(!state.has_keyword(krenko, KeywordAbility::Haste));

    cast_expedite(&mut state, krenko);
    pass_twice(&mut state);
    let three_grants = keyword_grants(&state, krenko, KeywordAbility::Haste);
    assert_eq!(three_grants.len(), 3);
    assert!(removal < three_grants[2].0);
    assert!(state.has_keyword(krenko, KeywordAbility::Haste));
}

#[test]
fn non_haste_keyword_grants_follow_the_same_timestamp_order() {
    let mut state = game();
    let creature = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    assert!(!state.has_keyword(creature, KeywordAbility::Flying));
    cast_flying_spell(&mut state, creature);
    assert!(state.has_keyword(creature, KeywordAbility::Flying));
    let first_grant = keyword_grants(&state, creature, KeywordAbility::Flying);
    assert_eq!(first_grant.len(), 1);

    let removal = keyword_layer(&mut state, creature,
        LayerModification::RemoveKeyword(KeywordAbility::Flying));
    assert!(first_grant[0].0 < removal);
    assert!(!state.has_keyword(creature, KeywordAbility::Flying));

    cast_flying_spell(&mut state, creature);
    let grants = keyword_grants(&state, creature, KeywordAbility::Flying);
    assert_eq!(grants.len(), 2);
    assert!(removal < grants[1].0);
    assert!(state.has_keyword(creature, KeywordAbility::Flying));
}

#[test]
fn blink_invalidates_old_grant_and_fresh_expedite_grants_again() {
    let mut state = game();
    let creature = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
    cast_expedite(&mut state, creature);
    pass_twice(&mut state);
    assert!(state.has_keyword(creature, KeywordAbility::Haste));
    let old_generation = state.objects[&creature].zone_change_count;
    let old_grant = keyword_grants(&state, creature, KeywordAbility::Haste);
    assert_eq!(old_grant.len(), 1);

    state.move_object(creature, ZoneType::Battlefield, ZoneType::Exile);
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
    state.move_object(creature, ZoneType::Exile, ZoneType::Battlefield);
    assert_ne!(state.objects[&creature].zone_change_count, old_generation);
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
    assert_eq!(keyword_grants(&state, creature, KeywordAbility::Haste), old_grant);

    cast_expedite(&mut state, creature);
    pass_twice(&mut state);
    let grants = keyword_grants(&state, creature, KeywordAbility::Haste);
    assert_eq!(grants.len(), 2);
    assert_eq!(grants[0].1, old_generation);
    assert_eq!(grants[1].1, state.objects[&creature].zone_change_count);
    assert!(state.has_keyword(creature, KeywordAbility::Haste));
}

#[test]
fn departing_ability_source_does_not_cancel_grant_but_target_departure_does() {
    const SOURCE: u64 = 990_102;
    let mut state = game();
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef {
        id: SOURCE,
        name: "Synthetic keyword source".into(),
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(),
            requires_tap: false,
            sacrifice_cost: None,
            life_cost: 0,
            effect: Effect::GainKeywordUntilEOT {
                keyword: KeywordAbility::Flying,
                target: TargetSpec::AnyCreature,
            },
            description: "Target creature gains flying until end of turn.".into(),
        }],
        ..Default::default()
    });
    let source = state.create_card_in_zone(SOURCE, 0, ZoneType::Battlefield);
    let target = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    let action = Action::ActivateAbility {
        object_id: source, ability_index: 0, targets: vec![Target::Object(target)],
    };
    // Generic activated-ability enumeration currently supplies empty target
    // vectors; apply an explicit valid target through the normal action path.
    assert!(mtg_gto::targeting::target_is_legal(&state, 0,
        &TargetSpec::AnyCreature, &Target::Object(target)));
    rules::apply_action(&mut state, &action);
    pass_twice(&mut state);
    assert!(state.has_keyword(target, KeywordAbility::Flying));
    let grant = state.continuous_effects.iter().find(|effect|
        effect.source_id == source && effect.modification == LayerModification::AddKeyword(KeywordAbility::Flying))
        .expect("ability grant retains source identity");
    assert_eq!(grant.duration, Duration::UntilEndOfTurn);

    state.move_object(source, ZoneType::Battlefield, ZoneType::Graveyard);
    state.refresh_continuous_effects();
    assert!(state.has_keyword(target, KeywordAbility::Flying));
    state.move_object(target, ZoneType::Battlefield, ZoneType::Exile);
    state.move_object(target, ZoneType::Exile, ZoneType::Battlefield);
    assert!(!state.has_keyword(target, KeywordAbility::Flying));
}

#[test]
fn zada_original_and_copy_grants_expire_together_at_real_cleanup() {
    let mut state = game();
    let zada = state.create_card_in_zone(ids::ZADA_HEDRON_GRINDER, 0, ZoneType::Battlefield);
    let other = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    cast_expedite(&mut state, zada);
    pass_twice(&mut state); // Zada trigger creates one copy.
    assert!(matches!(state.stack.last().unwrap().source, StackSource::SpellCopy { .. }));
    pass_twice(&mut state); // Copy resolves.
    assert!(state.has_keyword(other, KeywordAbility::Haste));
    pass_twice(&mut state); // Original resolves.
    assert!(state.has_keyword(zada, KeywordAbility::Haste));
    assert_eq!(keyword_grants(&state, other, KeywordAbility::Haste).len(), 1);
    assert_eq!(keyword_grants(&state, zada, KeywordAbility::Haste).len(), 1);

    pass_until(&mut state, 0, Phase::EndStep);
    assert!(state.has_keyword(other, KeywordAbility::Haste)); // Warm both cache entries.
    assert!(state.has_keyword(zada, KeywordAbility::Haste));
    pass_twice(&mut state); // Cleanup removes both UntilEndOfTurn layers.
    assert_eq!(state.active_player, 1);
    assert!(!state.has_keyword(other, KeywordAbility::Haste));
    assert!(!state.has_keyword(zada, KeywordAbility::Haste));
    assert!(keyword_grants(&state, other, KeywordAbility::Haste).is_empty());
    assert!(keyword_grants(&state, zada, KeywordAbility::Haste).is_empty());
}

#[test]
fn active_grants_keep_timestamps_and_generations_across_state_roundtrips() {
    let mut state = game();
    let creature = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    cast_expedite(&mut state, creature);
    pass_twice(&mut state);
    let removal = keyword_layer(&mut state, creature,
        LayerModification::RemoveKeyword(KeywordAbility::Haste));
    cast_expedite(&mut state, creature);
    pass_twice(&mut state);
    let expected_grants = keyword_grants(&state, creature, KeywordAbility::Haste);
    assert_eq!(expected_grants.len(), 2);
    assert!(expected_grants[0].0 < removal && removal < expected_grants[1].0);
    assert!(state.has_keyword(creature, KeywordAbility::Haste));

    let original_generation = state.objects[&creature].zone_change_count;
    let original_effects = state.continuous_effects.clone();
    let original_power = state.effective_power(creature);
    let original_toughness = state.effective_toughness(creature);
    let db = state.card_db.clone();
    // Advance once before taking the snapshot, so the expected next timestamp
    // after restore can be checked exactly.
    let timestamp_before_snapshot = state.new_timestamp();
    let expected_next_timestamp = timestamp_before_snapshot.checked_add(1).unwrap();
    let snapshot = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let mut restored_from_snapshot = state.clone();
    restored_from_snapshot.restore(snapshot.clone()).unwrap();

    // Change the live state after taking the snapshot: remove its EOT effects,
    // blink the target, warm the no-Haste cache, and grant Flying to the new
    // incarnation at a later timestamp.
    state.cleanup_eot_effects();
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
    state.move_object(creature, ZoneType::Battlefield, ZoneType::Exile);
    state.move_object(creature, ZoneType::Exile, ZoneType::Battlefield);
    let changed_generation = state.objects[&creature].zone_change_count;
    assert_ne!(changed_generation, original_generation);
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
    let post_snapshot_timestamp = state.new_timestamp();
    assert_eq!(post_snapshot_timestamp, expected_next_timestamp);
    state.continuous_effects.push(ContinuousEffect {
        source_id: creature,
        controller: 0,
        timestamp: post_snapshot_timestamp,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::SpecificIncarnation {
            object_id: creature,
            zone_change_count: changed_generation,
        },
        modification: LayerModification::AddKeyword(KeywordAbility::Flying),
    });
    state.invalidate_characteristics_cache();
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
    assert!(state.has_keyword(creature, KeywordAbility::Flying));
    assert_ne!(state.continuous_effects, original_effects);
    assert_ne!(keyword_grants(&state, creature, KeywordAbility::Haste), expected_grants);

    // Restoring must replace the changed effects and incarnation, and clear
    // the post-snapshot cache entry that described the new permanent.
    state.restore(snapshot).unwrap();
    assert_eq!(state.continuous_effects, original_effects);
    assert_eq!(state.objects[&creature].zone_change_count, original_generation);
    assert_eq!(keyword_grants(&state, creature, KeywordAbility::Haste), expected_grants);
    for effect in &state.continuous_effects {
        if matches!(effect.modification, LayerModification::AddKeyword(KeywordAbility::Haste))
            && matches!(effect.affected,
                AffectedObjects::SpecificIncarnation { object_id, .. } if object_id == creature)
        {
            assert_eq!(effect.duration, Duration::UntilEndOfTurn);
            assert!(matches!(effect.affected, AffectedObjects::SpecificIncarnation {
                object_id, zone_change_count
            } if object_id == creature && zone_change_count == original_generation));
        }
    }
    assert!(state.has_keyword(creature, KeywordAbility::Haste));
    assert!(!state.has_keyword(creature, KeywordAbility::Flying));
    assert_eq!(state.effective_power(creature), original_power);
    assert_eq!(state.effective_toughness(creature), original_toughness);
    assert_eq!(state.new_timestamp(), expected_next_timestamp);

    let mut candidates = vec![
        state.clone(),
        restored_from_snapshot,
        serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap(),
    ];
    for candidate in &mut candidates {
        candidate.card_db = db.clone();
        assert_eq!(keyword_grants(candidate, creature, KeywordAbility::Haste), expected_grants);
        assert!(candidate.has_keyword(creature, KeywordAbility::Haste));
        assert!(candidate.new_timestamp() > expected_grants[1].0);
        candidate.move_object(creature, ZoneType::Battlefield, ZoneType::Exile);
        candidate.move_object(creature, ZoneType::Exile, ZoneType::Battlefield);
        assert!(!candidate.has_keyword(creature, KeywordAbility::Haste));
    }
}

#[test]
fn affected_objects_legacy_binary_tags_and_decoding_stay_stable() {
    for (selector, tag) in [
        (AffectedObjects::Source, 0u32),
        (AffectedObjects::Specific(42), 1),
        (AffectedObjects::AttachedTo, 10),
        (AffectedObjects::OtherCreaturesWithSubtypeControlledBySource("Goblin".into()), 11),
        (AffectedObjects::SpecificIncarnation { object_id: 42, zone_change_count: 3 }, 12),
    ] {
        let bytes = bincode::serialize(&selector).unwrap();
        assert_eq!(&bytes[..4], &tag.to_le_bytes());
        assert_eq!(bincode::deserialize::<AffectedObjects>(&bytes).unwrap(), selector);
    }
    let mut old_specific = 1u32.to_le_bytes().to_vec();
    old_specific.extend_from_slice(&42u64.to_le_bytes());
    assert_eq!(bincode::deserialize::<AffectedObjects>(&old_specific).unwrap(),
        AffectedObjects::Specific(42));
}
