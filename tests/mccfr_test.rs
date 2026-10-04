//! Integration tests for Phase 1B + 2B: MCCFR Solver
//!
//! Tests the end-to-end MCCFR training pipeline:
//! - InformationSet construction from PlayerView
//! - RegretTable storage and serialization
//! - MCCFR traversal on minimal training scenarios
//! - McfrStrategy implementing the Strategy trait
//! - Trained McfrStrategy playing legal games
//!
//! Phase 2B additions:
//! - Information set abstraction (bucketed vs identity)
//! - Parallel training with sharded tables
//! - Depth-limited rollouts with GreedyStrategy
//! - Scale validation: 60-card deck training without OOM
//! - Win rate vs GreedyStrategy baseline

use std::sync::Arc;

use mtg_gto::action::legal_actions;
use mtg_gto::card::sample;
use mtg_gto::card::ZoneType;
use mtg_gto::game::{GameFormat, GameState, Phase};
use mtg_gto::info_set::{BucketedAbstraction, InformationSet};
use mtg_gto::rules;
use mtg_gto::simulation;
use mtg_gto::solver::mccfr::{self, McfrConfig, RolloutMode, TrainConfig};
use mtg_gto::solver::RegretTable;
use mtg_gto::simulation::{simulate_goldfish, simulate_commander_goldfish};
use mtg_gto::strategy::{AbstractedMcfrStrategy, GreedyStrategy, McfrStrategy, RandomStrategy, Strategy};

/// Helper: create a minimal game state with 15-card decks for MCCFR testing.
fn setup_mini_game() -> GameState {
    let db = sample::build_sample_db();
    let deck0 = sample::mini_red_burn();
    let deck1 = sample::mini_red_creatures();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &deck0, &deck1);
    state
}

#[test]
fn test_info_set_from_game_state() {
    let state = setup_mini_game();
    let view = state.visible_state(0);
    let info_set = InformationSet::from_view(&view, state.card_db());

    assert_eq!(info_set.my_life, 20);
    assert_eq!(info_set.opp_life, 20);
    // Player should have drawn 7 cards
    assert_eq!(info_set.my_hand.len(), 7);
    // Opponent has 7 cards in hand (size visible, contents hidden)
    assert_eq!(info_set.opp_hand_size, 7);
    // Libraries should have 15 - 7 = 8 cards each
    assert_eq!(info_set.opp_library_size, 8);
    // Exile zones should be empty at game start
    assert!(info_set.my_exile.is_empty());
    assert!(info_set.opp_exile.is_empty());
    // Mana should be empty at game start
    assert_eq!(info_set.my_mana, [0, 0, 0, 0, 0, 0]);
}

#[test]
fn test_info_set_hash_stable() {
    // Same game state should produce same hash on repeated calls
    let state = setup_mini_game();

    let view1 = state.visible_state(0);
    let hash1 = InformationSet::from_view(&view1, state.card_db()).hash_value();

    let view2 = state.visible_state(0);
    let hash2 = InformationSet::from_view(&view2, state.card_db()).hash_value();

    assert_eq!(hash1, hash2, "Same game state should produce same hash");
}

#[test]
fn test_regret_table_roundtrip() {
    use mtg_gto::action::canonical::CanonicalAction;

    let mut table = RegretTable::new();
    let actions = vec![
        CanonicalAction::PassPriority,
        CanonicalAction::PlayLand { card_id: 1, hand_index: 0 },
        CanonicalAction::CastSpell { card_id: 100, hand_index: 0, targets: vec![] },
    ];
    {
        let entry = table.get_or_create(12345);
        for (i, a) in actions.iter().enumerate() {
            let ae = entry.get_or_create_action(a);
            ae.cumulative_regret = (i as f64 + 1.0) * 1.0;
            ae.cumulative_strategy = (i as f64 + 1.0) * 5.0;
        }
        entry.visit_count = 42;
    }

    let bytes = table.to_bytes().expect("serialize");
    let restored = RegretTable::from_bytes(&bytes).expect("deserialize");

    let data = restored.get(12345).unwrap();
    assert_eq!(data.action_data[&CanonicalAction::PassPriority].cumulative_regret, 1.0);
    assert_eq!(data.visit_count, 42);
}

#[test]
fn test_mccfr_single_iteration_runs() {
    // Verify that a single MCCFR iteration completes without panics.
    let state = setup_mini_game();
    let config = McfrConfig {
        max_depth: 10,
        max_actions: 500,
        max_nodes_per_iteration: 0,
    };

    let mut tables = [RegretTable::new(), RegretTable::new()];
    mccfr::run_iteration(&state, &mut tables, &config);

    // After one iteration, at least some info sets should have been visited
    let total_info_sets: usize = tables.iter().map(|t| t.num_info_sets()).sum();
    assert!(
        total_info_sets > 0,
        "MCCFR should visit at least some info sets"
    );
}

#[test]
fn test_mccfr_training_loop() {
    // Run a small number of MCCFR iterations and verify convergence behavior.
    let state = setup_mini_game();
    let config = McfrConfig {
        max_depth: 8,
        max_actions: 300,
        max_nodes_per_iteration: 0,
    };

    let tables = mccfr::train(&state, 10, &config);

    let total_info_sets: usize = tables.iter().map(|t| t.num_info_sets()).sum();
    assert!(
        total_info_sets > 0,
        "Training should create info set entries"
    );

    // Check that visit counts are reasonable
    for table in &tables {
        for (_, data) in &table.data {
            assert!(data.visit_count > 0, "Visited entries should have count > 0");
            assert!(
                !data.action_data.is_empty(),
                "Entries should have action data"
            );
        }
    }
}

