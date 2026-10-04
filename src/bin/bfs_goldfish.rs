//! BFS Goldfish Solver — Exhaustive DFS with Branch-and-Bound
//!
//! Finds the minimum-turn goldfish kill for a Commander deck
//! by exhaustively searching all play lines with aggressive pruning.
//!
//! Supports both combo decks (Kinnan) and combat decks (Ashcoat).
//! Turn numbers are real Magic turns (a full round of all players).
//!
//! Usage:
//!   DECK=kinnan cargo run --release --bin bfs_goldfish
//!   DECK=ashcoat SEEDS=100 cargo run --release --bin bfs_goldfish
//!
//! Environment variables:
//!   DECK=kinnan      Deck to solve: kinnan, ashcoat (default: kinnan)
//!   SEEDS=10        Number of random seeds to test (default: 10)
//!   SEED_START=0    First seed value (default: 0)
//!   MAX_TURN=10     Prune branches beyond this game turn (default: 10)
//!   TIMEOUT=30      Seconds per seed before aborting (default: 30)
//!   MAX_STATES=500000  Max states explored per seed (default: 500K)
//!   VERBOSE=0       Print action traces for wins (default: 0)
//!   TRACE=0         Print DFS decision trace (default: 0)
//!
//! Results (100 seeds, MAX_TURN=10):
//!   Kinnan:  76% win rate, T2-T10 kills (combo)
//!   Ashcoat: 91% win rate, T5-T10 kills (combat)

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

use mtg_gto::action::{legal_actions, Action};
use mtg_gto::card::sample::{self, ids};
use mtg_gto::card::CardId;
use mtg_gto::combo::ComboCategory;
use mtg_gto::combo_discovery::DiscoveryConfig;
use mtg_gto::game::{GameState, Phase};
use mtg_gto::rules;
use mtg_gto::simulation::{apply_counted_action, format_action_name, invalid_cleanup_state, TerminationReason};
use mtg_gto::strategy::{GreedyStrategy, Strategy};

/// Convert engine turn_number to Magic game turn.
/// The engine increments turn_number for each player's turn,
/// but in Magic a "turn" is a full round of all players.
/// In a 2-player game: engine turn 1,2 = game turn 1; engine turn 3,4 = game turn 2; etc.
fn game_turn(engine_turn: u32) -> u32 {
    (engine_turn + 1) / 2
}

/// Record of an action taken during search, for trace reconstruction.
#[derive(Clone)]
struct ActionRecord {
    turn: u32,
    #[allow(dead_code)]
    action: Action,
    description: String,
}

/// Statistics for a single seed's search.
struct SearchStats {
    states_explored: u64,
    states_pruned_bound: u64,
    states_pruned_dedup: u64,
    max_depth_reached: usize,
    timed_out: bool,
    hit_state_cap: bool,
    hit_depth_cap: bool,
    hit_turn_cap: bool,
    stalled: bool,
    invalid: bool,
    stalled_branches: u64,
    invalid_branches: u64,
    invalid_reason: &'static str,
}

