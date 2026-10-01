//! Interactive Goldfish Mode
//!
//! Play a goldfish game manually — you pick every action for player 0 while
//! player 1 (the goldfish) does nothing. The game state is displayed after
//! each action, and the complete action sequence is printed at the end so
//! you can use it to troubleshoot why the solver isn't converging to fast
//! winning lines.
//!
//! Usage:
//!   cargo run --release --bin interactive
//!
//! Options (via environment variables):
//!   DECK=red        Deck to use: "red", "green", "kinnan", "brimaz", "ashcoat"
//!                   (default: red)

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::Arc;

use mtg_gto::action::{legal_actions, Action};
use mtg_gto::card::sample;
use mtg_gto::card::ObjectId;
use mtg_gto::game::{CardDatabase, GameState, PlayerIndex, Target};
use mtg_gto::rules;

/// Maximum turns before a goldfish game is declared a draw.
const MAX_TURNS: u32 = 20;
/// Maximum actions per game.
const MAX_ACTIONS: u32 = 10_000;

fn main() {
    let deck_name = std::env::var("DECK").unwrap_or_else(|_| "red".to_string());

    let db = sample::build_sample_db();

    // Select deck
    let (deck, commander, is_commander) = match deck_name.as_str() {
        "green" => (sample::green_stompy_deck(), None, false),
        "kinnan" => {
            let (d, c) = {
                let (deck, commander, _tutor) = sample::kinnan_commander_deck();
                (deck, commander)
            };
            (d, Some(c), true)
        }
        "brimaz" => {
            let (d, c) = sample::brimaz_commander_deck();
            (d, Some(c), true)
        }
        "ashcoat" => {
            let (d, c) = sample::ashcoat_commander_deck();
            (d, Some(c), true)
        }
        "flubs" => {
            let (d, c) = sample::flubs_commander_deck();
            (d, Some(c), true)
        }
        _ => (sample::red_aggro_deck(), None, false), // default: red
    };

    // Set up game
    let db_arc = Arc::new(db.clone());
    let mut state = if is_commander {
        let mut s = GameState::new_commander(2);
        s.card_db = Some(db_arc);
        let cmd = commander.unwrap();
        rules::setup_commander_game(&mut s, &deck, &deck, cmd, cmd);
        s
    } else {
        let mut s = GameState::new(2);
        s.card_db = Some(db_arc);
        rules::setup_game(&mut s, &deck, &deck);
        s
    };

    println!("=== Interactive Goldfish Mode ===");
    println!("Deck: {}", deck_name);
    if let Some(cmd) = commander {
        let name = db.get(cmd).map(|d| d.name.as_str()).unwrap_or("?");
        println!("Commander: {}", name);
    }
    println!("You are Player 0. The goldfish (Player 1) does nothing.");
    println!("Type the number of the action you want to take.");
    println!("Type 'q' to quit, 'u' to undo last action.");
    println!();

    let mut actions_taken: u32 = 0;
    let mut action_log: Vec<String> = Vec::new();
    let mut undo_stack: Vec<GameState> = Vec::new();

    let stdin = io::stdin();
    let mut reader = stdin.lock();

    while !state.game_over && state.turn_number <= MAX_TURNS && actions_taken < MAX_ACTIONS {
        // Fast-forward the goldfish's entire turn without prompting
        if state.active_player != 0 {
            let ff_actions = rules::fast_forward_goldfish_turn(&mut state);
            actions_taken += ff_actions;
            continue;
        }

        let player = state.priority_player;
        let actions = legal_actions(&state);

        if actions.is_empty()
            || (actions.len() == 1 && actions[0] == Action::PassPriority)
        {
            rules::apply_action(&mut state, &Action::PassPriority);
            actions_taken += 1;
            continue;
        }

        // Display game state for the human player
        display_game_state(&state, &db);

        // Display legal actions
        println!("--- Legal Actions ---");
        for (i, action) in actions.iter().enumerate() {
            let desc = format_action_rich(&state, action, &db);
            println!("  [{}] {}", i, desc);
        }
        println!();

        // Read player input
        loop {
            print!("> ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            if reader.read_line(&mut input).unwrap() == 0 {
                // EOF
                println!("\nEnd of input. Quitting.");
                print_action_log(&action_log);
                return;
            }

            let trimmed = input.trim();

            if trimmed == "q" || trimmed == "quit" {
                println!("Quitting.");
                print_action_log(&action_log);
                return;
            }

            if trimmed == "u" || trimmed == "undo" {
                if let Some(prev) = undo_stack.pop() {
                    state = prev;
                    if let Some(last) = action_log.pop() {
                        println!("Undid: {}", last);
                    }
                    actions_taken = actions_taken.saturating_sub(1);
                    println!();
                    break; // re-display state
                } else {
                    println!("Nothing to undo.");
                    continue;
                }
            }

            if trimmed == "h" || trimmed == "help" {
                println!("Commands:");
                println!("  <number>  - Pick that action");
                println!("  u/undo    - Undo last action");
                println!("  q/quit    - Quit game");
                println!("  h/help    - Show this help");
                continue;
            }

            match trimmed.parse::<usize>() {
                Ok(idx) if idx < actions.len() => {
                    let chosen = &actions[idx];
                    let desc = format_action_description(&state, chosen, player);

                    // Save state for undo
                    undo_stack.push(state.clone());

                    action_log.push(format!(
                        "[T{} {:?} P{}] {}",
                        state.turn_number, state.phase, player, desc
                    ));

                    rules::apply_action(&mut state, chosen);
                    actions_taken += 1;

                    println!(">>> {}", desc);
                    println!();
                    break;
                }
                Ok(idx) => {
                    println!(
                        "Invalid choice: {}. Must be 0-{}.",
                        idx,
                        actions.len() - 1
                    );
                }
                Err(_) => {
                    println!("Invalid input: '{}'. Enter a number, 'u' to undo, or 'q' to quit.", trimmed);
                }
            }
        }
    }

    // Game over
    println!();
    println!("========================================");
    display_game_state(&state, &db);

    match state.winner {
        Some(0) => println!("*** YOU WIN on turn {}! ***", state.turn_number),
        Some(_) => println!("*** YOU LOST on turn {}. ***", state.turn_number),
        None => println!("*** DRAW (turn limit reached). ***"),
    }
    println!(
        "Final life totals: You={} Goldfish={}",
        state.players[0].life, state.players[1].life
    );
    println!("Total actions: {}", actions_taken);
    println!();

    print_action_log(&action_log);
}

