//! Terminal-only adjudication. Continuing multiplayer elimination is rejected
//! before any leaving-game cleanup; this does not implement such cleanup.
use crate::game::{GameState, LossCause, LossFact, PlayerIndex, TerminalLoss, UnsupportedElimination};

/// Read the complete represented loss set, then establish the supported
/// terminal result or latch an explicit unsupported boundary. Returns true
/// when settlement must stop. No battlefield, zone, stack or trigger mutation.
pub(super) fn adjudicate(state: &mut GameState, concession: Option<PlayerIndex>) -> bool {
    if state.game_over || state.loss_boundary.unsupported.is_some() { return true; }
    let losses: Vec<LossFact> = state.players.iter().enumerate().filter_map(|(player, seat)| {
        let mut causes = Vec::new();
        if seat.life <= 0 { causes.push(LossCause::LifeTotal); }
        if seat.poison_counters >= 10 { causes.push(LossCause::Poison); }
        if state.is_commander_format() {
            for (source_player, damage) in seat.commander_damage_received.iter().enumerate() {
                if *damage >= 21 { causes.push(LossCause::CommanderDamage { source_player }); }
            }
        }
        if state.loss_boundary.pending_failed_draws.contains(&player) { causes.push(LossCause::FailedDraw); }
        if concession == Some(player) { causes.push(LossCause::Concession); }
        if seat.has_lost && causes.is_empty() { causes.push(LossCause::AlreadyLost); }
        (!causes.is_empty()).then_some(LossFact { player, causes })
    }).collect();
    if losses.is_empty() { return false; }
    let survivors: Vec<_> = (0..state.players.len()).filter(|player|
        !losses.iter().any(|loss| loss.player == *player)).collect();
    let coordinates = state.loss_coordinates();
    if survivors.len() >= 2 {
        state.loss_boundary.unsupported = Some(UnsupportedElimination { losses, coordinates });
    } else {
        for loss in &losses { state.players[loss.player].has_lost = true; }
        state.game_over = true;
        state.winner = survivors.first().copied();
        state.loss_boundary.pending_failed_draws.clear();
        state.loss_boundary.terminal = Some(TerminalLoss { losses, coordinates });
    }
    true
}
