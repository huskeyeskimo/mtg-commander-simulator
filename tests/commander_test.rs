//! Tests for 1v1 Commander format support.
//!
//! Covers: command zone, commander tax, commander damage (21), commander
//! redirect (graveyard/exile -> command zone), 100-card singleton deck
//! validation, and full commander game simulation.

use std::sync::Arc;

use mtg_gto::action::{legal_actions, Action};
use mtg_gto::card::sample::{self, ids};
use mtg_gto::card::{CardId, ZoneType};
use mtg_gto::game::{GameFormat, GameState, Phase};
use mtg_gto::rules;
use mtg_gto::simulation;
use mtg_gto::strategy::{GreedyStrategy, RandomStrategy};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a commander game state with the given commanders, without full
/// deck setup (for unit-level testing of individual mechanics).
fn setup_commander_state(commander0: CardId, commander1: CardId) -> GameState {
    let db = sample::build_sample_db();
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));

    // Designate commanders
    state.players[0].commander_card_id = Some(commander0);
    state.players[1].commander_card_id = Some(commander1);

    // Put commanders in command zone and record object IDs
    let cmd0_obj = state.create_card_in_zone(commander0, 0, ZoneType::Command);
    let cmd1_obj = state.create_card_in_zone(commander1, 1, ZoneType::Command);
    state.players[0].commander_object_id = Some(cmd0_obj);
    state.players[1].commander_object_id = Some(cmd1_obj);

    // Libraries so no one decks out
    for _ in 0..30 {
        state.create_card_in_zone(ids::PLAINS, 0, ZoneType::Library);
        state.create_card_in_zone(ids::FOREST, 1, ZoneType::Library);
    }

    state
}

// ---------------------------------------------------------------------------
// Game format and state
// ---------------------------------------------------------------------------

#[test]
fn test_commander_format_set_correctly() {
    let state = GameState::new_commander(2);
    assert_eq!(state.format, GameFormat::Commander);
    assert!(state.is_commander_format());
}

#[test]
fn test_commander_starting_life_is_40() {
    let state = GameState::new_commander(2);
    assert_eq!(state.players[0].life, 40);
    assert_eq!(state.players[1].life, 40);
}

#[test]
fn test_standard_starting_life_is_20() {
    let state = GameState::new(2);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(state.players[1].life, 20);
    assert_eq!(state.format, GameFormat::Standard);
}

#[test]
fn test_commander_damage_tracking_initialized() {
    let state = GameState::new_commander(2);
    assert_eq!(state.players[0].commander_damage_received.len(), 2);
    assert_eq!(state.players[1].commander_damage_received.len(), 2);
    assert_eq!(state.players[0].commander_damage_received[0], 0);
    assert_eq!(state.players[0].commander_damage_received[1], 0);
}

// ---------------------------------------------------------------------------
// Command zone
// ---------------------------------------------------------------------------

#[test]
fn test_commander_starts_in_command_zone() {
    let state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    assert_eq!(state.players[0].command_zone.len(), 1);
    assert_eq!(state.players[1].command_zone.len(), 1);

    let cmd0_obj = state.players[0].command_zone[0];
    let cmd1_obj = state.players[1].command_zone[0];

    assert_eq!(state.objects[&cmd0_obj].card_def_id, ids::BRIMAZ_KING);
    assert_eq!(state.objects[&cmd1_obj].card_def_id, ids::THRUN_LAST_TROLL);
}

#[test]
fn test_is_commander_check() {
    let state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    let cmd0_obj = state.players[0].command_zone[0];
    let cmd1_obj = state.players[1].command_zone[0];

    assert!(state.is_commander(cmd0_obj));
    assert!(state.is_commander(cmd1_obj));

    // A non-commander object should not be a commander
    let plains_id = state.players[0].library[0];
    assert!(!state.is_commander(plains_id));
}

// ---------------------------------------------------------------------------
// Casting from command zone
// ---------------------------------------------------------------------------