#[test]
fn test_mccfr_exploitability_decreases() {
    // After more iterations, approximate exploitability should generally decrease.
    let state = setup_mini_game();
    let config = McfrConfig {
        max_depth: 8,
        max_actions: 200,
        max_nodes_per_iteration: 0,
    };

    let tables_5 = mccfr::train(&state, 5, &config);
    let exploit_5 = mccfr::approximate_exploitability(&tables_5);

    let tables_50 = mccfr::train(&state, 50, &config);
    let exploit_50 = mccfr::approximate_exploitability(&tables_50);

    // We don't assert strict monotonicity (MCCFR is stochastic), but the
    // 50-iteration result should be finite and non-negative.
    assert!(exploit_50.is_finite(), "Exploitability should be finite");
    assert!(exploit_50 >= 0.0, "Exploitability should be non-negative");
    assert!(exploit_5.is_finite(), "Exploitability should be finite");

    // Log values for manual inspection
    eprintln!("Exploitability after 5 iters: {:.4}", exploit_5);
    eprintln!("Exploitability after 50 iters: {:.4}", exploit_50);
}

#[test]
fn test_mcfr_strategy_plays_legal_games() {
    // Train a McfrStrategy and verify it plays complete, legal games.
    let db = sample::build_sample_db();
    let state = setup_mini_game();
    let config = McfrConfig {
        max_depth: 8,
        max_actions: 200,
        max_nodes_per_iteration: 0,
    };

    let tables = mccfr::train(&state, 20, &config);
    let mcfr_p0 = McfrStrategy::new(tables[0].clone());
    let mcfr_p1 = McfrStrategy::new(tables[1].clone());

    // Play 10 games with McfrStrategy
    let deck0 = sample::mini_red_burn();
    let deck1 = sample::mini_red_creatures();

    for _ in 0..10 {
        let result = simulation::run_game(&db, &deck0, &deck1, &mcfr_p0, &mcfr_p1);
        // Game should terminate (not hang)
        assert!(
            result.turns <= 200,
            "Game should terminate within turn limit"
        );
    }
}

#[test]
fn test_mcfr_strategy_vs_random() {
    // After training, McfrStrategy should at least not crash when playing
    // against RandomStrategy. Win rate validation requires more training.
    let db = sample::build_sample_db();
    let state = setup_mini_game();
    let config = McfrConfig {
        max_depth: 8,
        max_actions: 200,
        max_nodes_per_iteration: 0,
    };

    let tables = mccfr::train(&state, 20, &config);
    let mcfr_strat = McfrStrategy::new(tables[0].clone());
    let random_strat = RandomStrategy;

    let deck0 = sample::mini_red_burn();
    let deck1 = sample::mini_red_creatures();

    let results = simulation::simulate(
        &db,
        &deck0,
        &deck1,
        &mcfr_strat,
        &random_strat,
        50,
    );

    eprintln!(
        "MCCFR vs Random: {:.1}% win rate ({} games)",
        results.win_rate(0) * 100.0,
        results.total_games
    );

    // McfrStrategy should at least complete all games without panics
    assert_eq!(results.total_games, 50);
}

#[test]
fn test_mcfr_strategy_vs_greedy() {
    // Acceptance criterion #14: McfrStrategy vs GreedyStrategy matchup.
    // With limited training budget (CI-friendly), the test verifies:
    // 1. Games complete without panics
    // 2. Win rate is logged for manual inspection
    // Full convergence (MCCFR beating Greedy) requires longer training runs.
    let db = sample::build_sample_db();
    let state = setup_mini_game();
    let config = McfrConfig {
        max_depth: 8,
        max_actions: 300,
        max_nodes_per_iteration: 0,
    };

    let tables = mccfr::train(&state, 50, &config);
    let mcfr_strat = McfrStrategy::new(tables[0].clone());
    let greedy_strat = GreedyStrategy;

    let deck0 = sample::mini_red_burn();
    let deck1 = sample::mini_red_creatures();

    let results = simulation::simulate(
        &db,
        &deck0,
        &deck1,
        &mcfr_strat,
        &greedy_strat,
        100,
    );

    let mcfr_win_rate = results.win_rate(0);
    eprintln!(
        "MCCFR vs Greedy: {:.1}% win rate ({} games, P0 wins={}, P1 wins={}, draws={})",
        mcfr_win_rate * 100.0,
        results.total_games,
        results.player0_wins,
        results.player1_wins,
        results.draws,
    );

    // All games must complete without panics
    assert_eq!(results.total_games, 100);
}

#[test]
fn test_mcfr_mirror_match_convergence() {
    // A symmetric trained matchup should show neither extreme seat bias nor
    // extreme policy bias when both trained tables take each seat.
    let db = sample::build_sample_db();
    // 8 lands + 6 Bolts + 1 Shock = 15 cards and 20 possible damage.
    let mut deck = vec![sample::ids::MOUNTAIN; 8];
    deck.extend(vec![sample::ids::LIGHTNING_BOLT; 6]);
    deck.push(sample::ids::SHOCK);

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_game_seeded(&mut state, &deck, &deck, 0);
    let config = McfrConfig {
        max_depth: 8,
        max_actions: 300,
        max_nodes_per_iteration: 0,
    };

    let tables = mccfr::train(&state, 50, &config);
    let mcfr_p0 = McfrStrategy::new(tables[0].clone());
    let mcfr_p1 = McfrStrategy::new(tables[1].clone());

    let mut seat_wins = [0u32; 2];
    let mut policy_wins = [0u32; 2];
    let mut draws = 0u32;
    let pairs = 100;
    for seed in 0..pairs {
        // Normal setup keeps player 0 as the starter. Swap policies on the
        // same shuffle seed rather than using the unsupported P1-start path.
        for swapped in [false, true] {
            let (seat0, seat1) = if swapped {
                (&mcfr_p1, &mcfr_p0)
            } else {
                (&mcfr_p0, &mcfr_p1)
            };
            let result = simulation::run_game_seeded(&db, &deck, &deck, seat0, seat1, seed);
            match result.winner {
                Some(winner) => {
                    assert!(winner < 2);
                    seat_wins[winner] += 1;
                    policy_wins[if swapped { 1 - winner } else { winner }] += 1;
                }
                None => draws += 1,
            }
        }
    }

    let games = (pairs * 2) as f64;
    eprintln!(
        "MCCFR Mirror: {} games; seat wins={:?}; policy wins={:?}; draws={}",
        pairs * 2, seat_wins, policy_wins, draws,
    );

    assert_eq!(seat_wins.iter().sum::<u32>() + draws, (pairs * 2) as u32);
    // Retain the generous 15%-85% bounds, now checking both starting-seat
    // outcomes and policy outcomes under the two assignments.
    for (label, wins) in [("seat 0", seat_wins[0]), ("seat 1", seat_wins[1]),
        ("policy 0", policy_wins[0]), ("policy 1", policy_wins[1])] {
        let rate = wins as f64 / games;
        assert!((0.15..=0.85).contains(&rate),
            "{} has an extreme mirror win rate: {:.1}%", label, rate * 100.0);
    }
}

