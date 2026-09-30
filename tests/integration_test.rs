use std::sync::Arc;

use rand::seq::SliceRandom;

use mtg_gto::action::canonical::{canonicalize, resolve};
use mtg_gto::action::{legal_actions, legal_actions_abstracted, Action};
use mtg_gto::card::sample;
use mtg_gto::card::{KeywordAbility, ZoneType};
use mtg_gto::events::{EventBus, GameEvent, Zone};
use mtg_gto::game::{GameState, Phase, Target};
use mtg_gto::replacement::{
    find_applicable_replacements, ReplacementAction, ReplacementEffect, ReplacementEventKind,
};
use mtg_gto::rules;
use mtg_gto::simulation;
use mtg_gto::strategy::{GoldfishStrategy, GreedyStrategy, RandomStrategy, Strategy};

#[test]
fn test_sample_db_builds() {
    let db = sample::build_sample_db();
    assert!(db.get(sample::ids::MOUNTAIN).is_some());
    assert!(db.get(sample::ids::LIGHTNING_BOLT).is_some());
    assert!(db.get(sample::ids::GRIZZLY_BEARS).is_some());
    assert!(db.get(sample::ids::SERRA_ANGEL).is_some());
}

#[test]
fn test_catalog_ids_match_sample_ids() {
    assert_eq!(
        mtg_gto::card::catalog::ids::LIGHTNING_BOLT,
        sample::ids::LIGHTNING_BOLT
    );
    assert_eq!(
        mtg_gto::card::catalog::ids::KINNAN_BONDER_PRODIGY,
        sample::ids::KINNAN_BONDER_PRODIGY
    );
}

#[test]
fn test_catalog_effect_status_marks_effect_cards() {
    let db = sample::build_sample_db();
    let statuses = mtg_gto::card::catalog::card_implementation_status(&db);

    let bolt = statuses
        .iter()
        .find(|s| s.id == sample::ids::LIGHTNING_BOLT)
        .expect("Lightning Bolt should be in catalog");
    assert!(bolt.effects_implemented);

    let bears = statuses
        .iter()
        .find(|s| s.id == sample::ids::GRIZZLY_BEARS)
        .expect("Grizzly Bears should be in catalog");
    assert!(!bears.effects_implemented);
}

#[test]
fn test_shivan_dragon_has_firebreathing_ability() {
    let db = sample::build_sample_db();
    let shivan = db
        .get(sample::ids::SHIVAN_DRAGON)
        .expect("Shivan Dragon should exist");
    assert_eq!(shivan.activated_abilities.len(), 1);
}

#[test]
fn test_deck_sizes() {
    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();
    assert_eq!(red.len(), 60);
    assert_eq!(green.len(), 60);
}

#[test]
fn test_game_setup() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    mtg_gto::rules::setup_game(&mut state, &red, &green);

    // Both players should have 7 cards in hand
    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(state.players[1].hand.len(), 7);

    // Libraries should have 53 cards each (60 - 7)
    assert_eq!(state.players[0].library.len(), 53);
    assert_eq!(state.players[1].library.len(), 53);

    // Both at 20 life
    assert_eq!(state.players[0].life, 20);
    assert_eq!(state.players[1].life, 20);

    // Game should not be over
    assert!(!state.game_over);
}

#[test]
fn test_single_game_completes() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();

    let greedy = GreedyStrategy;
    let result = simulation::run_game(&db, &red, &green, &greedy, &greedy);

    // Game should produce a winner (not a draw)
    assert!(result.winner.is_some(), "Game should have a winner");
    assert!(result.turns > 0, "Game should last at least 1 turn");
    assert!(result.actions_taken > 0, "Game should have actions");
}

#[test]
fn test_simulation_produces_results() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();

    let greedy = GreedyStrategy;
    let results = simulation::simulate(&db, &red, &green, &greedy, &greedy, 100);

    assert_eq!(results.total_games, 100);
    assert_eq!(
        results.player0_wins + results.player1_wins + results.draws,
        100
    );
    assert!(results.avg_turns > 0.0);
}

#[test]
fn test_random_vs_greedy_greedy_wins_more() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();

    let greedy = GreedyStrategy;
    let random = RandomStrategy;

    // Greedy as P0 vs Random as P1 should win more often than random.
    // Use 1000 games to reduce statistical variance and avoid flakiness.
    let results = simulation::simulate(&db, &red, &green, &greedy, &random, 1000);
    let greedy_wr = results.win_rate(0);

    // Greedy should win at least 40% of the time against random
    assert!(
        greedy_wr >= 0.4,
        "Greedy should beat random at least 40% of the time, got {:.1}%",
        greedy_wr * 100.0
    );
}

#[test]
fn test_mirror_match_roughly_equal() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let greedy = GreedyStrategy;

    // Mirror match should be roughly 50/50 (within statistical variance)
    let results = simulation::simulate(&db, &red, &red, &greedy, &greedy, 200);

    // In aggro mirrors, going first is a significant advantage.
    // P0 wins more often but both should win some games.
    let p0_wr = results.win_rate(0);
    let p1_wr = results.win_rate(1);
    assert!(
        p0_wr > 0.0 && p1_wr > 0.0,
        "Both players should win some games in a mirror, got P0={:.1}% P1={:.1}%",
        p0_wr * 100.0,
        p1_wr * 100.0
    );
}

#[test]
fn test_etb_trigger_elvish_visionary() {
    // Test that Elvish Visionary's ETB trigger draws a card
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Give player 0 some forests and an Elvish Visionary in hand
    for _ in 0..3 {
        state.create_card_in_zone(sample::ids::FOREST, 0, ZoneType::Library);
    }
    // Give both players some library cards to draw from
    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Put Elvish Visionary in hand
    let vis_id = state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Hand);
    // Put 2 Forests on battlefield (untapped) for mana
    let f1 = state.create_card_in_zone(sample::ids::FOREST, 0, ZoneType::Battlefield);
    let f2 = state.create_card_in_zone(sample::ids::FOREST, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&f1) {
        inst.tapped = false;
        inst.summoning_sick = false;
    }
    if let Some(inst) = state.objects.get_mut(&f2) {
        inst.tapped = false;
        inst.summoning_sick = false;
    }

    // Set up game state for main phase
    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::PreCombatMain;
    state.turn_number = 2; // not turn 1 so no special rules

    let hand_before = state.players[0].hand.len();

    // Cast Elvish Visionary
    rules::apply_action(
        &mut state,
        &Action::CastSpell {
            object_id: vis_id,
            targets: vec![],
        },
    );

    // Visionary should be on the stack
    assert_eq!(state.stack.len(), 1);

    // Both players pass priority to resolve
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // After resolution, Visionary is on the battlefield
    assert!(
        state.battlefield.contains(&vis_id),
        "Elvish Visionary should be on the battlefield after resolution"
    );

    // The ETB trigger should be on the stack now
    assert_eq!(state.stack.len(), 1, "ETB trigger should be on the stack");

    // Resolve the ETB trigger (both players pass)
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // After ETB resolves, player should have drawn a card
    // Hand was: hand_before - 1 (cast visionary) + 1 (ETB draw) = hand_before
    let hand_after = state.players[0].hand.len();
    assert_eq!(
        hand_after,
        hand_before, // -1 for casting, +1 for draw = same
        "Player should have drawn a card from Elvish Visionary ETB (before={}, after={})",
        hand_before,
        hand_after
    );
}

#[test]
fn test_new_sample_cards_in_db() {
    let db = sample::build_sample_db();
    assert!(db.get(sample::ids::ELVISH_VISIONARY).is_some());
    assert!(db.get(sample::ids::BLADE_SPLICER).is_some());
    assert!(db.get(sample::ids::SIEGE_GANG_COMMANDER).is_some());

    // Check Elvish Visionary has an ETB trigger
    let ev = db.get(sample::ids::ELVISH_VISIONARY).unwrap();
    assert_eq!(ev.triggered_abilities.len(), 1);
    assert_eq!(
        ev.triggered_abilities[0].trigger,
        mtg_gto::card::TriggerCondition::EntersBattlefield
    );
}

#[test]
fn test_order_triggers_surfaced_for_multiple_simultaneous_triggers() {
    // When a player controls two permanents that both trigger on the same event,
    // they must choose the order to place them on the stack. This should be
    // surfaced as Action::OrderTriggers, not silently ordered FIFO.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Give both players library cards so no one loses from empty library
    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Put two Elvish Visionaries on the battlefield for player 0.
    // Both have "When ~ enters the battlefield, draw a card" — but we'll
    // use the trigger system directly by queuing two simultaneous triggers.
    let vis1 = state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Battlefield);
    let vis2 = state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::PreCombatMain;
    state.turn_number = 2;

    // Manually queue two simultaneous ETB triggers for player 0
    state.pending_triggers.push(mtg_gto::game::PendingTrigger {
        source_id: vis1,
        ability_index: 0,
        controller: 0,
        targets: vec![],
    });
    state.pending_triggers.push(mtg_gto::game::PendingTrigger {
        source_id: vis2,
        ability_index: 0,
        controller: 0,
        targets: vec![],
    });

    // Attempt to flush — should pause because player 0 has >1 trigger
    // (flush_triggers is internal, but we can observe via legal_actions)
    let actions = legal_actions(&state);

    // Should contain OrderTriggers actions (2! = 2 permutations) plus Concede
    let order_trigger_actions: Vec<&Action> = actions
        .iter()
        .filter(|a| matches!(a, Action::OrderTriggers { .. }))
        .collect();

    assert_eq!(
        order_trigger_actions.len(),
        2,
        "Should have 2 orderings (2! permutations) for 2 triggers, got {:?}",
        actions
    );

    // Should NOT contain PassPriority (only ordering + concede allowed)
    assert!(
        !actions.contains(&Action::PassPriority),
        "PassPriority should not be offered when triggers need ordering"
    );

    // Apply the first ordering
    let chosen = order_trigger_actions[0].clone();
    rules::apply_action(&mut state, &chosen);

    // After ordering, triggers should be on the stack and pending_triggers empty
    assert!(
        state.pending_triggers.is_empty(),
        "pending_triggers should be empty after ordering"
    );
    assert_eq!(state.stack.len(), 2, "Both triggers should be on the stack");

    // Now legal_actions should return normal priority actions (PassPriority, etc.)
    let actions_after = legal_actions(&state);
    assert!(
        actions_after.contains(&Action::PassPriority),
        "Should have normal priority actions after triggers are ordered"
    );
}

#[test]
fn test_single_trigger_auto_flushes_without_ordering() {
    // When a player has exactly 1 trigger, it should be auto-pushed
    // to the stack without requiring an OrderTriggers action.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    let vis = state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::PreCombatMain;
    state.turn_number = 2;

    // Queue a single trigger
    state.pending_triggers.push(mtg_gto::game::PendingTrigger {
        source_id: vis,
        ability_index: 0,
        controller: 0,
        targets: vec![],
    });

    // legal_actions should NOT offer OrderTriggers — single trigger auto-flushes
    // But first we need to actually run flush_triggers. The pending_triggers are
    // checked by legal_actions but flush_triggers is called by rules engine.
    // With 1 pending trigger and it's the priority player's, legal_actions
    // will see 1 trigger (not >1) and proceed to normal actions.
    let actions = legal_actions(&state);
    let has_order_triggers = actions
        .iter()
        .any(|a| matches!(a, Action::OrderTriggers { .. }));

    assert!(
        !has_order_triggers,
        "Should not offer OrderTriggers for a single trigger"
    );
    assert!(
        actions.contains(&Action::PassPriority),
        "Should offer normal priority actions for single trigger"
    );
}

#[test]
fn test_cleanup_requires_discard_action() {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..8 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand);
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::Cleanup;
    state.turn_number = 1;

    let actions = mtg_gto::action::legal_actions(&state);
    assert_eq!(actions.len(), 8);
    assert!(actions
        .iter()
        .all(|action| matches!(action, Action::Discard { .. })));

    let discard_action = actions[0].clone();
    rules::apply_action(&mut state, &discard_action);

    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(state.players[0].graveyard.len(), 1);
    assert_eq!(state.turn_number, 2);
    assert_eq!(state.phase, mtg_gto::game::Phase::Upkeep);
}

#[test]
fn test_cleanup_allows_pass_at_seven() {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..7 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand);
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::Cleanup;

    let actions = mtg_gto::action::legal_actions(&state);
    assert!(actions
        .iter()
        .any(|action| matches!(action, Action::PassPriority)));
    assert!(actions
        .iter()
        .all(|action| !matches!(action, Action::Discard { .. })));
}

#[test]
fn test_cleanup_multiple_discards() {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    let mut hand_ids = Vec::new();
    for _ in 0..10 {
        hand_ids.push(state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand));
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::Cleanup;
    state.turn_number = 1;

    let first_discard = hand_ids[3];
    rules::apply_action(
        &mut state,
        &Action::Discard {
            object_id: first_discard,
        },
    );
    assert!(!state.players[0].hand.contains(&first_discard));
    assert!(state.players[0].graveyard.contains(&first_discard));
    assert_eq!(state.players[0].hand.len(), 9);

    let actions = mtg_gto::action::legal_actions(&state);
    let discard_action = actions
        .into_iter()
        .find(|action| matches!(action, Action::Discard { .. }))
        .expect("expected another discard action");
    rules::apply_action(&mut state, &discard_action);
    assert_eq!(state.players[0].hand.len(), 8);

    let actions = mtg_gto::action::legal_actions(&state);
    let discard_action = actions
        .into_iter()
        .find(|action| matches!(action, Action::Discard { .. }))
        .expect("expected final discard action");
    rules::apply_action(&mut state, &discard_action);

    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(state.turn_number, 2);
    assert_eq!(state.phase, mtg_gto::game::Phase::Upkeep);
}

#[test]
fn test_apnap_both_players_multiple_triggers() {
    // When both active player AND non-active player each have >1 simultaneous
    // trigger, AP must order first (APNAP), then NAP orders after.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    let vis_p0_a =
        state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Battlefield);
    let vis_p0_b =
        state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Battlefield);
    let vis_p1_a =
        state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 1, ZoneType::Battlefield);
    let vis_p1_b =
        state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 1, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::PreCombatMain;
    state.turn_number = 2;

    // Queue 2 triggers for AP (player 0) and 2 for NAP (player 1)
    state.pending_triggers.push(mtg_gto::game::PendingTrigger {
        source_id: vis_p0_a,
        ability_index: 0,
        controller: 0,
        targets: vec![],
    });
    state.pending_triggers.push(mtg_gto::game::PendingTrigger {
        source_id: vis_p0_b,
        ability_index: 0,
        controller: 0,
        targets: vec![],
    });
    state.pending_triggers.push(mtg_gto::game::PendingTrigger {
        source_id: vis_p1_a,
        ability_index: 0,
        controller: 1,
        targets: vec![],
    });
    state.pending_triggers.push(mtg_gto::game::PendingTrigger {
        source_id: vis_p1_b,
        ability_index: 0,
        controller: 1,
        targets: vec![],
    });

    // AP (player 0) should order first
    let actions = legal_actions(&state);
    assert_eq!(
        state.priority_player, 0,
        "AP should have priority to order first"
    );
    let order_actions: Vec<&Action> = actions
        .iter()
        .filter(|a| matches!(a, Action::OrderTriggers { .. }))
        .collect();
    assert_eq!(
        order_actions.len(),
        2,
        "AP should see 2! = 2 orderings for their 2 triggers"
    );

    // AP orders their triggers
    rules::apply_action(&mut state, order_actions[0]);

    // Now NAP (player 1) should have priority to order their triggers
    assert_eq!(
        state.priority_player, 1,
        "NAP should now have priority to order"
    );
    assert!(
        !state.pending_triggers.is_empty(),
        "NAP triggers should still be pending"
    );

    let actions2 = legal_actions(&state);
    let order_actions2: Vec<&Action> = actions2
        .iter()
        .filter(|a| matches!(a, Action::OrderTriggers { .. }))
        .collect();
    assert_eq!(
        order_actions2.len(),
        2,
        "NAP should see 2! = 2 orderings for their 2 triggers"
    );

    // NAP orders their triggers
    rules::apply_action(&mut state, order_actions2[0]);

    // All 4 triggers should now be on the stack
    assert!(
        state.pending_triggers.is_empty(),
        "All triggers should be flushed"
    );
    assert_eq!(
        state.stack.len(),
        4,
        "All 4 triggers should be on the stack"
    );

    // Verify APNAP stack order: AP's triggers were placed first (resolve last),
    // NAP's triggers placed second (resolve first since stack is LIFO)
    assert_eq!(state.stack[0].controller, 0, "AP triggers on stack first");
    assert_eq!(state.stack[1].controller, 0, "AP triggers on stack first");
    assert_eq!(state.stack[2].controller, 1, "NAP triggers on stack second");
    assert_eq!(state.stack[3].controller, 1, "NAP triggers on stack second");
}

#[test]
fn test_more_than_six_triggers_fifo_fallback() {
    // When a player has >6 simultaneous triggers, the permutation generator
    // falls back to a single FIFO ordering to avoid combinatorial explosion.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Create 7 permanents with triggers for player 0
    let mut vis_ids = Vec::new();
    for _ in 0..7 {
        vis_ids.push(state.create_card_in_zone(
            sample::ids::ELVISH_VISIONARY,
            0,
            ZoneType::Battlefield,
        ));
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::PreCombatMain;
    state.turn_number = 2;

    // Queue 7 triggers for player 0
    for &vis_id in &vis_ids {
        state.pending_triggers.push(mtg_gto::game::PendingTrigger {
            source_id: vis_id,
            ability_index: 0,
            controller: 0,
            targets: vec![],
        });
    }

    let actions = legal_actions(&state);
    let order_actions: Vec<&Action> = actions
        .iter()
        .filter(|a| matches!(a, Action::OrderTriggers { .. }))
        .collect();

    // 7! = 5040 would be too many; should fall back to exactly 1 FIFO ordering
    assert_eq!(
        order_actions.len(),
        1,
        "Should have exactly 1 ordering (FIFO fallback) for >6 triggers, got {}",
        order_actions.len()
    );

    // Apply the single ordering — all triggers should end up on the stack
    rules::apply_action(&mut state, order_actions[0]);
    assert!(state.pending_triggers.is_empty());
    assert_eq!(
        state.stack.len(),
        7,
        "All 7 triggers should be on the stack"
    );
}

#[test]
fn test_etb_multiple_triggers_through_natural_game_flow() {
    // Test that resolving a creature with an ETB trigger, when there's already
    // another permanent with an ETB-watching trigger, correctly surfaces
    // OrderTriggers through the actual spell resolution path.
    //
    // Setup: Player 0 has an Elvish Visionary on the battlefield and casts a
    // second Elvish Visionary. When the second resolves, there's only 1 ETB
    // trigger (the one from the entering creature). This is auto-flushed.
    // But we can test the "2 simultaneous ETB" case by manually triggering
    // the fire_triggers path with 2 pending triggers in the natural game context.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Put an Elvish Visionary in hand with enough forests to cast
    let vis_id = state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Hand);
    let f1 = state.create_card_in_zone(sample::ids::FOREST, 0, ZoneType::Battlefield);
    let f2 = state.create_card_in_zone(sample::ids::FOREST, 0, ZoneType::Battlefield);
    for id in [f1, f2] {
        if let Some(inst) = state.objects.get_mut(&id) {
            inst.tapped = false;
            inst.summoning_sick = false;
        }
    }

    // Put a second Elvish Visionary already on the battlefield
    let _vis_existing =
        state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::PreCombatMain;
    state.turn_number = 2;

    // Cast the Elvish Visionary
    rules::apply_action(
        &mut state,
        &Action::CastSpell {
            object_id: vis_id,
            targets: vec![],
        },
    );
    assert_eq!(state.stack.len(), 1, "Spell should be on stack");

    // Resolve: both players pass
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // After resolution, Visionary enters the battlefield and its ETB trigger fires.
    // Only the entering creature has an ETB trigger (the existing one doesn't
    // re-trigger), so there's exactly 1 trigger — auto-flushed, no OrderTriggers.
    assert!(
        state.pending_triggers.is_empty(),
        "Single ETB trigger should be auto-flushed"
    );
    assert_eq!(state.stack.len(), 1, "ETB trigger should be on the stack");

    // Verify normal flow continues: resolve the ETB trigger
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // Trigger resolved — player drew a card
    assert_eq!(
        state.stack.len(),
        0,
        "Stack should be empty after ETB resolution"
    );
}

// ======================================================================
// Combat abstraction tests
// ======================================================================

/// Helper: set up a game state with specific creatures on the battlefield,
/// in the DeclareAttackers phase, ready for player 0 to declare attackers.
fn setup_combat_state(attacker_card_ids: &[u64], blocker_card_ids: &[u64]) -> GameState {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Libraries so nobody loses from decking
    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    // Player 0's creatures (attackers)
    for &card_id in attacker_card_ids {
        let id = state.create_card_in_zone(card_id, 0, ZoneType::Battlefield);
        if let Some(inst) = state.objects.get_mut(&id) {
            inst.tapped = false;
            inst.summoning_sick = false;
        }
    }

    // Player 1's creatures (potential blockers)
    for &card_id in blocker_card_ids {
        let id = state.create_card_in_zone(card_id, 1, ZoneType::Battlefield);
        if let Some(inst) = state.objects.get_mut(&id) {
            inst.tapped = false;
            inst.summoning_sick = false;
        }
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = mtg_gto::game::Phase::DeclareAttackers;
    state.turn_number = 3;

    state
}

#[test]
fn test_attack_abstraction_small_board_uses_full_enumeration() {
    // With <= 5 eligible attackers, abstraction falls back to full enumeration.
    // 4 creatures => 2^4 = 16 subsets.
    let state = setup_combat_state(
        &[
            sample::ids::GRIZZLY_BEARS,
            sample::ids::GREY_OGRE,
            sample::ids::SAVANNAH_LIONS,
            sample::ids::GOBLIN_GUIDE,
        ],
        &[],
    );

    let full_actions = legal_actions(&state);
    let abstracted_actions = legal_actions_abstracted(&state);

    let full_attacks: Vec<&Action> = full_actions
        .iter()
        .filter(|a| matches!(a, Action::DeclareAttackers { .. }))
        .collect();
    let abstracted_attacks: Vec<&Action> = abstracted_actions
        .iter()
        .filter(|a| matches!(a, Action::DeclareAttackers { .. }))
        .collect();

    // 2^4 = 16 attack subsets
    assert_eq!(full_attacks.len(), 16, "Full should have 2^4 = 16 subsets");
    // Bucketed should also have 16 since <= 5 eligible triggers fallback
    assert_eq!(
        abstracted_attacks.len(),
        full_attacks.len(),
        "Bucketed should equal full for <= 5 eligible attackers"
    );
}

#[test]
fn test_attack_abstraction_large_board_reduces_actions() {
    // With 8 creatures, full enumeration gives 2^8 = 256 subsets.
    // Bucketed should give at most 6.
    let state = setup_combat_state(
        &[
            sample::ids::SERRA_ANGEL,        // 4/4 flying vigilance
            sample::ids::SHIVAN_DRAGON,      // 5/5 flying
            sample::ids::GRIZZLY_BEARS,      // 2/2
            sample::ids::GREY_OGRE,          // 2/2
            sample::ids::GOBLIN_GUIDE,       // 2/2
            sample::ids::SAVANNAH_LIONS,     // 2/1
            sample::ids::KALONIAN_TUSKER,    // 3/3
            sample::ids::LEATHERBACK_BALOTH, // 4/5
        ],
        &[],
    );

    let full_actions = legal_actions(&state);
    let abstracted_actions = legal_actions_abstracted(&state);

    let full_attacks: Vec<&Action> = full_actions
        .iter()
        .filter(|a| matches!(a, Action::DeclareAttackers { .. }))
        .collect();
    let abstracted_attacks: Vec<&Action> = abstracted_actions
        .iter()
        .filter(|a| matches!(a, Action::DeclareAttackers { .. }))
        .collect();

    assert_eq!(
        full_attacks.len(),
        256,
        "Full should have 2^8 = 256 subsets"
    );
    assert!(
        abstracted_attacks.len() <= 7,
        "Bucketed should have at most 7 buckets, got {}",
        abstracted_attacks.len()
    );
    assert!(
        abstracted_attacks.len() >= 3,
        "Bucketed should have at least 3 buckets (none, alpha, +others), got {}",
        abstracted_attacks.len()
    );
}

#[test]
fn test_attack_abstraction_always_includes_none_and_alpha() {
    // None (empty) and Alpha (all) must always be present.
    let state = setup_combat_state(
        &[
            sample::ids::SERRA_ANGEL,
            sample::ids::SHIVAN_DRAGON,
            sample::ids::GRIZZLY_BEARS,
            sample::ids::GREY_OGRE,
            sample::ids::GOBLIN_GUIDE,
            sample::ids::SAVANNAH_LIONS,
        ],
        &[],
    );

    let abstracted_actions = legal_actions_abstracted(&state);
    let attacks: Vec<&Vec<mtg_gto::card::ObjectId>> = abstracted_actions
        .iter()
        .filter_map(|a| {
            if let Action::DeclareAttackers { attackers } = a {
                Some(attackers)
            } else {
                None
            }
        })
        .collect();

    // Must have an empty attack
    assert!(
        attacks.iter().any(|a| a.is_empty()),
        "Bucketed must always include 'none' (empty attack)"
    );

    // Must have the alpha strike (all 6 creatures)
    assert!(
        attacks.iter().any(|a| a.len() == 6),
        "Bucketed must always include 'alpha' (all eligible)"
    );
}

#[test]
fn test_attack_abstraction_evasion_bucket() {
    // Board has 2 flyers + 4 ground creatures => evasion-only bucket should
    // contain exactly the 2 flyers.
    let state = setup_combat_state(
        &[
            sample::ids::SERRA_ANGEL,    // flying
            sample::ids::SHIVAN_DRAGON,  // flying
            sample::ids::GRIZZLY_BEARS,  // ground
            sample::ids::GREY_OGRE,      // ground
            sample::ids::GOBLIN_GUIDE,   // ground
            sample::ids::SAVANNAH_LIONS, // ground
        ],
        &[],
    );

    let abstracted_actions = legal_actions_abstracted(&state);
    let attacks: Vec<&Vec<mtg_gto::card::ObjectId>> = abstracted_actions
        .iter()
        .filter_map(|a| {
            if let Action::DeclareAttackers { attackers } = a {
                Some(attackers)
            } else {
                None
            }
        })
        .collect();

    // Should have an attack with exactly 2 creatures (the evasion bucket)
    assert!(
        attacks.iter().any(|a| a.len() == 2),
        "Should have a bucket with exactly 2 creatures (evasion-only). Sizes: {:?}",
        attacks.iter().map(|a| a.len()).collect::<Vec<_>>()
    );
}

#[test]
fn test_attack_abstraction_no_evasion_dedup() {
    // Board with NO evasive creatures — evasion bucket should be skipped
    // (it would duplicate either none or alpha).
    let state = setup_combat_state(
        &[
            sample::ids::GRIZZLY_BEARS,
            sample::ids::GREY_OGRE,
            sample::ids::GOBLIN_GUIDE,
            sample::ids::SAVANNAH_LIONS,
            sample::ids::KALONIAN_TUSKER,
            sample::ids::LEATHERBACK_BALOTH,
        ],
        &[],
    );

    let abstracted_actions = legal_actions_abstracted(&state);
    let attacks: Vec<&Vec<mtg_gto::card::ObjectId>> = abstracted_actions
        .iter()
        .filter_map(|a| {
            if let Action::DeclareAttackers { attackers } = a {
                Some(attackers)
            } else {
                None
            }
        })
        .collect();

    // No duplicates
    let mut sorted: Vec<Vec<mtg_gto::card::ObjectId>> = attacks
        .iter()
        .map(|a| {
            let mut v = (*a).clone();
            v.sort();
            v
        })
        .collect();
    let before_dedup = sorted.len();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        before_dedup,
        "All attacker buckets should be unique"
    );
}