// ---------------------------------------------------------------------------
// Game state display
// ---------------------------------------------------------------------------

fn display_game_state(state: &GameState, db: &CardDatabase) {
    let p = &state.players[0];
    let opp = &state.players[1];

    println!("========================================");
    println!(
        "Turn {} | Phase: {:?} | Priority: P{}",
        state.turn_number, state.phase, state.priority_player
    );
    println!("========================================");

    // Life totals
    println!(
        "Life: You={} | Goldfish={}",
        p.life, opp.life
    );

    // Mana pool
    let pool = &p.mana_pool;
    if pool.total() > 0 {
        print!("Mana pool: ");
        let mut parts = Vec::new();
        if pool.white > 0 { parts.push(format!("{}W", pool.white)); }
        if pool.blue > 0 { parts.push(format!("{}U", pool.blue)); }
        if pool.black > 0 { parts.push(format!("{}B", pool.black)); }
        if pool.red > 0 { parts.push(format!("{}R", pool.red)); }
        if pool.green > 0 { parts.push(format!("{}G", pool.green)); }
        if pool.colorless > 0 { parts.push(format!("{}C", pool.colorless)); }
        println!("{}", parts.join(" "));
    }

    // Land plays remaining
    if state.phase.is_main_phase() && state.priority_player == 0 {
        println!("Land plays remaining: {}", p.land_plays_remaining);
    }

    // Hand
    println!();
    println!("Hand ({}):", p.hand.len());
    for &obj_id in &p.hand {
        let inst = &state.objects[&obj_id];
        if let Some(def) = db.get(inst.card_def_id) {
            let cost_str = def.mana_cost.as_ref()
                .map(|c| format!(" {}", c))
                .unwrap_or_default();
            let type_str = format_card_type(def);
            let stats = if def.is_creature() {
                format!(" {}/{}",
                    def.power.unwrap_or(0),
                    def.toughness.unwrap_or(0))
            } else {
                String::new()
            };
            println!("  [obj {}] {}{} — {}{}", obj_id, def.name, cost_str, type_str, stats);
        }
    }

    // Battlefield (your side)
    let your_perms: Vec<ObjectId> = state.permanents_controlled_by(0);
    if !your_perms.is_empty() {
        println!();
        println!("Your Battlefield ({}):", your_perms.len());
        for &obj_id in &your_perms {
            let inst = &state.objects[&obj_id];
            if let Some(def) = db.get(inst.card_def_id) {
                let tapped = if inst.tapped { " (tapped)" } else { "" };
                let sick = if inst.summoning_sick && def.is_creature() { " (summoning sick)" } else { "" };
                let stats = if def.is_creature() {
                    let p = state.effective_power(obj_id);
                    let t = state.effective_toughness(obj_id);
                    let dmg = if inst.damage_marked > 0 {
                        format!(" [{} dmg]", inst.damage_marked)
                    } else {
                        String::new()
                    };
                    let counters = if inst.plus_counters != 0 || inst.minus_counters != 0 {
                        format!(" [+{}/-{}]", inst.plus_counters, inst.minus_counters)
                    } else {
                        String::new()
                    };
                    format!(" {}/{}{}{}", p, t, dmg, counters)
                } else {
                    String::new()
                };
                println!("  [obj {}] {}{}{}{}", obj_id, def.name, stats, tapped, sick);
            }
        }
    }

    // Opponent battlefield
    let opp_perms: Vec<ObjectId> = state.permanents_controlled_by(1);
    if !opp_perms.is_empty() {
        println!();
        println!("Goldfish Battlefield ({}):", opp_perms.len());
        for &obj_id in &opp_perms {
            let inst = &state.objects[&obj_id];
            if let Some(def) = db.get(inst.card_def_id) {
                let tapped = if inst.tapped { " (tapped)" } else { "" };
                println!("  [obj {}] {}{}", obj_id, def.name, tapped);
            }
        }
    }

    // Combat state
    if !state.combat.attackers.is_empty() {
        println!();
        println!("Combat:");
        for &atk_id in &state.combat.attackers {
            if let Some(def) = db.get(state.objects[&atk_id].card_def_id) {
                let p = state.effective_power(atk_id);
                let t = state.effective_toughness(atk_id);
                print!("  Attacking: {} {}/{}", def.name, p, t);
                // Show blockers
                if let Some(blockers) = state.combat.attacker_blockers.get(&atk_id) {
                    if !blockers.is_empty() {
                        let blocker_names: Vec<String> = blockers.iter().map(|&bid| {
                            db.get(state.objects[&bid].card_def_id)
                                .map(|d| d.name.clone())
                                .unwrap_or_else(|| "?".into())
                        }).collect();
                        print!(" blocked by [{}]", blocker_names.join(", "));
                    }
                }
                println!();
            }
        }
    }

    // Stack
    if !state.stack.is_empty() {
        println!();
        println!("Stack ({}):", state.stack.len());
        for (i, entry) in state.stack.iter().enumerate().rev() {
            let desc = match &entry.source {
                mtg_gto::game::StackSource::Spell(obj_id) => {
                    db.get(state.objects[obj_id].card_def_id)
                        .map(|d| format!("Spell: {}", d.name))
                        .unwrap_or_else(|| "Spell: ?".into())
                }
                mtg_gto::game::StackSource::SpellCopy { definition } => {
                    format!("Spell copy: {} (stack #{})", definition.name, entry.id)
                }
                mtg_gto::game::StackSource::ActivatedAbility { source_id, ability_index } => {
                    db.get(state.objects[source_id].card_def_id)
                        .map(|d| format!("Ability of {} (#{}) ", d.name, ability_index))
                        .unwrap_or_else(|| "Ability: ?".into())
                }
                mtg_gto::game::StackSource::TriggeredAbility { ability_index, context, .. } => {
                    db.get(context.source_card_id)
                        .map(|d| format!("Trigger of {} (#{}) ", d.name, ability_index))
                        .unwrap_or_else(|| "Trigger: ?".into())
                }
            };
            println!("  [{}] {} (P{})", i, desc, entry.controller);
        }
    }

    // Graveyard
    if !p.graveyard.is_empty() {
        println!();
        println!("Your Graveyard ({}):", p.graveyard.len());
        let mut grave_counts: HashMap<String, u32> = HashMap::new();
        for &obj_id in &p.graveyard {
            let name = db.get(state.objects[&obj_id].card_def_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "?".into());
            *grave_counts.entry(name).or_insert(0) += 1;
        }
        for (name, count) in &grave_counts {
            if *count > 1 {
                println!("  {}x {}", count, name);
            } else {
                println!("  {}", name);
            }
        }
    }

    // Library size
    println!();
    println!("Library: {} cards", p.library.len());
    println!("----------------------------------------");
}