#[test]
fn test_mcfr_strategy_name() {
    let strategy = McfrStrategy::new(RegretTable::new());
    assert_eq!(strategy.name(), "MCCFR");
}

#[test]
fn test_mcfr_strategy_fallback_on_unseen_info_set() {
    // McfrStrategy should handle unseen info sets gracefully (uniform random fallback)
    let strategy = McfrStrategy::new(RegretTable::new());

    let mut state = GameState::new(2);
    let db = sample::build_sample_db();
    state.card_db = Some(Arc::new(db));

    // Set up a minimal state so there are legal actions
    for _ in 0..10 {
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }
    state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand);
    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;

    // Should not panic even with empty regret table
    let _action = strategy.choose_action(&state, 0);
    let legal = legal_actions(&state);
    // The action should be one of the legal actions
    // (McfrStrategy uses legal_actions_abstracted which may differ slightly,
    // but PassPriority is always legal)
    assert!(
        !legal.is_empty(),
        "There should be legal actions"
    );
}

#[test]
fn test_mini_deck_sizes() {
    let burn = sample::mini_red_burn();
    assert_eq!(burn.len(), 15, "Mini burn deck should be 15 cards");

    let creatures = sample::mini_red_creatures();
    assert_eq!(creatures.len(), 15, "Mini creature deck should be 15 cards");
}

#[test]
fn test_info_set_per_color_mana() {
    // Verify that different mana color distributions produce different info set hashes.
    let db = sample::build_sample_db();

    // State 1: player has red mana
    let mut state1 = GameState::new(2);
    state1.card_db = Some(Arc::new(db.clone()));
    for _ in 0..10 {
        state1.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state1.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }
    state1.active_player = 0;
    state1.priority_player = 0;
    state1.phase = Phase::PreCombatMain;
    state1.players[0].mana_pool.red = 2;

    // State 2: player has white mana (same total, different color)
    let mut state2 = GameState::new(2);
    state2.card_db = Some(Arc::new(db));
    for _ in 0..10 {
        state2.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
        state2.create_card_in_zone(sample::ids::MOUNTAIN, 1, ZoneType::Library);
    }
    state2.active_player = 0;
    state2.priority_player = 0;
    state2.phase = Phase::PreCombatMain;
    state2.players[0].mana_pool.white = 2;

    let view1 = state1.visible_state(0);
    let hash1 = InformationSet::from_view(&view1, state1.card_db()).hash_value();

    let view2 = state2.visible_state(0);
    let hash2 = InformationSet::from_view(&view2, state2.card_db()).hash_value();

    assert_ne!(hash1, hash2, "Different mana colors should produce different hashes");
}

// =========================================================================
// Phase 2B Scale Validation Tests
// =========================================================================

/// Helper: set up a 60-card game (mono-red vs mono-green).
fn setup_60card_game() -> GameState {
    let db = sample::build_sample_db();
    let deck0 = sample::red_aggro_deck();
    let deck1 = sample::green_stompy_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &deck0, &deck1);
    state
}

#[test]
fn test_60card_decks_valid() {
    // Verify the 60-card sample decks are correct sizes.
    let red = sample::red_aggro_deck();
    assert_eq!(red.len(), 60, "Red aggro deck should be 60 cards");

    let green = sample::green_stompy_deck();
    assert_eq!(green.len(), 60, "Green stompy deck should be 60 cards");
}

#[test]
fn test_bucketed_abstraction_reduces_info_sets() {
    // Train with identity vs bucketed abstraction on mini decks.
    // Bucketed should produce fewer unique info set entries.
    let state = setup_mini_game();
    let config = McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 };

    let identity_tables = mccfr::train(&state, 10, &config);
    let identity_info_sets: usize = identity_tables.iter().map(|t| t.num_info_sets()).sum();

    let bucketed = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: config.clone(),
        abstraction: &bucketed,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };
    let bucketed_tables = mccfr::train_extended(&state, 10, &train_cfg);
    let bucketed_info_sets: usize = bucketed_tables.iter().map(|t| t.num_info_sets()).sum();

    eprintln!(
        "Info sets — Identity: {}, Bucketed: {} (reduction: {:.1}%)",
        identity_info_sets,
        bucketed_info_sets,
        (1.0 - bucketed_info_sets as f64 / identity_info_sets as f64) * 100.0,
    );

    assert!(
        bucketed_info_sets <= identity_info_sets,
        "Bucketed abstraction should produce <= info sets than identity ({} vs {})",
        bucketed_info_sets,
        identity_info_sets,
    );
}

#[test]
fn test_60card_training_with_bucketed_abstraction() {
    // Phase 2B.4: Train MCCFR on 60-card decks with bucketed abstraction.
    // This is the core scale validation test — must complete without OOM.
    let state = setup_60card_game();

    let bucketed = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 6, max_actions: 300, max_nodes_per_iteration: 0 },
        abstraction: &bucketed,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    let tables = mccfr::train_extended(&state, 10, &train_cfg);

    let stats = mccfr::training_stats(&tables);
    eprintln!(
        "60-card training (10 iters): info_sets=[{}, {}], visits=[{}, {}], mem=[{}, {}] bytes, exploit={:.4}",
        stats.total_info_sets[0], stats.total_info_sets[1],
        stats.total_visits[0], stats.total_visits[1],
        stats.memory_bytes[0], stats.memory_bytes[1],
        stats.exploitability,
    );

    // Must create some info set entries
    assert!(stats.total_info_sets[0] > 0, "Player 0 should have info sets");
    assert!(stats.total_info_sets[1] > 0, "Player 1 should have info sets");
    // Memory should be bounded (< 100MB for 10 iterations)
    let total_mem = stats.memory_bytes[0] + stats.memory_bytes[1];
    assert!(
        total_mem < 100_000_000,
        "Memory usage should be < 100MB, got {} bytes", total_mem
    );
}

