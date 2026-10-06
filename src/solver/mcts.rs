//! MCTS (Monte Carlo Tree Search) for goldfish/solitaire optimization.
//!
//! Unlike MCCFR which finds Nash equilibria for adversarial games, MCTS is
//! appropriate for single-agent optimization where the goal is to find the
//! best sequence of actions to maximize a reward (here: minimize kill turn).
//!
//! # Algorithm
//!
//! For each decision point, we build a search tree rooted at the current
//! game state and repeatedly:
//!
//! 1. **Select** — Walk down the tree using UCB1 to balance exploration
//!    and exploitation.
//! 2. **Expand** — When we reach a leaf node, expand it by adding children
//!    for each legal action.
//! 3. **Simulate** — Play out the rest of the game from the expanded node
//!    using a rollout policy (GreedyStrategy by default).
//! 4. **Backpropagate** — Update visit counts and reward sums back up the
//!    path from the expanded node to the root.
//!
//! Since this is solitaire (no adversary), all nodes are maximization nodes.
//! The goldfish opponent's actions are deterministic (always pass), so we
//! skip tree branching for their decisions.
//!
//! # Reward Function
//!
//! - Win on turn T: `reward = (MAX_TURN + 1 - T) / MAX_TURN`
//!   Faster kills get higher rewards (range: ~0.05 to 1.0).
//! - Draw/loss: partial credit for life reduction, scaled to `[0, 0.04]`
//!   so any actual kill always beats any non-kill.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use rand::seq::SliceRandom;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::action::{legal_actions, Action};
use crate::game::{GameFormat, GameState, Phase, PlayerIndex};
use crate::rules;
use crate::simulation::{format_action_name, format_hand, TerminationReason};
use crate::strategy::{GoldfishStrategy, GreedyStrategy, Strategy};

/// Maximum turns for goldfish MCTS games (matches simulation module).
const GOLDFISH_MAX_TURNS: u32 = 20;

/// Maximum actions per game to prevent infinite loops.
const GOLDFISH_MAX_ACTIONS: u32 = 10_000;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for MCTS search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MctsConfig {
    /// Number of MCTS iterations (tree walks) per decision point.
    /// More iterations = better play but slower decisions.
    pub iterations_per_move: u32,

    /// UCB1 exploration constant. Higher values explore more, lower values
    /// exploit known-good actions. sqrt(2) ≈ 1.414 is theoretically optimal
    /// for rewards in [0, 1]; 0.5–1.0 often works well in practice.
    pub exploration_constant: f64,

    /// Maximum depth of the search tree (in player-0 decisions, not total
    /// actions). 0 = unlimited. Deeper trees find longer-horizon plays
    /// but use more memory and time.
    pub max_tree_depth: u32,

    /// Maximum number of actions during rollout before declaring a draw.
    /// Prevents runaway rollouts in degenerate game states.
    pub max_rollout_actions: u32,

    /// Number of threads for parallel MCTS (root parallelization).
    /// Each thread builds an independent search tree with
    /// `iterations_per_move / num_threads` iterations, then results are
    /// merged by summing per-action visit counts and rewards.
    /// 0 or 1 = single-threaded (default). Values > 1 enable parallelism.
    pub num_threads: u32,
}

