use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::action::legal_actions;
use crate::card::CardId;
use crate::game::{CardDatabase, GameState, PlayerIndex};
use crate::rules;
use crate::strategy::Strategy;

/// Search horizon for a simulated game.
const MAX_TURNS: u32 = 200;

/// Action budget for a simulated game.
const MAX_ACTIONS: u32 = 50_000;

/// Why a run ended without a completed game result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    TurnLimit,
    ActionLimit,
    NoProgress,
    IncompleteCleanup,
    RejectedAction,
    StateEncoding,
    InvalidTerminalState,
    UnsupportedContinuingElimination,
}

impl TerminationReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::TurnLimit => "turn_limit",
            Self::ActionLimit => "action_limit",
            Self::NoProgress => "no_progress",
            Self::IncompleteCleanup => "incomplete_cleanup",
            Self::RejectedAction => "rejected_action",
            Self::StateEncoding => "state_encoding",
            Self::InvalidTerminalState => "invalid_terminal_state",
            Self::UnsupportedContinuingElimination => "unsupported_continuing_elimination",
        }
    }
}

/// Completed games and incomplete simulation attempts are distinct outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameOutcome {
    Win(PlayerIndex),
    Draw,
    Censored(TerminationReason),
    Stalled(TerminationReason),
    Invalid(TerminationReason),
}

impl std::fmt::Display for GameOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Win(0) => write!(f, "WIN"),
            Self::Win(_) => write!(f, "LOSS"),
            Self::Draw => write!(f, "DRAW"),
            Self::Censored(reason) => write!(f, "CENSORED reason={}", reason.code()),
            Self::Stalled(reason) => write!(f, "STALLED reason={}", reason.code()),
            Self::Invalid(reason) => write!(f, "INVALID reason={}", reason.code()),
        }
    }
}

pub(crate) fn classify_outcome(state: &GameState, max_turns: u32, max_actions: u32, actions: u32,
    interrupted: Option<GameOutcome>) -> GameOutcome {
    if state.unsupported_continuing_elimination() {
        return GameOutcome::Invalid(TerminationReason::UnsupportedContinuingElimination);
    }
    if state.winner.is_some_and(|winner| !state.game_over || winner >= state.players.len()) {
        return GameOutcome::Invalid(TerminationReason::InvalidTerminalState);
    }
    if state.game_over {
        return state.winner.map_or(GameOutcome::Draw, GameOutcome::Win);
    }
    if let Some(outcome) = interrupted { return outcome; }
    if state.turn_number > max_turns { return GameOutcome::Censored(TerminationReason::TurnLimit); }
    if actions >= max_actions { return GameOutcome::Censored(TerminationReason::ActionLimit); }
    GameOutcome::Stalled(TerminationReason::NoProgress)
}

/// Detect contradictory mandatory-cleanup ownership without advancing rules.
/// A pending copy choice owns the action while its resolution is suspended.
pub fn invalid_cleanup_state(state: &GameState) -> bool {
    if state.pending_copy_order.is_some() { return false; }
    let hand = state.players[state.active_player].hand.len();
    if state.cleanup_discard_in_progress {
        state.phase != crate::game::Phase::Cleanup
            || state.cleanup_needs_repeat
            || state.priority_player != state.active_player
            || hand <= 7
    } else {
        state.phase == crate::game::Phase::Cleanup
            && !state.cleanup_needs_repeat
            && hand > 7
            && state.priority_player != state.active_player
    }
}

pub(crate) fn no_progress_outcome(state: &GameState) -> GameOutcome {
    if state.gameplay_stopped() {
        classify_outcome(state, u32::MAX, u32::MAX, 0, None)
    } else if invalid_cleanup_state(state) {
        GameOutcome::Invalid(TerminationReason::IncompleteCleanup)
    } else {
        GameOutcome::Stalled(TerminationReason::NoProgress)
    }
}

pub(crate) const MAX_REJECTED_IN_ROW: u32 = 3;

/// Action legality alone does not prove application: several rules guards
/// reject a proposal without mutating the canonical state.
pub fn apply_counted_action(state: &mut GameState, action: &crate::action::Action,
    legal: &[crate::action::Action]) -> Result<bool, TerminationReason> {
    if state.unsupported_continuing_elimination() {
        return Err(TerminationReason::UnsupportedContinuingElimination);
    }
    if state.gameplay_stopped() || !legal.contains(action) { return Ok(false); }
    let accepted_before = state.loss_boundary.accepted_actions;
    // Top-level engine accounting already compares canonical mutation exactly.
    // Nested contexts intentionally do not increment that counter, so retain
    // the original comparison there (including its encoding-error behavior).
    let before = if state.loss_action_in_progress {
        Some(bincode::serialize(state).map_err(|_| TerminationReason::StateEncoding)?)
    } else { None };
    rules::apply_action(state, action);
    if state.unsupported_continuing_elimination() {
        return Err(TerminationReason::UnsupportedContinuingElimination);
    }
    if let Some(before) = before {
        let after = bincode::serialize(state).map_err(|_| TerminationReason::StateEncoding)?;
        Ok(before != after)
    } else {
        // Inequality also preserves the existing release overflow behavior.
        Ok(accepted_before != state.loss_boundary.accepted_actions)
    }
}

/// Preserve runner-local budgets when the completed action itself stops an
/// unsupported game. Restored global counts only contribute their new delta.
pub(crate) fn count_completed_unsupported_actions(
    state: &GameState,
    accepted_before: u64,
    reason: TerminationReason,
    actions_taken: &mut u32,
) {
    if reason == TerminationReason::UnsupportedContinuingElimination {
        let accepted = state.loss_boundary.accepted_actions.saturating_sub(accepted_before);
        *actions_taken = actions_taken.saturating_add(
            u32::try_from(accepted).unwrap_or(u32::MAX));
    }
}

/// Result of a single simulated game.
#[derive(Debug, Clone)]
pub struct GameResult {
    pub winner: Option<PlayerIndex>,
    pub outcome: GameOutcome,
    pub turns: u32,
    pub actions_taken: u32,
    /// Proposals that left canonical game state unchanged; excluded from the action budget.
    pub rejected_actions: u32,
    pub final_life: [i32; 2],
    /// Loss facts and coordinates retained after the runner drops its state.
    pub loss_boundary: crate::game::LossBoundary,
}

/// Aggregate results from many simulated games.
#[derive(Debug, Clone)]
pub struct SimulationResults {
    pub total_games: u64,
    pub player0_wins: u64,
    pub player1_wins: u64,
    pub draws: u64,
    pub censored: u64,
    pub stalled: u64,
    pub invalid: u64,
    pub avg_turns: f64,
    pub avg_actions: f64,
}

impl SimulationResults {
    pub fn completed_games(&self) -> u64 {
        self.player0_wins + self.player1_wins + self.draws
    }

    pub fn win_rate(&self, player: PlayerIndex) -> f64 {
        let wins = if player == 0 {
            self.player0_wins
        } else {
            self.player1_wins
        };
        let eligible = self.completed_games();
        if eligible == 0 { 0.0 } else { wins as f64 / eligible as f64 }
    }

    pub fn display(&self) {
        println!("=== Simulation Results ===");
        println!("Total games: {}", self.total_games);
        println!("Completed games: {}", self.completed_games());
        println!(
            "Player 0 wins: {} ({:.1}%)",
            self.player0_wins,
            self.win_rate(0) * 100.0
        );
        println!(
            "Player 1 wins: {} ({:.1}%)",
            self.player1_wins,
            self.win_rate(1) * 100.0
        );
        println!("Draws: {}", self.draws);
        println!("Censored: {}  Stalled: {}  Invalid: {}", self.censored, self.stalled, self.invalid);
        println!("Avg turns: {:.1}", self.avg_turns);
        println!("Avg actions: {:.1}", self.avg_actions);
    }
}

/// Run a single game to completion with the given strategies.
pub fn run_game(
    card_db: &CardDatabase,
    deck0: &[CardId],
    deck1: &[CardId],
    strategy0: &dyn Strategy,
    strategy1: &dyn Strategy,
) -> GameResult {
    let db = Arc::new(card_db.clone());
    run_game_inner(db, deck0, deck1, strategy0, strategy1, false)
}