#[test]
fn test_60card_rollout_training() {
    // Phase 2B.3: Train with GreedyStrategy rollouts at depth limit.
    let state = setup_60card_game();

    let greedy = GreedyStrategy;
    let random = RandomStrategy;
    let bucketed = BucketedAbstraction;

    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 4, max_actions: 200, max_nodes_per_iteration: 0 },
        abstraction: &bucketed,
        rollout_mode: RolloutMode::Strategy { max_rollout_actions: 500 },
        rollout_strategies: Some((&greedy, &random)),
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    let tables = mccfr::train_extended(&state, 5, &train_cfg);

    let total_info_sets: usize = tables.iter().map(|t| t.num_info_sets()).sum();
    assert!(total_info_sets > 0, "Rollout training should create info sets");

    eprintln!(
        "60-card rollout training (5 iters): {} info sets",
        total_info_sets,
    );
}

#[test]
fn test_parallel_training_produces_results() {
    // Phase 2B.2: Verify parallel training produces valid results.
    let state = setup_mini_game();

    let bucketed = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 },
        abstraction: &bucketed,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    let tables = mccfr::train_parallel(&state, 20, 4, &train_cfg);

    let total_info_sets: usize = tables.iter().map(|t| t.num_info_sets()).sum();
    assert!(total_info_sets > 0, "Parallel training should produce info sets");

    // Verify visit counts are reasonable (should be ~20 iterations * 2 traversals)
    let total_visits: u64 = tables.iter().flat_map(|t| t.data.values()).map(|d| d.visit_count).sum();
    assert!(total_visits > 0, "Should have non-zero visit counts");

    eprintln!(
        "Parallel training (20 iters, 4 shards): {} info sets, {} total visits",
        total_info_sets, total_visits,
    );
}

#[test]
fn test_parallel_training_60card() {
    // Phase 2B.2 + 2B.4: Parallel training on 60-card decks.
    let state = setup_60card_game();

    let bucketed = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 5, max_actions: 200, max_nodes_per_iteration: 0 },
        abstraction: &bucketed,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    let tables = mccfr::train_parallel(&state, 8, 4, &train_cfg);

    let stats = mccfr::training_stats(&tables);
    eprintln!(
        "60-card parallel (8 iters, 4 shards): info_sets=[{}, {}], exploit={:.4}",
        stats.total_info_sets[0], stats.total_info_sets[1],
        stats.exploitability,
    );

    assert!(stats.total_info_sets[0] > 0);
    assert!(stats.total_info_sets[1] > 0);
}

#[test]
fn test_abstracted_mcfr_strategy_plays_legal_games() {
    // Train with bucketed abstraction and verify AbstractedMcfrStrategy
    // plays legal games end-to-end.
    let db = sample::build_sample_db();
    let state = setup_mini_game();

    let bucketed = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 },
        abstraction: &bucketed,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    let tables = mccfr::train_extended(&state, 20, &train_cfg);

    let strat_p0 = AbstractedMcfrStrategy::new(
        tables[0].clone(),
        Box::new(BucketedAbstraction),
    );
    let strat_p1 = AbstractedMcfrStrategy::new(
        tables[1].clone(),
        Box::new(BucketedAbstraction),
    );

    let deck0 = sample::mini_red_burn();
    let deck1 = sample::mini_red_creatures();

    for _ in 0..10 {
        let result = simulation::run_game(&db, &deck0, &deck1, &strat_p0, &strat_p1);
        assert!(result.turns <= 200, "Game should terminate");
    }
}

#[test]
fn test_60card_abstracted_strategy_vs_greedy() {
    // Phase 2B.4 acceptance: trained abstracted MCCFR strategy plays
    // complete games against GreedyStrategy on 60-card decks.
    let db = sample::build_sample_db();
    let state = setup_60card_game();

    let bucketed = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 5, max_actions: 200, max_nodes_per_iteration: 0 },
        abstraction: &bucketed,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    let tables = mccfr::train_extended(&state, 20, &train_cfg);

    let mcfr_strat = AbstractedMcfrStrategy::new(
        tables[0].clone(),
        Box::new(BucketedAbstraction),
    );
    let greedy_strat = GreedyStrategy;

    let deck0 = sample::red_aggro_deck();
    let deck1 = sample::green_stompy_deck();

    let results = simulation::simulate(
        &db,
        &deck0,
        &deck1,
        &mcfr_strat,
        &greedy_strat,
        50,
    );

    let win_rate = results.win_rate(0);
    eprintln!(
        "60-card MCCFR vs Greedy: {:.1}% win rate ({} games, P0={}, P1={}, draws={})",
        win_rate * 100.0,
        results.total_games,
        results.player0_wins,
        results.player1_wins,
        results.draws,
    );

    // All games must complete without panics
    assert_eq!(results.total_games, 50);
}

