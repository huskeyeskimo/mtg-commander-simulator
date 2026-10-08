//! Commander Goldfish MCCFR Training
//!
//! Trains an MCCFR strategy for a commander deck in goldfish (solitaire) mode,
//! then reports:
//!   - Per-turn kill probability distribution
//!   - Cumulative win-by-turn-X probabilities
//!   - Comparison against Greedy and Random baselines
//!   - A sample game trace showing the trained strategy's decisions
//!
//! Usage:
//!   cargo run --release --bin commander_goldfish
//!
//! Options (via environment variables):
//!   ITERATIONS=100    Number of MCCFR training iterations (default: 100)
//!   DEPTH=10          Max tree depth per traversal (default: 10)
//!   NODES=100000      Max nodes per iteration, 0=unlimited (default: 100000)
//!   GAMES=1000        Number of simulation games (default: 1000)
//!   DECK=kinnan       Deck to use: "kinnan", "brimaz", or "ashcoat" (default: kinnan)

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use mtg_gto::card::sample;
use mtg_gto::game::{CardDatabase, GameState};
use mtg_gto::info_set::BucketedAbstraction;
use mtg_gto::rules;
use mtg_gto::simulation::{
    run_commander_goldfish_game_verbose, simulate_commander_goldfish, GoldfishResults,
};
use mtg_gto::solver::mccfr::{self, collect_policy_snapshots, McfrConfig};
use mtg_gto::strategy::{AbstractedMcfrStrategy, GreedyStrategy, RandomStrategy};