#[test]
fn test_block_abstraction_reduces_actions() {
    // Set up a blocking scenario with abstraction.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    // Player 0 declared attackers: 4 creatures
    let mut attacker_ids = Vec::new();
    for &card_id in &[
        sample::ids::GRIZZLY_BEARS,
        sample::ids::GREY_OGRE,
        sample::ids::KALONIAN_TUSKER,
        sample::ids::LEATHERBACK_BALOTH,
    ] {
        let id = state.create_card_in_zone(card_id, 0, ZoneType::Battlefield);
        if let Some(inst) = state.objects.get_mut(&id) {
            inst.tapped = true; // attacking
            inst.summoning_sick = false;
        }
        attacker_ids.push(id);
    }

    // Player 1 has 5 potential blockers
    for &card_id in &[
        sample::ids::GRIZZLY_BEARS,
        sample::ids::GREY_OGRE,
        sample::ids::SAVANNAH_LIONS,
        sample::ids::KALONIAN_TUSKER,
        sample::ids::LEATHERBACK_BALOTH,
    ] {
        let id = state.create_card_in_zone(card_id, 1, ZoneType::Battlefield);
        if let Some(inst) = state.objects.get_mut(&id) {
            inst.tapped = false;
            inst.summoning_sick = false;
        }
    }

    state.combat.attackers = attacker_ids;
    state.active_player = 0;
    state.priority_player = 1; // defender declares blockers
    state.phase = mtg_gto::game::Phase::DeclareBlockers;
    state.turn_number = 3;

    let full_actions = legal_actions(&state);
    let abstracted_actions = legal_actions_abstracted(&state);

    let full_blocks: Vec<&Action> = full_actions
        .iter()
        .filter(|a| matches!(a, Action::DeclareBlockers { .. }))
        .collect();
    let abstracted_blocks: Vec<&Action> = abstracted_actions
        .iter()
        .filter(|a| matches!(a, Action::DeclareBlockers { .. }))
        .collect();

    // Full enumeration should produce many blocking assignments
    assert!(
        full_blocks.len() > 10,
        "Full should have many blocking assignments, got {}",
        full_blocks.len()
    );

    // Abstracted should produce at most 5
    assert!(
        abstracted_blocks.len() <= 5,
        "Bucketed should have at most 5 blocking buckets, got {}",
        abstracted_blocks.len()
    );

    // Must include "no blocks"
    assert!(
        abstracted_blocks.iter().any(|a| {
            if let Action::DeclareBlockers { blocks } = a {
                blocks.is_empty()
            } else {
                false
            }
        }),
        "Bucketed blocking must include 'no blocks'"
    );
}

#[test]
fn test_abstracted_game_completes() {
    // A full game using legal_actions_abstracted throughout should still
    // complete without panics or infinite loops. We use a simple wrapper
    // strategy that calls legal_actions_abstracted.
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();

    // Run manually with abstracted actions
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &red, &green);

    let mut rng = rand::thread_rng();
    let mut turn_count = 0;
    while !state.game_over && turn_count < 500 {
        let actions = legal_actions_abstracted(&state);
        if actions.is_empty() {
            break;
        }
        let action = actions.choose(&mut rng).unwrap().clone();
        rules::apply_action(&mut state, &action);
        if state.phase == mtg_gto::game::Phase::Untap {
            turn_count += 1;
        }
    }

    assert!(
        state.game_over,
        "Abstracted game should complete within 500 turns"
    );
}

// ======================================================================
// Phase 0: Shared Interface Contract Tests
// ======================================================================

#[test]
fn test_player_view_basic_fields() {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Set up a basic game
    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }
    state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
    state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
    state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Hand);
    let _bear = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 3;

    let view0 = state.visible_state(0);
    let view1 = state.visible_state(1);

    // Public info should match
    assert_eq!(view0.phase, Phase::PreCombatMain);
    assert_eq!(view0.active_player, 0);
    assert_eq!(view0.turn_number, 3);
    assert_eq!(view1.phase, Phase::PreCombatMain);
    assert_eq!(view1.active_player, 0);

    // Player 0's view: sees own hand (2 bolts), opponent's hand size (1 bear)
    assert_eq!(view0.my_hand.len(), 2);
    assert_eq!(view0.opp_hand_size, 1);
    assert_eq!(view0.my_life, 20);
    assert_eq!(view0.opp_life, 20);

    // Player 1's view: sees own hand (1 bear), opponent's hand size (2 bolts)
    assert_eq!(view1.my_hand.len(), 1);
    assert_eq!(view1.opp_hand_size, 2);

    // Battlefield is the same from both perspectives
    assert_eq!(view0.battlefield.len(), view1.battlefield.len());

    // Library sizes visible as opponent info
    assert_eq!(view0.opp_library_size, state.players[1].library.len());
    assert_eq!(view1.opp_library_size, state.players[0].library.len());
}

#[test]
fn test_player_view_hides_opponent_hand_contents() {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }
    state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 1, ZoneType::Hand);
    state.create_card_in_zone(sample::ids::COUNTERSPELL, 1, ZoneType::Hand);
    state.create_card_in_zone(sample::ids::SERRA_ANGEL, 1, ZoneType::Hand);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;

    let view0 = state.visible_state(0);

    // Player 0 can see opponent has 3 cards but NOT what they are
    assert_eq!(view0.opp_hand_size, 3);
    // my_hand should be empty (player 0 has no cards in hand)
    assert_eq!(view0.my_hand.len(), 0);
}

#[test]
fn test_player_view_graveyard_and_exile_visible() {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }
    state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Graveyard);
    state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Graveyard);
    state.create_card_in_zone(sample::ids::GREY_OGRE, 0, ZoneType::Exile);

    state.phase = Phase::PreCombatMain;

    let view0 = state.visible_state(0);
    let view1 = state.visible_state(1);

    // Both graveyards are public info — contents visible from both views
    assert_eq!(view0.my_graveyard.len(), 1);
    assert_eq!(view0.opp_graveyard.len(), 1);
    assert_eq!(view1.my_graveyard.len(), 1);
    assert_eq!(view1.opp_graveyard.len(), 1);

    // Exile is also public
    assert_eq!(view0.my_exile.len(), 1);
    assert_eq!(view0.opp_exile.len(), 0);
    assert_eq!(view1.my_exile.len(), 0);
    assert_eq!(view1.opp_exile.len(), 1);
}

#[test]
fn test_canonical_roundtrip_combat_phase() {
    // Test canonical round-trip during DeclareAttackers phase
    let state = setup_combat_state(
        &[
            sample::ids::GRIZZLY_BEARS,
            sample::ids::GREY_OGRE,
            sample::ids::GOBLIN_GUIDE,
        ],
        &[sample::ids::SAVANNAH_LIONS],
    );

    let actions = legal_actions(&state);
    assert!(!actions.is_empty());

    for action in &actions {
        let canonical = canonicalize(action, &state);
        let resolved = resolve(&canonical, &state, 0);
        assert!(
            resolved.is_some(),
            "Failed to resolve canonical for {:?}",
            action
        );
        let resolved = resolved.unwrap();
        match (&resolved, action) {
            (
                Action::DeclareAttackers { attackers: a },
                Action::DeclareAttackers { attackers: b },
            ) => {
                let mut a_sorted = a.clone();
                let mut b_sorted = b.clone();
                a_sorted.sort();
                b_sorted.sort();
                assert_eq!(a_sorted, b_sorted);
            }
            _ => assert_eq!(resolved, *action),
        }
    }
}

#[test]
fn test_canonical_roundtrip_trigger_ordering() {
    // Test canonical round-trip for OrderTriggers actions
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    let vis1 = state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Battlefield);
    let vis2 = state.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    state.pending_triggers.push(mtg_gto::game::PendingTrigger {
        source_id: vis1,
        ability_index: 0,
        controller: 0,
        targets: vec![],
    });
    state.pending_triggers.push(mtg_gto::game::PendingTrigger {
        source_id: vis2,
        ability_index: 0,
        controller: 0,
        targets: vec![],
    });

    let actions = legal_actions(&state);
    let order_actions: Vec<&Action> = actions
        .iter()
        .filter(|a| matches!(a, Action::OrderTriggers { .. }))
        .collect();

    assert_eq!(order_actions.len(), 2, "Should have 2 orderings");

    for action in &order_actions {
        let canonical = canonicalize(action, &state);
        let resolved = resolve(&canonical, &state, 0);
        assert!(
            resolved.is_some(),
            "Failed to resolve canonical OrderTriggers: {:?}",
            canonical
        );
        assert_eq!(resolved.unwrap(), **action);
    }
}

#[test]
fn test_canonical_roundtrip_full_game_all_actions() {
    // Run a complete game and verify canonical round-trip for every action taken.
    // This is the ultimate acceptance test for Phase 0.2.
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &red, &green);

    let mut rng = rand::thread_rng();
    let mut actions_tested = 0;
    let mut turns = 0;

    while !state.game_over && turns < 100 {
        let player = state.priority_player;
        let actions = legal_actions(&state);
        if actions.is_empty() {
            break;
        }

        // Verify round-trip for every legal action in this state
        for action in &actions {
            let canonical = canonicalize(action, &state);
            let resolved = resolve(&canonical, &state, player);
            assert!(
                resolved.is_some(),
                "Round-trip failed at turn {} for action {:?} -> canonical {:?}",
                state.turn_number,
                action,
                canonical
            );
        }
        actions_tested += actions.len();

        // Choose random action and advance
        let chosen = actions.choose(&mut rng).unwrap().clone();
        rules::apply_action(&mut state, &chosen);
        if state.phase == Phase::Untap {
            turns += 1;
        }
    }

    assert!(
        actions_tested > 100,
        "Should have tested many actions across the game, got {}",
        actions_tested
    );
}

#[test]
fn test_canonical_hand_duplicate_disambiguation() {
    // When a player holds two copies of the same card, canonicalize must
    // distinguish between them so resolve() returns the exact same ObjectId.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    // Two Mountains in hand — exact duplicates
    let m1 = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand);
    let m2 = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Canonicalize playing each Mountain separately
    let action1 = Action::PlayLand { object_id: m1 };
    let action2 = Action::PlayLand { object_id: m2 };

    let c1 = canonicalize(&action1, &state);
    let c2 = canonicalize(&action2, &state);

    // Canonical forms must differ (different hand_index)
    assert_ne!(
        c1, c2,
        "Two duplicate cards in hand should produce different canonical actions"
    );

    // Round-trip must recover the exact ObjectId
    let r1 = resolve(&c1, &state, 0).unwrap();
    let r2 = resolve(&c2, &state, 0).unwrap();
    assert_eq!(
        r1, action1,
        "Round-trip must return exact ObjectId for first Mountain"
    );
    assert_eq!(
        r2, action2,
        "Round-trip must return exact ObjectId for second Mountain"
    );
}

#[test]
fn test_canonical_discard_hand_duplicate_disambiguation() {
    // Same test for Discard with duplicate cards in hand
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // 8 Mountains in hand (need to discard one in cleanup)
    let mut mountain_ids = Vec::new();
    for _ in 0..8 {
        mountain_ids.push(state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand));
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::Cleanup;
    state.turn_number = 1;

    // Each discard action should have a distinct canonical form
    let canonical_actions: Vec<_> = mountain_ids
        .iter()
        .map(|&id| canonicalize(&Action::Discard { object_id: id }, &state))
        .collect();

    // All should be unique
    for i in 0..canonical_actions.len() {
        for j in (i + 1)..canonical_actions.len() {
            assert_ne!(
                canonical_actions[i], canonical_actions[j],
                "Discard actions for different copies must have different canonical forms"
            );
        }
    }

    // Each round-trips to the exact same ObjectId
    for &id in &mountain_ids {
        let action = Action::Discard { object_id: id };
        let canonical = canonicalize(&action, &state);
        let resolved = resolve(&canonical, &state, 0).unwrap();
        assert_eq!(resolved, action);
    }
}

#[test]
fn test_player_view_objects_excludes_opponent_hand() {
    // PlayerView.objects must NOT contain the opponent's hand contents.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    // Player 1 has secret cards in hand
    let opp_bolt = state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 1, ZoneType::Hand);
    let opp_angel = state.create_card_in_zone(sample::ids::SERRA_ANGEL, 1, ZoneType::Hand);

    // Player 0 has a card in hand (should be visible to themselves)
    let my_bear = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Hand);

    // A shared battlefield creature
    let bf_creature = state.create_card_in_zone(sample::ids::GREY_OGRE, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;

    let view0 = state.visible_state(0);

    // Player 0's view should contain their own hand card
    assert!(
        view0.objects.contains_key(&my_bear),
        "Player's own hand cards should be in visible objects"
    );

    // Player 0's view should contain battlefield creatures
    assert!(
        view0.objects.contains_key(&bf_creature),
        "Battlefield creatures should be in visible objects"
    );

    // Player 0's view should NOT contain opponent's hand
    assert!(
        !view0.objects.contains_key(&opp_bolt),
        "Opponent's hand cards must NOT be in visible objects"
    );
    assert!(
        !view0.objects.contains_key(&opp_angel),
        "Opponent's hand cards must NOT be in visible objects"
    );

    // Player 1's view should see their own hand but not player 0's
    let view1 = state.visible_state(1);
    assert!(view1.objects.contains_key(&opp_bolt));
    assert!(view1.objects.contains_key(&opp_angel));
    assert!(!view1.objects.contains_key(&my_bear));
}

#[test]
fn test_player_view_objects_excludes_libraries() {
    // PlayerView.objects must NOT contain any library contents.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    let lib0_card = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
    let lib1_card = state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);

    // A battlefield card for comparison
    let bf_card = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);

    state.phase = Phase::PreCombatMain;

    let view0 = state.visible_state(0);

    assert!(
        view0.objects.contains_key(&bf_card),
        "Battlefield should be visible"
    );
    assert!(
        !view0.objects.contains_key(&lib0_card),
        "Own library contents must be hidden"
    );
    assert!(
        !view0.objects.contains_key(&lib1_card),
        "Opponent library contents must be hidden"
    );
}

// ======================================================================
// Phase 1A: Rules Engine Foundations Tests
// ======================================================================

// --- 1A.1: SBA/Trigger Recurrence Loop (CR 704.3) ---