#[test]
fn test_cast_commander_action_available() {
    let mut state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    // Give player 0 enough mana to cast Brimaz (1WW = 3 mana)
    for _ in 0..3 {
        state.create_card_in_zone(ids::PLAINS, 0, ZoneType::Battlefield);
    }
    // Untap the lands
    for &id in &state.battlefield.clone() {
        if let Some(inst) = state.objects.get_mut(&id) {
            inst.tapped = false;
            inst.summoning_sick = false;
        }
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;

    let actions = legal_actions(&state);
    let cast_commander = actions
        .iter()
        .find(|a| matches!(a, Action::CastCommander { .. }));
    assert!(
        cast_commander.is_some(),
        "Should have CastCommander action when commander is in command zone and mana available"
    );
}

#[test]
fn test_cast_commander_not_available_without_mana() {
    let mut state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    // No lands on battlefield — can't pay for Brimaz
    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;

    let actions = legal_actions(&state);
    let cast_commander = actions
        .iter()
        .find(|a| matches!(a, Action::CastCommander { .. }));
    assert!(
        cast_commander.is_none(),
        "Should not have CastCommander action without enough mana"
    );
}

#[test]
fn test_cast_commander_resolves_to_battlefield() {
    let mut state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    // Give player 0 lands
    for _ in 0..4 {
        let land = state.create_card_in_zone(ids::PLAINS, 0, ZoneType::Battlefield);
        if let Some(inst) = state.objects.get_mut(&land) {
            inst.tapped = false;
            inst.summoning_sick = false;
        }
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;

    let cmd_obj = state.players[0].command_zone[0];
    let action = Action::CastCommander {
        object_id: cmd_obj,
        targets: vec![],
    };

    rules::apply_action(&mut state, &action);

    // Commander should be on the stack now
    assert!(state.players[0].command_zone.is_empty());
    assert!(!state.stack.is_empty());

    // Both players pass -> resolves
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);

    // Brimaz should be on the battlefield now
    let on_battlefield = state
        .battlefield
        .iter()
        .any(|&id| state.objects[&id].card_def_id == ids::BRIMAZ_KING);
    assert!(
        on_battlefield,
        "Commander should be on the battlefield after resolution"
    );
}

// ---------------------------------------------------------------------------
// Commander tax
// ---------------------------------------------------------------------------

#[test]
fn test_commander_tax_increments() {
    let mut state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    // Give player 0 plenty of mana
    for _ in 0..10 {
        let land = state.create_card_in_zone(ids::PLAINS, 0, ZoneType::Battlefield);
        if let Some(inst) = state.objects.get_mut(&land) {
            inst.tapped = false;
        }
    }

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;

    assert_eq!(state.players[0].commander_tax, 0);

    // Cast commander first time
    let cmd_obj = state.players[0].command_zone[0];
    let action = Action::CastCommander {
        object_id: cmd_obj,
        targets: vec![],
    };
    rules::apply_action(&mut state, &action);

    assert_eq!(
        state.players[0].commander_tax, 1,
        "Tax should increment after first cast"
    );
}

// ---------------------------------------------------------------------------
// Commander redirect (death -> command zone)
// ---------------------------------------------------------------------------

#[test]
fn test_commander_redirects_to_command_zone_on_death() {
    let mut state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    // Move the actual commander object from command zone to battlefield
    // (simulating a resolved CastCommander)
    let brimaz_obj = state.players[0].commander_object_id.unwrap();
    state.move_object(brimaz_obj, ZoneType::Command, ZoneType::Battlefield);
    assert!(state.battlefield.contains(&brimaz_obj));

    // Move Brimaz to graveyard (simulating destruction)
    state.move_object(brimaz_obj, ZoneType::Battlefield, ZoneType::Graveyard);

    // Commander should be in command zone, NOT graveyard
    assert!(
        state.players[0].command_zone.contains(&brimaz_obj),
        "Commander should be redirected to command zone on death"
    );
    assert!(
        !state.players[0].graveyard.contains(&brimaz_obj),
        "Commander should NOT be in graveyard"
    );
}

#[test]
fn test_commander_redirects_to_command_zone_on_exile() {
    let mut state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    // Move the actual commander object to battlefield first
    let brimaz_obj = state.players[0].commander_object_id.unwrap();
    state.move_object(brimaz_obj, ZoneType::Command, ZoneType::Battlefield);

    // Move Brimaz to exile (simulating Path to Exile)
    state.move_object(brimaz_obj, ZoneType::Battlefield, ZoneType::Exile);

    assert!(
        state.players[0].command_zone.contains(&brimaz_obj),
        "Commander should be redirected to command zone on exile"
    );
    assert!(
        !state.players[0].exile.contains(&brimaz_obj),
        "Commander should NOT be in exile"
    );
}

#[test]
fn test_non_commander_goes_to_graveyard_normally() {
    let mut state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    // A non-commander creature
    let bear_obj = state.create_card_in_zone(ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);

    state.move_object(bear_obj, ZoneType::Battlefield, ZoneType::Graveyard);

    assert!(
        state.players[0].graveyard.contains(&bear_obj),
        "Non-commander should go to graveyard normally"
    );
    assert!(
        !state.players[0].command_zone.contains(&bear_obj),
        "Non-commander should NOT go to command zone"
    );
}

// ---------------------------------------------------------------------------
// Commander damage
// ---------------------------------------------------------------------------

#[test]
fn test_commander_damage_21_loses_game() {
    let mut state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    // Set player 1 to have received 21 commander damage from player 0's commander
    state.players[1].commander_damage_received[0] = 21;

    rules::check_state_based_actions(&mut state);

    assert!(
        state.players[1].has_lost,
        "Player should lose at 21 commander damage"
    );
    assert!(state.game_over, "Game should be over");
}

#[test]
fn test_commander_damage_20_does_not_lose() {
    let mut state = setup_commander_state(ids::BRIMAZ_KING, ids::THRUN_LAST_TROLL);

    state.players[1].commander_damage_received[0] = 20;

    rules::check_state_based_actions(&mut state);

    assert!(
        !state.players[1].has_lost,
        "Player should NOT lose at 20 commander damage"
    );
}

// ---------------------------------------------------------------------------
// Deck validation
// ---------------------------------------------------------------------------

#[test]
fn test_validate_commander_deck_valid() {
    let db = sample::build_sample_db();
    let (deck, commander) = sample::brimaz_commander_deck();
    let result = rules::validate_commander_deck(&db, &deck, commander);
    assert!(result.is_ok(), "Brimaz deck should be valid: {:?}", result);
}

#[test]
fn test_validate_commander_deck_wrong_size() {
    let db = sample::build_sample_db();
    let deck: Vec<CardId> = vec![ids::PLAINS; 60]; // Too small
    let result = rules::validate_commander_deck(&db, &deck, ids::BRIMAZ_KING);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("100 cards"));
}