/// Run a single game with optional verbose tracing.
pub fn run_game_verbose(
    card_db: &CardDatabase,
    deck0: &[CardId],
    deck1: &[CardId],
    strategy0: &dyn Strategy,
    strategy1: &dyn Strategy,
) -> GameResult {
    let db = Arc::new(card_db.clone());
    run_game_inner(db, deck0, deck1, strategy0, strategy1, true)
}

fn run_game_inner(
    card_db: Arc<CardDatabase>,
    deck0: &[CardId],
    deck1: &[CardId],
    strategy0: &dyn Strategy,
    strategy1: &dyn Strategy,
    verbose: bool,
) -> GameResult {
    let mut state = GameState::new(2);
    state.card_db = Some(card_db);
    rules::setup_game(&mut state, deck0, deck1);
    run_game_loop(&mut state, strategy0, strategy1, verbose)
}

/// Run a single game with a fixed random seed for deterministic replay.
///
/// Given the same seed, decks, and deterministic strategies, the game will
/// produce identical results every time. Useful for debugging and regression
/// testing.
pub fn run_game_seeded(
    card_db: &CardDatabase,
    deck0: &[CardId],
    deck1: &[CardId],
    strategy0: &dyn Strategy,
    strategy1: &dyn Strategy,
    seed: u64,
) -> GameResult {
    let db = Arc::new(card_db.clone());
    let mut state = GameState::new(2);
    state.card_db = Some(db);
    rules::setup_game_seeded(&mut state, deck0, deck1, seed);
    run_game_loop(&mut state, strategy0, strategy1, false)
}

/// Run a single Commander goldfish game with a fixed random seed.
pub fn run_commander_goldfish_game_seeded(
    card_db: &CardDatabase,
    deck: &[CardId],
    commander: CardId,
    strategy: &dyn Strategy,
    seed: u64,
) -> GameResult {
    let db = Arc::new(card_db.clone());
    let mut state = GameState::new_commander(2);
    state.card_db = Some(db);
    rules::setup_commander_game_seeded(&mut state, deck, deck, commander, commander, seed);
    run_goldfish_loop(&mut state, strategy, false)
}

/// Run a single Commander game to completion with the given strategies.
pub fn run_commander_game(
    card_db: &CardDatabase,
    deck0: &[CardId],
    deck1: &[CardId],
    commander0: CardId,
    commander1: CardId,
    strategy0: &dyn Strategy,
    strategy1: &dyn Strategy,
) -> GameResult {
    let db = Arc::new(card_db.clone());
    run_commander_game_inner(
        db, deck0, deck1, commander0, commander1, strategy0, strategy1, false,
    )
}

fn run_commander_game_inner(
    card_db: Arc<CardDatabase>,
    deck0: &[CardId],
    deck1: &[CardId],
    commander0: CardId,
    commander1: CardId,
    strategy0: &dyn Strategy,
    strategy1: &dyn Strategy,
    verbose: bool,
) -> GameResult {
    let mut state = GameState::new_commander(2);
    state.card_db = Some(card_db);
    rules::setup_commander_game(&mut state, deck0, deck1, commander0, commander1);
    run_game_loop(&mut state, strategy0, strategy1, verbose)
}

/// Shared game loop for both standard and commander formats.
fn run_game_loop(
    state: &mut GameState,
    strategy0: &dyn Strategy,
    strategy1: &dyn Strategy,
    verbose: bool,
) -> GameResult {
    let mut actions_taken: u32 = 0;
    let mut rejected_actions: u32 = 0;
    let mut rejected_in_row: u32 = 0;
    let mut interrupted = None;

    while !state.gameplay_stopped() && state.turn_number <= MAX_TURNS && actions_taken < MAX_ACTIONS {
        if invalid_cleanup_state(state) {
            interrupted = Some(GameOutcome::Invalid(TerminationReason::IncompleteCleanup));
            break;
        }
        let player = state.priority_player;
        let actions = legal_actions(state);

        if actions.is_empty()
            || (actions.len() == 1 && actions[0] == crate::action::Action::PassPriority)
        {
            if actions.is_empty() {
                interrupted = Some(no_progress_outcome(state));
                break;
            }
            let action = crate::action::Action::PassPriority;
            let accepted_before = state.loss_boundary.accepted_actions;
            match apply_counted_action(state, &action, &actions) {
                Ok(true) => { actions_taken += 1; rejected_in_row = 0; }
                Ok(false) => { rejected_actions += 1; rejected_in_row += 1; }
                Err(reason) => {
                    count_completed_unsupported_actions(state, accepted_before, reason, &mut actions_taken);
                    interrupted = Some(GameOutcome::Invalid(reason));
                    break;
                }
            }
            if rejected_in_row >= MAX_REJECTED_IN_ROW {
                interrupted = Some(GameOutcome::Stalled(TerminationReason::RejectedAction));
                break;
            }
            continue;
        }

        let strategy: &dyn Strategy = if player == 0 { strategy0 } else { strategy1 };
        let action = strategy.choose_action(state, player);

        if verbose && actions_taken < 200 {
            log_action(state, &action, player);
        }

        let accepted_before = state.loss_boundary.accepted_actions;
        let advanced = match apply_counted_action(state, &action, &actions) {
            Ok(true) => { actions_taken += 1; rejected_in_row = 0; true }
            Ok(false) => { rejected_actions += 1; rejected_in_row += 1; false }
            Err(reason) => {
                count_completed_unsupported_actions(state, accepted_before, reason, &mut actions_taken);
                interrupted = Some(GameOutcome::Invalid(reason));
                break;
            }
        };
        if rejected_in_row >= MAX_REJECTED_IN_ROW {
            interrupted = Some(GameOutcome::Stalled(TerminationReason::RejectedAction));
            break;
        }

        if advanced && !state.gameplay_stopped() && actions_taken.is_multiple_of(10) {
            rules::check_state_based_actions(state);
        }
    }

    GameResult {
        winner: state.winner,
        outcome: classify_outcome(state, MAX_TURNS, MAX_ACTIONS, actions_taken, interrupted),
        turns: state.turn_number,
        actions_taken,
        rejected_actions,
        final_life: [state.players[0].life, state.players[1].life],
        loss_boundary: state.loss_boundary.clone(),
    }
}

/// Run many Commander games in parallel and aggregate results.
pub fn simulate_commander(
    card_db: &CardDatabase,
    deck0: &[CardId],
    deck1: &[CardId],
    commander0: CardId,
    commander1: CardId,
    strategy0: &(dyn Strategy + Send + Sync),
    strategy1: &(dyn Strategy + Send + Sync),
    num_games: u64,
) -> SimulationResults {
    let db = Arc::new(card_db.clone());
    let p0_wins = AtomicU64::new(0);
    let p1_wins = AtomicU64::new(0);
    let draws = AtomicU64::new(0);
    let censored = AtomicU64::new(0);
    let stalled = AtomicU64::new(0);
    let invalid = AtomicU64::new(0);
    let total_turns = AtomicU64::new(0);
    let total_actions = AtomicU64::new(0);

    (0..num_games).into_par_iter().for_each(|_| {
        let result = run_commander_game_inner(
            Arc::clone(&db),
            deck0,
            deck1,
            commander0,
            commander1,
            strategy0,
            strategy1,
            false,
        );

        match result.outcome {
            GameOutcome::Win(0) => {
                p0_wins.fetch_add(1, Ordering::Relaxed);
            }
            GameOutcome::Win(_) => {
                p1_wins.fetch_add(1, Ordering::Relaxed);
            }
            GameOutcome::Draw => {
                draws.fetch_add(1, Ordering::Relaxed);
            }
            GameOutcome::Censored(_) => { censored.fetch_add(1, Ordering::Relaxed); return; }
            GameOutcome::Stalled(_) => { stalled.fetch_add(1, Ordering::Relaxed); return; }
            GameOutcome::Invalid(_) => { invalid.fetch_add(1, Ordering::Relaxed); return; }
        }
        total_turns.fetch_add(result.turns as u64, Ordering::Relaxed);
        total_actions.fetch_add(result.actions_taken as u64, Ordering::Relaxed);
    });

    let total = num_games;
    let completed = p0_wins.load(Ordering::Relaxed) + p1_wins.load(Ordering::Relaxed)
        + draws.load(Ordering::Relaxed);
    SimulationResults {
        total_games: total,
        player0_wins: p0_wins.load(Ordering::Relaxed),
        player1_wins: p1_wins.load(Ordering::Relaxed),
        draws: draws.load(Ordering::Relaxed),
        censored: censored.load(Ordering::Relaxed),
        stalled: stalled.load(Ordering::Relaxed),
        invalid: invalid.load(Ordering::Relaxed),
        avg_turns: total_turns.load(Ordering::Relaxed) as f64 / completed.max(1) as f64,
        avg_actions: total_actions.load(Ordering::Relaxed) as f64 / completed.max(1) as f64,
    }
}