// ---------------------------------------------------------------------------
// Action formatting
// ---------------------------------------------------------------------------

/// Rich description of an action for the menu display.
fn format_action_rich(state: &GameState, action: &Action, db: &CardDatabase) -> String {
    match action {
        Action::PassPriority => "Pass priority".into(),
        Action::PlayLand { object_id } => {
            let name = card_name(state, *object_id, db);
            format!("Play land: {}", name)
        }
        Action::CastSpell { object_id, targets } => {
            let name = card_name(state, *object_id, db);
            let cost = card_cost(state, *object_id, db);
            let tgt = format_targets(state, targets, db);
            if tgt.is_empty() {
                format!("Cast: {} {}", name, cost)
            } else {
                format!("Cast: {} {} -> {}", name, cost, tgt)
            }
        }
        Action::CastCommander { object_id, targets } => {
            let name = card_name(state, *object_id, db);
            let cost = card_cost(state, *object_id, db);
            let tgt = format_targets(state, targets, db);
            if tgt.is_empty() {
                format!("Cast commander: {} {}", name, cost)
            } else {
                format!("Cast commander: {} {} -> {}", name, cost, tgt)
            }
        }
        Action::ActivateManaAbility { object_id, ability_index } => {
            let name = card_name(state, *object_id, db);
            let inst = &state.objects[object_id];
            let mana_desc = db.get(inst.card_def_id)
                .and_then(|d| d.mana_abilities.get(*ability_index))
                .map(|ma| format!("{:?}", ma))
                .unwrap_or_else(|| "?".into());
            format!("Tap for mana: {} ({})", name, mana_desc)
        }
        Action::ActivateAbility { object_id, ability_index, targets } => {
            let name = card_name(state, *object_id, db);
            let tgt = format_targets(state, targets, db);
            if tgt.is_empty() {
                format!("Activate ability #{} of {}", ability_index, name)
            } else {
                format!("Activate ability #{} of {} -> {}", ability_index, name, tgt)
            }
        }
        Action::DeclareAttackers { attackers } => {
            if attackers.is_empty() {
                "Declare attackers: (none)".into()
            } else {
                let names: Vec<String> = attackers.iter().map(|&id| {
                    let name = card_name(state, id, db);
                    let p = state.effective_power(id);
                    let t = state.effective_toughness(id);
                    format!("{} {}/{}", name, p, t)
                }).collect();
                format!("Attack with: [{}]", names.join(", "))
            }
        }
        Action::DeclareBlockers { blocks } => {
            if blocks.is_empty() {
                "Declare blockers: (none)".into()
            } else {
                let descs: Vec<String> = blocks.iter().map(|&(blocker, attacker)| {
                    format!("{} blocks {}", card_name(state, blocker, db), card_name(state, attacker, db))
                }).collect();
                format!("Block: [{}]", descs.join(", "))
            }
        }
        Action::OrderDamageAssignment { attacker, assignment } => {
            let name = card_name(state, *attacker, db);
            let assigns: Vec<String> = assignment.iter().map(|&(target, dmg)| {
                format!("{} <- {} dmg", card_name(state, target, db), dmg)
            }).collect();
            format!("Assign damage from {}: [{}]", name, assigns.join(", "))
        }
        Action::OrderTriggers { ordering } => {
            let descs: Vec<String> = ordering.iter().map(|&(src, idx)| {
                format!("{}#{}", card_name(state, src, db), idx)
            }).collect();
            format!("Order triggers: [{}]", descs.join(", "))
        }
        Action::ChooseReplacementOrder { ordering } => {
            let descs: Vec<String> = ordering.iter().map(|&(src, idx)| {
                format!("{}#{}", card_name(state, src, db), idx)
            }).collect();
            format!("Order replacement effects: [{}]", descs.join(", "))
        }
        Action::Discard { object_id } => {
            let name = card_name(state, *object_id, db);
            format!("Discard: {}", name)
        }
        Action::MulliganKeep => "Keep hand".into(),
        Action::MulliganMulligan => "Mulligan".into(),
        Action::MulliganBottomCard { object_id } => {
            let name = card_name(state, *object_id, db);
            format!("Bottom: {}", name)
        }
        Action::ChooseTutorTarget { card_id } => {
            let name = db.get(*card_id)
                .map(|d| d.name.as_str())
                .unwrap_or("?");
            format!("Tutor for: {}", name)
        }
        Action::ActivateLoyalty { object_id, ability_index } => {
            let name = card_name(state, *object_id, db);
            let inst = &state.objects[object_id];
            let desc = db.get(inst.card_def_id)
                .and_then(|d| d.loyalty_abilities.get(*ability_index))
                .map(|la| {
                    let sign = if la.cost >= 0 { "+" } else { "" };
                    format!("[{}{}]: {}", sign, la.cost, la.description)
                })
                .unwrap_or_else(|| format!("ability #{}", ability_index));
            format!("Loyalty: {} {}", name, desc)
        }
        Action::Equip { equipment_id, target_id } => {
            let eq_name = card_name(state, *equipment_id, db);
            let tgt_name = card_name(state, *target_id, db);
            format!("Equip {} to {}", eq_name, tgt_name)
        }
        Action::CastFromGraveyard { object_id, .. } => {
            let name = card_name(state, *object_id, db);
            format!("Flashback/Escape: {}", name)
        }
        Action::Concede => "Concede".into(),
        Action::ActivateMacro { combo_id } => {
            format!("Activate combo #{}", combo_id)
        }
        Action::EndTurn => "End turn (skip remaining phases)".into(),
        Action::PlayLandFromGraveyard { object_id } => {
            let name = card_name(state, *object_id, db);
            format!("Play land from graveyard: {}", name)
        }
    }
}