#[test]
fn test_validate_commander_deck_non_legendary() {
    let db = sample::build_sample_db();
    let mut deck: Vec<CardId> = vec![ids::PLAINS; 99];
    deck.push(ids::GRIZZLY_BEARS); // Not legendary
    let result = rules::validate_commander_deck(&db, &deck, ids::GRIZZLY_BEARS);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("legendary creature"));
}

#[test]
fn test_validate_commander_deck_singleton_violation() {
    let db = sample::build_sample_db();
    // 96 Plains + 1 Brimaz + 2 Savannah Lions + 1 Serra Angel = 100
    let mut deck: Vec<CardId> = vec![ids::PLAINS; 96];
    deck.push(ids::BRIMAZ_KING);
    deck.push(ids::SERRA_ANGEL);
    deck.push(ids::SAVANNAH_LIONS);
    deck.push(ids::SAVANNAH_LIONS); // duplicate non-basic!
    assert_eq!(deck.len(), 100);
    let result = rules::validate_commander_deck(&db, &deck, ids::BRIMAZ_KING);
    assert!(
        result.is_err(),
        "Should reject duplicate non-basic: {:?}",
        result
    );
    assert!(
        result.unwrap_err().contains("only 1 copy"),
        "Error should mention singleton violation"
    );
}

