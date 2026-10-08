use std::sync::Arc;

use mtg_gto::card::sample;
use mtg_gto::card::CardId;
use mtg_gto::game::{CardDatabase, GameState};
use mtg_gto::rules;
use mtg_gto::simulation::{simulate, simulate_goldfish, GoldfishResults};
use mtg_gto::solver::mccfr::{self, McfrConfig};
use mtg_gto::strategy::{GreedyStrategy, McfrStrategy, RandomStrategy};

/// Train goldfish MCCFR for a deck and compare against the Greedy baseline.
fn goldfish_mccfr_report(
    db: &CardDatabase,
    deck: &[CardId],
    deck_name: &str,
    config: &McfrConfig,
    iterations: u32,
    num_games: u64,
    greedy_baseline: &GoldfishResults,
) -> Result<(), mtg_gto::simulation::TerminationReason> {
    let num_shards = mccfr::default_num_shards();
    println!(
        "Training MCCFR for {} goldfish ({} iterations, {} threads)...",
        deck_name, iterations, num_shards
    );
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_game(&mut state, deck, deck);
    let tables = mccfr::train_goldfish_parallel(&state, iterations, num_shards, config)?;

    let stats = mccfr::training_stats(&tables)?;
    println!(
        "  Trained: {} info sets, {} visits",
        stats.total_info_sets[0], stats.total_visits[0],
    );

    let mccfr_strat = McfrStrategy::new(tables[0].clone());
    println!("\n{} (MCCFR) — Goldfish:", deck_name);
    let mccfr_results = simulate_goldfish(db, deck, &mccfr_strat, num_games);
    mccfr_results.display();

    println!(
        "\n  >> Greedy avg kill: T{:.2}  |  MCCFR avg kill: T{:.2}  |  delta: {:.2} turns",
        greedy_baseline.avg_kill_turn,
        mccfr_results.avg_kill_turn,
        greedy_baseline.avg_kill_turn - mccfr_results.avg_kill_turn,
    );

    Ok(())
}

fn main() {
    println!("MTG GTO Simulator");
    println!("==================\n");

    // Build card database
    let db = sample::build_sample_db();

    // Build decks
    let red_deck = sample::red_aggro_deck();
    let green_deck = sample::green_stompy_deck();

    // Run simulations
    let greedy = GreedyStrategy;
    let random = RandomStrategy;

    println!("Red Aggro (Greedy) vs Green Stompy (Greedy):");
    let results = simulate(&db, &red_deck, &green_deck, &greedy, &greedy, 1000);
    results.display();

    println!();

    println!("Red Aggro (Greedy) vs Green Stompy (Random):");
    let results = simulate(&db, &red_deck, &green_deck, &greedy, &random, 1000);
    results.display();

    println!();

    println!("Red Aggro (Random) vs Green Stompy (Greedy):");
    let results = simulate(&db, &red_deck, &green_deck, &random, &greedy, 1000);
    results.display();

    // --- Goldfish mode ---
    println!("\n==================");
    println!("Goldfish Mode");
    println!("==================\n");

    println!("Red Aggro (Greedy) — Goldfish:");
    let greedy_red = simulate_goldfish(&db, &red_deck, &greedy, 1000);
    greedy_red.display();

    println!();

    println!("Green Stompy (Greedy) — Goldfish:");
    let greedy_green = simulate_goldfish(&db, &green_deck, &greedy, 1000);
    greedy_green.display();

    // --- MCCFR Goldfish ---
    println!("\n==================");
    println!("MCCFR Goldfish Training");
    println!("==================\n");

    let config = McfrConfig { max_depth: 8, max_actions: 1000, max_nodes_per_iteration: 0 };

    if let Err(reason) = goldfish_mccfr_report(&db, &red_deck, "Red Aggro", &config, 50, 1000, &greedy_red) {
        eprintln!("INVALID reason={}", reason.code());
        return;
    }

    println!();

    if let Err(reason) = goldfish_mccfr_report(&db, &green_deck, "Green Stompy", &config, 50, 1000, &greedy_green) {
        eprintln!("INVALID reason={}", reason.code());
        return;
    }
}
