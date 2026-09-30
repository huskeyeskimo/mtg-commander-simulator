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

/// A tap decision: which permanent to tap and what mana it produces.
enum TapDecision {
    Color(ObjectId, crate::mana::Color),
    Colorless(ObjectId, u32),
}

/// Compute a "constraint score" for a mana source: lower = more constrained.
/// More constrained sources should be tapped first (for their specific color)
/// so that flexible sources remain available for other requirements.
fn constraint_score(abilities: &[ManaAbility]) -> u32 {
    let mut colors_possible = std::collections::HashSet::new();
    let mut has_any = false;
    for ma in abilities {
        match ma {
            ManaAbility::TapForColor(c) => {
                colors_possible.insert(*c);
            }
            ManaAbility::TapForChoice(colors) => {
                for c in colors {
                    colors_possible.insert(*c);
                }
            }
            ManaAbility::TapForAny => {
                has_any = true;
            }
            ManaAbility::TapForLegendaryColors => {
                // Conditional — treat as flexible since it depends on board state
                has_any = true;
            }
            ManaAbility::TapForColorless | ManaAbility::TapForColorlessAmount(_) => {}
        }
    }
    if has_any {
        100 // Very flexible — tap last
    } else if colors_possible.is_empty() {
        50 // Colorless only — moderately flexible
    } else {
        colors_possible.len() as u32 // 1 = single color, 2 = dual, etc.
    }
}

/// Auto-tap mana sources (lands, artifacts, creatures) to pay a mana cost.
///
/// Uses a most-constrained-first strategy:
/// 1. Sort all mana sources by constraint score (single-color first, then dual, then any)
/// 2. Tap sources for colored requirements first, preferring single-color sources
/// 3. Tap remaining sources for generic requirements, preferring colorless-only sources
/// 4. Handle TapForAny by producing the actually needed color
/// 5. Handle TapForChoice by choosing the most needed color
/// 6. Apply Kinnan-style mana bonuses for nonland sources
///
/// Two-phase approach: collect tap decisions (read-only), then apply them (mutate).
pub fn auto_tap_lands(
    state: &mut GameState,
    player: PlayerIndex,
    cost: &crate::mana::ManaCost,
) {
    auto_tap_lands_excluding(state, player, cost, None);
}