#[test]
fn test_validate_commander_deck_multiple_basics_allowed() {
    let db = sample::build_sample_db();
    let (deck, commander) = sample::brimaz_commander_deck();
    // This deck has many Plains (basic land) — should be fine
    let plains_count = deck.iter().filter(|&&id| id == ids::PLAINS).count();
    assert!(plains_count > 1, "Deck should have multiple basics");
    let result = rules::validate_commander_deck(&db, &deck, commander);
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// Full game setup
// ---------------------------------------------------------------------------

#[test]
fn test_setup_commander_game() {
    let db = sample::build_sample_db();
    let (deck0, cmd0) = sample::brimaz_commander_deck();
    let (deck1, cmd1) = sample::thrun_commander_deck();

    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));

    rules::setup_commander_game(&mut state, &deck0, &deck1, cmd0, cmd1);

    // Commanders in command zone
    assert_eq!(state.players[0].command_zone.len(), 1);
    assert_eq!(state.players[1].command_zone.len(), 1);

    let cmd0_obj = state.players[0].command_zone[0];
    let cmd1_obj = state.players[1].command_zone[0];
    assert_eq!(state.objects[&cmd0_obj].card_def_id, ids::BRIMAZ_KING);
    assert_eq!(state.objects[&cmd1_obj].card_def_id, ids::THRUN_LAST_TROLL);

    // Libraries should have 99 - 7 = 92 cards (100 - 1 commander - 7 hand)
    assert_eq!(state.players[0].library.len(), 92);
    assert_eq!(state.players[1].library.len(), 92);

    // Hands should have 7 cards
    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(state.players[1].hand.len(), 7);

    // Starting life is 40
    assert_eq!(state.players[0].life, 40);
    assert_eq!(state.players[1].life, 40);
}

// ---------------------------------------------------------------------------
// Full game simulation
// ---------------------------------------------------------------------------

#[test]
fn test_commander_game_completes() {
    let db = sample::build_sample_db();
    let (deck0, cmd0) = sample::brimaz_commander_deck();
    let (deck1, cmd1) = sample::thrun_commander_deck();

    let greedy = GreedyStrategy;
    let result = simulation::run_commander_game(&db, &deck0, &deck1, cmd0, cmd1, &greedy, &greedy);

    assert!(
        result.winner.is_some() || result.turns >= 200,
        "Commander game should complete (winner or draw)"
    );
}

#[test]
fn test_commander_simulation_produces_results() {
    let db = sample::build_sample_db();
    let (deck0, cmd0) = sample::brimaz_commander_deck();
    let (deck1, cmd1) = sample::thrun_commander_deck();

    let greedy = GreedyStrategy;
    let random = RandomStrategy;
    let results =
        simulation::simulate_commander(&db, &deck0, &deck1, cmd0, cmd1, &greedy, &random, 10);

    assert_eq!(results.total_games, 10);
    assert!(
        results.player0_wins + results.player1_wins + results.draws + results.censored == 10,
        "All games should be accounted for"
    );
}

#[test]
fn test_commander_greedy_vs_random() {
    let db = sample::build_sample_db();
    let (deck0, cmd0) = sample::brimaz_commander_deck();
    let (deck1, cmd1) = sample::thrun_commander_deck();

    let greedy = GreedyStrategy;
    let random = RandomStrategy;
    let results =
        simulation::simulate_commander(&db, &deck0, &deck1, cmd0, cmd1, &greedy, &random, 50);

    // Greedy should beat random most of the time
    assert!(
        results.player0_wins > results.player1_wins,
        "Greedy should beat Random in Commander: {} vs {}",
        results.player0_wins,
        results.player1_wins,
    );
}

// ---------------------------------------------------------------------------
// London Mulligan
// ---------------------------------------------------------------------------

#[test]
fn test_commander_starts_in_mulligan_phase() {
    let db = sample::build_sample_db();
    let (deck, cmd) = sample::brimaz_commander_deck();

    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_commander_game(&mut state, &deck, &deck, cmd, cmd);

    assert_eq!(state.phase, Phase::Mulligan);
    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(state.players[1].hand.len(), 7);
    assert!(!state.players[0].mulligan_decided);
    assert!(!state.players[1].mulligan_decided);
}

#[test]
fn test_mulligan_keep_advances_to_next_player() {
    let db = sample::build_sample_db();
    let (deck, cmd) = sample::brimaz_commander_deck();

    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_commander_game(&mut state, &deck, &deck, cmd, cmd);

    // P0 keeps
    assert_eq!(state.priority_player, 0);
    rules::apply_action(&mut state, &Action::MulliganKeep);

    // Should advance to P1 (still in Mulligan phase)
    assert_eq!(state.phase, Phase::Mulligan);
    assert_eq!(state.priority_player, 1);
    assert!(state.players[0].mulligan_decided);
    assert!(!state.players[1].mulligan_decided);
}