impl Default for MctsConfig {
    fn default() -> Self {
        MctsConfig {
            iterations_per_move: 500,
            exploration_constant: 1.0,
            max_tree_depth: 0,
            max_rollout_actions: 5_000,
            num_threads: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Tree structure
// ---------------------------------------------------------------------------

/// A node in the MCTS search tree.
///
/// Each node represents a game state after a sequence of actions from the
/// root. In goldfish mode, only the pilot (player 0) has meaningful
/// decisions; goldfish actions are applied deterministically without
/// creating tree branches.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MctsNode {
    /// Number of times this node has been visited during search.
    visits: u32,

    /// Sum of rewards from all rollouts through this node.
    /// Reward is in [0, 1] where higher = faster kill.
    total_reward: f64,

    /// Children indexed by action. Populated on first visit (expansion).
    /// `None` means this node hasn't been expanded yet.
    children: Option<Vec<MctsChild>>,
}

/// A child edge in the MCTS tree (action + resulting node).
#[derive(Debug, Serialize, Deserialize)]
struct MctsChild {
    /// The action that transitions from the parent to this child.
    action: Action,
    /// The child node.
    node: MctsNode,
}

impl MctsNode {
    fn new() -> Self {
        MctsNode {
            visits: 0,
            total_reward: 0.0,
            children: None,
        }
    }

    /// Average reward (Q-value) for this node.
    fn avg_reward(&self) -> f64 {
        if self.visits == 0 {
            0.0
        } else {
            self.total_reward / self.visits as f64
        }
    }

    /// Whether this node has been expanded (children generated).
    fn is_expanded(&self) -> bool {
        self.children.is_some()
    }
}

// ---------------------------------------------------------------------------
// UCB1 selection
// ---------------------------------------------------------------------------

/// Select the child index with the highest UCB1 score.
///
/// UCB1 = Q_i + C * sqrt(ln(N_parent) / N_i)
///
/// where Q_i is the average reward of child i, N_parent is the parent's
/// visit count, N_i is the child's visit count, and C is the exploration
/// constant. Unvisited children get infinite UCB1 (selected first).
///
/// Ties are broken randomly to improve search diversity (e.g., when
/// multiple children are unvisited, they're explored in random order
/// rather than always left-to-right).
fn ucb1_select(children: &[MctsChild], parent_visits: u32, c: f64) -> usize {
    let ln_parent = (parent_visits as f64).ln();

    // Compute UCB1 for each child
    let ucb_scores: Vec<f64> = children
        .iter()
        .map(|child| {
            if child.node.visits == 0 {
                f64::INFINITY
            } else {
                child.node.avg_reward() + c * (ln_parent / child.node.visits as f64).sqrt()
            }
        })
        .collect();

    let best_ucb = ucb_scores
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    // Collect all indices tied at the best score
    let tied: Vec<usize> = ucb_scores
        .iter()
        .enumerate()
        .filter(|(_, &score)| {
            // For infinity (unvisited), check with is_infinite
            if best_ucb.is_infinite() {
                score.is_infinite()
            } else {
                (score - best_ucb).abs() < 1e-12
            }
        })
        .map(|(i, _)| i)
        .collect();

    // Random tie-breaking among best
    let mut rng = rand::thread_rng();
    *tied.choose(&mut rng).unwrap_or(&0)
}

// ---------------------------------------------------------------------------
// Reward function
// ---------------------------------------------------------------------------

/// Compute reward for a completed goldfish game.
///
/// Returns a value in [0, 1] where:
/// - Win on turn T: `(MAX_TURN + 1 - T) / MAX_TURN` — faster kills get
///   higher reward. Turn 1 kill = 1.0, turn 20 kill = 0.05.
/// - No kill: partial credit for damage dealt plus combo proximity,
///   scaled to `[0, 0.04]` so any actual kill (min 0.05) always outranks
///   any non-kill. Combo proximity rewards assembling combo pieces even
///   when the full combo hasn't fired.
fn goldfish_reward(
    winner: Option<PlayerIndex>,
    turn: u32,
    opponent_life: i32,
    starting_life: i32,
    combo_bonus: f64,
) -> f64 {
    match winner {
        Some(0) => {
            let t = turn.min(GOLDFISH_MAX_TURNS);
            (GOLDFISH_MAX_TURNS + 1 - t) as f64 / GOLDFISH_MAX_TURNS as f64
        }
        _ => {
            // Reward shaping: partial credit for damage + combo proximity.
            // Scale total to [0, 0.04] — strictly below worst win (T20 = 0.05).
            let damage = (starting_life - opponent_life).max(0) as f64;
            let damage_fraction = (damage / starting_life as f64).min(1.0);
            // Combo bonus (0-1 range) adds up to half the non-kill budget.
            let combo_fraction = combo_bonus.min(1.0);
            let total = damage_fraction * 0.02 + combo_fraction * 0.02;
            total.min(0.04)
        }
    }
}

/// Starting life total for a game format.
fn format_starting_life(format: GameFormat) -> i32 {
    match format {
        GameFormat::Commander => 40,
        GameFormat::Standard => 20,
    }
}

// ---------------------------------------------------------------------------
// Core MCTS algorithm
// ---------------------------------------------------------------------------

/// Run MCTS from the given game state and return the best action.
///
/// This is the main entry point for a single MCTS decision. It builds
/// (or reuses) a search tree, runs `config.iterations_per_move` iterations,
/// and returns the action with the highest visit count (most robust child).
///
/// When `config.num_threads > 1`, uses root parallelization: each thread
/// builds an independent tree with a share of the iterations, then results
/// are merged by summing per-action visit counts and rewards.
/// Legacy action-only search interface: unsupported continuations fail explicitly.
pub(crate) fn mcts_search(
    state: &GameState,
    config: &MctsConfig,
    root: &mut MctsNode,
) -> Option<Action> {
    try_mcts_search(state, config, root)
        .unwrap_or_else(|reason| panic!("INVALID reason={}", reason.code()))
}

fn try_mcts_search(
    state: &GameState,
    config: &MctsConfig,
    root: &mut MctsNode,
) -> Result<Option<Action>, TerminationReason> {
    if state.unsupported_continuing_elimination() {
        return Err(TerminationReason::UnsupportedContinuingElimination);
    }
    if state.gameplay_stopped() { return Ok(None); }
    let actions = legal_actions(state);
    if actions.is_empty() {
        return Ok(Some(Action::PassPriority));
    }
    if actions.len() == 1 {
        return Ok(Some(actions[0].clone()));
    }

    let num_threads = config.num_threads.max(1);

    if num_threads <= 1 {
        // Single-threaded path (original behavior)
        mcts_search_single(state, config, root, &actions)
    } else {
        // Parallel root parallelization
        mcts_search_parallel(state, config, root, &actions, num_threads)
    }
}

/// Single-threaded MCTS search. Runs all iterations on one tree.
fn mcts_search_single(
    state: &GameState,
    config: &MctsConfig,
    root: &mut MctsNode,
    actions: &[Action],
) -> Result<Option<Action>, TerminationReason> {
    // Expand root if needed
    if !root.is_expanded() {
        expand_node(root, actions);
    }

    let greedy = GreedyStrategy;
    let goldfish = GoldfishStrategy;

    for _ in 0..config.iterations_per_move {
        // Clone the state for this iteration (each iteration modifies state)
        let mut sim_state = state.clone();
        let reward = tree_walk(&mut sim_state, root, config, &greedy, &goldfish, 0)?;
        root.visits += 1;
        root.total_reward += reward;
    }

    // Select the action with the most visits (most robust child)
    Ok(best_action_by_visits(root))
}

/// Parallel MCTS search using root parallelization.
///
/// Each thread builds an independent search tree with a share of the total
/// iterations. After all threads complete, per-action visit counts and
/// rewards are summed to select the most robust action.
///
/// This is the simplest and most effective parallelization strategy for MCTS:
/// - No locks on tree nodes (each tree is thread-local)
/// - Linear speedup proportional to thread count
/// - Slightly more total iterations due to rounding, but never fewer
///
/// **Root overwrite:** The passed-in `root` node is overwritten with merged
/// statistics from all threads. This is fine because both callers
/// (`MctsStrategy::choose_action` and `run_mcts_goldfish_game`) create a
/// fresh `MctsNode::new()` per decision — tree reuse across decisions is
/// not supported. The root is populated solely so callers can read decision
/// stats (visit counts, avg reward) for logging.
///
/// **Nested parallelism:** This uses Rayon's `into_par_iter`, which shares
/// the global thread pool with the outer game-level parallelism in
/// `simulate_mcts_goldfish`. When many games run concurrently, inner MCTS
/// parallelism won't get additional threads (the pool is already saturated).
/// `num_threads > 1` is most useful when running a single game or few games.
fn mcts_search_parallel(
    state: &GameState,
    config: &MctsConfig,
    root: &mut MctsNode,
    actions: &[Action],
    num_threads: u32,
) -> Result<Option<Action>, TerminationReason> {
    let iters_per_thread = config.iterations_per_move / num_threads;
    let remainder = config.iterations_per_move % num_threads;

    // Each thread runs its own tree search and returns per-action (visits, total_reward).
    let thread_results: Result<Vec<Vec<(u32, f64)>>, TerminationReason> = (0..num_threads)
        .into_par_iter()
        .map(|thread_idx| {
            // First `remainder` threads get one extra iteration
            let my_iters = iters_per_thread + if thread_idx < remainder { 1 } else { 0 };
            if my_iters == 0 {
                return Ok(vec![(0, 0.0); actions.len()]);
            }

            let mut thread_root = MctsNode::new();
            expand_node(&mut thread_root, actions);

            // Single-threaded config for each thread's tree (avoid nested parallelism)
            let thread_config = MctsConfig {
                num_threads: 1,
                ..*config
            };

            let greedy = GreedyStrategy;
            let goldfish = GoldfishStrategy;

            for _ in 0..my_iters {
                let mut sim_state = state.clone();
                let reward = tree_walk(
                    &mut sim_state,
                    &mut thread_root,
                    &thread_config,
                    &greedy,
                    &goldfish,
                    0,
                )?;
                thread_root.visits += 1;
                thread_root.total_reward += reward;
            }

            // Extract per-action stats
            Ok(thread_root
                .children
                .as_ref()
                .map(|children| {
                    children
                        .iter()
                        .map(|c| (c.node.visits, c.node.total_reward))
                        .collect()
                })
                .unwrap_or_else(|| vec![(0, 0.0); actions.len()]))
        })
        .collect();
    let thread_results = thread_results?;

    // Merge results: sum visits and rewards per action across all threads
    let num_actions = actions.len();
    let mut merged_visits = vec![0u32; num_actions];
    let mut merged_rewards = vec![0.0f64; num_actions];

    for thread_result in &thread_results {
        for (i, &(visits, reward)) in thread_result.iter().enumerate() {
            if i < num_actions {
                merged_visits[i] += visits;
                merged_rewards[i] += reward;
            }
        }
    }

    // Update the root node with merged statistics
    if !root.is_expanded() {
        expand_node(root, actions);
    }
    let total_visits: u32 = merged_visits.iter().sum();
    let total_reward: f64 = merged_rewards.iter().sum();
    root.visits = total_visits;
    root.total_reward = total_reward;

    if let Some(children) = root.children.as_mut() {
        for (i, child) in children.iter_mut().enumerate() {
            if i < num_actions {
                child.node.visits = merged_visits[i];
                child.node.total_reward = merged_rewards[i];
            }
        }
    }

    // Select the action with the most total visits across all trees
    Ok(best_action_by_visits(root))
}

/// Perform one MCTS iteration: select → expand → simulate → backpropagate.
///
/// Returns the reward obtained from this iteration. The caller is responsible
/// for updating the root node's statistics.
///
/// Forced passes and goldfish turns are handled iteratively (loop) to avoid
/// unbounded recursion that could overflow the stack. Only player-0 decision
/// points use recursion (bounded by tree depth).
fn tree_walk(
    state: &mut GameState,
    node: &mut MctsNode,
    config: &MctsConfig,
    rollout_strategy: &dyn Strategy,
    goldfish_strategy: &GoldfishStrategy,
    depth: u32,
) -> Result<f64, TerminationReason> {
    // Advance past forced passes and goldfish turns iteratively.
    // This loop replaces what was previously tail-recursion through
    // non-decision states, preventing O(phases × turns) stack depth.
    loop {
        if state.unsupported_continuing_elimination() {
            return Err(TerminationReason::UnsupportedContinuingElimination);
        }
        // Terminal check
        if state.game_over || state.turn_number > GOLDFISH_MAX_TURNS {
            let sl = format_starting_life(state.format);
            let combo_bonus = if let Some(ref registry) = state.combo_registry {
                crate::combo::combo_proximity_bonus(state, 0, registry)
            } else {
                0.0
            };
            return Ok(goldfish_reward(state.winner, state.turn_number, state.players[1].life, sl, combo_bonus));
        }

        // Depth limit — switch to rollout
        if config.max_tree_depth > 0 && depth >= config.max_tree_depth {
            return rollout(state, rollout_strategy, goldfish_strategy, config);
        }

        let player = state.priority_player;
        let actions = legal_actions(state);

        // No actions or only pass — advance without branching
        if actions.is_empty()
            || (actions.len() == 1 && actions[0] == Action::PassPriority)
        {
            rules::apply_action(state, &Action::PassPriority);
            continue;
        }

        // Goldfish (player 1) — deterministic, no branching
        if player != 0 {
            let action = goldfish_strategy.choose_action(state, player);
            rules::apply_action(state, &action);
            continue;
        }

        // Mulligan phase — handle with rollout strategy, no tree branching.
        // MulliganMulligan introduces stochasticity (random shuffle/draw) that
        // breaks MCTS tree reuse: the same tree node would be visited with
        // different hands across iterations, causing action mismatches that
        // corrupt the mulligan state machine. The GreedyStrategy heuristic
        // handles mulligans well, so we skip tree search for this phase.
        if state.phase == Phase::Mulligan {
            let action = rollout_strategy.choose_action(state, player);
            rules::apply_action(state, &action);
            continue;
        }

        // --- Player 0 decision node — break out of loop to handle below ---
        break;
    }

    // We only reach here at a player-0 decision node.
    let actions = legal_actions(state);

    // Expand if this is a leaf
    if !node.is_expanded() {
        expand_node(node, &actions);
        // First visit to a new node: rollout from here.
        // Don't update node stats here — the caller's backpropagation handles it.
        return rollout(state, rollout_strategy, goldfish_strategy, config);
    }

    let children = node.children.as_mut().unwrap();

    // If our action set changed (e.g., different game path led here), re-expand.
    // This shouldn't happen in practice for goldfish since the tree is built
    // deterministically, but handle it defensively.
    if children.is_empty() {
        return rollout(state, rollout_strategy, goldfish_strategy, config);
    }

    // Select child using UCB1
    let child_idx = ucb1_select(children, node.visits.max(1), config.exploration_constant);

    // Apply the selected action
    let action = children[child_idx].action.clone();
    rules::apply_action(state, &action);

    // Recurse (bounded by tree depth — only player-0 decisions increment depth)
    let reward = tree_walk(
        state,
        &mut children[child_idx].node,
        config,
        rollout_strategy,
        goldfish_strategy,
        depth + 1,
    )?;

    // Backpropagate
    children[child_idx].node.visits += 1;
    children[child_idx].node.total_reward += reward;

    Ok(reward)
}

/// Expand a node by creating child entries for each legal action.
fn expand_node(node: &mut MctsNode, actions: &[Action]) {
    let children: Vec<MctsChild> = actions
        .iter()
        .map(|a| MctsChild {
            action: a.clone(),
            node: MctsNode::new(),
        })
        .collect();
    node.children = Some(children);
}

/// Run a rollout (simulation) from the current state to completion using
/// the rollout policy, and return the reward.
fn rollout(
    state: &mut GameState,
    rollout_strategy: &dyn Strategy,
    goldfish_strategy: &GoldfishStrategy,
    config: &MctsConfig,
) -> Result<f64, TerminationReason> {
    if state.unsupported_continuing_elimination() {
        return Err(TerminationReason::UnsupportedContinuingElimination);
    }
    let mut actions_taken: u32 = 0;

    while !state.gameplay_stopped()
        && state.turn_number <= GOLDFISH_MAX_TURNS
        && actions_taken < config.max_rollout_actions
    {
        let player = state.priority_player;
        let actions = legal_actions(state);

        if actions.is_empty()
            || (actions.len() == 1 && actions[0] == Action::PassPriority)
        {
            rules::apply_action(state, &Action::PassPriority);
            actions_taken += 1;
            continue;
        }

        let strategy: &dyn Strategy = if player == 0 {
            rollout_strategy
        } else {
            goldfish_strategy
        };
        let action = strategy.choose_action(state, player);
        rules::apply_action(state, &action);
        actions_taken += 1;

        if !state.gameplay_stopped() && actions_taken % 10 == 0 {
            rules::check_state_based_actions(state);
        }
    }

    if state.unsupported_continuing_elimination() {
        return Err(TerminationReason::UnsupportedContinuingElimination);
    }
    let sl = format_starting_life(state.format);
    let combo_bonus = if let Some(ref registry) = state.combo_registry {
        crate::combo::combo_proximity_bonus(state, 0, registry)
    } else {
        0.0
    };
    Ok(goldfish_reward(state.winner, state.turn_number, state.players[1].life, sl, combo_bonus))
}

/// Select the action with the highest visit count (most robust child selection).
///
/// In MCTS, the most-visited child is preferred over the highest-average-reward
/// child because visit count is more robust to noise from rollouts.
fn best_action_by_visits(root: &MctsNode) -> Option<Action> {
    let children = root.children.as_ref()?;
    children
        .iter()
        .max_by_key(|c| c.node.visits)
        .map(|c| c.action.clone())
}

// ---------------------------------------------------------------------------
// MctsStrategy — implements the Strategy trait
// ---------------------------------------------------------------------------

/// MCTS-based strategy for goldfish solitaire optimization.
///
/// At each decision point, runs MCTS from the current game state to find
/// the best action. This is an "online" planner — it builds a fresh search
/// tree at each decision point.
///
/// For goldfish, only player 0's decisions matter. When used as player 1's
/// strategy (shouldn't happen), falls back to GoldfishStrategy.
pub struct MctsStrategy {
    config: MctsConfig,
}

impl MctsStrategy {
    pub fn new(config: MctsConfig) -> Self {
        MctsStrategy { config }
    }
}

impl Strategy for MctsStrategy {
    fn choose_action(&self, state: &GameState, player: PlayerIndex) -> Action {
        assert!(!state.unsupported_continuing_elimination(),
            "INVALID reason=unsupported_continuing_elimination");
        assert!(!state.gameplay_stopped(), "MCTS action requested for stopped gameplay");
        // MCTS only makes sense for the pilot (player 0)
        if player != 0 {
            return GoldfishStrategy.choose_action(state, player);
        }

        // Mulligan phase — delegate to GreedyStrategy. MCTS tree search is
        // unsound for mulligans because MulliganMulligan introduces stochastic
        // shuffle/draw that breaks tree node reuse across iterations.
        if state.phase == Phase::Mulligan {
            return GreedyStrategy.choose_action(state, player);
        }

        let actions = legal_actions(state);
        if actions.is_empty() {
            return Action::PassPriority;
        }
        if actions.len() == 1 {
            return actions[0].clone();
        }

        let mut root = MctsNode::new();
        mcts_search(state, &self.config, &mut root).unwrap_or(Action::PassPriority)
    }

    fn name(&self) -> &str {
        "MCTS"
    }
}

// ---------------------------------------------------------------------------
// Full-game MCTS search: find optimal play for a fixed shuffle
// ---------------------------------------------------------------------------

/// Result of a single MCTS goldfish game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MctsOutcome {
    Win,
    Loss,
    Draw,
    Censored,
    Stalled,
    Invalid,
    /// An old record with no trustworthy nonwin termination reason.
    LegacyUnknown,
}

impl From<crate::simulation::GameOutcome> for MctsOutcome {
    fn from(outcome: crate::simulation::GameOutcome) -> Self {
        use crate::simulation::GameOutcome;
        match outcome {
            GameOutcome::Win(0) => Self::Win,
            GameOutcome::Win(_) => Self::Loss,
            GameOutcome::Draw => Self::Draw,
            GameOutcome::Censored(_) => Self::Censored,
            GameOutcome::Stalled(_) => Self::Stalled,
            GameOutcome::Invalid(_) => Self::Invalid,
        }
    }
}

fn apply_mcts_game_action(
    state: &mut GameState,
    action: &Action,
    legal: &[Action],
    actions_taken: &mut u32,
    rejected_in_row: &mut u32,
) -> Result<bool, crate::simulation::GameOutcome> {
    use crate::simulation::{GameOutcome, TerminationReason};
    let accepted_before = state.loss_boundary.accepted_actions;
    match crate::simulation::apply_counted_action(state, action, legal) {
        Ok(true) => {
            *actions_taken += 1;
            *rejected_in_row = 0;
            Ok(true)
        }
        Ok(false) => {
            *rejected_in_row += 1;
            if *rejected_in_row >= crate::simulation::MAX_REJECTED_IN_ROW {
                Err(GameOutcome::Stalled(TerminationReason::RejectedAction))
            } else {
                Ok(false)
            }
        }
        Err(reason) => {
            crate::simulation::count_completed_unsupported_actions(
                state, accepted_before, reason, actions_taken);
            Err(GameOutcome::Invalid(reason))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MctsGameResult {
    /// Whether player 0 won.
    pub won: bool,
    pub outcome: MctsOutcome,
    /// Turn the game ended.
    pub kill_turn: u32,
    /// Total actions taken.
    pub actions_taken: u32,
    /// Final life totals.
    pub final_life: [i32; 2],
    /// Per-decision statistics: (turn, phase, num_actions_considered, iterations_used, best_action_visits, best_action_avg_reward)
    pub decision_stats: Vec<DecisionStat>,
    /// Buffered verbose trace lines (only populated when verbose=true).
    #[serde(skip)]
    pub trace_lines: Vec<String>,
    /// Appended provenance; absent JSON fields default, old binary layouts are uncertified.
    #[serde(default)]
    pub loss_boundary: crate::game::LossBoundary,
}

/// Statistics for a single MCTS decision point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionStat {
    pub turn: u32,
    pub phase: Phase,
    pub num_legal_actions: usize,
    pub best_action_visits: u32,
    pub best_action_avg_reward: f64,
    pub action_description: String,
    /// Pilot's hand (card names) at the time of this decision.
    #[serde(default)]
    pub pilot_hand: Vec<String>,
}

/// Run a single goldfish game using MCTS for player 0's decisions.
///
/// For a fixed shuffle (determined by the caller), this finds near-optimal
/// play by running MCTS at each decision point. Returns detailed statistics
/// about each decision.
pub fn run_mcts_goldfish_game(
    state: &mut GameState,
    config: &MctsConfig,
    verbose: bool,
    decisions_counter: Option<&AtomicU64>,
) -> MctsGameResult {
    let goldfish = GoldfishStrategy;
    let mut actions_taken: u32 = 0;
    let mut decision_stats = Vec::new();
    let mut trace_lines: Vec<String> = Vec::new();
    let mut interrupted = None;
    let mut rejected_in_row = 0u32;

    while !state.gameplay_stopped()
        && state.winner.is_none()
        && state.turn_number <= GOLDFISH_MAX_TURNS
        && actions_taken < GOLDFISH_MAX_ACTIONS
    {
        if crate::simulation::invalid_cleanup_state(state) {
            interrupted = Some(crate::simulation::GameOutcome::Invalid(
                crate::simulation::TerminationReason::IncompleteCleanup));
            break;
        }
        let player = state.priority_player;
        let actions = legal_actions(state);

        if actions.is_empty()
            || (actions.len() == 1 && actions[0] == Action::PassPriority)
        {
            if actions.is_empty() {
                interrupted = Some(crate::simulation::no_progress_outcome(state));
                break;
            }
            match apply_mcts_game_action(state, &Action::PassPriority, &actions,
                &mut actions_taken, &mut rejected_in_row) {
                Ok(_) => {}
                Err(outcome) => { interrupted = Some(outcome); break; }
            }
            continue;
        }

        if player != 0 {
            // Goldfish — deterministic, no search needed
            let action = goldfish.choose_action(state, player);
            match apply_mcts_game_action(state, &action, &actions,
                &mut actions_taken, &mut rejected_in_row) {
                Ok(_) => {}
                Err(outcome) => { interrupted = Some(outcome); break; }
            }
            continue;
        }

        // Mulligan phase — use GreedyStrategy heuristic directly.
        // MCTS tree search over mulligan decisions is unsound because
        // MulliganMulligan involves stochastic shuffle/draw, causing
        // tree nodes to be visited with different hands across iterations.
        if state.phase == Phase::Mulligan {
            let greedy = GreedyStrategy;
            let action = greedy.choose_action(state, player);
            if verbose {
                let hand = format_hand(state, 0);
                trace_lines.push(format!("  Hand: [{}]", hand.join(", ")));
                trace_lines.push(format!(
                    "T{} {:?} P0: {} (greedy mulligan)",
                    state.turn_number,
                    state.phase,
                    format_action(&action, state),
                ));
            }
            match apply_mcts_game_action(state, &action, &actions,
                &mut actions_taken, &mut rejected_in_row) {
                Ok(_) => {}
                Err(outcome) => { interrupted = Some(outcome); break; }
            }
            continue;
        }

        // Capture pilot hand before the decision
        let pilot_hand = format_hand(state, 0);

        // Player 0 decision — run MCTS
        let action = if actions.len() == 1 {
            actions[0].clone()
        } else {
            let mut root = MctsNode::new();
            let best = match try_mcts_search(state, config, &mut root) {
                Ok(action) => action.unwrap_or(Action::PassPriority),
                Err(reason) => {
                    interrupted = Some(crate::simulation::GameOutcome::Invalid(reason));
                    break;
                }
            };

            // Collect decision statistics
            if let Some(children) = &root.children {
                let best_child = children.iter().max_by_key(|c| c.node.visits);
                if let Some(bc) = best_child {
                    let stat = DecisionStat {
                        turn: state.turn_number,
                        phase: state.phase,
                        num_legal_actions: children.len(),
                        best_action_visits: bc.node.visits,
                        best_action_avg_reward: bc.node.avg_reward(),
                        action_description: format_action(&best, state),
                        pilot_hand: pilot_hand.clone(),
                    };
                    decision_stats.push(stat);
                }
            }

            if verbose {
                trace_lines.push(format!("  Hand: [{}]", pilot_hand.join(", ")));
                trace_lines.push(format!(
                    "T{} {:?} P0: {} (of {} actions, {}/{} iters)",
                    state.turn_number,
                    state.phase,
                    format_action(&best, state),
                    actions.len(),
                    root.children
                        .as_ref()
                        .and_then(|c| c.iter().max_by_key(|c| c.node.visits))
                        .map(|c| c.node.visits)
                        .unwrap_or(0),
                    config.iterations_per_move,
                ));
            }

            best
        };

        if let Some(counter) = decisions_counter {
            counter.fetch_add(1, Ordering::Relaxed);
        }

        match apply_mcts_game_action(state, &action, &actions,
            &mut actions_taken, &mut rejected_in_row) {
            Ok(false) => continue,
            Ok(true) => {}
            Err(outcome) => { interrupted = Some(outcome); break; }
        }

        if !state.gameplay_stopped() && actions_taken % 10 == 0 {
            rules::check_state_based_actions(state);
        }
    }

    let outcome = MctsOutcome::from(crate::simulation::classify_outcome(
        state, GOLDFISH_MAX_TURNS, GOLDFISH_MAX_ACTIONS, actions_taken, interrupted));
    MctsGameResult {
        won: outcome == MctsOutcome::Win,
        outcome,
        kill_turn: state.turn_number,
        actions_taken,
        final_life: [state.players[0].life, state.players[1].life],
        decision_stats,
        trace_lines,
        loss_boundary: state.loss_boundary.clone(),
    }
}

/// Format an action for human-readable display.
/// Delegates to the shared `format_action_name` in the simulation module.
fn format_action(action: &Action, state: &GameState) -> String {
    format_action_name(state, action)
}

// ---------------------------------------------------------------------------
// Aggregate MCTS goldfish statistics
// ---------------------------------------------------------------------------

/// Aggregate results from many MCTS goldfish games.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MctsGoldfishResults {
    pub schema_version: u32,
    pub total_games: u64,
    pub wins: u64,
    pub losses: u64,
    pub draws: u64,
    pub censored: u64,
    pub stalled: u64,
    pub invalid: u64,
    pub legacy_unknown: u64,
    pub avg_kill_turn: Option<f64>,
    pub kill_turn_samples: u64,
    pub fastest_kill: u32,
    pub slowest_kill: u32,
    pub avg_actions: Option<f64>,
    pub actions_samples: u64,
    pub avg_decisions_per_game: Option<f64>,
    pub decision_samples: u64,
    pub avg_best_reward: Option<f64>,
    pub reward_samples: u64,
    /// Kill-turn distribution: index = turn number, value = number of wins.
    pub kill_turn_distribution: Vec<u64>,
    /// Decision sequence from the fastest winning game.
    pub fastest_sequence: Vec<DecisionStat>,
}

/// The pre-outcome checkpoint layout. It is decoded only during migration;
/// its nonwin counters cannot distinguish losses, rules draws, and timeouts.
///
/// Historical aggregate provenance (individual records can prove more):
///
/// | Metric | V1 aggregate | V2 aggregate | V3 |
/// |---|---|---|---|
/// | Actions/decisions | Mean over all attempts: known for all-win; otherwise unknown | Potential compatibility default: unknown | Explicit mean and sample count |
/// | Reward | Empty decisions defaulted to zero: known only for a single completed game with decisions; otherwise unknown | Potential compatibility default: unknown | Explicit mean and sample count |
/// | Kill turn | Mean over wins: known when wins exist | Preserved mean over wins: known when wins exist | Explicit mean and sample count |
///
/// V1/V2 per-game records recover completed-game actions and decisions and
/// rewards with nonempty decision lists; absent records prove nothing.
#[derive(Serialize, Deserialize)]
struct LegacyMctsGoldfishResults {
    total_games: u64,
    wins: u64,
    losses: u64,
    draws: u64,
    avg_kill_turn: f64,
    fastest_kill: u32,
    slowest_kill: u32,
    avg_actions: f64,
    avg_decisions_per_game: f64,
    avg_best_reward: f64,
    kill_turn_distribution: Vec<u64>,
    fastest_sequence: Vec<DecisionStat>,
}

impl From<LegacyMctsGoldfishResults> for MctsGoldfishResults {
    fn from(old: LegacyMctsGoldfishResults) -> Self {
        let _ambiguous_old_counts = (old.losses, old.draws);
        // V1 averaged actions and decisions over every attempt. They are
        // completed-game means only when every attempt is a known win.
        let all_attempts_won = old.total_games > 0 && old.wins == old.total_games;
        // Both V1 writers contributed a default zero only for a game with no
        // decisions. A single known win with decisions has one measured
        // per-game reward, including when its measured value is zero.
        let reward_is_measured = all_attempts_won && old.total_games == 1
            && old.avg_decisions_per_game > 0.0;
        Self {
            schema_version: 3,
            total_games: old.total_games,
            wins: old.wins,
            losses: 0,
            draws: 0,
            censored: 0,
            stalled: 0,
            invalid: 0,
            legacy_unknown: old.total_games.saturating_sub(old.wins),
            avg_kill_turn: (old.wins > 0).then_some(old.avg_kill_turn),
            kill_turn_samples: old.wins,
            fastest_kill: old.fastest_kill,
            slowest_kill: old.slowest_kill,
            avg_actions: all_attempts_won.then_some(old.avg_actions),
            actions_samples: if all_attempts_won { old.total_games } else { 0 },
            avg_decisions_per_game: all_attempts_won.then_some(old.avg_decisions_per_game),
            decision_samples: if all_attempts_won { old.total_games } else { 0 },
            avg_best_reward: reward_is_measured.then_some(old.avg_best_reward),
            reward_samples: u64::from(reward_is_measured),
            kill_turn_distribution: old.kill_turn_distribution,
            fastest_sequence: old.fastest_sequence,
        }
    }
}

/// Version 2 knew outcomes but did not persist which averages had actual
/// measurements. An aggregate containing migrated legacy records cannot
/// recover completed-game action, decision, or reward samples.
#[derive(Serialize, Deserialize)]
struct MctsGoldfishResultsV2 {
    schema_version: u32,
    total_games: u64,
    wins: u64,
    losses: u64,
    draws: u64,
    censored: u64,
    stalled: u64,
    invalid: u64,
    legacy_unknown: u64,
    avg_kill_turn: f64,
    fastest_kill: u32,
    slowest_kill: u32,
    avg_actions: f64,
    avg_decisions_per_game: f64,
    avg_best_reward: f64,
    kill_turn_distribution: Vec<u64>,
    fastest_sequence: Vec<DecisionStat>,
}

impl From<MctsGoldfishResultsV2> for MctsGoldfishResults {
    fn from(old: MctsGoldfishResultsV2) -> Self {
        let _unproven_aggregate_means = (old.avg_actions, old.avg_decisions_per_game,
            old.avg_best_reward);
        Self {
            schema_version: 3,
            total_games: old.total_games,
            wins: old.wins,
            losses: old.losses,
            draws: old.draws,
            censored: old.censored,
            stalled: old.stalled,
            invalid: old.invalid,
            legacy_unknown: old.legacy_unknown,
            avg_kill_turn: (old.wins > 0).then_some(old.avg_kill_turn),
            kill_turn_samples: old.wins,
            fastest_kill: old.fastest_kill,
            slowest_kill: old.slowest_kill,
            // V2 had no persisted measurement counts or provenance. Even with
            // known outcomes, these fields could include V1 compatibility
            // zeros, so neither zero nor a nonzero mean proves availability.
            avg_actions: None,
            actions_samples: 0,
            avg_decisions_per_game: None,
            decision_samples: 0,
            avg_best_reward: None,
            reward_samples: 0,
            kill_turn_distribution: old.kill_turn_distribution,
            fastest_sequence: old.fastest_sequence,
        }
    }
}

fn add_measurement(mean: &mut Option<f64>, samples: &mut u64, value: f64) {
    let previous = mean.unwrap_or(value);
    *samples += 1;
    *mean = Some(previous + (value - previous) / *samples as f64);
}

fn merge_measurement(a: Option<f64>, a_count: u64, b: Option<f64>, b_count: u64) -> (Option<f64>, u64) {
    let a_count = if a.is_some() { a_count } else { 0 };
    let b_count = if b.is_some() { b_count } else { 0 };
    let count = a_count + b_count;
    if count == 0 { return (None, 0); }
    let sum = a.unwrap_or(0.0) * a_count as f64 + b.unwrap_or(0.0) * b_count as f64;
    (Some(sum / count as f64), count)
}

/// Recover only measurements explicitly present in historical per-game
/// records. Aggregate outcome and kill-turn counts remain authoritative.
fn recover_record_measurements(results: &mut MctsGoldfishResults, records: &[MctsGameResult]) {
    let mut actions = None;
    let mut actions_samples = 0;
    let mut decisions = None;
    let mut decision_samples = 0;
    let mut rewards = None;
    let mut reward_samples = 0;
    for record in records {
        if !matches!(record.outcome, MctsOutcome::Win | MctsOutcome::Loss | MctsOutcome::Draw) {
            continue;
        }
        add_measurement(&mut actions, &mut actions_samples, record.actions_taken as f64);
        add_measurement(&mut decisions, &mut decision_samples, record.decision_stats.len() as f64);
        if !record.decision_stats.is_empty() {
            let reward = record.decision_stats.iter()
                .map(|decision| decision.best_action_avg_reward).sum::<f64>()
                / record.decision_stats.len() as f64;
            add_measurement(&mut rewards, &mut reward_samples, reward);
        }
    }
    // A direct V1 all-win aggregate already has a documented action/decision
    // mean for every game; present records add evidence only for missing means.
    if results.actions_samples == 0 {
        results.avg_actions = actions;
        results.actions_samples = actions_samples;
    }
    if results.decision_samples == 0 {
        results.avg_decisions_per_game = decisions;
        results.decision_samples = decision_samples;
    }
    if results.reward_samples == 0 {
        results.avg_best_reward = rewards;
        results.reward_samples = reward_samples;
    }
}

impl MctsGoldfishResults {
    pub fn completed_games(&self) -> u64 {
        self.wins + self.losses + self.draws
    }

    pub fn win_rate(&self) -> f64 {
        if self.completed_games() == 0 {
            return 0.0;
        }
        self.wins as f64 / self.completed_games() as f64
    }

    pub fn display(&self) {
        println!("=== MCTS Goldfish Results ===");
        println!("Total games: {}", self.total_games);
        println!("Wins: {} ({:.1}%)", self.wins, self.win_rate() * 100.0);
        println!("Rules draws: {}", self.draws);
        println!("Censored: {}, stalled: {}, invalid: {}, legacy unknown: {}",
            self.censored, self.stalled, self.invalid, self.legacy_unknown);
        if self.wins > 0 {
            println!("Avg kill turn: {} ({} measured)",
                self.avg_kill_turn.map_or_else(|| "unavailable".into(), |v| format!("{v:.2}")),
                self.kill_turn_samples);
            println!("Fastest kill: T{}", self.fastest_kill);
            println!("Slowest kill: T{}", self.slowest_kill);
            println!("Kill turn distribution:");
            for (turn, &count) in self.kill_turn_distribution.iter().enumerate() {
                if count > 0 {
                    let pct = count as f64 / self.wins as f64 * 100.0;
                    println!("  T{}: {} ({:.1}%)", turn, count, pct);
                }
            }
            if !self.fastest_sequence.is_empty() {
                println!("\nFastest win (T{}) sequence:", self.fastest_kill);
                for (i, stat) in self.fastest_sequence.iter().enumerate() {
                    if !stat.pilot_hand.is_empty() {
                        println!("        Hand: [{}]", stat.pilot_hand.join(", "));
                    }
                    println!(
                        "  #{:<3} T{} {:?}: {} (of {} options, Q={:.3})",
                        i + 1,
                        stat.turn,
                        stat.phase,
                        stat.action_description,
                        stat.num_legal_actions,
                        stat.best_action_avg_reward,
                    );
                }
            }
        }
        println!("Avg actions/measured game: {} ({} measured)",
            self.avg_actions.map_or_else(|| "unavailable".into(), |v| format!("{v:.1}")),
            self.actions_samples);
        println!("Avg decisions/measured game: {} ({} measured)",
            self.avg_decisions_per_game.map_or_else(|| "unavailable".into(), |v| format!("{v:.1}")),
            self.decision_samples);
        println!("Avg best-action reward/measured game: {} ({} measured)",
            self.avg_best_reward.map_or_else(|| "unavailable".into(), |v| format!("{v:.4}")),
            self.reward_samples);
    }

    /// Merge another set of results into this one, combining statistics
    /// from multiple runs so that iterative progress accumulates.
    ///
    /// Counts are summed, averages are recomputed from weighted totals,
    /// and the fastest win sequence is kept from whichever run had the
    /// faster kill.
    pub fn merge(&self, other: &MctsGoldfishResults) -> MctsGoldfishResults {
        let total_games = self.total_games + other.total_games;
        let wins = self.wins + other.wins;
        let losses = self.losses + other.losses;
        let draws = self.draws + other.draws;
        let censored = self.censored + other.censored;
        let stalled = self.stalled + other.stalled;
        let invalid = self.invalid + other.invalid;
        let legacy_unknown = self.legacy_unknown + other.legacy_unknown;
        let (avg_kill_turn, kill_turn_samples) = merge_measurement(
            self.avg_kill_turn, self.kill_turn_samples,
            other.avg_kill_turn, other.kill_turn_samples);
        let (avg_actions, actions_samples) = merge_measurement(
            self.avg_actions, self.actions_samples, other.avg_actions, other.actions_samples);
        let (avg_decisions_per_game, decision_samples) = merge_measurement(
            self.avg_decisions_per_game, self.decision_samples,
            other.avg_decisions_per_game, other.decision_samples);
        let (avg_best_reward, reward_samples) = merge_measurement(
            self.avg_best_reward, self.reward_samples,
            other.avg_best_reward, other.reward_samples);

        // Min/max across both runs (handle 0 = no wins).
        let fastest_kill = match (self.wins > 0, other.wins > 0) {
            (true, true) => self.fastest_kill.min(other.fastest_kill),
            (true, false) => self.fastest_kill,
            (false, true) => other.fastest_kill,
            (false, false) => 0,
        };
        let slowest_kill = self.slowest_kill.max(other.slowest_kill);

        // Merge kill-turn distributions (element-wise sum).
        let max_len = self
            .kill_turn_distribution
            .len()
            .max(other.kill_turn_distribution.len());
        let mut kill_turn_distribution = vec![0u64; max_len];
        for (i, v) in self.kill_turn_distribution.iter().enumerate() {
            kill_turn_distribution[i] += v;
        }
        for (i, v) in other.kill_turn_distribution.iter().enumerate() {
            kill_turn_distribution[i] += v;
        }

        // Keep the fastest sequence from whichever run achieved it.
        let fastest_sequence = match (self.wins > 0, other.wins > 0) {
            (true, true) => {
                if other.fastest_kill < self.fastest_kill {
                    other.fastest_sequence.clone()
                } else {
                    self.fastest_sequence.clone()
                }
            }
            (true, false) => self.fastest_sequence.clone(),
            (false, true) => other.fastest_sequence.clone(),
            (false, false) => vec![],
        };

        MctsGoldfishResults {
            schema_version: 3,
            total_games,
            wins,
            losses,
            draws,
            censored,
            stalled,
            invalid,
            legacy_unknown,
            avg_kill_turn,
            kill_turn_samples,
            fastest_kill,
            slowest_kill,
            avg_actions,
            actions_samples,
            avg_decisions_per_game,
            decision_samples,
            avg_best_reward,
            reward_samples,
            kill_turn_distribution,
            fastest_sequence,
        }
    }

    /// Save results to a JSON checkpoint file.
    pub fn save_checkpoint(&self, path: &std::path::Path) -> Result<(), String> {
        let json =
            serde_json::to_string_pretty(self).map_err(|e| format!("serialize: {}", e))?;
        std::fs::write(path, json)
            .map_err(|e| format!("write {}: {}", path.display(), e))
    }

    /// Load results from a JSON checkpoint file.
    pub fn load_checkpoint(path: &std::path::Path) -> Result<MctsGoldfishResults, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {}", path.display(), e))?;
        let value: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| format!("deserialize {}: {}", path.display(), e))?;
        match value.get("schema_version").and_then(|v| v.as_u64()) {
            Some(3) => serde_json::from_value(value)
                .map_err(|e| format!("deserialize {}: {}", path.display(), e)),
            Some(2) => serde_json::from_value::<MctsGoldfishResultsV2>(value)
                .map(Into::into)
                .map_err(|e| format!("deserialize v2 {}: {}", path.display(), e)),
            None | Some(1) => serde_json::from_value::<LegacyMctsGoldfishResults>(value)
                .map(Into::into)
                .map_err(|e| format!("deserialize legacy {}: {}", path.display(), e)),
            Some(version) => Err(format!("unsupported MCTS result schema {version}")),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct LegacyMctsGameResult {
    won: bool,
    kill_turn: u32,
    actions_taken: u32,
    final_life: [i32; 2],
    decision_stats: Vec<DecisionStat>,
    #[serde(skip)]
    trace_lines: Vec<String>,
}

impl From<LegacyMctsGameResult> for MctsGameResult {
    fn from(old: LegacyMctsGameResult) -> Self {
        Self {
            won: old.won,
            outcome: if old.won { MctsOutcome::Win } else { MctsOutcome::LegacyUnknown },
            kill_turn: old.kill_turn,
            actions_taken: old.actions_taken,
            final_life: old.final_life,
            decision_stats: old.decision_stats,
            trace_lines: old.trace_lines,
            loss_boundary: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct LegacyMctsCampaignCheckpoint {
    config: MctsConfig,
    deck_name: String,
    total_games_planned: u64,
    games_completed: u64,
    results: LegacyMctsGoldfishResults,
    game_results: Vec<LegacyMctsGameResult>,
}

#[derive(Serialize, Deserialize)]
struct MctsCampaignCheckpointV2 {
    config: MctsConfig,
    deck_name: String,
    total_games_planned: u64,
    games_completed: u64,
    results: MctsGoldfishResultsV2,
    game_results: Vec<MctsGameResult>,
}

// ---------------------------------------------------------------------------
// Campaign checkpoint: save/resume across sessions
// ---------------------------------------------------------------------------

/// A campaign checkpoint captures aggregate results plus metadata so that
/// a multi-game MCTS run can be resumed across sessions.
///
/// Save after every N games; on resume, load the checkpoint, skip the first
/// `games_completed` games, and continue accumulating into the same results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MctsCampaignCheckpoint {
    /// Config used for this campaign (verified on resume).
    pub config: MctsConfig,
    /// Deck identifier (for human reference / verification).
    pub deck_name: String,
    /// Total number of games planned.
    pub total_games_planned: u64,
    /// Number of games already completed.
    pub games_completed: u64,
    /// Aggregate results accumulated so far.
    pub results: MctsGoldfishResults,
    /// Per-game results for detailed analysis (optional, can be large).
    pub game_results: Vec<MctsGameResult>,
}

impl MctsCampaignCheckpoint {
    /// Create a new empty checkpoint for a fresh campaign.
    pub fn new(config: MctsConfig, deck_name: String, total_games: u64) -> Self {
        let max_turn = GOLDFISH_MAX_TURNS as usize;
        MctsCampaignCheckpoint {
            config,
            deck_name,
            total_games_planned: total_games,
            games_completed: 0,
            results: MctsGoldfishResults {
                schema_version: 3,
                total_games: 0,
                wins: 0,
                losses: 0,
                draws: 0,
                censored: 0,
                stalled: 0,
                invalid: 0,
                legacy_unknown: 0,
                avg_kill_turn: None,
                kill_turn_samples: 0,
                fastest_kill: 0,
                slowest_kill: 0,
                avg_actions: None,
                actions_samples: 0,
                avg_decisions_per_game: None,
                decision_samples: 0,
                avg_best_reward: None,
                reward_samples: 0,
                kill_turn_distribution: vec![0u64; max_turn + 1],
                fastest_sequence: Vec::new(),
            },
            game_results: Vec::new(),
        }
    }

    /// Fold a single game result into the aggregate.
    pub fn add_game(&mut self, result: MctsGameResult) {
        let r = &mut self.results;
        r.total_games += 1;
        self.games_completed += 1;

        let completed = matches!(result.outcome, MctsOutcome::Win | MctsOutcome::Loss | MctsOutcome::Draw);
        if completed {
            add_measurement(&mut r.avg_actions, &mut r.actions_samples, result.actions_taken as f64);
            add_measurement(&mut r.avg_decisions_per_game, &mut r.decision_samples,
                result.decision_stats.len() as f64);
            if !result.decision_stats.is_empty() {
                let avg_reward = result.decision_stats.iter()
                    .map(|decision| decision.best_action_avg_reward).sum::<f64>()
                    / result.decision_stats.len() as f64;
                add_measurement(&mut r.avg_best_reward, &mut r.reward_samples, avg_reward);
            }
        }

        if result.outcome == MctsOutcome::Win {
            r.wins += 1;
            let turn = result.kill_turn;
            add_measurement(&mut r.avg_kill_turn, &mut r.kill_turn_samples, turn as f64);

            if (turn as usize) < r.kill_turn_distribution.len() {
                r.kill_turn_distribution[turn as usize] += 1;
            }
            if r.fastest_kill == 0 || turn < r.fastest_kill {
                r.fastest_kill = turn;
                r.fastest_sequence = result.decision_stats.clone();
            }
            if turn > r.slowest_kill {
                r.slowest_kill = turn;
            }
        } else {
            match result.outcome {
                MctsOutcome::Loss => r.losses += 1,
                MctsOutcome::Draw => r.draws += 1,
                MctsOutcome::Censored => r.censored += 1,
                MctsOutcome::Stalled => r.stalled += 1,
                MctsOutcome::Invalid => r.invalid += 1,
                MctsOutcome::LegacyUnknown => r.legacy_unknown += 1,
                MctsOutcome::Win => unreachable!(),
            }
        }

        self.game_results.push(result);
    }

    /// Save checkpoint to disk (bincode).
    pub fn save(&self, dir: &str) -> Result<(), String> {
        let path = Path::new(dir);
        std::fs::create_dir_all(path).map_err(|e| format!("mkdir: {}", e))?;
        let filename = path.join("mcts_campaign.bin");
        let mut bytes = b"MCTSCAMP3".to_vec();
        bytes.extend(bincode::serialize(self).map_err(|e| format!("serialize: {}", e))?);
        std::fs::write(&filename, bytes).map_err(|e| format!("write: {}", e))?;
        // Also write a human-readable summary
        let summary = path.join("mcts_campaign_summary.txt");
        let text = format!(
            "MCTS Campaign Checkpoint\n\
             ========================\n\
             Deck: {}\n\
             Games: {}/{}\n\
             Win rate: {:.1}%\n\
             Avg kill turn: {} ({} measured)\n\
             Fastest: T{}\n\
             Slowest: T{}\n\
             Config: {} iters, C={:.2}, depth={}\n",
            self.deck_name,
            self.games_completed,
            self.total_games_planned,
            self.results.win_rate() * 100.0,
            self.results.avg_kill_turn.map_or_else(|| "unavailable".into(), |v| format!("{v:.2}")),
            self.results.kill_turn_samples,
            self.results.fastest_kill,
            self.results.slowest_kill,
            self.config.iterations_per_move,
            self.config.exploration_constant,
            self.config.max_tree_depth,
        );
        std::fs::write(&summary, text).map_err(|e| format!("write summary: {}", e))?;
        Ok(())
    }

    /// Load checkpoint from disk.
    pub fn load(dir: &str) -> Result<Self, String> {
        let path = Path::new(dir).join("mcts_campaign.bin");
        let bytes = std::fs::read(&path).map_err(|e| format!("read: {}", e))?;
        if let Some(payload) = bytes.strip_prefix(b"MCTSCAMP3") {
            let checkpoint: Self = bincode::deserialize(payload)
                .map_err(|e| format!("deserialize v3: {}", e))?;
            if checkpoint.results.schema_version != 3 {
                return Err(format!("unsupported MCTS campaign schema {}", checkpoint.results.schema_version));
            }
            return Ok(checkpoint);
        }
        if let Some(payload) = bytes.strip_prefix(b"MCTSCAMP2") {
            let old: MctsCampaignCheckpointV2 = bincode::deserialize(payload)
                .map_err(|e| format!("deserialize v2: {}", e))?;
            if old.results.schema_version != 2 {
                return Err(format!("unsupported MCTS campaign schema {}", old.results.schema_version));
            }
            let complete_records = old.game_results.len() as u64 == old.results.total_games
                && old.results.total_games == old.games_completed;
            let mut migrated = Self::new(old.config, old.deck_name, old.total_games_planned);
            if complete_records {
                for record in old.game_results { migrated.add_game(record); }
            } else {
                migrated.games_completed = old.games_completed;
                migrated.results = old.results.into();
                migrated.game_results = old.game_results;
                recover_record_measurements(&mut migrated.results, &migrated.game_results);
            }
            return Ok(migrated);
        }
        let old: LegacyMctsCampaignCheckpoint = bincode::deserialize(&bytes)
            .map_err(|e| format!("deserialize legacy: {}", e))?;
        let complete_records = old.game_results.len() as u64 == old.results.total_games
            && old.results.total_games == old.games_completed;
        let mut migrated = Self::new(old.config, old.deck_name, old.total_games_planned);
        if complete_records {
            for record in old.game_results { migrated.add_game(record.into()); }
        } else {
            migrated.games_completed = old.games_completed;
            migrated.results = old.results.into();
            migrated.game_results = old.game_results.into_iter().map(Into::into).collect();
            recover_record_measurements(&mut migrated.results, &migrated.game_results);
        }
        Ok(migrated)
    }

    /// How many games remain.
    pub fn games_remaining(&self) -> u64 {
        self.total_games_planned.saturating_sub(self.games_completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_json_cleanup_owner_mismatch_matches_normal_invalid() {
        use crate::card::{CardDef, CardType, ZoneType};
        use crate::game::CardDatabase;
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 90_123, name: "Filler".into(),
            card_types: vec![CardType::Land], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(std::sync::Arc::new(db));
        state.phase = Phase::Cleanup;
        state.turn_number = GOLDFISH_MAX_TURNS;
        state.cleanup_discard_in_progress = true;
        state.active_player = 1;
        state.priority_player = 0;
        for player in 0..2 {
            for _ in 0..8 { state.create_card_in_zone(90_123, player, ZoneType::Hand); }
        }
        let json = serde_json::to_string(&state).unwrap();
        let mut restored: GameState = serde_json::from_str(&json).unwrap();
        restored.card_db = state.card_db.clone();
        assert!(crate::simulation::invalid_cleanup_state(&restored));
        let result = run_mcts_goldfish_game(&mut restored, &MctsConfig::default(), false, None);
        assert_eq!(result.outcome, MctsOutcome::Invalid);
        assert_eq!(result.actions_taken, 0);
    }

    #[test]
    fn legacy_unknown_actions_do_not_reduce_a_new_measured_mean() {
        let legacy = LegacyMctsGoldfishResults {
            total_games: 2, wins: 1, losses: 1, draws: 0,
            avg_kill_turn: 5.0, fastest_kill: 5, slowest_kill: 5,
            avg_actions: 40.0, avg_decisions_per_game: 4.0, avg_best_reward: 0.8,
            kill_turn_distribution: vec![0, 0, 0, 0, 0, 1], fastest_sequence: vec![],
        };
        let mut current = MctsCampaignCheckpoint::new(MctsConfig::default(), "new".into(), 1);
        current.add_game(MctsGameResult {
            won: true, outcome: MctsOutcome::Win, kill_turn: 4,
            actions_taken: 100, final_life: [20, 0],
            decision_stats: vec![DecisionStat { turn: 4, phase: Phase::PreCombatMain,
                num_legal_actions: 1, best_action_visits: 1, best_action_avg_reward: 0.7,
                action_description: "measured".into(), pilot_hand: vec![] }],
            trace_lines: vec![],
            loss_boundary: Default::default(),
        });
        let mixed = MctsGoldfishResults::from(legacy).merge(&current.results);
        assert_eq!(mixed.total_games, 3);
        assert_eq!(mixed.wins, 2);
        assert_eq!(mixed.avg_actions, Some(100.0));
        assert_eq!(mixed.actions_samples, 1);
        assert_eq!(mixed.avg_decisions_per_game, Some(1.0));
        assert_eq!(mixed.decision_samples, 1);
        assert!((mixed.avg_best_reward.unwrap() - 0.7).abs() < 1e-10);
        assert_eq!(mixed.reward_samples, 1);
    }

    #[test]
    fn two_measured_action_counts_average_and_zero_is_a_measurement() {
        let mut campaign = MctsCampaignCheckpoint::new(MctsConfig::default(), "measured".into(), 3);
        for actions in [100, 200, 0] {
            campaign.add_game(MctsGameResult {
                won: true, outcome: MctsOutcome::Win, kill_turn: 5,
                actions_taken: actions, final_life: [20, 0],
                decision_stats: vec![], trace_lines: vec![],
                loss_boundary: Default::default(),
            });
            if actions == 200 {
                assert_eq!(campaign.results.avg_actions, Some(150.0));
                assert_eq!(campaign.results.actions_samples, 2);
            }
        }
        assert_eq!(campaign.results.avg_actions, Some(100.0));
        assert_eq!(campaign.results.actions_samples, 3);
        assert_eq!(campaign.results.avg_decisions_per_game, Some(0.0));
        assert_eq!(campaign.results.decision_samples, 3);
        assert_eq!(campaign.results.avg_best_reward, None);
        assert_eq!(campaign.results.reward_samples, 0);
    }

    #[test]
    fn v2_aggregate_migration_preserves_only_recoverable_measurements() {
        let old = MctsGoldfishResultsV2 {
            schema_version: 2, total_games: 3, wins: 1, losses: 1, draws: 0,
            censored: 0, stalled: 0, invalid: 0, legacy_unknown: 1,
            avg_kill_turn: 5.0, fastest_kill: 5, slowest_kill: 5,
            avg_actions: 50.0, avg_decisions_per_game: 4.0, avg_best_reward: 0.6,
            kill_turn_distribution: vec![0, 0, 0, 0, 0, 1], fastest_sequence: vec![],
        };
        let migrated: MctsGoldfishResults = old.into();
        assert_eq!(migrated.avg_kill_turn, Some(5.0));
        assert_eq!(migrated.kill_turn_samples, 1);
        assert_eq!(migrated.avg_actions, None);
        assert_eq!(migrated.actions_samples, 0);
        assert_eq!(migrated.avg_decisions_per_game, None);
        assert_eq!(migrated.reward_samples, 0);
        let json = serde_json::to_string(&migrated).unwrap();
        let roundtrip: MctsGoldfishResults = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.avg_actions, None);
        assert_eq!(roundtrip.actions_samples, 0);
    }

    #[test]
    fn v1_all_win_aggregate_retains_documented_action_and_decision_means() {
        // V1 add_game averaged actions and decisions over total_games. When
        // every attempt won, that denominator is also the completed count.
        let fastest_sequence = [0.6, 0.8].into_iter().map(|reward| DecisionStat {
            turn: 3, phase: Phase::PreCombatMain, num_legal_actions: 1,
            best_action_visits: 1, best_action_avg_reward: reward,
            action_description: "measured".into(), pilot_hand: vec![],
        }).collect();
        let old = LegacyMctsGoldfishResults {
            total_games: 1, wins: 1, losses: 0, draws: 0,
            avg_kill_turn: 3.0, fastest_kill: 3, slowest_kill: 3,
            avg_actions: 80.0, avg_decisions_per_game: 2.0, avg_best_reward: 0.7,
            kill_turn_distribution: vec![0, 0, 0, 1], fastest_sequence,
        };
        let migrated: MctsGoldfishResults = old.into();
        assert_eq!((migrated.avg_actions, migrated.actions_samples), (Some(80.0), 1));
        assert_eq!((migrated.avg_decisions_per_game, migrated.decision_samples), (Some(2.0), 1));
        // The singleton has decisions, so its V1 reward cannot be a default.
        assert_eq!((migrated.avg_best_reward, migrated.reward_samples), (Some(0.7), 1));
        let mut current = MctsCampaignCheckpoint::new(MctsConfig::default(), "new".into(), 1);
        current.add_game(MctsGameResult {
            won: true, outcome: MctsOutcome::Win, kill_turn: 4,
            actions_taken: 100, final_life: [20, 0],
            decision_stats: vec![DecisionStat { turn: 4, phase: Phase::PreCombatMain,
                num_legal_actions: 1, best_action_visits: 1, best_action_avg_reward: 0.5,
                action_description: "new".into(), pilot_hand: vec![] }], trace_lines: vec![],
            loss_boundary: Default::default(),
        });
        let mixed = migrated.merge(&current.results);
        assert_eq!((mixed.avg_actions, mixed.actions_samples), (Some(90.0), 2));
        assert_eq!((mixed.avg_best_reward, mixed.reward_samples), (Some(0.6), 2));
    }

    #[test]
    fn v1_singleton_measured_zero_reward_is_available() {
        let old = LegacyMctsGoldfishResults {
            total_games: 1, wins: 1, losses: 0, draws: 0,
            avg_kill_turn: 3.0, fastest_kill: 3, slowest_kill: 3,
            avg_actions: 80.0, avg_decisions_per_game: 1.0, avg_best_reward: 0.0,
            kill_turn_distribution: vec![0, 0, 0, 1],
            fastest_sequence: vec![DecisionStat {
                turn: 3, phase: Phase::PreCombatMain, num_legal_actions: 1,
                best_action_visits: 1, best_action_avg_reward: 0.0,
                action_description: "measured zero".into(), pilot_hand: vec![],
            }],
        };
        let migrated: MctsGoldfishResults = old.into();
        assert_eq!((migrated.avg_best_reward, migrated.reward_samples), (Some(0.0), 1));
    }

    #[test]
    fn v1_multi_game_reward_aggregate_cannot_count_defaulted_games() {
        let old = LegacyMctsGoldfishResults {
            total_games: 2, wins: 2, losses: 0, draws: 0,
            avg_kill_turn: 3.0, fastest_kill: 3, slowest_kill: 3,
            avg_actions: 80.0, avg_decisions_per_game: 0.5, avg_best_reward: 0.35,
            kill_turn_distribution: vec![0, 0, 0, 2], fastest_sequence: vec![],
        };
        let migrated: MctsGoldfishResults = old.into();
        assert_eq!((migrated.avg_actions, migrated.actions_samples), (Some(80.0), 2));
        assert_eq!((migrated.avg_best_reward, migrated.reward_samples), (None, 0));
    }

    #[test]
    fn actual_v1_to_v2_compatibility_zero_is_not_a_measured_zero() {
        // Previous V1 -> V2 migration retained the win and kill turn, but
        // wrote zeros into action/decision/reward fields without provenance.
        let old_v1 = LegacyMctsGoldfishResults {
            total_games: 1, wins: 1, losses: 0, draws: 0,
            avg_kill_turn: 3.0, fastest_kill: 3, slowest_kill: 3,
            avg_actions: 80.0, avg_decisions_per_game: 2.0, avg_best_reward: 0.7,
            kill_turn_distribution: vec![0, 0, 0, 1],
            fastest_sequence: [0.6, 0.8].into_iter().map(|reward| DecisionStat {
                turn: 3, phase: Phase::PreCombatMain, num_legal_actions: 1,
                best_action_visits: 1, best_action_avg_reward: reward,
                action_description: "measured".into(), pilot_hand: vec![],
            }).collect(),
        };
        let intermediate = MctsGoldfishResultsV2 {
            schema_version: 2, total_games: old_v1.total_games, wins: old_v1.wins,
            losses: 0, draws: 0, censored: 0, stalled: 0, invalid: 0,
            legacy_unknown: old_v1.total_games - old_v1.wins,
            avg_kill_turn: old_v1.avg_kill_turn,
            fastest_kill: old_v1.fastest_kill, slowest_kill: old_v1.slowest_kill,
            avg_actions: 0.0, avg_decisions_per_game: 0.0, avg_best_reward: 0.0,
            kill_turn_distribution: old_v1.kill_turn_distribution.clone(),
            fastest_sequence: old_v1.fastest_sequence.clone(),
        };
        let direct = MctsGoldfishResults::from(old_v1);
        assert_eq!((direct.avg_best_reward, direct.reward_samples), (Some(0.7), 1));
        let path = std::env::temp_dir().join(format!("mcts-v1-v2-chain-{}.json", std::process::id()));
        std::fs::write(&path, serde_json::to_vec(&intermediate).unwrap()).unwrap();
        let migrated = MctsGoldfishResults::load_checkpoint(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!((migrated.wins, migrated.legacy_unknown), (1, 0));
        assert_eq!((migrated.avg_kill_turn, migrated.kill_turn_samples), (Some(3.0), 1));
        assert_eq!((migrated.avg_actions, migrated.actions_samples), (None, 0));
        assert_eq!((migrated.avg_decisions_per_game, migrated.decision_samples), (None, 0));
        assert_eq!((migrated.avg_best_reward, migrated.reward_samples), (None, 0));

        let mut current = MctsCampaignCheckpoint::new(MctsConfig::default(), "new".into(), 1);
        current.add_game(MctsGameResult {
            won: true, outcome: MctsOutcome::Win, kill_turn: 4,
            actions_taken: 100, final_life: [20, 0],
            decision_stats: vec![DecisionStat { turn: 4, phase: Phase::PreCombatMain,
                num_legal_actions: 1, best_action_visits: 1, best_action_avg_reward: 0.5,
                action_description: "measured".into(), pilot_hand: vec![] }], trace_lines: vec![],
            loss_boundary: Default::default(),
        });
        let mixed = migrated.merge(&current.results);
        assert_eq!((mixed.wins, mixed.avg_actions, mixed.actions_samples), (2, Some(100.0), 1));
        assert_eq!((mixed.avg_decisions_per_game, mixed.decision_samples), (Some(1.0), 1));
        assert_eq!((mixed.avg_best_reward, mixed.reward_samples), (Some(0.5), 1));
        assert_eq!((mixed.avg_kill_turn, mixed.kill_turn_samples), (Some(3.5), 2));
    }

    #[test]
    fn v2_nonzero_aggregate_without_records_has_no_metric_provenance() {
        let old = MctsGoldfishResultsV2 {
            schema_version: 2, total_games: 1, wins: 1, losses: 0, draws: 0,
            censored: 0, stalled: 0, invalid: 0, legacy_unknown: 0,
            avg_kill_turn: 4.0, fastest_kill: 4, slowest_kill: 4,
            avg_actions: 80.0, avg_decisions_per_game: 2.0, avg_best_reward: 0.9,
            kill_turn_distribution: vec![0, 0, 0, 0, 1], fastest_sequence: vec![],
        };
        let migrated: MctsGoldfishResults = old.into();
        assert_eq!((migrated.wins, migrated.avg_kill_turn, migrated.kill_turn_samples),
            (1, Some(4.0), 1));
        assert_eq!((migrated.avg_actions, migrated.actions_samples), (None, 0));
        assert_eq!((migrated.avg_decisions_per_game, migrated.decision_samples), (None, 0));
        assert_eq!((migrated.avg_best_reward, migrated.reward_samples), (None, 0));
    }

    #[test]
    fn v2_partial_campaign_recovers_only_the_present_records() {
        let old = MctsCampaignCheckpointV2 {
            config: MctsConfig::default(), deck_name: "partial".into(),
            total_games_planned: 3, games_completed: 3,
            results: MctsGoldfishResultsV2 {
                schema_version: 2, total_games: 3, wins: 2, losses: 1, draws: 0,
                censored: 0, stalled: 0, invalid: 0, legacy_unknown: 0,
                avg_kill_turn: 5.0, fastest_kill: 4, slowest_kill: 6,
                avg_actions: 0.0, avg_decisions_per_game: 0.0, avg_best_reward: 0.0,
                kill_turn_distribution: vec![0, 0, 0, 0, 1, 0, 1], fastest_sequence: vec![],
            },
            game_results: vec![MctsGameResult {
                won: true, outcome: MctsOutcome::Win, kill_turn: 4,
                actions_taken: 80, final_life: [20, 0],
                decision_stats: vec![DecisionStat { turn: 4, phase: Phase::PreCombatMain,
                    num_legal_actions: 1, best_action_visits: 1, best_action_avg_reward: 0.0,
                    action_description: "measured zero reward".into(), pilot_hand: vec![] }],
                trace_lines: vec![],
                loss_boundary: Default::default(),
            }],
        };
        let dir = std::env::temp_dir().join(format!("mcts-v2-partial-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut bytes = b"MCTSCAMP2".to_vec();
        bytes.extend(bincode::serialize(&old).unwrap());
        std::fs::write(dir.join("mcts_campaign.bin"), bytes).unwrap();
        let mut loaded = MctsCampaignCheckpoint::load(dir.to_str().unwrap()).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!((loaded.results.wins, loaded.results.losses, loaded.results.total_games), (2, 1, 3));
        assert_eq!((loaded.results.avg_kill_turn, loaded.results.kill_turn_samples), (Some(5.0), 2));
        assert_eq!((loaded.results.avg_actions, loaded.results.actions_samples), (Some(80.0), 1));
        assert_eq!((loaded.results.avg_decisions_per_game, loaded.results.decision_samples), (Some(1.0), 1));
        assert_eq!((loaded.results.avg_best_reward, loaded.results.reward_samples), (Some(0.0), 1));
        loaded.add_game(MctsGameResult { won: true, outcome: MctsOutcome::Win, kill_turn: 3,
            actions_taken: 100, final_life: [20, 0], decision_stats: vec![], trace_lines: vec![],
            loss_boundary: Default::default(), });
        assert_eq!((loaded.results.wins, loaded.results.actions_samples, loaded.results.avg_actions),
            (3, 2, Some(90.0)));
        assert_eq!((loaded.results.avg_best_reward, loaded.results.reward_samples), (Some(0.0), 1));
    }

    #[test]
    fn v1_partial_campaign_recovers_record_metrics_without_inventing_outcomes() {
        let old = LegacyMctsCampaignCheckpoint {
            config: MctsConfig::default(), deck_name: "v1 partial".into(),
            total_games_planned: 3, games_completed: 3,
            results: LegacyMctsGoldfishResults {
                total_games: 3, wins: 2, losses: 1, draws: 0,
                avg_kill_turn: 5.0, fastest_kill: 4, slowest_kill: 6,
                avg_actions: 20.0, avg_decisions_per_game: 1.0, avg_best_reward: 0.4,
                kill_turn_distribution: vec![0, 0, 0, 0, 1, 0, 1], fastest_sequence: vec![],
            },
            game_results: vec![LegacyMctsGameResult {
                won: true, kill_turn: 4, actions_taken: 80,
                final_life: [20, 0], decision_stats: vec![DecisionStat {
                    turn: 4, phase: Phase::PreCombatMain, num_legal_actions: 1,
                    best_action_visits: 1, best_action_avg_reward: 0.0,
                    action_description: "known zero".into(), pilot_hand: vec![],
                }], trace_lines: vec![],
            }],
        };
        let dir = std::env::temp_dir().join(format!("mcts-v1-partial-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mcts_campaign.bin"), bincode::serialize(&old).unwrap()).unwrap();
        let loaded = MctsCampaignCheckpoint::load(dir.to_str().unwrap()).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!((loaded.results.total_games, loaded.results.wins, loaded.results.legacy_unknown),
            (3, 2, 1));
        assert_eq!((loaded.results.avg_kill_turn, loaded.results.kill_turn_samples), (Some(5.0), 2));
        assert_eq!((loaded.results.avg_actions, loaded.results.actions_samples), (Some(80.0), 1));
        assert_eq!((loaded.results.avg_decisions_per_game, loaded.results.decision_samples), (Some(1.0), 1));
        assert_eq!((loaded.results.avg_best_reward, loaded.results.reward_samples), (Some(0.0), 1));
    }

    #[test]
    fn complete_v1_and_v2_campaign_records_preserve_equal_evidence() {
        let decisions: Vec<DecisionStat> = [0.6, 0.8].into_iter().map(|reward| DecisionStat {
            turn: 4, phase: Phase::PreCombatMain, num_legal_actions: 1,
            best_action_visits: 1, best_action_avg_reward: reward,
            action_description: "measured".into(), pilot_hand: vec![],
        }).collect();
        let old_v1 = LegacyMctsCampaignCheckpoint {
            config: MctsConfig::default(), deck_name: "chain".into(),
            total_games_planned: 1, games_completed: 1,
            results: LegacyMctsGoldfishResults {
                total_games: 1, wins: 1, losses: 0, draws: 0,
                avg_kill_turn: 4.0, fastest_kill: 4, slowest_kill: 4,
                avg_actions: 80.0, avg_decisions_per_game: 2.0, avg_best_reward: 0.7,
                kill_turn_distribution: vec![0, 0, 0, 0, 1], fastest_sequence: vec![],
            },
            game_results: vec![LegacyMctsGameResult {
                won: true, kill_turn: 4, actions_taken: 80, final_life: [20, 0],
                decision_stats: decisions.clone(), trace_lines: vec![],
            }],
        };
        let dir = std::env::temp_dir().join(format!("mcts-evidence-chain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mcts_campaign.bin"), bincode::serialize(&old_v1).unwrap()).unwrap();
        let direct = MctsCampaignCheckpoint::load(dir.to_str().unwrap()).unwrap();
        let old_v2 = MctsCampaignCheckpointV2 {
            config: MctsConfig::default(), deck_name: "chain".into(),
            total_games_planned: 1, games_completed: 1,
            results: MctsGoldfishResultsV2 {
                schema_version: 2, total_games: 1, wins: 1, losses: 0, draws: 0,
                censored: 0, stalled: 0, invalid: 0, legacy_unknown: 0,
                avg_kill_turn: 4.0, fastest_kill: 4, slowest_kill: 4,
                avg_actions: 0.0, avg_decisions_per_game: 0.0, avg_best_reward: 0.0,
                kill_turn_distribution: vec![0, 0, 0, 0, 1], fastest_sequence: vec![],
            },
            game_results: vec![MctsGameResult {
                won: true, outcome: MctsOutcome::Win, kill_turn: 4, actions_taken: 80,
                final_life: [20, 0], decision_stats: decisions, trace_lines: vec![],
                loss_boundary: Default::default(),
            }],
        };
        let mut bytes = b"MCTSCAMP2".to_vec();
        bytes.extend(bincode::serialize(&old_v2).unwrap());
        std::fs::write(dir.join("mcts_campaign.bin"), bytes).unwrap();
        let chained = MctsCampaignCheckpoint::load(dir.to_str().unwrap()).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!((direct.results.avg_actions, direct.results.actions_samples),
            (chained.results.avg_actions, chained.results.actions_samples));
        assert_eq!((direct.results.avg_decisions_per_game, direct.results.decision_samples),
            (chained.results.avg_decisions_per_game, chained.results.decision_samples));
        assert_eq!((direct.results.avg_best_reward, direct.results.reward_samples),
            (chained.results.avg_best_reward, chained.results.reward_samples));
        assert_eq!((direct.results.avg_kill_turn, direct.results.kill_turn_samples),
            (chained.results.avg_kill_turn, chained.results.kill_turn_samples));
        assert_eq!((chained.results.avg_actions, chained.results.actions_samples), (Some(80.0), 1));
        assert_eq!((chained.results.avg_decisions_per_game, chained.results.decision_samples),
            (Some(2.0), 1));
        assert_eq!((chained.results.avg_best_reward, chained.results.reward_samples),
            (Some(0.7), 1));
    }

    #[test]
    fn v3_json_and_bincode_keep_independent_metric_availability() {
        let historical: MctsGoldfishResults = MctsGoldfishResultsV2 {
            schema_version: 2, total_games: 1, wins: 1, losses: 0, draws: 0,
            censored: 0, stalled: 0, invalid: 0, legacy_unknown: 0,
            avg_kill_turn: 3.0, fastest_kill: 3, slowest_kill: 3,
            avg_actions: 0.0, avg_decisions_per_game: 0.0, avg_best_reward: 0.0,
            kill_turn_distribution: vec![0, 0, 0, 1], fastest_sequence: vec![],
        }.into();
        let mut current = MctsCampaignCheckpoint::new(MctsConfig::default(), "roundtrip".into(), 1);
        current.add_game(MctsGameResult {
            won: true, outcome: MctsOutcome::Win, kill_turn: 4,
            actions_taken: 0, final_life: [20, 0],
            decision_stats: vec![], trace_lines: vec![],
            loss_boundary: Default::default(),
        });
        let mixed = historical.merge(&current.results);
        assert_eq!((mixed.avg_actions, mixed.actions_samples), (Some(0.0), 1));
        assert_eq!((mixed.avg_decisions_per_game, mixed.decision_samples), (Some(0.0), 1));
        assert_eq!((mixed.avg_best_reward, mixed.reward_samples), (None, 0));
        let path = std::env::temp_dir().join(format!("mcts-v3-availability-{}.json", std::process::id()));
        mixed.save_checkpoint(&path).unwrap();
        let json = MctsGoldfishResults::load_checkpoint(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        let binary: MctsGoldfishResults = bincode::deserialize(&bincode::serialize(&mixed).unwrap()).unwrap();
        for restored in [json, binary] {
            assert_eq!((restored.wins, restored.avg_kill_turn, restored.kill_turn_samples),
                (2, Some(3.5), 2));
            assert_eq!((restored.avg_actions, restored.actions_samples), (Some(0.0), 1));
            assert_eq!((restored.avg_decisions_per_game, restored.decision_samples), (Some(0.0), 1));
            assert_eq!((restored.avg_best_reward, restored.reward_samples), (None, 0));
        }
    }

    #[test]
    fn v3_known_reward_and_unknown_actions_remain_independent() {
        // V3 persists availability per metric. A known outcome and reward
        // never imply that an unrelated action aggregate was measured.
        let mut persisted: MctsGoldfishResults = MctsGoldfishResultsV2 {
            schema_version: 2, total_games: 1, wins: 1, losses: 0, draws: 0,
            censored: 0, stalled: 0, invalid: 0, legacy_unknown: 0,
            avg_kill_turn: 3.0, fastest_kill: 3, slowest_kill: 3,
            avg_actions: 0.0, avg_decisions_per_game: 0.0, avg_best_reward: 0.0,
            kill_turn_distribution: vec![0, 0, 0, 1], fastest_sequence: vec![],
        }.into();
        persisted.avg_best_reward = Some(0.7);
        persisted.reward_samples = 1;
        let path = std::env::temp_dir().join(format!("mcts-v3-reward-{}.json", std::process::id()));
        persisted.save_checkpoint(&path).unwrap();
        let json = MctsGoldfishResults::load_checkpoint(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        let binary: MctsGoldfishResults = bincode::deserialize(&bincode::serialize(&persisted).unwrap()).unwrap();
        for restored in [json, binary] {
            assert_eq!((restored.wins, restored.avg_actions, restored.actions_samples), (1, None, 0));
            assert_eq!((restored.avg_best_reward, restored.reward_samples), (Some(0.7), 1));
            let mut current = MctsCampaignCheckpoint::new(MctsConfig::default(), "current".into(), 1);
            current.add_game(MctsGameResult {
                won: true, outcome: MctsOutcome::Win, kill_turn: 4,
                actions_taken: 100, final_life: [20, 0],
                decision_stats: vec![DecisionStat {
                    turn: 4, phase: Phase::PreCombatMain, num_legal_actions: 1,
                    best_action_visits: 1, best_action_avg_reward: 0.5,
                    action_description: "current".into(), pilot_hand: vec![],
                }], trace_lines: vec![],
                loss_boundary: Default::default(),
            });
            let merged = restored.merge(&current.results);
            assert_eq!((merged.wins, merged.avg_actions, merged.actions_samples),
                (2, Some(100.0), 1));
            assert_eq!((merged.avg_best_reward, merged.reward_samples), (Some(0.6), 2));
        }
    }

    #[test]
    fn v2_bincode_campaign_recovers_measurements_from_complete_records() {
        let old = MctsCampaignCheckpointV2 {
            config: MctsConfig::default(), deck_name: "v2".into(),
            total_games_planned: 2, games_completed: 2,
            results: MctsGoldfishResultsV2 {
                schema_version: 2, total_games: 2, wins: 1, losses: 0, draws: 0,
                censored: 0, stalled: 0, invalid: 0, legacy_unknown: 1,
                avg_kill_turn: 5.0, fastest_kill: 5, slowest_kill: 5,
                avg_actions: 20.0, avg_decisions_per_game: 0.5, avg_best_reward: 0.3,
                kill_turn_distribution: vec![0, 0, 0, 0, 0, 1], fastest_sequence: vec![],
            },
            game_results: vec![
                MctsGameResult { won: true, outcome: MctsOutcome::Win, kill_turn: 5,
                    actions_taken: 30, final_life: [20, 0], decision_stats: vec![], trace_lines: vec![],
                    loss_boundary: Default::default(), },
                MctsGameResult { won: false, outcome: MctsOutcome::LegacyUnknown, kill_turn: 8,
                    actions_taken: 10, final_life: [20, 20], decision_stats: vec![], trace_lines: vec![],
                    loss_boundary: Default::default(), },
            ],
        };
        let dir = std::env::temp_dir().join(format!("mcts-v2-migration-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut bytes = b"MCTSCAMP2".to_vec();
        bytes.extend(bincode::serialize(&old).unwrap());
        std::fs::write(dir.join("mcts_campaign.bin"), bytes).unwrap();
        let loaded = MctsCampaignCheckpoint::load(dir.to_str().unwrap()).unwrap();
        assert_eq!(loaded.results.avg_actions, Some(30.0));
        assert_eq!(loaded.results.actions_samples, 1);
        assert_eq!(loaded.results.avg_decisions_per_game, Some(0.0));
        assert_eq!(loaded.results.decision_samples, 1);
        assert_eq!(loaded.results.avg_best_reward, None);
        assert_eq!(loaded.results.legacy_unknown, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn mcts_and_normal_classify_coherent_rejection_and_terminal_limits_equally() {
        use crate::card::{CardDef, CardType, ZoneType};
        use crate::game::CardDatabase;
        use crate::simulation::{classify_outcome, GameOutcome, TerminationReason};
        use std::sync::Arc;
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 90_124, name: "Filler".into(),
            card_types: vec![CardType::Land], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::Cleanup;
        state.cleanup_discard_in_progress = true;
        for _ in 0..8 { state.create_card_in_zone(90_124, 0, ZoneType::Hand); }
        let shared_rejection = classify_outcome(&state, 20, 10_000, 0,
            Some(GameOutcome::Stalled(TerminationReason::RejectedAction)));
        let legal = legal_actions(&state);
        let mut actions_taken = 0;
        let mut rejected_in_row = 0;
        for _ in 0..2 {
            assert!(apply_mcts_game_action(&mut state, &Action::PassPriority, &legal,
                &mut actions_taken, &mut rejected_in_row).unwrap() == false);
        }
        let rejection = apply_mcts_game_action(&mut state, &Action::PassPriority, &legal,
            &mut actions_taken, &mut rejected_in_row).unwrap_err();
        assert_eq!(actions_taken, 0);
        assert_eq!(rejection, shared_rejection);
        let config = MctsConfig::default();

        let mut terminal = GameState::new(2);
        terminal.game_over = true;
        terminal.winner = Some(0);
        assert_eq!(MctsOutcome::from(classify_outcome(&terminal, 20, 10_000, 0, None)),
            run_mcts_goldfish_game(&mut terminal, &config, false, None).outcome);
        let mut limit = GameState::new(2);
        limit.turn_number = 21;
        assert_eq!(MctsOutcome::from(classify_outcome(&limit, 20, 10_000, 0, None)),
            run_mcts_goldfish_game(&mut limit, &config, false, None).outcome);
    }

    #[test]
    fn explicit_outcomes_separate_completed_games_from_attempts() {
        let mut campaign = MctsCampaignCheckpoint::new(MctsConfig::default(), "test".into(), 7);
        for outcome in [MctsOutcome::Win, MctsOutcome::Loss, MctsOutcome::Draw,
            MctsOutcome::Censored, MctsOutcome::Stalled, MctsOutcome::Invalid,
            MctsOutcome::LegacyUnknown] {
            campaign.add_game(MctsGameResult {
                won: outcome == MctsOutcome::Win,
                outcome,
                kill_turn: 5,
                actions_taken: 10,
                final_life: [20, 20],
                decision_stats: Vec::new(),
                trace_lines: Vec::new(),
                loss_boundary: Default::default(),
            });
        }
        assert_eq!(campaign.results.total_games, 7);
        assert_eq!(campaign.results.completed_games(), 3);
        assert_eq!((campaign.results.wins, campaign.results.losses, campaign.results.draws), (1, 1, 1));
        assert_eq!((campaign.results.censored, campaign.results.stalled,
            campaign.results.invalid, campaign.results.legacy_unknown), (1, 1, 1, 1));
        assert!((campaign.results.win_rate() - 1.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn versioned_campaign_round_trips_every_outcome() {
        let mut campaign = MctsCampaignCheckpoint::new(MctsConfig::default(), "test".into(), 7);
        for outcome in [MctsOutcome::Win, MctsOutcome::Loss, MctsOutcome::Draw,
            MctsOutcome::Censored, MctsOutcome::Stalled, MctsOutcome::Invalid,
            MctsOutcome::LegacyUnknown] {
            campaign.add_game(MctsGameResult { won: outcome == MctsOutcome::Win, outcome,
                kill_turn: 5, actions_taken: 10, final_life: [20, 20],
                decision_stats: Vec::new(), trace_lines: Vec::new(),
                loss_boundary: Default::default(), });
        }
        let dir = std::env::temp_dir().join(format!("mcts-v2-{}", std::process::id()));
        campaign.save(dir.to_str().unwrap()).unwrap();
        let bytes = std::fs::read(dir.join("mcts_campaign.bin")).unwrap();
        assert!(bytes.starts_with(b"MCTSCAMP3"));
        let loaded = MctsCampaignCheckpoint::load(dir.to_str().unwrap()).unwrap();
        assert_eq!(loaded.game_results.iter().map(|r| r.outcome).collect::<Vec<_>>(),
            campaign.game_results.iter().map(|r| r.outcome).collect::<Vec<_>>());
        assert_eq!(loaded.results.completed_games(), 3);
        assert_eq!(loaded.results.actions_samples, 3);
        assert_eq!(loaded.results.reward_samples, 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn old_json_aggregate_marks_ambiguous_remainder_unknown() {
        let path = std::env::temp_dir().join(format!("mcts-old-{}.json", std::process::id()));
        let mut old = serde_json::to_value(make_results(4, 1, 1, 2, 5.0, 5, 5,
            vec![0, 0, 0, 0, 0, 1], vec![])).unwrap();
        for key in ["schema_version", "censored", "stalled", "invalid", "legacy_unknown",
            "kill_turn_samples", "actions_samples", "decision_samples", "reward_samples"] {
            old.as_object_mut().unwrap().remove(key);
        }
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        let loaded = MctsGoldfishResults::load_checkpoint(&path).unwrap();
        assert_eq!((loaded.wins, loaded.losses, loaded.draws, loaded.legacy_unknown), (1, 0, 0, 3));
        assert_eq!(loaded.completed_games(), 1);
        assert_eq!(loaded.avg_actions, None);
        assert_eq!(loaded.actions_samples, 0);
        assert_eq!(loaded.avg_kill_turn, Some(5.0));
        assert_eq!(loaded.kill_turn_samples, 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn old_bincode_campaign_preserves_attempts_without_inventing_nonwin_outcomes() {
        let old = LegacyMctsCampaignCheckpoint {
            config: MctsConfig::default(), deck_name: "old".into(),
            total_games_planned: 3, games_completed: 2,
            results: LegacyMctsGoldfishResults {
                total_games: 2, wins: 1, losses: 1, draws: 0,
                avg_kill_turn: 4.0, fastest_kill: 4, slowest_kill: 4,
                avg_actions: 10.0, avg_decisions_per_game: 2.0, avg_best_reward: 0.5,
                kill_turn_distribution: vec![0, 0, 0, 0, 1], fastest_sequence: vec![],
            },
            game_results: vec![
                LegacyMctsGameResult { won: true, kill_turn: 4, actions_taken: 10,
                    final_life: [20, 0], decision_stats: vec![], trace_lines: vec![] },
                LegacyMctsGameResult { won: false, kill_turn: 21, actions_taken: 10,
                    final_life: [0, 0], decision_stats: vec![], trace_lines: vec![] },
            ],
        };
        let dir = std::env::temp_dir().join(format!("mcts-v1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mcts_campaign.bin"), bincode::serialize(&old).unwrap()).unwrap();
        let loaded = MctsCampaignCheckpoint::load(dir.to_str().unwrap()).unwrap();
        assert_eq!(loaded.games_completed, 2);
        assert_eq!((loaded.results.wins, loaded.results.losses, loaded.results.draws,
            loaded.results.legacy_unknown), (1, 0, 0, 1));
        assert_eq!(loaded.game_results[1].outcome, MctsOutcome::LegacyUnknown);
        assert_eq!(loaded.results.avg_actions, Some(10.0));
        assert_eq!(loaded.results.actions_samples, 1);
        assert_eq!(loaded.results.reward_samples, 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn execution_distinguishes_rules_draw_from_horizon_and_invalid_state() {
        let config = MctsConfig::default();
        let mut draw = GameState::new(2);
        draw.game_over = true;
        draw.winner = None;
        assert_eq!(run_mcts_goldfish_game(&mut draw, &config, false, None).outcome, MctsOutcome::Draw);
        let mut censored = GameState::new(2);
        censored.turn_number = GOLDFISH_MAX_TURNS + 1;
        assert_eq!(run_mcts_goldfish_game(&mut censored, &config, false, None).outcome,
            MctsOutcome::Censored);
        let mut invalid = GameState::new(2);
        invalid.winner = Some(0);
        assert_eq!(run_mcts_goldfish_game(&mut invalid, &config, false, None).outcome,
            MctsOutcome::Invalid);
    }

    #[test]
    fn locked_empty_choice_is_stalled_instead_of_action_limit_censored() {
        let mut state = GameState::new(2);
        state.phase = Phase::PreCombatMain;
        state.pending_copy_order = Some(crate::game::PendingCopyOrder {
            controller: 1, items: vec![], selected_order: vec![],
            expected_stack_len: 0, expected_next_stack_id: 1,
            resolving_entry: None, resolving_source_generation: None,
        });
        let result = run_mcts_goldfish_game(&mut state, &MctsConfig::default(), false, None);
        assert_eq!(result.outcome, MctsOutcome::Stalled);
        assert_eq!(result.actions_taken, 0);
    }

    #[test]
    fn test_goldfish_reward() {
        // Win on turn 1 → highest reward (opponent life irrelevant for wins)
        assert!((goldfish_reward(Some(0), 1, 0, 20, 0.0) - 1.0).abs() < 1e-10);
        // Win on turn 20 → lowest positive reward
        assert!((goldfish_reward(Some(0), 20, 0, 20, 0.0) - 1.0 / 20.0).abs() < 1e-10);
        // Win on turn 5
        assert!((goldfish_reward(Some(0), 5, 0, 20, 0.0) - 16.0 / 20.0).abs() < 1e-10);
        // Loss with no damage → 0.0
        assert!((goldfish_reward(Some(1), 5, 20, 20, 0.0)).abs() < 1e-10);
        // Draw with no damage → 0.0
        assert!((goldfish_reward(None, 20, 20, 20, 0.0)).abs() < 1e-10);
    }

    #[test]
    fn test_goldfish_reward_shaping() {
        // No kill, no damage, no combo → 0.0
        assert!((goldfish_reward(None, 20, 40, 40, 0.0)).abs() < 1e-10);
        // No kill, half damage, no combo (40 → 20) → 0.5 * 0.02 = 0.01
        assert!((goldfish_reward(None, 20, 20, 40, 0.0) - 0.01).abs() < 1e-10);
        // No kill, no damage, full combo proximity → 0.02
        assert!((goldfish_reward(None, 20, 40, 40, 1.0) - 0.02).abs() < 1e-10);
        // No kill, half damage + half combo → 0.01 + 0.01 = 0.02
        assert!((goldfish_reward(None, 20, 20, 40, 0.5) - 0.02).abs() < 1e-10);
        // Shaping reward is always less than worst win (T20 = 0.05)
        let worst_win = goldfish_reward(Some(0), 20, 0, 40, 0.0);
        let best_shaping = goldfish_reward(None, 20, 0, 40, 1.0);
        assert!(best_shaping < worst_win);
        // Standard format: no kill, half damage (20 → 10) → 0.5 * 0.02 = 0.01
        assert!((goldfish_reward(None, 20, 10, 20, 0.0) - 0.01).abs() < 1e-10);
    }

    #[test]
    fn test_ucb1_select_unvisited_first() {
        // Unvisited children should be selected before visited ones
        let children = vec![
            MctsChild {
                action: Action::PassPriority,
                node: MctsNode {
                    visits: 10,
                    total_reward: 5.0,
                    children: None,
                },
            },
            MctsChild {
                action: Action::PassPriority,
                node: MctsNode::new(), // visits = 0
            },
        ];
        // Unvisited child (index 1) should be selected
        assert_eq!(ucb1_select(&children, 10, 1.0), 1);
    }

    #[test]
    fn test_ucb1_select_exploitation() {
        // With very low exploration constant, prefer high-reward child
        let children = vec![
            MctsChild {
                action: Action::PassPriority,
                node: MctsNode {
                    visits: 100,
                    total_reward: 90.0, // avg = 0.9
                    children: None,
                },
            },
            MctsChild {
                action: Action::PassPriority,
                node: MctsNode {
                    visits: 100,
                    total_reward: 10.0, // avg = 0.1
                    children: None,
                },
            },
        ];
        // With c=0 (pure exploitation), prefer the higher-reward child
        assert_eq!(ucb1_select(&children, 200, 0.0), 0);
    }

    #[test]
    fn test_mcts_node_avg_reward() {
        let node = MctsNode {
            visits: 10,
            total_reward: 7.5,
            children: None,
        };
        assert!((node.avg_reward() - 0.75).abs() < 1e-10);

        let empty = MctsNode::new();
        assert!((empty.avg_reward()).abs() < 1e-10);
    }

    #[test]
    fn test_expand_node() {
        let mut node = MctsNode::new();
        assert!(!node.is_expanded());

        let actions = vec![Action::PassPriority, Action::PlayLand { object_id: 42 }];
        expand_node(&mut node, &actions);

        assert!(node.is_expanded());
        let children = node.children.as_ref().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].action, Action::PassPriority);
    }

    #[test]
    fn test_mcts_config_num_threads_default() {
        let config = MctsConfig::default();
        assert_eq!(config.num_threads, 1);
    }

    #[test]
    fn test_parallel_merge_sums_visits() {
        // Verify that mcts_search_parallel correctly merges per-action stats.
        // We test the merge logic directly by constructing thread results.
        let actions = vec![
            Action::PassPriority,
            Action::PlayLand { object_id: 1 },
            Action::PlayLand { object_id: 2 },
        ];

        let mut root = MctsNode::new();
        expand_node(&mut root, &actions);

        // Simulate two threads' results
        let thread_results: Vec<Vec<(u32, f64)>> = vec![
            vec![(10, 5.0), (20, 8.0), (5, 2.0)],
            vec![(15, 7.0), (10, 4.0), (8, 3.0)],
        ];

        // Apply merge logic (same as mcts_search_parallel)
        let num_actions = actions.len();
        let mut merged_visits = vec![0u32; num_actions];
        let mut merged_rewards = vec![0.0f64; num_actions];

        for thread_result in &thread_results {
            for (i, &(visits, reward)) in thread_result.iter().enumerate() {
                if i < num_actions {
                    merged_visits[i] += visits;
                    merged_rewards[i] += reward;
                }
            }
        }

        assert_eq!(merged_visits, vec![25, 30, 13]);
        assert!((merged_rewards[0] - 12.0).abs() < 1e-10);
        assert!((merged_rewards[1] - 12.0).abs() < 1e-10);
        assert!((merged_rewards[2] - 5.0).abs() < 1e-10);

        // Update root and verify best action is PlayLand{1} (most visits = 30)
        let total_visits: u32 = merged_visits.iter().sum();
        let total_reward: f64 = merged_rewards.iter().sum();
        root.visits = total_visits;
        root.total_reward = total_reward;

        if let Some(children) = root.children.as_mut() {
            for (i, child) in children.iter_mut().enumerate() {
                child.node.visits = merged_visits[i];
                child.node.total_reward = merged_rewards[i];
            }
        }

        let best = best_action_by_visits(&root);
        assert_eq!(best, Some(Action::PlayLand { object_id: 1 }));
    }

    #[test]
    fn test_iteration_distribution_across_threads() {
        // Verify iterations are split correctly with remainder handling.
        let total_iters: u32 = 103;
        let num_threads: u32 = 4;
        let base = total_iters / num_threads; // 25
        let remainder = total_iters % num_threads; // 3

        let mut assigned: Vec<u32> = Vec::new();
        for t in 0..num_threads {
            assigned.push(base + if t < remainder { 1 } else { 0 });
        }

        // First 3 threads get 26, last gets 25
        assert_eq!(assigned, vec![26, 26, 26, 25]);
        assert_eq!(assigned.iter().sum::<u32>(), total_iters);
    }

    /// Helper to create a test MctsGoldfishResults with given parameters.
    fn make_results(
        total_games: u64,
        wins: u64,
        losses: u64,
        draws: u64,
        avg_kill_turn: f64,
        fastest_kill: u32,
        slowest_kill: u32,
        kill_turn_distribution: Vec<u64>,
        fastest_sequence: Vec<DecisionStat>,
    ) -> MctsGoldfishResults {
        let completed = wins + losses + draws;
        MctsGoldfishResults {
            schema_version: 3,
            total_games,
            wins,
            losses,
            draws,
            censored: 0,
            stalled: 0,
            invalid: 0,
            legacy_unknown: 0,
            avg_kill_turn: (wins > 0).then_some(avg_kill_turn),
            kill_turn_samples: wins,
            fastest_kill,
            slowest_kill,
            avg_actions: (completed > 0).then_some(50.0),
            actions_samples: completed,
            avg_decisions_per_game: (completed > 0).then_some(10.0),
            decision_samples: completed,
            avg_best_reward: (completed > 0).then_some(0.7),
            reward_samples: completed,
            kill_turn_distribution,
            fastest_sequence,
        }
    }

    #[test]
    fn test_merge_sums_counts() {
        let a = make_results(100, 80, 5, 15, 5.0, 3, 8, vec![0, 0, 0, 10, 30, 40], vec![]);
        let b = make_results(50, 45, 2, 3, 4.5, 2, 7, vec![0, 0, 5, 15, 15, 10], vec![]);

        let merged = a.merge(&b);

        assert_eq!(merged.total_games, 150);
        assert_eq!(merged.wins, 125);
        assert_eq!(merged.losses, 7);
        assert_eq!(merged.draws, 18);
    }

    #[test]
    fn test_merge_recomputes_averages() {
        // a: 100 games, 80 wins, avg_kill=5.0  → total_kill_turns = 400
        // b: 50 games, 40 wins, avg_kill=4.0  → total_kill_turns = 160
        // merged: 120 wins, total_kill_turns = 560, avg = 560/120 ≈ 4.667
        let a = make_results(100, 80, 10, 10, 5.0, 3, 8, vec![], vec![]);
        let b = make_results(50, 40, 5, 5, 4.0, 2, 6, vec![], vec![]);

        let merged = a.merge(&b);

        let expected_avg = (5.0 * 80.0 + 4.0 * 40.0) / 120.0;
        assert!((merged.avg_kill_turn.unwrap() - expected_avg).abs() < 1e-10);
    }

    #[test]
    fn test_merge_fastest_slowest() {
        let a = make_results(100, 80, 10, 10, 5.0, 3, 8, vec![], vec![]);
        let b = make_results(50, 40, 5, 5, 4.0, 2, 9, vec![], vec![]);

        let merged = a.merge(&b);

        assert_eq!(merged.fastest_kill, 2);
        assert_eq!(merged.slowest_kill, 9);
    }

    #[test]
    fn test_merge_keeps_faster_sequence() {
        let seq_a = vec![DecisionStat {
            turn: 3,
            phase: Phase::PreCombatMain,
            num_legal_actions: 5,
            best_action_visits: 100,
            best_action_avg_reward: 0.8,
            action_description: "Cast Bolt".to_string(),
            pilot_hand: vec![],
        }];
        let seq_b = vec![DecisionStat {
            turn: 2,
            phase: Phase::PreCombatMain,
            num_legal_actions: 3,
            best_action_visits: 80,
            best_action_avg_reward: 0.9,
            action_description: "Cast Elf".to_string(),
            pilot_hand: vec![],
        }];

        let a = make_results(100, 80, 10, 10, 5.0, 3, 8, vec![], seq_a);
        let b = make_results(50, 40, 5, 5, 4.0, 2, 6, vec![], seq_b);

        let merged = a.merge(&b);
        // b has faster kill (T2 < T3), so its sequence should be kept
        assert_eq!(merged.fastest_sequence.len(), 1);
        assert_eq!(merged.fastest_sequence[0].action_description, "Cast Elf");
    }

    #[test]
    fn test_merge_distribution() {
        // a: [0, 0, 0, 10, 20]
        // b: [0, 0, 5, 15, 10, 3]  (longer)
        let a = make_results(30, 30, 0, 0, 4.0, 3, 4, vec![0, 0, 0, 10, 20], vec![]);
        let b = make_results(33, 33, 0, 0, 4.0, 2, 5, vec![0, 0, 5, 15, 10, 3], vec![]);

        let merged = a.merge(&b);

        assert_eq!(merged.kill_turn_distribution, vec![0, 0, 5, 25, 30, 3]);
    }

    #[test]
    fn test_merge_with_no_wins() {
        let seq_b = vec![DecisionStat {
            turn: 3,
            phase: Phase::PreCombatMain,
            num_legal_actions: 4,
            best_action_visits: 90,
            best_action_avg_reward: 0.75,
            action_description: "Cast Llanowar Elves".to_string(),
            pilot_hand: vec![],
        }];
        let a = make_results(10, 0, 0, 10, 0.0, 0, 0, vec![], vec![]);
        let b = make_results(20, 15, 0, 5, 5.0, 3, 7, vec![0, 0, 0, 5, 5, 5], seq_b);

        let merged = a.merge(&b);

        assert_eq!(merged.wins, 15);
        assert_eq!(merged.fastest_kill, 3);
        assert_eq!(merged.slowest_kill, 7);
        assert!((merged.avg_kill_turn.unwrap() - 5.0).abs() < 1e-10);
        // Sequence from b should be kept since a has no wins
        assert_eq!(merged.fastest_sequence.len(), 1);
        assert_eq!(
            merged.fastest_sequence[0].action_description,
            "Cast Llanowar Elves"
        );
    }

    /// Drop guard that removes a file when it goes out of scope,
    /// ensuring cleanup even if a test panics.
    struct TempFileGuard(std::path::PathBuf);
    impl Drop for TempFileGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn test_checkpoint_round_trip() {
        let results = make_results(
            100, 80, 10, 10, 5.0, 3, 8,
            vec![0, 0, 0, 10, 30, 40],
            vec![DecisionStat {
                turn: 3,
                phase: Phase::PreCombatMain,
                num_legal_actions: 5,
                best_action_visits: 400,
                best_action_avg_reward: 0.85,
                action_description: "Cast Lightning Bolt".to_string(),
                pilot_hand: vec![],
            }],
        );

        let path = std::env::temp_dir().join("mcts_test_checkpoint.json");
        let _guard = TempFileGuard(path.clone());

        results.save_checkpoint(&path).unwrap();
        let loaded = MctsGoldfishResults::load_checkpoint(&path).unwrap();

        assert_eq!(loaded.total_games, results.total_games);
        assert_eq!(loaded.wins, results.wins);
        assert_eq!(loaded.losses, results.losses);
        assert_eq!(loaded.draws, results.draws);
        assert!((loaded.avg_kill_turn.unwrap() - results.avg_kill_turn.unwrap()).abs() < 1e-10);
        assert_eq!(loaded.kill_turn_samples, results.kill_turn_samples);
        assert_eq!(loaded.avg_actions, results.avg_actions);
        assert_eq!(loaded.actions_samples, results.actions_samples);
        assert_eq!(loaded.avg_decisions_per_game, results.avg_decisions_per_game);
        assert_eq!(loaded.decision_samples, results.decision_samples);
        assert_eq!(loaded.avg_best_reward, results.avg_best_reward);
        assert_eq!(loaded.reward_samples, results.reward_samples);
        assert_eq!(loaded.fastest_kill, results.fastest_kill);
        assert_eq!(loaded.slowest_kill, results.slowest_kill);
        assert_eq!(loaded.kill_turn_distribution, results.kill_turn_distribution);
        assert_eq!(loaded.fastest_sequence.len(), 1);
        assert_eq!(
            loaded.fastest_sequence[0].action_description,
            "Cast Lightning Bolt"
        );
    }

    #[test]
    fn test_checkpoint_load_missing_file() {
        let path = std::path::Path::new("/tmp/nonexistent_mcts_ckpt_12345.json");
        let result = MctsGoldfishResults::load_checkpoint(path);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod loss_boundary_search_tests {
    use super::*;

    fn unsupported_resume() -> GameState {
        let mut state = GameState::new(3);
        state.players[0].has_lost = true;
        state.turn_number = u32::MAX;
        state.loss_boundary.accepted_actions = 77;
        state
    }

    #[test]
    fn unsupported_search_and_rollout_reject_before_cache_reward_or_horizon() {
        let mut state = unsupported_resume();
        let before = bincode::serialize(&state).unwrap();
        let config = MctsConfig { max_rollout_actions: 0, ..Default::default() };
        let mut root = MctsNode::new();
        assert_eq!(try_mcts_search(&state, &config, &mut root),
            Err(TerminationReason::UnsupportedContinuingElimination));
        assert_eq!(tree_walk(&mut state, &mut root, &config,
            &GreedyStrategy, &GoldfishStrategy, u32::MAX),
            Err(TerminationReason::UnsupportedContinuingElimination));
        assert_eq!(rollout(&mut state, &GreedyStrategy, &GoldfishStrategy, &config),
            Err(TerminationReason::UnsupportedContinuingElimination));
        assert_eq!(root.visits, 0);
        assert_eq!(root.total_reward, 0.0);
        assert!(root.children.is_none());
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn unsupported_mcts_runner_is_invalid_before_horizon_without_decisions() {
        let mut state = unsupported_resume();
        let before = bincode::serialize(&state).unwrap();
        let result = run_mcts_goldfish_game(&mut state, &MctsConfig::default(), false, None);
        assert_eq!(result.outcome, MctsOutcome::Invalid);
        assert!(!result.won);
        assert_eq!(result.actions_taken, 0);
        assert!(result.decision_stats.is_empty());
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn unsupported_child_does_not_backpropagate_a_reward() {
        let mut db = crate::game::CardDatabase::new();
        db.insert(crate::card::CardDef { id: 987_001, name: "Land".into(),
            card_types: vec![crate::card::CardType::Land], ..Default::default() });
        let mut state = GameState::new(3);
        state.card_db = Some(std::sync::Arc::new(db));
        state.phase = Phase::PreCombatMain;
        state.create_card_in_zone(987_001, 0, crate::card::ZoneType::Hand);
        assert!(legal_actions(&state).len() > 1);
        let mut root = MctsNode::new();
        expand_node(&mut root, &[Action::Concede]);
        assert_eq!(tree_walk(&mut state, &mut root, &MctsConfig::default(),
            &GreedyStrategy, &GoldfishStrategy, 0),
            Err(TerminationReason::UnsupportedContinuingElimination));
        let child = &root.children.as_ref().unwrap()[0].node;
        assert_eq!(child.visits, 0);
        assert_eq!(child.total_reward, 0.0);
    }

    #[test]
    fn legacy_terminal_search_exposes_no_action_or_cache_work() {
        let mut state = GameState::new(2);
        state.game_over = true;
        let mut root = MctsNode::new();
        assert_eq!(try_mcts_search(&state, &MctsConfig::default(), &mut root), Ok(None));
        assert!(root.children.is_none());
    }
}

#[cfg(test)]
mod returned_loss_provenance_tests {
    use super::*;
    use crate::game::{LossCause, LossCoordinates, LossFact};

    #[test]
    fn mcts_result_retains_exact_terminal_and_unsupported_provenance_in_roundtrips() {
        for players in [2, 3] {
            let mut state = GameState::new(players);
            state.turn_number = 11;
            state.active_player = 1;
            state.priority_player = 1;
            state.phase = Phase::PreCombatMain;
            state.loss_boundary.accepted_actions = 23;
            state.loss_boundary.turns_taken = vec![5; players];
            state.players[0].poison_counters = 10;
            rules::check_state_based_actions(&mut state);
            let expected = state.loss_boundary.clone();
            let facts = vec![LossFact { player: 0, causes: vec![LossCause::Poison] }];
            let coordinates = LossCoordinates { global_turn: 11, active_seat: 1,
                player_turns: vec![5; players], phase: Phase::PreCombatMain, action_index: 23 };
            if players == 2 {
                let record = expected.terminal.as_ref().expect("supported terminal loss");
                assert_eq!(record.losses, facts);
                assert_eq!(record.coordinates, coordinates);
            } else {
                let record = expected.unsupported.as_ref().expect("unsupported continuing loss");
                assert_eq!(record.losses, facts);
                assert_eq!(record.coordinates, coordinates);
            }
            let result = run_mcts_goldfish_game(&mut state, &MctsConfig::default(), false, None);
            assert_eq!(result.outcome, if players == 2 { MctsOutcome::Loss } else { MctsOutcome::Invalid });
            assert_eq!(result.loss_boundary, expected);
            let json_result: MctsGameResult = serde_json::from_str(
                &serde_json::to_string(&result).unwrap()).unwrap();
            let binary_result: MctsGameResult = bincode::deserialize(
                &bincode::serialize(&result).unwrap()).unwrap();
            for restored in [json_result, binary_result] {
                assert_eq!(restored.outcome, result.outcome);
                assert_eq!(restored.loss_boundary, expected);
            }
        }
    }

    #[test]
    fn missing_result_provenance_in_old_json_defaults_without_inventing_facts() {
        let mut state = GameState::new(2);
        state.game_over = true;
        let result = run_mcts_goldfish_game(&mut state, &MctsConfig::default(), false, None);
        let mut json = serde_json::to_value(&result).unwrap();
        json.as_object_mut().unwrap().remove("loss_boundary");
        let restored: MctsGameResult = serde_json::from_value(json).unwrap();
        assert_eq!(restored.outcome, MctsOutcome::Draw);
        assert_eq!(restored.loss_boundary, crate::game::LossBoundary::default());
    }
}

#[cfg(test)]
mod completed_invalid_action_tests {
    use super::*;
    use crate::card::{ActivatedAbility, CardDef, CardType, Effect, ZoneType};
    use crate::game::{CardDatabase, LossCause};
    use crate::mana::ManaCost;
    use std::sync::Arc;

    fn life_cost_state() -> (GameState, Action) {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 987_102, name: "Life-cost artifact".into(),
            card_types: vec![CardType::Artifact], activated_abilities: vec![ActivatedAbility {
                cost: ManaCost::zero(), requires_tap: false, sacrifice_cost: None,
                life_cost: 1, effect: Effect::GainLife { amount: 1 }, description: "Pay 1 life".into(),
            }], ..Default::default() });
        let mut state = GameState::new(3);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::PreCombatMain;
        state.players[0].life = 1;
        state.loss_boundary.accepted_actions = 41;
        let source = state.create_card_in_zone(987_102, 0, ZoneType::Battlefield);
        (state, Action::ActivateAbility { object_id: source, ability_index: 0, targets: vec![] })
    }

    #[test]
    fn mcts_adapter_counts_completed_invalid_action_without_importing_global_history() {
        let (mut state, action) = life_cost_state();
        let legal = legal_actions(&state);
        assert!(legal.contains(&action));
        let mut actions_taken = 0;
        let mut rejected = 0;
        assert_eq!(apply_mcts_game_action(&mut state, &action, &legal,
            &mut actions_taken, &mut rejected),
            Err(crate::simulation::GameOutcome::Invalid(
                TerminationReason::UnsupportedContinuingElimination)));
        assert_eq!(actions_taken, 1);
        assert_eq!(rejected, 0);
        assert_eq!(state.loss_boundary.accepted_actions, 42);
        let record = state.loss_boundary.unsupported.as_ref().unwrap();
        assert_eq!(record.coordinates.action_index, 42);
        assert_eq!(record.losses[0].causes, vec![LossCause::LifeTotal]);
        let before = bincode::serialize(&state).unwrap();
        assert!(apply_mcts_game_action(&mut state, &Action::PassPriority, &[],
            &mut actions_taken, &mut rejected).is_err());
        assert_eq!(actions_taken, 1);
        assert_eq!(rejected, 0);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn mcts_runner_counts_its_live_life_cost_stop_as_one_action() {
        let (mut state, action) = life_cost_state();
        // With no sampling, the existing visit tie-break chooses the final
        // legal action. This isolates live action application from hypothetical
        // search branches, which must fail without counting a real action.
        assert_eq!(legal_actions(&state).last(), Some(&action));
        let config = MctsConfig { iterations_per_move: 0, ..Default::default() };
        let result = run_mcts_goldfish_game(&mut state, &config, false, None);
        assert_eq!(result.outcome, MctsOutcome::Invalid);
        assert_eq!(result.actions_taken, 1);
        assert_eq!(result.loss_boundary.accepted_actions, 42);
        let record = result.loss_boundary.unsupported.unwrap();
        assert_eq!(record.coordinates.action_index, 42);
        assert_eq!(record.losses[0].causes, vec![LossCause::LifeTotal]);
    }
}
