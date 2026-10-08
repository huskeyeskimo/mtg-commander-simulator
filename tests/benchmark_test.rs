//! Phase 3C — Engine and Solver Throughput Benchmarks
//!
//! These benchmarks measure:
//! - **Engine throughput**: games/sec for GreedyStrategy vs. GreedyStrategy
//! - **Solver throughput**: states-evaluated/sec (clone + legal_actions_abstracted +
//!   apply_action + is_terminal)
//!
//! Results are printed to stdout. Run with:
//! ```bash
//! cargo test --release --test benchmark_test -- --nocapture
//! ```

use mtg_gto::card::sample;
use mtg_gto::simulation::simulate;
use mtg_gto::strategy::GreedyStrategy;
use mtg_gto::action::legal_actions_abstracted;
use mtg_gto::rules;
use mtg_gto::solver::mccfr::{
    McfrConfig, TrainConfig, RolloutMode,
    train_extended, training_stats, approximate_exploitability,
};
use mtg_gto::info_set::BucketedAbstraction;
use mtg_gto::game::GameState;
use std::sync::Arc;
use std::time::Instant;

/// Benchmark: Engine throughput — games/sec for GreedyStrategy vs. GreedyStrategy
#[test]
fn benchmark_engine_throughput() {
    let db = sample::build_sample_db();
    let deck0 = sample::red_aggro_deck();
    let deck1 = sample::green_stompy_deck();

    let greedy = GreedyStrategy;
    let num_games = 500;

    let start = Instant::now();
    let results = simulate(&db, &deck0, &deck1, &greedy, &greedy, num_games);
    let elapsed = start.elapsed();

    let games_per_sec = num_games as f64 / elapsed.as_secs_f64();
    let actions_per_sec = (results.avg_actions * num_games as f64) / elapsed.as_secs_f64();

    println!("\n=== Engine Throughput Benchmark ===");
    println!("Games:          {}", num_games);
    println!("Elapsed:        {:.2}s", elapsed.as_secs_f64());
    println!("Games/sec:      {:.1}", games_per_sec);
    println!("Actions/sec:    {:.0}", actions_per_sec);
    println!("Avg turns/game: {:.1}", results.avg_turns);
    println!("Avg actions:    {:.0}", results.avg_actions);
    println!("P0 wins:        {}  P1 wins:  {}  Draws: {}", results.player0_wins, results.player1_wins, results.draws);

    // Sanity check: we should be able to run at least 100 games/sec in release mode
    // In debug mode, this will be slower but the test still validates correctness
    assert!(games_per_sec > 1.0, "Engine throughput too low: {:.1} games/sec", games_per_sec);
}

/// Benchmark: Solver state evaluation throughput
#[test]
fn benchmark_solver_throughput() {
    let db = sample::build_sample_db();
    let deck0 = sample::red_aggro_deck();
    let deck1 = sample::green_stompy_deck();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &deck0, &deck1);

    let num_evaluations = 5000;
    let mut total_actions_evaluated = 0u64;
    let mut total_clones = 0u64;

    let start = Instant::now();

    // Simulate the solver's hot loop: clone → legal_actions → apply_action → check terminal
    let mut current = state.clone();
    for _ in 0..num_evaluations {
        if current.game_over {
            current = state.clone();
            total_clones += 1;
        }

        let cloned = current.clone();
        total_clones += 1;

        let actions = legal_actions_abstracted(&cloned);
        total_actions_evaluated += actions.len() as u64;

        if !actions.is_empty() {
            let mut next = cloned;
            rules::apply_action(&mut next, &actions[0]);
            let _terminal = next.game_over;
            current = next;
        } else {
            rules::apply_action(&mut current, &mtg_gto::action::Action::PassPriority);
        }
    }

    let elapsed = start.elapsed();
    let evals_per_sec = num_evaluations as f64 / elapsed.as_secs_f64();
    let clones_per_sec = total_clones as f64 / elapsed.as_secs_f64();

    println!("\n=== Solver Throughput Benchmark ===");
    println!("Evaluations:    {}", num_evaluations);
    println!("Elapsed:        {:.2}s", elapsed.as_secs_f64());
    println!("Evals/sec:      {:.0}", evals_per_sec);
    println!("Clones/sec:     {:.0}", clones_per_sec);
    println!("Total actions:  {}", total_actions_evaluated);

    assert!(evals_per_sec > 10.0, "Solver throughput too low: {:.0} evals/sec", evals_per_sec);
}

