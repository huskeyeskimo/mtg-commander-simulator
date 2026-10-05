//! Phase 1B.3 + 2B.2/2B.3 — MCCFR Training Engine
//!
//! Implements the external sampling variant of Monte Carlo Counterfactual
//! Regret Minimization (MCCFR). In each iteration, one player is the
//! "traverser" — all their actions are explored — while the opponent's
//! actions and chance outcomes are sampled according to the current strategy.
//!
//! # Algorithm Overview
//!
//! ```text
//! for each iteration:
//!   for each player p:
//!     traverse(root_state, p)
//!
//! traverse(state, traverser):
//!   if terminal(state): return utility(state, traverser)
//!   if chance(state): sample outcome, recurse
//!
//!   player = to_act(state)
//!   actions = legal_actions_abstracted(state)
//!   canonical = actions.map(|a| canonicalize(a, state))
//!   info_set = InformationSet::from_view(state.visible_state(player))
//!   strategy = regret_match(info_set, canonical)
//!
//!   if player == traverser:
//!     // Explore ALL actions
//!     for each action a:
//!       child = apply(state.clone(), a)
//!       util[a] = traverse(child, traverser)
//!     node_util = sum(strategy[a] * util[a])
//!     for each action a:
//!       regret[canonical[a]] += util[a] - node_util
//!     return node_util
//!   else:
//!     // Sample ONE action from opponent's strategy
//!     a = sample(strategy)
//!     child = apply(state.clone(), a)
//!     return traverse(child, traverser)
//! ```
//!
//! # Key Design Decisions
//!
//! - Uses `legal_actions_abstracted()` (bucketed combat) to bound branching factor
//! - Uses `canonicalize()` to map concrete actions to stable identifiers for
//!   regret table keying — actions are identified by card identity, not ObjectId
//! - Skips CFR nodes with only one legal action (trivial pass-through)
//! - Depth-limited: returns heuristic evaluation after `max_depth` plies
//! - Only increments depth at multi-action decision nodes, not forced passes
//!
//! # Phase 2B Additions
//!
//! - **Information set abstraction** (`InfoSetAbstraction` trait) for state space reduction
//! - **Parallel training** (`train_parallel`) via rayon with sharded regret tables
//! - **Depth-limited rollouts** — play out games with a rollout strategy beyond depth limit
//! - **Checkpointing** — serialize regret tables to disk every N iterations

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use rand::Rng;

use crate::action::canonical::canonicalize_actions;
use crate::action::{legal_actions, legal_actions_abstracted, Action};
use crate::game::{GameState, PlayerIndex};
use crate::info_set::{IdentityAbstraction, InfoSetAbstraction, InformationSet};
use crate::rules;
use crate::solver::{sample_from_distribution, RegretTable};
use crate::strategy::Strategy;

/// Configuration for MCCFR training.
#[derive(Debug, Clone)]
pub struct McfrConfig {
    /// Maximum depth (decision points) before falling back to heuristic evaluation.
    /// 0 = unlimited (full game tree).
    pub max_depth: u32,
    /// Maximum actions before declaring a draw (prevents infinite loops).
    pub max_actions: u32,
    /// Maximum recursive nodes explored per iteration in goldfish traversal.
    /// When exceeded, remaining branches fall back to heuristic evaluation.
    /// 0 = unlimited. Prevents exponential blowup with high-branching hands.
    pub max_nodes_per_iteration: u32,
}

/// Default maximum actions before declaring a draw in MCCFR training.
/// Prevents infinite loops while being high enough for realistic games.
pub const DEFAULT_MAX_ACTIONS: u32 = 10_000;

/// Returns the number of threads in the rayon thread pool.
///
/// Use this as the default `num_shards` for parallel training functions,
/// rather than calling `rayon::current_num_threads()` directly from callers.
pub fn default_num_shards() -> u32 {
    rayon::current_num_threads() as u32
}

impl Default for McfrConfig {
    fn default() -> Self {
        McfrConfig {
            max_depth: 0, // unlimited
            max_actions: DEFAULT_MAX_ACTIONS,
            max_nodes_per_iteration: 0, // unlimited
        }
    }
}

/// Rollout mode for depth-limited evaluation (Phase 2B.3).
///
/// When the MCCFR traversal reaches its depth limit, instead of using
/// a static heuristic, it can play out the rest of the game with a
/// simpler strategy and use the game outcome as the evaluation.
#[derive(Debug)]
pub enum RolloutMode {
    /// Use the static heuristic (life + board presence). Phase 1B default.
    Heuristic,
    /// Play out with the given strategy pair for up to `max_rollout_actions` actions.
    Strategy {
        max_rollout_actions: u32,
    },
}

/// Extended configuration for Phase 2B training.
pub struct TrainConfig<'a> {
    /// Base MCCFR configuration.
    pub mccfr: McfrConfig,
    /// Information set abstraction to use.
    pub abstraction: &'a dyn InfoSetAbstraction,
    /// Rollout mode for depth-limited evaluation.
    pub rollout_mode: RolloutMode,
    /// Rollout strategies (player 0, player 1) — used when rollout_mode is Strategy.
    pub rollout_strategies: Option<(&'a dyn Strategy, &'a dyn Strategy)>,
    /// Checkpoint interval (serialize tables every N iterations, 0 = no checkpoint).
    pub checkpoint_interval: u32,
    /// Checkpoint directory path.
    pub checkpoint_dir: Option<String>,
}

impl<'a> Default for TrainConfig<'a> {
    fn default() -> Self {
        TrainConfig {
            mccfr: McfrConfig::default(),
            abstraction: &IdentityAbstraction,
            rollout_mode: RolloutMode::Heuristic,
            rollout_strategies: None,
            checkpoint_interval: 0,
            checkpoint_dir: None,
        }
    }
}

/// Run one complete MCCFR iteration: traverse for both players.
///
/// This is the main entry point for the training loop. Each call
/// performs two traversals (one per player), updating regret tables
/// for both players.
pub fn run_iteration(
    initial_state: &GameState,
    regret_tables: &mut [RegretTable; 2],
    config: &McfrConfig,
) {
    run_iteration_with_abstraction(initial_state, regret_tables, config, &IdentityAbstraction, &RolloutMode::Heuristic, None)
}

/// Run one MCCFR iteration with info set abstraction and rollout support.
pub fn run_iteration_with_abstraction(
    initial_state: &GameState,
    regret_tables: &mut [RegretTable; 2],
    config: &McfrConfig,
    abstraction: &dyn InfoSetAbstraction,
    rollout_mode: &RolloutMode,
    rollout_strategies: Option<(&dyn Strategy, &dyn Strategy)>,
) {
    for traverser in 0..2 {
        let state = initial_state.clone();
        let mut rng = rand::thread_rng();
        traverse(
            state,
            traverser,
            regret_tables,
            config,
            abstraction,
            rollout_mode,
            rollout_strategies,
            0,
            0,
            &mut rng,
        );
    }
}