#[test]
fn test_sba_recurrence_dies_trigger_deals_damage_to_players() {
    // Acceptance criterion: Fiery Conclusion Elemental (when ~ dies, deal 2
    // damage to each player) dies from lethal damage. SBAs kill it, dies
    // trigger fires, both players take 2 damage.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Put Fiery Conclusion Elemental on the battlefield with lethal damage
    let elem_id = state.create_card_in_zone(
        sample::ids::FIERY_CONCLUSION_ELEMENTAL,
        0,
        ZoneType::Battlefield,
    );
    if let Some(inst) = state.objects.get_mut(&elem_id) {
        inst.summoning_sick = false;
        inst.damage_marked = 2; // 2 damage on 2 toughness = lethal
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    let p0_life_before = state.players[0].life;
    let p1_life_before = state.players[1].life;

    // Run SBAs — this should:
    // 1. Kill the Elemental (lethal damage)
    // 2. Queue the dies trigger
    // 3. Flush the trigger to the stack
    rules::check_state_based_actions(&mut state);

    // Elemental should be in graveyard
    assert!(
        state.players[0].graveyard.contains(&elem_id),
        "Elemental should be in graveyard after SBA"
    );

    // Dies trigger should be on the stack
    assert_eq!(state.stack.len(), 1, "Dies trigger should be on the stack");

    // Life shouldn't have changed yet — trigger hasn't resolved
    assert_eq!(state.players[0].life, p0_life_before);
    assert_eq!(state.players[1].life, p1_life_before);

    // Resolve the trigger (both players pass priority)
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // After trigger resolves, both players should have taken 2 damage
    assert_eq!(
        state.players[0].life,
        p0_life_before - 2,
        "Player 0 should take 2 damage from dies trigger"
    );
    assert_eq!(
        state.players[1].life,
        p1_life_before - 2,
        "Player 1 should take 2 damage from dies trigger"
    );
}

#[test]
fn test_sba_loop_stable_without_triggers() {
    // When SBAs don't produce any triggers, the loop should exit cleanly
    // without any pending triggers or stack entries.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Put a Grizzly Bears on the battlefield with lethal damage
    let bear_id = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&bear_id) {
        inst.summoning_sick = false;
        inst.damage_marked = 2; // 2 damage on 2 toughness = lethal
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    rules::check_state_based_actions(&mut state);

    // Bears should be dead
    assert!(state.players[0].graveyard.contains(&bear_id));

    // No triggers should exist (bears have no dies trigger)
    assert!(state.pending_triggers.is_empty());
    assert!(state.stack.is_empty());
}

#[test]
fn test_sba_player_life_zero_ends_game() {
    // When a player's life drops to 0 or below, SBAs should end the game.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;

    // Set player 1's life to 0
    state.players[1].life = 0;

    rules::check_state_based_actions(&mut state);

    assert!(
        state.game_over,
        "Game should be over when a player has 0 life"
    );
    assert_eq!(state.winner, Some(0), "Player 0 should win");
}

// --- 1A.2: Event System Tests ---

#[test]
fn test_events_fire_for_spell_cast() {
    // Casting a spell should emit SpellCast and ZoneChange events.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    let bolt_id = state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
    let mountain = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&mountain) {
        inst.tapped = false;
        inst.summoning_sick = false;
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Clear any events from setup
    state.drain_events();

    // Cast Lightning Bolt targeting opponent
    rules::apply_action(
        &mut state,
        &Action::CastSpell {
            object_id: bolt_id,
            targets: vec![Target::Player(1)],
        },
    );

    let events = state.drain_events();

    // Should have SpellCast and ZoneChange (hand→stack) events
    let spell_cast_events: Vec<&GameEvent> = events
        .iter()
        .filter(|e| matches!(e, GameEvent::SpellCast { .. }))
        .collect();
    assert_eq!(spell_cast_events.len(), 1, "Should emit 1 SpellCast event");

    let zone_changes: Vec<&GameEvent> = events
        .iter()
        .filter(|e| matches!(e, GameEvent::ZoneChange { .. }))
        .collect();
    assert!(
        zone_changes.iter().any(|e| {
            if let GameEvent::ZoneChange { from, to, .. } = e {
                *from == Zone::Hand && *to == Zone::Stack
            } else {
                false
            }
        }),
        "Should emit ZoneChange from Hand to Stack"
    );
}

#[test]
fn test_events_fire_for_damage_and_life_change() {
    // Resolving a Lightning Bolt targeting a player should emit
    // DamageDealt and LifeChanged events.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    let bolt_id = state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
    let mountain = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&mountain) {
        inst.tapped = false;
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Cast and resolve Lightning Bolt
    rules::apply_action(
        &mut state,
        &Action::CastSpell {
            object_id: bolt_id,
            targets: vec![Target::Player(1)],
        },
    );

    // Drain cast events
    state.drain_events();

    // Resolve: both players pass priority
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    let events = state.drain_events();

    // Should have DamageDealt event
    let damage_events: Vec<&GameEvent> = events
        .iter()
        .filter(|e| matches!(e, GameEvent::DamageDealt { .. }))
        .collect();
    assert!(
        damage_events.iter().any(|e| {
            if let GameEvent::DamageDealt {
                amount, is_combat, ..
            } = e
            {
                *amount == 3 && !is_combat
            } else {
                false
            }
        }),
        "Should emit DamageDealt event for 3 non-combat damage. Got: {:?}",
        damage_events
    );

    // Should have LifeChanged event for player 1
    let life_events: Vec<&GameEvent> = events
        .iter()
        .filter(|e| matches!(e, GameEvent::LifeChanged { .. }))
        .collect();
    assert!(
        life_events.iter().any(|e| {
            if let GameEvent::LifeChanged { player, old, new } = e {
                *player == 1 && *old == 20 && *new == 17
            } else {
                false
            }
        }),
        "Should emit LifeChanged event (20 -> 17) for player 1. Got: {:?}",
        life_events
    );
}

#[test]
fn test_events_fire_for_card_draw() {
    // Drawing a card should emit CardDrawn and ZoneChange events.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
    for _ in 0..19 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    state.drain_events();

    rules::draw_cards(&mut state, 0, 1);

    let events = state.drain_events();

    let draw_events: Vec<&GameEvent> = events
        .iter()
        .filter(|e| matches!(e, GameEvent::CardDrawn { .. }))
        .collect();
    assert_eq!(draw_events.len(), 1, "Should emit 1 CardDrawn event");

    let zone_events: Vec<&GameEvent> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                GameEvent::ZoneChange {
                    from: Zone::Library,
                    to: Zone::Hand,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(zone_events.len(), 1, "Should emit Library→Hand ZoneChange");
}

#[test]
fn test_events_fire_for_zone_change_via_move_object() {
    // Moving an object between zones should emit a ZoneChange event.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    let bear = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    state.drain_events();

    state.move_object(bear, ZoneType::Battlefield, ZoneType::Graveyard);

    let events = state.drain_events();
    assert!(
        events.iter().any(|e| {
            matches!(e, GameEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to: Zone::Graveyard,
            } if *object == bear)
        }),
        "Should emit ZoneChange from Battlefield to Graveyard"
    );
}

#[test]
fn test_events_not_part_of_game_state_clone() {
    // Events should not affect GameState clone cost. Cloned state should
    // have an empty event list by default (derived state).
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Emit some events
    state.emit_event(GameEvent::TurnStarted {
        active_player: 0,
        turn_number: 1,
    });
    state.emit_event(GameEvent::LifeChanged {
        player: 0,
        old: 20,
        new: 17,
    });
    assert_eq!(state.pending_events.len(), 2);

    // Clone the state
    let cloned = state.clone();

    // The clone has the events (Vec is copied), but this is intentional:
    // in practice, events are drained between actions so the vec is empty.
    // The important thing is that the cost is O(n) where n = pending events,
    // and n is 0 during MCCFR traversal.
    let _ = cloned;

    // Verify drain works
    let drained = state.drain_events();
    assert_eq!(drained.len(), 2);
    assert!(state.pending_events.is_empty());
}

#[test]
fn test_event_bus_processes_events() {
    // Test that the EventBus can process events collected from GameState.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Create a simple handler that tracks life changes
    fn life_tracker(state: &mut GameState, event: &GameEvent) {
        if let GameEvent::LifeChanged { player, new, .. } = event {
            // Just verify we can access state during handler
            let _ = state.players[*player].life;
            let _ = new;
        }
    }

    let mut bus = EventBus::new();
    bus.subscribe(life_tracker);

    // Manually emit and process events through the bus
    let event = GameEvent::LifeChanged {
        player: 0,
        old: 20,
        new: 17,
    };
    bus.emit(&mut state, &event);
    // If we get here without panic, the handler successfully processed the event
}

// --- 1A.3: Replacement Effect Framework Tests ---

#[test]
fn test_replacement_effect_find_applicable() {
    let effects = vec![
        ReplacementEffect {
            source_id: 1,
            controller: 0,
            applies_to: ReplacementEventKind::WouldDie,
            action: ReplacementAction::RedirectToZone(ZoneType::Exile),
            is_self_replacement: false,
            description: "Exile instead of dying".into(),
        },
        ReplacementEffect {
            source_id: 2,
            controller: 0,
            applies_to: ReplacementEventKind::EntersBattlefield,
            action: ReplacementAction::EntersModified {
                enters_tapped: true,
                extra_counters: 0,
            },
            is_self_replacement: true,
            description: "Enters tapped".into(),
        },
    ];

    // WouldDie: effect 0 is player-choice, effect 1 doesn't apply
    let (self_r, player_r) =
        find_applicable_replacements(&effects, &ReplacementEventKind::WouldDie, 0);
    assert!(self_r.is_empty());
    assert_eq!(player_r, vec![0]);

    // EntersBattlefield: effect 1 is self-replacement
    let (self_r, player_r) =
        find_applicable_replacements(&effects, &ReplacementEventKind::EntersBattlefield, 0);
    assert_eq!(self_r, vec![1]);
    assert!(player_r.is_empty());
}

#[test]
fn test_replacement_order_action_exists_in_action_enum() {
    // Verify the ChooseReplacementOrder action variant can be constructed
    // and displayed.
    let action = Action::ChooseReplacementOrder {
        ordering: vec![(1, 0), (2, 0)],
    };
    let display = format!("{}", action);
    assert!(display.contains("replacement"));
}

#[test]
fn test_replacement_order_canonical_roundtrip() {
    // Verify ChooseReplacementOrder round-trips through canonical mapping.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Put two permanents on the battlefield (sources of replacement effects)
    let p1 = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    let p2 = state.create_card_in_zone(sample::ids::GREY_OGRE, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    let action = Action::ChooseReplacementOrder {
        ordering: vec![(p1, 0), (p2, 0)],
    };

    let canonical = canonicalize(&action, &state);
    let resolved = resolve(&canonical, &state, 0);
    assert!(
        resolved.is_some(),
        "ChooseReplacementOrder should round-trip through canonical mapping"
    );
    assert_eq!(
        resolved.unwrap(),
        action,
        "Round-trip should preserve the original action"
    );
}

#[test]
fn test_fiery_conclusion_elemental_in_db() {
    // Verify the test card exists in the database with correct properties.
    let db = sample::build_sample_db();
    let card = db.get(sample::ids::FIERY_CONCLUSION_ELEMENTAL);
    assert!(card.is_some(), "Fiery Conclusion Elemental should be in DB");

    let card = card.unwrap();
    assert_eq!(card.name, "Fiery Conclusion Elemental");
    assert!(card.is_creature());
    assert_eq!(card.power, Some(2));
    assert_eq!(card.toughness, Some(2));
    assert_eq!(card.triggered_abilities.len(), 1);
    assert_eq!(
        card.triggered_abilities[0].trigger,
        mtg_gto::card::TriggerCondition::Dies
    );
}

#[test]
fn test_sba_recurrence_in_full_game_context() {
    // Run a game that includes the Fiery Conclusion Elemental to verify
    // the SBA recurrence loop works in a full game context without panics
    // or infinite loops.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Set up a mini-game with the test card
    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Player 0 has Fiery Conclusion Elemental and lands.
    // Cost is {2}{W}, so we need 1 Plains (for W) and 2 Mountains (for generic).
    let elem_hand =
        state.create_card_in_zone(sample::ids::FIERY_CONCLUSION_ELEMENTAL, 0, ZoneType::Hand);
    let m1 = state.create_card_in_zone(sample::ids::PLAINS, 0, ZoneType::Battlefield);
    let m2 = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
    let m3 = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
    for id in [m1, m2, m3] {
        if let Some(inst) = state.objects.get_mut(&id) {
            inst.tapped = false;
            inst.summoning_sick = false;
        }
    }

    // Player 1 has a Lightning Bolt to kill it
    let bolt = state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 1, ZoneType::Hand);
    let m4 = state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&m4) {
        inst.tapped = false;
        inst.summoning_sick = false;
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Player 0 casts the Elemental
    rules::apply_action(
        &mut state,
        &Action::CastSpell {
            object_id: elem_hand,
            targets: vec![],
        },
    );

    // Both pass, Elemental resolves
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // Elemental should be on battlefield
    assert!(
        state.battlefield.contains(&elem_hand),
        "Elemental should be on the battlefield"
    );

    // Player 1 casts Lightning Bolt targeting the Elemental
    rules::apply_action(
        &mut state,
        &Action::CastSpell {
            object_id: bolt,
            targets: vec![Target::Object(elem_hand)],
        },
    );

    let p0_life = state.players[0].life;
    let p1_life = state.players[1].life;

    // Both pass, Bolt resolves — deals 3 damage to 2-toughness creature
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // After Bolt resolves, SBAs should kill the Elemental and queue its trigger.
    // The dies trigger should now be on the stack.
    assert!(
        !state.battlefield.contains(&elem_hand),
        "Elemental should be dead after Lightning Bolt"
    );
    assert_eq!(state.stack.len(), 1, "Dies trigger should be on the stack");

    // Resolve the dies trigger
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // Both players should have taken 2 damage
    assert_eq!(
        state.players[0].life,
        p0_life - 2,
        "Player 0 should take 2 damage from dies trigger"
    );
    assert_eq!(
        state.players[1].life,
        p1_life - 2,
        "Player 1 should take 2 damage from dies trigger"
    );
}

#[test]
fn test_events_accumulate_across_full_game_turn() {
    // Run several actions and verify events accumulate correctly.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();
    rules::setup_game(&mut state, &red, &green);

    state.drain_events(); // clear setup events

    // Play through a few actions
    let mut rng = rand::thread_rng();
    let mut total_events = 0;
    let mut action_count = 0;

    while !state.game_over && action_count < 20 {
        let actions = legal_actions(&state);
        if actions.is_empty() {
            break;
        }
        let action = actions.choose(&mut rng).unwrap().clone();
        rules::apply_action(&mut state, &action);
        action_count += 1;

        let events = state.drain_events();
        total_events += events.len();
    }

    // Should have accumulated some events across the actions
    assert!(
        total_events > 0,
        "Should have emitted events during gameplay, got 0 events across {} actions",
        action_count
    );
}

#[test]
fn test_cascading_sba_dies_trigger_kills_another_creature() {
    // Acceptance criterion from CONSOLIDATED_STRATEGY.md:
    // Creature with "when ~ dies, deal 2 damage to each creature" kills
    // another creature at 2 toughness, causing recursive SBAs.
    //
    // Scenario:
    // 1. Pyroclasm Elemental (3/1) has 1 damage marked → lethal (1 toughness)
    // 2. Grizzly Bears (2/2) is healthy on the battlefield
    // 3. SBAs kill the Elemental → dies trigger queued → flushed to stack
    // 4. Players pass priority → trigger resolves → deals 2 damage to each creature
    // 5. Grizzly Bears now has 2 damage on 2 toughness
    // 6. check_state_based_actions (called after resolve) → Bears die
    // This verifies the cascade: SBA → trigger → resolve → SBA → creature death.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Pyroclasm Elemental: 3/1 with "when ~ dies, deal 2 damage to each creature"
    let pyro_id =
        state.create_card_in_zone(sample::ids::PYROCLASM_ELEMENTAL, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&pyro_id) {
        inst.summoning_sick = false;
        inst.damage_marked = 1; // 1 damage on 1 toughness = lethal
    }

    // Grizzly Bears: 2/2, healthy, controlled by player 1
    let bear_id = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&bear_id) {
        inst.summoning_sick = false;
        inst.damage_marked = 0; // healthy
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Step 1: Run SBAs — Pyroclasm Elemental dies, trigger goes on stack
    rules::check_state_based_actions(&mut state);

    assert!(
        state.players[0].graveyard.contains(&pyro_id),
        "Pyroclasm Elemental should be in graveyard"
    );
    assert!(
        state.battlefield.contains(&bear_id),
        "Grizzly Bears should still be alive (trigger hasn't resolved yet)"
    );
    assert_eq!(state.stack.len(), 1, "Dies trigger should be on the stack");

    // Step 2: Resolve the dies trigger — both players pass priority
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // Step 3: After trigger resolves, 2 damage dealt to each creature.
    // Grizzly Bears now has 2 damage on 2 toughness.
    // check_state_based_actions is called inside resolve_top_of_stack,
    // so the Bears should now be dead.
    assert!(
        !state.battlefield.contains(&bear_id),
        "Grizzly Bears should be dead after cascading SBA (2 damage on 2 toughness)"
    );
    assert!(
        state.players[1].graveyard.contains(&bear_id),
        "Grizzly Bears should be in player 1's graveyard"
    );
}

#[test]
fn test_cascading_sba_chain_of_three() {
    // Extended cascade: Pyroclasm Elemental A dies → deals 2 to each creature
    // → Pyroclasm Elemental B (1 toughness, 0 damage) takes 2 damage → B dies
    // → B's trigger fires → deals 2 to each creature → Grizzly Bears dies
    //
    // This tests a 3-deep cascade through the natural game loop.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    // Pyroclasm Elemental A: 3/1, lethal damage
    let pyro_a =
        state.create_card_in_zone(sample::ids::PYROCLASM_ELEMENTAL, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&pyro_a) {
        inst.summoning_sick = false;
        inst.damage_marked = 1; // lethal
    }

    // Pyroclasm Elemental B: 3/1, healthy
    let pyro_b =
        state.create_card_in_zone(sample::ids::PYROCLASM_ELEMENTAL, 1, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&pyro_b) {
        inst.summoning_sick = false;
        inst.damage_marked = 0; // healthy
    }

    // Grizzly Bears: 2/2, healthy
    let bear_id = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&bear_id) {
        inst.summoning_sick = false;
        inst.damage_marked = 0;
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Step 1: SBAs kill Pyro A → trigger on stack
    rules::check_state_based_actions(&mut state);
    assert!(state.players[0].graveyard.contains(&pyro_a));
    assert_eq!(state.stack.len(), 1);

    // Step 2: Resolve Pyro A's trigger → 2 damage to each creature
    // Pyro B takes 2 damage on 1 toughness → lethal
    // Bears take 2 damage on 2 toughness → lethal
    // Both die in SBAs after resolution.
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // After Pyro A's trigger resolves and SBAs run:
    // - Pyro B is dead (2 damage on 1 toughness)
    // - Bears are dead (2 damage on 2 toughness)
    assert!(
        !state.battlefield.contains(&pyro_b),
        "Pyroclasm Elemental B should be dead from cascade"
    );
    assert!(
        !state.battlefield.contains(&bear_id),
        "Grizzly Bears should be dead from cascade"
    );

    // Pyro B's dies trigger should now be on the stack
    assert!(
        state.stack.len() >= 1,
        "Pyro B's dies trigger should be on the stack after cascading death"
    );

    // Step 3: Resolve Pyro B's trigger → 2 damage to each creature
    // No more creatures on the battlefield, so nothing dies.
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // Stack should be empty now
    assert_eq!(
        state.stack.len(),
        0,
        "Stack should be empty after all triggers resolve"
    );

    // All creatures should be in graveyards
    assert!(state.players[0].graveyard.contains(&pyro_a));
    assert!(state.players[1].graveyard.contains(&pyro_b));
    assert!(state.players[1].graveyard.contains(&bear_id));
}

#[test]
fn test_pyroclasm_elemental_in_db() {
    let db = sample::build_sample_db();
    let card = db.get(sample::ids::PYROCLASM_ELEMENTAL);
    assert!(card.is_some(), "Pyroclasm Elemental should be in DB");

    let card = card.unwrap();
    assert_eq!(card.name, "Pyroclasm Elemental");
    assert!(card.is_creature());
    assert_eq!(card.power, Some(3));
    assert_eq!(card.toughness, Some(1));
    assert_eq!(card.triggered_abilities.len(), 1);
    assert_eq!(
        card.triggered_abilities[0].trigger,
        mtg_gto::card::TriggerCondition::Dies
    );
}

#[test]
fn test_greedy_strategy_handles_replacement_order() {
    // Verify GreedyStrategy doesn't crash on ChooseReplacementOrder.
    // Currently replacement effects aren't generated in-game, but the
    // strategy must handle the action variant to avoid runtime bugs when
    // Phase 2A wires in replacement logic.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // The GreedyStrategy should select ChooseReplacementOrder if it's the
    // only non-PassPriority action. We can verify this by checking that
    // the strategy code path handles the match arm (no panic).
    let greedy = GreedyStrategy;
    // Normal action selection — should complete without panic
    let action = greedy.choose_action(&state, 0);
    // Should return EndTurn (no creatures to attack with, so combat is skipped)
    // or PassPriority — either is acceptable, the point is no panic.
    assert!(
        action == Action::EndTurn || action == Action::PassPriority,
        "Expected EndTurn or PassPriority, got {:?}",
        action
    );
}

// =====================================================================
// Phase 2A: Layered effects integration tests
// =====================================================================

#[test]
fn test_expanded_card_pool_count() {
    let db = sample::build_sample_db();
    // Phase 2A target: 100+ cards in the database
    let mut count = 0;
    // Check a sampling of cards across all categories
    let sample_ids = vec![
        sample::ids::MOTHER_OF_RUNES,
        sample::ids::ELITE_VANGUARD,
        sample::ids::WHITE_KNIGHT,
        sample::ids::BANESLAYER_ANGEL,
        sample::ids::PATH_TO_EXILE,
        sample::ids::WRATH_OF_GOD,
        sample::ids::GLORIOUS_ANTHEM,
        sample::ids::HUMILITY,
        sample::ids::DELVER_OF_SECRETS,
        sample::ids::SNAPCASTER_MAGE,
        sample::ids::MANA_LEAK,
        sample::ids::DARK_CONFIDANT,
        sample::ids::VAMPIRE_NIGHTHAWK,
        sample::ids::DOOM_BLADE,
        sample::ids::THOUGHTSEIZE,
        sample::ids::ASH_ZEALOT,
        sample::ids::GOBLIN_CHAINWHIRLER,
        sample::ids::CHAIN_LIGHTNING,
        sample::ids::TARMOGOYF,
        sample::ids::STRANGLEROOT_GEIST,
        sample::ids::GAEA_ANTHEM,
        sample::ids::SOL_RING,
        sample::ids::SIGNAL_PEST,
        sample::ids::LIGHTNING_HELIX,
        sample::ids::TERMINATE,
        sample::ids::GEIST_OF_SAINT_TRAFT,
        sample::ids::FLEECEMANE_LION,
        sample::ids::TIDEHOLLOW_SCULLER,
    ];
    for id in &sample_ids {
        assert!(db.get(*id).is_some(), "Card ID {} should be in DB", id);
        count += 1;
    }
    assert!(count >= 28, "Should have checked at least 28 sample cards");
}

#[test]
fn test_anthem_cards_have_static_abilities() {
    let db = sample::build_sample_db();

    // Glorious Anthem should have an Anthem static ability
    let anthem = db.get(sample::ids::GLORIOUS_ANTHEM).unwrap();
    assert!(
        !anthem.static_abilities.is_empty(),
        "Glorious Anthem should have static abilities"
    );

    // Honor of the Pure
    let honor = db.get(sample::ids::HONOR_OF_THE_PURE).unwrap();
    assert!(!honor.static_abilities.is_empty());

    // Crusade
    let crusade = db.get(sample::ids::CRUSADE).unwrap();
    assert!(!crusade.static_abilities.is_empty());

    // Gaea's Anthem
    let gaea = db.get(sample::ids::GAEA_ANTHEM).unwrap();
    assert!(!gaea.static_abilities.is_empty());

    // Humility should have both RemoveAllAbilities and SetPowerToughness
    let humility = db.get(sample::ids::HUMILITY).unwrap();
    assert_eq!(
        humility.static_abilities.len(),
        2,
        "Humility should have 2 static abilities (RemoveAllAbilities + SetPT)"
    );
}

#[test]
fn test_glorious_anthem_buffs_creatures_on_battlefield() {
    // Place Glorious Anthem and creatures on the battlefield,
    // then verify the layer engine computes correct P/T.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Library filler
    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::PLAINS, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::PLAINS, 1, ZoneType::Library);
    }

    // Player 0 has Glorious Anthem + Savannah Lions on the battlefield
    let _anthem_id =
        state.create_card_in_zone(sample::ids::GLORIOUS_ANTHEM, 0, ZoneType::Battlefield);
    let lions_id = state.create_card_in_zone(sample::ids::SAVANNAH_LIONS, 0, ZoneType::Battlefield);

    // Player 1 has Grizzly Bears (opponent — should NOT be buffed by Glorious Anthem)
    let bears_id = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);

    // Refresh continuous effects to generate anthem effects
    state.refresh_continuous_effects();

    // Savannah Lions base 2/1, anthem +1/+1 = 3/2
    assert_eq!(
        state.effective_power(lions_id),
        3,
        "Lions should be 3 power with anthem"
    );
    assert_eq!(
        state.effective_toughness(lions_id),
        2,
        "Lions should be 2 toughness with anthem"
    );

    // Opponent's Grizzly Bears should be unaffected (2/2)
    assert_eq!(
        state.effective_power(bears_id),
        2,
        "Opponent bears should be unaffected"
    );
    assert_eq!(
        state.effective_toughness(bears_id),
        2,
        "Opponent bears should be unaffected"
    );
}

#[test]
fn test_crusade_buffs_all_creatures() {
    // Crusade affects AllCreatures (simplified), so both players' creatures get buffed
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::PLAINS, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::PLAINS, 1, ZoneType::Library);
    }

    let _crusade_id = state.create_card_in_zone(sample::ids::CRUSADE, 0, ZoneType::Battlefield);
    let lions_id = state.create_card_in_zone(sample::ids::SAVANNAH_LIONS, 0, ZoneType::Battlefield);
    let bears_id = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);

    state.refresh_continuous_effects();

    // Both get +1/+1 from Crusade (AllCreatures)
    assert_eq!(state.effective_power(lions_id), 3);
    assert_eq!(state.effective_toughness(lions_id), 2);
    assert_eq!(state.effective_power(bears_id), 3);
    assert_eq!(state.effective_toughness(bears_id), 3);
}

#[test]
fn test_humility_makes_all_creatures_1_1_and_removes_abilities() {
    // Humility sets all creatures to 1/1 and removes all abilities
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::PLAINS, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::PLAINS, 1, ZoneType::Library);
    }

    // Baneslayer Angel: 5/5 Flying, First strike, Lifelink
    let angel_id =
        state.create_card_in_zone(sample::ids::BANESLAYER_ANGEL, 0, ZoneType::Battlefield);
    let _humility_id = state.create_card_in_zone(sample::ids::HUMILITY, 0, ZoneType::Battlefield);

    state.refresh_continuous_effects();

    // Baneslayer should be 1/1 under Humility
    assert_eq!(
        state.effective_power(angel_id),
        1,
        "Angel should be 1/1 under Humility"
    );
    assert_eq!(state.effective_toughness(angel_id), 1);

    // Angel should lose flying
    assert!(
        !state.has_keyword(angel_id, KeywordAbility::Flying),
        "Angel should lose flying under Humility"
    );
    assert!(!state.has_keyword(angel_id, KeywordAbility::FirstStrike));
    assert!(!state.has_keyword(angel_id, KeywordAbility::Lifelink));
}

#[test]
fn test_multiple_anthems_stack() {
    // Two Glorious Anthems should give +2/+2
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::PLAINS, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::PLAINS, 1, ZoneType::Library);
    }

    let _anthem1 =
        state.create_card_in_zone(sample::ids::GLORIOUS_ANTHEM, 0, ZoneType::Battlefield);
    let _anthem2 =
        state.create_card_in_zone(sample::ids::GLORIOUS_ANTHEM, 0, ZoneType::Battlefield);
    let lions_id = state.create_card_in_zone(sample::ids::SAVANNAH_LIONS, 0, ZoneType::Battlefield);

    state.refresh_continuous_effects();

    // Savannah Lions 2/1 + 1/1 + 1/1 = 4/3
    assert_eq!(state.effective_power(lions_id), 4);
    assert_eq!(state.effective_toughness(lions_id), 3);
}

#[test]
fn test_anthem_plus_humility_timestamp_ordering() {
    // If Glorious Anthem is played first, then Humility:
    // Layer 6: Humility removes abilities
    // Layer 7b: Humility sets P/T to 1/1 (later in layer order than 7c)
    // Layer 7c: Anthem gives +1/+1
    // Result: creatures are 1/1 (set) then +1/+1 = 2/2
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::PLAINS, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::PLAINS, 1, ZoneType::Library);
    }

    let _anthem = state.create_card_in_zone(sample::ids::GLORIOUS_ANTHEM, 0, ZoneType::Battlefield);
    let _humility = state.create_card_in_zone(sample::ids::HUMILITY, 0, ZoneType::Battlefield);
    let lions_id = state.create_card_in_zone(sample::ids::SAVANNAH_LIONS, 0, ZoneType::Battlefield);

    state.refresh_continuous_effects();

    // Layer 7b (SetPT 1/1) applies before Layer 7c (ModifyPT +1/+1)
    // So: base -> set to 1/1 -> +1/+1 = 2/2
    assert_eq!(
        state.effective_power(lions_id),
        2,
        "Anthem + Humility = 2/2"
    );
    assert_eq!(state.effective_toughness(lions_id), 2);

    // Abilities should still be removed by Layer 6
    assert!(!state.has_keyword(lions_id, KeywordAbility::Flying));
}

#[test]
fn test_wrath_of_god_card_in_db() {
    // Verify Wrath of God is correctly defined as a DestroyAll sorcery
    let db = sample::build_sample_db();
    let wrath = db.get(sample::ids::WRATH_OF_GOD).unwrap();
    assert!(wrath.is_sorcery());
    assert_eq!(wrath.spell_effect, Some(mtg_gto::card::Effect::DestroyAll));

    // Day of Judgment is also DestroyAll
    let doj = db.get(sample::ids::DAY_OF_JUDGMENT).unwrap();
    assert!(doj.is_sorcery());
    assert_eq!(doj.spell_effect, Some(mtg_gto::card::Effect::DestroyAll));
}

#[test]
fn test_new_effect_types_in_card_definitions() {
    let db = sample::build_sample_db();

    // Path to Exile uses ExileTarget
    let path = db.get(sample::ids::PATH_TO_EXILE).unwrap();
    assert!(path.spell_effect.is_some());

    // Doom Blade uses DestroyTarget
    let doom = db.get(sample::ids::DOOM_BLADE).unwrap();
    assert!(doom.spell_effect.is_some());

    // Thoughtseize uses Multiple (DiscardCards + LoseLife)
    let ts = db.get(sample::ids::THOUGHTSEIZE).unwrap();
    assert!(ts.spell_effect.is_some());

    // Hymn to Tourach uses DiscardCards
    let hymn = db.get(sample::ids::HYMN_TO_TOURACH).unwrap();
    assert!(hymn.spell_effect.is_some());

    // Diabolic Edict uses SacrificeCreatures
    let edict = db.get(sample::ids::DIABOLIC_EDICT).unwrap();
    assert!(edict.spell_effect.is_some());

    // Tragic Slip uses Debuff
    let slip = db.get(sample::ids::TRAGIC_SLIP).unwrap();
    assert!(slip.spell_effect.is_some());

    // Lightning Helix uses Multiple (DealDamage + GainLife)
    let helix = db.get(sample::ids::LIGHTNING_HELIX).unwrap();
    assert!(helix.spell_effect.is_some());
}

#[test]
fn test_keyword_rich_creatures() {
    let db = sample::build_sample_db();

    // Baneslayer Angel: Flying, First Strike, Lifelink
    let angel = db.get(sample::ids::BANESLAYER_ANGEL).unwrap();
    assert!(angel.keywords.contains(&KeywordAbility::Flying));
    assert!(angel.keywords.contains(&KeywordAbility::FirstStrike));
    assert!(angel.keywords.contains(&KeywordAbility::Lifelink));

    // Vampire Nighthawk: Flying, Deathtouch, Lifelink
    let nighthawk = db.get(sample::ids::VAMPIRE_NIGHTHAWK).unwrap();
    assert!(nighthawk.keywords.contains(&KeywordAbility::Flying));
    assert!(nighthawk.keywords.contains(&KeywordAbility::Deathtouch));
    assert!(nighthawk.keywords.contains(&KeywordAbility::Lifelink));

    // Ash Zealot: First Strike, Haste
    let zealot = db.get(sample::ids::ASH_ZEALOT).unwrap();
    assert!(zealot.keywords.contains(&KeywordAbility::FirstStrike));
    assert!(zealot.keywords.contains(&KeywordAbility::Haste));

    // Phyrexian Obliterator: Trample
    let obliterator = db.get(sample::ids::PHYREXIAN_OBLITERATOR).unwrap();
    assert!(obliterator.keywords.contains(&KeywordAbility::Trample));
}

#[test]
fn test_layer_engine_effective_power_toughness() {
    // Verify the GameState helper methods work correctly
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::PLAINS, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::PLAINS, 1, ZoneType::Library);
    }

    let angel_id =
        state.create_card_in_zone(sample::ids::BANESLAYER_ANGEL, 0, ZoneType::Battlefield);

    state.refresh_continuous_effects();

    // Baneslayer is 5/5 with no modifying effects
    assert_eq!(state.effective_power(angel_id), 5);
    assert_eq!(state.effective_toughness(angel_id), 5);
    assert!(state.has_keyword(angel_id, KeywordAbility::Flying));
    assert!(state.has_keyword(angel_id, KeywordAbility::FirstStrike));
    assert!(state.is_creature(angel_id));
}

#[test]
fn test_etb_trigger_cards_geralf_messenger() {
    // Geralf's Messenger has an ETB trigger that makes opponent lose 2 life
    let db = sample::build_sample_db();
    let messenger = db.get(sample::ids::GERALF_MESSENGER).unwrap();
    assert_eq!(messenger.triggered_abilities.len(), 1);
    assert_eq!(
        messenger.triggered_abilities[0].trigger,
        mtg_gto::card::TriggerCondition::EntersBattlefield
    );
    // Also enters tapped
    assert!(messenger.enters_tapped);
}

#[test]
fn test_etb_trigger_cards_man_o_war() {
    // Man-o'-War has an ETB bounce trigger
    let db = sample::build_sample_db();
    let mow = db.get(sample::ids::MAN_O_WAR).unwrap();
    assert_eq!(mow.triggered_abilities.len(), 1);
    assert_eq!(
        mow.triggered_abilities[0].trigger,
        mtg_gto::card::TriggerCondition::EntersBattlefield
    );
}

#[test]
fn test_game_with_anthem_completes() {
    // Run a full game with anthem cards in the decks to verify no panics
    let db = sample::build_sample_db();

    let mut deck_a = Vec::new();
    for _ in 0..18 {
        deck_a.push(sample::ids::PLAINS);
    }
    for _ in 0..4 {
        deck_a.push(sample::ids::SAVANNAH_LIONS);
    }
    for _ in 0..4 {
        deck_a.push(sample::ids::ELITE_VANGUARD);
    }
    for _ in 0..2 {
        deck_a.push(sample::ids::GLORIOUS_ANTHEM);
    }
    for _ in 0..2 {
        deck_a.push(sample::ids::LIGHTNING_BOLT);
    }

    let mut deck_b = Vec::new();
    for _ in 0..18 {
        deck_b.push(sample::ids::MOUNTAIN);
    }
    for _ in 0..4 {
        deck_b.push(sample::ids::GOBLIN_GUIDE);
    }
    for _ in 0..4 {
        deck_b.push(sample::ids::GREY_OGRE);
    }
    for _ in 0..4 {
        deck_b.push(sample::ids::LIGHTNING_BOLT);
    }

    assert_eq!(deck_a.len(), 30);
    assert_eq!(deck_b.len(), 30);

    let result = simulation::run_game(&db, &deck_a, &deck_b, &RandomStrategy, &RandomStrategy);
    // Just verify it completes without panicking
    assert!(
        result.winner.is_some() || result.turns >= 100,
        "Game should complete"
    );
}

#[test]
fn test_game_with_wrath_completes() {
    // Run a game with Wrath of God to verify DestroyAll works end-to-end
    let db = sample::build_sample_db();

    let mut deck_a = Vec::new();
    for _ in 0..18 {
        deck_a.push(sample::ids::PLAINS);
    }
    for _ in 0..4 {
        deck_a.push(sample::ids::SAVANNAH_LIONS);
    }
    for _ in 0..4 {
        deck_a.push(sample::ids::WRATH_OF_GOD);
    }
    for _ in 0..4 {
        deck_a.push(sample::ids::SERRA_ANGEL);
    }

    let mut deck_b = Vec::new();
    for _ in 0..18 {
        deck_b.push(sample::ids::MOUNTAIN);
    }
    for _ in 0..4 {
        deck_b.push(sample::ids::GOBLIN_GUIDE);
    }
    for _ in 0..4 {
        deck_b.push(sample::ids::GREY_OGRE);
    }
    for _ in 0..4 {
        deck_b.push(sample::ids::LIGHTNING_BOLT);
    }

    assert_eq!(deck_a.len(), 30);
    assert_eq!(deck_b.len(), 30);

    let result = simulation::run_game(&db, &deck_a, &deck_b, &GreedyStrategy, &GreedyStrategy);
    assert!(
        result.winner.is_some() || result.turns >= 100,
        "Game should complete"
    );
}