fn main() {
    // Configure rayon thread pool with larger stack size (8 MB) for deep MCCFR
    // traversal. The default thread stack (512 KB on macOS, 2 MB on Linux) can
    // overflow at high DEPTH values because each recursive decision node keeps
    // a GameState clone on the stack.
    rayon::ThreadPoolBuilder::new()
        .stack_size(8 * 1024 * 1024)
        .build_global()
        .expect("Failed to configure rayon thread pool");

    let iterations: u32 = std::env::var("ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let max_depth: u32 = std::env::var("DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let max_nodes: u32 = std::env::var("NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let num_games: u64 = std::env::var("GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let deck_name = std::env::var("DECK").unwrap_or_else(|_| "kinnan".to_string());

    let db = sample::build_sample_db();
    let (deck, commander, tutor_targets) = match deck_name.as_str() {
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

    println!("Commander Goldfish MCCFR Training");
    println!("==================================");
    println!("Deck:       {} ({})", deck_name, commander_name);
    println!("Iterations: {}", iterations);
    println!("Depth:      {}", max_depth);
    println!("Node budget:{}", if max_nodes == 0 { "unlimited".to_string() } else { format!("{}", max_nodes) });
    println!("Sim games:  {}", num_games);
    println!();

    // Build card name lookup for readable output
    let card_names = build_card_names(&db, &deck, commander);

    // ── 1. Train MCCFR ──────────────────────────────────────────────────
    let num_shards = mccfr::default_num_shards();
    println!("Training ({} threads)...", num_shards);
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_commander_game(&mut state, &deck, &deck, commander, commander);
    rules::set_tutor_targets(&mut state, 0, &tutor_targets);
    rules::set_tutor_targets(&mut state, 1, &tutor_targets);

    let config = McfrConfig {
        max_depth,
        max_actions: 2000,
        max_nodes_per_iteration: max_nodes,
    };
    let abstraction = BucketedAbstraction;

    let t0 = Instant::now();
    let progress_counter = Arc::new(AtomicU32::new(0));
    let training_done = Arc::new(AtomicBool::new(false));

    // Spawn a background thread that prints progress every 2 seconds,
    // so the user sees updates even while a long iteration is in-flight.
    let printer_handle = {
        let progress = progress_counter.clone();
        let done = training_done.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                if done.load(Ordering::Relaxed) {
                    break;
                }
                let completed = progress.load(Ordering::Relaxed);
                let elapsed = t0.elapsed().as_secs_f64();
                let eta = if completed > 0 {
                    elapsed / completed as f64 * (iterations - completed) as f64
                } else {
                    0.0
                };
                eprintln!(
                    "  iter {}/{} | {:.1}s elapsed | ETA {:.0}s",
                    completed, iterations, elapsed, eta,
                );
            }
        })
    };

    let progress_for_callback = progress_counter.clone();
    let attempted_tables = mccfr::train_goldfish_parallel_with_progress(
        &state,
        iterations,
        num_shards,
        &config,
        &abstraction,
        0,
        None,
        None,
        |completed, _total, _progress| {
            progress_for_callback.store(completed, Ordering::Relaxed);
        },
    );
    training_done.store(true, Ordering::Relaxed);
    let train_time = t0.elapsed();
    let tables = match attempted_tables {
        Ok(tables) => tables,
        Err(reason) => {
            let _ = printer_handle.join();
            eprintln!("INVALID reason={}", reason.code());
            return;
        }
    };
    // Print final progress line
    eprintln!(
        "  iter {}/{} | {:.1}s elapsed",
        iterations, iterations, train_time.as_secs_f64(),
    );
    let _ = printer_handle.join();

    let stats = match mccfr::training_stats(&tables) {
        Ok(value) => value,
        Err(reason) => { eprintln!("INVALID reason={}", reason.code()); return; }
    };
    let exploit = match mccfr::approximate_exploitability(&tables) {
        Ok(value) => value,
        Err(reason) => { eprintln!("INVALID reason={}", reason.code()); return; }
    };

    println!("Training complete in {:.1}s", train_time.as_secs_f64());
    println!("  Info sets:      {}", stats.total_info_sets[0]);
    println!("  Total visits:   {}", stats.total_visits[0]);
    println!("  Exploitability: {:.6}", exploit);
    println!();

    // ── 2. Simulate all three strategies ─────────────────────────────────
    let mccfr_strat =
        AbstractedMcfrStrategy::new(tables[0].clone(), Box::new(BucketedAbstraction));

    println!("Simulating {} games per strategy...", num_games);
    let t0 = Instant::now();
    let random_results =
        simulate_commander_goldfish(&db, &deck, commander, &RandomStrategy, num_games);
    let greedy_results =
        simulate_commander_goldfish(&db, &deck, commander, &GreedyStrategy, num_games);
    let mccfr_results =
        simulate_commander_goldfish(&db, &deck, commander, &mccfr_strat, num_games);
    let sim_time = t0.elapsed();
    println!("Simulation complete in {:.1}s\n", sim_time.as_secs_f64());

    // ── 3. Strategy comparison ───────────────────────────────────────────
    println!("Strategy Comparison");
    println!("───────────────────");
    print_strategy_row("Random", &random_results);
    print_strategy_row("Greedy", &greedy_results);
    print_strategy_row("MCCFR", &mccfr_results);
    println!();

    // ── 4. Per-turn kill probability distribution ────────────────────────
    println!("Kill Turn Distribution (MCCFR)");
    println!("──────────────────────────────");
    print_kill_distribution(&mccfr_results);
    println!();

    // Side-by-side comparison
    println!("Kill Turn Distribution Comparison (cumulative share among wins)");
    println!("───────────────────────────────────────────────────────────────");
    print_distribution_comparison(&random_results, &greedy_results, &mccfr_results);
    println!();

    // ── 5. Sample game trace ─────────────────────────────────────────────
    println!("Sample Game Trace (MCCFR trained strategy)");
    println!("──────────────────────────────────────────");
    println!("(Actions logged to stderr)\n");

    let result = run_commander_goldfish_game_verbose(&db, &deck, commander, &mccfr_strat);
    println!(
        "Result: {} in {} turns ({} accepted actions, {} rejected proposals), final life: {}/{}",
        result.outcome,
        result.turns,
        result.actions_taken,
        result.rejected_actions,
        result.final_life[0],
        result.final_life[1],
    );
    println!();

    // ── 6. Policy snapshots at key decision points ───────────────────────
    println!("MCCFR Policy at Key Decision Points (pilot only)");
    println!("─────────────────────────────────────────────────");

    let mut replay_state = GameState::new_commander(2);
    replay_state.card_db = Some(Arc::new(db.clone()));
    rules::setup_commander_game(&mut replay_state, &deck, &deck, commander, commander);

    let snapshots = match collect_policy_snapshots(&replay_state, &tables, &abstraction, 60) {
        Ok(value) => value,
        Err(reason) => { eprintln!("INVALID reason={}", reason.code()); return; }
    };

    let mut shown = 0;
    for snap in &snapshots {
        // Only show pilot (P0) decisions
        if snap.player != 0 {
            continue;
        }
        shown += 1;
        println!(
            "Decision #{}: {} (visits: {})",
            shown, snap.state_description, snap.visit_count,
        );
        for (action_raw, prob) in &snap.action_distribution {
            if *prob > 0.01 {
                let action_pretty = resolve_card_names(action_raw, &card_names);
                println!("  {:5.1}%  {}", prob * 100.0, action_pretty);
            }
        }
        println!();
        if shown >= 20 {
            break;
        }
    }
}

/// Build a map of card_id -> card name for all cards in the deck.
fn build_card_names(db: &CardDatabase, deck: &[u64], commander: u64) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for &card_id in deck {
        if let Some(def) = db.get(card_id) {
            names.insert(format!("{}", card_id), def.name.clone());
        }
    }
    if let Some(def) = db.get(commander) {
        names.insert(format!("{}", commander), def.name.clone());
    }
    names
}