/// Recursive MCCFR traversal.
///
/// Returns the counterfactual utility for the `traverser` at this node.
fn traverse(
    mut state: GameState,
    traverser: PlayerIndex,
    regret_tables: &mut [RegretTable; 2],
    config: &McfrConfig,
    abstraction: &dyn InfoSetAbstraction,
    rollout_mode: &RolloutMode,
    rollout_strategies: Option<(&dyn Strategy, &dyn Strategy)>,
    mut depth: u32,
    mut actions_taken: u32,
    rng: &mut impl Rng,
) -> f64 {
    loop {
        // Terminal check: game over
        if state.game_over {
            return terminal_utility(&state, traverser);
        }

        // Action limit: use heuristic evaluation
        if actions_taken >= config.max_actions {
            return heuristic_utility(&state, traverser);
        }

        let player = state.priority_player;
        let actions = legal_actions_abstracted(&state);

        // No actions available — pass priority (not a decision node)
        // Loop instead of recursing to avoid stack overflow at high depth.
        if actions.is_empty() {
            rules::apply_action(&mut state, &Action::PassPriority);
            actions_taken += 1;
            continue;
        }

        // Only one legal action — no decision to make, skip CFR node
        if actions.len() == 1 {
            rules::apply_action(&mut state, &actions[0]);
            actions_taken += 1;
            continue;
        }

        // Depth limit at multi-action decision nodes
        if config.max_depth > 0 && depth >= config.max_depth {
            return evaluate_at_depth_limit(&state, traverser, rollout_mode, rollout_strategies);
        }

        // Canonicalize all legal actions for stable regret table keying
        let view = state.visible_state(player);
        let normalized = InformationSet::normalize_retained_view(&view);
        let canonical_actions = canonicalize_actions(&actions, &state, &normalized);
        let info_set = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized);
        let info_hash = abstraction.abstract_info_set(&info_set);

        // Get or create regret table entry and compute current strategy
        let strategy = {
            let entry = regret_tables[player].get_or_create(info_hash);
            entry.current_strategy(&canonical_actions)
        };

        if player == traverser {
            // Traverser node: explore ALL actions, compute counterfactual regret
            let num_actions = actions.len();
            let mut action_utilities = vec![0.0f64; num_actions];

            for (i, action) in actions.iter().enumerate() {
                let mut child_state = state.clone();
                rules::apply_action(&mut child_state, action);
                action_utilities[i] = traverse(
                    child_state,
                    traverser,
                    regret_tables,
                    config,
                    abstraction,
                    rollout_mode,
                    rollout_strategies,
                    depth + 1,
                    actions_taken + 1,
                    rng,
                );
            }

            // Expected utility under current strategy
            let node_utility: f64 = strategy
                .iter()
                .zip(action_utilities.iter())
                .map(|(&s, &u)| s * u)
                .sum();

            // Update cumulative regret and strategy for each action via canonical key
            let entry = regret_tables[player].get_or_create(info_hash);
            for (i, ca) in canonical_actions.iter().enumerate() {
                let action_entry = entry.get_or_create_action(ca);
                action_entry.cumulative_regret += action_utilities[i] - node_utility;
                action_entry.cumulative_strategy += strategy[i];
            }
            entry.visit_count += 1;

            return node_utility;
        } else {
            // Opponent node: sample ONE action from their current strategy
            let action_idx = sample_from_distribution(&strategy, rng);

            // Update opponent's cumulative strategy via canonical keys
            let entry = regret_tables[player].get_or_create(info_hash);
            for (i, ca) in canonical_actions.iter().enumerate() {
                entry.get_or_create_action(ca).cumulative_strategy += strategy[i];
            }
            entry.visit_count += 1;

            rules::apply_action(&mut state, &actions[action_idx]);
            depth += 1;
            actions_taken += 1;
            continue;
        }
    }
}

/// Evaluate a position at the depth limit using either heuristic or rollout.
fn evaluate_at_depth_limit(
    state: &GameState,
    traverser: PlayerIndex,
    rollout_mode: &RolloutMode,
    rollout_strategies: Option<(&dyn Strategy, &dyn Strategy)>,
) -> f64 {
    match rollout_mode {
        RolloutMode::Heuristic => heuristic_utility(state, traverser),
        RolloutMode::Strategy { max_rollout_actions } => {
            rollout_utility(state, traverser, rollout_strategies, *max_rollout_actions)
        }
    }
}

/// Play out the game from the current state using rollout strategies
/// and return +1/-1/0 based on the outcome (or heuristic if not terminal).
///
/// Note: rollouts use `legal_actions()` (full action space, not abstracted)
/// because rollout speed matters more than matching training-time bucketing.
/// The strategy's `choose_action()` handles its own action selection.
fn rollout_utility(
    state: &GameState,
    traverser: PlayerIndex,
    rollout_strategies: Option<(&dyn Strategy, &dyn Strategy)>,
    max_actions: u32,
) -> f64 {
    let (strat0, strat1) = match rollout_strategies {
        Some((s0, s1)) => (s0, s1),
        None => return heuristic_utility(state, traverser),
    };

    let mut rollout_state = state.clone();
    let mut actions_taken: u32 = 0;

    while !rollout_state.game_over && actions_taken < max_actions {
        let player = rollout_state.priority_player;
        let actions = legal_actions(&rollout_state);

        if actions.is_empty()
            || (actions.len() == 1 && actions[0] == Action::PassPriority)
        {
            rules::apply_action(&mut rollout_state, &Action::PassPriority);
            actions_taken += 1;
            continue;
        }

        let strategy: &dyn Strategy = if player == 0 { strat0 } else { strat1 };
        let action = strategy.choose_action(&rollout_state, player);
        rules::apply_action(&mut rollout_state, &action);
        actions_taken += 1;
    }

    if rollout_state.game_over {
        terminal_utility(&rollout_state, traverser)
    } else {
        // Rollout didn't finish — fall back to heuristic
        heuristic_utility(&rollout_state, traverser)
    }
}

/// Terminal utility: +1 for win, -1 for loss, 0 for draw.
fn terminal_utility(state: &GameState, player: PlayerIndex) -> f64 {
    match state.winner {
        Some(w) if w == player => 1.0,
        Some(_) => -1.0,
        None => 0.0,
    }
}