/// Reserve a permanent that must remain untapped to pay another activation cost.
pub(super) fn auto_tap_lands_excluding(
    state: &mut GameState,
    player: PlayerIndex,
    cost: &crate::mana::ManaCost,
    reserved: Option<ObjectId>,
) {
    let mut decisions: Vec<TapDecision> = Vec::new();

    // Pre-compute legendary colors for Mox Amber (needs immutable borrow)
    let leg_colors = legendary_colors(state, player);

    // Phase 1: Collect tap decisions (immutable borrow)
    {
        let db = state.card_db();
        let sources = state.untapped_mana_sources(player);

        // Build a list of (object_id, mana_abilities, is_land, constraint_score)
        struct SourceInfo {
            id: ObjectId,
            abilities: Vec<ManaAbility>,
            score: u32,
        }

        let mut source_infos: Vec<SourceInfo> = sources
            .iter()
            .filter_map(|&id| {
                if Some(id) == reserved { return None; }
                let inst = state.objects.get(&id)?;
                let def = db.get(inst.card_def_id)?;
                Some(SourceInfo {
                    id,
                    abilities: def.mana_abilities.clone(),
                    score: constraint_score(&def.mana_abilities),
                })
            })
            .collect();

        // Sort by constraint score: most constrained (lowest) first
        source_infos.sort_by_key(|s| s.score);

        let mut tapped_set = std::collections::HashSet::new();

        // --- Pass 1: Pay colored costs ---
        // For each color needed, find the most constrained source that can produce it.
        for &color in &Color::ALL {
            let needed = cost.color_amount(color);
            let already_have = state.players[player].mana_pool.get(color);
            if needed <= already_have {
                continue;
            }
            let mut still_need = needed - already_have;

            // First pass: prefer single-color sources that make exactly this color
            for info in &source_infos {
                if still_need == 0 {
                    break;
                }
                if tapped_set.contains(&info.id) {
                    continue;
                }
                // Only use the most constrained sources for colored costs
                // (score == 1 means single-color, which is ideal for colored payment)
                if info.score > 1 {
                    continue;
                }
                let produces = info.abilities.iter().any(|ma| matches!(ma, ManaAbility::TapForColor(c) if *c == color));
                if produces {
                    decisions.push(TapDecision::Color(info.id, color));
                    tapped_set.insert(info.id);
                    still_need -= 1;
                }
            }

            // Second pass: use multi-color / any sources if single-color wasn't enough
            for info in &source_infos {
                if still_need == 0 {
                    break;
                }
                if tapped_set.contains(&info.id) {
                    continue;
                }
                let produces_color = info.abilities.iter().any(|ma| match ma {
                    ManaAbility::TapForColor(c) => *c == color,
                    ManaAbility::TapForChoice(colors) => colors.contains(&color),
                    ManaAbility::TapForAny => true,
                    ManaAbility::TapForLegendaryColors => leg_colors.contains(&color),
                    _ => false,
                });
                if produces_color {
                    decisions.push(TapDecision::Color(info.id, color));
                    tapped_set.insert(info.id);
                    still_need -= 1;
                }
            }
        }

        // --- Pass 2: Pay generic costs ---
        let colored_from_decisions: u32 = decisions.len() as u32;
        // Count total mana already accounted for (pool + decisions)
        let pool_total = state.players[player].mana_pool.total() + colored_from_decisions;
        let colored_total: u32 = Color::ALL.iter().map(|&c| cost.color_amount(c)).sum();
        let total_needed = colored_total + cost.generic;

        if pool_total < total_needed {
            let mut still_need = total_needed.saturating_sub(pool_total);

            // Track remaining color needs for smart color choice
            let _remaining_color_needs: [u32; 5] = [0; 5]; // future use

            // Prefer colorless-only sources for generic costs (save colored for later)
            // Sort remaining by: colorless-only first, then by constraint score desc
            // (most flexible last = we keep them available)
            let mut remaining: Vec<&SourceInfo> = source_infos
                .iter()
                .filter(|s| !tapped_set.contains(&s.id))
                .collect();

            // Partition: colorless-only sources first, then colored sources
            // Within each group, sort by amount produced (descending) to minimize taps
            remaining.sort_by_key(|s| {
                let is_colorless_only = s.abilities.iter().all(|ma| {
                    matches!(
                        ma,
                        ManaAbility::TapForColorless | ManaAbility::TapForColorlessAmount(_)
                    )
                });
                let amount = mana_amount(&s.abilities);
                if is_colorless_only {
                    (0, std::cmp::Reverse(amount)) // colorless first, most mana first
                } else {
                    (1, std::cmp::Reverse(amount)) // colored later
                }
            });

            for info in &remaining {
                if still_need == 0 {
                    break;
                }
                if tapped_set.contains(&info.id) {
                    continue;
                }

                if let Some(ma) = info.abilities.first() {
                    let produced = match ma {
                        ManaAbility::TapForColor(c) => {
                            decisions.push(TapDecision::Color(info.id, *c));
                            1
                        }
                        ManaAbility::TapForColorless => {
                            decisions.push(TapDecision::Colorless(info.id, 1));
                            1
                        }
                        ManaAbility::TapForAny => {
                            // For generic, produce colorless
                            decisions.push(TapDecision::Colorless(info.id, 1));
                            1
                        }
                        ManaAbility::TapForColorlessAmount(n) => {
                            decisions.push(TapDecision::Colorless(info.id, *n));
                            *n
                        }
                        ManaAbility::TapForChoice(colors) => {
                            if let Some(&c) = colors.first() {
                                decisions.push(TapDecision::Color(info.id, c));
                            } else {
                                decisions.push(TapDecision::Colorless(info.id, 1));
                            }
                            1
                        }
                        ManaAbility::TapForLegendaryColors => {
                            if let Some(&c) = leg_colors.first() {
                                decisions.push(TapDecision::Color(info.id, c));
                                1
                            } else {
                                0 // No legendary creatures/planeswalkers — produces nothing
                            }
                        }
                    };
                    tapped_set.insert(info.id);
                    still_need = still_need.saturating_sub(produced);
                }
            }
        }
    }

    // Check for Kinnan-style mana bonus (nonland sources produce extra)
    let bonus_count = super::triggers::mana_from_nonland_bonus_count(state, player);

    // Phase 2: Apply decisions (mutable borrow)
    for decision in &decisions {
        match decision {
            TapDecision::Color(source_id, color) => {
                state.players[player].mana_pool.add_color(*color, 1);
                if let Some(inst) = state.objects.get_mut(source_id) {
                    inst.tapped = true;
                }
                // Apply Kinnan bonus for nonland sources
                if bonus_count > 0 {
                    let is_nonland = {
                        let db = state.card_db();
                        state
                            .objects
                            .get(source_id)
                            .and_then(|inst| db.get(inst.card_def_id))
                            .map_or(false, |def| !def.card_types.contains(&CardType::Land))
                    };
                    if is_nonland {
                        // Add bonus colorless mana (simplified: any type → colorless)
                        state.players[player].mana_pool.colorless += bonus_count;
                    }
                }
            }
            TapDecision::Colorless(source_id, amount) => {
                state.players[player].mana_pool.colorless += amount;
                if let Some(inst) = state.objects.get_mut(source_id) {
                    inst.tapped = true;
                }
                // Apply Kinnan bonus for nonland sources
                if bonus_count > 0 {
                    let is_nonland = {
                        let db = state.card_db();
                        state
                            .objects
                            .get(source_id)
                            .and_then(|inst| db.get(inst.card_def_id))
                            .map_or(false, |def| !def.card_types.contains(&CardType::Land))
                    };
                    if is_nonland {
                        state.players[player].mana_pool.colorless += bonus_count;
                    }
                }
            }
        }
    }
}

/// How much total mana a source produces from its first ability.
fn mana_amount(abilities: &[ManaAbility]) -> u32 {
    abilities
        .first()
        .map(|ma| match ma {
            ManaAbility::TapForColorlessAmount(n) => *n,
            _ => 1,
        })
        .unwrap_or(0)
}
