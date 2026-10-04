use crate::card::{CardType, ManaAbility, ObjectId};
use crate::game::{GameState, PlayerIndex};
use crate::mana::Color;

/// Return the set of colors among legendary creatures and planeswalkers
/// controlled by `player`. Used for Mox Amber.
pub fn legendary_colors(state: &GameState, player: PlayerIndex) -> Vec<Color> {
    let db = state.card_db();
    let mut colors = std::collections::HashSet::new();
    for &obj_id in &state.battlefield {
        let inst = &state.objects[&obj_id];
        if inst.controller != player {
            continue;
        }
        let def = match db.get(inst.card_def_id) {
            Some(d) => d,
            None => continue,
        };
        let is_legendary = def.supertypes.contains(&crate::card::Supertype::Legendary);
        let is_creature = def.card_types.contains(&CardType::Creature);
        let is_planeswalker = def.card_types.contains(&crate::card::CardType::Planeswalker);
        if is_legendary && (is_creature || is_planeswalker) {
            if let Some(ref cost) = def.mana_cost {
                for c in cost.colors() {
                    colors.insert(c);
                }
            }
        }
    }
    let mut result: Vec<Color> = colors.into_iter().collect();
    result.sort_by_key(|c| *c as u8);
    result
}

/// Compute the total generic cost reduction for a spell being cast by `player`.
/// Checks all permanents the player controls for `CostReduction` abilities,
/// plus keyword-based cost reductions (Affinity for Artifacts).
pub fn total_cost_reduction(state: &GameState, player: PlayerIndex, is_creature: bool) -> u32 {
    use crate::card::CostReductionTarget;
    let db = state.card_db();
    let mut total = 0u32;
    for &obj_id in &state.battlefield {
        let inst = &state.objects[&obj_id];
        if inst.controller != player {
            continue;
        }
        let def = match db.get(inst.card_def_id) {
            Some(d) => d,
            None => continue,
        };
        if let Some(ref reduction) = def.cost_reduction {
            let applies = match reduction.applies_to {
                CostReductionTarget::AllSpells => true,
                CostReductionTarget::CreatureSpells => is_creature,
            };
            if applies {
                total += reduction.generic_reduction;
            }
        }
    }
    total
}

/// Compute the total generic cost increase (tax) for a spell being cast by `player`.
/// Checks all permanents on the battlefield for `CostIncrease` abilities.
pub fn total_cost_increase(state: &GameState, player: PlayerIndex, is_creature: bool) -> u32 {
    use crate::card::CostIncreaseTarget;
    let db = state.card_db();
    let mut total = 0u32;
    for &obj_id in &state.battlefield {
        let inst = &state.objects[&obj_id];
        let def = match db.get(inst.card_def_id) {
            Some(d) => d,
            None => continue,
        };
        if let Some(ref increase) = def.cost_increase {
            // Check if this tax applies to the caster
            let is_opponents_permanent = inst.controller != player;
            let applies_to_caster = if increase.affects_controller {
                inst.controller == player
            } else {
                is_opponents_permanent
            };
            if !applies_to_caster {
                continue;
            }
            let spell_matches = match increase.applies_to {
                CostIncreaseTarget::AllSpells => true,
                CostIncreaseTarget::NoncreatureSpells => !is_creature,
                CostIncreaseTarget::CreatureSpells => is_creature,
            };
            if spell_matches {
                total += increase.generic_increase;
            }
        }
    }
    total
}

/// Compute cost reduction for a specific spell, including spell-intrinsic keywords
/// like Affinity for Artifacts, Convoke, and Delve.
pub fn spell_cost_reduction(state: &GameState, player: PlayerIndex, card_def_id: crate::card::CardId) -> u32 {
    use crate::card::KeywordAbility;
    let db = state.card_db();
    let def = match db.get(card_def_id) {
        Some(d) => d,
        None => return 0,
    };

    let mut extra = 0u32;

    // Affinity for Artifacts: reduce by 1 for each artifact you control
    if def.keywords.contains(&KeywordAbility::AffinityForArtifacts) {
        let artifact_count = state
            .battlefield
            .iter()
            .filter(|&&id| {
                state
                    .objects
                    .get(&id)
                    .map_or(false, |inst| {
                        inst.controller == player
                            && db
                                .get(inst.card_def_id)
                                .map_or(false, |d| d.is_artifact())
                    })
            })
            .count() as u32;
        extra += artifact_count;
    }

    // Convoke: simplified — reduce by number of untapped creatures you could tap
    // (auto-convoke: tap as many as needed, up to generic cost)
    if def.keywords.contains(&KeywordAbility::Convoke) {
        let untapped_creatures = state
            .battlefield
            .iter()
            .filter(|&&id| {
                state.objects.get(&id).map_or(false, |inst| {
                    inst.controller == player
                        && !inst.tapped
                        && !inst.summoning_sick
                        && db.get(inst.card_def_id).map_or(false, |d| d.is_creature())
                })
            })
            .count() as u32;
        extra += untapped_creatures;
    }

    // Delve: simplified — reduce by number of cards in graveyard
    // (auto-delve: exile as many as needed, up to generic cost)
    if def.keywords.contains(&KeywordAbility::Delve) {
        let graveyard_count = state.players[player].graveyard.len() as u32;
        extra += graveyard_count;
    }

    extra
}

/// Apply cost reduction to a ManaCost, returning the reduced cost.
pub fn apply_cost_reduction(cost: &crate::mana::ManaCost, reduction: u32) -> crate::mana::ManaCost {
    let mut reduced = cost.clone();
    reduced.generic = reduced.generic.saturating_sub(reduction);
    reduced
}