/// Heuristic evaluation when depth limit is reached.
///
/// Uses a multi-factor evaluation:
/// - Life total advantage (normalized)
/// - Board presence (creature power differential)
/// - Combo proximity bonus (reward shaping for having combo pieces together)
///
/// The combo proximity bonus gives partial credit for assembling combo pieces,
/// incentivizing the solver to collect pieces even before the full combo is
/// available. This is the "reward shaping" component described in the design.
fn heuristic_utility(state: &GameState, player: PlayerIndex) -> f64 {
    use crate::game::GameFormat;

    let opp = state.opponent(player);
    let my_life = state.players[player].life as f64;
    let opp_life = state.players[opp].life as f64;

    // Life advantage, normalized by format starting life (20 for Standard, 40 for Commander)
    let starting_life = match state.format {
        GameFormat::Commander => 40.0,
        _ => 20.0,
    };
    let life_diff = my_life - opp_life;
    let normalized = (life_diff / starting_life).clamp(-1.0, 1.0);

    // Board presence bonus (using layer engine for accurate power)
    let my_power: i32 = state
        .creatures_controlled_by(player)
        .iter()
        .map(|&id| state.effective_power(id))
        .sum();
    let opp_power: i32 = state
        .creatures_controlled_by(opp)
        .iter()
        .map(|&id| state.effective_power(id))
        .sum();

    let board_diff = (my_power - opp_power) as f64 / 10.0;
    let board_normalized = board_diff.clamp(-0.5, 0.5);

    // Combo proximity bonus: partial credit for having combo pieces on
    // the battlefield. Differential: my proximity minus opponent's.
    let combo_bonus = if let Some(ref registry) = state.combo_registry {
        let my_proximity = crate::combo::combo_proximity_bonus(state, player, registry);
        let opp_proximity = crate::combo::combo_proximity_bonus(state, opp, registry);
        (my_proximity - opp_proximity).clamp(-0.3, 0.3)
    } else {
        0.0
    };

    // Mana pool bonus: having a large mana pool (from combo activation)
    // is worth something — it means the player can cast expensive spells.
    let my_mana = state.players[player].mana_pool.total() as f64;
    let opp_mana = state.players[opp].mana_pool.total() as f64;
    let mana_bonus = ((my_mana - opp_mana) / 50.0).clamp(-0.2, 0.2);

    // Weighted combination with combo awareness
    let base = normalized * 0.6 + board_normalized * 0.2;
    (base + combo_bonus + mana_bonus).clamp(-1.0, 1.0)
}

/// Training loop: run many MCCFR iterations and return the trained regret tables.
///
/// This is the high-level training function for the minimal scenario.
/// Uses identity abstraction and heuristic evaluation (Phase 1B compatibility).
pub fn train(
    initial_state: &GameState,
    num_iterations: u32,
    config: &McfrConfig,
) -> [RegretTable; 2] {
    let mut regret_tables = [RegretTable::new(), RegretTable::new()];

    for _ in 0..num_iterations {
        run_iteration(initial_state, &mut regret_tables, config);
    }

    regret_tables
}

/// Extended training with info set abstraction, rollouts, and checkpointing.
///
/// Phase 2B training function that supports all new features.
pub fn train_extended(
    initial_state: &GameState,
    num_iterations: u32,
    train_config: &TrainConfig,
) -> [RegretTable; 2] {
    let mut regret_tables = [RegretTable::new(), RegretTable::new()];

    for i in 0..num_iterations {
        run_iteration_with_abstraction(
            initial_state,
            &mut regret_tables,
            &train_config.mccfr,
            train_config.abstraction,
            &train_config.rollout_mode,
            train_config.rollout_strategies,
        );

        // Checkpoint at configured intervals
        if train_config.checkpoint_interval > 0
            && (i + 1) % train_config.checkpoint_interval == 0
        {
            if let Some(ref dir) = train_config.checkpoint_dir {
                let _ = save_checkpoint(&regret_tables, dir, i + 1);
            }
        }
    }

    regret_tables
}

// =========================================================================
// Phase 2B.2 — Parallel MCCFR Training
// =========================================================================

/// Train MCCFR in parallel using rayon.
///
/// Each thread runs independent iterations on its own sharded regret tables.
/// After all iterations complete, the sharded tables are merged by summing
/// cumulative regret and cumulative strategy values. This avoids lock
/// contention while producing a valid CFR result (regret sums are linear).
///
/// # Arguments
///
/// * `initial_state` — Starting game state for each iteration.
/// * `num_iterations` — Total iterations to run across all threads.
/// * `num_shards` — Number of independent table shards (typically = number of CPU cores).
/// * `train_config` — Extended training configuration.
pub fn train_parallel(
    initial_state: &GameState,
    num_iterations: u32,
    num_shards: u32,
    train_config: &TrainConfig,
) -> [RegretTable; 2] {
    use rayon::prelude::*;

    let iterations_per_shard = num_iterations / num_shards;
    let remainder = num_iterations % num_shards;

    // Run shards in parallel, each producing its own regret tables
    let shard_results: Vec<[RegretTable; 2]> = (0..num_shards)
        .into_par_iter()
        .map(|shard_idx| {
            let iters = if shard_idx < remainder {
                iterations_per_shard + 1
            } else {
                iterations_per_shard
            };

            // Skip zero-work shards (happens when num_shards > num_iterations)
            if iters == 0 {
                return [RegretTable::new(), RegretTable::new()];
            }

            let mut tables = [RegretTable::new(), RegretTable::new()];

            for i in 0..iters {
                run_iteration_with_abstraction(
                    initial_state,
                    &mut tables,
                    &train_config.mccfr,
                    train_config.abstraction,
                    &train_config.rollout_mode,
                    train_config.rollout_strategies,
                );

                // Per-shard checkpointing — writes to {dir}/shard_{idx}/.
                // Note: these per-shard checkpoints are not automatically cleaned
                // up after merging. For long runs, consider only checkpointing
                // the final merged result via train_extended() instead.
                if train_config.checkpoint_interval > 0
                    && (i + 1) % train_config.checkpoint_interval == 0
                {
                    if let Some(ref dir) = train_config.checkpoint_dir {
                        let shard_dir = format!("{}/shard_{}", dir, shard_idx);
                        let _ = save_checkpoint(&tables, &shard_dir, i + 1);
                    }
                }
            }

            tables
        })
        .collect();

    // Merge all shard results into a single pair of regret tables
    merge_regret_tables(&shard_results)
}

/// Merge multiple pairs of regret tables by summing cumulative values.
///
/// CFR regret and strategy sums are linear — summing independent
/// traversals produces a valid combined result.
fn merge_regret_tables(shard_results: &[[RegretTable; 2]]) -> [RegretTable; 2] {
    let mut merged = [RegretTable::new(), RegretTable::new()];

    for shard in shard_results {
        for player in 0..2 {
            for (&info_hash, shard_data) in &shard[player].data {
                let merged_data = merged[player].get_or_create(info_hash);
                merged_data.visit_count += shard_data.visit_count;

                for (action, shard_entry) in &shard_data.action_data {
                    let merged_entry = merged_data.get_or_create_action(action);
                    merged_entry.cumulative_regret += shard_entry.cumulative_regret;
                    merged_entry.cumulative_strategy += shard_entry.cumulative_strategy;
                }
            }
        }
    }

    merged
}

// =========================================================================
// Checkpointing
// =========================================================================

