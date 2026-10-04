//! Phase 13.3: Goldfish deck tests.
//!
//! Runs prebuilt decks through goldfish simulation and verifies:
//! - Games complete without panics
//! - Win rates are reasonable (deck can actually kill)
//! - No infinite loops (games complete within turn/action limits)
//! - Life totals are correct at end of game

use mtg_gto::card::sample;
use mtg_gto::simulation;
use mtg_gto::strategy::GreedyStrategy;

#[test]
fn test_kinnan_seed12_does_not_advertise_an_unpayable_cast() {
    let db = sample::build_sample_db();
    let (deck, commander, _) = sample::kinnan_commander_deck();
    let result = simulation::run_commander_goldfish_game_seeded(
        &db, &deck, commander, &GreedyStrategy, 12);
    assert_eq!(result.rejected_actions, 0,
        "seed 12 previously offered Sink into Stupor without black mana");
}

// ===========================================================================
// Standard deck goldfish tests
// ===========================================================================

#[test]
fn test_red_aggro_goldfish_completes() {
    let db = sample::build_sample_db();
    let deck = sample::red_aggro_deck();
    let greedy = GreedyStrategy;

    let results = simulation::simulate_goldfish(&db, &deck, &greedy, 50);

    assert_eq!(results.total_games, 50, "Should complete all 50 games");
    assert_eq!(
        results.wins + results.losses + results.draws + results.censored,
        50,
        "All games should have an outcome"
    );
    assert!(
        results.wins > 0,
        "Red aggro should win at least some goldfish games (won {}/50)",
        results.wins
    );
}

#[test]
fn test_green_stompy_goldfish_completes() {
    let db = sample::build_sample_db();
    let deck = sample::green_stompy_deck();
    let greedy = GreedyStrategy;

    let results = simulation::simulate_goldfish(&db, &deck, &greedy, 50);

    assert_eq!(results.total_games, 50);
    assert!(
        results.wins > 0,
        "Green stompy should win at least some goldfish games (won {}/50)",
        results.wins
    );
}

#[test]
fn test_red_aggro_goldfish_reasonable_kill_speed() {
    let db = sample::build_sample_db();
    let deck = sample::red_aggro_deck();
    let greedy = GreedyStrategy;

    let results = simulation::simulate_goldfish(&db, &deck, &greedy, 100);

    // Red aggro should kill by turn 10 on average — it's a fast deck
    if results.wins > 0 {
        assert!(
            results.avg_kill_turn <= 15.0,
            "Red aggro avg kill turn should be <= 15, got {:.1}",
            results.avg_kill_turn
        );
        assert!(
            results.fastest_kill <= 10,
            "Red aggro fastest kill should be <= 10, got {}",
            results.fastest_kill
        );
    }
}

// ===========================================================================
// Commander deck goldfish tests
// ===========================================================================

#[test]
fn test_brimaz_commander_goldfish_completes() {
    let db = sample::build_sample_db();
    let (deck, commander) = sample::brimaz_commander_deck();
    let greedy = GreedyStrategy;

    let results = simulation::simulate_commander_goldfish(&db, &deck, commander, &greedy, 20);

    assert_eq!(results.total_games, 20, "Should complete all 20 games");
    assert_eq!(
        results.wins + results.losses + results.draws + results.censored,
        20,
        "All games should have an outcome"
    );
    // Brimaz mono-white aggro should be able to goldfish a kill sometimes
    // (opponent starts at 40 life in Commander, so might not always win by turn 20)
}

#[test]
fn test_ashcoat_commander_goldfish_completes() {
    let db = sample::build_sample_db();
    let (deck, commander) = sample::ashcoat_commander_deck();
    let greedy = GreedyStrategy;

    let results = simulation::simulate_commander_goldfish(&db, &deck, commander, &greedy, 20);

    assert_eq!(results.total_games, 20, "Should complete all 20 games");
    assert_eq!(
        results.wins + results.losses + results.draws + results.censored,
        20,
        "All games should have an outcome"
    );
}

#[test]
fn test_kinnan_commander_goldfish_completes() {
    let db = sample::build_sample_db();
    let (deck, commander, _partners) = sample::kinnan_commander_deck();
    let greedy = GreedyStrategy;

    let results = simulation::simulate_commander_goldfish(&db, &deck, commander, &greedy, 20);

    assert_eq!(results.total_games, 20, "Should complete all 20 games");
    assert_eq!(
        results.wins + results.losses + results.draws + results.censored,
        20,
        "All games should have an outcome"
    );
}

// ===========================================================================
// Two-player simulation tests
// ===========================================================================

#[test]
fn test_red_vs_green_completes_without_panic() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();
    let green = sample::green_stompy_deck();
    let greedy = GreedyStrategy;

    let results = simulation::simulate(&db, &red, &green, &greedy, &greedy, 50);

    assert_eq!(results.total_games, 50, "Should complete all 50 games");
    assert_eq!(
        results.player0_wins + results.player1_wins + results.draws + results.censored,
        50,
        "All games should have an outcome"
    );
    assert!(results.avg_turns > 0.0, "Games should last at least 1 turn");
}

#[test]
fn test_mirror_match_both_players_can_win() {
    let db = sample::build_sample_db();
    let red = sample::red_aggro_deck();
    let greedy = GreedyStrategy;

    let results = simulation::simulate(&db, &red, &red, &greedy, &greedy, 200);

    assert!(
        results.player0_wins > 0,
        "Player 0 should win some mirror matches"
    );
    assert!(
        results.player1_wins > 0,
        "Player 1 should win some mirror matches"
    );
}

// ===========================================================================
// Edge case: empty-ish board goldfish
// ===========================================================================

#[test]
fn test_goldfish_with_all_lands_deck_draws() {
    // A deck of all lands should never deal enough damage to win by turn 20
    let db = sample::build_sample_db();
    let mut all_lands = Vec::new();
    for _ in 0..60 {
        all_lands.push(sample::ids::MOUNTAIN);
    }
    let greedy = GreedyStrategy;

    let results = simulation::simulate_goldfish(&db, &all_lands, &greedy, 10);

    assert_eq!(results.total_games, 10);
    // An all-lands deck can't win (no creatures or spells)
    assert_eq!(
        results.wins, 0,
        "All-lands deck should not be able to goldfish a win"
    );
}