/// Benchmark: MCCFR training throughput
#[test]
fn benchmark_mccfr_training_throughput() {
    let db = sample::build_sample_db();
    let deck0 = sample::mini_red_burn();
    let deck1 = sample::mini_red_creatures();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &deck0, &deck1);

    let abstraction = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 },
        abstraction: &abstraction,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    let num_iterations = 20;
    let start = Instant::now();
    let tables = train_extended(&state, num_iterations, &train_cfg).unwrap();
    let elapsed = start.elapsed();

    let stats = training_stats(&tables).unwrap();
    let iters_per_sec = num_iterations as f64 / elapsed.as_secs_f64();
    let exploitability = approximate_exploitability(&tables).unwrap();

    println!("\n=== MCCFR Training Throughput Benchmark ===");
    println!("Iterations:     {}", num_iterations);
    println!("Elapsed:        {:.2}s", elapsed.as_secs_f64());
    println!("Iters/sec:      {:.1}", iters_per_sec);
    println!("Info sets P0:   {}", stats.total_info_sets[0]);
    println!("Info sets P1:   {}", stats.total_info_sets[1]);
    println!("Memory P0:      {} bytes", stats.memory_bytes[0]);
    println!("Memory P1:      {} bytes", stats.memory_bytes[1]);
    println!("Exploitability: {:.4}", exploitability);

    assert!(iters_per_sec > 0.1, "MCCFR throughput too low: {:.1} iters/sec", iters_per_sec);
}

/// Benchmark: Warm-started training comparison
#[test]
fn benchmark_warm_start_vs_cold_start() {
    use mtg_gto::solver::mccfr::warm_start_from_greedy;

    let db = sample::build_sample_db();
    let deck0 = sample::mini_red_burn();
    let deck1 = sample::mini_red_creatures();

    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    rules::setup_game(&mut state, &deck0, &deck1);

    let abstraction = BucketedAbstraction;
    let train_cfg = TrainConfig {
        mccfr: McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 },
        abstraction: &abstraction,
        rollout_mode: RolloutMode::Heuristic,
        rollout_strategies: None,
        checkpoint_interval: 0,
        checkpoint_dir: None,
    };

    // Cold start
    let start = Instant::now();
    let cold_tables = train_extended(&state, 10, &train_cfg).unwrap();
    let cold_elapsed = start.elapsed();
    let cold_exploit = approximate_exploitability(&cold_tables).unwrap();

    // Warm start (5 warmup games + 10 iterations)
    let start = Instant::now();
    let warm_tables = {
        let mut tables = warm_start_from_greedy(&state, 5, &abstraction, 1.0).unwrap();
        for _ in 0..10 {
            mtg_gto::solver::mccfr::run_iteration_with_abstraction(
                &state,
                &mut tables,
                &train_cfg.mccfr,
                train_cfg.abstraction,
                &train_cfg.rollout_mode,
                train_cfg.rollout_strategies,
            ).unwrap();
        }
        tables
    };
    let warm_elapsed = start.elapsed();
    let warm_exploit = approximate_exploitability(&warm_tables).unwrap();

    println!("\n=== Warm Start vs Cold Start ===");
    println!("Cold start: {:.2}s  exploitability={:.4}", cold_elapsed.as_secs_f64(), cold_exploit);
    println!("Warm start: {:.2}s  exploitability={:.4}", warm_elapsed.as_secs_f64(), warm_exploit);

    // Both should produce valid results
    assert!(cold_exploit.is_finite());
    assert!(warm_exploit.is_finite());
}