/// Save regret tables to disk as a checkpoint.
pub fn save_checkpoint(
    tables: &[RegretTable; 2],
    dir: &str,
    iteration: u32,
) -> Result<(), String> {
    let path = Path::new(dir);
    std::fs::create_dir_all(path).map_err(|e| format!("mkdir: {}", e))?;

    for (i, table) in tables.iter().enumerate() {
        let filename = path.join(format!("player_{}_iter_{}.bin", i, iteration));
        let bytes = table.to_bytes().map_err(|e| format!("serialize: {}", e))?;
        std::fs::write(&filename, bytes).map_err(|e| format!("write: {}", e))?;
    }

    Ok(())
}

/// Load a checkpoint from disk.
pub fn load_checkpoint(
    dir: &str,
    iteration: u32,
) -> Result<[RegretTable; 2], String> {
    let path = Path::new(dir);
    let mut tables = [RegretTable::new(), RegretTable::new()];

    for i in 0..2 {
        let filename = path.join(format!("player_{}_iter_{}.bin", i, iteration));
        let bytes = std::fs::read(&filename).map_err(|e| format!("read: {}", e))?;
        tables[i] = RegretTable::from_bytes(&bytes).map_err(|e| format!("deserialize: {}", e))?;
    }

    Ok(tables)
}

// =========================================================================
// Parallel Goldfish MCCFR Training
// =========================================================================

/// Train goldfish MCCFR in parallel using rayon.
///
/// Each thread runs independent iterations on its own sharded regret table.
/// After all iterations complete, the sharded tables are merged by summing
/// cumulative regret and cumulative strategy values — the same approach
/// used by `train_parallel` for 2-player MCCFR.
///
/// # Arguments
///
/// * `initial_state` — Starting game state for each iteration.
/// * `num_iterations` — Total iterations to run across all threads.
/// * `num_shards` — Number of independent table shards (typically = number of CPU cores).
/// * `config` — MCCFR configuration.
pub fn train_goldfish_parallel(
    initial_state: &GameState,
    num_iterations: u32,
    num_shards: u32,
    config: &McfrConfig,
) -> [RegretTable; 2] {
    train_goldfish_parallel_with_progress(
        initial_state,
        num_iterations,
        num_shards,
        config,
        &IdentityAbstraction,
        0,
        None,
        None,
        |_, _, _| {},
    )
}

/// Train goldfish MCCFR in parallel with information set abstraction.
///
/// # Arguments
///
/// * `initial_state` — Starting game state for each iteration.
/// * `num_iterations` — Total iterations to run across all threads.
/// * `num_shards` — Number of independent table shards (typically = number of CPU cores).
/// * `config` — MCCFR configuration.
/// * `abstraction` — Information set abstraction to use.
/// * `pilot` — Which player is optimized (typically 0).
pub fn train_goldfish_parallel_with_abstraction(
    initial_state: &GameState,
    num_iterations: u32,
    num_shards: u32,
    config: &McfrConfig,
    abstraction: &dyn InfoSetAbstraction,
    pilot: PlayerIndex,
) -> [RegretTable; 2] {
    train_goldfish_parallel_with_progress(
        initial_state,
        num_iterations,
        num_shards,
        config,
        abstraction,
        pilot,
        None,
        None,
        |_, _, _| {},
    )
}

/// Train goldfish MCCFR in parallel with progress reporting and checkpointing.
///
/// An `AtomicU32` progress counter is shared across all shards. Each thread
/// increments the counter after completing an iteration, so the callback can
/// report aggregate progress without lock contention.
///
/// The callback receives `(completed_iterations, total_iterations, &progress_counter)`.
/// Because tables are shard-private, the callback cannot inspect regret tables
/// mid-training — use checkpointing for intermediate snapshots.
///
/// # Arguments
///
/// * `initial_state` — Starting game state for each iteration.
/// * `num_iterations` — Total iterations to run across all threads.
/// * `num_shards` — Number of independent table shards.
/// * `config` — MCCFR configuration.
/// * `abstraction` — Information set abstraction to use.
/// * `pilot` — Which player is optimized (typically 0).
/// * `checkpoint_interval` — If `Some(n)`, save per-shard checkpoints every n iterations.
/// * `checkpoint_dir` — Directory for checkpoints.
/// * `on_progress` — Called from each shard after each iteration with
///   `(completed_globally, total, &AtomicU32)`.
pub fn train_goldfish_parallel_with_progress<F>(
    initial_state: &GameState,
    num_iterations: u32,
    num_shards: u32,
    config: &McfrConfig,
    abstraction: &dyn InfoSetAbstraction,
    pilot: PlayerIndex,
    checkpoint_interval: Option<u32>,
    checkpoint_dir: Option<&str>,
    on_progress: F,
) -> [RegretTable; 2]
where
    F: Fn(u32, u32, &AtomicU32) + Send + Sync,
{
    use rayon::prelude::*;

    let iterations_per_shard = num_iterations / num_shards;
    let remainder = num_iterations % num_shards;
    let progress = AtomicU32::new(0);

    let shard_results: Vec<[RegretTable; 2]> = (0..num_shards)
        .into_par_iter()
        .map(|shard_idx| {
            let iters = if shard_idx < remainder {
                iterations_per_shard + 1
            } else {
                iterations_per_shard
            };

            if iters == 0 {
                return [RegretTable::new(), RegretTable::new()];
            }

            let mut regret_tables = [RegretTable::new(), RegretTable::new()];
            let goldfish = crate::strategy::GoldfishStrategy;

            for i in 0..iters {
                let mut state = initial_state.clone();
                // Reshuffle opening hand each iteration so the solver trains on diverse
                // starting hands rather than memorizing one fixed deal.
                rules::reshuffle_opening_hand(&mut state);
                let mut nodes_visited = 0u32;
                traverse_goldfish(
                    state,
                    &mut regret_tables[pilot as usize],
                    config,
                    abstraction,
                    &goldfish,
                    pilot,
                    0,
                    0,
                    &mut nodes_visited,
                );

                let completed = progress.fetch_add(1, Ordering::Relaxed) + 1;
                on_progress(completed, num_iterations, &progress);

                // Per-shard checkpointing
                if let Some(interval) = checkpoint_interval {
                    if interval > 0 && (i + 1) % interval == 0 {
                        if let Some(dir) = checkpoint_dir {
                            let shard_dir = format!("{}/shard_{}", dir, shard_idx);
                            let _ = save_checkpoint(&regret_tables, &shard_dir, i + 1);
                        }
                    }
                }
            }

            regret_tables
        })
        .collect();

    merge_regret_tables(&shard_results)
}

// =========================================================================
// Basic Parallel MCCFR Training (2-player)
// =========================================================================