/// Run many games in parallel and aggregate results.
pub fn simulate(
    card_db: &CardDatabase,
    deck0: &[CardId],
    deck1: &[CardId],
    strategy0: &(dyn Strategy + Send + Sync),
    strategy1: &(dyn Strategy + Send + Sync),
    num_games: u64,
) -> SimulationResults {
    let db = Arc::new(card_db.clone());
    let p0_wins = AtomicU64::new(0);
    let p1_wins = AtomicU64::new(0);
    let draws = AtomicU64::new(0);
    let censored = AtomicU64::new(0);
    let stalled = AtomicU64::new(0);
    let invalid = AtomicU64::new(0);
    let total_turns = AtomicU64::new(0);
    let total_actions = AtomicU64::new(0);

    (0..num_games).into_par_iter().for_each(|_| {
        let result = run_game_inner(Arc::clone(&db), deck0, deck1, strategy0, strategy1, false);

        match result.outcome {
            GameOutcome::Win(0) => {
                p0_wins.fetch_add(1, Ordering::Relaxed);
            }
            GameOutcome::Win(_) => {
                p1_wins.fetch_add(1, Ordering::Relaxed);
            }
            GameOutcome::Draw => {
                draws.fetch_add(1, Ordering::Relaxed);
            }
            GameOutcome::Censored(_) => { censored.fetch_add(1, Ordering::Relaxed); return; }
            GameOutcome::Stalled(_) => { stalled.fetch_add(1, Ordering::Relaxed); return; }
            GameOutcome::Invalid(_) => { invalid.fetch_add(1, Ordering::Relaxed); return; }
        }
        total_turns.fetch_add(result.turns as u64, Ordering::Relaxed);
        total_actions.fetch_add(result.actions_taken as u64, Ordering::Relaxed);
    });

    let total = num_games;
    let completed = p0_wins.load(Ordering::Relaxed) + p1_wins.load(Ordering::Relaxed)
        + draws.load(Ordering::Relaxed);
    SimulationResults {
        total_games: total,
        player0_wins: p0_wins.load(Ordering::Relaxed),
        player1_wins: p1_wins.load(Ordering::Relaxed),
        draws: draws.load(Ordering::Relaxed),
        censored: censored.load(Ordering::Relaxed),
        stalled: stalled.load(Ordering::Relaxed),
        invalid: invalid.load(Ordering::Relaxed),
        avg_turns: total_turns.load(Ordering::Relaxed) as f64 / completed.max(1) as f64,
        avg_actions: total_actions.load(Ordering::Relaxed) as f64 / completed.max(1) as f64,
    }
}

// ---------------------------------------------------------------------------
// Goldfish mode — solitaire simulation against a passive opponent
// ---------------------------------------------------------------------------

/// Goldfish search horizon (incomplete runs are censored).
const GOLDFISH_MAX_TURNS: u32 = 20;

/// Maximum actions per goldfish game (lower bound since opponent does nothing).
const GOLDFISH_MAX_ACTIONS: u32 = 10_000;

/// Aggregate results from goldfish simulation.
///
/// Tracks kill-turn distribution in addition to standard win/loss stats.
/// Since the opponent takes no actions, the key metric is how quickly
/// the deck can win — the "goldfish kill turn".
#[derive(Debug, Clone)]
pub struct GoldfishResults {
    pub total_games: u64,
    /// Games where the pilot (player 0) reduced the goldfish to 0 life.
    pub wins: u64,
    /// Games where the pilot (player 0) lost — e.g. self-inflicted life loss,
    /// decking out, or an effect that causes the pilot to lose. Should be rare
    /// against a passive opponent, but tracked for completeness.
    pub losses: u64,
    /// Completed games with no winner.
    pub draws: u64,
    pub censored: u64,
    pub stalled: u64,
    pub invalid: u64,
    pub avg_kill_turn: f64,
    pub fastest_kill: u32,
    pub slowest_kill: u32,
    pub avg_actions: f64,
    /// Kill-turn distribution: index = turn number, value = number of wins on that turn.
    /// Index 0 is unused (games start at turn 1).
    pub kill_turn_distribution: Vec<u64>,
}

impl GoldfishResults {
    pub fn completed_games(&self) -> u64 {
        self.wins + self.losses + self.draws
    }

    pub fn kill_share(&self, count: u64) -> f64 {
        if self.wins == 0 { 0.0 } else { count as f64 / self.wins as f64 }
    }

    pub fn win_rate(&self) -> f64 {
        let eligible = self.completed_games();
        if eligible == 0 { 0.0 } else { self.wins as f64 / eligible as f64 }
    }

    pub fn display(&self) {
        println!("=== Goldfish Results ===");
        println!("Total games: {}", self.total_games);
        println!("Completed games: {}", self.completed_games());
        println!("Wins: {} ({:.1}% of completed)", self.wins, self.win_rate() * 100.0);
        println!("Draws: {}", self.draws);
        println!("Censored: {}  Stalled: {}  Invalid: {}", self.censored, self.stalled, self.invalid);
        if self.wins > 0 {
            println!("Avg kill turn: {:.2}", self.avg_kill_turn);
            println!("Fastest kill: T{}", self.fastest_kill);
            println!("Slowest kill: T{}", self.slowest_kill);
            println!("Avg actions/completed game: {:.1}", self.avg_actions);
            println!("Kill turn distribution:");
            for (turn, &count) in self.kill_turn_distribution.iter().enumerate() {
                if count > 0 {
                    let pct = count as f64 / self.wins as f64 * 100.0;
                    println!("  T{}: {} ({:.1}%)", turn, count, pct);
                }
            }
        }
    }
}

/// Run a single goldfish game: player 0 plays the deck under test,
/// player 1 uses GoldfishStrategy (does nothing).
pub fn run_goldfish_game(
    card_db: &CardDatabase,
    deck: &[CardId],
    strategy: &dyn Strategy,
) -> GameResult {
    let db = Arc::new(card_db.clone());
    init_and_run_goldfish(db, strategy, |state| {
        rules::setup_game(state, deck, deck);
    })
}

/// Run a single goldfish game with verbose tracing.
pub fn run_goldfish_game_verbose(
    card_db: &CardDatabase,
    deck: &[CardId],
    strategy: &dyn Strategy,
) -> GameResult {
    let db = Arc::new(card_db.clone());
    let mut state = GameState::new(2);
    state.card_db = Some(db);
    rules::setup_game(&mut state, deck, deck);
    run_goldfish_loop(&mut state, strategy, true)
}

/// Run a single goldfish game in Commander format.
///
/// Player 0 uses the provided strategy; player 1 is a passive goldfish.
/// Commander-specific rules apply: 40 starting life, command zone,
/// commander tax, commander damage, and commander redirect.
pub fn run_commander_goldfish_game(
    card_db: &CardDatabase,
    deck: &[CardId],
    commander: CardId,
    strategy: &dyn Strategy,
) -> GameResult {
    let db = Arc::new(card_db.clone());
    init_and_run_goldfish(db, strategy, |state| {
        *state = GameState::new_commander(2);
        // card_db is set by the caller before this closure; re-set after replacing state
        rules::setup_commander_game(state, deck, deck, commander, commander);
    })
}

/// Run a single Commander goldfish game with verbose tracing.
pub fn run_commander_goldfish_game_verbose(
    card_db: &CardDatabase,
    deck: &[CardId],
    commander: CardId,
    strategy: &dyn Strategy,
) -> GameResult {
    let db = Arc::new(card_db.clone());
    let mut state = GameState::new_commander(2);
    state.card_db = Some(db);
    rules::setup_commander_game(&mut state, deck, deck, commander, commander);
    run_goldfish_loop(&mut state, strategy, true)
}

