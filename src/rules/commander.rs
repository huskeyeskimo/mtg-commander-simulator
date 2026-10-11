//! Shared admission and bounded printed-cost quotation for command-zone casts.
use crate::card::ObjectId;
use crate::game::{GameState, Phase};
use crate::mana::ManaCost;

pub(crate) fn commander_cast_cost(state: &GameState, object: ObjectId) -> Option<ManaCost> {
    if state.gameplay_stopped() || state.validate_commanders().is_err()
        || state.validate_trigger_placement().is_err()
        || state.trigger_placement.is_some() || state.pending_copy_order.is_some()
        || state.pending_tutor.is_some() || state.cleanup_discard_in_progress
        || matches!(state.phase, Phase::Untap | Phase::Mulligan)
        || (state.phase == Phase::Cleanup && !state.cleanup_needs_repeat)
        || (state.phase == Phase::DeclareAttackers && state.priority_player == state.active_player && state.combat.attackers.is_empty())
        || (state.phase == Phase::DeclareBlockers && state.priority_player != state.active_player) {
        return None;
    }
    let player = state.priority_player;
    let seat = state.players.get(player)?;
    let c = state.commander_for_object(object)?;
    if seat.has_lost || c.designation.owner != player || !seat.command_zone.contains(&object) { return None; }
    let inst = state.objects.get(&object)?;
    let def = state.card_db().get(inst.card_def_id)?;
    if def.is_land() || def.is_aura() { return None; }
    let mut future = inst.clone();
    future.controller = player;
    future.zone_change_count = future.zone_change_count.checked_add(2)?;
    if state.prospective_aura(future, def).ok()? { return None; }
    if !def.is_instant_speed() && !(state.phase.is_main_phase() && state.active_player == player && state.stack.is_empty()) {
        return None;
    }
    state.next_stack_id.checked_add(1)?;
    let casts = *c.casts_by_player.get(player)?;
    casts.checked_add(1)?.checked_mul(2)?;
    let mut cost = def.mana_cost.clone()?;
    cost.generic = cost.generic.checked_add(casts.checked_mul(2)?)?
        .checked_add(super::mana::total_cost_increase(state, player, def.is_creature()))?;
    let mut reduction = super::mana::total_cost_reduction(state, player, def.is_creature());
    // Affinity is an automatic reduction. Do not import the general helper's
    // approximate Convoke/Delve payments into this printed-cost path.
    if def.keywords.contains(&crate::card::KeywordAbility::AffinityForArtifacts) {
        let artifacts = state.battlefield.iter().filter(|id| state.objects.get(id).is_some_and(|inst|
            inst.controller == player && state.card_db().get(inst.card_def_id).is_some_and(|def| def.is_artifact()))).count();
        reduction = reduction.checked_add(u32::try_from(artifacts).ok()?)?;
    }
    let cost = super::mana::apply_cost_reduction(&cost, reduction);
    // The existing planner caps its search by total generic + colored cost.
    // Reject an unrepresentable quote before entering its arithmetic.
    [cost.white, cost.blue, cost.black, cost.red, cost.green]
        .into_iter().try_fold(cost.generic, u32::checked_add)?;
    super::mana::can_pay_cost(state, player, &cost, None).then_some(cost)
}