/// Train 2-player MCCFR in parallel using rayon with default settings.
///
/// This is the parallel equivalent of `train()` — uses identity abstraction
/// and heuristic evaluation, but distributes iterations across multiple threads.
/// Delegates to `train_parallel` with a default `TrainConfig`.
///
/// # Arguments
///
/// * `initial_state` — Starting game state for each iteration.
/// * `num_iterations` — Total iterations to run across all threads.
/// * `num_shards` — Number of independent table shards (typically = number of CPU cores).
/// * `config` — MCCFR configuration.
pub fn train_parallel_basic(
    initial_state: &GameState,
    num_iterations: u32,
    num_shards: u32,
    config: &McfrConfig,
) -> [RegretTable; 2] {
    let train_config = TrainConfig {
        mccfr: config.clone(),
        ..TrainConfig::default()
    };
    train_parallel(initial_state, num_iterations, num_shards, &train_config)
}

// =========================================================================
// Phase 3B.1 — Warm-Starting from GreedyStrategy Heuristics
// =========================================================================

/// Warm-start regret tables by pre-populating info set entries from greedy
/// play-throughs. This gives the MCCFR solver a "map" of reachable info sets
/// and actions without biasing the regret values.
///
/// Instead of seeding positive regret (which biases the solver away from
/// equilibrium), we only register info sets and their legal actions so that
/// the first real MCCFR iterations don't start from a completely empty table.
/// All regret and strategy values start at zero — the warm-start benefit
/// comes from pre-discovering the reachable game tree via greedy rollouts.
pub fn warm_start_from_greedy(
    initial_state: &GameState,
    num_warmup_games: u32,
    abstraction: &dyn InfoSetAbstraction,
    _warmup_weight: f64, // kept for API compatibility, no longer used for seeding
) -> [RegretTable; 2] {
    use crate::strategy::GreedyStrategy;

    let mut tables = [RegretTable::new(), RegretTable::new()];
    let greedy = GreedyStrategy;

    for _ in 0..num_warmup_games {
        let mut state = initial_state.clone();
        // Reshuffle opening hand so warm-start covers diverse starting hands.
        rules::reshuffle_opening_hand(&mut state);
        let mut actions_taken = 0u32;

        while !state.game_over && actions_taken < 500 {
            let player = state.priority_player;
            let actions = legal_actions_abstracted(&state);

            if actions.len() <= 1 {
                let action = if actions.is_empty() {
                    Action::PassPriority
                } else {
                    actions[0].clone()
                };
                rules::apply_action(&mut state, &action);
                actions_taken += 1;
                continue;
            }

            // Canonicalize actions
            let view = state.visible_state(player);
            let normalized = InformationSet::normalize_retained_view(&view);
            let canonical_actions = canonicalize_actions(&actions, &state, &normalized);
            let info_set = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized);
            let info_hash = abstraction.abstract_info_set(&info_set);

            // Pre-populate the info set entry with all legal actions.
            // Regrets and strategy values remain at zero — no bias introduced.
            let entry = tables[player].get_or_create(info_hash);
            for ca in &canonical_actions {
                entry.get_or_create_action(ca);
            }

            // Play the greedy action to explore realistic game paths
            let greedy_action = greedy.choose_action(&state, player);
            rules::apply_action(&mut state, &greedy_action);
            actions_taken += 1;
        }
    }

    tables
}

/// Train with warm-starting: first seed tables from GreedyStrategy,
/// then run standard MCCFR iterations.
pub fn train_warm_started(
    initial_state: &GameState,
    num_warmup_games: u32,
    num_iterations: u32,
    train_config: &TrainConfig,
) -> [RegretTable; 2] {
    // Phase 1: Warm-start
    let mut regret_tables = warm_start_from_greedy(
        initial_state,
        num_warmup_games,
        train_config.abstraction,
        1.0,
    );

    // Phase 2: Regular MCCFR training
    for i in 0..num_iterations {
        run_iteration_with_abstraction(
            initial_state,
            &mut regret_tables,
            &train_config.mccfr,
            train_config.abstraction,
            &train_config.rollout_mode,
            train_config.rollout_strategies,
        );

        if train_config.checkpoint_interval > 0
            && (i + 1) % train_config.checkpoint_interval == 0
        {
            if let Some(ref dir) = train_config.checkpoint_dir {
                let _ = save_checkpoint(&regret_tables, dir, i + 1);
            }
        }
    }

    regret_tables
}

// =========================================================================
// Phase 3B.2 — Opponent Modeling / Deck Inference
// =========================================================================

/// Bayesian opponent model that updates beliefs about the opponent's deck
/// based on observed actions and revealed cards.
#[derive(Debug, Clone)]
pub struct OpponentModel {
    /// Known deck archetypes with names and prior probabilities.
    pub archetypes: Vec<DeckArchetype>,
    /// Posterior probabilities for each archetype.
    pub posteriors: Vec<f64>,
    /// Cards observed from the opponent.
    pub observed_cards: Vec<u64>,
    /// Likelihood of a card appearing given it IS a signature card for the archetype.
    pub signature_likelihood: f64,
    /// Likelihood of a card appearing given it is NOT a signature card.
    pub non_signature_likelihood: f64,
}

/// A deck archetype for opponent modeling.
#[derive(Debug, Clone)]
pub struct DeckArchetype {
    pub name: String,
    /// Key card IDs that are characteristic of this archetype.
    pub signature_cards: Vec<u64>,
    /// Prior probability of facing this archetype.
    pub prior: f64,
}

impl OpponentModel {
    /// Create a new opponent model with uniform priors and default likelihoods.
    pub fn new(archetypes: Vec<DeckArchetype>) -> Self {
        Self::with_likelihoods(archetypes, 0.8, 0.2)
    }

    /// Create a new opponent model with custom likelihood values.
    /// `sig` is the likelihood when a card IS a signature card for the archetype.
    /// `non_sig` is the likelihood when the card is NOT a signature card.
    pub fn with_likelihoods(archetypes: Vec<DeckArchetype>, sig: f64, non_sig: f64) -> Self {
        let n = archetypes.len();
        let uniform = if n > 0 { 1.0 / n as f64 } else { 1.0 };
        let posteriors = vec![uniform; n];
        OpponentModel {
            archetypes,
            posteriors,
            observed_cards: Vec::new(),
            signature_likelihood: sig,
            non_signature_likelihood: non_sig,
        }
    }

    /// Update beliefs after observing a card from the opponent.
    /// Uses Bayesian updating: P(archetype | card) ∝ P(card | archetype) × P(archetype)
    pub fn observe_card(&mut self, card_id: u64) {
        self.observed_cards.push(card_id);

        if self.archetypes.is_empty() {
            return;
        }

        let sig = self.signature_likelihood;
        let non_sig = self.non_signature_likelihood;
        let likelihoods: Vec<f64> = self.archetypes
            .iter()
            .map(|arch| {
                if arch.signature_cards.contains(&card_id) {
                    sig
                } else {
                    non_sig
                }
            })
            .collect();

        let mut unnormalized: Vec<f64> = self.posteriors
            .iter()
            .zip(likelihoods.iter())
            .map(|(&post, &lik)| post * lik)
            .collect();

        let total: f64 = unnormalized.iter().sum();
        if total > 0.0 {
            for p in &mut unnormalized {
                *p /= total;
            }
            self.posteriors = unnormalized;
        }
    }