/// Initialize a game state and run the goldfish loop.
///
/// The `setup` closure receives a `&mut GameState` with `card_db` already set.
/// For Commander, it should replace the state with `GameState::new_commander`
/// and call the appropriate setup function.
fn init_and_run_goldfish(
    card_db: Arc<CardDatabase>,
    strategy: &dyn Strategy,
    setup: impl FnOnce(&mut GameState),
) -> GameResult {
    let mut state = GameState::new(2);
    state.card_db = Some(card_db.clone());
    setup(&mut state);
    // Ensure card_db is set after setup (Commander setup replaces the state)
    if state.card_db.is_none() {
        state.card_db = Some(card_db);
    }
    run_goldfish_loop(&mut state, strategy, false)
}

/// Shared goldfish game loop for both Standard and Commander formats.
///
/// Player 0 uses the provided strategy; player 1 is a passive goldfish
/// that always passes priority.
pub(crate) fn run_goldfish_loop(
    state: &mut GameState,
    strategy: &dyn Strategy,
    verbose: bool,
) -> GameResult {
    let mut actions_taken: u32 = 0;
    let mut rejected_actions: u32 = 0;
    let mut rejected_in_row: u32 = 0;
    let mut interrupted = None;

    while !state.gameplay_stopped()
        && state.turn_number <= GOLDFISH_MAX_TURNS
        && actions_taken < GOLDFISH_MAX_ACTIONS
    {
        if invalid_cleanup_state(state) {
            interrupted = Some(GameOutcome::Invalid(TerminationReason::IncompleteCleanup));
            break;
        }
        // Fast-forward the goldfish's entire turn without calling legal_actions
        if state.active_player != 0 {
            let before = match bincode::serialize(state) {
                Ok(bytes) => bytes,
                Err(_) => { interrupted = Some(GameOutcome::Invalid(TerminationReason::StateEncoding)); break; }
            };
            let accepted_before = state.loss_boundary.accepted_actions;
            let advanced = rules::fast_forward_goldfish_turn(state);
            if state.unsupported_continuing_elimination() {
                count_completed_unsupported_actions(state, accepted_before,
                    TerminationReason::UnsupportedContinuingElimination, &mut actions_taken);
                interrupted = Some(GameOutcome::Invalid(TerminationReason::UnsupportedContinuingElimination));
                break;
            }
            if advanced == 0 {
                interrupted = Some(no_progress_outcome(state));
                break;
            }
            match bincode::serialize(state) {
                Ok(after) if after != before => {}
                Ok(_) => { interrupted = Some(no_progress_outcome(state)); break; }
                Err(_) => { interrupted = Some(GameOutcome::Invalid(TerminationReason::StateEncoding)); break; }
            }
            actions_taken += advanced;
            continue;
        }

        let player = state.priority_player;
        let actions = legal_actions(state);

        if actions.is_empty()
            || (actions.len() == 1 && actions[0] == crate::action::Action::PassPriority)
        {
            if actions.is_empty() {
                interrupted = Some(no_progress_outcome(state));
                break;
            }
            let action = crate::action::Action::PassPriority;
            let accepted_before = state.loss_boundary.accepted_actions;
            match apply_counted_action(state, &action, &actions) {
                Ok(true) => { actions_taken += 1; rejected_in_row = 0; }
                Ok(false) => { rejected_actions += 1; rejected_in_row += 1; }
                Err(reason) => {
                    count_completed_unsupported_actions(state, accepted_before, reason, &mut actions_taken);
                    interrupted = Some(GameOutcome::Invalid(reason));
                    break;
                }
            }
            if rejected_in_row >= MAX_REJECTED_IN_ROW {
                interrupted = Some(GameOutcome::Stalled(TerminationReason::RejectedAction));
                break;
            }
            continue;
        }

        let action = strategy.choose_action(state, player);

        if verbose && actions_taken < 200 {
            log_action(state, &action, player);
        }

        let accepted_before = state.loss_boundary.accepted_actions;
        let advanced = match apply_counted_action(state, &action, &actions) {
            Ok(true) => { actions_taken += 1; rejected_in_row = 0; true }
            Ok(false) => { rejected_actions += 1; rejected_in_row += 1; false }
            Err(reason) => {
                count_completed_unsupported_actions(state, accepted_before, reason, &mut actions_taken);
                interrupted = Some(GameOutcome::Invalid(reason));
                break;
            }
        };
        if rejected_in_row >= MAX_REJECTED_IN_ROW {
            interrupted = Some(GameOutcome::Stalled(TerminationReason::RejectedAction));
            break;
        }

        if advanced && !state.gameplay_stopped() && actions_taken.is_multiple_of(10) {
            rules::check_state_based_actions(state);
        }
    }

    GameResult {
        winner: state.winner,
        outcome: classify_outcome(state, GOLDFISH_MAX_TURNS, GOLDFISH_MAX_ACTIONS, actions_taken, interrupted),
        turns: state.turn_number,
        actions_taken,
        rejected_actions,
        final_life: [state.players[0].life, state.players[1].life],
        loss_boundary: state.loss_boundary.clone(),
    }
}

/// Run many goldfish games in parallel and aggregate results with kill-turn distribution.
///
/// Goldfish simulation measures the fastest possible win speed for a deck by
/// playing against an opponent who takes no actions (no blocking, no spells).
/// This produces lower branching complexity and faster convergence than
/// a full two-player simulation.
pub fn simulate_goldfish(
    card_db: &CardDatabase,
    deck: &[CardId],
    strategy: &(dyn Strategy + Send + Sync),
    num_games: u64,
) -> GoldfishResults {
    let db = Arc::new(card_db.clone());
    aggregate_goldfish_results(num_games, |_| {
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::clone(&db));
        rules::setup_game(&mut state, deck, deck);
        run_goldfish_loop(&mut state, strategy, false)
    })
}

/// Run many commander goldfish games in parallel and aggregate results.
pub fn simulate_commander_goldfish(
    card_db: &CardDatabase,
    deck: &[CardId],
    commander: CardId,
    strategy: &(dyn Strategy + Send + Sync),
    num_games: u64,
) -> GoldfishResults {
    let db = Arc::new(card_db.clone());
    aggregate_goldfish_results(num_games, |_| {
        let mut state = GameState::new_commander(2);
        state.card_db = Some(Arc::clone(&db));
        rules::setup_commander_game(&mut state, deck, deck, commander, commander);
        run_goldfish_loop(&mut state, strategy, false)
    })
}