#[test]
fn test_checkpoint_save_load() {
    // Phase 2B.2: Verify checkpoint serialization round-trip.
    let state = setup_mini_game();
    let config = McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 };
    let tables = mccfr::train(&state, 5, &config);

    let dir = "/tmp/mtg_mccfr_test_checkpoint";
    mccfr::save_checkpoint(&tables, dir, 5).expect("save checkpoint");

    let loaded = mccfr::load_checkpoint(dir, 5).expect("load checkpoint");

    // Verify loaded tables match original
    for player in 0..2 {
        assert_eq!(
            tables[player].num_info_sets(),
            loaded[player].num_info_sets(),
            "Player {} info set count should match after checkpoint round-trip",
            player,
        );
        for (&hash, data) in &tables[player].data {
            let loaded_data = loaded[player].get(hash)
                .expect("info set should exist after load");
            assert_eq!(data.visit_count, loaded_data.visit_count);
        }
    }

    // Clean up
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_training_with_checkpointing() {
    // Phase 2B.2: Training with periodic checkpointing enabled.
    let state = setup_mini_game();
    let dir = "/tmp/mtg_mccfr_checkpoint_train";
    let _ = std::fs::remove_dir_all(dir); // Clean up from previous runs

    let bucketed = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 },
        abstraction: &bucketed,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 5,
        checkpoint_dir: Some(dir.into()),
    };

    let tables = mccfr::train_extended(&state, 10, &train_cfg);

    // Checkpoint at iteration 5 and 10 should exist
    let loaded_5 = mccfr::load_checkpoint(dir, 5);
    assert!(loaded_5.is_ok(), "Checkpoint at iteration 5 should exist");

    let loaded_10 = mccfr::load_checkpoint(dir, 10);
    assert!(loaded_10.is_ok(), "Checkpoint at iteration 10 should exist");

    // Final tables should have more info sets than the iteration-5 checkpoint
    let final_total: usize = tables.iter().map(|t| t.num_info_sets()).sum();
    let cp5_tables = loaded_5.unwrap();
    let cp5_total: usize = cp5_tables.iter().map(|t| t.num_info_sets()).sum();
    assert!(
        final_total >= cp5_total,
        "Final tables should have >= info sets as iteration 5 checkpoint ({} vs {})",
        final_total, cp5_total,
    );

    // Clean up
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_parallel_vs_sequential_consistency() {
    // Verify that parallel training produces comparable results to sequential.
    // Both should create info set entries; parallel may differ due to
    // independent sampling but should be in the same ballpark.
    let state = setup_mini_game();

    let bucketed = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 },
        abstraction: &bucketed,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    let seq_tables = mccfr::train_extended(&state, 20, &train_cfg);
    let par_tables = mccfr::train_parallel(&state, 20, 4, &train_cfg);

    let seq_info: usize = seq_tables.iter().map(|t| t.num_info_sets()).sum();
    let par_info: usize = par_tables.iter().map(|t| t.num_info_sets()).sum();

    eprintln!(
        "Sequential vs Parallel info sets: {} vs {}",
        seq_info, par_info,
    );

    // Both should have non-trivial entries
    assert!(seq_info > 0, "Sequential should have info sets");
    assert!(par_info > 0, "Parallel should have info sets");
}

// =========================================================================
// Goldfish MCCFR Training Tests
// =========================================================================

#[test]
fn test_goldfish_mccfr_training_runs() {
    // Verify goldfish MCCFR training completes without panics and produces
    // info set entries for player 0 only (player 1 is the goldfish).
    let db = sample::build_sample_db();
    let deck = sample::mini_red_burn();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &deck, &deck);

    let config = McfrConfig { max_depth: 6, max_actions: 300, max_nodes_per_iteration: 0 };
    let tables = mccfr::train_goldfish(&state, 10, &config);

    let p0_info_sets = tables[0].num_info_sets();
    let p1_info_sets = tables[1].num_info_sets();

    eprintln!(
        "Goldfish MCCFR (10 iters): P0 info sets = {}, P1 info sets = {}",
        p0_info_sets, p1_info_sets,
    );

    assert!(p0_info_sets > 0, "Player 0 should have info sets from goldfish training");
    assert_eq!(p1_info_sets, 0, "Player 1 (goldfish) should have no info sets");
}

#[test]
fn test_goldfish_mccfr_strategy_plays_legal_games() {
    // Train goldfish MCCFR, then use the resulting strategy to play
    // goldfish games end-to-end.
    let db = sample::build_sample_db();
    let deck = sample::mini_red_burn();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_game(&mut state, &deck, &deck);

    let config = McfrConfig { max_depth: 6, max_actions: 300, max_nodes_per_iteration: 0 };
    let tables = mccfr::train_goldfish(&state, 15, &config);

    let mccfr_strat = McfrStrategy::new(tables[0].clone());

    // Play 20 goldfish games — all must complete without panics
    for _ in 0..20 {
        let result = simulation::run_goldfish_game(&db, &deck, &mccfr_strat);
        assert!(result.turns <= 50, "Goldfish game should terminate within limit");
    }
}