    /// Get the most likely archetype.
    pub fn most_likely_archetype(&self) -> Option<(&DeckArchetype, f64)> {
        self.archetypes
            .iter()
            .zip(self.posteriors.iter())
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(arch, &prob)| (arch, prob))
    }

    /// Get the posterior probability distribution.
    pub fn distribution(&self) -> Vec<(&str, f64)> {
        let mut result: Vec<(&str, f64)> = self.archetypes
            .iter()
            .zip(self.posteriors.iter())
            .map(|(arch, &prob)| (arch.name.as_str(), prob))
            .collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        result
    }
}

// =========================================================================
// Phase 3B.3 — Policy Visualization
// =========================================================================

/// Action distribution at a decision point.
#[derive(Debug, Clone)]
pub struct PolicySnapshot {
    pub state_description: String,
    pub phase: String,
    pub turn: u32,
    pub player: PlayerIndex,
    /// Action probabilities: (canonical_action_description, probability).
    pub action_distribution: Vec<(String, f64)>,
    pub visit_count: u64,
}

/// Collect policy snapshots from a trained regret table at key decision points
/// during a sample game.
pub fn collect_policy_snapshots(
    initial_state: &GameState,
    regret_tables: &[RegretTable; 2],
    abstraction: &dyn InfoSetAbstraction,
    max_snapshots: usize,
) -> Vec<PolicySnapshot> {
    let mut snapshots = Vec::new();
    let mut state = initial_state.clone();
    let mut actions_taken = 0u32;

    while !state.game_over && actions_taken < 500 && snapshots.len() < max_snapshots {
        let player = state.priority_player;
        let actions = legal_actions_abstracted(&state);

        if actions.len() <= 1 {
            let action = if actions.is_empty() {
                Action::PassPriority
            } else {
                actions[0].clone()
            };
            rules::apply_action(&mut state, &action);
            actions_taken += 1;
            continue;
        }

        let view = state.visible_state(player);
        let normalized = InformationSet::normalize_retained_view(&view);
        let canonical_actions = canonicalize_actions(&actions, &state, &normalized);
        let info_set = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized);
        let info_hash = abstraction.abstract_info_set(&info_set);

        let (distribution, visit_count) = match regret_tables[player].get(info_hash) {
            Some(data) => (data.average_strategy(&canonical_actions), data.visit_count),
            None => {
                let n = actions.len();
                (vec![1.0 / n as f64; n], 0)
            }
        };

        let action_dist: Vec<(String, f64)> = canonical_actions
            .iter()
            .zip(distribution.iter())
            .map(|(ca, &prob)| (format!("{:?}", ca), prob))
            .collect();

        snapshots.push(PolicySnapshot {
            state_description: format!(
                "Turn {} {:?} P{} life={}/{} hand={} board={}",
                state.turn_number,
                state.phase,
                player,
                state.players[player].life,
                state.players[state.opponent(player)].life,
                state.players[player].hand.len(),
                state.creatures_controlled_by(player).len(),
            ),
            phase: format!("{:?}", state.phase),
            turn: state.turn_number,
            player,
            action_distribution: action_dist,
            visit_count,
        });

        let mut rng = rand::thread_rng();
        let idx = sample_from_distribution(&distribution, &mut rng);
        rules::apply_action(&mut state, &actions[idx]);
        actions_taken += 1;
    }

    snapshots
}

// =========================================================================
// Goldfish MCCFR Training
// =========================================================================

/// Train MCCFR for goldfish (solitaire) mode.
///
/// In goldfish mode, the opponent is completely passive (uses `GoldfishStrategy`),
/// so this is a single-agent optimization problem. Key differences from
/// standard 2-player MCCFR:
///
/// - Only traverses for the `pilot` player. The opponent's strategy is fixed.
/// - When the opponent has priority, `GoldfishStrategy` chooses the action
///   directly — no regret table lookup or strategy sampling needed.
/// - Returns the pilot's regret table (wrapped in the 2-element array
///   for API compatibility; the opponent's table will be empty).
///
/// `pilot` selects which player is optimized (typically 0).
///
/// This converges faster than standard MCCFR because the opponent's
/// action space is collapsed to a single deterministic choice at each node.
pub fn train_goldfish(
    initial_state: &GameState,
    num_iterations: u32,
    config: &McfrConfig,
) -> [RegretTable; 2] {
    train_goldfish_with_abstraction(
        initial_state,
        num_iterations,
        config,
        &IdentityAbstraction,
        0,
    )
}

/// Train goldfish MCCFR with information set abstraction.
///
/// `pilot` selects which player is optimized (typically 0).
pub fn train_goldfish_with_abstraction(
    initial_state: &GameState,
    num_iterations: u32,
    config: &McfrConfig,
    abstraction: &dyn InfoSetAbstraction,
    pilot: PlayerIndex,
) -> [RegretTable; 2] {
    train_goldfish_with_progress(initial_state, num_iterations, config, abstraction, pilot, |_, _, _| {})
}

/// Train goldfish MCCFR with information set abstraction and a progress callback.
///
/// The callback is invoked after each iteration with
/// `(iteration_1based, total_iterations, &regret_tables)`.
pub fn train_goldfish_with_progress<F>(
    initial_state: &GameState,
    num_iterations: u32,
    config: &McfrConfig,
    abstraction: &dyn InfoSetAbstraction,
    pilot: PlayerIndex,
    mut on_progress: F,
) -> [RegretTable; 2]
where
    F: FnMut(u32, u32, &[RegretTable; 2]),
{
    let mut regret_tables = [RegretTable::new(), RegretTable::new()];
    let goldfish = crate::strategy::GoldfishStrategy;

    for i in 0..num_iterations {
        let mut state = initial_state.clone();
        // Reshuffle opening hand each iteration so the solver trains on diverse
        // starting hands rather than memorizing one fixed deal.
        rules::reshuffle_opening_hand(&mut state);
        let mut nodes_visited = 0u32;
        traverse_goldfish(
            state,
            &mut regret_tables[pilot as usize],
            config,
            abstraction,
            &goldfish,
            pilot,
            0,
            0,
            &mut nodes_visited,
        );
        on_progress(i + 1, num_iterations, &regret_tables);
    }

    regret_tables
}

