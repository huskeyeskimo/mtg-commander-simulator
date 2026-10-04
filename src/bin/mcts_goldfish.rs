//! MCTS Goldfish — Solitaire optimization using Monte Carlo Tree Search
//!
//! Runs MCTS at each decision point to find near-optimal play for goldfish
//! (solitaire) games. Unlike MCCFR which trains a policy offline, MCTS
//! does online planning: it builds a search tree from the current position
//! each time it needs to make a decision.
//!
//! This is appropriate for single-agent optimization where the goal is to
//! minimize kill turn, rather than finding Nash equilibria for adversarial
//! play.
//!
//! Usage:
//!   cargo run --release --bin mcts_goldfish
//!
//! Options (via environment variables):
//!   ITERATIONS=500    MCTS iterations per decision (default: 500)
//!   EXPLORE=1.0       UCB1 exploration constant (default: 1.0)
//!   DEPTH=0           Max tree depth, 0=unlimited (default: 0)
//!   THREADS=1         Threads per MCTS decision, root parallelization (default: 1)
//!   GAMES=100         Number of games to simulate (default: 100)
//!   DECK=red          Deck: "red", "green", "kinnan", "brimaz", "ashcoat" (default: red)
//!   FORMAT=standard   Format: "standard" or "commander" (default: auto-detect)
//!   CHECKPOINT=path   Save/resume results to/from a JSON checkpoint file (optional)
//!   CHECKPOINT_DIR=   Directory for checkpoint save/resume (default: none)
//!   CHECKPOINT_EVERY= Save checkpoint every N games (default: 10)

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use mtg_gto::card::sample;
use mtg_gto::combo_discovery::{discover_and_register, DiscoveryConfig};
use mtg_gto::game::{CardDatabase, GameState};
use mtg_gto::rules;
use mtg_gto::simulation::{
    simulate_goldfish, simulate_commander_goldfish, GoldfishResults,
    simulate_mcts_goldfish_with_progress, simulate_mcts_commander_goldfish_with_progress,
    run_mcts_goldfish_game, run_mcts_commander_goldfish_game,
};
use mtg_gto::solver::mcts::{self, MctsConfig, MctsCampaignCheckpoint, MctsGoldfishResults};
use mtg_gto::strategy::GreedyStrategy;