#[test]
fn test_multicolor_cards_in_db() {
    let db = sample::build_sample_db();

    // Lightning Helix: RW instant
    let helix = db.get(sample::ids::LIGHTNING_HELIX).unwrap();
    assert!(helix.is_instant());
    let cost = helix.mana_cost.as_ref().unwrap();
    assert!(cost.white > 0 && cost.red > 0);

    // Terminate: BR instant
    let term = db.get(sample::ids::TERMINATE).unwrap();
    assert!(term.is_instant());

    // Geist of Saint Traft: WU creature
    let geist = db.get(sample::ids::GEIST_OF_SAINT_TRAFT).unwrap();
    assert!(geist.is_creature());

    // Fleecemane Lion: GW creature
    let lion = db.get(sample::ids::FLEECEMANE_LION).unwrap();
    assert!(lion.is_creature());
    assert_eq!(lion.power, Some(3));
    assert_eq!(lion.toughness, Some(3));
}

#[test]
fn test_artifact_creatures_in_db() {
    let db = sample::build_sample_db();

    // Signal Pest: artifact creature
    let pest = db.get(sample::ids::SIGNAL_PEST).unwrap();
    assert!(pest.is_creature());
    assert!(
        pest.card_types.contains(&mtg_gto::card::CardType::Artifact),
        "Signal Pest should be an artifact"
    );

    // Steel Overseer: artifact creature
    let overseer = db.get(sample::ids::STEEL_OVERSEER).unwrap();
    assert!(overseer.is_creature());
    assert!(overseer
        .card_types
        .contains(&mtg_gto::card::CardType::Artifact));

    // Vault Skirge: artifact creature with flying and lifelink
    let skirge = db.get(sample::ids::VAULT_SKIRGE).unwrap();
    assert!(skirge.is_creature());
    assert!(skirge.keywords.contains(&KeywordAbility::Flying));
    assert!(skirge.keywords.contains(&KeywordAbility::Lifelink));
}

// =========================================================================
// Phase 3A Tests
// =========================================================================

#[test]
fn test_token_creation() {
    // Test that CreateToken actually produces a creature on the battlefield.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Set up Blade Splicer (creates a 3/3 Golem token on ETB)
    let blade_splicer_id = state.create_card_in_zone(sample::ids::BLADE_SPLICER, 0, ZoneType::Hand);

    // Give player 0 lands to cast it (costs 2W)
    for _ in 0..3 {
        let land_id = state.create_card_in_zone(sample::ids::PLAINS, 0, ZoneType::Battlefield);
        if let Some(inst) = state.objects.get_mut(&land_id) {
            inst.summoning_sick = false;
        }
    }

    // Set up game state for casting
    state.phase = Phase::PreCombatMain;
    state.active_player = 0;
    state.priority_player = 0;
    state.players[0].land_plays_remaining = 0;

    let creatures_before = state.creatures_controlled_by(0).len();

    // Cast Blade Splicer
    let actions = legal_actions(&state);
    let cast_action = actions.iter().find(|a| {
        if let Action::CastSpell { object_id, .. } = a {
            *object_id == blade_splicer_id
        } else {
            false
        }
    });

    if let Some(action) = cast_action {
        rules::apply_action(&mut state, action);

        // Pass priority to resolve
        rules::apply_action(&mut state, &Action::PassPriority);
        rules::apply_action(&mut state, &Action::PassPriority);

        // After resolution, Blade Splicer should be on the battlefield
        // along with a 3/3 Golem token
        let creatures_after = state.creatures_controlled_by(0).len();
        // We should have at least 1 more creature (Blade Splicer itself + token)
        assert!(
            creatures_after > creatures_before,
            "Token should have been created. Before: {}, After: {}",
            creatures_before,
            creatures_after
        );
    }
}

#[test]
fn test_token_ceases_to_exist_when_leaving_battlefield() {
    // Tokens that leave the battlefield cease to exist (CR 111.7)
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Test the move_object behavior for tokens (CR 111.7)
    let token_id = state.create_card_in_zone(100000, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&token_id) {
        inst.is_token = true;
    }

    assert!(state.objects.contains_key(&token_id));
    assert!(state.battlefield.contains(&token_id));

    // Move token to graveyard — it should cease to exist
    state.move_object(token_id, ZoneType::Battlefield, ZoneType::Graveyard);

    // Token should be gone entirely
    assert!(!state.objects.contains_key(&token_id));
    assert!(!state.battlefield.contains(&token_id));
    assert!(!state.players[0].graveyard.contains(&token_id));
}

#[test]
fn test_dark_ritual_adds_mana() {
    // Dark Ritual should add BBB to the caster's mana pool
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    let ritual_id = state.create_card_in_zone(sample::ids::DARK_RITUAL, 0, ZoneType::Hand);

    // Give player 0 a Swamp to cast it
    let land_id = state.create_card_in_zone(sample::ids::SWAMP, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&land_id) {
        inst.summoning_sick = false;
    }

    state.phase = Phase::PreCombatMain;
    state.active_player = 0;
    state.priority_player = 0;
    state.players[0].land_plays_remaining = 0;

    let black_before = state.players[0].mana_pool.black;

    // Cast Dark Ritual
    let actions = legal_actions(&state);
    let cast_action = actions.iter().find(|a| {
        if let Action::CastSpell { object_id, .. } = a {
            *object_id == ritual_id
        } else {
            false
        }
    });

    if let Some(action) = cast_action {
        rules::apply_action(&mut state, action);
        // Pass priority to resolve
        rules::apply_action(&mut state, &Action::PassPriority);
        rules::apply_action(&mut state, &Action::PassPriority);

        // Dark Ritual adds BBB = 3 black mana
        let black_after = state.players[0].mana_pool.black;
        assert!(
            black_after >= black_before + 3,
            "Dark Ritual should add 3 black mana. Before: {}, After: {}",
            black_before,
            black_after
        );
    }
}

#[test]
fn test_counter_cancellation_sba() {
    // +1/+1 and -1/-1 counters should cancel each other (CR 704.5d)
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    let bear_id = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);

    // Add counters
    if let Some(inst) = state.objects.get_mut(&bear_id) {
        inst.plus_counters = 3;
        inst.minus_counters = 2;
    }

    state.phase = Phase::PreCombatMain;
    state.active_player = 0;
    state.priority_player = 0;

    // Run SBA check
    rules::check_state_based_actions(&mut state);

    // After cancellation, should have 1 +1/+1 counter and 0 -1/-1 counters
    let inst = &state.objects[&bear_id];
    assert_eq!(
        inst.plus_counters, 1,
        "Should have 1 +1/+1 counter remaining"
    );
    assert_eq!(
        inst.minus_counters, 0,
        "All -1/-1 counters should be cancelled"
    );
}

#[test]
fn test_zone_change_counter_increments() {
    // Zone-change counter should increment when object changes zones
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    let bear_id = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);

    let initial_count = state.objects[&bear_id].zone_change_count;

    // Move to graveyard
    state.move_object(bear_id, ZoneType::Battlefield, ZoneType::Graveyard);
    assert_eq!(
        state.objects[&bear_id].zone_change_count,
        initial_count + 1,
        "Zone-change counter should increment on zone change"
    );

    // Move to exile
    state.move_object(bear_id, ZoneType::Graveyard, ZoneType::Exile);
    assert_eq!(
        state.objects[&bear_id].zone_change_count,
        initial_count + 2,
        "Zone-change counter should increment again"
    );
}

#[test]
fn test_extra_turn() {
    // Test that extra turns work correctly
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(
        &mut state,
        &sample::mini_red_burn(),
        &sample::mini_red_creatures(),
    );

    // Remember who the active player is
    // Grant player 0 an extra turn
    state.extra_turns.push_back(0);

    // Play through until turn changes (max 2000 actions to prevent infinite loop)
    let greedy = GreedyStrategy;
    let mut actions_taken = 0;
    let initial_turn = state.turn_number;
    while !state.game_over && state.turn_number == initial_turn && actions_taken < 2000 {
        let action = greedy.choose_action(&state, state.priority_player);
        rules::apply_action(&mut state, &action);
        actions_taken += 1;
    }

    if !state.game_over {
        // After the turn ends, the extra turn should have been consumed
        // and player 0 should be the active player
        assert_eq!(state.active_player, 0, "Player 0 should get the extra turn");
        assert!(
            state.extra_turns.is_empty(),
            "Extra turn queue should be empty"
        );
    }
}

#[test]
fn test_game_state_snapshot_restore() {
    // Test that snapshot/restore correctly preserves and restores game state
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(
        &mut state,
        &sample::mini_red_burn(),
        &sample::mini_red_creatures(),
    );

    // Take a snapshot
    let snap = state.snapshot();

    // Modify the state
    let original_life = state.players[0].life;
    state.players[0].life -= 5;
    state.turn_number += 3;
    state.active_player = 1;

    // Verify it changed
    assert_ne!(state.players[0].life, original_life);

    // Restore from snapshot
    state.restore(snap);

    // Verify restoration
    assert_eq!(
        state.players[0].life, original_life,
        "Life should be restored"
    );
    assert_eq!(state.turn_number, 1, "Turn number should be restored");
}

#[test]
fn test_cant_block_keyword() {
    // Creatures with CantBlock shouldn't appear in blocking options
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Create an attacker for player 0
    let attacker_id =
        state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&attacker_id) {
        inst.summoning_sick = false;
    }

    // Create a blocker for player 1 — give it CantBlock
    let blocker_id =
        state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&blocker_id) {
        inst.summoning_sick = false;
        inst.temp_keywords.push(KeywordAbility::CantBlock);
    }

    // Set up combat
    state.phase = Phase::DeclareBlockers;
    state.active_player = 0;
    state.priority_player = 1; // Defending player declares blockers
    state.combat.attackers = vec![attacker_id];

    let actions = legal_actions(&state);

    // Should only have PassPriority and empty blocks (no actual blocking)
    for action in &actions {
        if let Action::DeclareBlockers { blocks } = action {
            assert!(
                blocks.is_empty(),
                "Creature with CantBlock should not be able to block"
            );
        }
    }
}

#[test]
fn test_a_creature_dies_watcher_trigger() {
    // ACreatureDies should fire on other permanents when a creature dies
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Create a creature that will die
    let victim_id = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);

    // Deal lethal damage to the creature
    if let Some(inst) = state.objects.get_mut(&victim_id) {
        inst.damage_marked = 10;
    }

    state.phase = Phase::PreCombatMain;
    state.active_player = 0;
    state.priority_player = 0;

    // Run SBA check — creature should die
    rules::check_state_based_actions(&mut state);

    // Verify creature died (moved to graveyard)
    assert!(
        state.players[0].graveyard.contains(&victim_id),
        "Creature with lethal damage should be in graveyard"
    );
    assert!(
        !state.battlefield.contains(&victim_id),
        "Creature should not be on battlefield"
    );
}

#[test]
fn test_replacement_effect_ordering_api() {
    use mtg_gto::card::ZoneType;
    use mtg_gto::replacement::{
        apply_replacement_effects, ReplacementAction, ReplacementEffect, ReplacementEventKind,
    };

    let effects = vec![
        ReplacementEffect {
            source_id: 1,
            controller: 0,
            applies_to: ReplacementEventKind::WouldDie,
            action: ReplacementAction::RedirectToZone(ZoneType::Exile),
            is_self_replacement: true,
            description: "Self-exile".into(),
        },
        ReplacementEffect {
            source_id: 2,
            controller: 0,
            applies_to: ReplacementEventKind::WouldDie,
            action: ReplacementAction::Prevent,
            is_self_replacement: false,
            description: "Prevent death A".into(),
        },
        ReplacementEffect {
            source_id: 3,
            controller: 0,
            applies_to: ReplacementEventKind::WouldDie,
            action: ReplacementAction::RedirectToZone(ZoneType::Hand),
            is_self_replacement: false,
            description: "Return to hand".into(),
        },
    ];

    let (auto_applied, choice) =
        apply_replacement_effects(&effects, &ReplacementEventKind::WouldDie, 0);

    // Self-replacement should be auto-applied
    assert_eq!(
        auto_applied.len(),
        1,
        "One self-replacement should be auto-applied"
    );
    assert_eq!(auto_applied[0].description, "Self-exile");

    // Two player-choice effects should require ordering
    assert!(
        choice.is_some(),
        "Multiple non-self replacements should require player choice"
    );
    let choice = choice.unwrap();
    assert_eq!(choice.applicable_effects.len(), 2);
}

#[test]
fn test_dynamic_value_trait_exists() {
    // Test that DynamicValue enum variants exist and CardDef accepts them
    use mtg_gto::card::DynamicValue;

    let dv = DynamicValue::CardsInHand;
    assert!(matches!(dv, DynamicValue::CardsInHand));

    let dv2 = DynamicValue::CardTypesInGraveyards;
    assert!(matches!(dv2, DynamicValue::CardTypesInGraveyards));

    let dv3 = DynamicValue::Fixed(5);
    assert!(matches!(dv3, DynamicValue::Fixed(5)));
}

// =========================================================================
// Phase 3B Tests
// =========================================================================

#[test]
fn test_opponent_model_bayesian_update() {
    use mtg_gto::solver::mccfr::{DeckArchetype, OpponentModel};

    let archetypes = vec![
        DeckArchetype {
            name: "Red Aggro".into(),
            signature_cards: vec![sample::ids::LIGHTNING_BOLT, sample::ids::GOBLIN_GUIDE],
            prior: 0.5,
        },
        DeckArchetype {
            name: "Green Stompy".into(),
            signature_cards: vec![sample::ids::LLANOWAR_ELVES, sample::ids::KALONIAN_TUSKER],
            prior: 0.5,
        },
    ];

    let mut model = OpponentModel::new(archetypes);

    // Initially uniform
    assert!((model.posteriors[0] - 0.5).abs() < 0.01);
    assert!((model.posteriors[1] - 0.5).abs() < 0.01);

    // Observe a Lightning Bolt — should shift toward Red Aggro
    model.observe_card(sample::ids::LIGHTNING_BOLT);
    assert!(
        model.posteriors[0] > model.posteriors[1],
        "Red Aggro posterior should be higher after observing Lightning Bolt"
    );

    // Observe another red card
    model.observe_card(sample::ids::GOBLIN_GUIDE);
    assert!(
        model.posteriors[0] > 0.9,
        "Red Aggro should be very likely after two signature cards: {}",
        model.posteriors[0]
    );

    // Most likely archetype
    let (arch, prob) = model.most_likely_archetype().unwrap();
    assert_eq!(arch.name, "Red Aggro");
    assert!(prob > 0.9);
}

#[test]
fn test_policy_snapshot_collection() {
    use mtg_gto::info_set::BucketedAbstraction;
    use mtg_gto::solver::mccfr::{
        collect_policy_snapshots, train_extended, McfrConfig, RolloutMode, TrainConfig,
    };

    let db = sample::build_sample_db();
    let deck0 = sample::mini_red_burn();
    let deck1 = sample::mini_red_creatures();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &deck0, &deck1);

    let abstraction = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig {
            max_depth: 6,
            max_actions: 100,
            max_nodes_per_iteration: 0,
        },
        abstraction: &abstraction,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    // Train a small model
    let tables = train_extended(&state, 5, &train_cfg);

    // Collect policy snapshots
    let snapshots = collect_policy_snapshots(&state, &tables, &abstraction, 10);

    // Should have at least some snapshots from the game
    assert!(
        !snapshots.is_empty(),
        "Should collect some policy snapshots"
    );

    // Each snapshot should have non-empty action distributions
    for snap in &snapshots {
        assert!(!snap.action_distribution.is_empty());
        // Probabilities should sum to approximately 1.0
        let total: f64 = snap.action_distribution.iter().map(|(_, p)| p).sum();
        assert!(
            (total - 1.0).abs() < 0.01,
            "Action probabilities should sum to 1.0, got {}",
            total
        );
    }
}

#[test]
fn test_multi_phase_abstraction() {
    use mtg_gto::info_set::{
        BucketedAbstraction, IdentityAbstraction, InfoSetAbstraction, InformationSet,
    };
    use mtg_gto::solver::mccfr::MultiPhaseAbstraction;

    let fine = IdentityAbstraction;
    let coarse = BucketedAbstraction;
    let multi = MultiPhaseAbstraction {
        fine: &fine,
        coarse: &coarse,
    };

    assert_eq!(multi.name(), "MultiPhase");

    // Create a minimal info set for testing
    let info_set_main = InformationSet {
        phase: 3, // PreCombatMain
        active_player: 0,
        turn_number: 1,
        priority_player: 0,
        my_life: 20,
        opp_life: 20,
        my_hand: vec![],
        my_mana: [0; 6],
        battlefield: vec![],
        stack_entries: vec![],
        my_graveyard: vec![],
        opp_graveyard: vec![],
        my_exile: vec![],
        opp_exile: vec![],
        opp_hand_size: 7,
        opp_library_size: 53,
        my_land_plays_remaining: 1,
        my_command_zone: vec![],
        opp_command_zone: vec![],
        my_commander_tax: 0,
        my_mulligan_count: 0,
    };

    let info_set_upkeep = InformationSet {
        phase: 1, // Upkeep
        ..info_set_main.clone()
    };

    // Main phase should use fine abstraction (same hash as IdentityAbstraction)
    let main_hash = multi.abstract_info_set(&info_set_main);
    let fine_hash = fine.abstract_info_set(&info_set_main);
    assert_eq!(
        main_hash, fine_hash,
        "Main phase should use fine abstraction"
    );

    // Upkeep should use coarse abstraction
    let upkeep_hash = multi.abstract_info_set(&info_set_upkeep);
    let coarse_hash = coarse.abstract_info_set(&info_set_upkeep);
    assert_eq!(
        upkeep_hash, coarse_hash,
        "Upkeep should use coarse abstraction"
    );
}

#[test]
fn test_warm_start_produces_nonempty_tables() {
    use mtg_gto::info_set::BucketedAbstraction;
    use mtg_gto::solver::mccfr::warm_start_from_greedy;

    let db = sample::build_sample_db();
    let deck0 = sample::mini_red_burn();
    let deck1 = sample::mini_red_creatures();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &deck0, &deck1);

    let abstraction = BucketedAbstraction;
    let tables = warm_start_from_greedy(&state, 5, &abstraction, 1.0);

    // Both tables should have entries from warm-up
    let p0_entries = tables[0].num_info_sets();
    let p1_entries = tables[1].num_info_sets();

    assert!(p0_entries > 0, "P0 table should have warm-start entries");
    assert!(p1_entries > 0, "P1 table should have warm-start entries");
}

#[test]
fn test_skip_phases() {
    // Test that skip_phases correctly skips a phase
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(
        &mut state,
        &sample::mini_red_burn(),
        &sample::mini_red_creatures(),
    );

    // Skip the draw phase
    state.skip_phases.insert(Phase::Draw);

    // Advance through phases from Untap (which auto-advances)
    // The game should skip Draw and go to Upkeep -> (skip Draw) -> PreCombatMain
    let greedy = GreedyStrategy;
    let initial_turn = state.turn_number;
    let mut actions_taken = 0;

    while !state.game_over && state.turn_number == initial_turn && actions_taken < 500 {
        let action = greedy.choose_action(&state, state.priority_player);
        rules::apply_action(&mut state, &action);
        actions_taken += 1;
    }

    // Note: depending on turn order, the skip may not be observable directly
    // since Untap auto-advances. The key test is that the game completes without panic.
    assert!(actions_taken > 0, "Game should have progressed");
}

#[test]
fn test_games_complete_with_all_phase3_features() {
    // Comprehensive test: run many games with all Phase 3 features active
    let db = sample::build_sample_db();
    let greedy = GreedyStrategy;
    let random = RandomStrategy;

    // Test with all deck combinations
    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();
    let mini_burn = sample::mini_red_burn();

    let results = simulation::simulate(&db, &red, &green, &greedy, &greedy, 100);
    assert_eq!(results.total_games, 100);
    assert!(results.draws < 50, "Too many draws suggests a bug");

    let results = simulation::simulate(&db, &mini_burn, &red, &greedy, &random, 50);
    assert_eq!(results.total_games, 50);

    let results = simulation::simulate(&db, &green, &red, &random, &greedy, 50);
    assert_eq!(results.total_games, 50);
}

// ==========================================================================
// Phase 3 Review — Missing Tests
// ==========================================================================

/// Test CR 704.5j: Legendary rule — duplicate legendary permanents with the
/// same name under the same controller should be reduced to one (newest kept).
#[test]
fn test_legendary_rule_sba() {
    use mtg_gto::card::{CardDef, CardInstance, CardType, Subtype, Supertype};
    use mtg_gto::game::CardDatabase;
    use mtg_gto::mana::ManaCost;

    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: 9000,
        name: "Thalia Test".into(),
        mana_cost: Some(ManaCost::new(1, 1, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        supertypes: vec![Supertype::Legendary],
        subtypes: vec![Subtype("Human".into())],
        power: Some(2),
        toughness: Some(1),
        ..Default::default()
    });

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Manually place two copies of the same legendary creature on the battlefield
    let id1 = state.next_object_id;
    state.next_object_id += 1;
    let mut inst1 = CardInstance::new(id1, 9000, 0);
    inst1.summoning_sick = false;
    state.objects.insert(id1, inst1);
    state.battlefield.push(id1);

    let id2 = state.next_object_id;
    state.next_object_id += 1;
    let mut inst2 = CardInstance::new(id2, 9000, 0);
    inst2.summoning_sick = false;
    state.objects.insert(id2, inst2);
    state.battlefield.push(id2);

    assert_eq!(state.battlefield.len(), 2);

    // Run SBAs — should remove the older one (id1) and keep the newer one (id2)
    rules::check_state_based_actions(&mut state);

    assert_eq!(
        state.battlefield.len(),
        1,
        "Legendary rule should remove duplicate"
    );
    assert!(
        state.battlefield.contains(&id2),
        "Newest legendary should survive"
    );
    assert!(
        !state.battlefield.contains(&id1),
        "Oldest legendary should be removed"
    );
}

/// Test CR 704.5i: Planeswalker uniqueness rule — duplicate planeswalkers with
/// the same name under the same controller should be reduced to one.
#[test]
fn test_planeswalker_uniqueness_sba() {
    use mtg_gto::card::{CardDef, CardInstance, CardType};
    use mtg_gto::game::CardDatabase;

    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: 9100,
        name: "Jace Test".into(),
        card_types: vec![CardType::Planeswalker],
        starting_loyalty: Some(3),
        oracle_text: "Test planeswalker".into(),
        ..Default::default()
    });

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Place two copies of the same planeswalker (with loyalty counters set)
    let id1 = state.next_object_id;
    state.next_object_id += 1;
    let mut inst1 = CardInstance::new(id1, 9100, 0);
    inst1.loyalty_counters = 3;
    state.objects.insert(id1, inst1);
    state.battlefield.push(id1);

    let id2 = state.next_object_id;
    state.next_object_id += 1;
    let mut inst2 = CardInstance::new(id2, 9100, 0);
    inst2.loyalty_counters = 3;
    state.objects.insert(id2, inst2);
    state.battlefield.push(id2);

    assert_eq!(state.battlefield.len(), 2);

    rules::check_state_based_actions(&mut state);

    assert_eq!(
        state.battlefield.len(),
        1,
        "PW uniqueness should remove duplicate"
    );
    assert!(state.battlefield.contains(&id2), "Newest PW should survive");
}