/// Recursive goldfish MCCFR traversal for the pilot player.
///
/// When it's the pilot's turn: explore all actions and accumulate regrets
/// (same as standard MCCFR traverser node).
/// When it's the opponent's turn: use GoldfishStrategy deterministically
/// (no sampling, no regret tracking).
///
/// Unlike the standard `traverse`, the depth limit falls back to
/// `heuristic_utility` unconditionally. Rollout support is intentionally
/// omitted: goldfish games have much lower branching on the opponent
/// side, so the shallow search reaches meaningful terminal states without
/// needing strategy-based rollouts. Adding rollouts here would be
/// straightforward (accept a `RolloutMode` parameter and call
/// `rollout_utility` at the depth limit) if deeper search is ever needed.
fn traverse_goldfish(
    mut state: GameState,
    regret_table: &mut RegretTable,
    config: &McfrConfig,
    abstraction: &dyn InfoSetAbstraction,
    goldfish: &dyn Strategy,
    pilot: PlayerIndex,
    depth: u32,
    mut actions_taken: u32,
    nodes_visited: &mut u32,
) -> f64 {
    loop {
        // Terminal check
        if state.game_over {
            return terminal_utility(&state, pilot);
        }

        // Action limit
        if actions_taken >= config.max_actions {
            return heuristic_utility(&state, pilot);
        }

        // Node budget: fall back to heuristic when iteration budget is exhausted.
        // This prevents exponential blowup with high-branching hands.
        *nodes_visited += 1;
        if config.max_nodes_per_iteration > 0
            && *nodes_visited >= config.max_nodes_per_iteration
        {
            return heuristic_utility(&state, pilot);
        }

        let player = state.priority_player;

        // Opponent (goldfish): deterministic, no regret tracking.
        // Loop instead of recursing to avoid stack overflow at high depth.
        if player != pilot {
            let action = goldfish.choose_action(&state, player);
            rules::apply_action(&mut state, &action);
            // depth unchanged for opponent actions
            actions_taken += 1;
            continue;
        }

        // Pilot: MCCFR decision node
        let actions = legal_actions_abstracted(&state);

        // No actions — pass
        if actions.is_empty() {
            rules::apply_action(&mut state, &Action::PassPriority);
            actions_taken += 1;
            continue;
        }

        // Single action — no decision to make
        if actions.len() == 1 {
            rules::apply_action(&mut state, &actions[0]);
            actions_taken += 1;
            continue;
        }

        // Depth limit — uses heuristic_utility directly; see doc comment above
        // for rationale on omitting rollout support.
        if config.max_depth > 0 && depth >= config.max_depth {
            return heuristic_utility(&state, pilot);
        }

        // Canonicalize actions
        let view = state.visible_state(pilot);
        let normalized = InformationSet::normalize_retained_view(&view);
        let canonical_actions = canonicalize_actions(&actions, &state, &normalized);
        let info_set = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized);
        let info_hash = abstraction.abstract_info_set(&info_set);

        // Get current strategy via regret matching
        let strategy = {
            let entry = regret_table.get_or_create(info_hash);
            entry.current_strategy(&canonical_actions)
        };

        // Explore ALL actions (traverser node)
        let num_actions = actions.len();
        let mut action_utilities = vec![0.0f64; num_actions];

        for (i, action) in actions.iter().enumerate() {
            let mut child_state = state.clone();
            rules::apply_action(&mut child_state, action);
            action_utilities[i] = traverse_goldfish(
                child_state,
                regret_table,
                config,
                abstraction,
                goldfish,
                pilot,
                depth + 1,
                actions_taken + 1,
                nodes_visited,
            );
        }

        // Expected utility under current strategy
        let node_utility: f64 = strategy
            .iter()
            .zip(action_utilities.iter())
            .map(|(&s, &u)| s * u)
            .sum();

        // Update regrets and cumulative strategy
        let entry = regret_table.get_or_create(info_hash);
        for (i, ca) in canonical_actions.iter().enumerate() {
            let action_entry = entry.get_or_create_action(ca);
            action_entry.cumulative_regret += action_utilities[i] - node_utility;
            action_entry.cumulative_strategy += strategy[i];
        }
        entry.visit_count += 1;

        return node_utility;
    }
}

// =========================================================================
// Phase 3B.4 — Multi-Abstraction
// =========================================================================

/// Multi-abstraction that uses different granularity for different game phases.
///
/// Main/combat phases use fine-grained abstraction; other phases use coarser.
pub struct MultiPhaseAbstraction<'a> {
    pub fine: &'a dyn InfoSetAbstraction,
    pub coarse: &'a dyn InfoSetAbstraction,
}

/// Phase indices that use fine-grained abstraction (PreCombatMain through
/// PostCombatMain). These must match the encoding in `info_set::phase_to_u8`.
const STRATEGIC_PHASES: std::ops::RangeInclusive<u8> = 3..=10;
// 3=PreCombatMain, 4=BeginningOfCombat, 5=DeclareAttackers,
// 6=DeclareBlockers, 7=FirstStrikeDamage, 8=CombatDamage,
// 9=EndOfCombat, 10=PostCombatMain

impl<'a> InfoSetAbstraction for MultiPhaseAbstraction<'a> {
    fn abstract_info_set(&self, info_set: &InformationSet) -> u64 {
        if STRATEGIC_PHASES.contains(&info_set.phase) {
            self.fine.abstract_info_set(info_set)
        } else {
            self.coarse.abstract_info_set(info_set)
        }
    }

    fn name(&self) -> &str {
        "MultiPhase"
    }
}

// =========================================================================
// Exploitability
// =========================================================================

/// Compute a rough measure of exploitability by comparing the two players'
/// expected values. In a perfect Nash equilibrium of a zero-sum game,
/// both players' values sum to zero and neither can improve unilaterally.
///
/// Lower values indicate closer approximation to Nash equilibrium.
/// This is an approximation — true exploitability requires a best-response
/// computation, which is expensive.
pub fn approximate_exploitability(regret_tables: &[RegretTable; 2]) -> f64 {
    // Sum of absolute average regret across all info sets
    let mut total_regret = 0.0;
    let mut total_entries = 0;

    for table in regret_tables {
        for (_, data) in &table.data {
            if data.visit_count == 0 {
                continue;
            }
            let avg_regret: f64 = data
                .action_data
                .values()
                .map(|e| e.cumulative_regret.max(0.0))
                .sum::<f64>()
                / data.visit_count as f64;
            total_regret += avg_regret;
            total_entries += 1;
        }
    }

    if total_entries > 0 {
        total_regret / total_entries as f64
    } else {
        f64::INFINITY
    }
}

/// Report training statistics for diagnostics.
pub struct TrainingStats {
    pub total_info_sets: [usize; 2],
    pub total_visits: [u64; 2],
    pub memory_bytes: [usize; 2],
    pub exploitability: f64,
}