/// Shared aggregation logic for goldfish simulations.
fn aggregate_goldfish_results(
    num_games: u64,
    run_one: impl Fn(u64) -> GameResult + Send + Sync,
) -> GoldfishResults {
    let wins = AtomicU64::new(0);
    let losses = AtomicU64::new(0);
    let draws = AtomicU64::new(0);
    let censored = AtomicU64::new(0);
    let stalled = AtomicU64::new(0);
    let invalid = AtomicU64::new(0);
    let total_kill_turns = AtomicU64::new(0);
    let total_actions = AtomicU64::new(0);
    let fastest = AtomicU64::new(u64::MAX);
    let slowest = AtomicU64::new(0);

    let distribution: Vec<AtomicU64> = (0..=GOLDFISH_MAX_TURNS)
        .map(|_| AtomicU64::new(0))
        .collect();

    (0..num_games).into_par_iter().for_each(|i| {
        let result = run_one(i);

        match result.outcome {
            GameOutcome::Win(0) => {
                wins.fetch_add(1, Ordering::Relaxed);
                let turn = result.turns;
                total_kill_turns.fetch_add(turn as u64, Ordering::Relaxed);
                if (turn as usize) < distribution.len() {
                    distribution[turn as usize].fetch_add(1, Ordering::Relaxed);
                }
                fastest.fetch_min(turn as u64, Ordering::Relaxed);
                slowest.fetch_max(turn as u64, Ordering::Relaxed);
            }
            GameOutcome::Win(_) => {
                losses.fetch_add(1, Ordering::Relaxed);
            }
            GameOutcome::Draw => {
                draws.fetch_add(1, Ordering::Relaxed);
            }
            GameOutcome::Censored(_) => { censored.fetch_add(1, Ordering::Relaxed); return; }
            GameOutcome::Stalled(_) => { stalled.fetch_add(1, Ordering::Relaxed); return; }
            GameOutcome::Invalid(_) => { invalid.fetch_add(1, Ordering::Relaxed); return; }
        }
        total_actions.fetch_add(result.actions_taken as u64, Ordering::Relaxed);
    });

    let total_wins = wins.load(Ordering::Relaxed);
    let fast = fastest.load(Ordering::Relaxed);
    let slow = slowest.load(Ordering::Relaxed);

    let kill_turn_dist: Vec<u64> = distribution
        .iter()
        .map(|a| a.load(Ordering::Relaxed))
        .collect();

    GoldfishResults {
        total_games: num_games,
        wins: total_wins,
        losses: losses.load(Ordering::Relaxed),
        draws: draws.load(Ordering::Relaxed),
        censored: censored.load(Ordering::Relaxed),
        stalled: stalled.load(Ordering::Relaxed),
        invalid: invalid.load(Ordering::Relaxed),
        avg_kill_turn: if total_wins > 0 {
            total_kill_turns.load(Ordering::Relaxed) as f64 / total_wins as f64
        } else {
            0.0
        },
        fastest_kill: if total_wins > 0 { fast as u32 } else { 0 },
        slowest_kill: if total_wins > 0 { slow as u32 } else { 0 },
        avg_actions: total_actions.load(Ordering::Relaxed) as f64 / (total_wins + losses.load(Ordering::Relaxed) + draws.load(Ordering::Relaxed)).max(1) as f64,
        kill_turn_distribution: kill_turn_dist,
    }
}

// ---------------------------------------------------------------------------
// MCTS goldfish — solitaire optimization using Monte Carlo Tree Search
// ---------------------------------------------------------------------------

use crate::solver::mcts::{self, MctsConfig, MctsGoldfishResults};

/// Run a single goldfish game using MCTS for player 0's decisions.
///
/// Unlike `run_goldfish_game()` which uses a fixed strategy, this runs
/// MCTS search at each decision point to find near-optimal play for
/// the given shuffle.
pub fn run_mcts_goldfish_game(
    card_db: &CardDatabase,
    deck: &[CardId],
    config: &MctsConfig,
    verbose: bool,
) -> mcts::MctsGameResult {
    let db = Arc::new(card_db.clone());
    let mut state = GameState::new(2);
    state.card_db = Some(db);
    rules::setup_game(&mut state, deck, deck);
    mcts::run_mcts_goldfish_game(&mut state, config, verbose, None)
}

/// Run a single Commander goldfish game using MCTS.
pub fn run_mcts_commander_goldfish_game(
    card_db: &CardDatabase,
    deck: &[CardId],
    commander: CardId,
    config: &MctsConfig,
    verbose: bool,
    tutor_targets: &[CardId],
) -> mcts::MctsGameResult {
    let db = Arc::new(card_db.clone());
    let mut state = GameState::new_commander(2);
    state.card_db = Some(db);
    rules::setup_commander_game(&mut state, deck, deck, commander, commander);
    rules::set_tutor_targets(&mut state, 0, tutor_targets);
    // Discover combos using only the commander + tutor targets (known combo
    // pieces). Using the full 99-card deck finds hundreds of spurious combos
    // and makes legal_actions() extremely slow.
    let mut combo_cards = vec![commander];
    combo_cards.extend_from_slice(tutor_targets);
    let (mut registry, _) = crate::combo_discovery::discover_and_register(
        card_db,
        &combo_cards,
        &crate::combo_discovery::DiscoveryConfig::default(),
    );
    crate::combo::register_ballista_win_combo(&mut registry);
    state.combo_registry = Some(Arc::new(registry));
    mcts::run_mcts_goldfish_game(&mut state, config, verbose, None)
}

/// Run many MCTS goldfish games in parallel and aggregate results.
///
/// Each game gets a fresh shuffle and runs MCTS at every decision point
/// for player 0. This measures how well MCTS-optimized play performs
/// across many random draws.
pub fn simulate_mcts_goldfish(
    card_db: &CardDatabase,
    deck: &[CardId],
    config: &MctsConfig,
    num_games: u64,
) -> MctsGoldfishResults {
    let db = Arc::new(card_db.clone());
    aggregate_mcts_goldfish_results(num_games, config, |_| {
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::clone(&db));
        rules::setup_game(&mut state, deck, deck);
        state
    }, None, None)
}

/// Like [`simulate_mcts_goldfish`], but increments `progress` after each game
/// completes so a background thread can report progress.
pub fn simulate_mcts_goldfish_with_progress(
    card_db: &CardDatabase,
    deck: &[CardId],
    config: &MctsConfig,
    num_games: u64,
    progress: &AtomicU64,
    decisions: &AtomicU64,
) -> MctsGoldfishResults {
    let db = Arc::new(card_db.clone());
    aggregate_mcts_goldfish_results(num_games, config, |_| {
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::clone(&db));
        rules::setup_game(&mut state, deck, deck);
        state
    }, Some(progress), Some(decisions))
}

/// Run many Commander MCTS goldfish games in parallel and aggregate results.
pub fn simulate_mcts_commander_goldfish(
    card_db: &CardDatabase,
    deck: &[CardId],
    commander: CardId,
    config: &MctsConfig,
    num_games: u64,
    tutor_targets: &[CardId],
) -> MctsGoldfishResults {
    let db = Arc::new(card_db.clone());
    let targets = tutor_targets.to_vec();
    // Discover combos using commander + tutor targets only
    let mut combo_cards = vec![commander];
    combo_cards.extend_from_slice(tutor_targets);
    let (mut registry, _) = crate::combo_discovery::discover_and_register(
        card_db,
        &combo_cards,
        &crate::combo_discovery::DiscoveryConfig::default(),
    );
    crate::combo::register_ballista_win_combo(&mut registry);
    let combo_reg = Arc::new(registry);
    aggregate_mcts_goldfish_results(num_games, config, |_| {
        let mut state = GameState::new_commander(2);
        state.card_db = Some(Arc::clone(&db));
        rules::setup_commander_game(&mut state, deck, deck, commander, commander);
        rules::set_tutor_targets(&mut state, 0, &targets);
        state.combo_registry = Some(Arc::clone(&combo_reg));
        state
    }, None, None)
}

/// Like [`simulate_mcts_commander_goldfish`], but increments `progress` after
/// each game completes so a background thread can report progress.
pub fn simulate_mcts_commander_goldfish_with_progress(
    card_db: &CardDatabase,
    deck: &[CardId],
    commander: CardId,
    config: &MctsConfig,
    num_games: u64,
    progress: &AtomicU64,
    decisions: &AtomicU64,
    tutor_targets: &[CardId],
) -> MctsGoldfishResults {
    let db = Arc::new(card_db.clone());
    let targets = tutor_targets.to_vec();
    // Discover combos using commander + tutor targets only
    let mut combo_cards = vec![commander];
    combo_cards.extend_from_slice(tutor_targets);
    let (mut registry, _) = crate::combo_discovery::discover_and_register(
        card_db,
        &combo_cards,
        &crate::combo_discovery::DiscoveryConfig::default(),
    );
    crate::combo::register_ballista_win_combo(&mut registry);
    let combo_reg = Arc::new(registry);
    aggregate_mcts_goldfish_results(num_games, config, |_| {
        let mut state = GameState::new_commander(2);
        state.card_db = Some(Arc::clone(&db));
        rules::setup_commander_game(&mut state, deck, deck, commander, commander);
        rules::set_tutor_targets(&mut state, 0, &targets);
        state.combo_registry = Some(Arc::clone(&combo_reg));
        state
    }, Some(progress), Some(decisions))
}