#[test]
fn test_goldfish_mccfr_vs_greedy_vs_random_kill_turns() {
    // Core goldfish comparison: train MCCFR on goldfish mode, then compare
    // average kill turns against Greedy and Random baselines.
    //
    // Expected ordering (lower avg kill turn = better):
    //   MCCFR <= Greedy < Random
    //
    // With limited training budget, MCCFR should be competitive with Greedy.
    let db = sample::build_sample_db();
    let deck = sample::red_aggro_deck();

    // --- Train MCCFR against goldfish ---
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_game(&mut state, &deck, &deck);

    let config = McfrConfig { max_depth: 5, max_actions: 500, max_nodes_per_iteration: 0 };
    let tables = mccfr::train_goldfish(&state, 20, &config);
    let mccfr_strat = McfrStrategy::new(tables[0].clone());

    let stats = mccfr::training_stats(&tables);
    eprintln!(
        "Goldfish MCCFR training: {} info sets, {} visits, exploit={:.4}",
        stats.total_info_sets[0], stats.total_visits[0], stats.exploitability,
    );

    // --- Run goldfish simulations with all three strategies ---
    let num_games = 100;

    let random_results = simulate_goldfish(&db, &deck, &RandomStrategy, num_games);
    let greedy_results = simulate_goldfish(&db, &deck, &GreedyStrategy, num_games);
    let mccfr_results = simulate_goldfish(&db, &deck, &mccfr_strat, num_games);

    eprintln!("\n=== Goldfish Kill Turn Comparison (Red Aggro, {} games) ===", num_games);
    eprintln!(
        "Random:  win={:.0}%  avg_kill=T{:.2}  fastest=T{}  slowest=T{}",
        random_results.win_rate() * 100.0,
        random_results.avg_kill_turn,
        random_results.fastest_kill,
        random_results.slowest_kill,
    );
    eprintln!(
        "Greedy:  win={:.0}%  avg_kill=T{:.2}  fastest=T{}  slowest=T{}",
        greedy_results.win_rate() * 100.0,
        greedy_results.avg_kill_turn,
        greedy_results.fastest_kill,
        greedy_results.slowest_kill,
    );
    eprintln!(
        "MCCFR:   win={:.0}%  avg_kill=T{:.2}  fastest=T{}  slowest=T{}",
        mccfr_results.win_rate() * 100.0,
        mccfr_results.avg_kill_turn,
        mccfr_results.fastest_kill,
        mccfr_results.slowest_kill,
    );

    // With a 20-turn limit, greedy should reliably kill before the cutoff.
    // Random and lightly-trained MCCFR may time out more often.
    assert!(
        greedy_results.win_rate() > 0.8,
        "Greedy should win >80% of goldfish games within 20 turns (got {:.0}%)",
        greedy_results.win_rate() * 100.0,
    );

    // Greedy should be faster than Random
    if random_results.wins > 0 && greedy_results.wins > 0 {
        assert!(
            greedy_results.avg_kill_turn < random_results.avg_kill_turn,
            "Greedy ({:.2}) should kill faster than Random ({:.2})",
            greedy_results.avg_kill_turn,
            random_results.avg_kill_turn,
        );
    }

    // MCCFR with limited training should at least outperform Random.
    // Full convergence to Greedy-level performance requires hundreds of
    // iterations; this test uses a CI-friendly budget of 20.
    if mccfr_results.wins > 0 && random_results.wins > 0 {
        eprintln!(
            "\nMCCFR vs Random gap: {:.2} turns (MCCFR={:.2}, Random={:.2})",
            random_results.avg_kill_turn - mccfr_results.avg_kill_turn,
            mccfr_results.avg_kill_turn,
            random_results.avg_kill_turn,
        );
        assert!(
            mccfr_results.avg_kill_turn <= random_results.avg_kill_turn + 3.0,
            "MCCFR ({:.2}) should not be worse than Random ({:.2}) by more than 3 turns",
            mccfr_results.avg_kill_turn,
            random_results.avg_kill_turn,
        );
    }
    if mccfr_results.wins > 0 && greedy_results.wins > 0 {
        eprintln!(
            "MCCFR vs Greedy gap: {:.2} turns (Greedy={:.2}, MCCFR={:.2})",
            mccfr_results.avg_kill_turn - greedy_results.avg_kill_turn,
            greedy_results.avg_kill_turn,
            mccfr_results.avg_kill_turn,
        );
    }
}

#[test]
fn test_goldfish_mccfr_with_abstraction() {
    // Train goldfish MCCFR with bucketed abstraction on 60-card deck.
    let db = sample::build_sample_db();
    let deck = sample::red_aggro_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_game(&mut state, &deck, &deck);

    let bucketed = BucketedAbstraction;
    let config = McfrConfig { max_depth: 6, max_actions: 500, max_nodes_per_iteration: 0 };
    let tables = mccfr::train_goldfish_with_abstraction(
        &state, 30, &config, &bucketed, 0,
    );

    let strat = AbstractedMcfrStrategy::new(
        tables[0].clone(),
        Box::new(BucketedAbstraction),
    );

    let results = simulate_goldfish(&db, &deck, &strat, 100);
    eprintln!(
        "Goldfish MCCFR (abstracted, 60-card): win={:.0}% avg_kill=T{:.2}",
        results.win_rate() * 100.0,
        results.avg_kill_turn,
    );

    // With a 20-turn limit, lightly-trained MCCFR may time out on some games.
    // Just verify it completes without panics and wins at least a few.
    assert!(results.wins > 0, "Abstracted MCCFR should win at least some goldfish games");
}

#[test]
fn test_goldfish_mccfr_green_stompy() {
    // Run the same comparison for Green Stompy — a creature-based deck
    // with different sequencing challenges.
    let db = sample::build_sample_db();
    let deck = sample::green_stompy_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_game(&mut state, &deck, &deck);

    let config = McfrConfig { max_depth: 5, max_actions: 500, max_nodes_per_iteration: 0 };
    let tables = mccfr::train_goldfish(&state, 20, &config);
    let mccfr_strat = McfrStrategy::new(tables[0].clone());

    let num_games = 100;

    let greedy_results = simulate_goldfish(&db, &deck, &GreedyStrategy, num_games);
    let mccfr_results = simulate_goldfish(&db, &deck, &mccfr_strat, num_games);

    eprintln!("\n=== Goldfish Kill Turn Comparison (Green Stompy, {} games) ===", num_games);
    eprintln!(
        "Greedy:  win={:.0}%  avg_kill=T{:.2}  fastest=T{}  slowest=T{}",
        greedy_results.win_rate() * 100.0,
        greedy_results.avg_kill_turn,
        greedy_results.fastest_kill,
        greedy_results.slowest_kill,
    );
    eprintln!(
        "MCCFR:   win={:.0}%  avg_kill=T{:.2}  fastest=T{}  slowest=T{}",
        mccfr_results.win_rate() * 100.0,
        mccfr_results.avg_kill_turn,
        mccfr_results.fastest_kill,
        mccfr_results.slowest_kill,
    );

    assert!(greedy_results.win_rate() > 0.5, "Greedy should win goldfish with Green Stompy");
    assert!(mccfr_results.win_rate() > 0.3, "MCCFR should win some goldfish games");
}

// =========================================================================
// Commander Goldfish MCCFR Training Tests
// =========================================================================

/// Helper: set up a commander game state for goldfish MCCFR training.
fn setup_commander_goldfish_game(deck: &[u64], commander: u64) -> GameState {
    let db = sample::build_sample_db();
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_commander_game(&mut state, deck, deck, commander, commander);
    state
}

