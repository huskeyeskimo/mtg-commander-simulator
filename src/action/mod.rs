pub mod canonical;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

use crate::card::{CardId, Effect, KeywordAbility, ManaAbility, ObjectId};
use crate::game::{GameState, PlayerIndex, Target};

/// Controls whether combat actions use full enumeration or strategic bucketing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatAbstraction {
    /// Enumerate all 2^n attacker subsets and all blocking combinations.
    /// Exact but exponential — only feasible for small boards.
    Full,
    /// Bucket attackers into ~6 strategic postures and blockers into ~5 categories.
    /// Lossy but reduces the action space from O(2^n) to O(1).
    /// Falls back to Full when eligible attackers <= 5 (32 subsets).
    Bucketed,
}

/// An action a player can take when they have priority.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    /// Append this prepared copy to the bottom-to-top stack order. The last
    /// remaining copy is appended automatically, so two copies need one choice.
    ChooseNextCopy { item_index: usize },
    /// Pass priority.
    PassPriority,

    /// Discard a card from hand (used in cleanup).
    Discard { object_id: ObjectId },

    /// Play a land from hand.
    PlayLand { object_id: ObjectId },

    /// Cast a spell from hand.
    CastSpell {
        object_id: ObjectId,
        targets: Vec<Target>,
    },

    /// Activate a mana ability (doesn't use the stack).
    ActivateManaAbility {
        object_id: ObjectId,
        ability_index: usize,
    },

    /// Activate a non-mana ability.
    ActivateAbility {
        object_id: ObjectId,
        ability_index: usize,
        targets: Vec<Target>,
    },

    /// Declare attackers (a set of creature ObjectIds).
    DeclareAttackers { attackers: Vec<ObjectId> },

    /// Declare blockers (list of (blocker_id, attacker_id) pairs).
    DeclareBlockers {
        blocks: Vec<(ObjectId, ObjectId)>,
    },

    /// Order damage assignment for a blocked attacker.
    OrderDamageAssignment {
        attacker: ObjectId,
        assignment: Vec<(ObjectId, u32)>,
    },

    /// Choose the order to place simultaneous triggered abilities on the stack.
    /// When a player controls multiple triggers that would go on the stack at once,
    /// they choose the ordering. First element goes on the stack first (resolves last
    /// due to LIFO). Each entry is (source_id, ability_index).
    OrderTriggers {
        ordering: Vec<(ObjectId, usize)>,
    },

    /// Choose the order in which replacement effects apply to an event (CR 614).
    /// When multiple replacement effects could modify the same event, the affected
    /// player chooses the order. Each entry is (source_id, effect_index).
    /// The first element is applied first; subsequent effects apply to the
    /// already-modified event.
    ChooseReplacementOrder {
        ordering: Vec<(ObjectId, usize)>,
    },

    /// Cast commander from the command zone (Commander format).
    /// Functions like CastSpell but sourced from the command zone with
    /// commander tax applied to the cost.
    CastCommander {
        object_id: ObjectId,
        targets: Vec<Target>,
    },

    /// Keep the current hand (London Mulligan).
    MulliganKeep,

    /// Mulligan: shuffle hand into library and draw 7 new cards (London Mulligan).
    MulliganMulligan,

    /// Put a card from hand on the bottom of the library after keeping a mulliganed hand.
    /// The player must bottom N cards where N = mulligan_count.
    MulliganBottomCard { object_id: ObjectId },

    /// Choose a card to find from the library when a tutor effect resolves.
    /// The `card_id` identifies the card template to search for. Only cards
    /// in the player's `tutor_targets` list that exist in the library are
    /// offered as choices. MCCFR learns the optimal target per game state.
    ChooseTutorTarget { card_id: CardId },

    /// Concede the game.
    Concede,

    /// Equip an equipment to a target creature you control (sorcery speed).
    Equip {
        equipment_id: ObjectId,
        target_id: ObjectId,
    },

    /// Activate a loyalty ability on a planeswalker (sorcery speed, once per turn).
    ActivateLoyalty {
        object_id: ObjectId,
        ability_index: usize,
    },

    /// Cast a spell from the graveyard using Flashback or Escape.
    /// After resolution, the card is exiled instead of going to graveyard.
    CastFromGraveyard {
        object_id: ObjectId,
        targets: Vec<Target>,
    },

    /// Activate a pre-defined combo as a single macro-action.
    /// The combo_id indexes into the ComboRegistry attached to GameState.
    /// This collapses an infinite loop (e.g., Basalt Monolith + Kinnan
    /// for infinite mana) into a single action so the solver doesn't need
    /// to discover the loop step-by-step through depth.
    ActivateMacro { combo_id: usize },

    /// End the turn immediately, fast-forwarding through all remaining phases.
    /// Phase entries (triggers, SBA, combat damage) still execute, but
    /// priority is never yielded — all decisions are auto-resolved.
    /// This collapses O(phases) PassPriority actions into a single action,
    /// dramatically reducing search depth when the optimal play is "do nothing."
    EndTurn,

    /// Play a land from the graveyard (Crucible of Worlds, Conduit of Worlds, etc.).
    PlayLandFromGraveyard { object_id: ObjectId },
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::ChooseNextCopy { item_index } => write!(f, "Place copy {} next (bottom to top)", item_index),
            Action::PassPriority => write!(f, "Pass"),
            Action::Discard { object_id } => write!(f, "Discard (obj {})", object_id),
            Action::PlayLand { object_id } => write!(f, "Play land (obj {})", object_id),
            Action::CastSpell { object_id, .. } => write!(f, "Cast spell (obj {})", object_id),
            Action::ActivateManaAbility { object_id, .. } => {
                write!(f, "Tap for mana (obj {})", object_id)
            }
            Action::ActivateAbility { object_id, .. } => {
                write!(f, "Activate ability (obj {})", object_id)
            }
            Action::DeclareAttackers { attackers } => {
                write!(f, "Attack with {} creatures", attackers.len())
            }
            Action::DeclareBlockers { blocks } => {
                write!(f, "Block with {} creatures", blocks.len())
            }
            Action::OrderDamageAssignment { .. } => write!(f, "Assign damage"),
            Action::OrderTriggers { ordering } => {
                write!(f, "Order {} triggers", ordering.len())
            }
            Action::ChooseReplacementOrder { ordering } => {
                write!(f, "Order {} replacement effects", ordering.len())
            }
            Action::CastCommander { object_id, .. } => {
                write!(f, "Cast commander (obj {})", object_id)
            }
            Action::MulliganKeep => write!(f, "Keep hand"),
            Action::MulliganMulligan => write!(f, "Mulligan"),
            Action::MulliganBottomCard { object_id } => {
                write!(f, "Bottom card (obj {})", object_id)
            }
            Action::ChooseTutorTarget { card_id } => {
                write!(f, "Tutor for card {}", card_id)
            }
            Action::Equip { equipment_id, target_id } => {
                write!(f, "Equip (obj {} -> obj {})", equipment_id, target_id)
            }
            Action::ActivateLoyalty { object_id, ability_index } => {
                write!(f, "Activate loyalty #{} (obj {})", ability_index, object_id)
            }
            Action::CastFromGraveyard { object_id, .. } => {
                write!(f, "Cast from graveyard (obj {})", object_id)
            }
            Action::Concede => write!(f, "Concede"),
            Action::ActivateMacro { combo_id } => {
                write!(f, "Activate combo #{}", combo_id)
            }
            Action::EndTurn => write!(f, "End turn"),
            Action::PlayLandFromGraveyard { object_id } => {
                write!(f, "Play land from graveyard (obj {})", object_id)
            }
        }
    }
}