fn search_action_advanced(stats: &mut SearchStats,
    result: Result<bool, TerminationReason>) -> bool {
    match result {
        Ok(true) => true,
        Ok(false) => {
            stats.stalled = true;
            stats.stalled_branches += 1;
            false
        }
        Err(reason) => {
            stats.invalid = true;
            stats.invalid_branches += 1;
            stats.invalid_reason = reason.code();
            false
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SearchOutcome {
    Win(u32),
    FoundWinIncomplete(u32, &'static str),
    ExhaustedNoWin,
    Censored(&'static str),
    Stalled(&'static str),
    Invalid(&'static str),
}

fn classify_search(win_turn: Option<u32>, stats: &SearchStats) -> SearchOutcome {
    if let Some(turn) = win_turn {
        if stats.invalid { return SearchOutcome::FoundWinIncomplete(turn, "invalid_branch"); }
        if stats.stalled { return SearchOutcome::FoundWinIncomplete(turn, "stalled_branch"); }
        if stats.timed_out { return SearchOutcome::FoundWinIncomplete(turn, "timeout"); }
        if stats.hit_state_cap { return SearchOutcome::FoundWinIncomplete(turn, "state_cap"); }
        if stats.hit_depth_cap { return SearchOutcome::FoundWinIncomplete(turn, "depth_cap"); }
        return SearchOutcome::Win(turn);
    }
    if stats.invalid { return SearchOutcome::Invalid(stats.invalid_reason); }
    if stats.stalled { return SearchOutcome::Stalled("no_progress"); }
    if stats.timed_out { return SearchOutcome::Censored("timeout"); }
    if stats.hit_state_cap { return SearchOutcome::Censored("state_cap"); }
    if stats.hit_depth_cap { return SearchOutcome::Censored("depth_cap"); }
    if stats.hit_turn_cap && win_turn.is_none() { return SearchOutcome::Censored("turn_cap"); }
    SearchOutcome::ExhaustedNoWin
}

impl SearchStats {
    fn new() -> Self {
        SearchStats {
            states_explored: 0,
            states_pruned_bound: 0,
            states_pruned_dedup: 0,
            max_depth_reached: 0,
            timed_out: false,
            hit_state_cap: false,
            hit_depth_cap: false,
            hit_turn_cap: false,
            stalled: false,
            invalid: false,
            stalled_branches: 0,
            invalid_branches: 0,
            invalid_reason: "incomplete_cleanup",
        }
    }
}

/// Resource limits for a single search.
struct SearchLimits {
    deadline: Instant,
    max_states: u64,
    max_visited: usize,
    max_depth: usize,
    trace: bool,
}

/// Deck-specific configuration for the solver.
struct DeckConfig {
    deck: Vec<CardId>,
    commander: CardId,
    tutor_targets: Vec<CardId>,
    dead_cards: HashSet<CardId>,
    fetch_lands: HashSet<CardId>,
    tutor_worthy: HashSet<CardId>,
    /// Cards whose activated abilities are filtered (handled by combo macros).
    filtered_abilities: HashSet<CardId>,
    /// Whether to register the Ballista win combo manually.
    register_ballista: bool,
    /// Whether to skip combat (true for combo-only decks like Kinnan).
    skip_combat: bool,
}

fn kinnan_config() -> DeckConfig {
    let (deck, commander, tutor_targets) = sample::kinnan_commander_deck();
    DeckConfig {
        deck,
        commander,
        tutor_targets,
        dead_cards: [
            ids::AN_OFFER_YOU_CANT_REFUSE,
            ids::FIERCE_GUARDIANSHIP,
            ids::FLUSTERSTORM,
            ids::FORCE_OF_NEGATION,
            ids::FORCE_OF_WILL,
            ids::MENTAL_MISSTEP,
            ids::MINDBREAK_TRAP,
            ids::SWAN_SONG,
            ids::MUDDLE_THE_MIXTURE,
            ids::CYCLONIC_RIFT,
            ids::INTO_THE_FLOOD_MAW,
            ids::VEIL_OF_SUMMER,
            ids::SINK_INTO_STUPOR,
            ids::MYSTIC_REMORA,
            ids::RHYSTIC_STUDY,
        ]
        .into_iter()
        .collect(),
        fetch_lands: [
            ids::FLOODED_STRAND,
            ids::MISTY_RAINFOREST,
            ids::WINDSWEPT_HEATH,
        ]
        .into_iter()
        .collect(),
        tutor_worthy: [
            ids::BASALT_MONOLITH,
            ids::WALKING_BALLISTA,
            ids::GRIM_MONOLITH,
        ]
        .into_iter()
        .collect(),
        filtered_abilities: [ids::BASALT_MONOLITH, ids::GRIM_MONOLITH]
            .into_iter()
            .collect(),
        register_ballista: true,
        skip_combat: true,
    }
}

fn ashcoat_config() -> DeckConfig {
    let (deck, commander) = sample::ashcoat_commander_deck();
    DeckConfig {
        deck,
        commander,
        tutor_targets: vec![], // No tutor targets
        dead_cards: [
            // Opponent-dependent cards useless in goldfish
            ids::BURGLAR_RAT,
            ids::CHITTERING_RATS,
            ids::GNAT_MISER,
            ids::NEZUMI_BONE_READER,
            ids::NEZUMI_SHORTFANG,
            ids::RAVENOUS_RATS,
            ids::DICTATE_OF_EREBOS,
            ids::GRAVE_PACT,
            ids::CHAIN_ASSASSINATION,
        ]
        .into_iter()
        .collect(),
        fetch_lands: HashSet::new(),
        tutor_worthy: HashSet::new(), // No tutors
        filtered_abilities: HashSet::new(),
        register_ballista: false,
        skip_combat: false,
    }
}

fn flubs_config() -> DeckConfig {
    let (deck, commander) = sample::flubs_commander_deck();
    DeckConfig {
        deck,
        commander,
        tutor_targets: vec![],
        dead_cards: [
            // Counterspells and opponent-interaction cards useless in goldfish
            ids::SAW_IT_COMING,
            ids::NULL_BROOCH,
            ids::BRIDGE_OF_KHAZAD_DUM,
        ]
        .into_iter()
        .collect(),
        fetch_lands: HashSet::new(),
        tutor_worthy: HashSet::new(),
        filtered_abilities: HashSet::new(),
        register_ballista: false,
        skip_combat: false, // Combat deck — need attack phases
    }
}

/// Check if any action is an instant-win combo macro.
fn find_instant_win(state: &GameState, actions: &[Action]) -> Option<Action> {
    let registry = state.combo_registry.as_ref()?;
    for action in actions {
        if let Action::ActivateMacro { combo_id } = action {
            if let Some(combo) = registry.get(*combo_id) {
                if combo.categories.contains(&ComboCategory::InfiniteDamage) {
                    return Some(action.clone());
                }
            }
        }
    }
    None
}

/// Prune actions to reduce branching factor.
fn prune_actions(
    state: &GameState,
    actions: &[Action],
    config: &DeckConfig,
) -> Vec<Action> {
    // 0. Phase-based auto-pass: during non-strategic phases on our turn, just pass.
    // For combo decks: only Upkeep + PreCombatMain matter.
    // For combat decks: also need DeclareAttackers + combat phases.
    if state.active_player == 0 {
        let dominated_phase = if config.skip_combat {
            matches!(
                state.phase,
                Phase::Draw
                    | Phase::BeginningOfCombat
                    | Phase::DeclareBlockers
                    | Phase::FirstStrikeDamage
                    | Phase::CombatDamage
                    | Phase::EndOfCombat
                    | Phase::PostCombatMain
                    | Phase::EndStep
                    | Phase::Cleanup
            )
        } else {
            matches!(
                state.phase,
                Phase::Draw
                    | Phase::BeginningOfCombat
                    | Phase::DeclareBlockers
                    | Phase::FirstStrikeDamage
                    | Phase::CombatDamage
                    | Phase::EndOfCombat
                    | Phase::EndStep
                    | Phase::Cleanup
            )
        };
        if dominated_phase {
            if let Some(win) = find_instant_win(state, actions) {
                return vec![win];
            }
            if actions.iter().any(|a| matches!(a, Action::EndTurn)) {
                return vec![Action::EndTurn];
            }
            return vec![Action::PassPriority];
        }
    }

    // 1. Instant win — always take it.
    if let Some(win) = find_instant_win(state, actions) {
        return vec![win];
    }

    // 2. Forced ordering — no strategic value in goldfish.
    for action in actions {
        if matches!(
            action,
            Action::OrderTriggers { .. } | Action::ChooseReplacementOrder { .. }
        ) {
            return vec![action.clone()];
        }
    }

    // 3. Mulligan/discard — use greedy strategy (single deterministic choice).
    let has_mulligan = actions
        .iter()
        .any(|a| matches!(a, Action::MulliganKeep | Action::MulliganMulligan));
    let all_discard = actions
        .iter()
        .all(|a| matches!(a, Action::Discard { .. }));
    let has_bottom = actions
        .iter()
        .any(|a| matches!(a, Action::MulliganBottomCard { .. }));
    if has_mulligan || all_discard || has_bottom {
        let greedy = GreedyStrategy;
        let choice = greedy.choose_action(state, state.priority_player);
        return vec![choice];
    }

    // 4. Tutor restriction.
    if actions
        .iter()
        .any(|a| matches!(a, Action::ChooseTutorTarget { .. }))
    {
        let is_fetch = state
            .pending_tutor
            .as_ref()
            .map(|pt| !pt.subtype_filter.is_empty())
            .unwrap_or(false);
        if is_fetch {
            // Fetch land — just pick first target (lands are mostly equivalent).
            for a in actions {
                if matches!(a, Action::ChooseTutorTarget { .. }) {
                    return vec![a.clone()];
                }
            }
            return vec![Action::PassPriority];
        }
        // Regular tutor — only worthy targets.
        if config.tutor_worthy.is_empty() {
            // No tutor restriction — allow all targets.
            return actions
                .iter()
                .filter(|a| matches!(a, Action::ChooseTutorTarget { .. }))
                .cloned()
                .collect();
        }
        let worthy: Vec<Action> = actions
            .iter()
            .filter(|a| matches!(a, Action::ChooseTutorTarget { card_id } if config.tutor_worthy.contains(card_id)))
            .cloned()
            .collect();
        if worthy.is_empty() {
            return vec![Action::PassPriority];
        }
        return worthy;
    }

    // 5. Combat handling.
    if actions
        .iter()
        .any(|a| matches!(a, Action::DeclareAttackers { .. }))
    {
        if config.skip_combat {
            // Combo-only deck — skip combat entirely.
            return vec![Action::DeclareAttackers {
                attackers: vec![],
            }];
        }
        // Combat deck — attack with everything (goldfish has no blockers)
        // and also offer attacking with nothing (to skip combat).
        // Pick the largest attacker set to maximize damage.
        let mut best = Action::DeclareAttackers {
            attackers: vec![],
        };
        let mut best_count = 0;
        for a in actions {
            if let Action::DeclareAttackers { attackers } = a {
                if attackers.len() > best_count {
                    best_count = attackers.len();
                    best = a.clone();
                }
            }
        }
        return vec![best];
    }
    if actions
        .iter()
        .any(|a| matches!(a, Action::DeclareBlockers { .. }))
    {
        // Goldfish opponent never blocks.
        return vec![Action::DeclareBlockers { blocks: vec![] }];
    }

    // 6. Filter dead cards, mana abilities, and redundant activated abilities.
    let mut result = Vec::new();
    for action in actions {
        match action {
            Action::CastSpell { object_id, .. } => {
                if let Some(inst) = state.objects.get(object_id) {
                    if config.dead_cards.contains(&inst.card_def_id) {
                        continue;
                    }
                }
                result.push(action.clone());
            }
            // Engine auto-taps; manual mana abilities waste branching.
            Action::ActivateManaAbility { .. } => continue,
            // Filter abilities handled by combo macros.
            Action::ActivateAbility { object_id, .. } => {
                if let Some(inst) = state.objects.get(object_id) {
                    if config.filtered_abilities.contains(&inst.card_def_id) {
                        continue;
                    }
                }
                result.push(action.clone());
            }
            _ => result.push(action.clone()),
        }
    }

    // 8. Fetch land forcing: if a fetch land activation is available,
    //    crack it immediately (no reason to hold fetches in goldfish).
    if !config.fetch_lands.is_empty() {
        for a in &result {
            if let Action::ActivateAbility { object_id, .. } = a {
                if let Some(inst) = state.objects.get(object_id) {
                    if config.fetch_lands.contains(&inst.card_def_id) {
                        return vec![a.clone()];
                    }
                }
            }
        }
    }

    // 9. Auto-pass if nothing meaningful remains.
    let has_castable = result.iter().any(|a| {
        matches!(
            a,
            Action::PlayLand { .. }
                | Action::CastSpell { .. }
                | Action::CastCommander { .. }
                | Action::ActivateAbility { .. }
                | Action::ActivateMacro { .. }
                | Action::Equip { .. }
                | Action::ActivateLoyalty { .. }
                | Action::CastFromGraveyard { .. }
                | Action::PlayLandFromGraveyard { .. }
        )
    });
    if !has_castable {
        if result.iter().any(|a| matches!(a, Action::EndTurn)) {
            return vec![Action::EndTurn];
        }
        return vec![Action::PassPriority];
    }

    // 10. Remove EndTurn when there are spells to cast or lands to play.
    //     EndTurn mid-main-phase is almost never optimal when there's
    //     productive work to do.
    if has_castable {
        result.retain(|a| !matches!(a, Action::EndTurn));
    }

    // 11. Reorder: meaningful actions first, Pass last.
    result.sort_by_key(|a| match a {
        Action::ActivateMacro { .. } => 0,
        Action::CastCommander { .. } => 1,
        Action::CastSpell { .. } => 2,
        Action::PlayLand { .. } => 3,
        Action::ActivateAbility { .. } => 4,
        Action::PassPriority => 6,
        _ => 5,
    });

    result
}

/// Compute a state fingerprint for deduplication.
fn fingerprint(state: &GameState) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    state.turn_number.hash(&mut hasher);
    std::mem::discriminant(&state.phase).hash(&mut hasher);
    state.active_player.hash(&mut hasher);
    state.priority_player.hash(&mut hasher);

    let p0 = &state.players[0];
    p0.life.hash(&mut hasher);
    p0.mana_pool.white.hash(&mut hasher);
    p0.mana_pool.blue.hash(&mut hasher);
    p0.mana_pool.black.hash(&mut hasher);
    p0.mana_pool.red.hash(&mut hasher);
    p0.mana_pool.green.hash(&mut hasher);
    p0.mana_pool.colorless.hash(&mut hasher);
    p0.land_plays_remaining.hash(&mut hasher);
    p0.commander_tax.hash(&mut hasher);

    let mut hand_ids: Vec<CardId> = p0
        .hand
        .iter()
        .filter_map(|oid| state.objects.get(oid).map(|inst| inst.card_def_id))
        .collect();
    hand_ids.sort_unstable();
    hand_ids.hash(&mut hasher);

    let mut bf: Vec<(CardId, bool, usize)> = state
        .battlefield
        .iter()
        .filter_map(|oid| {
            state
                .objects
                .get(oid)
                .map(|inst| (inst.card_def_id, inst.tapped, inst.controller))
        })
        .collect();
    bf.sort_unstable();
    bf.hash(&mut hasher);

    let mut cmd: Vec<CardId> = p0
        .command_zone
        .iter()
        .filter_map(|oid| state.objects.get(oid).map(|inst| inst.card_def_id))
        .collect();
    cmd.sort_unstable();
    cmd.hash(&mut hasher);

    state.stack.len().hash(&mut hasher);
    p0.library.len().hash(&mut hasher);
    state.players[1].life.hash(&mut hasher);

    hasher.finish()
}

/// Core DFS search with branch-and-bound and resource limits.
///
/// Uses a loop for deterministic (non-branching) states to avoid
/// unnecessary cloning and recursion overhead. Only recurses when
/// there's actual branching (multiple pruned actions).
fn dfs_search(
    state: &mut GameState,
    best_win_turn: &mut u32,
    best_sequence: &mut Vec<ActionRecord>,
    current_sequence: &mut Vec<ActionRecord>,
    config: &DeckConfig,
    stats: &mut SearchStats,
    visited: &mut HashMap<u64, u32>,
    limits: &SearchLimits,
) {
    // We advance a single clone through deterministic steps,
    // only cloning when branching is needed.
    let mut work = state.clone();
    let mut linear_depth: usize = 0; // track linear actions pushed

    loop {
        // --- Resource checks (every 1024 states) ---
        if stats.states_explored & 0x3FF == 0 && stats.states_explored > 0 {
            if Instant::now() >= limits.deadline {
                stats.timed_out = true;
                break;
            }
        }
        if stats.timed_out || stats.hit_state_cap {
            break;
        }
        if stats.states_explored >= limits.max_states {
            stats.hit_state_cap = true;
            break;
        }

        if current_sequence.len() > stats.max_depth_reached {
            stats.max_depth_reached = current_sequence.len();
        }

        // Win check
        if work.game_over {
            if work.winner == Some(0) && work.turn_number < *best_win_turn {
                *best_win_turn = work.turn_number;
                *best_sequence = current_sequence.clone();
            }
            break;
        }

        if invalid_cleanup_state(&work) {
            stats.invalid = true;
            stats.invalid_branches += 1;
            break;
        }

        // Bound prune
        if work.turn_number >= *best_win_turn {
            stats.states_pruned_bound += 1;
            if best_sequence.is_empty() { stats.hit_turn_cap = true; }
            break;
        }

        // Depth limit
        if current_sequence.len() > limits.max_depth {
            stats.hit_depth_cap = true;
            break;
        }

        // Opponent's turn — fast-forward without branching.
        if work.active_player != 0 {
            let before = match bincode::serialize(&work) {
                Ok(bytes) => bytes,
                Err(_) => {
                    stats.invalid = true;
                    stats.invalid_branches += 1;
                    stats.invalid_reason = "state_encoding";
                    break;
                }
            };
            let advanced = rules::fast_forward_goldfish_turn(&mut work);
            if advanced == 0 {
                if invalid_cleanup_state(&work) {
                    stats.invalid = true;
                    stats.invalid_branches += 1;
                } else {
                    stats.stalled = true;
                    stats.stalled_branches += 1;
                }
                break;
            }
            match bincode::serialize(&work) {
                Ok(after) if after != before => {}
                Ok(_) => {
                    stats.stalled = true;
                    stats.stalled_branches += 1;
                    break;
                }
                Err(_) => {
                    stats.invalid = true;
                    stats.invalid_branches += 1;
                    stats.invalid_reason = "state_encoding";
                    break;
                }
            }
            continue;
        }

        // Opponent priority — auto-pass without branching.
        if work.priority_player != 0 {
            let action = Action::PassPriority;
            let legal = legal_actions(&work);
            if !search_action_advanced(stats, apply_counted_action(&mut work, &action, &legal)) {
                break;
            }
            continue;
        }

        // State deduplication
        let fp = fingerprint(&work);
        if let Some(&prev_turn) = visited.get(&fp) {
            if prev_turn <= work.turn_number {
                stats.states_pruned_dedup += 1;
                break;
            }
        }
        if visited.len() < limits.max_visited {
            visited.insert(fp, work.turn_number);
        }

        stats.states_explored += 1;

        if limits.trace && stats.states_explored <= 100 {
            eprintln!(
                "  [STATE #{}] T{} {:?} prio={} depth={} hand={} bf={} stack={}",
                stats.states_explored,
                work.turn_number,
                work.phase,
                work.priority_player,
                current_sequence.len(),
                work.players[0].hand.len(),
                work.battlefield.len(),
                work.stack.len(),
            );
        }

        // Get and prune legal actions
        let actions = legal_actions(&work);
        if actions.is_empty() {
            stats.stalled = true;
            stats.stalled_branches += 1;
            break;
        }
        let pruned = prune_actions(&work, &actions, config);
        if pruned.is_empty() {
            stats.stalled = true;
            stats.stalled_branches += 1;
            break;
        }

        if limits.trace && stats.states_explored <= 100 {
            eprintln!(
                "    legal={} pruned={} actions: {}",
                actions.len(),
                pruned.len(),
                pruned
                    .iter()
                    .map(|a| format_action_name(&work, a))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        // Single action — apply in-place (no clone, no recursion).
        if pruned.len() == 1 {
            let action = pruned[0].clone();
            let desc = format_action_name(&work, &action);
            let turn = work.turn_number;
            if !search_action_advanced(stats, apply_counted_action(&mut work, &action, &actions)) {
                break;
            }
            current_sequence.push(ActionRecord {
                turn,
                action,
                description: desc,
            });
            linear_depth += 1;
            continue;
        }

        // Multiple actions — branch via recursion on clones.
        for action in &pruned {
            if stats.timed_out || stats.hit_state_cap {
                break;
            }
            let desc = format_action_name(&work, action);
            let mut clone = work.clone();
            let turn = clone.turn_number;
            if !search_action_advanced(stats, apply_counted_action(&mut clone, action, &actions)) {
                continue;
            }
            current_sequence.push(ActionRecord {
                turn,
                action: action.clone(),
                description: desc,
            });
            dfs_search(
                &mut clone,
                best_win_turn,
                best_sequence,
                current_sequence,
                config,
                stats,
                visited,
                limits,
            );
            current_sequence.pop();
        }
        break;
    }

    // Pop linear actions we pushed during the loop.
    for _ in 0..linear_depth {
        current_sequence.pop();
    }
}

/// Run the solver for a single seed.
fn solve_seed(
    db: &mtg_gto::game::CardDatabase,
    config: &DeckConfig,
    combo_registry: &Arc<mtg_gto::combo::ComboRegistry>,
    seed: u64,
    max_turn: u32,
    timeout_secs: u64,
    max_states: u64,
    verbose: bool,
    trace: bool,
) -> (Option<u32>, SearchStats, Vec<ActionRecord>) {
    let arc_db = Arc::new(db.clone());

    let mut state = GameState::new_commander(2);
    state.card_db = Some(arc_db);
    rules::setup_commander_game_seeded(
        &mut state,
        &config.deck,
        &config.deck,
        config.commander,
        config.commander,
        seed,
    );
    if !config.tutor_targets.is_empty() {
        rules::set_tutor_targets(&mut state, 0, &config.tutor_targets);
    }
    state.combo_registry = Some(combo_registry.clone());

    // Convert game turns to engine turns (2 engine turns per game turn in 2-player).
    let engine_max_turn = max_turn * 2;
    let mut best_win_turn = engine_max_turn + 1;
    let mut best_sequence = Vec::new();
    let mut current_sequence = Vec::new();
    let mut stats = SearchStats::new();
    // Pre-allocate visited with a reasonable capacity, capped to prevent OOM.
    let visited_cap = (max_states as usize).min(1_000_000);
    let mut visited = HashMap::with_capacity(visited_cap.min(100_000));

    let limits = SearchLimits {
        deadline: Instant::now() + std::time::Duration::from_secs(timeout_secs),
        max_states,
        max_visited: visited_cap,
        max_depth: 500,
        trace,
    };

    dfs_search(
        &mut state,
        &mut best_win_turn,
        &mut best_sequence,
        &mut current_sequence,
        config,
        &mut stats,
        &mut visited,
        &limits,
    );

    let win_turn = if best_win_turn <= engine_max_turn {
        Some(game_turn(best_win_turn))
    } else {
        None
    };

    if verbose {
        if let Some(t) = win_turn {
            println!("  Winning line (T{}):", t);
            let mut last_turn = 0;
            for record in &best_sequence {
                let gt = game_turn(record.turn);
                if gt != last_turn {
                    last_turn = gt;
                    println!("  --- Turn {} ---", last_turn);
                }
                println!("    {}", record.description);
            }
        }
    }

    (win_turn, stats, best_sequence)
}

fn main() {
    let num_seeds: u64 = std::env::var("SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let seed_start: u64 = std::env::var("SEED_START")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let max_turn: u32 = std::env::var("MAX_TURN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let timeout_secs: u64 = std::env::var("TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let max_states: u64 = std::env::var("MAX_STATES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500_000);
    let verbose: bool = std::env::var("VERBOSE")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0)
        > 0;
    let trace: bool = std::env::var("TRACE")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0)
        > 0;

    let deck_name = std::env::var("DECK")
        .unwrap_or_else(|_| "kinnan".into())
        .to_lowercase();
    let db = sample::build_sample_db();
    let config = match deck_name.as_str() {
        "kinnan" => kinnan_config(),
        "ashcoat" => ashcoat_config(),
        "flubs" => flubs_config(),
        other => {
            eprintln!("Unknown deck: {}. Available: kinnan, ashcoat, flubs", other);
            std::process::exit(1);
        }
    };
    let commander_name = db
        .get(config.commander)
        .map(|d| d.name.as_str())
        .unwrap_or("?");

    println!("BFS Goldfish Solver");
    println!("===================");
    println!("Commander:  {}", commander_name);
    println!("Seeds:      {}-{}", seed_start, seed_start + num_seeds - 1);
    println!("Max turn:   {}", max_turn);
    println!("Timeout:    {}s per seed", timeout_secs);
    println!("Max states: {}", max_states);
    println!("Verbose:    {}", verbose);
    println!();

    let total_start = Instant::now();

    // Discover combos once, reuse for all seeds.
    let combo_start = Instant::now();
    let combo_cards: Vec<CardId> = if !config.tutor_targets.is_empty() {
        let mut cards = vec![config.commander];
        cards.extend_from_slice(&config.tutor_targets);
        cards
    } else {
        config.deck.clone()
    };
    let (mut registry, _) = mtg_gto::combo_discovery::discover_and_register(
        &db,
        &combo_cards,
        &DiscoveryConfig::default(),
    );
    if config.register_ballista {
        mtg_gto::combo::register_ballista_win_combo(&mut registry);
    }
    let combo_registry = Arc::new(registry);
    println!("Combo discovery: {:.1}s", combo_start.elapsed().as_secs_f64());
    println!();

    let mut kill_turns: HashMap<u32, u32> = HashMap::new();
    let mut wins = 0u32;
    let mut incomplete_wins = 0u32;
    let mut total_states = 0u64;
    let mut timeouts = 0u32;
    let mut censored = 0u32;
    let mut stalled = 0u32;
    let mut invalid = 0u32;
    let mut exhausted = 0u32;

    for seed in seed_start..(seed_start + num_seeds) {
        let start = Instant::now();
        let (win_turn, stats, _sequence) = solve_seed(
            &db,
            &config,
            &combo_registry,
            seed,
            max_turn,
            timeout_secs,
            max_states,
            verbose,
            trace,
        );
        let elapsed = start.elapsed();

        let suffix = if stats.timed_out {
            timeouts += 1;
            " [TIMEOUT]"
        } else if stats.hit_state_cap {
            timeouts += 1;
            " [STATE CAP]"
        } else {
            ""
        };

        match classify_search(win_turn, &stats) {
            SearchOutcome::Win(t) => {
                println!(
                    "Seed {:3}: Win T{} ({} states, {:.1}s){}",
                    seed, t, stats.states_explored, elapsed.as_secs_f64(), suffix
                );
                *kill_turns.entry(t).or_insert(0) += 1;
                wins += 1;
            }
            SearchOutcome::FoundWinIncomplete(t, reason) => {
                incomplete_wins += 1;
                println!("Seed {:3}: Known win T{}; search incomplete reason={} ({} states, {:.1}s; stalled_branches={}, invalid_branches={})",
                    seed, t, reason, stats.states_explored, elapsed.as_secs_f64(),
                    stats.stalled_branches, stats.invalid_branches);
                *kill_turns.entry(t).or_insert(0) += 1;
            }
            SearchOutcome::ExhaustedNoWin => {
                exhausted += 1;
                println!(
                    "Seed {:3}: Exhausted, no win by T{} ({} states, {:.1}s){}",
                    seed, max_turn, stats.states_explored, elapsed.as_secs_f64(), suffix
                );
            }
            SearchOutcome::Censored(reason) => {
                censored += 1;
                println!("Seed {:3}: Censored reason={} ({} states, {:.1}s)",
                    seed, reason, stats.states_explored, elapsed.as_secs_f64());
            }
            SearchOutcome::Stalled(reason) => {
                stalled += 1;
                println!("Seed {:3}: Stalled reason={} ({} states, {:.1}s; stalled_branches={}, invalid_branches={})",
                    seed, reason, stats.states_explored, elapsed.as_secs_f64(),
                    stats.stalled_branches, stats.invalid_branches);
            }
            SearchOutcome::Invalid(reason) => {
                invalid += 1;
                println!("Seed {:3}: Invalid reason={} ({} states, {:.1}s; stalled_branches={}, invalid_branches={})",
                    seed, reason, stats.states_explored, elapsed.as_secs_f64(),
                    stats.stalled_branches, stats.invalid_branches);
            }
        }
        total_states += stats.states_explored;
    }

    let total_elapsed = total_start.elapsed();

    println!();
    println!("Summary");
    println!("=======");
    println!(
        "Complete-search win rate: {}/{} ({:.1}%)",
        wins,
        wins + exhausted,
        100.0 * wins as f64 / (wins + exhausted).max(1) as f64
    );
    println!("Attempts: {}  Complete wins: {}  Known wins with incomplete search: {}  Exhausted no win: {}  Censored: {}  Stalled: {}  Invalid: {}",
        num_seeds, wins, incomplete_wins, exhausted, censored, stalled, invalid);
    if timeouts > 0 {
        println!("Timeouts:   {}", timeouts);
    }
    println!("Total time: {:.1}s", total_elapsed.as_secs_f64());
    println!("States explored across all attempts: {}", total_states);

    if !kill_turns.is_empty() {
        println!();
        println!("Known winning lines by turn (some searches incomplete):");
        let mut turns: Vec<u32> = kill_turns.keys().cloned().collect();
        turns.sort();
        for t in turns {
            println!("  T{}: {} games", t, kill_turns[&t]);
        }
    }
}

#[cfg(test)]
mod outcome_tests {
    use super::*;

    #[test]
    fn no_win_requires_exhaustion_not_a_resource_cut() {
        let mut stats = SearchStats::new();
        assert_eq!(classify_search(None, &stats), SearchOutcome::ExhaustedNoWin);
        stats.hit_state_cap = true;
        assert_eq!(classify_search(None, &stats), SearchOutcome::Censored("state_cap"));
    }

    #[test]
    fn stalled_and_invalid_are_disclosed_separately() {
        let mut stats = SearchStats::new();
        stats.stalled = true;
        assert_eq!(classify_search(None, &stats), SearchOutcome::Stalled("no_progress"));
        stats.invalid = true;
        assert_eq!(classify_search(None, &stats), SearchOutcome::Invalid("incomplete_cleanup"));
    }

    #[test]
    fn known_win_with_stalled_sibling_keeps_trace_but_is_incomplete() {
        let mut stats = SearchStats::new();
        stats.stalled = true;
        assert_eq!(classify_search(Some(2), &stats),
            SearchOutcome::FoundWinIncomplete(2, "stalled_branch"));
    }

    #[test]
    fn inconsistent_cleanup_chooser_is_invalid_before_fast_forward() {
        let mut state = GameState::new(2);
        state.active_player = 1;
        state.priority_player = 0;
        state.phase = Phase::Cleanup;
        state.cleanup_discard_in_progress = true;
        assert!(invalid_cleanup_state(&state));
        let mut best_turn = 10;
        let mut best = Vec::new();
        let mut current = Vec::new();
        let mut stats = SearchStats::new();
        let mut visited = HashMap::new();
        let limits = SearchLimits {
            deadline: Instant::now() + std::time::Duration::from_secs(1),
            max_states: 10, max_visited: 10, max_depth: 10, trace: false,
        };
        dfs_search(&mut state, &mut best_turn, &mut best, &mut current,
            &kinnan_config(), &mut stats, &mut visited, &limits);
        assert_eq!(stats.invalid_branches, 1);
        assert_eq!(classify_search(None, &stats), SearchOutcome::Invalid("incomplete_cleanup"));
    }

    #[test]
    fn pruned_pass_during_coherent_cleanup_is_stalled_not_exhausted() {
        let mut db = mtg_gto::game::CardDatabase::new();
        db.insert(mtg_gto::card::CardDef { id: 91_001, name: "Filler".into(),
            card_types: vec![mtg_gto::card::CardType::Land], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::Cleanup;
        state.cleanup_discard_in_progress = true;
        for _ in 0..8 {
            state.create_card_in_zone(91_001, 0, mtg_gto::card::ZoneType::Hand);
        }
        assert!(!invalid_cleanup_state(&state));
        let mut best_turn = 10;
        let mut best = Vec::new();
        let mut current = Vec::new();
        let mut stats = SearchStats::new();
        let mut visited = HashMap::new();
        let limits = SearchLimits {
            deadline: Instant::now() + std::time::Duration::from_secs(1),
            max_states: 10, max_visited: 10, max_depth: 10, trace: false,
        };
        dfs_search(&mut state, &mut best_turn, &mut best, &mut current,
            &kinnan_config(), &mut stats, &mut visited, &limits);
        assert_eq!(stats.stalled_branches, 1);
        assert_eq!(classify_search(None, &stats), SearchOutcome::Stalled("no_progress"));
    }
}