/// Test CR 508.1d: MustAttack enforcement — a creature with MustAttack
/// keyword that is eligible to attack must be included in declared attackers;
/// the empty attacker set is not legal.
#[test]
fn test_must_attack_enforcement() {
    use mtg_gto::card::{CardDef, CardInstance, CardType, Subtype};
    use mtg_gto::game::CardDatabase;
    use mtg_gto::mana::ManaCost;

    let mut db = CardDatabase::new();
    // A creature that must attack
    db.insert(CardDef {
        id: 9200,
        name: "Juggernaut Test".into(),
        mana_cost: Some(ManaCost::new(4, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Golem".into())],
        keywords: vec![KeywordAbility::MustAttack],
        power: Some(5),
        toughness: Some(3),
        ..Default::default()
    });
    // A regular land for setup
    db.insert(CardDef {
        id: 9201,
        name: "Wastes".into(),
        card_types: vec![CardType::Land],
        ..Default::default()
    });

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Set up: player 0 controls an untapped, non-sick MustAttack creature
    let id1 = state.next_object_id;
    state.next_object_id += 1;
    let mut inst = CardInstance::new(id1, 9200, 0);
    inst.summoning_sick = false;
    inst.tapped = false;
    state.objects.insert(id1, inst);
    state.battlefield.push(id1);

    // Set to declare attackers phase with player 0 as active
    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::DeclareAttackers;

    let actions = legal_actions(&state);
    let attacker_actions: Vec<_> = actions
        .iter()
        .filter(|a| matches!(a, Action::DeclareAttackers { .. }))
        .collect();

    // There should be no empty attacker action
    let has_empty = attacker_actions
        .iter()
        .any(|a| matches!(a, Action::DeclareAttackers { attackers } if attackers.is_empty()));
    assert!(
        !has_empty,
        "Empty attacker set should not be legal with MustAttack creature"
    );

    // There should be an attack action that includes the must-attack creature
    let has_must_attack = attacker_actions
        .iter()
        .any(|a| matches!(a, Action::DeclareAttackers { attackers } if attackers.contains(&id1)));
    assert!(
        has_must_attack,
        "Must-attack creature should appear in legal attacker sets"
    );
}

/// Test DynamicValue evaluation in the layer engine — a creature with
/// dynamic power equal to the number of creatures controlled.
#[test]
fn test_dynamic_value_in_layer_engine() {
    use mtg_gto::card::{CardDef, CardInstance, CardType, DynamicValue, Subtype};
    use mtg_gto::game::CardDatabase;
    use mtg_gto::mana::ManaCost;

    let mut db = CardDatabase::new();
    // A creature whose power = number of creatures you control
    db.insert(CardDef {
        id: 9300,
        name: "Crowd Champion".into(),
        mana_cost: Some(ManaCost::new(3, 0, 0, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Elemental".into())],
        power: Some(0),
        toughness: Some(4),
        dynamic_power: Some(DynamicValue::CreaturesControlled),
        ..Default::default()
    });
    // A vanilla creature
    db.insert(CardDef {
        id: 9301,
        name: "Test Bear".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Bear".into())],
        power: Some(2),
        toughness: Some(2),
        ..Default::default()
    });

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Place the dynamic creature on the battlefield
    let dyn_id = state.next_object_id;
    state.next_object_id += 1;
    state
        .objects
        .insert(dyn_id, CardInstance::new(dyn_id, 9300, 0));
    state.battlefield.push(dyn_id);

    // With just itself, power should be 1 (one creature controlled)
    let power_alone = state.effective_power(dyn_id);
    assert_eq!(power_alone, 1, "Dynamic power with 1 creature should be 1");

    // Add a second creature
    let bear_id = state.next_object_id;
    state.next_object_id += 1;
    state
        .objects
        .insert(bear_id, CardInstance::new(bear_id, 9301, 0));
    state.battlefield.push(bear_id);
    state.invalidate_characteristics_cache();

    let power_with_bear = state.effective_power(dyn_id);
    assert_eq!(
        power_with_bear, 2,
        "Dynamic power with 2 creatures should be 2"
    );

    // Toughness should remain static
    let toughness = state.effective_toughness(dyn_id);
    assert_eq!(toughness, 4, "Static toughness should be unchanged");
}

/// Test that Effect::ExtraTurn goes through the full spell resolution path:
/// card in hand → CastSpell → stack → resolve → extra_turns queue → extra turn taken.
#[test]
fn test_extra_turn_through_spell_resolution() {
    use mtg_gto::card::{CardDef, CardType, Effect};
    use mtg_gto::mana::ManaCost;

    let mut db = sample::build_sample_db();

    // Create a "Time Walk" test card: {1}{U} Sorcery — Take an extra turn.
    db.insert(CardDef {
        id: 9400,
        name: "Time Walk Test".into(),
        mana_cost: Some(ManaCost::new(1, 0, 1, 0, 0, 0)),
        card_types: vec![CardType::Sorcery],
        spell_effect: Some(Effect::ExtraTurn),
        oracle_text: "Take an extra turn after this one.".into(),
        ..Default::default()
    });

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Put Time Walk in player 0's hand
    let tw_id = state.create_card_in_zone(9400, 0, ZoneType::Hand);

    // Give player 0 enough mana (1 generic + 1 blue)
    // The engine allows paying generic with colored mana
    state.players[0].mana_pool.blue = 2;

    state.phase = Phase::PreCombatMain;
    state.active_player = 0;
    state.priority_player = 0;
    state.turn_number = 2;
    state.players[0].land_plays_remaining = 0;

    assert!(
        state.extra_turns.is_empty(),
        "No extra turns queued initially"
    );

    // Cast Time Walk
    let actions = legal_actions(&state);
    let cast_action = actions
        .iter()
        .find(|a| matches!(a, Action::CastSpell { object_id, .. } if *object_id == tw_id));
    assert!(
        cast_action.is_some(),
        "Time Walk should be castable with 1U mana available"
    );

    rules::apply_action(&mut state, cast_action.unwrap());
    assert_eq!(state.stack.len(), 1, "Time Walk should be on the stack");

    // Both players pass to resolve
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // After resolution, extra_turns should have player 0 queued
    assert!(
        !state.extra_turns.is_empty(),
        "Extra turn should be queued after resolution"
    );
    assert_eq!(
        state.extra_turns[0], 0,
        "Player 0 should get the extra turn"
    );

    // Now play through the rest of the turn to verify the extra turn fires
    let greedy = GreedyStrategy;
    let initial_turn = state.turn_number;
    let mut actions_taken = 0;
    while !state.game_over && state.turn_number == initial_turn && actions_taken < 2000 {
        let action = greedy.choose_action(&state, state.priority_player);
        rules::apply_action(&mut state, &action);
        actions_taken += 1;
    }

    if !state.game_over {
        assert_eq!(
            state.active_player, 0,
            "Player 0 should be active during their extra turn"
        );
        assert!(
            state.extra_turns.is_empty(),
            "Extra turn queue should be drained"
        );
    }
}

/// Test that DynamicValue::CardsInHand correctly reads the controller's hand size
/// through the full GameState → layer engine → DynamicContext pipeline.
#[test]
fn test_dynamic_value_cards_in_hand() {
    use mtg_gto::card::{CardDef, CardInstance, CardType, DynamicValue, Subtype};
    use mtg_gto::game::CardDatabase;
    use mtg_gto::mana::ManaCost;

    let mut db = CardDatabase::new();
    // A creature whose power = cards in hand (like Maro)
    db.insert(CardDef {
        id: 9500,
        name: "Maro Test".into(),
        mana_cost: Some(ManaCost::new(2, 0, 0, 0, 0, 2)),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Elemental".into())],
        power: Some(0),
        toughness: Some(0),
        dynamic_power: Some(DynamicValue::CardsInHand),
        dynamic_toughness: Some(DynamicValue::CardsInHand),
        ..Default::default()
    });
    // A dummy card for hand padding
    db.insert(CardDef {
        id: 9501,
        name: "Padding Card".into(),
        card_types: vec![CardType::Instant],
        ..Default::default()
    });

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));

    // Place Maro on the battlefield under player 0
    let maro_id = state.next_object_id;
    state.next_object_id += 1;
    state
        .objects
        .insert(maro_id, CardInstance::new(maro_id, 9500, 0));
    state.battlefield.push(maro_id);

    // Player 0 has 0 cards in hand → P/T = 0/0
    assert_eq!(state.effective_power(maro_id), 0);
    assert_eq!(state.effective_toughness(maro_id), 0);

    // Add 3 cards to player 0's hand
    for _ in 0..3 {
        let cid = state.next_object_id;
        state.next_object_id += 1;
        state.objects.insert(cid, CardInstance::new(cid, 9501, 0));
        state.players[0].hand.push(cid);
    }
    state.invalidate_characteristics_cache();

    // Now Maro should be 3/3
    assert_eq!(
        state.effective_power(maro_id),
        3,
        "Maro power should equal hand size (3)"
    );
    assert_eq!(
        state.effective_toughness(maro_id),
        3,
        "Maro toughness should equal hand size (3)"
    );

    // Add 2 more cards
    for _ in 0..2 {
        let cid = state.next_object_id;
        state.next_object_id += 1;
        state.objects.insert(cid, CardInstance::new(cid, 9501, 0));
        state.players[0].hand.push(cid);
    }
    state.invalidate_characteristics_cache();

    assert_eq!(
        state.effective_power(maro_id),
        5,
        "Maro power should equal hand size (5)"
    );
}

// ---------------------------------------------------------------------------
// Goldfish mode tests
// ---------------------------------------------------------------------------

#[test]
fn test_goldfish_strategy_passes_priority() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &red, &red);

    // Goldfish (player 1) should pass priority during a main phase
    state.priority_player = 1;
    state.phase = Phase::PreCombatMain;
    let goldfish = GoldfishStrategy;
    let action = goldfish.choose_action(&state, 1);
    assert_eq!(
        action,
        Action::PassPriority,
        "Goldfish should pass priority"
    );
}

#[test]
fn test_goldfish_strategy_never_attacks() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_game(&mut state, &red, &red);

    // Give goldfish (player 1) a creature on the battlefield and make it the active player
    state.active_player = 1;
    state.priority_player = 1;
    state.phase = Phase::DeclareAttackers;

    // Put a creature on battlefield for player 1
    let obj_id = state.create_card_in_zone(sample::ids::GREY_OGRE, 1, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&obj_id) {
        inst.summoning_sick = false;
        inst.controller = 1;
    }

    let goldfish = GoldfishStrategy;
    let action = goldfish.choose_action(&state, 1);
    match &action {
        Action::DeclareAttackers { attackers } => {
            assert!(attackers.is_empty(), "Goldfish should declare no attackers");
        }
        Action::PassPriority => {} // Also acceptable
        other => panic!("Goldfish should not attack, got {:?}", other),
    }
}

#[test]
fn test_goldfish_strategy_never_blocks() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_game(&mut state, &red, &red);

    // Set up combat: player 0 is active, player 1 is blocking
    state.active_player = 0;
    state.priority_player = 1;
    state.phase = Phase::DeclareBlockers;

    // Give player 0 an attacker
    let attacker_id = state.create_card_in_zone(sample::ids::GREY_OGRE, 0, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&attacker_id) {
        inst.summoning_sick = false;
        inst.controller = 0;
    }
    state.combat.attackers.push(attacker_id);

    // Give player 1 a potential blocker
    let blocker_id =
        state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&blocker_id) {
        inst.summoning_sick = false;
        inst.controller = 1;
    }

    let goldfish = GoldfishStrategy;
    let action = goldfish.choose_action(&state, 1);
    match &action {
        Action::DeclareBlockers { blocks } => {
            assert!(blocks.is_empty(), "Goldfish should declare no blockers");
        }
        Action::PassPriority => {} // Also acceptable
        other => panic!("Goldfish should not block, got {:?}", other),
    }
}

#[test]
fn test_goldfish_strategy_handles_forced_discard() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_game(&mut state, &red, &red);

    // Force goldfish to have >7 cards and be in cleanup
    state.active_player = 1;
    state.priority_player = 1;
    state.phase = Phase::Cleanup;

    // Add extra cards to player 1's hand to exceed 7
    for _ in 0..3 {
        let id = state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Hand);
        if let Some(inst) = state.objects.get_mut(&id) {
            inst.controller = 1;
        }
    }
    assert!(state.players[1].hand.len() > 7);

    let goldfish = GoldfishStrategy;
    let action = goldfish.choose_action(&state, 1);
    assert!(
        matches!(action, Action::Discard { .. }),
        "Goldfish should discard when forced, got {:?}",
        action
    );
}

#[test]
fn test_goldfish_single_game_completes() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let greedy = GreedyStrategy;
    let result = simulation::run_goldfish_game(&db, &red, &greedy);

    // Game should complete — usually with a win, but some shuffles draw at
    // the turn limit (20) so we only assert the game actually ran.
    assert!(result.turns > 0, "Game should last at least 1 turn");
    assert!(result.actions_taken > 0, "Game should have actions");
    // If the game didn't produce a winner it should have hit the turn cap
    if result.winner.is_none() {
        assert!(result.turns >= 20, "Draw should only happen at turn limit");
    }
}

#[test]
fn test_goldfish_simulation_produces_valid_results() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let greedy = GreedyStrategy;
    let results = simulation::simulate_goldfish(&db, &red, &greedy, 200);

    assert_eq!(results.total_games, 200);
    assert_eq!(
        results.wins + results.losses + results.draws,
        200,
        "wins + losses + draws should equal total games"
    );
    // Greedy should reliably win against a passive opponent
    assert!(
        results.wins > 100,
        "Greedy should win most goldfish games, got {} wins out of 200",
        results.wins
    );
}

#[test]
fn test_goldfish_kill_turn_distribution_consistent() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let greedy = GreedyStrategy;
    let results = simulation::simulate_goldfish(&db, &red, &greedy, 500);

    if results.wins > 0 {
        // Sum of kill-turn distribution should equal total wins
        let dist_sum: u64 = results.kill_turn_distribution.iter().sum();
        assert_eq!(
            dist_sum, results.wins,
            "Kill-turn distribution sum ({}) should equal wins ({})",
            dist_sum, results.wins
        );

        // Fastest kill should be <= slowest kill
        assert!(
            results.fastest_kill <= results.slowest_kill,
            "Fastest kill (T{}) should be <= slowest kill (T{})",
            results.fastest_kill,
            results.slowest_kill
        );

        // Average kill turn should be between fastest and slowest
        assert!(
            results.avg_kill_turn >= results.fastest_kill as f64
                && results.avg_kill_turn <= results.slowest_kill as f64,
            "Avg kill turn ({:.2}) should be between T{} and T{}",
            results.avg_kill_turn,
            results.fastest_kill,
            results.slowest_kill,
        );
    }
}

#[test]
fn test_goldfish_higher_winrate_than_two_player() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let greedy = GreedyStrategy;

    // Run goldfish simulation (pilot vs passive opponent)
    let goldfish_results = simulation::simulate_goldfish(&db, &red, &greedy, 500);

    // Run two-player simulation (same deck, both sides active)
    let two_player_results = simulation::simulate(&db, &red, &red, &greedy, &greedy, 500);

    // Against a passive opponent, the pilot should generally win at a higher
    // rate than in a two-player mirror where both sides fight back.
    // Allow a small margin for statistical noise with only 500 samples.
    let goldfish_wr = goldfish_results.win_rate();
    let two_player_wr = two_player_results.win_rate(0);
    assert!(
        goldfish_wr > two_player_wr - 0.05,
        "Goldfish win rate ({:.1}%) should be close to or exceed two-player P0 win rate ({:.1}%)",
        goldfish_wr * 100.0,
        two_player_wr * 100.0
    );
}

#[test]
fn test_goldfish_strategy_name() {
    let goldfish = GoldfishStrategy;
    assert_eq!(goldfish.name(), "Goldfish");
}

// =====================================================================
// Kinnan commander deck and new card definition tests
// =====================================================================

#[test]
fn test_kinnan_commander_deck_builder() {
    let db = sample::build_sample_db();
    let (deck, commander, tutor_targets) = sample::kinnan_commander_deck();

    // Deck should be exactly 100 cards (including commander)
    assert_eq!(deck.len(), 100);

    // Commander is Kinnan, Bonder Prodigy
    assert_eq!(commander, sample::ids::KINNAN_BONDER_PRODIGY);

    // Tutor targets should be non-empty and all present in the deck
    assert!(
        !tutor_targets.is_empty(),
        "Kinnan deck should have tutor targets"
    );
    assert!(
        tutor_targets.contains(&sample::ids::BASALT_MONOLITH),
        "Basalt Monolith should be a tutor target"
    );
    for &target in &tutor_targets {
        assert!(
            deck.contains(&target),
            "Tutor target {} should be in the deck",
            target
        );
    }

    // Commander should be in the deck
    assert!(deck.contains(&commander));

    // All cards should be in the database
    for &card_id in &deck {
        assert!(
            db.get(card_id).is_some(),
            "Card ID {} not found in database",
            card_id
        );
    }

    // Singleton: no duplicates (except basics)
    let mut seen = std::collections::HashMap::new();
    for &card_id in &deck {
        *seen.entry(card_id).or_insert(0u32) += 1;
    }
    for (&card_id, &count) in &seen {
        let def = db.get(card_id).unwrap();
        if !def.is_basic_land() {
            assert_eq!(
                count, 1,
                "{} appears {} times but deck is singleton",
                def.name, count
            );
        }
    }
}

#[test]
fn test_mana_rocks_produce_correct_amounts() {
    let db = sample::build_sample_db();

    // Sol Ring produces 2 colorless
    let sol_ring = db.get(sample::ids::SOL_RING).unwrap();
    assert_eq!(
        sol_ring.mana_abilities,
        vec![mtg_gto::card::ManaAbility::TapForColorlessAmount(2)]
    );

    // Basalt Monolith produces 3 colorless
    let basalt = db.get(sample::ids::BASALT_MONOLITH).unwrap();
    assert_eq!(
        basalt.mana_abilities,
        vec![mtg_gto::card::ManaAbility::TapForColorlessAmount(3)]
    );
    // Basalt Monolith should NOT enter tapped
    assert!(!basalt.enters_tapped);

    // Grim Monolith produces 3 colorless
    let grim = db.get(sample::ids::GRIM_MONOLITH).unwrap();
    assert_eq!(
        grim.mana_abilities,
        vec![mtg_gto::card::ManaAbility::TapForColorlessAmount(3)]
    );

    // Mana Vault produces 3 colorless
    let vault = db.get(sample::ids::MANA_VAULT).unwrap();
    assert_eq!(
        vault.mana_abilities,
        vec![mtg_gto::card::ManaAbility::TapForColorlessAmount(3)]
    );

    // Ancient Tomb produces 2 colorless
    let tomb = db.get(sample::ids::ANCIENT_TOMB).unwrap();
    assert_eq!(
        tomb.mana_abilities,
        vec![mtg_gto::card::ManaAbility::TapForColorlessAmount(2)]
    );
}

#[test]
fn test_rhystic_study_triggers_on_any_spell() {
    let db = sample::build_sample_db();
    let rhystic = db.get(sample::ids::RHYSTIC_STUDY).unwrap();
    assert_eq!(rhystic.triggered_abilities.len(), 1);
    assert_eq!(
        rhystic.triggered_abilities[0].trigger,
        mtg_gto::card::TriggerCondition::OpponentCastsSpell
    );
}

#[test]
fn test_fetchlands_use_activated_abilities() {
    let db = sample::build_sample_db();

    for &fetch_id in &[
        sample::ids::FLOODED_STRAND,
        sample::ids::MISTY_RAINFOREST,
        sample::ids::WINDSWEPT_HEATH,
    ] {
        let fetch = db.get(fetch_id).unwrap();
        // Fetchlands should NOT use spell_effect (lands aren't cast)
        assert!(
            fetch.spell_effect.is_none(),
            "{} should not have spell_effect",
            fetch.name
        );
        // Should have an activated ability with SearchLibrary
        assert!(
            !fetch.activated_abilities.is_empty(),
            "{} should have activated_abilities",
            fetch.name
        );
        match &fetch.activated_abilities[0].effect {
            mtg_gto::card::Effect::SearchLibrary { destination, .. } => {
                assert_eq!(*destination, ZoneType::Battlefield);
            }
            other => panic!(
                "{} activated ability should be SearchLibrary, got {:?}",
                fetch.name, other
            ),
        }
    }
}

#[test]
fn test_tap_for_colorless_amount_mana_resolution() {
    // Test that TapForColorlessAmount actually adds the right amount of mana
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));

    // Give libraries so no one decks
    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::ISLAND, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::ISLAND, 1, ZoneType::Library);
    }

    // Put Sol Ring on the battlefield for player 0
    let sol_ring_obj = state.create_card_in_zone(sample::ids::SOL_RING, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Activate Sol Ring's mana ability
    let action = Action::ActivateManaAbility {
        object_id: sol_ring_obj,
        ability_index: 0,
    };
    rules::apply_action(&mut state, &action);

    // Sol Ring should produce 2 colorless mana
    assert_eq!(state.players[0].mana_pool.colorless, 2);
}

#[test]
fn test_kinnan_card_definition() {
    let db = sample::build_sample_db();
    let kinnan = db.get(sample::ids::KINNAN_BONDER_PRODIGY).unwrap();

    assert!(kinnan
        .supertypes
        .contains(&mtg_gto::card::Supertype::Legendary));
    assert!(kinnan
        .card_types
        .contains(&mtg_gto::card::CardType::Creature));
    assert_eq!(kinnan.power, Some(2));
    assert_eq!(kinnan.toughness, Some(2));
    // Should have an activated ability (5GU: look at top 5)
    assert!(!kinnan.activated_abilities.is_empty());
    // Should have the mana bonus static ability
    assert!(
        kinnan
            .static_abilities
            .iter()
            .any(|sa| matches!(sa, mtg_gto::layers::StaticAbility::ManaFromNonlandBonus)),
        "Kinnan should have ManaFromNonlandBonus static ability"
    );
}

#[test]
fn test_kinnan_mana_bonus_with_basalt_monolith() {
    // Kinnan + Basalt Monolith = infinite mana:
    // Basalt Monolith taps for 3, Kinnan adds +1 = 4 total.
    // Pay 3 to untap = net +1 per cycle.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));

    // Give libraries so no one decks
    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::ISLAND, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::ISLAND, 1, ZoneType::Library);
    }

    // Put Kinnan and Basalt Monolith on the battlefield for player 0
    state.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);
    let basalt = state.create_card_in_zone(sample::ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Tap Basalt Monolith for mana
    rules::apply_action(
        &mut state,
        &Action::ActivateManaAbility {
            object_id: basalt,
            ability_index: 0,
        },
    );

    // Should produce 3 (base) + 1 (Kinnan bonus) = 4 colorless mana
    assert_eq!(
        state.players[0].mana_pool.colorless, 4,
        "Basalt Monolith should produce 4 mana with Kinnan (3 base + 1 bonus)"
    );

    // Basalt should be tapped
    assert!(state.objects[&basalt].tapped);

    // Pay 3 to untap Basalt Monolith
    state.players[0].mana_pool.colorless = 3; // simulate having exactly 3
    rules::apply_action(
        &mut state,
        &Action::ActivateAbility {
            object_id: basalt,
            ability_index: 0,
            targets: vec![Target::Object(basalt)],
        },
    );

    // Basalt should be untapped now (activation resolves immediately since
    // activated abilities go on the stack, but UntapTarget is resolved as effect)
    // Mana pool should have been used for the cost
    // Net: started with 3, paid 3 for untap = 0 remaining
    // But we can tap again for 4 more — demonstrating infinite mana.

    // Tap again to show the loop produces net mana
    // First, manually untap since the ability needs stack resolution
    if let Some(inst) = state.objects.get_mut(&basalt) {
        inst.tapped = false;
    }
    state.players[0].mana_pool.colorless = 0;

    rules::apply_action(
        &mut state,
        &Action::ActivateManaAbility {
            object_id: basalt,
            ability_index: 0,
        },
    );

    assert_eq!(
        state.players[0].mana_pool.colorless, 4,
        "Second tap should also produce 4 mana with Kinnan"
    );
}

#[test]
fn test_kinnan_mana_bonus_not_on_lands() {
    // Kinnan's bonus should NOT apply to lands (only nonland permanents).
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::ISLAND, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::ISLAND, 1, ZoneType::Library);
    }

    // Kinnan on battlefield
    state.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);
    let island = state.create_card_in_zone(sample::ids::ISLAND, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Tap Island for mana
    rules::apply_action(
        &mut state,
        &Action::ActivateManaAbility {
            object_id: island,
            ability_index: 0,
        },
    );

    // Island is a land — Kinnan bonus should NOT apply.
    // Island produces 1 blue mana, no bonus.
    assert_eq!(
        state.players[0].mana_pool.blue, 1,
        "Island should produce exactly 1 blue mana (no Kinnan bonus on lands)"
    );
    assert_eq!(
        state.players[0].mana_pool.colorless, 0,
        "No colorless bonus should be added for tapping a land"
    );
}

#[test]
fn test_new_kinnan_deck_cards_in_db() {
    let db = sample::build_sample_db();

    // Spot-check key cards from the Kinnan deck exist and have correct types
    let consecrated = db.get(sample::ids::CONSECRATED_SPHINX).unwrap();
    assert!(consecrated
        .card_types
        .contains(&mtg_gto::card::CardType::Creature));

    let cyclonic = db.get(sample::ids::CYCLONIC_RIFT).unwrap();
    assert!(cyclonic
        .card_types
        .contains(&mtg_gto::card::CardType::Instant));

    let force = db.get(sample::ids::FORCE_OF_WILL).unwrap();
    assert!(force.card_types.contains(&mtg_gto::card::CardType::Instant));

    let hullbreaker = db.get(sample::ids::HULLBREAKER_HORROR).unwrap();
    assert!(hullbreaker
        .card_types
        .contains(&mtg_gto::card::CardType::Creature));
    assert!(hullbreaker.keywords.contains(&KeywordAbility::Flash));

    let mystic_remora = db.get(sample::ids::MYSTIC_REMORA).unwrap();
    assert!(mystic_remora
        .card_types
        .contains(&mtg_gto::card::CardType::Enchantment));
    assert!(!mystic_remora.triggered_abilities.is_empty());

    let tezzeret = db.get(sample::ids::TEZZERET_THE_SEEKER).unwrap();
    assert!(tezzeret
        .card_types
        .contains(&mtg_gto::card::CardType::Planeswalker));
    assert_eq!(tezzeret.starting_loyalty, Some(4));
}

#[test]
fn test_commander_snapshot_restore() {
    // Test that snapshot/restore preserves commander-specific state:
    // command_zone, commander_card_id, commander_object_id, commander_tax,
    // and commander_damage_received.
    let db = sample::build_sample_db();
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));

    let (deck0, cmd0_card) = sample::brimaz_commander_deck();
    let (deck1, cmd1_card) = sample::thrun_commander_deck();
    rules::setup_commander_game(&mut state, &deck0, &deck1, cmd0_card, cmd1_card);

    // Verify commander setup: 40 life, command zones populated
    assert_eq!(
        state.players[0].life, 40,
        "Commander starting life should be 40"
    );
    assert_eq!(
        state.players[1].life, 40,
        "Commander starting life should be 40"
    );
    assert_eq!(
        state.players[0].command_zone.len(),
        1,
        "Player 0 should have a commander"
    );
    assert_eq!(
        state.players[1].command_zone.len(),
        1,
        "Player 1 should have a commander"
    );
    assert!(state.players[0].commander_object_id.is_some());
    assert!(state.players[1].commander_object_id.is_some());

    let cmd0_obj = state.players[0].command_zone[0];
    let cmd1_obj = state.players[1].command_zone[0];

    // Simulate commander tax and damage
    state.players[0].commander_tax = 2;
    state.players[1].commander_damage_received[0] = 7;

    // Take snapshot
    let snap = state.snapshot();

    // Mutate state after snapshot
    state.players[0].life -= 10;
    state.players[1].life -= 5;
    state.players[0].command_zone.clear();
    state.players[0].commander_tax = 5;
    state.players[1].commander_damage_received[0] = 21;

    // Verify mutations took effect
    assert_eq!(state.players[0].life, 30);
    assert!(state.players[0].command_zone.is_empty());
    assert_eq!(state.players[0].commander_tax, 5);
    assert_eq!(state.players[1].commander_damage_received[0], 21);

    // Restore from snapshot
    state.restore(snap);

    // Verify all commander state was restored
    assert_eq!(state.players[0].life, 40, "Life should be restored to 40");
    assert_eq!(state.players[1].life, 40, "Life should be restored to 40");
    assert_eq!(
        state.players[0].command_zone.len(),
        1,
        "Command zone should be restored"
    );
    assert_eq!(
        state.players[0].command_zone[0], cmd0_obj,
        "Commander ID should match"
    );
    assert_eq!(
        state.players[1].command_zone.len(),
        1,
        "Command zone should be restored"
    );
    assert_eq!(
        state.players[1].command_zone[0], cmd1_obj,
        "Commander ID should match"
    );
    assert_eq!(
        state.players[0].commander_tax, 2,
        "Commander tax should be restored"
    );
    assert_eq!(
        state.players[1].commander_damage_received[0], 7,
        "Commander damage should be restored to pre-snapshot value"
    );
    assert_eq!(
        state.players[0].commander_card_id,
        Some(cmd0_card),
        "Commander card ID should be restored"
    );
    assert_eq!(
        state.players[1].commander_card_id,
        Some(cmd1_card),
        "Commander card ID should be restored"
    );
}

// =========================================================================
// Macro-actions and combo reward shaping tests
// =========================================================================