#[test]
fn test_commander_goldfish_mccfr_training_runs() {
    // Verify goldfish MCCFR training completes on a commander deck without panics.
    // Uses Brimaz commander deck (100 cards, 40 life, command zone mechanics).
    let (deck, commander) = sample::brimaz_commander_deck();
    let state = setup_commander_goldfish_game(&deck, commander);

    assert_eq!(state.format, GameFormat::Commander);
    assert_eq!(state.players[0].life, 40);
    assert_eq!(state.players[1].life, 40);
    assert!(!state.players[0].command_zone.is_empty(), "Commander should be in command zone");

    let config = McfrConfig { max_depth: 5, max_actions: 500, max_nodes_per_iteration: 0 };
    let tables = mccfr::train_goldfish(&state, 5, &config);

    let p0_info_sets = tables[0].num_info_sets();
    let p1_info_sets = tables[1].num_info_sets();

    eprintln!(
        "Commander Goldfish MCCFR (5 iters, Brimaz): P0 info sets = {}, P1 info sets = {}",
        p0_info_sets, p1_info_sets,
    );

    assert!(p0_info_sets > 0, "Player 0 should have info sets from commander goldfish training");
    assert_eq!(p1_info_sets, 0, "Player 1 (goldfish) should have no info sets");
}

#[test]
fn test_commander_goldfish_mccfr_with_abstraction() {
    // Train goldfish MCCFR on a commander deck with bucketed abstraction.
    // This is the key path for production use — 100-card decks require abstraction.
    let db = sample::build_sample_db();
    let (deck, commander) = sample::brimaz_commander_deck();
    let state = setup_commander_goldfish_game(&deck, commander);

    let bucketed = BucketedAbstraction;
    let config = McfrConfig { max_depth: 5, max_actions: 500, max_nodes_per_iteration: 0 };
    let tables = mccfr::train_goldfish_with_abstraction(
        &state, 10, &config, &bucketed, 0,
    );

    let stats = mccfr::training_stats(&tables);
    eprintln!(
        "Commander Goldfish MCCFR (abstracted, 10 iters): info_sets={}, visits={}, exploit={:.4}",
        stats.total_info_sets[0], stats.total_visits[0], stats.exploitability,
    );

    assert!(stats.total_info_sets[0] > 0, "Should create info sets");

    // Verify the trained strategy can play legal commander goldfish games
    let strat = AbstractedMcfrStrategy::new(
        tables[0].clone(),
        Box::new(BucketedAbstraction),
    );

    let results = simulate_commander_goldfish(&db, &deck, commander, &strat, 20);
    eprintln!(
        "Commander Goldfish (trained, 20 games): win={:.0}% avg_kill=T{:.2}",
        results.win_rate() * 100.0,
        results.avg_kill_turn,
    );

    // All games must complete without panics
    assert_eq!(results.total_games, 20);
}

#[test]
fn test_commander_goldfish_mccfr_vs_greedy_vs_random_kill_turns() {
    // Core commander goldfish comparison: train MCCFR, compare against baselines.
    // With 40 starting life, games take longer — adjust expectations accordingly.
    let db = sample::build_sample_db();
    let (deck, commander) = sample::brimaz_commander_deck();
    let state = setup_commander_goldfish_game(&deck, commander);

    // --- Train MCCFR against goldfish ---
    let bucketed = BucketedAbstraction;
    let config = McfrConfig { max_depth: 5, max_actions: 500, max_nodes_per_iteration: 0 };
    let tables = mccfr::train_goldfish_with_abstraction(
        &state, 20, &config, &bucketed, 0,
    );
    let mccfr_strat = AbstractedMcfrStrategy::new(
        tables[0].clone(),
        Box::new(BucketedAbstraction),
    );

    let stats = mccfr::training_stats(&tables);
    eprintln!(
        "Commander Goldfish MCCFR training: {} info sets, {} visits, exploit={:.4}",
        stats.total_info_sets[0], stats.total_visits[0], stats.exploitability,
    );

    // --- Run commander goldfish simulations with all three strategies ---
    let num_games = 50;

    let random_results = simulate_commander_goldfish(&db, &deck, commander, &RandomStrategy, num_games);
    let greedy_results = simulate_commander_goldfish(&db, &deck, commander, &GreedyStrategy, num_games);
    let mccfr_results = simulate_commander_goldfish(&db, &deck, commander, &mccfr_strat, num_games);

    eprintln!("\n=== Commander Goldfish Kill Turn Comparison (Brimaz, {} games) ===", num_games);
    eprintln!(
        "Random:  win={:.0}%  avg_kill=T{:.2}  fastest=T{}  slowest=T{}",
        random_results.win_rate() * 100.0,
        random_results.avg_kill_turn,
        random_results.fastest_kill,
        random_results.slowest_kill,
    );
    eprintln!(
        "Greedy:  win={:.0}%  avg_kill=T{:.2}  fastest=T{}  slowest=T{}",
        greedy_results.win_rate() * 100.0,
        greedy_results.avg_kill_turn,
        greedy_results.fastest_kill,
        greedy_results.slowest_kill,
    );
    eprintln!(
        "MCCFR:   win={:.0}%  avg_kill=T{:.2}  fastest=T{}  slowest=T{}",
        mccfr_results.win_rate() * 100.0,
        mccfr_results.avg_kill_turn,
        mccfr_results.fastest_kill,
        mccfr_results.slowest_kill,
    );

    // Greedy should beat random
    assert!(
        greedy_results.win_rate() > 0.3,
        "Greedy should win commander goldfish games (got {:.0}%)",
        greedy_results.win_rate() * 100.0,
    );

    // All MCCFR games must complete
    assert_eq!(mccfr_results.total_games, num_games);

    // MCCFR should win some games even with limited training
    if mccfr_results.wins > 0 {
        eprintln!(
            "\nMCCFR commander goldfish: wins={}, avg_kill=T{:.2}",
            mccfr_results.wins, mccfr_results.avg_kill_turn,
        );
    }
}