/// A payment plan is computed without changing the game. Each source is tapped
/// at most once, with one of its currently modeled mana abilities selected.
#[derive(Clone)]
pub(crate) struct PaymentPlan {
    taps: Vec<(ObjectId, crate::mana::ManaPool)>,
    pool_after_taps: crate::mana::ManaPool,
}

struct SourceOptions {
    id: ObjectId,
    options: Vec<crate::mana::ManaPool>,
}

/// Find a concrete assignment for the current pool and available sources.
/// The same result drives action enumeration and committed payment.
pub(crate) fn plan_payment(
    state: &GameState,
    player: PlayerIndex,
    cost: &crate::mana::ManaCost,
    reserved: Option<ObjectId>,
) -> Option<PaymentPlan> {
    use crate::mana::ManaPool;
    if state.pending_copy_order.is_some() { return None; }
    let db = state.card_db();
    let legendary = legendary_colors(state, player);
    let nonland_bonus = super::triggers::mana_from_nonland_bonus_count(state, player);
    let swamp_bonus = super::triggers::mana_from_swamp_bonus_count(state, player);
    let mut sources = Vec::new();
    for id in state.untapped_mana_sources(player) {
        if Some(id) == reserved { continue; }
        let Some(inst) = state.objects.get(&id) else { continue; };
        let Some(def) = db.get(inst.card_def_id) else { continue; };
        let extra_colorless = if def.card_types.contains(&CardType::Land) { 0 } else { nonland_bonus };
        let extra_black = if def.subtypes.iter().any(|subtype| subtype.0 == "Swamp") { swamp_bonus } else { 0 };
        let mut options = Vec::new();
        for ability in &def.mana_abilities {
            let mut base = Vec::new();
            match ability {
                ManaAbility::TapForColor(color) => base.push(Some(*color)),
                ManaAbility::TapForAny => base.extend(Color::ALL.into_iter().map(Some)),
                ManaAbility::TapForChoice(colors) => base.extend(colors.iter().copied().map(Some)),
                ManaAbility::TapForLegendaryColors => base.extend(legendary.iter().copied().map(Some)),
                ManaAbility::TapForColorless | ManaAbility::TapForColorlessAmount(_) => base.push(None),
            }
            for color in base {
                let mut produced = ManaPool::empty();
                match color {
                    Some(color) => produced.add_color(color, 1),
                    None => produced.colorless += match ability {
                        ManaAbility::TapForColorlessAmount(amount) => *amount,
                        _ => 1,
                    },
                }
                produced.colorless += extra_colorless;
                produced.black += extra_black;
                if !options.contains(&produced) { options.push(produced); }
            }
        }
        if !options.is_empty() { sources.push(SourceOptions { id, options }); }
    }

    fn pool_key(pool: &ManaPool, cap: u32) -> [u32; 6] {
        [pool.white.min(cap), pool.blue.min(cap), pool.black.min(cap),
            pool.red.min(cap), pool.green.min(cap), pool.colorless.min(cap)]
    }
    fn search(
        index: usize,
        sources: &[SourceOptions],
        pool: ManaPool,
        cost: &crate::mana::ManaCost,
        taps: &mut Vec<(ObjectId, ManaPool)>,
        seen: &mut std::collections::HashSet<(usize, [u32; 6])>,
        cap: u32,
    ) -> Option<PaymentPlan> {
        if pool.can_pay(cost) {
            return Some(PaymentPlan { taps: taps.clone(), pool_after_taps: pool });
        }
        if index == sources.len() || !seen.insert((index, pool_key(&pool, cap))) {
            return None;
        }
        // Skip first: avoid spending an unnecessary source when later sources suffice.
        if let Some(plan) = search(index + 1, sources, pool.clone(), cost, taps, seen, cap) {
            return Some(plan);
        }
        for produced in &sources[index].options {
            taps.push((sources[index].id, produced.clone()));
            if let Some(plan) = search(index + 1, sources, pool.clone() + produced.clone(), cost, taps, seen, cap) {
                return Some(plan);
            }
            taps.pop();
        }
        None
    }
    let cap = cost.generic + Color::ALL.iter().map(|&color| cost.color_amount(color)).sum::<u32>();
    search(0, &sources, state.players[player].mana_pool.clone(), cost,
        &mut Vec::new(), &mut std::collections::HashSet::new(), cap)
}

pub(crate) fn can_pay_cost(
    state: &GameState, player: PlayerIndex, cost: &crate::mana::ManaCost,
    reserved: Option<ObjectId>,
) -> bool {
    plan_payment(state, player, cost, reserved).is_some()
}

/// Commit only a plan whose projected pool can pay the full adjusted cost.
pub(crate) fn pay_cost(
    state: &mut GameState, player: PlayerIndex, cost: &crate::mana::ManaCost,
    reserved: Option<ObjectId>,
) -> bool {
    let Some(plan) = plan_payment(state, player, cost, reserved) else { return false; };
    let mut paid = plan.pool_after_taps;
    if !paid.pay(cost) { return false; }
    for (id, _) in plan.taps {
        state.objects.get_mut(&id).expect("planned mana source").tapped = true;
    }
    state.players[player].mana_pool = paid;
    true
}

/// Produce mana for a known payable cost without consuming the cost.
pub fn auto_tap_lands(state: &mut GameState, player: PlayerIndex, cost: &crate::mana::ManaCost) {
    if let Some(plan) = plan_payment(state, player, cost, None) {
        for (id, _) in plan.taps {
            state.objects.get_mut(&id).expect("planned mana source").tapped = true;
        }
        state.players[player].mana_pool = plan.pool_after_taps;
    }
}