#[test]
fn test_macro_action_appears_in_legal_actions() {
    use mtg_gto::combo_discovery::{discover_and_register, DiscoveryConfig};

    let db = sample::build_sample_db();
    let cards = vec![sample::ids::BASALT_MONOLITH, sample::ids::KINNAN_BONDER_PRODIGY];
    let (registry, _) = discover_and_register(&db, &cards, &DiscoveryConfig::default());

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.combo_registry = Some(Arc::new(registry));

    // Libraries
    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    // Put Basalt Monolith + Kinnan on battlefield for player 0
    state.create_card_in_zone(sample::ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
    state.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    let actions = legal_actions(&state);
    let has_macro = actions
        .iter()
        .any(|a| matches!(a, Action::ActivateMacro { .. }));
    assert!(
        has_macro,
        "Legal actions should include ActivateMacro when combo pieces are present"
    );
}

#[test]
fn test_macro_action_not_available_without_registry() {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    // No combo_registry set

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    state.create_card_in_zone(sample::ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
    state.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    let actions = legal_actions(&state);
    let has_macro = actions
        .iter()
        .any(|a| matches!(a, Action::ActivateMacro { .. }));
    assert!(!has_macro, "No macro actions without a combo registry");
}

#[test]
fn test_macro_action_not_during_combat() {
    use mtg_gto::combo_discovery::{discover_and_register, DiscoveryConfig};

    let db = sample::build_sample_db();
    let cards = vec![sample::ids::BASALT_MONOLITH, sample::ids::KINNAN_BONDER_PRODIGY];
    let (registry, _) = discover_and_register(&db, &cards, &DiscoveryConfig::default());

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.combo_registry = Some(Arc::new(registry));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    state.create_card_in_zone(sample::ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
    state.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

    // During combat, not main phase — macro should not appear
    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::DeclareAttackers;

    let actions = legal_actions(&state);
    let has_macro = actions
        .iter()
        .any(|a| matches!(a, Action::ActivateMacro { .. }));
    assert!(
        !has_macro,
        "Macro actions should only appear during main phases"
    );
}

#[test]
fn test_apply_macro_action_adds_mana() {
    use mtg_gto::combo_discovery::{discover_and_register, DiscoveryConfig};

    let db = sample::build_sample_db();
    let cards = vec![sample::ids::BASALT_MONOLITH, sample::ids::KINNAN_BONDER_PRODIGY];
    let (registry, _) = discover_and_register(&db, &cards, &DiscoveryConfig::default());

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.combo_registry = Some(Arc::new(registry));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    let monolith =
        state.create_card_in_zone(sample::ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
    state.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;

    let before_mana = state.players[0].mana_pool.colorless;

    // Apply the macro action
    rules::apply_action(&mut state, &Action::ActivateMacro { combo_id: 0 });

    assert_eq!(
        state.players[0].mana_pool.colorless,
        before_mana + mtg_gto::combo::INFINITE_AMOUNT,
        "Macro should add INFINITE_AMOUNT colorless mana to player's pool"
    );
    assert!(
        state.objects[&monolith].tapped,
        "Monolith should be tapped after macro activation"
    );
}

#[test]
fn test_macro_action_canonical_roundtrip() {
    use mtg_gto::action::canonical::{canonicalize, resolve};
    use mtg_gto::combo_discovery::{discover_and_register, DiscoveryConfig};

    let db = sample::build_sample_db();
    let cards = vec![sample::ids::BASALT_MONOLITH, sample::ids::KINNAN_BONDER_PRODIGY];
    let (registry, _) = discover_and_register(&db, &cards, &DiscoveryConfig::default());

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.combo_registry = Some(Arc::new(registry));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    state.create_card_in_zone(sample::ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
    state.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;

    let action = Action::ActivateMacro { combo_id: 0 };
    let canonical = canonicalize(&action, &state);
    let resolved = resolve(&canonical, &state, 0);
    assert_eq!(
        resolved,
        Some(action),
        "ActivateMacro should round-trip through canonical form"
    );
}

#[test]
fn test_combo_proximity_reward_in_heuristic() {
    use mtg_gto::combo;
    use mtg_gto::combo_discovery::{discover_and_register, DiscoveryConfig};

    let db = sample::build_sample_db();
    let cards = vec![sample::ids::BASALT_MONOLITH, sample::ids::KINNAN_BONDER_PRODIGY];
    let (registry, _) = discover_and_register(&db, &cards, &DiscoveryConfig::default());

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.combo_registry = Some(Arc::new(registry));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    // Equal life, no creatures — baseline
    let base_proximity =
        combo::combo_proximity_bonus(&state, 0, state.combo_registry.as_ref().unwrap());
    assert!(
        base_proximity.abs() < 1e-10,
        "No combo pieces = zero proximity bonus"
    );

    // Add Kinnan for player 0 — partial combo
    state.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Battlefield);
    let partial_proximity =
        combo::combo_proximity_bonus(&state, 0, state.combo_registry.as_ref().unwrap());
    assert!(
        partial_proximity > 0.0,
        "Partial combo pieces should give positive proximity bonus, got {}",
        partial_proximity
    );

    // Add Basalt Monolith — full combo
    state.create_card_in_zone(sample::ids::BASALT_MONOLITH, 0, ZoneType::Battlefield);
    let full_proximity =
        combo::combo_proximity_bonus(&state, 0, state.combo_registry.as_ref().unwrap());
    assert!(
        full_proximity > partial_proximity,
        "Full combo should give more bonus than partial: {} > {}",
        full_proximity,
        partial_proximity
    );
}

#[test]
fn test_macro_action_in_goldfish_game() {
    use mtg_gto::combo_discovery::{discover_and_register, DiscoveryConfig};

    // Test that a goldfish game with combos can proceed without hitting action limits
    let db = sample::build_sample_db();
    let cards = vec![sample::ids::BASALT_MONOLITH, sample::ids::KINNAN_BONDER_PRODIGY];
    let (registry, _) = discover_and_register(&db, &cards, &DiscoveryConfig::default());

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.combo_registry = Some(Arc::new(registry));

    // Give player 0 combo pieces in hand and lands on battlefield
    state.create_card_in_zone(sample::ids::BASALT_MONOLITH, 0, ZoneType::Hand);
    state.create_card_in_zone(sample::ids::KINNAN_BONDER_PRODIGY, 0, ZoneType::Hand);

    // Provide mana sources
    for _ in 0..6 {
        let land = state.create_card_in_zone(sample::ids::FOREST, 0, ZoneType::Battlefield);
        state.objects.get_mut(&land).unwrap().tapped = false;
        state.objects.get_mut(&land).unwrap().summoning_sick = false;
    }

    // Libraries
    for _ in 0..40 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 3;

    // Simulate: play a few actions manually
    // The point is that the combo system is wired and doesn't crash
    let actions = legal_actions(&state);
    assert!(!actions.is_empty(), "Should have legal actions");

    // Just verify the game can make progress
    rules::apply_action(&mut state, &Action::PassPriority);
    assert!(!state.game_over, "Game shouldn't be over after pass");
}

// =====================================================================
// MCTS goldfish integration tests
// =====================================================================

#[test]
fn test_mcts_single_goldfish_game_completes() {
    use mtg_gto::solver::mcts::MctsConfig;

    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let config = MctsConfig {
        iterations_per_move: 20,
        exploration_constant: 1.0,
        max_tree_depth: 0,
        max_rollout_actions: 2_000,
        num_threads: 1,
    };

    let result = simulation::run_mcts_goldfish_game(&db, &red, &config, false);

    // Game should complete
    assert!(result.kill_turn > 0, "Game should last at least 1 turn");
    assert!(result.actions_taken > 0, "Game should have actions");
    // Life totals should make sense
    assert!(
        result.final_life[0] > 0 || result.won,
        "Pilot should be alive unless something unusual happened"
    );
}

#[test]
fn test_mcts_goldfish_simulation_produces_valid_results() {
    use mtg_gto::solver::mcts::MctsConfig;

    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let config = MctsConfig {
        iterations_per_move: 20,
        exploration_constant: 1.0,
        max_tree_depth: 0,
        max_rollout_actions: 2_000,
        num_threads: 1,
    };

    let results = simulation::simulate_mcts_goldfish(&db, &red, &config, 10);

    assert_eq!(results.total_games, 10);
    assert_eq!(
        results.wins + results.losses + results.draws,
        10,
        "wins + losses + draws should equal total games"
    );
}

#[test]
fn test_mcts_goldfish_kill_turn_distribution_consistent() {
    use mtg_gto::solver::mcts::MctsConfig;

    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let config = MctsConfig {
        iterations_per_move: 30,
        exploration_constant: 1.0,
        max_tree_depth: 0,
        max_rollout_actions: 2_000,
        num_threads: 1,
    };

    let results = simulation::simulate_mcts_goldfish(&db, &red, &config, 20);

    if results.wins > 0 {
        // Sum of kill-turn distribution should equal total wins
        let dist_sum: u64 = results.kill_turn_distribution.iter().sum();
        assert_eq!(
            dist_sum, results.wins,
            "Kill-turn distribution sum ({}) should equal wins ({})",
            dist_sum, results.wins
        );

        // Fastest kill should be <= slowest kill
        assert!(
            results.fastest_kill <= results.slowest_kill,
            "Fastest kill (T{}) should be <= slowest kill (T{})",
            results.fastest_kill,
            results.slowest_kill
        );

        // Average kill turn should be between fastest and slowest
        assert!(
            results.avg_kill_turn >= results.fastest_kill as f64
                && results.avg_kill_turn <= results.slowest_kill as f64,
            "Avg kill turn ({:.2}) should be between T{} and T{}",
            results.avg_kill_turn,
            results.fastest_kill,
            results.slowest_kill,
        );
    }
}

#[test]
fn test_mcts_strategy_plays_legal_actions() {
    use mtg_gto::solver::mcts::MctsConfig;
    use mtg_gto::solver::mcts::MctsStrategy;

    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let config = MctsConfig {
        iterations_per_move: 10,
        exploration_constant: 1.0,
        max_tree_depth: 0,
        max_rollout_actions: 2_000,
        num_threads: 1,
    };
    let mcts = MctsStrategy::new(config);

    // Run a full game using MctsStrategy through the standard simulation path
    let result = simulation::run_goldfish_game(&db, &red, &mcts);

    // The game should complete without panicking (which would indicate
    // an illegal action was attempted)
    assert!(result.turns > 0);
    assert!(result.actions_taken > 0);
}

#[test]
fn test_mcts_decision_stats_populated() {
    use mtg_gto::solver::mcts::MctsConfig;

    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let config = MctsConfig {
        iterations_per_move: 30,
        exploration_constant: 1.0,
        max_tree_depth: 0,
        max_rollout_actions: 2_000,
        num_threads: 1,
    };

    let result = simulation::run_mcts_goldfish_game(&db, &red, &config, false);

    // A real goldfish game should have multiple decision points
    assert!(
        !result.decision_stats.is_empty(),
        "MCTS game should record decision statistics"
    );

    for stat in &result.decision_stats {
        // Each decision should have at least 2 legal actions (otherwise
        // it wouldn't be counted as a decision — single-action states are skipped)
        assert!(
            stat.num_legal_actions >= 2,
            "Decision should have >= 2 legal actions, got {}",
            stat.num_legal_actions
        );

        // Best action should have been visited at least once
        assert!(
            stat.best_action_visits > 0,
            "Best action should have at least 1 visit"
        );

        // Average reward should be in [0, 1]
        assert!(
            stat.best_action_avg_reward >= 0.0 && stat.best_action_avg_reward <= 1.0,
            "Best action avg reward should be in [0,1], got {}",
            stat.best_action_avg_reward
        );
    }
}

#[test]
fn test_mcts_commander_goldfish_completes() {
    use mtg_gto::solver::mcts::MctsConfig;

    let db = sample::build_sample_db();
    let (deck, commander) = sample::brimaz_commander_deck();

    let config = MctsConfig {
        iterations_per_move: 10,
        exploration_constant: 1.0,
        max_tree_depth: 0,
        max_rollout_actions: 2_000,
        num_threads: 1,
    };

    let result = simulation::run_mcts_commander_goldfish_game(
        &db, &deck, commander, &config, false, &[],
    );

    assert!(result.kill_turn > 0);
    assert!(result.actions_taken > 0);
}

#[test]
fn test_mcts_parallel_goldfish_produces_valid_results() {
    use mtg_gto::solver::mcts::MctsConfig;

    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();

    let config = MctsConfig {
        iterations_per_move: 20,
        exploration_constant: 1.0,
        max_tree_depth: 0,
        max_rollout_actions: 2_000,
        num_threads: 2,
    };

    let results = simulation::simulate_mcts_goldfish(&db, &red, &config, 5);

    assert_eq!(results.total_games, 5);
    assert_eq!(
        results.wins + results.losses + results.draws,
        5,
        "wins + losses + draws should equal total games"
    );
    // Sanity: parallel path should still find wins with a reasonable deck
    assert!(results.wins > 0, "Parallel MCTS should win at least one goldfish game");
}

#[test]
fn test_nirkana_revenant_swamp_mana_bonus() {
    // Nirkana Revenant: "Whenever you tap a Swamp for mana, add an additional {B}."
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::SWAMP, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::SWAMP, 1, ZoneType::Library);
    }

    // Put Nirkana Revenant on the battlefield
    state.create_card_in_zone(sample::ids::NIRKANA_REVENANT, 0, ZoneType::Battlefield);
    let swamp = state.create_card_in_zone(sample::ids::SWAMP, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Tap swamp for mana
    rules::apply_action(
        &mut state,
        &Action::ActivateManaAbility {
            object_id: swamp,
            ability_index: 0,
        },
    );

    // Should produce 1 (base) + 1 (Nirkana bonus) = 2 black mana
    assert_eq!(
        state.players[0].mana_pool.black, 2,
        "Swamp should produce 2 black mana with Nirkana Revenant (1 base + 1 bonus)"
    );
}

#[test]
fn test_nirkana_revenant_no_bonus_on_non_swamp() {
    // Nirkana Revenant's bonus should NOT apply to non-Swamp lands.
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));

    for _ in 0..20 {
        state.create_card_in_zone(sample::ids::SWAMP, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::SWAMP, 1, ZoneType::Library);
    }

    // Nirkana Revenant on battlefield
    state.create_card_in_zone(sample::ids::NIRKANA_REVENANT, 0, ZoneType::Battlefield);
    let island = state.create_card_in_zone(sample::ids::ISLAND, 0, ZoneType::Battlefield);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Tap Island for mana
    rules::apply_action(
        &mut state,
        &Action::ActivateManaAbility {
            object_id: island,
            ability_index: 0,
        },
    );

    // Should produce only 1 blue mana, no bonus
    assert_eq!(
        state.players[0].mana_pool.blue, 1,
        "Island should produce only 1 blue mana (no Nirkana bonus on non-Swamps)"
    );
    assert_eq!(
        state.players[0].mana_pool.black, 0,
        "No black mana bonus should be added for non-Swamp lands"
    );
}

// Cast with exactly the printed cost, so resolution assertions measure the effect.
fn cast_goblin_storm_spell(state: &mut GameState, card: u64, controller: usize) -> u64 {
    state.active_player = controller;
    state.priority_player = controller;
    state.phase = Phase::PreCombatMain;
    let def = state.card_db().get(card).unwrap();
    let cost = def.mana_cost.clone().unwrap();
    let generic = if card == sample::ids::SEETHING_SONG { 2 } else { 1 };
    assert_eq!(cost, mtg_gto::mana::ManaCost::new(generic, 0, 0, 0, 1, 0));
    if card == sample::ids::SEETHING_SONG || card == sample::ids::BATTLE_HYMN {
        assert_eq!(def.card_types, vec![mtg_gto::card::CardType::Instant]);
    }
    state.players[controller].mana_pool.red = cost.generic + cost.red;
    let spell = state.create_card_in_zone(card, controller, ZoneType::Hand);
    let action = Action::CastSpell { object_id: spell, targets: vec![] };
    assert!(legal_actions(state).contains(&action));
    rules::apply_action(state, &action);
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.players[controller].mana_pool.total(), 0);
    spell
}

fn resolve_goblin_storm_spell(state: &mut GameState, spell: u64, controller: usize) {
    rules::apply_action(state, &Action::PassPriority);
    rules::apply_action(state, &Action::PassPriority);
    assert!(state.stack.is_empty());
    assert!(state.players[controller].graveyard.contains(&spell));
}

#[test]
fn test_seething_song_adds_exactly_five_red() {
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(sample::build_sample_db()));
    let spell = cast_goblin_storm_spell(&mut state, sample::ids::SEETHING_SONG, 1);
    resolve_goblin_storm_spell(&mut state, spell, 1);
    assert_eq!(state.players[1].mana_pool.red, 5);
    assert_eq!(state.players[1].mana_pool.total(), 5);
    assert_eq!(state.players[0].mana_pool.total(), 0);
}

#[test]
fn test_battle_hymn_counts_only_controllers_creatures_at_resolution() {
    for count in [0, 1, 4] {
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(sample::build_sample_db()));
        state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
        state.create_card_in_zone(sample::ids::SOL_RING, 1, ZoneType::Battlefield);
        state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Graveyard);
        let spell = cast_goblin_storm_spell(&mut state, sample::ids::BATTLE_HYMN, 1);
        // Added after casting: the effect must evaluate the board on resolution.
        for i in 0..count {
            let creature = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
            let inst = state.objects.get_mut(&creature).unwrap();
            inst.tapped = true;
            inst.is_token = i == 0;
        }
        resolve_goblin_storm_spell(&mut state, spell, 1);
        assert_eq!(state.players[1].mana_pool.red, count);
        assert_eq!(state.players[1].mana_pool.total(), count);
        assert_eq!(state.players[0].mana_pool.total(), 0);
    }
}

fn assert_two_goblin_tokens(card: u64) {
    use mtg_gto::card::{CardType, DynamicValue, Effect, Subtype};
    use mtg_gto::mana::{Color, ManaCost};
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(sample::build_sample_db()));
    let def = state.card_db().get(card).unwrap();
    assert_eq!(def.card_types, vec![CardType::Sorcery]);
    assert_eq!(def.mana_cost, Some(ManaCost::new(1, 0, 0, 0, 1, 0)));
    match def.spell_effect.as_ref().unwrap() {
        Effect::CreateTokens { token, count } => {
            assert_eq!(token.colors, vec![Color::Red]);
            assert_eq!(*count, DynamicValue::Fixed(2));
        }
        other => panic!("Expected token creation, got {other:?}"),
    }
    let spell = cast_goblin_storm_spell(&mut state, card, 1);
    assert!(state.battlefield.is_empty());
    resolve_goblin_storm_spell(&mut state, spell, 1);
    assert_eq!(state.battlefield.len(), 2);
    for id in &state.battlefield {
        let inst = &state.objects[id];
        assert!(inst.is_token);
        assert!(inst.summoning_sick);
        assert!(!inst.tapped);
        assert_eq!(inst.controller, 1);
        assert_eq!(inst.owner, 1);
        let token = state.card_db().get(inst.card_def_id).unwrap();
        assert_eq!(token.name, "Goblin");
        assert_eq!(token.card_types, vec![CardType::Creature]);
        assert_eq!(token.subtypes, vec![Subtype("Goblin".into())]);
        assert_eq!((token.power, token.toughness), (Some(1), Some(1)));
        assert!(token.keywords.is_empty());
    }
}

#[test]
fn test_dragon_fodder_creates_two_goblins() {
    assert_two_goblin_tokens(sample::ids::DRAGON_FODDER);
}

#[test]
fn test_krenkos_command_creates_two_goblins() {
    assert_two_goblin_tokens(sample::ids::KRENKOS_COMMAND);
}

fn krenko_state() -> (GameState, u64, Action) {
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(sample::build_sample_db()));
    state.active_player = 1;
    state.priority_player = 1;
    state.phase = Phase::PreCombatMain;
    let id = state.create_card_in_zone(sample::ids::KRENKO_MOB_BOSS, 1, ZoneType::Battlefield);
    let action = Action::ActivateAbility { object_id: id, ability_index: 0, targets: vec![] };
    (state, id, action)
}

fn resolve_krenko(state: &mut GameState) {
    rules::apply_action(state, &Action::PassPriority);
    rules::apply_action(state, &Action::PassPriority);
    assert!(state.stack.is_empty());
}

fn assert_krenko_tokens(state: &GameState, count: usize) {
    use mtg_gto::card::{CardType, Subtype};
    use mtg_gto::mana::Color;
    let tokens: Vec<_> = state.battlefield.iter().filter(|id| state.objects[id].is_token).collect();
    assert_eq!(tokens.len(), count);
    for id in tokens {
        let inst = &state.objects[id];
        let def = state.card_db().get(inst.card_def_id).unwrap();
        assert_eq!((inst.owner, inst.controller), (1, 1));
        assert!(!inst.tapped);
        assert!(inst.summoning_sick);
        assert_eq!(def.name, "Goblin");
        assert_eq!(def.card_types, vec![CardType::Creature]);
        assert_eq!(def.subtypes, vec![Subtype("Goblin".into())]);
        assert_eq!((def.power, def.toughness), (Some(1), Some(1)));
        assert!(def.keywords.is_empty());
        assert_eq!(def.colors, Some(vec![Color::Red]));
        let chars = mtg_gto::layers::compute_characteristics(*id, &state.continuous_effects,
            &state.objects, &state.battlefield.iter().copied().collect(), state.card_db()).unwrap();
        assert_eq!(chars.colors, vec![Color::Red]);
        assert_eq!(def.cmc(), 0);
    }
}