/// Shared aggregation logic for MCTS goldfish simulations.
///
/// If `progress` is provided, it is incremented (atomically) after each game
/// completes, allowing a background thread to report progress.
fn aggregate_mcts_goldfish_results(
    num_games: u64,
    config: &MctsConfig,
    make_state: impl Fn(u64) -> GameState + Send + Sync,
    progress: Option<&AtomicU64>,
    decisions: Option<&AtomicU64>,
) -> MctsGoldfishResults {
    use std::sync::Mutex;

    let wins = AtomicU64::new(0);
    let losses = AtomicU64::new(0);
    let draws = AtomicU64::new(0);
    let censored = AtomicU64::new(0);
    let stalled = AtomicU64::new(0);
    let invalid = AtomicU64::new(0);
    let legacy_unknown = AtomicU64::new(0);
    let completed = AtomicU64::new(0);
    let total_kill_turns = AtomicU64::new(0);
    let total_actions = AtomicU64::new(0);
    let fastest = AtomicU64::new(u64::MAX);
    let slowest = AtomicU64::new(0);
    let total_decisions = AtomicU64::new(0);
    let reward_samples = AtomicU64::new(0);
    // Use Mutex<f64> for exact floating-point accumulation (no ×1000 truncation).
    let total_reward = Mutex::new(0.0f64);
    // Track the decision sequence from the fastest winning game.
    let fastest_sequence: Mutex<Vec<mcts::DecisionStat>> = Mutex::new(Vec::new());

    let max_turn = 20u32;
    let distribution: Vec<AtomicU64> = (0..=max_turn)
        .map(|_| AtomicU64::new(0))
        .collect();

    (0..num_games).into_par_iter().for_each(|i| {
        let mut state = make_state(i);
        let result = mcts::run_mcts_goldfish_game(&mut state, config, false, decisions);

        if matches!(result.outcome, mcts::MctsOutcome::Win | mcts::MctsOutcome::Loss | mcts::MctsOutcome::Draw) {
            completed.fetch_add(1, Ordering::Relaxed);
            total_actions.fetch_add(result.actions_taken as u64, Ordering::Relaxed);
            total_decisions.fetch_add(result.decision_stats.len() as u64, Ordering::Relaxed);
            if !result.decision_stats.is_empty() {
                let avg_reward: f64 = {
                result.decision_stats.iter().map(|d| d.best_action_avg_reward).sum::<f64>()
                    / result.decision_stats.len() as f64
                };
                *total_reward.lock().unwrap() += avg_reward;
                reward_samples.fetch_add(1, Ordering::Relaxed);
            }
        }

        if result.outcome == mcts::MctsOutcome::Win {
            wins.fetch_add(1, Ordering::Relaxed);
            let turn = result.kill_turn;
            total_kill_turns.fetch_add(turn as u64, Ordering::Relaxed);
            if (turn as usize) < distribution.len() {
                distribution[turn as usize].fetch_add(1, Ordering::Relaxed);
            }
            let prev_fastest = fastest.fetch_min(turn as u64, Ordering::Relaxed);
            slowest.fetch_max(turn as u64, Ordering::Relaxed);
            // If this game is the new fastest (or tied), save its decision sequence
            if (turn as u64) <= prev_fastest {
                *fastest_sequence.lock().unwrap() = result.decision_stats;
            }
        } else {
            match result.outcome {
                mcts::MctsOutcome::Loss => { losses.fetch_add(1, Ordering::Relaxed); }
                mcts::MctsOutcome::Draw => { draws.fetch_add(1, Ordering::Relaxed); }
                mcts::MctsOutcome::Censored => { censored.fetch_add(1, Ordering::Relaxed); }
                mcts::MctsOutcome::Stalled => { stalled.fetch_add(1, Ordering::Relaxed); }
                mcts::MctsOutcome::Invalid => { invalid.fetch_add(1, Ordering::Relaxed); }
                mcts::MctsOutcome::LegacyUnknown => { legacy_unknown.fetch_add(1, Ordering::Relaxed); }
                mcts::MctsOutcome::Win => unreachable!(),
            }
        }

        if let Some(p) = progress {
            p.fetch_add(1, Ordering::Relaxed);
        }
    });

    let total_wins = wins.load(Ordering::Relaxed);
    let fast = fastest.load(Ordering::Relaxed);
    let slow = slowest.load(Ordering::Relaxed);
    let tot_decisions = total_decisions.load(Ordering::Relaxed);
    let tot_reward = *total_reward.lock().unwrap();
    let completed_games = completed.load(Ordering::Relaxed);
    let measured_rewards = reward_samples.load(Ordering::Relaxed);

    let kill_turn_dist: Vec<u64> = distribution
        .iter()
        .map(|a| a.load(Ordering::Relaxed))
        .collect();

    MctsGoldfishResults {
        schema_version: 3,
        total_games: num_games,
        wins: total_wins,
        losses: losses.load(Ordering::Relaxed),
        draws: draws.load(Ordering::Relaxed),
        censored: censored.load(Ordering::Relaxed),
        stalled: stalled.load(Ordering::Relaxed),
        invalid: invalid.load(Ordering::Relaxed),
        legacy_unknown: legacy_unknown.load(Ordering::Relaxed),
        avg_kill_turn: (total_wins > 0).then(||
            total_kill_turns.load(Ordering::Relaxed) as f64 / total_wins as f64),
        kill_turn_samples: total_wins,
        fastest_kill: if total_wins > 0 { fast as u32 } else { 0 },
        slowest_kill: if total_wins > 0 { slow as u32 } else { 0 },
        avg_actions: (completed_games > 0).then(||
            total_actions.load(Ordering::Relaxed) as f64 / completed_games as f64),
        actions_samples: completed_games,
        avg_decisions_per_game: (completed_games > 0).then(||
            tot_decisions as f64 / completed_games as f64),
        decision_samples: completed_games,
        avg_best_reward: (measured_rewards > 0).then(||
            tot_reward / measured_rewards as f64),
        reward_samples: measured_rewards,
        kill_turn_distribution: kill_turn_dist,
        fastest_sequence: fastest_sequence.into_inner().unwrap(),
    }
}

#[cfg(test)]
mod mcts_outcome_tests {
    use super::*;

    #[test]
    fn parallel_aggregation_counts_only_rules_completions_in_denominators() {
        let results = aggregate_mcts_goldfish_results(4, &MctsConfig::default(), |index| {
            let mut state = GameState::new(2);
            match index {
                0 => { state.game_over = true; state.winner = Some(0); }
                1 => { state.game_over = true; state.winner = Some(1); }
                2 => { state.game_over = true; state.winner = None; }
                _ => { state.turn_number = 21; }
            }
            state
        }, None, None);
        assert_eq!(results.total_games, 4);
        assert_eq!(results.completed_games(), 3);
        assert_eq!((results.wins, results.losses, results.draws, results.censored), (1, 1, 1, 1));
        assert!((results.win_rate() - 1.0 / 3.0).abs() < 1e-10);
        assert_eq!(results.avg_actions, Some(0.0));
    }
}