#[test]
fn test_mulligan_both_keep_transitions_to_untap() {
    let db = sample::build_sample_db();
    let (deck, cmd) = sample::brimaz_commander_deck();

    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_commander_game(&mut state, &deck, &deck, cmd, cmd);

    // Both players keep
    rules::apply_action(&mut state, &Action::MulliganKeep);
    rules::apply_action(&mut state, &Action::MulliganKeep);

    // Should have transitioned out of Mulligan phase
    assert_ne!(state.phase, Phase::Mulligan);
    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(state.players[1].hand.len(), 7);
}

#[test]
fn test_mulligan_once_then_keep_requires_bottom_one() {
    let db = sample::build_sample_db();
    let (deck, cmd) = sample::brimaz_commander_deck();

    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_commander_game(&mut state, &deck, &deck, cmd, cmd);

    // P0 mulligans once
    rules::apply_action(&mut state, &Action::MulliganMulligan);
    assert_eq!(state.players[0].mulligan_count, 1);
    assert_eq!(state.players[0].hand.len(), 7); // drew 7 new cards
    assert!(!state.players[0].mulligan_decided);

    // P0 keeps (now needs to bottom 1 card)
    rules::apply_action(&mut state, &Action::MulliganKeep);
    assert!(state.players[0].mulligan_decided);

    // Still in Mulligan phase — need to bottom a card
    assert_eq!(state.phase, Phase::Mulligan);
    assert_eq!(state.priority_player, 0);

    // Legal actions should be MulliganBottomCard for each card in hand
    let actions = legal_actions(&state);
    assert_eq!(actions.len(), 7);
    assert!(actions
        .iter()
        .all(|a| matches!(a, Action::MulliganBottomCard { .. })));

    // Bottom a card
    let bottom_action = actions[0].clone();
    rules::apply_action(&mut state, &bottom_action);

    // P0 now has 6 cards; priority passes to P1
    assert_eq!(state.players[0].hand.len(), 6);
    assert_eq!(state.priority_player, 1);

    // P1 keeps → game starts
    rules::apply_action(&mut state, &Action::MulliganKeep);
    assert_ne!(state.phase, Phase::Mulligan);
    assert_eq!(state.players[0].hand.len(), 6);
    assert_eq!(state.players[1].hand.len(), 7);
}

#[test]
fn test_mulligan_twice_bottoms_two() {
    let db = sample::build_sample_db();
    let (deck, cmd) = sample::brimaz_commander_deck();

    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_commander_game(&mut state, &deck, &deck, cmd, cmd);

    // P0 mulligans twice
    rules::apply_action(&mut state, &Action::MulliganMulligan);
    rules::apply_action(&mut state, &Action::MulliganMulligan);
    assert_eq!(state.players[0].mulligan_count, 2);

    // P0 keeps
    rules::apply_action(&mut state, &Action::MulliganKeep);

    // Bottom 2 cards
    let actions = legal_actions(&state);
    assert_eq!(actions.len(), 7); // 7 cards to choose from
    rules::apply_action(&mut state, &actions[0].clone());
    assert_eq!(state.players[0].hand.len(), 6);
    assert_eq!(state.priority_player, 0); // still P0's turn to bottom

    let actions = legal_actions(&state);
    assert_eq!(actions.len(), 6); // 6 cards to choose from
    rules::apply_action(&mut state, &actions[0].clone());
    assert_eq!(state.players[0].hand.len(), 5);

    // Now P1 decides
    assert_eq!(state.priority_player, 1);
    rules::apply_action(&mut state, &Action::MulliganKeep);

    // Game started, P0 has 5 cards
    assert_ne!(state.phase, Phase::Mulligan);
    assert_eq!(state.players[0].hand.len(), 5);
    assert_eq!(state.players[1].hand.len(), 7);
}

#[test]
fn test_mulligan_full_game_with_greedy() {
    // Verify a full commander game completes correctly when mulligans are active
    let db = sample::build_sample_db();
    let (deck0, cmd0) = sample::brimaz_commander_deck();
    let (deck1, cmd1) = sample::thrun_commander_deck();

    let greedy = GreedyStrategy;
    let result = simulation::run_commander_game(&db, &deck0, &deck1, cmd0, cmd1, &greedy, &greedy);

    assert!(
        result.winner.is_some() || result.turns >= 200,
        "Commander game with mulligans should complete"
    );
}