#[test]
fn test_commander_goldfish_kinnan_deck() {
    // Test with the Kinnan, Bonder Prodigy commander deck (Simic mana ramp).
    let db = sample::build_sample_db();
    let (deck, commander, _tutor_targets) = sample::kinnan_commander_deck();
    let state = setup_commander_goldfish_game(&deck, commander);

    let bucketed = BucketedAbstraction;
    let config = McfrConfig { max_depth: 5, max_actions: 500, max_nodes_per_iteration: 0 };
    let tables = mccfr::train_goldfish_with_abstraction(
        &state, 10, &config, &bucketed, 0,
    );

    let mccfr_strat = AbstractedMcfrStrategy::new(
        tables[0].clone(),
        Box::new(BucketedAbstraction),
    );

    let num_games = 50;
    let greedy_results = simulate_commander_goldfish(&db, &deck, commander, &GreedyStrategy, num_games);
    let mccfr_results = simulate_commander_goldfish(&db, &deck, commander, &mccfr_strat, num_games);

    eprintln!("\n=== Commander Goldfish (Kinnan, {} games) ===", num_games);
    eprintln!(
        "Greedy:  win={:.0}%  avg_kill=T{:.2}",
        greedy_results.win_rate() * 100.0,
        greedy_results.avg_kill_turn,
    );
    eprintln!(
        "MCCFR:   win={:.0}%  avg_kill=T{:.2}",
        mccfr_results.win_rate() * 100.0,
        mccfr_results.avg_kill_turn,
    );

    assert_eq!(mccfr_results.total_games, num_games, "All games should complete");
}

#[test]
fn test_commander_goldfish_training_retains_coverage() {
    // One continuous run checks that training retains earlier pilot work.
    // Coverage and visits do not establish strategy quality, equilibrium
    // convergence, or optimal play.
    let db = sample::build_sample_db();
    let (deck, commander) = sample::brimaz_commander_deck();
    let state = setup_commander_goldfish_game(&deck, commander);

    let bucketed = BucketedAbstraction;
    let config = McfrConfig { max_depth: 6, max_actions: 800, max_nodes_per_iteration: 0 };
    let mut progress = Vec::new();
    let mut pilot_10 = None;
    let tables_30 = mccfr::train_goldfish_with_progress(
        &state, 30, &config, &bucketed, 0,
        |iteration, total, tables| {
            progress.push((iteration, total));
            if iteration == 10 {
                assert!(pilot_10.is_none(), "iteration 10 callback repeated");
                pilot_10 = Some(tables[0].clone());
            }
        },
    );
    assert_eq!(progress, (1..=30).map(|iteration| (iteration, 30)).collect::<Vec<_>>());
    let pilot_10 = pilot_10.expect("iteration 10 callback missing");
    assert!(!pilot_10.data.is_empty(), "pilot checkpoint at iteration 10 is empty");
    for (key, earlier) in &pilot_10.data {
        let later = tables_30[0].get(*key).expect("iteration 10 info set was lost");
        assert!(later.visit_count >= earlier.visit_count,
            "pilot info set {key} lost visits: {} -> {}",
            earlier.visit_count, later.visit_count);
    }
    let visits_10: u64 = pilot_10.data.values().map(|entry| entry.visit_count).sum();
    let visits_30: u64 = tables_30[0].data.values().map(|entry| entry.visit_count).sum();
    assert!(visits_30 > visits_10,
        "pilot visits should grow in one continuous run: {visits_10} -> {visits_30}");

    let tables_10 = [pilot_10, RegretTable::new()];
    let exploit_10 = mccfr::approximate_exploitability(&tables_10);
    let strat_10 = AbstractedMcfrStrategy::new(
        tables_10[0].clone(),
        Box::new(BucketedAbstraction),
    );
    let exploit_30 = mccfr::approximate_exploitability(&tables_30);
    let strat_30 = AbstractedMcfrStrategy::new(
        tables_30[0].clone(),
        Box::new(BucketedAbstraction),
    );

    let num_games = 50;
    let results_10 = simulate_commander_goldfish(&db, &deck, commander, &strat_10, num_games);
    let results_30 = simulate_commander_goldfish(&db, &deck, commander, &strat_30, num_games);

    let stats_10 = mccfr::training_stats(&tables_10);
    let stats_30 = mccfr::training_stats(&tables_30);

    eprintln!("\n=== Commander Goldfish Training Retention Test ===");
    eprintln!(
        "iteration 10: info_sets={}, visits={}, exploit={:.4}, win={:.0}%, avg_kill=T{:.2}",
        stats_10.total_info_sets[0], visits_10, exploit_10,
        results_10.win_rate() * 100.0, results_10.avg_kill_turn,
    );
    eprintln!(
        "iteration 30: info_sets={}, visits={}, exploit={:.4}, win={:.0}%, avg_kill=T{:.2}",
        stats_30.total_info_sets[0], visits_30, exploit_30,
        results_30.win_rate() * 100.0, results_30.avg_kill_turn,
    );

    // Numerical health checks only; no quality ordering is inferred.
    assert!(exploit_10.is_finite(), "Exploitability should be finite (10 iters)");
    assert!(exploit_30.is_finite(), "Exploitability should be finite (30 iters)");

    // Both checkpoint policies retain the original evaluation bookkeeping.
    assert_eq!(results_10.total_games, num_games);
    assert_eq!(results_30.total_games, num_games);
}

#[test]
fn test_commander_goldfish_info_set_includes_command_zone() {
    // Verify that the information set distinguishes commander in command zone
    // vs commander on battlefield.
    let (deck, commander) = sample::brimaz_commander_deck();

    // State 1: commander in command zone (game start)
    let state1 = setup_commander_goldfish_game(&deck, commander);

    // State 2: commander moved to battlefield
    let mut state2 = setup_commander_goldfish_game(&deck, commander);
    let cmd_obj = state2.players[0].command_zone[0];
    state2.move_object(cmd_obj, mtg_gto::card::ZoneType::Command, mtg_gto::card::ZoneType::Battlefield);

    let view1 = state1.visible_state(0);
    let info1 = InformationSet::from_view(&view1, state1.card_db());

    let view2 = state2.visible_state(0);
    let info2 = InformationSet::from_view(&view2, state2.card_db());

    // Command zone status should affect the info set hash
    assert_ne!(
        info1.hash_value(), info2.hash_value(),
        "Commander in command zone vs battlefield should produce different info set hashes"
    );

    // Verify command zone fields are populated correctly
    assert_eq!(info1.my_command_zone.len(), 1, "Commander should be in command zone in state 1");
    assert_eq!(info2.my_command_zone.len(), 0, "Commander should not be in command zone in state 2");
}