/// Compute training statistics from regret tables.
pub fn training_stats(tables: &[RegretTable; 2]) -> TrainingStats {
    let mut stats = TrainingStats {
        total_info_sets: [0; 2],
        total_visits: [0; 2],
        memory_bytes: [0; 2],
        exploitability: approximate_exploitability(tables),
    };

    for i in 0..2 {
        stats.total_info_sets[i] = tables[i].num_info_sets();
        stats.total_visits[i] = tables[i].data.values().map(|d| d.visit_count).sum();
        // Rough memory estimate (lower bound). Does not account for HashMap
        // overhead (load factor, bucket metadata), so actual RSS may be 1.5-2x
        // higher. Suitable for relative comparisons, not absolute sizing.
        let estimated = tables[i].data.iter().map(|(_, d)| {
            8 + 8 + d.action_data.len() * 48
        }).sum();
        stats.memory_bytes[i] = estimated;
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_utility() {
        let mut state = GameState::new(2);
        state.game_over = true;
        state.winner = Some(0);
        assert_eq!(terminal_utility(&state, 0), 1.0);
        assert_eq!(terminal_utility(&state, 1), -1.0);

        state.winner = None;
        assert_eq!(terminal_utility(&state, 0), 0.0);
    }

    #[test]
    fn test_heuristic_utility_equal_life() {
        use crate::card::sample;
        use std::sync::Arc;

        let db = sample::build_sample_db();
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));

        // Equal life totals, no creatures — should be close to 0
        let util = heuristic_utility(&state, 0);
        assert!(util.abs() < 0.01, "Equal game should have ~0 utility, got {}", util);
    }

    #[test]
    fn test_heuristic_utility_life_advantage() {
        use crate::card::sample;
        use std::sync::Arc;

        let db = sample::build_sample_db();
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.players[0].life = 20;
        state.players[1].life = 10;

        let util = heuristic_utility(&state, 0);
        assert!(util > 0.0, "Player with more life should have positive utility");
    }

    #[test]
    fn test_merge_regret_tables() {
        use crate::action::canonical::CanonicalAction;

        let mut shard1 = [RegretTable::new(), RegretTable::new()];
        let mut shard2 = [RegretTable::new(), RegretTable::new()];

        let a = CanonicalAction::PassPriority;
        // Shard 1: player 0, info hash 42, action PassPriority with regret 5.0, strategy 3.0
        {
            let entry = shard1[0].get_or_create(42);
            entry.visit_count = 10;
            let ae = entry.get_or_create_action(&a);
            ae.cumulative_regret = 5.0;
            ae.cumulative_strategy = 3.0;
        }
        // Shard 2: player 0, info hash 42, action PassPriority with regret 7.0, strategy 2.0
        {
            let entry = shard2[0].get_or_create(42);
            entry.visit_count = 15;
            let ae = entry.get_or_create_action(&a);
            ae.cumulative_regret = 7.0;
            ae.cumulative_strategy = 2.0;
        }

        let merged = merge_regret_tables(&[shard1, shard2]);

        let data = merged[0].get(42).unwrap();
        assert_eq!(data.visit_count, 25);
        let ae = &data.action_data[&a];
        assert!((ae.cumulative_regret - 12.0).abs() < 1e-10);
        assert!((ae.cumulative_strategy - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_train_extended_with_abstraction() {
        use crate::card::sample;
        use crate::info_set::BucketedAbstraction;
        use std::sync::Arc;

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

        let tables = train_extended(&state, 5, &train_cfg);

        let total_info_sets: usize = tables.iter().map(|t| t.num_info_sets()).sum();
        assert!(total_info_sets > 0, "Training should create entries");
    }

    #[test]
    fn test_rollout_evaluation() {
        use crate::card::sample;
        use crate::strategy::{GreedyStrategy, RandomStrategy};
        use std::sync::Arc;

        let db = sample::build_sample_db();
        let deck0 = sample::mini_red_burn();
        let deck1 = sample::mini_red_creatures();

        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        rules::setup_game(&mut state, &deck0, &deck1);

        let greedy = GreedyStrategy;
        let random = RandomStrategy;

        // Rollout should produce a value in [-1, 1]
        let util = rollout_utility(&state, 0, Some((&greedy, &random)), 500);
        assert!(
            util >= -1.0 && util <= 1.0,
            "Rollout utility should be in [-1, 1], got {}", util
        );
    }

    #[test]
    fn test_training_stats() {
        use crate::card::sample;
        use std::sync::Arc;

        let db = sample::build_sample_db();
        let deck0 = sample::mini_red_burn();
        let deck1 = sample::mini_red_creatures();

        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        rules::setup_game(&mut state, &deck0, &deck1);

        let config = McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 };
        let tables = train(&state, 5, &config);

        let stats = training_stats(&tables);
        assert!(stats.total_info_sets[0] > 0);
        assert!(stats.total_visits[0] > 0);
        assert!(stats.memory_bytes[0] > 0);
        assert!(stats.exploitability.is_finite());
    }

    #[test]
    fn test_train_goldfish_parallel() {
        use crate::card::sample;
        use std::sync::Arc;

        let db = sample::build_sample_db();
        let deck = sample::red_aggro_deck();

        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        rules::setup_game(&mut state, &deck, &deck);

        let config = McfrConfig { max_depth: 8, max_actions: 1000, max_nodes_per_iteration: 0 };
        let tables = train_goldfish_parallel(&state, 8, 4, &config);

        // Pilot's table should have entries
        assert!(tables[0].num_info_sets() > 0, "Parallel goldfish training should create info sets");
        let total_visits: u64 = tables[0].data.values().map(|d| d.visit_count).sum();
        assert!(total_visits > 0, "Parallel goldfish training should accumulate visits");
        // Opponent's table should remain empty (goldfish)
        assert_eq!(tables[1].num_info_sets(), 0, "Opponent table should be empty in goldfish mode");
    }

    #[test]
    fn test_train_parallel_basic() {
        use crate::card::sample;
        use std::sync::Arc;

        let db = sample::build_sample_db();
        let deck0 = sample::mini_red_burn();
        let deck1 = sample::mini_red_creatures();

        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        rules::setup_game(&mut state, &deck0, &deck1);

        let config = McfrConfig { max_depth: 8, max_actions: 200, max_nodes_per_iteration: 0 };
        let tables = train_parallel_basic(&state, 8, 4, &config);

        // Both players should have info set entries
        let total_info_sets: usize = tables.iter().map(|t| t.num_info_sets()).sum();
        assert!(total_info_sets > 0, "Parallel basic training should create entries");
    }

    #[test]
    fn test_train_goldfish_parallel_more_shards_than_iterations() {
        use crate::card::sample;
        use std::sync::Arc;

        let db = sample::build_sample_db();
        let deck = sample::red_aggro_deck();

        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        rules::setup_game(&mut state, &deck, &deck);

        let config = McfrConfig { max_depth: 8, max_actions: 1000, max_nodes_per_iteration: 0 };
        // 2 iterations spread across 8 shards: should not panic
        let tables = train_goldfish_parallel(&state, 2, 8, &config);
        assert!(tables[0].num_info_sets() > 0);
    }
}
