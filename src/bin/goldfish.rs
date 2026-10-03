//! Unified Commander Goldfish Simulator
//!
//! The primary entry point for the MTG rules engine. Loads a Commander deck,
//! runs goldfish (solitaire) simulations, and reports results.
//!
//! Usage:
//!   cargo run --release --bin goldfish
//!   cargo run --release --bin goldfish -- --deck decks/kinnan.txt
//!   cargo run --release --bin goldfish -- --preset kinnan --games 500
//!   cargo run --release --bin goldfish -- --deck decks/custom.txt --verbose
//!   cargo run --release --bin goldfish -- --preset kinnan --trace
//!
//! Options:
//!   --deck PATH       Resolve a Commander deck from an offline file/catalog
//!   --preset NAME     Use a built-in or saved deck (default: kinnan)
//!   --games N         Number of simulation games (default: 1000)
//!   --strategy S      Strategy: greedy, random (default: greedy)
//!   --trace           Print a single game trace with per-turn actions
//!   --verbose         Print actions during simulation
//!   --coverage        Inspect support/readiness and exit

use std::sync::Arc;

use mtg_gto::card::catalog;
use mtg_gto::card::sample;
use mtg_gto::card::CardId;
use mtg_gto::game::{CardDatabase, GameState};
use mtg_gto::rules;
use mtg_gto::simulation::{
    run_commander_goldfish_game_verbose, simulate_commander_goldfish, GoldfishResults,
};
use mtg_gto::strategy::{GreedyStrategy, RandomStrategy, Strategy};
use mtg_gto::deck_loader::{self, ResolvedDeck};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let deck_path = get_arg(&args, "--deck");
    let preset = get_arg(&args, "--preset").unwrap_or_else(|| "kinnan".to_string());
    let num_games: u64 = get_arg(&args, "--games")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let strategy_name = get_arg(&args, "--strategy").unwrap_or_else(|| "greedy".to_string());
    let trace_mode = args.contains(&"--trace".to_string());
    let verbose = args.contains(&"--verbose".to_string());
    let coverage_only = args.contains(&"--coverage".to_string());

    // Load deck
    let resolved = match deck_path {
        Some(path) => deck_loader::resolve_file(std::path::Path::new(&path)),
        None => deck_loader::resolve_preset(&preset),
    }.unwrap_or_else(|e| { eprintln!("Error loading deck: {e}"); std::process::exit(1) });
    let deck_name = resolved.decklist.name.clone();
    let ResolvedDeck { db, setup_cards: deck, commander, tutor_targets, readiness_reasons, .. } = resolved;

    let commander_name = db.get(commander).map(|d| d.name.as_str()).unwrap_or("?");

    println!("Commander Goldfish Simulator");
    println!("============================");
    println!("Deck:       {} ({})", deck_name, commander_name);
    println!("Cards:      {}", deck.len());
    println!("Strategy:   {}", strategy_name);
    println!();

    // Card coverage report
    if !readiness_reasons.is_empty() {
        println!("Benchmark readiness: BLOCKED ({} reasons)", readiness_reasons.len());
        if coverage_only { for reason in &readiness_reasons { println!("  {reason}"); } }
    } else {
        print_coverage_report(&db, &deck, commander, coverage_only);
    }

    if coverage_only { return; }
    if !readiness_reasons.is_empty() {
        eprintln!("Goblin Storm benchmark execution blocked: {} uncertified requirements. Use --coverage for reasons.", readiness_reasons.len());
        std::process::exit(2);
    }

    println!();

    // Create strategy
    let strategy: Box<dyn Strategy + Send + Sync> = match strategy_name.as_str() {
        "random" => Box::new(RandomStrategy),
        _ => Box::new(GreedyStrategy),
    };

    // Single trace mode
    if trace_mode {
        println!("Game Trace");
        println!("----------");
        let result = run_commander_goldfish_game_verbose(&db, &deck, commander, strategy.as_ref());
        println!(
            "\nResult: {} in {} turns ({} actions)",
            match result.winner {
                Some(0) => "WIN",
                Some(_) => "LOSS",
                None => "DRAW",
            },
            result.turns,
            result.actions_taken,
        );
        println!(
            "Final life: {} / {}",
            result.final_life[0], result.final_life[1]
        );
        return;
    }

    // Setup tutor targets for simulation
    let mut test_state = GameState::new_commander(2);
    test_state.card_db = Some(Arc::new(db.clone()));
    rules::setup_commander_game(&mut test_state, &deck, &deck, commander, commander);
    rules::set_tutor_targets(&mut test_state, 0, &tutor_targets);

    // Run simulations
    println!("Simulating {} games...", num_games);
    let t0 = std::time::Instant::now();
    let results = simulate_commander_goldfish(&db, &deck, commander, strategy.as_ref(), num_games);
    let elapsed = t0.elapsed();
    println!("Done in {:.1}s\n", elapsed.as_secs_f64());

    // Print results
    print_results(&results);

    // If verbose, run a single trace game too
    if verbose {
        println!("\nSample Game Trace");
        println!("-----------------");
        let _result = run_commander_goldfish_game_verbose(&db, &deck, commander, strategy.as_ref());
    }
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn print_coverage_report(db: &CardDatabase, deck: &[CardId], commander: CardId, full: bool) {
    println!("Card Coverage");
    println!("-------------");

    let mut card_names: Vec<&str> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Collect unique card names from deck
    for &card_id in deck {
        if seen.insert(card_id) {
            if let Some(def) = db.get(card_id) {
                card_names.push(&def.name);
            }
        }
    }
    if seen.insert(commander) {
        if let Some(def) = db.get(commander) {
            card_names.push(&def.name);
        }
    }

    // Auto-parsing is provenance, not proof that all Oracle text is implemented.
    let coverage = catalog::analyze_deck_coverage(&card_names, &sample::build_sample_db());

    let mut fully = 0;
    let mut stubbed = 0;
    let mut unimpl_cards: Vec<String> = Vec::new();

    for entry in &coverage.cards {
        match entry.level {
            catalog::CoverageLevel::FullyImplemented => fully += 1,
            catalog::CoverageLevel::Stubbed => {
                stubbed += 1;
                if entry.has_unimplemented {
                    unimpl_cards.push(format!("  {} (has Unimplemented effects)", entry.name));
                } else {
                    unimpl_cards.push(format!("  {} (stub)", entry.name));
                }
            }
            catalog::CoverageLevel::AutoParsed => {
                unimpl_cards.push(format!("  {} (not in sample DB)", entry.name));
            }
            catalog::CoverageLevel::Unknown => {
                unimpl_cards.push(format!("  {} (unknown)", entry.name));
            }
        }
    }

    let total = coverage.cards.len();
    println!(
        "  Fully implemented: {}/{} ({:.0}%)",
        fully,
        total,
        if total > 0 {
            fully as f64 / total as f64 * 100.0
        } else {
            100.0
        }
    );
    println!("  Stubbed/partial:   {}", stubbed);
    println!("  Auto-parsed:       {}", coverage.cards.iter().filter(|e| e.level == catalog::CoverageLevel::AutoParsed).count());
    if full {
        for entry in &coverage.cards {
            println!("  {:?}: {}", entry.level, entry.name);
        }
        return;
    }

    if !unimpl_cards.is_empty() && unimpl_cards.len() <= 20 {
        println!("\n  Cards with incomplete effects:");
        for card in &unimpl_cards {
            println!("{}", card);
        }
    } else if !unimpl_cards.is_empty() {
        println!(
            "\n  {} cards with incomplete effects (use --coverage for full list)",
            unimpl_cards.len()
        );
    }
}