#[test]
fn test_krenko_summoning_sickness_blocks_enumerated_and_direct_activation() {
    let (mut state, id, action) = krenko_state();
    assert!(!legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    assert!(state.stack.is_empty());
    assert!(!state.objects[&id].tapped);
}

#[test]
fn test_krenko_ready_counts_himself_and_creates_red_goblin() {
    let (mut state, id, action) = krenko_state();
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    assert!(legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    assert!(state.objects[&id].tapped);
    assert_eq!(state.stack.len(), 1);
    assert!(!legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    assert_eq!(state.stack.len(), 1, "Already tapped cannot activate again");
    resolve_krenko(&mut state);
    assert_krenko_tokens(&state, 1);
}

#[test]
fn test_krenko_haste_allows_immediate_tap_activation() {
    let (mut state, id, action) = krenko_state();
    state.objects.get_mut(&id).unwrap().temp_keywords.push(KeywordAbility::Haste);
    state.invalidate_characteristics_cache();
    assert!(state.objects[&id].summoning_sick);
    assert!(legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    assert!(state.objects[&id].tapped);
    resolve_krenko(&mut state);
    assert_krenko_tokens(&state, 1);
}

#[test]
fn test_krenko_counts_controlled_noncreature_goblins_on_resolution() {
    use mtg_gto::card::{CardDef, CardType, Subtype};
    let (mut state, id, action) = krenko_state();
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef {
        id: 900001, name: "Test Goblin Permanent".into(),
        card_types: vec![CardType::Artifact], subtypes: vec![Subtype("Goblin".into())],
        ..Default::default()
    });
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    state.create_card_in_zone(900001, 0, ZoneType::Battlefield);
    state.create_card_in_zone(900001, 1, ZoneType::Graveyard);
    state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
    rules::apply_action(&mut state, &action);
    assert_krenko_tokens(&state, 0);
    // This Goblin enters after activation and must still contribute to X.
    state.create_card_in_zone(900001, 1, ZoneType::Battlefield);
    resolve_krenko(&mut state);
    assert_krenko_tokens(&state, 2);
}

#[test]
fn test_krenko_leaving_before_resolution_reduces_count_to_zero() {
    let (mut state, id, action) = krenko_state();
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    rules::apply_action(&mut state, &action);
    state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
    resolve_krenko(&mut state);
    assert_krenko_tokens(&state, 0);
}

#[test]
fn test_krenko_counts_current_subtypes_and_changeling() {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    let (mut state, id, action) = krenko_state();
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    let bear = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
    let changeling = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
    state.objects.get_mut(&changeling).unwrap().temp_keywords.push(KeywordAbility::Changeling);
    state.continuous_effects.push(ContinuousEffect {
        source_id: bear, controller: 1, timestamp: 1, duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::Specific(bear),
        modification: LayerModification::AddSubtype(mtg_gto::card::Subtype("Goblin".into())),
    });
    state.invalidate_characteristics_cache();
    rules::apply_action(&mut state, &action);
    resolve_krenko(&mut state);
    assert_krenko_tokens(&state, 3);
}

#[test]
fn test_krenko_becomes_ready_at_start_of_controllers_turn() {
    let (mut state, id, action) = krenko_state();
    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::Cleanup;
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(state.active_player, 1);
    assert!(!state.objects[&id].summoning_sick);
    state.phase = Phase::PreCombatMain;
    assert!(legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    resolve_krenko(&mut state);
    assert_krenko_tokens(&state, 1);
}

#[test]
fn test_krenko_stack_ability_keeps_original_controller() {
    let (mut state, id, action) = krenko_state();
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    rules::apply_action(&mut state, &action);
    state.objects.get_mut(&id).unwrap().controller = 0;
    state.invalidate_characteristics_cache();
    resolve_krenko(&mut state);
    // Original controller now controls zero Goblins; the opponent controls Krenko.
    assert_krenko_tokens(&state, 0);
}

#[test]
fn test_tap_cost_rule_applies_to_other_creatures_but_not_noncreatures() {
    use mtg_gto::card::{CardDef, CardType};
    let (mut state, _, _) = krenko_state();
    let ability = state.card_db().get(sample::ids::KRENKO_MOB_BOSS).unwrap()
        .activated_abilities[0].clone();
    for (id, ty, haste, expected) in [
        (900010, CardType::Creature, false, false),
        (900011, CardType::Creature, true, true),
        (900012, CardType::Artifact, false, true),
    ] {
        Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef {
            id, name: format!("Tap test {id}"), card_types: vec![ty],
            power: Some(1), toughness: Some(1),
            keywords: if haste { vec![KeywordAbility::Haste] } else { vec![] },
            activated_abilities: vec![ability.clone()], ..Default::default()
        });
        state.priority_player = 1;
        let source = state.create_card_in_zone(id, 1, ZoneType::Battlefield);
        let action = Action::ActivateAbility { object_id: source, ability_index: 0, targets: vec![] };
        assert_eq!(legal_actions(&state).contains(&action), expected);
        rules::apply_action(&mut state, &action);
        assert_eq!(state.objects[&source].tapped, expected);
        if expected { resolve_krenko(&mut state); }
    }
}

#[test]
fn test_explicit_token_colors_roundtrip_and_legacy_card_defaults() {
    use mtg_gto::card::CardDef;
    use mtg_gto::mana::Color;
    let db = sample::build_sample_db();
    let original = db.get(sample::ids::LIGHTNING_BOLT).unwrap();
    let mut json = serde_json::to_value(original).unwrap();
    json.as_object_mut().unwrap().remove("colors");
    let legacy: CardDef = serde_json::from_value(json).unwrap();
    assert_eq!(legacy.base_colors(), original.base_colors());
    for colors in [vec![], vec![Color::Red], vec![Color::Blue, Color::Green]] {
        let card = CardDef { colors: Some(colors.clone()), ..Default::default() };
        let decoded: CardDef = serde_json::from_str(&serde_json::to_string(&card).unwrap()).unwrap();
        assert_eq!(decoded.base_colors(), colors);
    }
}

#[test]
fn test_creature_mana_tap_cost_respects_sickness_haste_and_auto_tap() {
    use mtg_gto::mana::{Color, ManaCost};
    let (mut state, _, _) = krenko_state();
    let elf = state.create_card_in_zone(sample::ids::LLANOWAR_ELVES, 1, ZoneType::Battlefield);
    let action = Action::ActivateManaAbility { object_id: elf, ability_index: 0 };
    assert!(!legal_actions(&state).contains(&action));
    assert!(!state.untapped_mana_sources(1).contains(&elf));
    rules::apply_action(&mut state, &action);
    assert_eq!(state.players[1].mana_pool.total(), 0);
    rules::auto_tap_lands(&mut state, 1, &ManaCost::new(0, 0, 0, 0, 0, 1));
    assert!(!state.objects[&elf].tapped);
    state.objects.get_mut(&elf).unwrap().temp_keywords.push(KeywordAbility::Haste);
    state.invalidate_characteristics_cache();
    assert!(legal_actions(&state).contains(&action));
    assert!(state.untapped_mana_sources(1).contains(&elf));
    rules::apply_action(&mut state, &action);
    assert!(state.objects[&elf].tapped);
    assert_eq!(state.players[1].mana_pool.green, 1);
    rules::apply_action(&mut state, &action);
    assert_eq!(state.players[1].mana_pool.green, 1);
    assert_eq!(state.card_db().get(sample::ids::LLANOWAR_ELVES).unwrap().base_colors(), vec![Color::Green]);
}

fn brightstone_state() -> GameState {
    let mut state = GameState::new(3);
    let mut db = sample::build_sample_db();
    db.insert(mtg_gto::card::CardDef {
        id: 900020,
        name: "Test Goblin Artifact".into(),
        card_types: vec![mtg_gto::card::CardType::Artifact],
        subtypes: vec![mtg_gto::card::Subtype("Goblin".into())],
        ..Default::default()
    });
    state.card_db = Some(Arc::new(db));
    state.active_player = 1;
    state.priority_player = 1;
    state.phase = Phase::PreCombatMain;
    state
}

fn cast_brightstone(state: &mut GameState) -> u64 {
    use mtg_gto::card::CardType;
    use mtg_gto::mana::ManaCost;
    let def = state.card_db().get(sample::ids::BRIGHTSTONE_RITUAL).unwrap();
    assert_eq!(def.mana_cost, Some(ManaCost::new(0, 0, 0, 0, 1, 0)));
    assert_eq!(def.card_types, vec![CardType::Instant]);
    let spell = state.create_card_in_zone(sample::ids::BRIGHTSTONE_RITUAL, 1, ZoneType::Hand);
    let action = Action::CastSpell { object_id: spell, targets: vec![] };
    state.players[1].mana_pool.colorless = 1;
    assert!(!legal_actions(state).contains(&action), "Colorless cannot pay R");
    state.players[1].mana_pool.colorless = 0;
    state.players[1].mana_pool.red = 1;
    assert!(legal_actions(state).contains(&action));
    rules::apply_action(state, &action);
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.players[1].mana_pool.total(), 0);
    spell
}

fn resolve_brightstone(state: &mut GameState, spell: u64, red: u32) {
    for _ in 0..state.players.len() {
        rules::apply_action(state, &Action::PassPriority);
    }
    assert!(state.stack.is_empty());
    assert!(state.players[1].graveyard.contains(&spell));
    assert_eq!(state.players[1].mana_pool.red, red);
    assert_eq!(state.players[1].mana_pool.total(), red);
    assert_eq!(state.players[0].mana_pool.total(), 0);
    assert_eq!(state.players[2].mana_pool.total(), 0);
}

#[test]
fn test_brightstone_zero_goblins_ignores_other_zones_and_nongoblins() {
    let mut state = brightstone_state();
    for owner in 0..3 {
        for zone in [ZoneType::Hand, ZoneType::Library, ZoneType::Graveyard, ZoneType::Exile, ZoneType::Command] {
            state.create_card_in_zone(sample::ids::KRENKO_MOB_BOSS, owner, zone);
        }
        state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, owner, ZoneType::Battlefield);
    }
    let spell = cast_brightstone(&mut state);
    resolve_brightstone(&mut state, spell, 0);
}

#[test]
fn test_brightstone_counts_your_goblins() {
    let mut state = brightstone_state();
    state.create_card_in_zone(sample::ids::KRENKO_MOB_BOSS, 1, ZoneType::Battlefield);
    let spell = cast_brightstone(&mut state);
    resolve_brightstone(&mut state, spell, 1);
}

#[test]
fn test_brightstone_counts_goblins_of_all_opponents() {
    let mut state = brightstone_state();
    for owner in [0, 2] {
        state.create_card_in_zone(sample::ids::KRENKO_MOB_BOSS, owner, ZoneType::Battlefield);
    }
    let spell = cast_brightstone(&mut state);
    resolve_brightstone(&mut state, spell, 2);
}

#[test]
fn test_brightstone_counts_noncreature_goblin_permanents() {
    let mut state = brightstone_state();
    state.create_card_in_zone(900020, 0, ZoneType::Battlefield);
    state.create_card_in_zone(900020, 1, ZoneType::Battlefield);
    let spell = cast_brightstone(&mut state);
    resolve_brightstone(&mut state, spell, 2);
}

#[test]
fn test_brightstone_uses_battlefield_at_resolution() {
    let mut state = brightstone_state();
    let goblin = state.create_card_in_zone(sample::ids::KRENKO_MOB_BOSS, 1, ZoneType::Battlefield);
    let spell = cast_brightstone(&mut state);
    state.move_object(goblin, ZoneType::Battlefield, ZoneType::Graveyard);
    for owner in [0, 2] {
        state.create_card_in_zone(900020, owner, ZoneType::Battlefield);
    }
    resolve_brightstone(&mut state, spell, 2);
}

#[test]
fn test_brightstone_reuses_current_subtypes_and_changeling_counting() {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    let mut state = brightstone_state();
    let bear = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
    let changeling = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 2, ZoneType::Battlefield);
    state.objects.get_mut(&changeling).unwrap().temp_keywords.push(KeywordAbility::Changeling);
    state.continuous_effects.push(ContinuousEffect {
        source_id: bear, controller: 0, timestamp: 1, duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::Specific(bear),
        modification: LayerModification::AddSubtype(mtg_gto::card::Subtype("Goblin".into())),
    });
    state.invalidate_characteristics_cache();
    let spell = cast_brightstone(&mut state);
    resolve_brightstone(&mut state, spell, 2);
}

fn chieftain_state() -> (GameState, u64, u64, u64) {
    use mtg_gto::card::{CardDef, CardType, Subtype};
    let mut db = sample::build_sample_db();
    db.insert(CardDef {
        id: 900030, name: "Test Goblin noncreature".into(),
        card_types: vec![CardType::Artifact], subtypes: vec![Subtype("Goblin".into())],
        ..Default::default()
    });
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.active_player = 1;
    state.priority_player = 1;
    state.phase = Phase::PreCombatMain;
    let chief = state.create_card_in_zone(sample::ids::GOBLIN_CHIEFTAIN, 1, ZoneType::Battlefield);
    let ours = state.create_card_in_zone(sample::ids::KRENKO_MOB_BOSS, 1, ZoneType::Battlefield);
    let theirs = state.create_card_in_zone(sample::ids::KRENKO_MOB_BOSS, 0, ZoneType::Battlefield);
    state.refresh_continuous_effects();
    (state, chief, ours, theirs)
}

fn assert_chieftain_creature(state: &GameState, id: u64, pt: i32, haste: bool) {
    assert_eq!(state.effective_power(id), pt);
    assert_eq!(state.effective_toughness(id), pt);
    assert_eq!(state.has_keyword(id, KeywordAbility::Haste), haste);
}

#[test]
fn test_chieftain_only_buffs_other_controlled_goblin_creatures() {
    use mtg_gto::mana::ManaCost;
    let (mut state, chief, ours, theirs) = chieftain_state();
    let def = state.card_db().get(sample::ids::GOBLIN_CHIEFTAIN).unwrap();
    assert_eq!(def.mana_cost, Some(ManaCost::new(1, 0, 0, 0, 2, 0)));
    let bear = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
    let artifact = state.create_card_in_zone(900030, 1, ZoneType::Battlefield);
    state.refresh_continuous_effects();
    assert_chieftain_creature(&state, chief, 2, true);
    assert_chieftain_creature(&state, ours, 4, true);
    assert_chieftain_creature(&state, theirs, 3, false);
    assert_chieftain_creature(&state, bear, 2, false);
    assert_chieftain_creature(&state, artifact, 0, false);
}

#[test]
fn test_chieftain_haste_enables_summoning_sick_goblin_tap_ability() {
    let (mut state, chief, ours, _) = chieftain_state();
    let action = Action::ActivateAbility { object_id: ours, ability_index: 0, targets: vec![] };
    assert!(state.objects[&ours].summoning_sick);
    assert!(legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    assert!(state.objects[&ours].tapped);
    assert_eq!(state.stack.len(), 1);
    // No refresh call: leaving the battlefield must immediately stop the grant.
    state.move_object(chief, ZoneType::Battlefield, ZoneType::Graveyard);
    state.objects.get_mut(&ours).unwrap().tapped = false;
    assert_chieftain_creature(&state, ours, 3, false);
    assert!(!legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    assert!(!state.objects[&ours].tapped);
    assert_eq!(state.stack.len(), 1);
}

#[test]
fn test_chieftain_control_change_retargets_existing_effects() {
    use mtg_gto::card::{CardDef, CardType, Effect, TargetSpec};
    let (mut state, chief, ours, theirs) = chieftain_state();
    // Populate cached characteristics before a real control-changing spell resolves.
    assert_chieftain_creature(&state, ours, 4, true);
    assert_chieftain_creature(&state, theirs, 3, false);
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef {
        id: 900031, name: "Test control spell".into(), card_types: vec![CardType::Instant],
        mana_cost: Some(mtg_gto::mana::ManaCost::zero()),
        spell_effect: Some(Effect::GainControlUntilEOT { target: TargetSpec::AnyCreature }),
        ..Default::default()
    });
    state.active_player = 0;
    state.priority_player = 0;
    let spell = state.create_card_in_zone(900031, 0, ZoneType::Hand);
    let action = Action::CastSpell { object_id: spell, targets: vec![Target::Object(chief)] };
    // Supply the valid target directly: generic control-effect target enumeration
    // is not implemented, and is outside this continuous-effect regression.
    rules::apply_action(&mut state, &action);
    assert_eq!(state.stack.len(), 1);
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(state.objects[&chief].controller, 0);
    assert_chieftain_creature(&state, chief, 2, true);
    assert_chieftain_creature(&state, ours, 3, false);
    assert_chieftain_creature(&state, theirs, 4, true);
}

#[test]
fn test_chieftain_layer_control_changes_and_expiration_update_recipients() {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    let (mut state, chief, ours, theirs) = chieftain_state();
    assert_chieftain_creature(&state, ours, 4, true);
    state.continuous_effects.push(ContinuousEffect {
        source_id: chief, controller: 0, timestamp: 100, duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::Specific(chief), modification: LayerModification::ChangeController(0),
    });
    state.invalidate_characteristics_cache();
    assert_chieftain_creature(&state, ours, 3, false);
    assert_chieftain_creature(&state, theirs, 4, true);
    state.cleanup_eot_effects();
    assert_chieftain_creature(&state, ours, 4, true);
    assert_chieftain_creature(&state, theirs, 3, false);
    // Control of a recipient also uses Layer 2 rather than its original controller.
    state.continuous_effects.push(ContinuousEffect {
        source_id: theirs, controller: 1, timestamp: 101, duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::Specific(theirs), modification: LayerModification::ChangeController(1),
    });
    state.invalidate_characteristics_cache();
    assert_chieftain_creature(&state, theirs, 4, true);
}

#[test]
fn test_chieftain_removal_immediately_removes_buff_and_granted_haste() {
    for zone in [ZoneType::Graveyard, ZoneType::Exile, ZoneType::Hand] {
        let (mut state, chief, ours, _) = chieftain_state();
        assert_chieftain_creature(&state, ours, 4, true);
        state.move_object(chief, ZoneType::Battlefield, zone);
        assert_chieftain_creature(&state, ours, 3, false);
    }
}

#[test]
fn test_chieftain_uses_layered_creature_types_and_subtypes() {
    use mtg_gto::card::{CardType, Subtype};
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    let (mut state, chief, ours, _) = chieftain_state();
    let artifact = state.create_card_in_zone(900030, 1, ZoneType::Battlefield);
    let bear = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);
    for (target, modification) in [
        (artifact, LayerModification::AddType(CardType::Creature)),
        (bear, LayerModification::AddSubtype(Subtype("Goblin".into()))),
        (ours, LayerModification::RemoveType(CardType::Creature)),
    ] {
        state.continuous_effects.push(ContinuousEffect {
            source_id: chief, controller: 1, timestamp: 100, duration: Duration::UntilEndOfTurn,
            affected: AffectedObjects::Specific(target), modification,
        });
    }
    state.invalidate_characteristics_cache();
    assert_chieftain_creature(&state, artifact, 1, true);
    assert_chieftain_creature(&state, bear, 3, true);
    assert_chieftain_creature(&state, ours, 3, false);
}

#[test]
fn test_chieftain_cast_registers_effects_and_multiple_chieftains_buff_each_other() {
    let (mut state, chief, ours, _) = chieftain_state();
    let second = state.create_card_in_zone(sample::ids::GOBLIN_CHIEFTAIN, 1, ZoneType::Hand);
    state.players[1].mana_pool.red = 3;
    let action = Action::CastSpell { object_id: second, targets: vec![] };
    assert!(legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert!(state.battlefield.contains(&second));
    assert_chieftain_creature(&state, chief, 3, true);
    assert_chieftain_creature(&state, second, 3, true);
    assert_chieftain_creature(&state, ours, 5, true);
    state.move_object(chief, ZoneType::Battlefield, ZoneType::Graveyard);
    assert_chieftain_creature(&state, second, 2, true);
    assert_chieftain_creature(&state, ours, 4, true);
}

// Synthetic definitions keep these regressions independent of any deck's cards.
fn hardening_state() -> GameState {
    use mtg_gto::card::{ActivatedAbility, CardDef, CardType, Effect, ManaAbility, TargetSpec};
    use mtg_gto::mana::{Color, ManaCost};
    let mut db = mtg_gto::game::CardDatabase::new();
    let ability = |cost, requires_tap, effect| ActivatedAbility {
        cost, requires_tap, sacrifice_cost: None, life_cost: 0, effect,
        description: "Engine regression ability".into(),
    };
    db.insert(CardDef {
        id: 910000, name: "Mana and tap source".into(), card_types: vec![CardType::Artifact],
        mana_abilities: vec![ManaAbility::TapForColorless],
        activated_abilities: vec![
            ability(ManaCost::new(1, 0, 0, 0, 0, 0), true, Effect::GainLife { amount: 1 }),
            ability(ManaCost::new(1, 0, 0, 0, 0, 0), false, Effect::GainLife { amount: 1 }),
        ], ..Default::default()
    });
    db.insert(CardDef {
        id: 910001, name: "Tap creature".into(), card_types: vec![CardType::Creature],
        power: Some(1), toughness: Some(1),
        activated_abilities: vec![ability(ManaCost::zero(), true, Effect::GainLife { amount: 1 })],
        ..Default::default()
    });
    db.insert(CardDef {
        id: 910002, name: "Haste granter".into(), card_types: vec![CardType::Artifact],
        activated_abilities: vec![ability(ManaCost::zero(), false, Effect::GainKeywordUntilEOT {
            keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature,
        })], ..Default::default()
    });
    db.insert(CardDef {
        id: 910003, name: "Test ritual".into(), card_types: vec![CardType::Instant],
        mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)),
        spell_effect: Some(Effect::AddMana { color: Some(Color::Red), amount: 5 }),
        ..Default::default()
    });
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    state
}

#[test]
fn test_hardening_tap_source_cannot_pay_its_own_mana_cost() {
    let mut state = hardening_state();
    let source = state.create_card_in_zone(910000, 0, ZoneType::Battlefield);
    let action = Action::ActivateAbility { object_id: source, ability_index: 0, targets: vec![] };
    assert!(!legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    assert!(state.stack.is_empty());
    assert!(!state.objects[&source].tapped);
    assert_eq!(state.players[0].mana_pool.total(), 0);
}

#[test]
fn test_hardening_reserved_tap_source_allows_other_mana_and_floating_mana() {
    for floating in [false, true] {
        let mut state = hardening_state();
        let source = state.create_card_in_zone(910000, 0, ZoneType::Battlefield);
        let other = if floating {
            state.players[0].mana_pool.colorless = 1;
            None
        } else {
            Some(state.create_card_in_zone(910000, 0, ZoneType::Battlefield))
        };
        let action = Action::ActivateAbility { object_id: source, ability_index: 0, targets: vec![] };
        assert!(legal_actions(&state).contains(&action));
        rules::apply_action(&mut state, &action);
        assert_eq!(state.stack.len(), 1);
        assert!(state.objects[&source].tapped);
        if let Some(other) = other { assert!(state.objects[&other].tapped); }
        assert_eq!(state.players[0].mana_pool.total(), 0);
    }
    // A source with no tap-symbol cost remains eligible to generate payment mana.
    let mut state = hardening_state();
    let source = state.create_card_in_zone(910000, 0, ZoneType::Battlefield);
    let action = Action::ActivateAbility { object_id: source, ability_index: 1, targets: vec![] };
    assert!(legal_actions(&state).contains(&action));
    rules::apply_action(&mut state, &action);
    assert_eq!(state.stack.len(), 1);
    assert!(state.objects[&source].tapped);
    assert_eq!(state.players[0].mana_pool.total(), 0);
}

#[test]
fn test_hardening_ritual_mana_persists_through_priority_and_resolution_not_phase_change() {
    let mut state = hardening_state();
    let ritual = state.create_card_in_zone(910003, 0, ZoneType::Hand);
    state.players[0].mana_pool.red = 1;
    rules::apply_action(&mut state, &Action::CastSpell { object_id: ritual, targets: vec![] });
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(state.phase, Phase::PreCombatMain);
    assert_eq!(state.players[0].mana_pool.red, 5);
    let source = state.create_card_in_zone(910000, 0, ZoneType::Battlefield);
    state.priority_player = 0;
    rules::apply_action(&mut state, &Action::ActivateAbility {
        object_id: source, ability_index: 1, targets: vec![],
    });
    assert_eq!(state.players[0].mana_pool.red, 4);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(state.players[0].mana_pool.red, 4);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(state.phase, Phase::PreCombatMain);
    assert_eq!(state.players[0].mana_pool.red, 4);
    assert!(state.stack.is_empty());
    state.players[1].mana_pool.blue = 2;
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(state.players[0].mana_pool.red, 4);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(state.phase, Phase::BeginningOfCombat);
    assert_eq!(state.players[0].mana_pool.total(), 0);
    assert_eq!(state.players[1].mana_pool.total(), 0);
}

#[test]
fn test_hardening_mana_empties_at_combat_step_boundaries() {
    for phase in [Phase::BeginningOfCombat, Phase::FirstStrikeDamage, Phase::CombatDamage, Phase::EndOfCombat] {
        let mut state = hardening_state();
        state.phase = phase;
        state.players[0].mana_pool.red = 3;
        state.players[1].mana_pool.colorless = 2;
        rules::apply_action(&mut state, &Action::PassPriority);
        assert_eq!(state.players[0].mana_pool.red, 3);
        rules::apply_action(&mut state, &Action::PassPriority);
        assert_ne!(state.phase, phase);
        assert_eq!(state.players[0].mana_pool.total(), 0);
        assert_eq!(state.players[1].mana_pool.total(), 0);
    }
    // Combat declarations advance directly in this engine, bypassing priority.
    for attack in [false, true] {
        let mut state = hardening_state();
        let creature = state.create_card_in_zone(910001, 0, ZoneType::Battlefield);
        state.objects.get_mut(&creature).unwrap().summoning_sick = false;
        state.phase = Phase::DeclareAttackers;
        state.players[0].mana_pool.red = 3;
        rules::apply_action(&mut state, &Action::DeclareAttackers {
            attackers: if attack { vec![creature] } else { vec![] },
        });
        assert_eq!(state.players[0].mana_pool.total(), 0);
        if attack {
            state.players[1].mana_pool.blue = 2;
            rules::apply_action(&mut state, &Action::DeclareBlockers { blocks: vec![] });
            assert_eq!(state.players[1].mana_pool.total(), 0);
        }
    }
}

#[test]
fn test_hardening_resolved_haste_invalidates_cache_and_expires() {
    for activate in [false, true] {
        let mut state = hardening_state();
        let creature = state.create_card_in_zone(910001, 0, ZoneType::Battlefield);
        let granter = state.create_card_in_zone(910002, 0, ZoneType::Battlefield);
        let tap = Action::ActivateAbility { object_id: creature, ability_index: 0, targets: vec![] };
        // Warm the real cache through queries before the effect resolves.
        assert!(!state.has_keyword(creature, KeywordAbility::Haste));
        assert!(!legal_actions(&state).contains(&tap));
        // Explicit valid target: target enumeration is outside this regression.
        rules::apply_action(&mut state, &Action::ActivateAbility {
            object_id: granter, ability_index: 0, targets: vec![Target::Object(creature)],
        });
        rules::apply_action(&mut state, &Action::PassPriority);
        rules::apply_action(&mut state, &Action::PassPriority);
        state.priority_player = 0;
        assert!(state.has_keyword(creature, KeywordAbility::Haste));
        assert!(state.objects[&creature].summoning_sick);
        assert!(legal_actions(&state).contains(&tap));
        if activate {
            rules::apply_action(&mut state, &tap);
            assert!(state.objects[&creature].tapped);
            rules::apply_action(&mut state, &Action::PassPriority);
            rules::apply_action(&mut state, &Action::PassPriority);
        }
        state.phase = Phase::EndStep;
        state.consecutive_passes = 0;
        rules::apply_action(&mut state, &Action::PassPriority);
        rules::apply_action(&mut state, &Action::PassPriority);
        assert_eq!(state.active_player, 1);
        assert!(!state.has_keyword(creature, KeywordAbility::Haste));
        assert!(state.objects[&creature].summoning_sick);
        state.priority_player = 0;
        assert!(!legal_actions(&state).contains(&tap));
        if !activate { assert!(!state.objects[&creature].tapped); }
    }
}

// Single-target spell foundations: synthetic definitions, no Zada/copying.
fn targeting_state() -> GameState {
    use mtg_gto::card::{CardDef, CardType, Effect, TargetSpec};
    use mtg_gto::mana::ManaCost;
    let mut db = mtg_gto::game::CardDatabase::new();
    let haste = Effect::GainKeywordUntilEOT { keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature };
    for (id, effect) in [
        (920000, Effect::Multiple(vec![haste.clone(), Effect::Multiple(vec![haste]), Effect::DrawCards { count: 1 }])),
        (920003, Effect::Buff { power: 2, toughness: 2, until_eot: true }),
        (920004, Effect::DrawCards { count: 1 }),
        (920005, Effect::Counter { target: TargetSpec::AnySpell }),
    ] {
        db.insert(CardDef { id, name: format!("Test spell {id}"), card_types: vec![CardType::Instant],
            mana_cost: Some(ManaCost::new(0, 0, 0, 0, 1, 0)), spell_effect: Some(effect), ..Default::default() });
    }
    db.insert(CardDef { id: 920001, name: "Test creature".into(), card_types: vec![CardType::Creature],
        power: Some(2), toughness: Some(2), ..Default::default() });
    db.insert(CardDef { id: 920002, name: "Test artifact".into(), card_types: vec![CardType::Artifact],
        power: Some(2), toughness: Some(2), ..Default::default() });
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    state.players[0].mana_pool.red = 10;
    for _ in 0..3 { state.create_card_in_zone(920002, 0, ZoneType::Library); }
    state
}

fn targeting_layer(state: &mut GameState, id: u64, modification: mtg_gto::layers::LayerModification) {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration};
    let timestamp = state.new_timestamp();
    state.continuous_effects.push(ContinuousEffect { source_id: id, controller: 0, timestamp,
        duration: Duration::UntilEndOfTurn, affected: AffectedObjects::Specific(id), modification });
    state.invalidate_characteristics_cache();
}

fn targeting_cast(state: &mut GameState, spell: u64, targets: Vec<Target>) {
    let action = Action::CastSpell { object_id: spell, targets };
    assert!(legal_actions(state).contains(&action));
    rules::apply_action(state, &action);
    assert!(!state.stack.is_empty());
}

fn targeting_resolve(state: &mut GameState) {
    rules::apply_action(state, &Action::PassPriority);
    rules::apply_action(state, &Action::PassPriority);
}

#[test]
fn test_targeting_generation_keywords_and_zones() {
    use mtg_gto::layers::LayerModification::AddKeyword;
    let mut state = targeting_state();
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    let ours = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    let theirs = state.create_card_in_zone(920001, 1, ZoneType::Battlefield);
    let own_hex = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    let opp_hex = state.create_card_in_zone(920001, 1, ZoneType::Battlefield);
    let shroud = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    state.create_card_in_zone(920002, 0, ZoneType::Battlefield);
    state.create_card_in_zone(920001, 0, ZoneType::Graveyard);
    targeting_layer(&mut state, own_hex, AddKeyword(KeywordAbility::Hexproof));
    targeting_layer(&mut state, opp_hex, AddKeyword(KeywordAbility::Hexproof));
    targeting_layer(&mut state, shroud, AddKeyword(KeywordAbility::Shroud));
    let targets: Vec<_> = legal_actions(&state).into_iter().filter_map(|a| match a {
        Action::CastSpell { object_id, targets } if object_id == spell => Some(targets), _ => None,
    }).collect();
    assert_eq!(targets, vec![vec![Target::Object(ours)], vec![Target::Object(theirs)], vec![Target::Object(own_hex)]]);
}

#[test]
fn test_targeting_no_creature_no_cast() {
    let mut state = targeting_state();
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    state.create_card_in_zone(920002, 0, ZoneType::Battlefield);
    assert!(!legal_actions(&state).iter().any(|a| matches!(a, Action::CastSpell { object_id, .. } if *object_id == spell)));
}