/// Format a game action as a human-readable string, resolving card names
/// from the game state's card database.
///
/// Shared by `log_action` (simulation verbose tracing) and MCTS decision
/// logging so that card-name resolution isn't duplicated.
pub fn format_action_name(state: &GameState, action: &crate::action::Action) -> String {
    let db = state.card_db();
    match action {
        crate::action::Action::CastSpell { object_id, .. }
        | crate::action::Action::CastCommander { object_id, .. } => {
            let inst = &state.objects[object_id];
            format!(
                "Cast {}",
                db.get(inst.card_def_id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("?")
            )
        }
        crate::action::Action::PlayLand { object_id } => {
            let inst = &state.objects[object_id];
            format!(
                "Play {}",
                db.get(inst.card_def_id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("?")
            )
        }
        crate::action::Action::DeclareAttackers { attackers } => {
            let names: Vec<String> = attackers
                .iter()
                .filter_map(|id| {
                    state.objects.get(id).and_then(|inst| {
                        db.get(inst.card_def_id).map(|d| d.name.clone())
                    })
                })
                .collect();
            if names.is_empty() {
                "Attack: none".to_string()
            } else {
                format!("Attack: {}", names.join(", "))
            }
        }
        crate::action::Action::ActivateAbility { object_id, ability_index, .. } => {
            let inst = &state.objects[object_id];
            format!(
                "Activate {} ability #{}",
                db.get(inst.card_def_id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("?"),
                ability_index,
            )
        }
        crate::action::Action::ActivateMacro { combo_id } => {
            format!("Activate combo #{}", combo_id)
        }
        crate::action::Action::OrderTriggers { ordering } => {
            format!("Order {} triggers", ordering.len())
        }
        crate::action::Action::ChooseTutorTarget { card_id } => {
            format!(
                "Tutor for {}",
                db.get(*card_id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("?")
            )
        }
        crate::action::Action::PassPriority => "Pass".to_string(),
        other => format!("{}", other),
    }
}

/// Return the card names in a player's hand.
pub fn format_hand(state: &GameState, player: PlayerIndex) -> Vec<String> {
    let db = state.card_db();
    state.players[player].hand.iter().filter_map(|oid| {
        state.objects.get(oid).and_then(|inst| {
            db.get(inst.card_def_id).map(|d| d.name.clone())
        })
    }).collect()
}

/// Log a game action to stderr for verbose tracing.
fn log_action(state: &GameState, action: &crate::action::Action, player: PlayerIndex) {
    let action_name = format_action_name(state, action);
    eprintln!(
        "T{} {:?} P{}: {} (life: {}/{})",
        state.turn_number,
        state.phase,
        player,
        action_name,
        state.players[0].life,
        state.players[1].life,
    );
}

#[cfg(test)]
mod cleanup_continuation_tests {
    use super::*;
    use crate::action::Action;
    use crate::card::{CardDef, CardType, Effect, TriggerCondition, TriggeredAbility, ZoneType};
    use crate::game::Phase;
    use crate::strategy::GoldfishStrategy;
    use std::sync::mpsc;
    use std::time::Duration;


    struct AlwaysPass;
    impl Strategy for AlwaysPass {
        fn choose_action(&self, _: &GameState, _: PlayerIndex) -> Action { Action::PassPriority }
        fn name(&self) -> &str { "always-pass" }
    }

    struct RejectOnce(std::sync::atomic::AtomicUsize);
    impl Strategy for RejectOnce {
        fn choose_action(&self, state: &GameState, _: PlayerIndex) -> Action {
            if self.0.fetch_add(1, Ordering::Relaxed) == 0 { Action::PassPriority }
            else { legal_actions(state)[0].clone() }
        }
        fn name(&self) -> &str { "reject-once" }
    }

    fn mandatory_cleanup() -> GameState {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 90_123, name: "Filler".into(),
            card_types: vec![CardType::Land], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::Cleanup;
        state.turn_number = GOLDFISH_MAX_TURNS;
        state.cleanup_discard_in_progress = true;
        for _ in 0..8 { state.create_card_in_zone(90_123, 0, ZoneType::Hand); }
        state
    }

    #[test]
    fn original_json_cleanup_chooser_mismatch_is_invalid() {
        let mut state = mandatory_cleanup();
        state.active_player = 1;
        state.priority_player = 0;
        for _ in 0..8 { state.create_card_in_zone(90_123, 1, ZoneType::Hand); }
        let json = serde_json::to_string(&state).unwrap();
        let mut restored: GameState = serde_json::from_str(&json).unwrap();
        restored.card_db = state.card_db.clone();
        let result = run_goldfish_loop(&mut restored, &GoldfishStrategy, false);
        assert_eq!(result.outcome, GameOutcome::Invalid(TerminationReason::IncompleteCleanup));
        assert_eq!(result.actions_taken, 0);
    }

    #[test]
    fn repeated_rejected_action_stalls_without_spending_accepted_budget() {
        let mut state = mandatory_cleanup();
        let result = run_goldfish_loop(&mut state, &AlwaysPass, false);
        assert_eq!(result.outcome, GameOutcome::Stalled(TerminationReason::RejectedAction));
        assert_eq!(result.actions_taken, 0);
        assert!(result.rejected_actions > 0);
    }

    #[test]
    fn one_rejected_proposal_can_recover_with_legal_discard() {
        let mut state = mandatory_cleanup();
        let strategy = RejectOnce(std::sync::atomic::AtomicUsize::new(0));
        let result = run_goldfish_loop(&mut state, &strategy, false);
        assert_eq!(result.rejected_actions, 1);
        assert!(result.actions_taken > 0);
        assert_eq!(state.players[0].hand.len(), 7);
        assert_eq!(result.outcome, GameOutcome::Censored(TerminationReason::TurnLimit));
    }

    #[test]
    fn mixed_attempts_use_completed_game_denominator() {
        let make = |outcome| GameResult { winner: None, outcome, turns: 2,
            actions_taken: 4, rejected_actions: 0, final_life: [20, 20],
            loss_boundary: Default::default(), };
        let sample = [make(GameOutcome::Win(0)),
            make(GameOutcome::Stalled(TerminationReason::NoProgress)),
            make(GameOutcome::Invalid(TerminationReason::IncompleteCleanup))];
        let results = aggregate_goldfish_results(3, |i| sample[i as usize].clone());
        assert_eq!((results.total_games, results.completed_games(), results.wins,
            results.stalled, results.invalid), (3, 1, 1, 1, 1));
        assert_eq!(results.win_rate(), 1.0);
        assert_eq!(results.kill_share(1), 1.0);
        assert_eq!(results.avg_actions, 4.0);
    }

    #[test]
    fn incomplete_cleanup_is_invalid_and_not_aggregated_as_draw() {
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(CardDatabase::new()));
        state.active_player = 1;
        state.priority_player = 1;
        state.phase = Phase::PreCombatMain;
        state.cleanup_discard_in_progress = true;
        // A discard continuation outside cleanup is inconsistent.
        let result = run_goldfish_loop(&mut state, &GoldfishStrategy, false);
        assert!(matches!(result.outcome, GameOutcome::Invalid(TerminationReason::IncompleteCleanup)));
        let results = aggregate_goldfish_results(1, |_| result.clone());
        assert_eq!((results.draws, results.invalid, results.total_games), (0, 1, 1));
    }

    #[test]
    fn turn_limit_is_censored_and_terminal_loss_is_a_loss() {
        let mut state = GameState::new(2);
        state.turn_number = GOLDFISH_MAX_TURNS + 1;
        let capped = run_goldfish_loop(&mut state, &GoldfishStrategy, false);
        assert!(matches!(capped.outcome, GameOutcome::Censored(TerminationReason::TurnLimit)));
        state.game_over = true;
        state.winner = Some(1);
        let loss = run_goldfish_loop(&mut state, &GoldfishStrategy, false);
        assert!(matches!(loss.outcome, GameOutcome::Win(1)));
        let aggregate = aggregate_goldfish_results(2, |i| if i == 0 { capped.clone() } else { loss.clone() });
        assert_eq!((aggregate.losses, aggregate.draws, aggregate.censored), (1, 0, 1));
        assert_eq!(aggregate.completed_games(), 1);
        assert_eq!(aggregate.avg_actions, 0.0);
    }

    #[test]
    fn interrupted_goldfish_cleanup_makes_progress_in_simulation_loop() {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 90_121, name: "Observer".into(),
            card_types: vec![CardType::Enchantment],
            triggered_abilities: vec![TriggeredAbility {
                trigger: TriggerCondition::YouDiscardACard,
                effect: Effect::GainLife { amount: 1 }, description: "Discard".into(),
            }], ..Default::default() });
        db.insert(CardDef { id: 90_122, name: "Filler".into(),
            card_types: vec![CardType::Land], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.active_player = 1;
        state.priority_player = 1;
        state.turn_number = GOLDFISH_MAX_TURNS;
        state.phase = Phase::Cleanup;
        state.create_card_in_zone(90_121, 1, ZoneType::Battlefield);
        for _ in 0..9 { state.create_card_in_zone(90_122, 1, ZoneType::Hand); }
        let first = state.players[1].hand[0];
        rules::apply_action(&mut state, &Action::Discard { object_id: first });
        assert!(state.cleanup_discard_in_progress);

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = run_goldfish_loop(&mut state, &GoldfishStrategy, false);
            tx.send((result.actions_taken, state.active_player, state.turn_number,
                state.players[1].hand.len(), state.players[1].life)).unwrap();
        });
        let outcome = rx.recv_timeout(Duration::from_secs(3))
            .expect("simulation must not spin on an interrupted cleanup discard");
        assert!(outcome.0 > 0);
        assert_eq!((outcome.1, outcome.2, outcome.3, outcome.4),
            (0, GOLDFISH_MAX_TURNS + 1, 7, 22));
    }
}

#[cfg(test)]
mod loss_boundary_consumer_tests {
    use super::*;
    use crate::action::Action;

    fn unsupported_resume() -> GameState {
        let mut state = GameState::new(3);
        state.players[0].has_lost = true;
        state.turn_number = u32::MAX;
        state.loss_boundary.accepted_actions = 77;
        state
    }

    #[test]
    fn unsupported_classification_precedes_horizons_and_interruptions() {
        let state = unsupported_resume();
        let invalid = GameOutcome::Invalid(TerminationReason::UnsupportedContinuingElimination);
        assert_eq!(classify_outcome(&state, 0, 0, 0,
            Some(GameOutcome::Stalled(TerminationReason::NoProgress))), invalid);
        assert_eq!(no_progress_outcome(&state), invalid);
        assert_eq!(invalid.to_string(), "INVALID reason=unsupported_continuing_elimination");
    }

    #[test]
    fn persisted_unsupported_failure_precedes_legacy_terminal_flags_after_restore() {
        let mut state = GameState::new(3);
        state.loss_boundary.unsupported = Some(crate::game::UnsupportedElimination {
            losses: vec![crate::game::LossFact { player: 0,
                causes: vec![crate::game::LossCause::Concession] }],
            coordinates: crate::game::LossCoordinates { global_turn: 1, active_seat: 0,
                player_turns: vec![1, 0, 0], phase: state.phase, action_index: 0 },
        });
        state.game_over = true;
        state.winner = Some(0);
        let restored: GameState = serde_json::from_str(
            &serde_json::to_string(&state).unwrap()).unwrap();
        let invalid = GameOutcome::Invalid(TerminationReason::UnsupportedContinuingElimination);
        assert_eq!(classify_outcome(&restored, 0, 0, 0, None), invalid);
        assert_eq!(no_progress_outcome(&restored), invalid);
        let mut runner_state = restored.clone();
        assert_eq!(crate::solver::mcts::run_mcts_goldfish_game(&mut runner_state,
            &crate::solver::mcts::MctsConfig::default(), false, None).outcome,
            crate::solver::mcts::MctsOutcome::Invalid);
    }

    #[test]
    fn unsupported_counted_action_rejects_before_legality_or_mutation() {
        let mut state = unsupported_resume();
        let before = bincode::serialize(&state).unwrap();
        assert_eq!(apply_counted_action(&mut state, &Action::PassPriority, &[]),
            Err(TerminationReason::UnsupportedContinuingElimination));
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn resumed_unsupported_simulation_never_calls_a_strategy() {
        struct UnreachableStrategy;
        impl Strategy for UnreachableStrategy {
            fn choose_action(&self, _: &GameState, _: PlayerIndex) -> Action {
                panic!("stopped simulation requested an action")
            }
            fn name(&self) -> &str { "unreachable" }
        }
        for goldfish in [false, true] {
            let mut state = unsupported_resume();
            let before = bincode::serialize(&state).unwrap();
            let result = if goldfish {
                run_goldfish_loop(&mut state, &UnreachableStrategy, false)
            } else {
                run_game_loop(&mut state, &UnreachableStrategy, &UnreachableStrategy, false)
            };
            assert_eq!(result.outcome,
                GameOutcome::Invalid(TerminationReason::UnsupportedContinuingElimination));
            assert_eq!(result.actions_taken, 0);
            assert_eq!(bincode::serialize(&state).unwrap(), before);
        }
    }

    #[test]
    fn legacy_terminal_counted_action_is_an_unchanged_rules_draw() {
        let mut state = GameState::new(2);
        state.game_over = true;
        let before = bincode::serialize(&state).unwrap();
        assert_eq!(apply_counted_action(&mut state, &Action::PassPriority,
            &[Action::PassPriority]), Ok(false));
        assert_eq!(no_progress_outcome(&state), GameOutcome::Draw);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }
}

#[cfg(test)]
mod returned_loss_provenance_tests {
    use super::*;
    use crate::game::{LossCause, LossCoordinates, LossFact, Phase};
    use crate::strategy::GoldfishStrategy;

    #[test]
    fn simulation_results_retain_exact_terminal_and_unsupported_loss_records() {
        for players in [2, 3] {
            let mut state = GameState::new(players);
            state.turn_number = 9;
            state.active_player = 1;
            state.priority_player = 1;
            state.phase = Phase::PreCombatMain;
            state.loss_boundary.accepted_actions = 17;
            state.loss_boundary.turns_taken = vec![4; players];
            state.players[0].life = 0;
            rules::check_state_based_actions(&mut state);
            let expected = state.loss_boundary.clone();
            let facts = vec![LossFact { player: 0, causes: vec![LossCause::LifeTotal] }];
            let coordinates = LossCoordinates { global_turn: 9, active_seat: 1,
                player_turns: vec![4; players], phase: Phase::PreCombatMain, action_index: 17 };
            if players == 2 {
                let record = expected.terminal.as_ref().expect("supported terminal loss");
                assert_eq!(record.losses, facts);
                assert_eq!(record.coordinates, coordinates);
            } else {
                let record = expected.unsupported.as_ref().expect("unsupported continuing loss");
                assert_eq!(record.losses, facts);
                assert_eq!(record.coordinates, coordinates);
            }
            let result = run_game_loop(&mut state.clone(), &GoldfishStrategy, &GoldfishStrategy, false);
            let goldfish_result = run_goldfish_loop(&mut state, &GoldfishStrategy, false);
            assert_eq!(result.loss_boundary, expected);
            assert_eq!(goldfish_result.loss_boundary, expected);
            assert_eq!(result.actions_taken, 0);
            assert_eq!(goldfish_result.actions_taken, 0);
        }
    }
}

#[cfg(test)]
mod completed_invalid_action_tests {
    use super::*;
    use crate::action::Action;
    use crate::card::{ActivatedAbility, CardDef, CardType, Effect, ObjectId, ZoneType};
    use crate::game::{LossCause, Phase};
    use crate::mana::ManaCost;
    use crate::strategy::GoldfishStrategy;

    struct PayLastLife(ObjectId);
    impl Strategy for PayLastLife {
        fn choose_action(&self, _: &GameState, _: PlayerIndex) -> Action {
            Action::ActivateAbility { object_id: self.0, ability_index: 0, targets: vec![] }
        }
        fn name(&self) -> &str { "pay last life" }
    }

    #[test]
    fn completed_life_cost_action_is_counted_in_invalid_simulation_results() {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 987_101, name: "Life-cost artifact".into(),
            card_types: vec![CardType::Artifact], activated_abilities: vec![ActivatedAbility {
                cost: ManaCost::zero(), requires_tap: false, sacrifice_cost: None,
                life_cost: 1, effect: Effect::GainLife { amount: 1 }, description: "Pay 1 life".into(),
            }], ..Default::default() });
        let mut state = GameState::new(3);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::PreCombatMain;
        state.players[0].life = 1;
        state.loss_boundary.accepted_actions = 41;
        let source = state.create_card_in_zone(987_101, 0, ZoneType::Battlefield);
        let strategy = PayLastLife(source);
        assert!(legal_actions(&state).contains(&strategy.choose_action(&state, 0)));
        let result = run_game_loop(&mut state.clone(), &strategy, &GoldfishStrategy, false);
        let goldfish_result = run_goldfish_loop(&mut state, &strategy, false);
        for result in [result, goldfish_result] {
            assert_eq!(result.outcome,
                GameOutcome::Invalid(TerminationReason::UnsupportedContinuingElimination));
            assert_eq!(result.actions_taken, 1);
            assert_eq!(result.rejected_actions, 0);
            assert_eq!(result.loss_boundary.accepted_actions, 42);
            let record = result.loss_boundary.unsupported.unwrap();
            assert_eq!(record.coordinates.action_index, 42);
            assert_eq!(record.losses[0].player, 0);
            assert_eq!(record.losses[0].causes, vec![LossCause::LifeTotal]);
        }
    }
}