fn main() {
    // Configure rayon thread pool with larger stack for deep MCTS rollouts
    rayon::ThreadPoolBuilder::new()
        .stack_size(8 * 1024 * 1024)
        .build_global()
        .expect("Failed to configure rayon thread pool");

    let iterations: u32 = std::env::var("ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    let explore: f64 = std::env::var("EXPLORE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    let max_depth: u32 = std::env::var("DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let num_games: u64 = std::env::var("GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let num_threads: u32 = std::env::var("THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let deck_name = std::env::var("DECK").unwrap_or_else(|_| "red".to_string());
    let checkpoint_path = std::env::var("CHECKPOINT").ok();
    let checkpoint_dir = std::env::var("CHECKPOINT_DIR").ok();
    let checkpoint_every: u64 = std::env::var("CHECKPOINT_EVERY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let db = sample::build_sample_db();

    let config = MctsConfig {
        iterations_per_move: iterations,
        exploration_constant: explore,
        max_tree_depth: max_depth,
        max_rollout_actions: 5_000,
        num_threads,
    };

    // Determine deck and format
    let is_commander = matches!(deck_name.as_str(), "kinnan" | "brimaz" | "ashcoat");

    println!("MCTS Goldfish Optimizer");
    println!("=======================");
    println!("Deck:       {}", deck_name);
    println!("Format:     {}", if is_commander { "Commander" } else { "Standard" });
    println!("MCTS iters: {} per decision", iterations);
    println!("Explore C:  {:.2}", explore);
    println!("Tree depth: {}", if max_depth == 0 { "unlimited".to_string() } else { format!("{}", max_depth) });
    println!("Threads:    {}{}", num_threads.max(1), if num_threads <= 1 { " (single-threaded)" } else { " (root parallelization)" });
    println!("Games:      {}", num_games);
    if let Some(ref cp) = checkpoint_path {
        println!("Checkpoint: {}", cp);
    }
    if let Some(ref dir) = checkpoint_dir {
        println!("Checkpoint dir: {} (every {} games)", dir, checkpoint_every);
    }
    println!();

    if let Some(ref dir) = checkpoint_dir {
        // ── Campaign checkpoint mode: run with save/resume ───────────────
        if is_commander {
            run_commander_checkpoint(&db, &deck_name, &config, num_games, dir, checkpoint_every);
        } else {
            run_standard_checkpoint(&db, &deck_name, &config, num_games, dir, checkpoint_every);
        }
    } else {
        // ── Standard mode (with optional simple checkpoint) ──────────────
        if is_commander {
            run_commander_goldfish(&db, &deck_name, &config, num_games, checkpoint_path.as_deref());
        } else {
            run_standard_goldfish(&db, &deck_name, &config, num_games, checkpoint_path.as_deref());
        }
    }
}

// ---------------------------------------------------------------------------
// Checkpoint mode: standard format
// ---------------------------------------------------------------------------

fn run_standard_checkpoint(
    db: &CardDatabase,
    deck_name: &str,
    config: &MctsConfig,
    num_games: u64,
    checkpoint_dir: &str,
    checkpoint_every: u64,
) {
    let deck = match deck_name {
        "green" => sample::green_stompy_deck(),
        _ => sample::red_aggro_deck(),
    };

    // Try to resume from existing checkpoint
    let mut checkpoint = match MctsCampaignCheckpoint::load(checkpoint_dir) {
        Ok(cp) => {
            println!(
                "Resumed from checkpoint: {}/{} attempts recorded ({:.1}% wins/completed game)",
                cp.games_completed,
                cp.total_games_planned,
                cp.results.win_rate() * 100.0,
            );
            // Allow extending: if user asks for more games than originally planned
            let mut cp = cp;
            if num_games > cp.total_games_planned {
                cp.total_games_planned = num_games;
            }
            cp
        }
        Err(_) => {
            println!("Starting fresh campaign ({} games)", num_games);
            MctsCampaignCheckpoint::new(config.clone(), deck_name.to_string(), num_games)
        }
    };

    let remaining = checkpoint.games_remaining();
    if remaining == 0 {
        println!("Campaign already complete!");
        display_mcts_results(&checkpoint.results);
        return;
    }

    println!("Running {} remaining games...\n", remaining);
    let t0 = Instant::now();

    run_campaign_batched(
        &mut checkpoint,
        remaining,
        checkpoint_every,
        checkpoint_dir,
        |_game_idx| {
            let db_arc = Arc::new(db.clone());
            let mut state = GameState::new(2);
            state.card_db = Some(db_arc);
            rules::setup_game(&mut state, &deck, &deck);
            mcts::run_mcts_goldfish_game(&mut state, config, false, None)
        },
    );

    let elapsed = t0.elapsed();
    println!("\nCompleted in {:.1}s ({:.2} games/sec)\n", elapsed.as_secs_f64(), remaining as f64 / elapsed.as_secs_f64());
    display_mcts_results(&checkpoint.results);
}

// ---------------------------------------------------------------------------
// Checkpoint mode: commander format
// ---------------------------------------------------------------------------

fn run_commander_checkpoint(
    db: &CardDatabase,
    deck_name: &str,
    config: &MctsConfig,
    num_games: u64,
    checkpoint_dir: &str,
    checkpoint_every: u64,
) {
    let (deck, commander, tutor_targets) = match deck_name {
        "brimaz" => {
            let (d, c) = sample::brimaz_commander_deck();
            (d, c, Vec::new())
        }
        "ashcoat" => {
            let (d, c) = sample::ashcoat_commander_deck();
            (d, c, Vec::new())
        }
        "flubs" => {
            let (d, c) = sample::flubs_commander_deck();
            (d, c, Vec::new())
        }
        _ => sample::kinnan_commander_deck(),
    };

    let commander_name = db.get(commander).map(|d| d.name.as_str()).unwrap_or("?");
    println!("Commander: {}\n", commander_name);

    // Discover combos using commander + tutor targets (known combo pieces).
    // Using the full deck finds hundreds of spurious combos.
    let mut combo_cards = vec![commander];
    combo_cards.extend_from_slice(&tutor_targets);
    let (mut registry, discovered) = discover_and_register(db, &combo_cards, &DiscoveryConfig::default());
    if !discovered.is_empty() {
        println!("Discovered {} combo(s) for macro-actions", discovered.len());
    }
    // Register manual win combos (e.g., infinite mana + Walking Ballista)
    mtg_gto::combo::register_ballista_win_combo(&mut registry);
    println!("Total registered combos: {}", registry.combos.len());
    let combo_reg = Arc::new(registry);

    // Try to resume from existing checkpoint
    let mut checkpoint = match MctsCampaignCheckpoint::load(checkpoint_dir) {
        Ok(cp) => {
            println!(
                "Resumed from checkpoint: {}/{} attempts recorded ({:.1}% wins/completed game)",
                cp.games_completed,
                cp.total_games_planned,
                cp.results.win_rate() * 100.0,
            );
            let mut cp = cp;
            if num_games > cp.total_games_planned {
                cp.total_games_planned = num_games;
            }
            cp
        }
        Err(_) => {
            println!("Starting fresh campaign ({} games)", num_games);
            MctsCampaignCheckpoint::new(config.clone(), deck_name.to_string(), num_games)
        }
    };

    let remaining = checkpoint.games_remaining();
    if remaining == 0 {
        println!("Campaign already complete!");
        display_mcts_results(&checkpoint.results);
        return;
    }

    println!("Running {} remaining games...\n", remaining);
    let t0 = Instant::now();

    run_campaign_batched(
        &mut checkpoint,
        remaining,
        checkpoint_every,
        checkpoint_dir,
        |_game_idx| {
            let db_arc = Arc::new(db.clone());
            let mut state = GameState::new_commander(2);
            state.card_db = Some(db_arc);
            rules::setup_commander_game(&mut state, &deck, &deck, commander, commander);
            rules::set_tutor_targets(&mut state, 0, &tutor_targets);
            state.combo_registry = Some(Arc::clone(&combo_reg));
            mcts::run_mcts_goldfish_game(&mut state, config, false, None)
        },
    );

    let elapsed = t0.elapsed();
    println!("\nCompleted in {:.1}s ({:.2} games/sec)\n", elapsed.as_secs_f64(), remaining as f64 / elapsed.as_secs_f64());
    display_mcts_results(&checkpoint.results);
}

// ---------------------------------------------------------------------------
// Shared campaign runner with batched checkpointing
// ---------------------------------------------------------------------------

/// Run games in parallel batches, checkpointing after each batch.
///
/// Games within a batch run in parallel (via rayon). Between batches,
/// results are folded into the checkpoint and saved to disk.
fn run_campaign_batched(
    checkpoint: &mut MctsCampaignCheckpoint,
    total_remaining: u64,
    batch_size: u64,
    checkpoint_dir: &str,
    run_game: impl Fn(u64) -> mcts::MctsGameResult + Send + Sync,
) {
    let mut done = 0u64;
    while done < total_remaining {
        let batch = batch_size.min(total_remaining - done);
        let batch_start = checkpoint.games_completed;

        // Run batch in parallel
        let results: Vec<mcts::MctsGameResult> = (0..batch)
            .into_par_iter()
            .map(|i| run_game(batch_start + i))
            .collect();

        // Fold results into checkpoint (sequential — cheap)
        for result in results {
            checkpoint.add_game(result);
        }
        done += batch;

        // Save checkpoint
        if let Err(e) = checkpoint.save(checkpoint_dir) {
            eprintln!("Warning: failed to save checkpoint: {}", e);
        } else {
            println!(
                "  [{}/{}] saved checkpoint ({:.1}% win/completed, avg kill {})",
                checkpoint.games_completed,
                checkpoint.total_games_planned,
                checkpoint.results.win_rate() * 100.0,
                checkpoint.results.avg_kill_turn.map_or_else(|| "unavailable".into(), |v| format!("T{v:.2}")),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Non-checkpoint mode (original behavior)
// ---------------------------------------------------------------------------

fn run_standard_goldfish(
    db: &CardDatabase,
    deck_name: &str,
    config: &MctsConfig,
    num_games: u64,
    checkpoint_path: Option<&str>,
) {
    let deck = match deck_name {
        "green" => sample::green_stompy_deck(),
        _ => sample::red_aggro_deck(),
    };

    // ── 1. Greedy baseline ─────────────────────────────────────────────
    println!("Running Greedy baseline ({} games)...", num_games);
    let t0 = Instant::now();
    let greedy_results = simulate_goldfish(db, &deck, &GreedyStrategy, num_games);
    let greedy_time = t0.elapsed();
    println!("Greedy baseline complete in {:.1}s\n", greedy_time.as_secs_f64());

    // ── 2. MCTS simulation ─────────────────────────────────────────────
    println!("Running MCTS goldfish ({} games, {} iters/decision)...", num_games, config.iterations_per_move);
    let t0 = Instant::now();
    let progress = Arc::new(AtomicU64::new(0));
    let decisions = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let printer = spawn_progress_thread(progress.clone(), decisions.clone(), done.clone(), num_games, t0);
    let mcts_results = simulate_mcts_goldfish_with_progress(db, &deck, config, num_games, &progress, &decisions);
    done.store(true, Ordering::Relaxed);
    let mcts_time = t0.elapsed();
    eprintln!(
        "  game {}/{} | {} decisions | {:.1}s elapsed",
        num_games, num_games, decisions.load(Ordering::Relaxed), mcts_time.as_secs_f64(),
    );
    let _ = printer.join();
    println!("MCTS simulation complete in {:.1}s\n", mcts_time.as_secs_f64());

    // ── 2b. Checkpoint: load previous + merge + save ──────────────────
    let mcts_results = merge_and_save_checkpoint(mcts_results, checkpoint_path);

    // ── 3. Comparison ──────────────────────────────────────────────────
    println!("Strategy Comparison");
    println!("───────────────────");
    print_strategy_row("Greedy", &greedy_results);
    print_mcts_strategy_row("MCTS", &mcts_results);
    println!();

    print_improvement_summary(&greedy_results, &mcts_results);
    println!();

    // ── 4. Kill turn distribution comparison ───────────────────────────
    println!("Kill Turn Distribution (cumulative share among wins)");
    println!("────────────────────────────────────────────────────");
    print_distribution_comparison(&greedy_results, &mcts_results);
    println!();

    // ── 5. Fastest win sequence ──────────────────────────────────────
    print_fastest_sequence(&mcts_results, config);

    // ── 6. Sample game trace ───────────────────────────────────────────
    let result = run_mcts_goldfish_game(db, &deck, config, true);

    println!("Sample Game Trace (MCTS)");
    println!("────────────────────────");
    for line in &result.trace_lines {
        println!("{}", line);
    }
    println!(
        "Result: {} on T{} ({} actions), life: {}/{}",
        format!("{:?}", result.outcome),
        result.kill_turn,
        result.actions_taken,
        result.final_life[0],
        result.final_life[1],
    );
    if !result.decision_stats.is_empty() {
        println!("\nKey decisions ({} total):", result.decision_stats.len());
        for (i, stat) in result.decision_stats.iter().enumerate().take(30) {
            if !stat.pilot_hand.is_empty() {
                println!("        Hand: [{}]", stat.pilot_hand.join(", "));
            }
            println!(
                "  #{:<3} T{} {:?}: {} (of {} options, {}/{} visits, Q={:.3})",
                i + 1,
                stat.turn,
                stat.phase,
                stat.action_description,
                stat.num_legal_actions,
                stat.best_action_visits,
                config.iterations_per_move,
                stat.best_action_avg_reward,
            );
        }
    }
}

fn run_commander_goldfish(
    db: &CardDatabase,
    deck_name: &str,
    config: &MctsConfig,
    num_games: u64,
    checkpoint_path: Option<&str>,
) {
    let (deck, commander, tutor_targets) = match deck_name {
        "brimaz" => {
            let (d, c) = sample::brimaz_commander_deck();
            (d, c, Vec::new())
        }
        "ashcoat" => {
            let (d, c) = sample::ashcoat_commander_deck();
            (d, c, Vec::new())
        }
        "flubs" => {
            let (d, c) = sample::flubs_commander_deck();
            (d, c, Vec::new())
        }
        _ => sample::kinnan_commander_deck(),
    };

    let commander_name = db.get(commander).map(|d| d.name.as_str()).unwrap_or("?");
    println!("Commander: {}\n", commander_name);

    // Discover combos using commander + tutor targets (known combo pieces)
    let mut combo_cards = vec![commander];
    combo_cards.extend_from_slice(&tutor_targets);
    let (mut registry, discovered) = discover_and_register(db, &combo_cards, &DiscoveryConfig::default());
    if !discovered.is_empty() {
        println!("Discovered {} combo(s) for macro-actions", discovered.len());
    }
    mtg_gto::combo::register_ballista_win_combo(&mut registry);
    println!("Total registered combos: {}\n", registry.combos.len());

    // ── 1. Greedy baseline ─────────────────────────────────────────────
    println!("Running Greedy baseline ({} games)...", num_games);
    let t0 = Instant::now();
    let greedy_results = simulate_commander_goldfish(db, &deck, commander, &GreedyStrategy, num_games);
    let greedy_time = t0.elapsed();
    println!("Greedy baseline complete in {:.1}s\n", greedy_time.as_secs_f64());

    // ── 2. MCTS simulation ─────────────────────────────────────────────
    println!("Running MCTS goldfish ({} games, {} iters/decision)...", num_games, config.iterations_per_move);
    let t0 = Instant::now();
    let progress = Arc::new(AtomicU64::new(0));
    let decisions = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let printer = spawn_progress_thread(progress.clone(), decisions.clone(), done.clone(), num_games, t0);
    let mcts_results = simulate_mcts_commander_goldfish_with_progress(db, &deck, commander, config, num_games, &progress, &decisions, &tutor_targets);
    done.store(true, Ordering::Relaxed);
    let mcts_time = t0.elapsed();
    eprintln!(
        "  game {}/{} | {} decisions | {:.1}s elapsed",
        num_games, num_games, decisions.load(Ordering::Relaxed), mcts_time.as_secs_f64(),
    );
    let _ = printer.join();
    println!("MCTS simulation complete in {:.1}s\n", mcts_time.as_secs_f64());

    // ── 2b. Checkpoint: load previous + merge + save ──────────────────
    let mcts_results = merge_and_save_checkpoint(mcts_results, checkpoint_path);

    // ── 3. Comparison ──────────────────────────────────────────────────
    println!("Strategy Comparison");
    println!("───────────────────");
    print_strategy_row("Greedy", &greedy_results);
    print_mcts_strategy_row("MCTS", &mcts_results);
    println!();

    print_improvement_summary(&greedy_results, &mcts_results);
    println!();

    // ── 4. Kill turn distribution comparison ───────────────────────────
    println!("Kill Turn Distribution (cumulative share among wins)");
    println!("────────────────────────────────────────────────────");
    print_distribution_comparison(&greedy_results, &mcts_results);
    println!();

    // ── 5. Fastest win sequence ──────────────────────────────────────
    print_fastest_sequence(&mcts_results, config);

    // ── 6. Sample game trace ───────────────────────────────────────────
    let result = run_mcts_commander_goldfish_game(db, &deck, commander, config, true, &tutor_targets);

    println!("Sample Game Trace (MCTS)");
    println!("────────────────────────");
    for line in &result.trace_lines {
        println!("{}", line);
    }
    println!(
        "Result: {} on T{} ({} actions), life: {}/{}",
        format!("{:?}", result.outcome),
        result.kill_turn,
        result.actions_taken,
        result.final_life[0],
        result.final_life[1],
    );
    if !result.decision_stats.is_empty() {
        println!("\nKey decisions ({} total):", result.decision_stats.len());
        for (i, stat) in result.decision_stats.iter().enumerate().take(30) {
            if !stat.pilot_hand.is_empty() {
                println!("        Hand: [{}]", stat.pilot_hand.join(", "));
            }
            println!(
                "  #{:<3} T{} {:?}: {} (of {} options, {}/{} visits, Q={:.3})",
                i + 1,
                stat.turn,
                stat.phase,
                stat.action_description,
                stat.num_legal_actions,
                stat.best_action_visits,
                config.iterations_per_move,
                stat.best_action_avg_reward,
            );
        }
    }
}

fn print_fastest_sequence(
    r: &mtg_gto::solver::mcts::MctsGoldfishResults,
    config: &MctsConfig,
) {
    if r.fastest_sequence.is_empty() {
        return;
    }
    println!("Fastest Win Sequence (T{} kill)", r.fastest_kill);
    println!("─────────────────────────────────");
    for (i, stat) in r.fastest_sequence.iter().enumerate() {
        if !stat.pilot_hand.is_empty() {
            println!("        Hand: [{}]", stat.pilot_hand.join(", "));
        }
        println!(
            "  #{:<3} T{} {:?}: {} (of {} options, {}/{} visits, Q={:.3})",
            i + 1,
            stat.turn,
            stat.phase,
            stat.action_description,
            stat.num_legal_actions,
            stat.best_action_visits,
            config.iterations_per_move,
            stat.best_action_avg_reward,
        );
    }
    println!();
}

fn display_mcts_results(r: &MctsGoldfishResults) {
    println!("=== MCTS Goldfish Results ===");
    println!("Attempts recorded: {}", r.total_games);
    println!("Completed: {}", r.completed_games());
    println!("Wins: {} ({:.1}% of completed)", r.wins, r.win_rate() * 100.0);
    println!("Losses: {}, rules draws: {}", r.losses, r.draws);
    println!("Censored: {}, stalled: {}, invalid: {}, legacy unknown: {}",
        r.censored, r.stalled, r.invalid, r.legacy_unknown);
    if r.wins > 0 {
        println!("Avg kill turn among wins: {} ({} measured)", r.avg_kill_turn.map_or_else(|| "unavailable".into(), |v| format!("{v:.2}")), r.kill_turn_samples);
        println!("Fastest kill: T{}", r.fastest_kill);
        println!("Slowest kill: T{}", r.slowest_kill);
        println!("Kill turn distribution among wins:");
        for (turn, &count) in r.kill_turn_distribution.iter().enumerate() {
            if count > 0 {
                println!("  T{}: {} ({:.1}%)", turn, count,
                    count as f64 / r.wins as f64 * 100.0);
            }
        }
        if !r.fastest_sequence.is_empty() {
            println!("\nFastest observed win (T{}) sequence:", r.fastest_kill);
            for (i, stat) in r.fastest_sequence.iter().enumerate() {
                if !stat.pilot_hand.is_empty() {
                    println!("        Hand: [{}]", stat.pilot_hand.join(", "));
                }
                println!("  #{:<3} T{} {:?}: {} (of {} options, Q={:.3})",
                    i + 1, stat.turn, stat.phase, stat.action_description,
                    stat.num_legal_actions, stat.best_action_avg_reward);
            }
        }
    }
    println!("Avg actions/measured game: {} ({} measured)", r.avg_actions.map_or_else(|| "unavailable".into(), |v| format!("{v:.1}")), r.actions_samples);
    println!("Avg decisions/measured game: {} ({} measured)", r.avg_decisions_per_game.map_or_else(|| "unavailable".into(), |v| format!("{v:.1}")), r.decision_samples);
    println!("Avg best-action reward/measured game: {} ({} measured)", r.avg_best_reward.map_or_else(|| "unavailable".into(), |v| format!("{v:.4}")), r.reward_samples);
}

fn print_strategy_row(name: &str, r: &GoldfishResults) {
    if r.wins > 0 {
        println!(
            "  {:<8} attempted={} completed={} win={:>5.1}%  avg_kill=T{:<5.2}  fastest=T{:<3}  slowest=T{:<3}  loss={} draw={} censored={} stalled={} invalid={}",
            name,
            r.total_games,
            r.completed_games(),
            r.win_rate() * 100.0,
            r.avg_kill_turn,
            r.fastest_kill,
            r.slowest_kill,
            r.losses,
            r.draws,
            r.censored,
            r.stalled,
            r.invalid,
        );
    } else {
        println!(
            "  {:<8} attempted={} completed={} win={:>5.1}%  avg_kill={:<6}  fastest={:<4}  slowest={:<4}  loss={} draw={} censored={} stalled={} invalid={}",
            name,
            r.total_games,
            r.completed_games(),
            r.win_rate() * 100.0,
            "-",
            "-",
            "-",
            r.losses,
            r.draws,
            r.censored,
            r.stalled,
            r.invalid,
        );
    }
}

fn print_mcts_strategy_row(
    name: &str,
    r: &mtg_gto::solver::mcts::MctsGoldfishResults,
) {
    if r.wins > 0 {
        println!(
            "  {:<8} attempted={} completed={} win={:>5.1}%  avg_kill={} (n={})  fastest=T{:<3}  slowest=T{:<3}  loss={} draw={} censored={} stalled={} invalid={} legacy_unknown={}",
            name,
            r.total_games,
            r.completed_games(),
            r.win_rate() * 100.0,
            r.avg_kill_turn.map_or_else(|| "unavailable".into(), |v| format!("T{v:.2}")),
            r.kill_turn_samples,
            r.fastest_kill,
            r.slowest_kill,
            r.losses,
            r.draws,
            r.censored,
            r.stalled,
            r.invalid,
            r.legacy_unknown,
        );
    } else {
        println!(
            "  {:<8} attempted={} completed={} win={:>5.1}%  avg_kill={:<6}  fastest={:<4}  slowest={:<4}  loss={} draw={} censored={} stalled={} invalid={} legacy_unknown={}",
            name,
            r.total_games,
            r.completed_games(),
            r.win_rate() * 100.0,
            "-",
            "-",
            "-",
            r.losses,
            r.draws,
            r.censored,
            r.stalled,
            r.invalid,
            r.legacy_unknown,
        );
    }
}

fn print_improvement_summary(
    greedy: &GoldfishResults,
    mcts: &mtg_gto::solver::mcts::MctsGoldfishResults,
) {
    if greedy.wins > 0 && mcts.wins > 0 && mcts.avg_kill_turn.is_some() {
        let mcts_kill_turn = mcts.avg_kill_turn.unwrap();
        let delta = greedy.avg_kill_turn - mcts_kill_turn;
        println!(
            "MCTS improves by {:.2} turns on average (Greedy T{:.2} → MCTS T{:.2})",
            delta, greedy.avg_kill_turn, mcts_kill_turn,
        );
    } else if greedy.wins == 0 && mcts.wins > 0 {
        println!(
            "Greedy never won; MCTS wins/completed game {:.1}% with avg kill {}",
            mcts.win_rate() * 100.0,
            mcts.avg_kill_turn.map_or_else(|| "unavailable".into(), |v| format!("T{v:.2}")),
        );
    } else if greedy.wins > 0 && mcts.wins == 0 {
        println!(
            "MCTS never won; Greedy wins/completed game {:.1}% with avg kill T{:.2}",
            greedy.win_rate() * 100.0,
            greedy.avg_kill_turn,
        );
    } else if greedy.wins > 0 && mcts.wins > 0 {
        println!("Kill-turn comparison unavailable: MCTS kill-turn mean has no measured samples");
    } else {
        println!("Neither strategy won any games");
    }
}

fn print_distribution_comparison(
    greedy: &GoldfishResults,
    mcts: &mtg_gto::solver::mcts::MctsGoldfishResults,
) {
    println!("  Turn │   Greedy   │    MCTS");
    println!("  ─────┼────────────┼────────────");

    let max_turn = [greedy.slowest_kill, mcts.slowest_kill, 20]
        .into_iter()
        .max()
        .unwrap_or(20) as usize;

    let mut cum_g = 0u64;
    let mut cum_m = 0u64;

    for turn in 1..=max_turn {
        let gc = greedy.kill_turn_distribution.get(turn).copied().unwrap_or(0);
        let mc = mcts.kill_turn_distribution.get(turn).copied().unwrap_or(0);
        cum_g += gc;
        cum_m += mc;

        if cum_g == 0 && cum_m == 0 {
            continue;
        }

        let pg = greedy.kill_share(cum_g) * 100.0;
        let pm = if mcts.wins == 0 { 0.0 } else { cum_m as f64 / mcts.wins as f64 * 100.0 };

        println!(
            "  T{:<3} │  {:>5.1}%     │  {:>5.1}%",
            turn, pg, pm,
        );
    }
}

/// Load a previous checkpoint (if it exists), merge new results into it,
/// save the combined results back, and return them. If no checkpoint path
/// is given, the new results are returned as-is.
fn merge_and_save_checkpoint(
    new_results: MctsGoldfishResults,
    checkpoint_path: Option<&str>,
) -> MctsGoldfishResults {
    let Some(cp) = checkpoint_path else {
        return new_results;
    };
    let path = std::path::Path::new(cp);

    // Try to load a previous checkpoint.
    let merged = if path.exists() {
        match MctsGoldfishResults::load_checkpoint(path) {
            Ok(previous) => {
                let merged = previous.merge(&new_results);
                println!(
                    "Checkpoint: merged {} new games with {} previous → {} total",
                    new_results.total_games, previous.total_games, merged.total_games,
                );
                merged
            }
            Err(e) => {
                eprintln!("Warning: failed to load checkpoint {}: {}", path.display(), e);
                eprintln!("Starting fresh checkpoint with current results.");
                new_results
            }
        }
    } else {
        println!(
            "Checkpoint: saving {} games (new checkpoint)",
            new_results.total_games,
        );
        new_results
    };

    // Save the (possibly merged) results.
    if let Err(e) = merged.save_checkpoint(path) {
        eprintln!("Warning: failed to save checkpoint {}: {}", path.display(), e);
    }
    println!();

    merged
}

/// Spawn a background thread that prints MCTS game-level progress every 2 seconds.
///
/// Returns the thread handle so the caller can join after the simulation completes.
fn spawn_progress_thread(
    progress: Arc<AtomicU64>,
    decisions: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
    total: u64,
    start: Instant,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(2));
            if done.load(Ordering::Relaxed) {
                break;
            }
            let completed = progress.load(Ordering::Relaxed);
            let total_decisions = decisions.load(Ordering::Relaxed);
            let elapsed = start.elapsed().as_secs_f64();
            let eta_str = if completed > 0 {
                let eta = elapsed / completed as f64 * (total - completed) as f64;
                format!("{:.0}s", eta)
            } else {
                "?".to_string()
            };
            eprintln!(
                "  game {}/{} | {} decisions | {:.1}s elapsed | ETA {}",
                completed, total, total_decisions, elapsed, eta_str,
            );
        }
    })
}