/// Replace `card_id: NNN` patterns in an action string with card names.
fn resolve_card_names(action: &str, names: &HashMap<String, String>) -> String {
    let mut result = action.to_string();
    // Sort keys by length descending so "306" is replaced before "3"
    let mut keys: Vec<&String> = names.keys().collect();
    keys.sort_by(|a, b| b.len().cmp(&a.len()));

    for id_str in keys {
        let name = &names[id_str];

        // Match card_id followed by the number and a non-digit delimiter
        for delim in [",", "}", ")"] {
            let pattern = format!("card_id: {}{}", id_str, delim);
            let replacement = format!("card_id: {} ({}){}", id_str, name, delim);
            result = result.replace(&pattern, &replacement);
        }

        // Match attacker_card_ids tuples: (NNN, 0)
        // Only match when preceded by ( or ", " and followed by ", "
        let attack_pattern = format!("({}, ", id_str);
        let attack_replacement = format!("({} [{}], ", id_str, name);
        result = result.replace(&attack_pattern, &attack_replacement);
    }
    result
}

fn print_strategy_row(name: &str, r: &GoldfishResults) {
    println!("{}", strategy_row(name, r));
}

fn strategy_row(name: &str, r: &GoldfishResults) -> String {
    let (avg, fastest, slowest) = if r.wins > 0 {
        (format!("T{:.2}", r.avg_kill_turn), format!("T{}", r.fastest_kill),
            format!("T{}", r.slowest_kill))
    } else {
        ("-".into(), "-".into(), "-".into())
    };
    format!(
        "  {:<8} attempted={} completed={} win={:>5.1}%  avg_kill={} fastest={} slowest={} loss={} draw={} censored={} stalled={} invalid={}",
        name, r.total_games, r.completed_games(), r.win_rate() * 100.0,
        avg, fastest, slowest, r.losses, r.draws, r.censored, r.stalled, r.invalid,
    )
}

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn mixed_strategy_row_discloses_incomplete_attempts() {
        let r = GoldfishResults { total_games: 3, wins: 1, losses: 0, draws: 0,
            censored: 0, stalled: 1, invalid: 1, avg_kill_turn: 2.0,
            fastest_kill: 2, slowest_kill: 2, avg_actions: 4.0,
            kill_turn_distribution: vec![0, 0, 1] };
        let row = strategy_row("Greedy", &r);
        assert!(row.contains("attempted=3 completed=1 win=100.0%"));
        assert!(row.contains("stalled=1 invalid=1"));
    }
}

fn print_kill_distribution(r: &GoldfishResults) {
    if r.wins == 0 {
        println!("  No wins recorded.");
        return;
    }
    let mut cumulative = 0u64;
    println!("  Turn │ Wins │  P(kill | win) │ P(by | win)");
    println!("  ─────┼──────┼──────────┼──────────");
    for (turn, &count) in r.kill_turn_distribution.iter().enumerate() {
        if count == 0 && cumulative == 0 {
            continue;
        }
        cumulative += count;
        if count > 0 || (cumulative > 0 && turn <= r.slowest_kill as usize) {
            let p_kill = r.kill_share(count);
            let p_cum = r.kill_share(cumulative);
            println!(
                "  T{:<3} │ {:>4} │  {:>5.1}%  │  {:>5.1}%",
                turn,
                count,
                p_kill * 100.0,
                p_cum * 100.0,
            );
        }
    }
}

fn print_distribution_comparison(
    random: &GoldfishResults,
    greedy: &GoldfishResults,
    mccfr: &GoldfishResults,
) {
    println!("  Turn │   Random   │   Greedy   │    MCCFR");
    println!("  ─────┼────────────┼────────────┼────────────");

    let max_turn = [
        random.slowest_kill,
        greedy.slowest_kill,
        mccfr.slowest_kill,
        20,
    ]
    .into_iter()
    .max()
    .unwrap_or(20) as usize;

    let mut cum_r = 0u64;
    let mut cum_g = 0u64;
    let mut cum_m = 0u64;

    for turn in 1..=max_turn {
        let rc = random.kill_turn_distribution.get(turn).copied().unwrap_or(0);
        let gc = greedy.kill_turn_distribution.get(turn).copied().unwrap_or(0);
        let mc = mccfr.kill_turn_distribution.get(turn).copied().unwrap_or(0);
        cum_r += rc;
        cum_g += gc;
        cum_m += mc;

        if cum_r == 0 && cum_g == 0 && cum_m == 0 {
            continue;
        }

        let pr = random.kill_share(cum_r) * 100.0;
        let pg = greedy.kill_share(cum_g) * 100.0;
        let pm = mccfr.kill_share(cum_m) * 100.0;

        println!(
            "  T{:<3} │  {:>5.1}%     │  {:>5.1}%     │  {:>5.1}%",
            turn, pr, pg, pm,
        );
    }
}