fn print_results(r: &GoldfishResults) {
    println!("Results");
    println!("-------");
    println!(
        "  Win rate:    {:.1}% ({}/{})",
        r.win_rate() * 100.0,
        r.wins,
        r.total_games
    );
    if r.wins > 0 {
        println!("  Avg kill:    T{:.1}", r.avg_kill_turn);
        println!("  Fastest:     T{}", r.fastest_kill);
        println!("  Slowest:     T{}", r.slowest_kill);
    }
    println!("  Draws:       {}", r.draws);
    println!("  Avg actions: {:.0}", r.avg_actions);

    if r.wins > 0 {
        println!("\n  Kill Turn Distribution:");
        println!("  Turn | Wins |  P(kill) | P(by)");
        println!("  -----+------+----------+------");
        let mut cumulative = 0u64;
        for (turn, &count) in r.kill_turn_distribution.iter().enumerate() {
            if count == 0 && cumulative == 0 {
                continue;
            }
            cumulative += count;
            if count > 0 || (cumulative > 0 && turn <= r.slowest_kill as usize) {
                let p_kill = count as f64 / r.total_games as f64;
                let p_cum = cumulative as f64 / r.total_games as f64;
                println!(
                    "  T{:<3} | {:>4} |  {:>5.1}%  | {:>5.1}%",
                    turn,
                    count,
                    p_kill * 100.0,
                    p_cum * 100.0,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_deck_rejects_commander_in_mainboard() {
        let path = std::env::temp_dir().join(format!("goldfish-deck-{}.txt", std::process::id()));
        std::fs::write(&path, "~~Commanders~~\n1 Kinnan, Bonder Prodigy\n~~Mainboard~~\n1 Kinnan, Bonder Prodigy\n99 Forest\n~~Tutor Targets~~\n1 Forest\n").unwrap();
        let loaded = deck_loader::resolve_file(&path);
        std::fs::remove_file(path).unwrap();
        assert!(loaded.err().unwrap().contains("commander also appears"));
    }

    #[test]
    fn presets_preserve_sample_decks() {
        for name in ["kinnan", "brimaz", "ashcoat", "flubs"] {
            let resolved = deck_loader::resolve_preset(name).unwrap();
            let (expected, expected_commander) = match name {
                "kinnan" => { let (d,c,_) = sample::kinnan_commander_deck(); (d,c) }
                "brimaz" => sample::brimaz_commander_deck(),
                "ashcoat" => sample::ashcoat_commander_deck(),
                _ => sample::flubs_commander_deck(),
            };
            let mut actual = resolved.setup_cards.clone();
            let mut expected = expected;
            actual.sort_unstable();
            if name == "flubs" { expected.push(expected_commander); }
            expected.sort_unstable();
            assert_eq!(actual, expected);
            assert_eq!(resolved.commander, expected_commander);
        }
    }
}