/// Enumerate legal actions with combat abstraction applied.
/// Uses strategic bucketing for attacker/blocker combinations to reduce
/// the action space from O(2^n) to O(1) for MCCFR traversal.
pub fn legal_actions_abstracted(state: &GameState) -> Vec<Action> {
    legal_actions_with(state, CombatAbstraction::Bucketed)
}

/// Enumerate all legal actions for the player who currently has priority.
pub fn legal_actions(state: &GameState) -> Vec<Action> {
    legal_actions_with(state, CombatAbstraction::Full)
}

/// Maximum number of mulligans allowed before auto-keeping.
pub const MAX_MULLIGANS: u32 = 4;

/// Core action enumeration with configurable combat abstraction level.
fn legal_actions_with(state: &GameState, abstraction: CombatAbstraction) -> Vec<Action> {
    use crate::game::Phase;

    let player = state.priority_player;
    let mut actions = Vec::new();

    if let Some(pending) = &state.pending_copy_order {
        if player != pending.controller() { return actions; }
        return (0..pending.items().len())
            .filter(|index| !pending.selected_order().contains(index))
            .map(|item_index| Action::ChooseNextCopy { item_index })
            .collect();
    }

    // ---- Mulligan phase ----
    if state.phase == Phase::Mulligan {
        let ps = &state.players[player];
        if !ps.mulligan_decided {
            // Player hasn't decided yet: offer keep or mulligan
            actions.push(Action::MulliganKeep);
            if ps.mulligan_count < MAX_MULLIGANS {
                actions.push(Action::MulliganMulligan);
            }
            return actions;
        }
        // Player decided to keep but still has cards to bottom
        let target_hand_size = 7u32.saturating_sub(ps.mulligan_count) as usize;
        if ps.hand.len() > target_hand_size {
            // Must choose a card to put on bottom
            for &obj_id in &ps.hand {
                actions.push(Action::MulliganBottomCard { object_id: obj_id });
            }
            return actions;
        }
        // Should not reach here — advance_mulligan handles phase transitions
        unreachable!(
            "Mulligan phase: player {} has decided (mull_count={}) and hand size {} <= target {}; \
             advance_mulligan should have transitioned out of Mulligan phase",
            player,
            ps.mulligan_count,
            ps.hand.len(),
            target_hand_size,
        );
    }

    // Before normal priority actions, check for pending triggers needing ordering.
    // When a player controls multiple simultaneous triggers, they must choose the
    // order to place them on the stack. This is a real strategic decision that
    // MCCFR must be able to observe and optimize over.
    if !state.pending_triggers.is_empty() {
        let player_triggers: Vec<&crate::game::PendingTrigger> = state
            .pending_triggers
            .iter()
            .filter(|t| t.controller == player)
            .collect();

        if player_triggers.len() > 1 {
            let keys: Vec<(ObjectId, usize)> = player_triggers
                .iter()
                .map(|t| (t.source_id, t.ability_index))
                .collect();

            for perm in generate_permutations(&keys) {
                actions.push(Action::OrderTriggers { ordering: perm });
            }
            actions.push(Action::Concede);
            return actions;
        }
    }

    // ---- Pending tutor: controller must choose a card from their library ----
    // The tutor choice is part of ability/spell resolution and is always made
    // by the controller, regardless of who currently has priority.
    if let Some(ref pending) = state.pending_tutor {
        let tutor_controller = pending.controller;
        let available = tutor_target_actions(state, tutor_controller);
        if available.is_empty() {
            // No valid targets in library — "fail to find" (pass clears it)
            actions.push(Action::PassPriority);
        } else {
            actions.extend(available);
        }
        actions.push(Action::Concede);
        return actions;
    }

    let forced_discard = state.phase == Phase::Cleanup
        && player == state.active_player
        && state.players[player].hand.len() > 7;

    if forced_discard {
        // Cleanup discard is mandatory; PassPriority is intentionally omitted here.
        for &obj_id in &state.players[player].hand {
            actions.push(Action::Discard { object_id: obj_id });
        }
        return actions;
    }

    // Player can always pass priority
    actions.push(Action::PassPriority);

    // EndTurn: collapses all remaining PassPriority actions for the turn
    // into one, reducing search depth.  Offered when:
    //  - post-combat phases (always safe), OR
    //  - PreCombatMain when the active player has no eligible attackers,
    //    since combat would be a no-op anyway.
    if player == state.active_player
        && state.stack.is_empty()
        && state.pending_triggers.is_empty()
    {
        let dominated_combat = matches!(
            state.phase,
            Phase::PostCombatMain | Phase::EndStep | Phase::EndOfCombat
        );
        let no_attackers_precombat = state.phase == Phase::PreCombatMain && {
            let creatures = state.creatures_controlled_by(player);
            !creatures.iter().any(|&id| {
                let inst = &state.objects[&id];
                !inst.tapped
                    && (!inst.summoning_sick
                        || state.has_keyword(id, KeywordAbility::Haste))
                    && !state.has_keyword(id, KeywordAbility::Defender)
            })
        };
        if dominated_combat || no_attackers_precombat {
            actions.push(Action::EndTurn);
        }
    }

    // Phase-specific action generation
    match state.phase {
        Phase::DeclareAttackers
            if player == state.active_player && state.combat.attackers.is_empty() =>
        {
            // Enumerate attacker combinations
            let creatures = state.creatures_controlled_by(player);
            let eligible: Vec<ObjectId> = creatures
                .into_iter()
                .filter(|&id| {
                    let inst = &state.objects[&id];
                    !inst.tapped
                        && (!inst.summoning_sick || state.has_keyword(id, KeywordAbility::Haste))
                        && !state.has_keyword(id, KeywordAbility::Defender)
                })
                .collect();

            // CR 508.1d: Creatures that must attack do so if able.
            let must_attack: Vec<ObjectId> = eligible
                .iter()
                .filter(|&&id| state.has_keyword(id, KeywordAbility::MustAttack))
                .copied()
                .collect();

            let use_buckets = abstraction == CombatAbstraction::Bucketed
                && eligible.len() > 5;

            let subsets = if use_buckets {
                generate_attack_buckets(&eligible, state)
            } else {
                // Full enumeration (including empty = no attack).
                // For large boards without abstraction, cap at 10 attackers.
                generate_subsets(&eligible, 10)
            };
            for subset in subsets {
                // CR 508.1d: Must-attack creatures must attack if able.
                // If any must-attack creature is eligible, the empty set (no attack)
                // is only legal if none of them can attack — but they're already in
                // `eligible`, so they can attack. Any non-empty subset must include
                // all must-attack creatures.
                if !must_attack.is_empty() {
                    if subset.is_empty() {
                        // Can't decline to attack when must-attack creatures exist
                        continue;
                    }
                    if !must_attack.iter().all(|ma| subset.contains(ma)) {
                        continue;
                    }
                }
                actions.push(Action::DeclareAttackers { attackers: subset });
            }
        }

        Phase::DeclareBlockers if player != state.active_player => {
            // Enumerate blocking assignments
            let creatures = state.creatures_controlled_by(player);
            let db = state.card_db();
            let eligible_blockers: Vec<ObjectId> = creatures
                .into_iter()
                .filter(|&id| {
                    let inst = &state.objects[&id];
                    !inst.tapped
                        && db.get(inst.card_def_id).is_some()
                        && !state.has_keyword(id, KeywordAbility::CantBlock)
                })
                .collect();

            let attackers = &state.combat.attackers;
            if attackers.is_empty() {
                // No attackers — just pass
            } else if abstraction == CombatAbstraction::Bucketed {
                let blocking_combos =
                    generate_block_buckets(&eligible_blockers, attackers, state);
                for combo in blocking_combos {
                    actions.push(Action::DeclareBlockers { blocks: combo });
                }
            } else {
                // Full enumeration of blocking combinations.
                let blocking_combos =
                    generate_blocking_assignments(&eligible_blockers, attackers, state);
                for combo in blocking_combos {
                    actions.push(Action::DeclareBlockers { blocks: combo });
                }
            }
        }

        _ => {
            // Standard priority actions
            let is_main = state.phase.is_main_phase()
                && player == state.active_player
                && state.stack.is_empty();

            let db = state.card_db();
            let hand = &state.players[player].hand;

            for &obj_id in hand {
                let inst = &state.objects[&obj_id];
                let def = match db.get(inst.card_def_id) {
                    Some(d) => d,
                    None => continue,
                };

                // Play lands (sorcery speed, once per turn)
                if def.is_land()
                    && is_main
                    && state.players[player].land_plays_remaining > 0
                {
                    actions.push(Action::PlayLand { object_id: obj_id });
                }

                // Cast spells
                if !def.is_land() {
                    let can_cast_timing = if def.is_instant_speed() {
                        true // instants and flash anytime you have priority
                    } else {
                        is_main // sorcery speed
                    };

                    if can_cast_timing {
                        // Check if player can pay the mana cost (with cost reduction & tax)
                        if let Some(ref cost) = def.mana_cost {
                            let reduction = crate::rules::total_cost_reduction(
                                state, player, def.is_creature(),
                            );
                            let spell_reduction = crate::rules::spell_cost_reduction(
                                state, player, inst.card_def_id,
                            );
                            let tax = crate::rules::total_cost_increase(
                                state, player, def.is_creature(),
                            );
                            let mut adjusted = crate::rules::apply_cost_reduction(cost, reduction + spell_reduction);
                            adjusted.generic += tax;
                            if can_potentially_pay(state, player, &adjusted) {
                                let targets = enumerate_targets_for_spell(state, player, def);
                                if targets.is_empty() {
                                    // Only offer targetless cast if the spell doesn't
                                    // require targets. Spells like counterspells that
                                    // need a target on the stack shouldn't be castable
                                    // when no valid target exists.
                                    if !spell_requires_target(def) {
                                        actions.push(Action::CastSpell {
                                            object_id: obj_id,
                                            targets: vec![],
                                        });
                                    }
                                } else {
                                    for target in targets {
                                        actions.push(Action::CastSpell {
                                            object_id: obj_id,
                                            targets: vec![target],
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Flashback / Escape: cast from graveyard
            {
                let graveyard = state.players[player].graveyard.clone();
                for &obj_id in &graveyard {
                    let inst = &state.objects[&obj_id];
                    let def = match db.get(inst.card_def_id) {
                        Some(d) => d,
                        None => continue,
                    };

                    // Flashback: cast instant/sorcery from graveyard for flashback cost
                    if let Some(ref fb_cost) = def.flashback_cost {
                        let can_cast_timing = if def.is_instant_speed() {
                            true
                        } else {
                            is_main
                        };
                        if can_cast_timing {
                            let reduction = crate::rules::total_cost_reduction(
                                state, player, def.is_creature(),
                            );
                            let reduced = crate::rules::apply_cost_reduction(fb_cost, reduction);
                            if can_potentially_pay(state, player, &reduced) {
                                let targets = enumerate_targets_for_spell(state, player, def);
                                if targets.is_empty() && !spell_requires_target(def) {
                                    actions.push(Action::CastFromGraveyard {
                                        object_id: obj_id,
                                        targets: vec![],
                                    });
                                } else {
                                    for target in targets {
                                        actions.push(Action::CastFromGraveyard {
                                            object_id: obj_id,
                                            targets: vec![target],
                                        });
                                    }
                                }
                            }
                        }
                    }

                    // Escape: cast from graveyard for regular mana cost + exile N cards
                    if let Some(exile_count) = def.escape_exile_count {
                        let can_cast_timing = if def.is_instant_speed() {
                            true
                        } else {
                            is_main
                        };
                        // Need enough other cards in graveyard to exile
                        let gy_count = state.players[player].graveyard.len() as u32;
                        if can_cast_timing && gy_count > exile_count {
                            if let Some(ref cost) = def.mana_cost {
                                let reduction = crate::rules::total_cost_reduction(
                                    state, player, def.is_creature(),
                                );
                                let reduced = crate::rules::apply_cost_reduction(cost, reduction);
                                if can_potentially_pay(state, player, &reduced) {
                                    let targets = enumerate_targets_for_spell(state, player, def);
                                    if targets.is_empty() && !spell_requires_target(def) {
                                        actions.push(Action::CastFromGraveyard { object_id: obj_id, targets: vec![] });
                                    } else {
                                        for target in targets {
                                            actions.push(Action::CastFromGraveyard { object_id: obj_id, targets: vec![target] });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Play lands from graveyard (Crucible of Worlds, Conduit of Worlds, etc.)
            if is_main && state.players[player].land_plays_remaining > 0 {
                let has_gy_land_play = state.battlefield.iter().any(|&bid| {
                    let binst = &state.objects[&bid];
                    if binst.controller != player { return false; }
                    if let Some(bdef) = db.get(binst.card_def_id) {
                        bdef.static_abilities.iter().any(|sa| {
                            matches!(sa, crate::layers::StaticAbility::PlayLandsFromGraveyard)
                        })
                    } else {
                        false
                    }
                });
                if has_gy_land_play {
                    let graveyard = state.players[player].graveyard.clone();
                    for &obj_id in &graveyard {
                        let inst = &state.objects[&obj_id];
                        let def = match db.get(inst.card_def_id) {
                            Some(d) => d,
                            None => continue,
                        };
                        if def.is_land() {
                            actions.push(Action::PlayLandFromGraveyard { object_id: obj_id });
                        }
                    }
                }
            }

            // Commander: cast commander(s) from command zone
            if state.is_commander_format() && is_main {
                let cmd_zone = &state.players[player].command_zone;
                for &obj_id in cmd_zone {
                    let inst = &state.objects[&obj_id];
                    let def = match db.get(inst.card_def_id) {
                        Some(d) => d,
                        None => continue,
                    };
                    // Must be this player's commander or partner commander
                    let is_primary = state.players[player].commander_card_id == Some(inst.card_def_id);
                    let is_partner = state.players[player].partner_commander_card_id == Some(inst.card_def_id);
                    if !is_primary && !is_partner {
                        continue;
                    }
                    if let Some(ref cost) = def.mana_cost {
                        // Commander tax: separate tracking for partner
                        let tax = if is_partner {
                            state.players[player].partner_commander_tax
                        } else {
                            state.players[player].commander_tax
                        };
                        let mut taxed_cost = cost.clone();
                        taxed_cost.generic += tax * 2;
                        let reduction = crate::rules::total_cost_reduction(
                            state, player, def.is_creature(),
                        );
                        let final_cost = crate::rules::apply_cost_reduction(&taxed_cost, reduction);
                        if can_potentially_pay(state, player, &final_cost) {
                            let targets = enumerate_targets_for_spell(state, player, def);
                            if targets.is_empty() {
                                actions.push(Action::CastCommander {
                                    object_id: obj_id,
                                    targets: vec![],
                                });
                            } else {
                                for target in targets {
                                    actions.push(Action::CastCommander {
                                        object_id: obj_id,
                                        targets: vec![target],
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Activated abilities and mana abilities from permanents
            let permanents = state.permanents_controlled_by(player);
            for &obj_id in &permanents {
                let inst = &state.objects[&obj_id];
                let def = match db.get(inst.card_def_id) {
                    Some(d) => d,
                    None => continue,
                };

                for (i, ability) in def.activated_abilities.iter().enumerate() {
                    if ability.requires_tap && !state.can_pay_tap_cost(obj_id) {
                        continue;
                    }
                    let reserved = ability.requires_tap.then_some(obj_id);
                    if can_potentially_pay_excluding(state, player, &ability.cost, reserved) {
                        match &ability.effect {
                            Effect::ExileFromHandLinked => {
                                // Generate one action per card in hand
                                for &hand_id in &state.players[player].hand {
                                    actions.push(Action::ActivateAbility {
                                        object_id: obj_id,
                                        ability_index: i,
                                        targets: vec![Target::Object(hand_id)],
                                    });
                                }
                            }
                            Effect::ReturnLinkedExileToHand => {
                                // Generate one action per card exiled by this permanent
                                for &eid in &state.players[player].exile {
                                    if state.objects[&eid].exiled_by == Some(obj_id) {
                                        actions.push(Action::ActivateAbility {
                                            object_id: obj_id,
                                            ability_index: i,
                                            targets: vec![Target::Object(eid)],
                                        });
                                    }
                                }
                            }
                            _ => {
                                actions.push(Action::ActivateAbility {
                                    object_id: obj_id,
                                    ability_index: i,
                                    targets: vec![], // simplified
                                });
                            }
                        }
                    }
                }

                // Mana abilities (tap abilities that don't use the stack)
                if state.can_pay_tap_cost(obj_id) && !def.mana_abilities.is_empty() {
                    for (i, ma) in def.mana_abilities.iter().enumerate() {
                        // Skip conditional mana abilities that can't produce mana
                        if matches!(ma, ManaAbility::TapForLegendaryColors) {
                            // Mox Amber: only offer if we control a legendary
                            // creature or planeswalker
                            let has_legendary = state.battlefield.iter().any(|&bid| {
                                if bid == obj_id { return false; } // skip self
                                let binst = &state.objects[&bid];
                                if binst.controller != player { return false; }
                                let bdef = match db.get(binst.card_def_id) {
                                    Some(d) => d,
                                    None => return false,
                                };
                                let is_leg = bdef.supertypes.contains(&crate::card::Supertype::Legendary);
                                let is_creature = bdef.card_types.contains(&crate::card::CardType::Creature);
                                let is_pw = bdef.card_types.contains(&crate::card::CardType::Planeswalker);
                                is_leg && (is_creature || is_pw)
                            });
                            if !has_legendary {
                                continue;
                            }
                        }
                        actions.push(Action::ActivateManaAbility {
                            object_id: obj_id,
                            ability_index: i,
                        });
                    }
                }

                // Equip abilities (sorcery speed, main phase, empty stack)
                if is_main && state.stack.is_empty() {
                    if let Some(ref equip_cost) = def.equip_cost {
                        if can_potentially_pay(state, player, equip_cost) {
                            // Find all creatures we control that we could equip to
                            let creatures = state.creatures_controlled_by(player);
                            for &creature_id in &creatures {
                                // Can't equip to itself; can equip to any creature we control
                                if creature_id != obj_id {
                                    actions.push(Action::Equip {
                                        equipment_id: obj_id,
                                        target_id: creature_id,
                                    });
                                }
                            }
                        }
                    }
                }

                // Loyalty abilities (sorcery speed, main phase, empty stack, once per turn)
                if is_main && state.stack.is_empty() && !def.loyalty_abilities.is_empty() {
                    if let Some(inst) = state.objects.get(&obj_id) {
                        if !inst.loyalty_activated_this_turn {
                            for (i, la) in def.loyalty_abilities.iter().enumerate() {
                                // Can activate if: positive cost, or loyalty >= abs(negative cost)
                                let can_pay = if la.cost >= 0 {
                                    true // +N: always available, adds counters
                                } else {
                                    inst.loyalty_counters as i32 + la.cost >= 0
                                };
                                if can_pay {
                                    actions.push(Action::ActivateLoyalty {
                                        object_id: obj_id,
                                        ability_index: i,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Macro-actions: inject registered combos that are currently available.
            // Only during main phases (when the player has full priority).
            if is_main {
                if let Some(ref registry) = state.combo_registry {
                    let available = crate::combo::detect_available_combos(state, player, registry);
                    for combo_id in available {
                        actions.push(Action::ActivateMacro { combo_id });
                    }
                }
            }
        }
    }

    actions
}

/// Check if a player can potentially pay a mana cost by tapping untapped mana sources
/// (lands, mana rocks, mana dorks, etc.).
fn can_potentially_pay(
    state: &GameState,
    player: PlayerIndex,
    cost: &crate::mana::ManaCost,
) -> bool {
    can_potentially_pay_excluding(state, player, cost, None)
}

fn can_potentially_pay_excluding(
    state: &GameState,
    player: PlayerIndex,
    cost: &crate::mana::ManaCost,
    reserved: Option<ObjectId>,
) -> bool {
    use crate::card::ManaAbility;
    use crate::mana::Color;

    // Start with current pool
    let mut pool = state.players[player].mana_pool.clone();

    // Add mana from all untapped mana sources (not just lands)
    let db = state.card_db();
    let sources = state.untapped_mana_sources(player);

    // First pass: count how much of each specific color is available
    // and how much flexible mana (TapForAny / TapForChoice) we have.
    let mut flexible_count: u32 = 0;
    let mut flexible_colors: Vec<Vec<Color>> = Vec::new();

    for &source_id in &sources {
        if Some(source_id) == reserved { continue; }
        let inst = &state.objects[&source_id];
        let def = match db.get(inst.card_def_id) {
            Some(d) => d,
            None => continue,
        };
        if let Some(ma) = def.mana_abilities.first() {
            match ma {
                ManaAbility::TapForColor(color) => {
                    pool.add_color(*color, 1);
                }
                ManaAbility::TapForColorless => {
                    pool.colorless += 1;
                }
                ManaAbility::TapForAny => {
                    // TapForAny can produce any color — optimistically assume it covers
                    // whatever we need most. Track as flexible mana.
                    flexible_count += 1;
                    flexible_colors.push(Color::ALL.to_vec());
                }
                ManaAbility::TapForChoice(colors) => {
                    // Can produce any of the listed colors
                    flexible_count += 1;
                    flexible_colors.push(colors.clone());
                }
                ManaAbility::TapForColorlessAmount(n) => {
                    pool.colorless += n;
                }
                ManaAbility::TapForLegendaryColors => {
                    // Mox Amber: check what colors legendary creatures/PWs provide
                    let leg_colors: Vec<Color> = state.battlefield.iter().filter_map(|&bid| {
                        let binst = state.objects.get(&bid)?;
                        if binst.controller != player { return None; }
                        let bdef = db.get(binst.card_def_id)?;
                        let is_leg = bdef.supertypes.contains(&crate::card::Supertype::Legendary);
                        let is_creature = bdef.card_types.contains(&crate::card::CardType::Creature);
                        let is_pw = bdef.card_types.contains(&crate::card::CardType::Planeswalker);
                        if is_leg && (is_creature || is_pw) {
                            bdef.mana_cost.as_ref().map(|c| c.colors())
                        } else {
                            None
                        }
                    }).flatten().collect();
                    if !leg_colors.is_empty() {
                        flexible_count += 1;
                        flexible_colors.push(leg_colors);
                    }
                }
            }
        }
    }

    // Check if we can pay with the fixed mana + flexible mana optimally allocated.
    // For each color shortfall, try to use flexible sources.
    let mut remaining_flexible = flexible_count;
    for &color in &Color::ALL {
        let needed = cost.color_amount(color);
        let have = pool.get(color);
        if needed > have {
            let shortfall = needed - have;
            if shortfall > remaining_flexible {
                return false;
            }
            remaining_flexible -= shortfall;
            // Account for it
            pool.add_color(color, shortfall);
        }
    }
    // Add remaining flexible as colorless (covers generic)
    pool.colorless += remaining_flexible;

    pool.can_pay(cost)
}

/// Returns true if a spell's effect requires a target to be legal.
/// Non-targeted spells (draw, destroy all, gain life, etc.) return false.
pub fn spell_requires_target(def: &crate::card::CardDef) -> bool {
    !matches!(crate::targeting::spell_targeting(def), crate::targeting::SpellTargeting::Untargeted)
}

fn enumerate_targets_for_spell(state: &GameState, caster: PlayerIndex, def: &crate::card::CardDef) -> Vec<Target> {
    crate::targeting::enumerate_spell_targets(state, caster, def)
}

/// Enumerate valid tutor target actions for a player with a pending tutor.
///
/// When the pending tutor has a non-empty `subtype_filter`, only cards in the
/// library with at least one matching subtype are offered (e.g., fetch lands
/// searching for Forest/Plains). Otherwise, falls back to the player's
/// `tutor_targets` list.
///
/// Returns `ChooseTutorTarget` actions deduplicated by CardId so MCCFR sees
/// one action per card type, not per copy.
fn tutor_target_actions(state: &GameState, player: PlayerIndex) -> Vec<Action> {
    let library = &state.players[player].library;
    let db = state.card_db();

    let subtype_filter = state
        .pending_tutor
        .as_ref()
        .map(|pt| &pt.subtype_filter[..])
        .unwrap_or(&[]);

    let mut seen = HashSet::new();
    let mut actions = Vec::new();

    if !subtype_filter.is_empty() {
        // Fetch-land style: filter by subtype
        for &obj_id in library {
            let card_id = state.objects[&obj_id].card_def_id;
            if seen.insert(card_id) {
                if let Some(def) = db.get(card_id) {
                    let matches = def.subtypes.iter().any(|st| subtype_filter.contains(st));
                    if matches {
                        actions.push(Action::ChooseTutorTarget { card_id });
                    }
                }
            }
        }
    } else {
        // Original behavior: use tutor_targets list
        let targets = &state.players[player].tutor_targets;
        for &obj_id in library {
            let card_id = state.objects[&obj_id].card_def_id;
            if targets.contains(&card_id) && seen.insert(card_id) {
                actions.push(Action::ChooseTutorTarget { card_id });
            }
        }
    }

    actions
}

/// Generate subsets of up to `max_size` elements from `items`.
fn generate_subsets(items: &[ObjectId], max_items: usize) -> Vec<Vec<ObjectId>> {
    let items = if items.len() > max_items {
        &items[..max_items]
    } else {
        items
    };

    let n = items.len();
    let mut subsets = Vec::with_capacity(1 << n);

    for mask in 0..(1u32 << n) {
        let mut subset = Vec::new();
        for (i, &item) in items.iter().enumerate() {
            if mask & (1 << i) != 0 {
                subset.push(item);
            }
        }
        subsets.push(subset);
    }
    subsets
}

/// Generate strategically distinct attacker buckets instead of the full power set.
///
/// Buckets:
/// 1. **None**: don't attack (preserve board)
/// 2. **Alpha**: attack with all eligible creatures
/// 3. **Evasion-only**: attack with only evasive creatures (flying/fear/intimidate/menace)
/// 4. **Best-1**: attack with just the highest-power creature
/// 5. **Top-half**: attack with the top ceil(n/2) creatures by power
/// 6. **Bottom-half**: attack with the bottom ceil(n/2) creatures by power
/// 7. **Safe-attackers**: attack with only vigilance creatures (they don't tap, zero risk)
///
/// Produces at most 7 distinct actions (after dedup) instead of 2^n.
fn generate_attack_buckets(eligible: &[ObjectId], state: &GameState) -> Vec<Vec<ObjectId>> {
    // Sort eligible by effective power descending, break ties by object ID for stability.
    let mut by_power: Vec<(ObjectId, i32)> = eligible
        .iter()
        .map(|&id| (id, state.effective_power(id)))
        .collect();
    by_power.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let sorted_ids: Vec<ObjectId> = by_power.iter().map(|&(id, _)| id).collect();
    let n = sorted_ids.len();

    // Collect candidate buckets, then deduplicate.
    let mut seen: HashSet<Vec<ObjectId>> = HashSet::new();
    let mut buckets: Vec<Vec<ObjectId>> = Vec::with_capacity(8);

    let add_bucket = |mut bucket: Vec<ObjectId>, seen: &mut HashSet<Vec<ObjectId>>, buckets: &mut Vec<Vec<ObjectId>>| {
        bucket.sort();
        if seen.insert(bucket.clone()) {
            buckets.push(bucket);
        }
    };

    // 1. None (empty attack)
    add_bucket(vec![], &mut seen, &mut buckets);

    // 2. Alpha (all eligible)
    add_bucket(eligible.to_vec(), &mut seen, &mut buckets);

    // 3. Evasion-only: creatures with flying, fear, intimidate, or menace
    let evasive: Vec<ObjectId> = eligible
        .iter()
        .filter(|&&id| {
            state.has_keyword(id, KeywordAbility::Flying)
                || state.has_keyword(id, KeywordAbility::Fear)
                || state.has_keyword(id, KeywordAbility::Intimidate)
                || state.has_keyword(id, KeywordAbility::Menace)
        })
        .copied()
        .collect();
    if !evasive.is_empty() {
        add_bucket(evasive, &mut seen, &mut buckets);
    }

    // 4. Best-1 (highest power creature)
    if n >= 1 {
        add_bucket(vec![sorted_ids[0]], &mut seen, &mut buckets);
    }

    // 5. Top-half by power (upper ceil(n/2))
    let half = (n + 1) / 2;
    if half > 0 && half < n {
        add_bucket(sorted_ids[..half].to_vec(), &mut seen, &mut buckets);
    }

    // 6. Bottom-half by power (lower ceil(n/2))
    let bottom_start = n - half;
    if bottom_start < n && half < n {
        add_bucket(sorted_ids[bottom_start..].to_vec(), &mut seen, &mut buckets);
    }

    // 7. Safe-attackers: vigilance creatures don't tap to attack, so attacking
    //    with them carries no defensive cost. Strategically distinct posture.
    let vigilant: Vec<ObjectId> = eligible
        .iter()
        .filter(|&&id| state.has_keyword(id, KeywordAbility::Vigilance))
        .copied()
        .collect();
    if !vigilant.is_empty() {
        add_bucket(vigilant, &mut seen, &mut buckets);
    }

    buckets
}

/// Generate strategically distinct blocking buckets instead of full enumeration.
///
/// Buckets:
/// 1. **No blocks**: take all damage, preserve creatures
/// 2. **Chump-all**: assign the smallest available blocker to each attacker (biggest first)
/// 3. **Favorable-only**: block where our creature kills theirs AND survives (biggest blocker first)
/// 4. **Block-all**: assign one blocker to each attacker we can (greedy by attacker power)
/// 5. **Trade-down**: block to trade, even if we lose our creature, when their creature dies
fn generate_block_buckets(
    blockers: &[ObjectId],
    attackers: &[ObjectId],
    state: &GameState,
) -> Vec<Vec<(ObjectId, ObjectId)>> {
    if blockers.is_empty() || attackers.is_empty() {
        return vec![vec![]];
    }

    // Pre-compute legality, power, and toughness using layer engine.
    let can_block_matrix: Vec<Vec<bool>> = blockers
        .iter()
        .map(|&b| {
            attackers.iter().map(|&a| can_block(state, b, a)).collect()
        })
        .collect();

    struct CreatureStats {
        power: i32,
        toughness: i32,
    }

    let attacker_stats: Vec<CreatureStats> = attackers
        .iter()
        .map(|&id| CreatureStats {
            power: state.effective_power(id),
            toughness: state.effective_toughness(id),
        })
        .collect();

    let blocker_stats: Vec<CreatureStats> = blockers
        .iter()
        .map(|&id| CreatureStats {
            power: state.effective_power(id),
            toughness: state.effective_toughness(id),
        })
        .collect();

    // Attackers sorted by power descending (indices into the attackers slice).
    let mut attacker_order: Vec<usize> = (0..attackers.len()).collect();
    attacker_order.sort_by(|&a, &b| attacker_stats[b].power.cmp(&attacker_stats[a].power));

    // Blockers sorted by power ascending (smallest first, for chump selection).
    let mut blocker_by_power_asc: Vec<usize> = (0..blockers.len()).collect();
    blocker_by_power_asc.sort_by(|&a, &b| blocker_stats[a].power.cmp(&blocker_stats[b].power));

    // Blockers sorted by power descending (biggest first, for favorable trades).
    let mut blocker_by_power_desc: Vec<usize> = (0..blockers.len()).collect();
    blocker_by_power_desc.sort_by(|&a, &b| blocker_stats[b].power.cmp(&blocker_stats[a].power));

    let mut seen: HashSet<Vec<(ObjectId, ObjectId)>> = HashSet::new();
    let mut results: Vec<Vec<(ObjectId, ObjectId)>> = Vec::with_capacity(6);

    let add_assignment = |mut assignment: Vec<(ObjectId, ObjectId)>, seen: &mut HashSet<Vec<(ObjectId, ObjectId)>>, results: &mut Vec<Vec<(ObjectId, ObjectId)>>| {
        assignment.sort();
        if seen.insert(assignment.clone()) {
            results.push(assignment);
        }
    };

    // 1. No blocks
    add_assignment(vec![], &mut seen, &mut results);

    // 2. Chump-all: assign the smallest available blocker to each attacker,
    //    biggest attackers first. Prevents maximum total damage.
    {
        let mut assignment = Vec::new();
        let mut used_blockers: HashSet<usize> = HashSet::new();
        for &ai in &attacker_order {
            for &bi in &blocker_by_power_asc {
                if !used_blockers.contains(&bi) && can_block_matrix[bi][ai] {
                    assignment.push((blockers[bi], attackers[ai]));
                    used_blockers.insert(bi);
                    break;
                }
            }
        }
        if !assignment.is_empty() {
            add_assignment(assignment, &mut seen, &mut results);
        }
    }

    // 3. Favorable-only: block where our creature kills theirs AND survives.
    //    Iterates biggest-blocker-first so the most capable blockers get matched
    //    to attackers they can profitably handle, rather than wasting small
    //    blockers on big attackers where they can't achieve favorable trades.
    {
        let mut assignment = Vec::new();
        let mut used_blockers: HashSet<usize> = HashSet::new();
        for &ai in &attacker_order {
            for &bi in &blocker_by_power_desc {
                if used_blockers.contains(&bi) || !can_block_matrix[bi][ai] {
                    continue;
                }
                let our_survives = attacker_stats[ai].power < blocker_stats[bi].toughness;
                let theirs_dies = blocker_stats[bi].power >= attacker_stats[ai].toughness;
                if our_survives && theirs_dies {
                    assignment.push((blockers[bi], attackers[ai]));
                    used_blockers.insert(bi);
                    break;
                }
            }
        }
        if !assignment.is_empty() {
            add_assignment(assignment, &mut seen, &mut results);
        }
    }

    // 4. Block-all: greedily assign one blocker to each attacker, biggest attackers first
    {
        let mut assignment = Vec::new();
        let mut used_blockers: HashSet<usize> = HashSet::new();
        for &ai in &attacker_order {
            // Prefer the best blocker that can kill this attacker
            let mut best_bi: Option<usize> = None;
            for (bi, _) in blockers.iter().enumerate() {
                if used_blockers.contains(&bi) || !can_block_matrix[bi][ai] {
                    continue;
                }
                let kills = blocker_stats[bi].power >= attacker_stats[ai].toughness;
                let survives = attacker_stats[ai].power < blocker_stats[bi].toughness;
                match best_bi {
                    None => best_bi = Some(bi),
                    Some(prev) => {
                        let prev_kills = blocker_stats[prev].power >= attacker_stats[ai].toughness;
                        let prev_survives = attacker_stats[ai].power < blocker_stats[prev].toughness;
                        // Prefer: kills+survives > kills > survives > any
                        let score = |k: bool, s: bool| (k as u8) * 2 + (s as u8);
                        if score(kills, survives) > score(prev_kills, prev_survives) {
                            best_bi = Some(bi);
                        }
                    }
                }
            }
            if let Some(bi) = best_bi {
                assignment.push((blockers[bi], attackers[ai]));
                used_blockers.insert(bi);
            }
        }
        if !assignment.is_empty() {
            add_assignment(assignment, &mut seen, &mut results);
        }
    }

    // 5. Trade-down: block where our creature kills theirs, even if ours dies too
    {
        let mut assignment = Vec::new();
        let mut used_blockers: HashSet<usize> = HashSet::new();
        for &ai in &attacker_order {
            for &bi in &blocker_by_power_asc {
                if used_blockers.contains(&bi) || !can_block_matrix[bi][ai] {
                    continue;
                }
                let theirs_dies = blocker_stats[bi].power >= attacker_stats[ai].toughness;
                if theirs_dies {
                    assignment.push((blockers[bi], attackers[ai]));
                    used_blockers.insert(bi);
                    break;
                }
            }
        }
        if !assignment.is_empty() {
            add_assignment(assignment, &mut seen, &mut results);
        }
    }

    results
}

/// Check if a specific blocker can legally block a specific attacker.
fn can_block(
    state: &GameState,
    blocker_id: ObjectId,
    attacker_id: ObjectId,
) -> bool {
    use crate::card::KeywordAbility;

    // Unblockable: can't be blocked at all
    if state.has_keyword(attacker_id, KeywordAbility::Unblockable) {
        return false;
    }

    // Flying: only flying/reach creatures can block flyers
    if state.has_keyword(attacker_id, KeywordAbility::Flying)
        && !state.has_keyword(blocker_id, KeywordAbility::Flying)
        && !state.has_keyword(blocker_id, KeywordAbility::Reach)
    {
        return false;
    }

    // Shadow: can only be blocked by creatures with shadow
    if state.has_keyword(attacker_id, KeywordAbility::Shadow)
        && !state.has_keyword(blocker_id, KeywordAbility::Shadow)
    {
        return false;
    }
    // Creatures without shadow can't block creatures with shadow
    if !state.has_keyword(attacker_id, KeywordAbility::Shadow)
        && state.has_keyword(blocker_id, KeywordAbility::Shadow)
    {
        return false;
    }

    // Horsemanship: can only be blocked by creatures with horsemanship
    if state.has_keyword(attacker_id, KeywordAbility::Horsemanship)
        && !state.has_keyword(blocker_id, KeywordAbility::Horsemanship)
    {
        return false;
    }

    // Skulk: can't be blocked by creatures with greater power
    if state.has_keyword(attacker_id, KeywordAbility::Skulk) {
        let attacker_power = state.effective_power(attacker_id);
        let blocker_power = state.effective_power(blocker_id);
        if blocker_power > attacker_power {
            return false;
        }
    }

    // Fear: can only be blocked by artifact creatures or black creatures
    if state.has_keyword(attacker_id, KeywordAbility::Fear) {
        let db = state.card_db();
        let blocker_inst = &state.objects[&blocker_id];
        let blocker_def = match db.get(blocker_inst.card_def_id) {
            Some(d) => d,
            None => return false,
        };
        let is_artifact = blocker_def.card_types.contains(&crate::card::CardType::Artifact);
        let is_black = blocker_def.color_identity().contains(&crate::mana::Color::Black);
        if !is_artifact && !is_black {
            return false;
        }
    }

    // Intimidate: can only be blocked by artifact creatures or creatures sharing a color
    if state.has_keyword(attacker_id, KeywordAbility::Intimidate) {
        let db = state.card_db();
        let attacker_def = match db.get(state.objects[&attacker_id].card_def_id) {
            Some(d) => d,
            None => return false,
        };
        let blocker_def = match db.get(state.objects[&blocker_id].card_def_id) {
            Some(d) => d,
            None => return false,
        };
        let is_artifact = blocker_def.card_types.contains(&crate::card::CardType::Artifact);
        let attacker_colors = attacker_def.color_identity();
        let blocker_colors = blocker_def.color_identity();
        let shares_color = attacker_colors.iter().any(|c| blocker_colors.contains(c));
        if !is_artifact && !shares_color {
            return false;
        }
    }

    true
}

/// Generate legal blocking assignments.
/// Each eligible blocker can block one attacker or not block at all.
/// Handles menace (requires 2+ blockers) and other blocking restrictions.
fn generate_blocking_assignments(
    blockers: &[ObjectId],
    attackers: &[ObjectId],
    state: &GameState,
) -> Vec<Vec<(ObjectId, ObjectId)>> {
    use crate::card::KeywordAbility;

    let mut assignments = Vec::new();

    // Always include "no blocks"
    assignments.push(vec![]);

    if blockers.is_empty() || attackers.is_empty() {
        return assignments;
    }

    // Determine which attackers have menace (using layer engine)
    let menace_attackers: Vec<bool> = attackers
        .iter()
        .map(|&id| state.has_keyword(id, KeywordAbility::Menace))
        .collect();

    // Build a legal-block matrix: which blocker can block which attacker
    let can_block_matrix: Vec<Vec<bool>> = blockers
        .iter()
        .map(|&b| {
            attackers.iter().map(|&a| can_block(state, b, a)).collect()
        })
        .collect();

    // Generate single-blocker assignments (only for non-menace attackers)
    for (bi, &blocker) in blockers.iter().enumerate() {
        for (ai, &attacker) in attackers.iter().enumerate() {
            if !can_block_matrix[bi][ai] {
                continue;
            }
            // Menace: can't be single-blocked
            if menace_attackers[ai] {
                continue;
            }
            assignments.push(vec![(blocker, attacker)]);
        }
    }

    // Generate multi-blocker assignments (important for menace and gang-blocking)
    // For each attacker, generate combinations of 2+ blockers that can each block it
    // Limit to 6 eligible blockers per attacker to keep combinatorics manageable
    for (ai, &attacker) in attackers.iter().enumerate() {
        let eligible: Vec<ObjectId> = blockers
            .iter()
            .enumerate()
            .filter(|&(bi, _)| can_block_matrix[bi][ai])
            .map(|(_, &b)| b)
            .take(6)
            .collect();

        let min_blockers = if menace_attackers[ai] { 2 } else { 2 };
        let max_blockers = eligible.len().min(4); // cap at 4 blockers per attacker

        for size in min_blockers..=max_blockers {
            for combo in combinations(&eligible, size) {
                let blocks: Vec<(ObjectId, ObjectId)> =
                    combo.into_iter().map(|b| (b, attacker)).collect();
                assignments.push(blocks);
            }
        }
    }

    // Also allow mixed blocking: one blocker on each of two different attackers
    // (important when facing multiple attackers)
    if attackers.len() >= 2 && blockers.len() >= 2 {
        for (bi1, &b1) in blockers.iter().enumerate() {
            for (ai1, &a1) in attackers.iter().enumerate() {
                if !can_block_matrix[bi1][ai1] || menace_attackers[ai1] {
                    continue;
                }
                for (bi2, &b2) in blockers.iter().enumerate() {
                    if bi2 <= bi1 {
                        continue; // avoid duplicate pairs
                    }
                    for (ai2, &a2) in attackers.iter().enumerate() {
                        if ai2 == ai1 {
                            continue; // already covered by multi-blocker above
                        }
                        if !can_block_matrix[bi2][ai2] || menace_attackers[ai2] {
                            continue;
                        }
                        assignments.push(vec![(b1, a1), (b2, a2)]);
                    }
                }
            }
        }
    }

    assignments
}

/// Generate all combinations of `k` items from `items`.
fn combinations(items: &[ObjectId], k: usize) -> Vec<Vec<ObjectId>> {
    let mut result = Vec::new();
    let mut combo = Vec::with_capacity(k);
    combinations_helper(items, k, 0, &mut combo, &mut result);
    result
}

fn combinations_helper(
    items: &[ObjectId],
    k: usize,
    start: usize,
    current: &mut Vec<ObjectId>,
    result: &mut Vec<Vec<ObjectId>>,
) {
    if current.len() == k {
        result.push(current.clone());
        return;
    }
    for i in start..items.len() {
        current.push(items[i]);
        combinations_helper(items, k, i + 1, current, result);
        current.pop();
    }
}

/// Generate all permutations of the given items.
/// Caps at 6 items (720 permutations) to avoid combinatorial explosion;
/// beyond that, returns only the original order (FIFO fallback).
fn generate_permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    if items.len() > 6 {
        // Too many permutations; fall back to single FIFO ordering
        return vec![items.to_vec()];
    }

    let mut result = Vec::new();
    let mut current = Vec::with_capacity(items.len());
    let mut used = vec![false; items.len()];
    permute_helper(items, &mut current, &mut used, &mut result);
    result
}

fn permute_helper<T: Clone>(
    items: &[T],
    current: &mut Vec<T>,
    used: &mut Vec<bool>,
    result: &mut Vec<Vec<T>>,
) {
    if current.len() == items.len() {
        result.push(current.clone());
        return;
    }
    for i in 0..items.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        current.push(items[i].clone());
        permute_helper(items, current, used, result);
        current.pop();
        used[i] = false;
    }
}