/// Short description of an action for the log.
fn format_action_description(state: &GameState, action: &Action, _player: PlayerIndex) -> String {
    let db = state.card_db();
    format_action_rich(state, action, db)
}

fn card_name(state: &GameState, obj_id: ObjectId, db: &CardDatabase) -> String {
    db.get(state.objects[&obj_id].card_def_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| format!("obj#{}", obj_id))
}

fn card_cost(state: &GameState, obj_id: ObjectId, db: &CardDatabase) -> String {
    let inst = &state.objects[&obj_id];
    db.get(inst.card_def_id)
        .and_then(|d| d.mana_cost.as_ref())
        .map(|c| format!("{}", c))
        .unwrap_or_default()
}

fn format_targets(state: &GameState, targets: &[Target], db: &CardDatabase) -> String {
    if targets.is_empty() {
        return String::new();
    }
    let descs: Vec<String> = targets.iter().map(|t| match t {
        Target::Player(p) => {
            if *p == 0 { "You".into() } else { "Goldfish".into() }
        }
        Target::Object(id) => card_name(state, *id, db),
        Target::StackEntry(id) => state.stack.iter().find(|entry| entry.id == *id)
            .and_then(|entry| match entry.source {
                mtg_gto::game::StackSource::Spell(object) => Some(format!("{} (stack #{})", card_name(state, object, db), id)),
                mtg_gto::game::StackSource::SpellCopy { ref definition } => Some(format!("{} copy (stack #{})", definition.name, id)),
                _ => None,
            }).unwrap_or_else(|| format!("Stack #{} (gone)", id)),
    }).collect();
    descs.join(", ")
}

fn format_card_type(def: &mtg_gto::card::CardDef) -> String {
    let types: Vec<&str> = def.card_types.iter().map(|t| match t {
        mtg_gto::card::CardType::Creature => "Creature",
        mtg_gto::card::CardType::Instant => "Instant",
        mtg_gto::card::CardType::Sorcery => "Sorcery",
        mtg_gto::card::CardType::Enchantment => "Enchantment",
        mtg_gto::card::CardType::Artifact => "Artifact",
        mtg_gto::card::CardType::Land => "Land",
        mtg_gto::card::CardType::Planeswalker => "Planeswalker",
    }).collect();
    types.join(" ")
}

// ---------------------------------------------------------------------------
// Action log output
// ---------------------------------------------------------------------------

fn print_action_log(log: &[String]) {
    println!();
    println!("=== Action Sequence ({} actions) ===", log.len());
    for (i, entry) in log.iter().enumerate() {
        println!("  {:>3}. {}", i + 1, entry);
    }
    println!("=== End Action Sequence ===");
}