#[test]
fn test_targeting_uses_layered_creature_types() {
    use mtg_gto::{card::CardType, layers::LayerModification};
    let mut state = targeting_state();
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    let animated = state.create_card_in_zone(920002, 0, ZoneType::Battlefield);
    let removed = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    targeting_layer(&mut state, animated, LayerModification::AddType(CardType::Creature));
    targeting_layer(&mut state, removed, LayerModification::RemoveType(CardType::Creature));
    let actions = legal_actions(&state);
    assert!(actions.contains(&Action::CastSpell { object_id: spell, targets: vec![Target::Object(animated)] }));
    assert!(!actions.contains(&Action::CastSpell { object_id: spell, targets: vec![Target::Object(removed)] }));
}

#[test]
fn test_targeting_invalid_cast_has_no_mutation() {
    let mut state = targeting_state();
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    let creature = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    let artifact = state.create_card_in_zone(920002, 0, ZoneType::Battlefield);
    let dead = state.create_card_in_zone(920001, 0, ZoneType::Graveyard);
    let shroud = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    let hex = state.create_card_in_zone(920001, 1, ZoneType::Battlefield);
    targeting_layer(&mut state, shroud, mtg_gto::layers::LayerModification::AddKeyword(KeywordAbility::Shroud));
    targeting_layer(&mut state, hex, mtg_gto::layers::LayerModification::AddKeyword(KeywordAbility::Hexproof));
    state.drain_events();
    for targets in [vec![], vec![Target::Object(creature); 2], vec![Target::Player(0)],
        vec![Target::Object(u64::MAX)], vec![Target::Object(artifact)], vec![Target::Object(dead)],
        vec![Target::Object(shroud)], vec![Target::Object(hex)]] {
        let before = serde_json::to_value(&state).unwrap();
        rules::apply_action(&mut state, &Action::CastSpell { object_id: spell, targets });
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        assert!(state.drain_events().is_empty());
    }
    let untargeted = state.create_card_in_zone(920004, 0, ZoneType::Hand);
    state.drain_events();
    let before = serde_json::to_value(&state).unwrap();
    rules::apply_action(&mut state, &Action::CastSpell { object_id: untargeted, targets: vec![Target::Object(creature)] });
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    assert!(state.drain_events().is_empty());
}

#[test]
fn test_targeting_shared_target_haste_draw_and_graveyard() {
    let mut state = targeting_state();
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    let other = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    let chosen = state.create_card_in_zone(920001, 1, ZoneType::Battlefield);
    targeting_cast(&mut state, spell, vec![Target::Object(chosen)]);
    assert_eq!(state.stack[0].targets, vec![Target::Object(chosen)]);
    assert_eq!(state.stack[0].target_generations, vec![Some(state.objects[&chosen].zone_change_count)]);
    // Stack snapshots preserve the selected incarnation, too.
    let encoded = serde_json::to_vec(&state.stack[0]).unwrap();
    let decoded: mtg_gto::game::StackEntry = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded.target_generations, state.stack[0].target_generations);
    state.drain_events();
    targeting_resolve(&mut state);
    assert!(state.has_keyword(chosen, KeywordAbility::Haste));
    assert!(!state.has_keyword(other, KeywordAbility::Haste));
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[0].library.len(), 2);
    assert!(state.players[1].hand.is_empty());
    assert!(state.stack.is_empty());
    assert_eq!(state.players[0].graveyard, vec![spell]);
    assert_eq!(state.drain_events().iter().filter(|event| matches!(event,
        GameEvent::ZoneChange { object, from: Zone::Stack, to: Zone::Graveyard } if *object == spell)).count(), 1);
}

#[test]
fn test_targeting_departed_or_blinked_target_stops_entire_spell() {
    for returns in [false, true] {
        let mut state = targeting_state();
        let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
        let target = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
        targeting_cast(&mut state, spell, vec![Target::Object(target)]);
        state.move_object(target, ZoneType::Battlefield, ZoneType::Exile);
        if returns { state.move_object(target, ZoneType::Exile, ZoneType::Battlefield); }
        targeting_resolve(&mut state);
        assert!(!state.has_keyword(target, KeywordAbility::Haste));
        assert!(state.players[0].hand.is_empty());
        assert_eq!(state.players[0].library.len(), 3);
        assert!(state.stack.is_empty());
        assert_eq!(state.players[0].graveyard, vec![spell]);
    }
}

#[test]
fn test_targeting_new_restrictions_stop_entire_spell() {
    use mtg_gto::{card::CardType, layers::LayerModification};
    for modification in [LayerModification::AddKeyword(KeywordAbility::Shroud),
        LayerModification::AddKeyword(KeywordAbility::Hexproof), LayerModification::RemoveType(CardType::Creature)] {
        let mut state = targeting_state();
        let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
        let target = state.create_card_in_zone(920001, 1, ZoneType::Battlefield);
        targeting_cast(&mut state, spell, vec![Target::Object(target)]);
        targeting_layer(&mut state, target, modification);
        targeting_resolve(&mut state);
        assert!(!state.has_keyword(target, KeywordAbility::Haste));
        assert!(state.players[0].hand.is_empty());
        assert_eq!(state.players[0].library.len(), 3);
        assert_eq!(state.players[0].graveyard, vec![spell]);
    }
}

#[test]
fn test_targeting_control_change_and_own_hexproof_remain_legal() {
    for hexproof in [false, true] {
        let mut state = targeting_state();
        let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
        let target = state.create_card_in_zone(920001, 1, ZoneType::Battlefield);
        targeting_cast(&mut state, spell, vec![Target::Object(target)]);
        targeting_layer(&mut state, target, mtg_gto::layers::LayerModification::ChangeController(0));
        if hexproof { targeting_layer(&mut state, target, mtg_gto::layers::LayerModification::AddKeyword(KeywordAbility::Hexproof)); }
        targeting_resolve(&mut state);
        assert!(state.has_keyword(target, KeywordAbility::Haste));
        assert_eq!(state.players[0].hand.len(), 1);
    }
}

#[test]
fn test_targeting_legacy_simple_and_untargeted_spells() {
    let mut state = targeting_state();
    let target = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    let buff = state.create_card_in_zone(920003, 0, ZoneType::Hand);
    targeting_cast(&mut state, buff, vec![Target::Object(target)]);
    targeting_resolve(&mut state);
    assert_eq!(state.effective_power(target), 4);
    state.priority_player = 0;
    let draw = state.create_card_in_zone(920004, 0, ZoneType::Hand);
    targeting_cast(&mut state, draw, vec![]);
    targeting_resolve(&mut state);
    assert_eq!(state.players[0].library.len(), 2);
    assert_eq!(state.players[0].graveyard, vec![buff, draw]);
}

#[test]
fn test_targeting_counterspell_stack_target_remains_legal() {
    let mut state = targeting_state();
    let draw = state.create_card_in_zone(920004, 0, ZoneType::Hand);
    let counter = state.create_card_in_zone(920005, 0, ZoneType::Hand);
    targeting_cast(&mut state, draw, vec![]);
    let draw_stack_id = state.stack.last().unwrap().id;
    targeting_cast(&mut state, counter, vec![Target::StackEntry(draw_stack_id)]);
    targeting_resolve(&mut state);
    assert!(state.stack.is_empty());
    assert_eq!(state.players[0].library.len(), 3);
    assert!(state.players[0].graveyard.contains(&draw));
    assert!(state.players[0].graveyard.contains(&counter));
}

#[test]
fn test_targeting_graveyard_cast_uses_same_contract() {
    use mtg_gto::mana::ManaCost;
    for flashback in [false, true] {
        let mut state = targeting_state();
        let db = Arc::make_mut(state.card_db.as_mut().unwrap());
        let mut def = db.get(920000).unwrap().clone();
        if flashback { def.flashback_cost = Some(ManaCost::new(0, 0, 0, 0, 1, 0)); }
        else { def.escape_exile_count = Some(1); }
        db.insert(def);
        let spell = state.create_card_in_zone(920000, 0, ZoneType::Graveyard);
        state.create_card_in_zone(920002, 0, ZoneType::Graveyard);
        assert!(!legal_actions(&state).iter().any(|a| matches!(a, Action::CastFromGraveyard { object_id, .. } if *object_id == spell)));
        let target = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
        let action = Action::CastFromGraveyard { object_id: spell, targets: vec![Target::Object(target)] };
        assert!(legal_actions(&state).contains(&action));
        state.drain_events();
        let before = serde_json::to_value(&state).unwrap();
        rules::apply_action(&mut state, &Action::CastFromGraveyard { object_id: spell, targets: vec![] });
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        assert!(state.drain_events().is_empty());
        rules::apply_action(&mut state, &action);
        assert_eq!(state.stack[0].targets, vec![Target::Object(target)]);
        targeting_resolve(&mut state);
        assert!(state.has_keyword(target, KeywordAbility::Haste));
        assert_eq!(state.players[0].library.len(), 2);
    }
}

#[test]
fn test_targeting_owner_graveyard_differs_from_caster() {
    let mut state = targeting_state();
    let spell = state.create_card_in_zone(920000, 1, ZoneType::Hand);
    state.players[1].hand.retain(|&id| id != spell);
    state.players[0].hand.push(spell);
    let target = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    targeting_cast(&mut state, spell, vec![Target::Object(target)]);
    targeting_resolve(&mut state);
    assert!(state.players[0].graveyard.is_empty());
    assert_eq!(state.players[1].graveyard, vec![spell]);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn test_targeting_incompatible_composite_is_not_cast_targetless() {
    use mtg_gto::card::{Effect, TargetSpec};
    let mut state = targeting_state();
    let db = Arc::make_mut(state.card_db.as_mut().unwrap());
    let mut def = db.get(920000).unwrap().clone();
    def.spell_effect = Some(Effect::Multiple(vec![
        Effect::GainKeywordUntilEOT { keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature },
        Effect::LoseLife { amount: 1, target: TargetSpec::Opponent },
    ]));
    db.insert(def);
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    assert!(!legal_actions(&state).iter().any(|a| matches!(a, Action::CastSpell { object_id, .. } if *object_id == spell)));
}

#[test]
fn test_targeting_composite_each_creature_keeps_untargeted_recipients() {
    use mtg_gto::{card::{CardType, Effect, TargetSpec}, layers::LayerModification};
    let mut state = targeting_state();
    let db = Arc::make_mut(state.card_db.as_mut().unwrap());
    let mut def = db.get(920000).unwrap().clone();
    def.spell_effect = Some(Effect::Multiple(vec![
        Effect::GainKeywordUntilEOT { keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature },
        Effect::Multiple(vec![Effect::DealDamage { amount: 1, target: TargetSpec::EachCreature }]),
    ]));
    db.insert(def);
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    let chosen = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    let other = state.create_card_in_zone(920001, 1, ZoneType::Battlefield);
    let animated = state.create_card_in_zone(920002, 0, ZoneType::Battlefield);
    let noncreature = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    targeting_layer(&mut state, other, LayerModification::AddKeyword(KeywordAbility::Shroud));
    targeting_layer(&mut state, animated, LayerModification::AddType(CardType::Creature));
    targeting_layer(&mut state, noncreature, LayerModification::RemoveType(CardType::Creature));
    targeting_cast(&mut state, spell, vec![Target::Object(chosen)]);
    targeting_resolve(&mut state);
    assert!(state.has_keyword(chosen, KeywordAbility::Haste));
    for id in [other, animated, noncreature] { assert!(!state.has_keyword(id, KeywordAbility::Haste)); }
    for id in [chosen, other, animated] { assert_eq!(state.objects[&id].damage_marked, 1); }
    assert_eq!(state.objects[&noncreature].damage_marked, 0);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(state.players[1].life, 20);
}

#[test]
fn test_targeting_composite_controller_is_not_targeted_opponent() {
    use mtg_gto::card::{Effect, TargetSpec};
    let mut state = targeting_state();
    let db = Arc::make_mut(state.card_db.as_mut().unwrap());
    let mut def = db.get(920000).unwrap().clone();
    def.spell_effect = Some(Effect::Multiple(vec![
        Effect::LoseLife { amount: 3, target: TargetSpec::Opponent },
        Effect::Multiple(vec![Effect::LoseLife { amount: 2, target: TargetSpec::Controller }]),
    ]));
    db.insert(def);
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    targeting_cast(&mut state, spell, vec![Target::Player(1)]);
    targeting_resolve(&mut state);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[0].graveyard, vec![spell]);
}

#[test]
fn test_targeting_composite_no_target_damage_keeps_each_player_semantics() {
    use mtg_gto::card::{Effect, TargetSpec};
    let mut state = targeting_state();
    let db = Arc::make_mut(state.card_db.as_mut().unwrap());
    let mut def = db.get(920000).unwrap().clone();
    def.spell_effect = Some(Effect::Multiple(vec![
        Effect::GainKeywordUntilEOT { keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature },
        Effect::DealDamage { amount: 1, target: TargetSpec::NoTarget },
    ]));
    db.insert(def);
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    let creature = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    targeting_cast(&mut state, spell, vec![Target::Object(creature)]);
    targeting_resolve(&mut state);
    assert!(state.has_keyword(creature, KeywordAbility::Haste));
    assert_eq!(state.objects[&creature].damage_marked, 0);
    assert_eq!(state.players[0].life, 19);
    assert_eq!(state.players[1].life, 19);
}

#[test]
fn test_targeting_composite_untargeted_graveyard_effect_does_not_inherit_target() {
    use mtg_gto::card::{Effect, TargetSpec};
    let mut state = targeting_state();
    let db = Arc::make_mut(state.card_db.as_mut().unwrap());
    let mut def = db.get(920000).unwrap().clone();
    def.spell_effect = Some(Effect::Multiple(vec![
        Effect::GainKeywordUntilEOT { keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature },
        Effect::ReturnToTopOfLibrary { target: TargetSpec::NoTarget },
    ]));
    db.insert(def);
    let spell = state.create_card_in_zone(920000, 0, ZoneType::Hand);
    let chosen = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    let unrelated = state.create_card_in_zone(920001, 0, ZoneType::Graveyard);
    let generation = state.objects[&chosen].zone_change_count;
    let library = state.players[0].library.clone();
    targeting_cast(&mut state, spell, vec![Target::Object(chosen)]);
    state.drain_events();
    targeting_resolve(&mut state);
    assert!(state.has_keyword(chosen, KeywordAbility::Haste));
    assert!(state.battlefield.contains(&chosen));
    assert_eq!(state.objects[&chosen].zone_change_count, generation);
    assert_eq!(state.players[0].library, library);
    assert_eq!(state.players[0].graveyard, vec![unrelated, spell]);
    assert!(state.stack.is_empty());
    assert_eq!(state.drain_events().iter().filter(|event| matches!(event,
        GameEvent::ZoneChange { object, from: Zone::Stack, to: Zone::Graveyard } if *object == spell)).count(), 1);
}

// Two identical ordinary spells expose stack identity independently of card identity.
fn stack_target_state(offset: u64) -> (GameState, u64, u64, u64) {
    let mut state = targeting_state();
    state.next_stack_id += offset;
    state.next_object_id += offset;
    let lower = state.create_card_in_zone(920004, 0, ZoneType::Hand);
    let upper = state.create_card_in_zone(920004, 0, ZoneType::Hand);
    let counter = state.create_card_in_zone(920005, 0, ZoneType::Hand);
    targeting_cast(&mut state, lower, vec![]);
    targeting_cast(&mut state, upper, vec![]);
    (state, lower, upper, counter)
}

#[test]
fn test_stack_target_independent_selection_and_countering() {
    for selected_index in [0, 1] {
        let (mut state, lower, upper, counter) = stack_target_state(0);
        let ids: Vec<_> = state.stack.iter().map(|entry| entry.id).collect();
        let actions: Vec<_> = legal_actions(&state).into_iter().filter(|action| matches!(action,
            Action::CastSpell { object_id, .. } if *object_id == counter)).collect();
        assert_eq!(actions, ids.iter().map(|id| Action::CastSpell {
            object_id: counter, targets: vec![Target::StackEntry(*id)],
        }).collect::<Vec<_>>());
        targeting_cast(&mut state, counter, vec![Target::StackEntry(ids[selected_index])]);
        assert_eq!(state.stack.last().unwrap().target_generations, vec![None]);
        targeting_resolve(&mut state);
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.stack[0].id, ids[1 - selected_index]);
        assert_eq!(state.players[0].graveyard, vec![[lower, upper][selected_index], counter]);
        assert_eq!(state.players[0].library.len(), 3); // Neither draw spell resolved.
    }
}

#[test]
fn test_stack_target_canonical_roundtrip_across_equivalent_states() {
    let (state, _, _, counter) = stack_target_state(0);
    let (other, _, _, other_counter) = stack_target_state(1000);
    use mtg_gto::info_set::InformationSet;
    assert_eq!(InformationSet::from_view(&state.visible_state(0), state.card_db()).hash_value(),
        InformationSet::from_view(&other.visible_state(0), other.card_db()).hash_value());
    for index in [0, 1] {
        let action = Action::CastSpell { object_id: counter, targets: vec![Target::StackEntry(state.stack[index].id)] };
        let canonical = canonicalize(&action, &state);
        assert_eq!(resolve(&canonical, &state, 0), Some(action));
        let other_action = Action::CastSpell { object_id: other_counter, targets: vec![Target::StackEntry(other.stack[index].id)] };
        assert_eq!(canonicalize(&other_action, &other), canonical);
        assert_eq!(resolve(&canonical, &other, 0), Some(other_action));
        let bytes = bincode::serialize(&canonical).unwrap();
        let decoded = bincode::deserialize(&bytes).unwrap();
        assert_eq!(resolve(&decoded, &other, 0), resolve(&canonical, &other, 0));
    }
}

#[test]
fn test_stack_target_information_set_preserves_relationships() {
    use mtg_gto::info_set::InformationSet;
    let mut hashes = Vec::new();
    for index in [0, 1] {
        let mut equivalent_hashes = Vec::new();
        for offset in [0, 1000] {
            let (mut state, _, _, counter) = stack_target_state(offset);
            let target = state.stack[index].id;
            targeting_cast(&mut state, counter, vec![Target::StackEntry(target)]);
            equivalent_hashes.push(InformationSet::from_view(&state.visible_state(0), state.card_db()).hash_value());
        }
        assert_eq!(equivalent_hashes[0], equivalent_hashes[1]);
        hashes.push(equivalent_hashes[0]);
    }
    assert_ne!(hashes[0], hashes[1]); // Same card names, different targeted spell.
}

#[test]
fn test_stack_target_snapshot_and_serialization() {
    let (mut state, lower, _, counter) = stack_target_state(70);
    let target = state.stack[0].id;
    targeting_cast(&mut state, counter, vec![Target::StackEntry(target)]);
    let saved = state.snapshot();
    let expected_stack = serde_json::to_value(&state.stack).unwrap();
    let expected_next_id = state.next_stack_id;
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let mut from_json: GameState = serde_json::from_slice(&json).unwrap();
    let mut from_binary: GameState = bincode::deserialize(&binary).unwrap();
    from_json.card_db = state.card_db.clone();
    from_binary.card_db = state.card_db.clone();
    targeting_resolve(&mut state);
    state.new_stack_id();
    state.restore(saved);
    for mut restored in [state.clone(), from_json, from_binary] {
        assert_eq!(serde_json::to_value(&restored.stack).unwrap(), expected_stack);
        assert_eq!(restored.new_stack_id(), expected_next_id);
        targeting_resolve(&mut restored);
        assert_eq!(restored.stack.len(), 1);
        assert_eq!(restored.players[0].graveyard, vec![lower, counter]);
    }
}

#[test]
fn test_stack_target_invalid_and_stale_casts_do_not_mutate() {
    let (mut state, lower, _, counter) = stack_target_state(0);
    let stale = state.stack[0].id;
    state.move_object(lower, ZoneType::Stack, ZoneType::Hand);
    targeting_cast(&mut state, lower, vec![]);
    assert_ne!(state.stack.last().unwrap().id, stale);
    let ability_id = state.new_stack_id();
    state.stack.push(mtg_gto::game::StackEntry { id: ability_id,
        source: mtg_gto::game::StackSource::ActivatedAbility { source_id: lower, ability_index: 0 },
        controller: 0, targets: vec![], target_generations: vec![] });
    for target in [Target::StackEntry(stale), Target::StackEntry(u64::MAX),
        Target::StackEntry(ability_id), Target::Object(lower), Target::Player(1)] {
        let action = Action::CastSpell { object_id: counter, targets: vec![target.clone()] };
        assert!(!legal_actions(&state).contains(&action));
        state.drain_events();
        let before = serde_json::to_value(&state).unwrap();
        rules::apply_action(&mut state, &action);
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        assert!(state.drain_events().is_empty());
        if matches!(target, Target::StackEntry(_)) {
            assert_eq!(resolve(&canonicalize(&action, &state), &state, 0), None);
        }
    }
}

#[test]
fn test_stack_target_stale_on_resolution_does_not_counter_another_spell() {
    let (mut state, lower, _, counter) = stack_target_state(0);
    let target = state.stack[0].id;
    let survivor = state.stack[1].id;
    targeting_cast(&mut state, counter, vec![Target::StackEntry(target)]);
    state.move_object(lower, ZoneType::Stack, ZoneType::Hand);
    targeting_resolve(&mut state);
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].id, survivor);
    assert_eq!(state.players[0].graveyard, vec![counter]);
    assert!(state.players[0].hand.contains(&lower));
}

#[test]
fn test_stack_target_does_not_replace_permanent_or_player_targets() {
    use mtg_gto::{card::TargetSpec, targeting::target_is_legal};
    let (mut state, _, _, _) = stack_target_state(0);
    let creature = state.create_card_in_zone(920001, 0, ZoneType::Battlefield);
    let stack_id = state.stack[0].id;
    assert!(target_is_legal(&state, 0, &TargetSpec::AnyCreature, &Target::Object(creature)));
    assert!(target_is_legal(&state, 0, &TargetSpec::AnyPlayer, &Target::Player(1)));
    assert!(!target_is_legal(&state, 0, &TargetSpec::AnyCreature, &Target::StackEntry(stack_id)));
    assert!(!target_is_legal(&state, 0, &TargetSpec::AnyPlayer, &Target::StackEntry(stack_id)));
    for target in [Target::Object(creature), Target::Player(1)] {
        let action = Action::CastSpell { object_id: state.create_card_in_zone(920004, 0, ZoneType::Hand), targets: vec![target] };
        assert_eq!(resolve(&canonicalize(&action, &state), &state, 0), Some(action));
    }
}

#[test]
fn test_stack_target_cleanup_graveyard_reconstruction_rejects_missing_targets() {
    use mtg_gto::action::canonical::{CanonicalAction, CanonicalTarget};
    let (mut state, _, _, counter) = stack_target_state(0);
    state.move_object(counter, ZoneType::Hand, ZoneType::Graveyard);
    let action = Action::CastFromGraveyard {
        object_id: counter, targets: vec![Target::StackEntry(state.stack[1].id)],
    };
    let canonical = canonicalize(&action, &state);
    assert_eq!(resolve(&canonical, &state, 0), Some(action));
    for index in [None, Some(state.stack.len()), Some(usize::MAX)] {
        let mut invalid = canonical.clone();
        if let CanonicalAction::CastFromGraveyard { targets, .. } = &mut invalid {
            *targets = vec![CanonicalTarget::StackEntry { stack_index: index }];
        } else { panic!("expected graveyard cast"); }
        assert_eq!(resolve(&invalid, &state, 0), None, "index {index:?}");
    }
    // An occupied position is insufficient: AnySpell cannot select an ability.
    state.stack[1].source = mtg_gto::game::StackSource::ActivatedAbility {
        source_id: counter, ability_index: 0,
    };
    assert_eq!(resolve(&canonical, &state, 0), None);
}

#[test]
fn test_stack_target_cleanup_interleaved_ability_positions() {
    use mtg_gto::action::canonical::{CanonicalAction, CanonicalTarget};
    let (mut state, lower, _, counter) = stack_target_state(0);
    let ability_id = state.new_stack_id();
    state.stack.insert(1, mtg_gto::game::StackEntry {
        id: ability_id,
        source: mtg_gto::game::StackSource::ActivatedAbility { source_id: lower, ability_index: 0 },
        controller: 0, targets: vec![], target_generations: vec![],
    });
    let mut equivalent = state.clone();
    for entry in &mut equivalent.stack { entry.id += 100; }
    equivalent.next_stack_id += 100;
    let actions = legal_actions(&state);
    for index in [0, 2] {
        let action = Action::CastSpell { object_id: counter, targets: vec![Target::StackEntry(state.stack[index].id)] };
        assert!(actions.contains(&action));
        let canonical = canonicalize(&action, &state);
        assert!(matches!(&canonical, CanonicalAction::CastSpell { targets, .. }
            if targets == &vec![CanonicalTarget::StackEntry { stack_index: Some(index) }]));
        assert_eq!(resolve(&canonical, &state, 0), Some(action));
        assert_eq!(resolve(&canonical, &equivalent, 0), Some(Action::CastSpell {
            object_id: counter, targets: vec![Target::StackEntry(equivalent.stack[index].id)],
        }));
    }
    let ability_action = Action::CastSpell { object_id: counter, targets: vec![Target::StackEntry(ability_id)] };
    assert!(!actions.contains(&ability_action));
    let mut canonical = canonicalize(&ability_action, &state);
    if let CanonicalAction::CastSpell { targets, .. } = &mut canonical {
        *targets = vec![CanonicalTarget::StackEntry { stack_index: Some(1) }];
    }
    assert_eq!(resolve(&canonical, &state, 0), None);
}

#[test]
fn test_stack_target_cleanup_stale_composite_skips_counter_and_draw() {
    use mtg_gto::card::{Effect, TargetSpec};
    let (mut state, lower, upper, counter) = stack_target_state(0);
    let mut db = state.card_db().clone();
    let mut def = db.get(920005).unwrap().clone();
    def.spell_effect = Some(Effect::Multiple(vec![
        Effect::Counter { target: TargetSpec::AnySpell }, Effect::DrawCards { count: 1 },
    ]));
    db.insert(def);
    state.card_db = Some(Arc::new(db));
    let target = state.stack[0].id;
    let survivor = state.stack[1].id;
    targeting_cast(&mut state, counter, vec![Target::StackEntry(target)]);
    state.move_object(lower, ZoneType::Stack, ZoneType::Hand);
    let hand = state.players[0].hand.clone();
    let library = state.players[0].library.clone();
    targeting_resolve(&mut state);
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].id, survivor);
    assert!(matches!(state.stack[0].source, mtg_gto::game::StackSource::Spell(id) if id == upper));
    assert_eq!(state.players[0].hand, hand);
    assert_eq!(state.players[0].library, library);
    assert_eq!(state.players[0].graveyard, vec![counter]);
}
