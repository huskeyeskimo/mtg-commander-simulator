//! Phase 0.2 — Canonical Action Mapping
//!
//! Stable action identifiers independent of ObjectId assignment.
//! Two game states in the same information set map concrete `Action`s
//! to the same `CanonicalAction`, enabling MCCFR regret tables to
//! index actions by *what* they do rather than by ephemeral ObjectIds.
//!
//! # Round-trip invariant
//!
//! For every legal action `a` in a state `s` with priority player `p`:
//! ```text
//! resolve(&canonicalize(&a, &s), &s, p) == Some(a)
//! ```

use serde::{Deserialize, Serialize};

use crate::card::{CardId, ObjectId};
use crate::game::{GameState, PlayerIndex, Target};

use super::Action;

/// A target expressed in terms of card template IDs and player indices,
/// rather than ephemeral ObjectIds. This makes it stable across different
/// game instances that share the same information set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CanonicalTarget {
    /// Target a player by index.
    Player(PlayerIndex),
    /// Target a permanent/object identified by its card template ID and
    /// a disambiguation index (0 = first instance found on the battlefield
    /// owned by that controller, 1 = second, etc.).
    Object {
        card_id: CardId,
        controller: PlayerIndex,
        instance_index: usize,
    },
    /// Bottom-based position in the public ordered stack. None denotes a stale
    /// runtime target and must never resolve to a different spell.
    StackEntry { stack_index: Option<usize> },
}

/// Stable action identifier independent of ObjectId assignment.
///
/// Two game states in the same information set map concrete `Action`s
/// to the same `CanonicalAction`. MCCFR regret tables key on this type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CanonicalAction {
    ChooseNextCopy { item_index: usize },
    PassPriority,

    PlayLand {
        card_id: CardId,
        /// Disambiguation: which instance of this card in hand (sorted by ObjectId).
        hand_index: usize,
    },

    CastSpell {
        card_id: CardId,
        /// Disambiguation: which instance of this card in hand (sorted by ObjectId).
        hand_index: usize,
        targets: Vec<CanonicalTarget>,
    },

    ActivateManaAbility {
        source_card_id: CardId,
        /// Disambiguation: which instance of this card on the battlefield.
        source_instance_index: usize,
        ability_index: usize,
    },

    ActivateAbility {
        source_card_id: CardId,
        source_instance_index: usize,
        ability_index: usize,
        targets: Vec<CanonicalTarget>,
    },

    /// Attacker card IDs, sorted for stable comparison.
    DeclareAttackers {
        attacker_card_ids: Vec<(CardId, usize)>,
    },

    /// Blocking assignments as (blocker_card_id, blocker_instance, attacker_card_id, attacker_instance), sorted.
    DeclareBlockers {
        assignments: Vec<(CardId, usize, CardId, usize)>,
    },

    Discard {
        card_id: CardId,
        /// Disambiguation: which instance of this card in hand (sorted by ObjectId).
        hand_index: usize,
    },

    OrderTriggers {
        /// Trigger sources as (card_id, instance_index, ability_index), in the chosen order.
        /// The instance_index disambiguates multiple copies of the same card on the battlefield.
        source_card_ids: Vec<(CardId, usize, usize)>,
    },

    OrderDamageAssignment {
        attacker_card_id: CardId,
        attacker_instance_index: usize,
        /// (blocker_card_id, blocker_instance_index, damage_amount)
        assignment: Vec<(CardId, usize, u32)>,
    },

    /// Choose the order to apply replacement effects (CR 614).
    /// Each entry is (source_card_id, source_instance_index, effect_index).
    ChooseReplacementOrder {
        source_card_ids: Vec<(CardId, usize, usize)>,
    },

    /// Cast commander from command zone (Commander format).
    CastCommander {
        card_id: CardId,
        targets: Vec<CanonicalTarget>,
    },

    /// Keep the current hand (London Mulligan).
    MulliganKeep,

    /// Mulligan: shuffle hand into library and draw 7 new cards.
    MulliganMulligan,

    /// Put a card on the bottom of the library after keeping a mulliganed hand.
    /// Uses card_id only (no hand_index) because which *copy* to bottom is
    /// strategically irrelevant — the decision is "bottom this card or not".
    MulliganBottomCard {
        card_id: CardId,
    },

    /// Choose a card from library during tutor resolution.
    /// Already keyed by CardId — no disambiguation needed.
    ChooseTutorTarget { card_id: CardId },

    Concede,

    /// Activate a loyalty ability on a planeswalker.
    ActivateLoyalty {
        source_card_id: CardId,
        source_instance_index: usize,
        ability_index: usize,
    },

    /// Equip an equipment to a target creature.
    Equip {
        equipment_card_id: CardId,
        equipment_coordinate: crate::public_projection::SemanticCoordinate,
        target_card_id: CardId,
        target_coordinate: crate::public_projection::SemanticCoordinate,
    },

    /// Cast a spell from the graveyard (Flashback, Escape).
    CastFromGraveyard {
        card_id: CardId,
        /// Disambiguation: which instance of this card in the graveyard.
        graveyard_index: usize,
        targets: Vec<CanonicalTarget>,
    },

    /// Activate a pre-defined combo as a single macro-action.
    /// Combo ID is stable across game states (it's a registry index, not
    /// dependent on ObjectIds), so this is already canonical.
    ActivateMacro { combo_id: usize },

    /// End the turn, fast-forwarding through remaining phases.
    EndTurn,

    /// Play a land from the graveyard (Crucible of Worlds, etc.).
    PlayLandFromGraveyard {
        card_id: CardId,
        graveyard_index: usize,
    },

    /// Public source/subject facts and rank among semantic duplicates.
    /// Appended to preserve existing canonical-action discriminants.
    OrderTriggerOccurrences {
        occurrences: Vec<crate::rules::transitions::ZoneOccurrenceInfo>,
    },
}

/// Convert a concrete `Action` (with ObjectIds) into a `CanonicalAction`
/// (with CardIds) given the current game state for lookups.
///
/// # Timing contract
///
/// Must be called **before** `apply_action()`. The action's ObjectIds must
/// still reside in their pre-action zones (hand for PlayLand/CastSpell/Discard,
/// battlefield for ActivateAbility/DeclareAttackers/etc.). Calling this after
/// the action has already moved objects to different zones will produce
/// incorrect instance indices or panic on missing ObjectIds.
pub fn canonicalize(action: &Action, state: &GameState) -> Result<CanonicalAction, crate::simulation::TerminationReason> {
    let normalized = normalize_player_retained(state, state.priority_player)?;
    canonicalize_with_normalization(action, state, &normalized)
}

/// Reuse one immutable decision result, preserving every action and its order.
pub fn canonicalize_actions(
    actions: &[Action], state: &GameState,
    normalized: &crate::public_projection::JointPublicNormalization,
) -> Result<Vec<CanonicalAction>, crate::simulation::TerminationReason> {
    actions.iter().map(|action| canonicalize_with_normalization(action, state, normalized)).collect()
}

fn canonicalize_with_normalization(
    action: &Action, state: &GameState,
    normalized: &crate::public_projection::JointPublicNormalization,
) -> Result<CanonicalAction, crate::simulation::TerminationReason> {
    validate_action_references(action, state, normalized)?;
    Ok(match action {
        Action::ChooseNextCopy { item_index } => CanonicalAction::ChooseNextCopy { item_index: *item_index },
        Action::PassPriority => CanonicalAction::PassPriority,
        Action::Concede => CanonicalAction::Concede,

        Action::PlayLand { object_id } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let hand_index = hand_instance_index(state, inst.owner, *object_id, normalized);
            CanonicalAction::PlayLand { card_id, hand_index }
        }

        Action::CastSpell { object_id, targets } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let hand_index = hand_instance_index(state, inst.owner, *object_id, normalized);
            let canonical_targets: Vec<CanonicalTarget> = targets
                .iter()
                .map(|t| canonicalize_target(t, state, normalized))
                .collect();
            CanonicalAction::CastSpell {
                card_id,
                hand_index,
                targets: canonical_targets,
            }
        }

        Action::ActivateManaAbility {
            object_id,
            ability_index,
        } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let instance_index = battlefield_instance_index(state, *object_id, normalized);
            CanonicalAction::ActivateManaAbility {
                source_card_id: card_id,
                source_instance_index: instance_index,
                ability_index: *ability_index,
            }
        }

        Action::ActivateAbility {
            object_id,
            ability_index,
            targets,
        } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let instance_index = battlefield_instance_index(state, *object_id, normalized);
            let canonical_targets: Vec<CanonicalTarget> = targets
                .iter()
                .map(|t| canonicalize_target(t, state, normalized))
                .collect();
            CanonicalAction::ActivateAbility {
                source_card_id: card_id,
                source_instance_index: instance_index,
                ability_index: *ability_index,
                targets: canonical_targets,
            }
        }

        Action::DeclareAttackers { attackers } => {
            let mut attacker_entries: Vec<(CardId, usize)> = attackers
                .iter()
                .map(|&obj_id| {
                    let card_id = state.objects[&obj_id].card_def_id;
                    let idx = battlefield_instance_index(state, obj_id, normalized);
                    (card_id, idx)
                })
                .collect();
            attacker_entries.sort();
            CanonicalAction::DeclareAttackers {
                attacker_card_ids: attacker_entries,
            }
        }

        Action::DeclareBlockers { blocks } => {
            let mut assignments: Vec<(CardId, usize, CardId, usize)> = blocks
                .iter()
                .map(|&(blocker_id, attacker_id)| {
                    let b_card = state.objects[&blocker_id].card_def_id;
                    let b_idx = battlefield_instance_index(state, blocker_id, normalized);
                    let a_card = state.objects[&attacker_id].card_def_id;
                    let a_idx = battlefield_instance_index(state, attacker_id, normalized);
                    (b_card, b_idx, a_card, a_idx)
                })
                .collect();
            assignments.sort();
            CanonicalAction::DeclareBlockers { assignments }
        }

        Action::Discard { object_id } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let hand_index = hand_instance_index(state, inst.owner, *object_id, normalized);
            CanonicalAction::Discard { card_id, hand_index }
        }

        Action::OrderTriggers { ordering } => {
            let source_card_ids: Vec<(CardId, usize, usize)> = ordering
                .iter()
                .map(|&(source_id, ability_index)| {
                    let trigger = state.pending_triggers.iter()
                        .find(|t| t.source_id == source_id && t.ability_index == ability_index)
                        .ok_or(crate::simulation::TerminationReason::StateEncoding)?;
                    let card_id = trigger.context.source_card_id;
                    let instance_index = pending_source_index(state, source_id, card_id, normalized);
                    Ok((card_id, instance_index, ability_index))
                })
                .collect::<Result<Vec<_>, crate::simulation::TerminationReason>>()?;
            CanonicalAction::OrderTriggers { source_card_ids }
        }

        Action::OrderTriggerOccurrences { ordering } => {
            let retained = &normalized.retained;
            let occurrences = ordering.iter().map(|&slot| {
                canonical_pending_occurrence(state, slot, retained)
                    .ok_or(crate::simulation::TerminationReason::StateEncoding)
            }).collect::<Result<Vec<_>, _>>()?;
            CanonicalAction::OrderTriggerOccurrences { occurrences }
        }

        Action::OrderDamageAssignment {
            attacker,
            assignment,
        } => {
            let a_card = state.objects[attacker].card_def_id;
            let a_idx = battlefield_instance_index(state, *attacker, normalized);
            let canonical_assignment: Vec<(CardId, usize, u32)> = assignment
                .iter()
                .map(|&(blocker_id, damage)| {
                    let b_card = state.objects[&blocker_id].card_def_id;
                    let b_idx = battlefield_instance_index(state, blocker_id, normalized);
                    (b_card, b_idx, damage)
                })
                .collect();
            CanonicalAction::OrderDamageAssignment {
                attacker_card_id: a_card,
                attacker_instance_index: a_idx,
                assignment: canonical_assignment,
            }
        }

        Action::CastCommander { object_id, targets } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let canonical_targets: Vec<CanonicalTarget> = targets
                .iter()
                .map(|t| canonicalize_target(t, state, normalized))
                .collect();
            CanonicalAction::CastCommander {
                card_id,
                targets: canonical_targets,
            }
        }

        Action::ChooseReplacementOrder { ordering } => {
            let source_card_ids: Vec<(CardId, usize, usize)> = ordering
                .iter()
                .map(|&(source_id, effect_index)| {
                    let card_id = state.objects[&source_id].card_def_id;
                    let instance_index = battlefield_instance_index(state, source_id, normalized);
                    (card_id, instance_index, effect_index)
                })
                .collect();
            CanonicalAction::ChooseReplacementOrder { source_card_ids }
        }

        Action::ChooseTutorTarget { card_id } => {
            CanonicalAction::ChooseTutorTarget { card_id: *card_id }
        }

        Action::MulliganKeep => CanonicalAction::MulliganKeep,
        Action::MulliganMulligan => CanonicalAction::MulliganMulligan,
        Action::MulliganBottomCard { object_id } => {
            let inst = &state.objects[object_id];
            CanonicalAction::MulliganBottomCard { card_id: inst.card_def_id }
        }

        Action::ActivateLoyalty { object_id, ability_index } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let instance_index = battlefield_instance_index(state, *object_id, normalized);
            CanonicalAction::ActivateLoyalty {
                source_card_id: card_id,
                source_instance_index: instance_index,
                ability_index: *ability_index,
            }
        }

        Action::Equip { equipment_id, target_id } => {
            let eq_inst = &state.objects[equipment_id];
            let eq_card_id = eq_inst.card_def_id;
            let equipment_coordinate = coordinate(state, normalized, *equipment_id)?;
            let tgt_inst = &state.objects[target_id];
            let tgt_card_id = tgt_inst.card_def_id;
            let target_coordinate = coordinate(state, normalized, *target_id)?;
            CanonicalAction::Equip {
                equipment_card_id: eq_card_id,
                equipment_coordinate,
                target_card_id: tgt_card_id,
                target_coordinate,
            }
        }

        Action::CastFromGraveyard { object_id, targets } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let graveyard_index = graveyard_instance_index(state, inst.owner, *object_id, normalized);
            let canonical_targets: Vec<CanonicalTarget> = targets
                .iter()
                .map(|t| canonicalize_target(t, state, normalized))
                .collect();
            CanonicalAction::CastFromGraveyard {
                card_id,
                graveyard_index,
                targets: canonical_targets,
            }
        }

        Action::ActivateMacro { combo_id } => {
            CanonicalAction::ActivateMacro { combo_id: *combo_id }
        }

        Action::EndTurn => CanonicalAction::EndTurn,

        Action::PlayLandFromGraveyard { object_id } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let graveyard_index = graveyard_instance_index(state, inst.owner, *object_id, normalized);
            CanonicalAction::PlayLandFromGraveyard {
                card_id,
                graveyard_index,
            }
        }
    })
}

/// Convert a `CanonicalAction` back into a concrete `Action` given the
/// current game state. Returns `None` if the mapping is ambiguous or the
/// referenced objects cannot be found.
///
/// # Timing contract
///
/// Same as `canonicalize()`: the game state must reflect the **pre-action**
/// position. Objects referenced by the canonical action must still reside
/// in their expected zones (hand, battlefield, pending triggers, etc.).
pub fn resolve(
    canonical: &CanonicalAction,
    state: &GameState,
    player: PlayerIndex,
) -> Result<Option<Action>, crate::simulation::TerminationReason> {
    let normalized = normalize_player_retained(state, player)?;
    resolve_with_normalization(canonical, state, player, &normalized)
}

pub fn resolve_with_normalization(
    canonical: &CanonicalAction,
    state: &GameState,
    player: PlayerIndex,
    normalized: &crate::public_projection::JointPublicNormalization,
) -> Result<Option<Action>, crate::simulation::TerminationReason> {
    Ok((|| -> Option<Action> {
    match canonical {
        CanonicalAction::ChooseNextCopy { item_index } => {
            let pending = state.pending_copy_order.as_ref()?;
            (player == pending.controller() && *item_index < pending.items().len()
                && !pending.selected_order().contains(item_index))
                .then_some(Action::ChooseNextCopy { item_index: *item_index })
        }
        CanonicalAction::PassPriority => Some(Action::PassPriority),
        CanonicalAction::Concede => Some(Action::Concede),

        CanonicalAction::PlayLand { card_id, hand_index } => {
            let obj_id = find_in_hand_by_index(state, player, *card_id, *hand_index, normalized)?;
            Some(Action::PlayLand { object_id: obj_id })
        }

        CanonicalAction::CastSpell { card_id, hand_index, targets } => {
            let obj_id = find_in_hand_by_index(state, player, *card_id, *hand_index, normalized)?;
            let concrete_targets: Vec<Target> = targets
                .iter()
                .filter_map(|t| resolve_target(t, state, normalized))
                .collect();
            if concrete_targets.len() != targets.len() {
                return None;
            }
            Some(Action::CastSpell {
                object_id: obj_id,
                targets: concrete_targets,
            })
        }

        CanonicalAction::ActivateManaAbility {
            source_card_id,
            source_instance_index,
            ability_index,
        } => {
            let obj_id =
                find_on_battlefield_by_index(state, *source_card_id, *source_instance_index, normalized)?;
            Some(Action::ActivateManaAbility {
                object_id: obj_id,
                ability_index: *ability_index,
            })
        }

        CanonicalAction::ActivateAbility {
            source_card_id,
            source_instance_index,
            ability_index,
            targets,
        } => {
            let obj_id =
                find_on_battlefield_by_index(state, *source_card_id, *source_instance_index, normalized)?;
            let concrete_targets: Vec<Target> = targets
                .iter()
                .filter_map(|t| resolve_target(t, state, normalized))
                .collect();
            if concrete_targets.len() != targets.len() {
                return None;
            }
            Some(Action::ActivateAbility {
                object_id: obj_id,
                ability_index: *ability_index,
                targets: concrete_targets,
            })
        }

        CanonicalAction::DeclareAttackers { attacker_card_ids } => {
            let mut attackers = Vec::with_capacity(attacker_card_ids.len());
            for &(card_id, idx) in attacker_card_ids {
                let obj_id = find_on_battlefield_by_index(state, card_id, idx, normalized)?;
                attackers.push(obj_id);
            }
            Some(Action::DeclareAttackers { attackers })
        }

        CanonicalAction::DeclareBlockers { assignments } => {
            let mut blocks = Vec::with_capacity(assignments.len());
            for &(b_card, b_idx, a_card, a_idx) in assignments {
                let blocker = find_on_battlefield_by_index(state, b_card, b_idx, normalized)?;
                let attacker = find_on_battlefield_by_index(state, a_card, a_idx, normalized)?;
                blocks.push((blocker, attacker));
            }
            Some(Action::DeclareBlockers { blocks })
        }

        CanonicalAction::Discard { card_id, hand_index } => {
            let obj_id = find_in_hand_by_index(state, player, *card_id, *hand_index, normalized)?;
            Some(Action::Discard { object_id: obj_id })
        }

        CanonicalAction::OrderTriggers { source_card_ids } => {
            // Rebuild the ordering from pending triggers. Each (card_id, instance_index, ability_index)
            // must match a pending trigger owned by this player.
            let player_triggers: Vec<(ObjectId, usize)> = state
                .pending_triggers
                .iter()
                .filter(|t| t.controller == player)
                .map(|t| (t.source_id, t.ability_index))
                .collect();

            let mut ordering = Vec::with_capacity(source_card_ids.len());
            let mut used: Vec<bool> = vec![false; player_triggers.len()];

            for &(card_id, instance_index, ability_index) in source_card_ids {
                let target_obj_id = find_pending_source_by_index(state, card_id, instance_index, normalized);
                let mut found = false;
                for (i, &(src_id, ab_idx)) in player_triggers.iter().enumerate() {
                    if !used[i]
                        && ab_idx == ability_index
                        && target_obj_id == Some(src_id)
                    {
                        ordering.push((src_id, ab_idx));
                        used[i] = true;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return None;
                }
            }
            Some(Action::OrderTriggers { ordering })
        }

        CanonicalAction::OrderTriggerOccurrences { occurrences } => {
            let retained = &normalized.retained;
            let mut ordering = Vec::with_capacity(occurrences.len());
            let mut used = vec![false; state.pending_triggers.len()];
            for wanted in occurrences {
                let slot = state.pending_triggers.iter().enumerate()
                    .find(|(slot, trigger)| !used[*slot] && trigger.controller == player
                        && canonical_pending_occurrence(state, *slot, retained).as_ref() == Some(wanted))
                    .map(|(slot, _)| slot)?;
                used[slot] = true;
                ordering.push(slot);
            }
            Some(Action::OrderTriggerOccurrences { ordering })
        }

        CanonicalAction::OrderDamageAssignment {
            attacker_card_id,
            attacker_instance_index,
            assignment,
        } => {
            let attacker =
                find_on_battlefield_by_index(state, *attacker_card_id, *attacker_instance_index, normalized)?;
            let mut concrete: Vec<(ObjectId, u32)> = Vec::with_capacity(assignment.len());
            for &(b_card, b_idx, damage) in assignment {
                let blocker = find_on_battlefield_by_index(state, b_card, b_idx, normalized)?;
                concrete.push((blocker, damage));
            }
            Some(Action::OrderDamageAssignment {
                attacker,
                assignment: concrete,
            })
        }

        CanonicalAction::CastCommander { card_id, targets } => {
            // Find the commander in the player's command zone
            let obj_id = find_in_command_zone(state, player, *card_id)?;
            let concrete_targets: Vec<Target> = targets
                .iter()
                .filter_map(|t| resolve_target(t, state, normalized))
                .collect();
            if concrete_targets.len() != targets.len() {
                return None;
            }
            Some(Action::CastCommander {
                object_id: obj_id,
                targets: concrete_targets,
            })
        }

        CanonicalAction::ChooseReplacementOrder { source_card_ids } => {
            let mut ordering = Vec::with_capacity(source_card_ids.len());
            for &(card_id, instance_index, effect_index) in source_card_ids {
                let obj_id = find_on_battlefield_by_index(state, card_id, instance_index, normalized)?;
                ordering.push((obj_id, effect_index));
            }
            Some(Action::ChooseReplacementOrder { ordering })
        }

        CanonicalAction::ChooseTutorTarget { card_id } => {
            Some(Action::ChooseTutorTarget { card_id: *card_id })
        }

        CanonicalAction::MulliganKeep => Some(Action::MulliganKeep),
        CanonicalAction::MulliganMulligan => Some(Action::MulliganMulligan),
        CanonicalAction::MulliganBottomCard { card_id } => {
            // Match first instance — strategically equivalent for duplicates
            let obj_id = find_in_hand_by_index(state, player, *card_id, 0, normalized)?;
            Some(Action::MulliganBottomCard { object_id: obj_id })
        }

        CanonicalAction::ActivateLoyalty {
            source_card_id,
            source_instance_index,
            ability_index,
        } => {
            let obj_id =
                find_on_battlefield_by_index(state, *source_card_id, *source_instance_index, normalized)?;
            Some(Action::ActivateLoyalty {
                object_id: obj_id,
                ability_index: *ability_index,
            })
        }

        CanonicalAction::Equip {
            equipment_card_id,
            equipment_coordinate,
            target_card_id,
            target_coordinate,
        } => {
            let equipment_id = resolve_coordinate(state, normalized, *equipment_card_id, *equipment_coordinate)?;
            let target_id = resolve_coordinate(state, normalized, *target_card_id, *target_coordinate)?;
            Some(Action::Equip { equipment_id, target_id })
        }

        CanonicalAction::CastFromGraveyard {
            card_id,
            graveyard_index,
            targets,
        } => {
            let obj_id = find_in_graveyard_by_index(state, player, *card_id, *graveyard_index, normalized)?;
            let concrete_targets: Vec<Target> = targets
                .iter()
                .filter_map(|ct| resolve_target(ct, state, normalized))
                .collect();
            if concrete_targets.len() != targets.len() {
                return None;
            }
            Some(Action::CastFromGraveyard {
                object_id: obj_id,
                targets: concrete_targets,
            })
        }

        CanonicalAction::ActivateMacro { combo_id } => {
            Some(Action::ActivateMacro { combo_id: *combo_id })
        }

        CanonicalAction::EndTurn => Some(Action::EndTurn),

        CanonicalAction::PlayLandFromGraveyard {
            card_id,
            graveyard_index,
        } => {
            let obj_id = find_in_graveyard_by_index(state, player, *card_id, *graveyard_index, normalized)?;
            Some(Action::PlayLandFromGraveyard { object_id: obj_id })
        }
    }

    })())
}

/// Pending abilities retain their source identity after the source leaves.
/// Use occurrence among pending sources rather than battlefield position.
fn pending_sources_with_card(state: &GameState, card_id: CardId, normalized: &crate::public_projection::JointPublicNormalization) -> Vec<ObjectId> {
    let mut sources = Vec::new();
    for trigger in &state.pending_triggers {
        if trigger.context.source_card_id == card_id && !sources.contains(&trigger.source_id) {
            sources.push(trigger.source_id);
        }
    }
    // Preserve the accepted >6 own-state FIFO boundary. Bounded ordering
    // decisions use the source identity from this same immutable witness.
    if state.pending_triggers.len() <= 6 {
        sources.sort_by_key(|id| state.pending_triggers.iter().find(|trigger| trigger.source_id == *id && trigger.context.source_card_id == card_id)
            .and_then(|trigger| normalized.exact_to_coordinate.get(&crate::card::ExactObjectRef { id: *id, generation: trigger.context.source_generation })).copied());
    }
    sources
}

fn pending_source_index(state: &GameState, source_id: ObjectId, card_id: CardId, normalized: &crate::public_projection::JointPublicNormalization) -> usize {
    pending_sources_with_card(state, card_id, normalized).iter().position(|&id| id == source_id).unwrap_or(0)
}

fn find_pending_source_by_index(state: &GameState, card_id: CardId, index: usize, normalized: &crate::public_projection::JointPublicNormalization) -> Option<ObjectId> {
    pending_sources_with_card(state, card_id, normalized).get(index).copied()
}

fn canonical_pending_occurrence(
    state: &GameState, slot: usize,
    normalized: &crate::rules::transitions::RetainedNormalization,
) -> Option<crate::rules::transitions::ZoneOccurrenceInfo> {
    let _ = state.pending_triggers.get(slot)?;
    normalized.pending_occurrences.get(slot)?.clone()
}

/// Public action keys use the same player-relative projection as information
/// sets. Raw object lookups are used only after these coordinates are fixed.
pub fn normalize_player_retained<'a>(state: &'a GameState, player: PlayerIndex) -> Result<crate::public_projection::JointPublicNormalization<'a>, crate::simulation::TerminationReason> {
    crate::public_projection::JointPublicNormalization::for_state(state, player)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Canonicalize a concrete `Target` to a `CanonicalTarget`.
fn canonicalize_target(target: &Target, state: &GameState, normalized: &crate::public_projection::JointPublicNormalization) -> CanonicalTarget {
    match target {
        Target::StackEntry(id) => CanonicalTarget::StackEntry {
            stack_index: state.stack.iter().position(|entry| entry.id == *id && entry.source.is_spell()),
        },
        Target::Player(idx) => CanonicalTarget::Player(*idx),
        Target::Object(obj_id) => {
            let inst = &state.objects[obj_id];
            let card_id = inst.card_def_id;
            let controller = inst.controller;
            let mut siblings: Vec<_> = state.battlefield.iter().copied().filter(|id| {
                let object = &state.objects[id]; object.card_def_id == card_id && object.controller == controller
            }).collect();
            siblings.sort_by_key(|id| state.exact_object(*id).and_then(|exact| normalized.exact_to_coordinate.get(&exact)).copied());
            let instance_index = siblings.iter().position(|id| id == obj_id).unwrap_or(0);
            CanonicalTarget::Object {
                card_id,
                controller,
                instance_index,
            }
        }
    }
}

/// Resolve a `CanonicalTarget` back to a concrete `Target`.
fn resolve_target(target: &CanonicalTarget, state: &GameState, normalized: &crate::public_projection::JointPublicNormalization) -> Option<Target> {
    match target {
        CanonicalTarget::StackEntry { stack_index } => {
            let entry = state.stack.get((*stack_index)?)?;
            entry.source.is_spell().then_some(Target::StackEntry(entry.id))
        }
        CanonicalTarget::Player(idx) => Some(Target::Player(*idx)),
        CanonicalTarget::Object {
            card_id,
            controller,
            instance_index,
        } => {
            let obj_id = find_on_battlefield_by_controller(
                state,
                *card_id,
                *controller,
                *instance_index,
                normalized)?;
            Some(Target::Object(obj_id))
        }
    }
}

/// Compute the instance index of `obj_id` among all battlefield permanents
/// sharing the same `card_def_id`. Ordered by the immutable public decision witness.
fn battlefield_instance_index(state: &GameState, obj_id: ObjectId, normalized: &crate::public_projection::JointPublicNormalization) -> usize {
    let card_id = state.objects[&obj_id].card_def_id;
    let mut siblings: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects[&id].card_def_id == card_id)
        .collect();
    siblings.sort_by_key(|id| state.exact_object(*id).and_then(|exact| normalized.exact_to_coordinate.get(&exact)).copied());
    siblings.iter().position(|&id| id == obj_id).unwrap_or(0)
}

/// Compute the instance index of `obj_id` among cards in the player's hand
/// sharing the same `card_def_id`. Ordered by the immutable public decision witness.
fn hand_instance_index(state: &GameState, player: PlayerIndex, obj_id: ObjectId, normalized: &crate::public_projection::JointPublicNormalization) -> usize {
    let card_id = state.objects[&obj_id].card_def_id;
    let mut siblings: Vec<ObjectId> = state.players[player]
        .hand
        .iter()
        .copied()
        .filter(|&id| state.is_card(id) && state.objects[&id].card_def_id == card_id)
        .collect();
    siblings.sort_by_key(|id| state.exact_object(*id).and_then(|exact| normalized.exact_to_coordinate.get(&exact)).copied());
    siblings.iter().position(|&id| id == obj_id).unwrap_or(0)
}

/// Find the N-th instance (by witness order) of `card_id` in a player's hand.
fn find_in_hand_by_index(
    state: &GameState,
    player: PlayerIndex,
    card_id: CardId,
    instance_index: usize, normalized: &crate::public_projection::JointPublicNormalization) -> Option<ObjectId> {
    let mut matches: Vec<ObjectId> = state.players[player]
        .hand
        .iter()
        .copied()
        .filter(|&id| state.is_card(id) && state.objects[&id].card_def_id == card_id)
        .collect();
    matches.sort_by_key(|id| state.exact_object(*id).and_then(|exact| normalized.exact_to_coordinate.get(&exact)).copied());
    matches.get(instance_index).copied()
}

/// Find the N-th instance (by witness order) of `card_id` on the battlefield.
fn find_on_battlefield_by_index(
    state: &GameState,
    card_id: CardId,
    instance_index: usize,
    normalized: &crate::public_projection::JointPublicNormalization) -> Option<ObjectId> {
    let mut matches: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects[&id].card_def_id == card_id)
        .collect();
    matches.sort_by_key(|id| state.exact_object(*id).and_then(|exact| normalized.exact_to_coordinate.get(&exact)).copied());
    matches.get(instance_index).copied()
}

/// Compute the instance index of `obj_id` among cards in the player's graveyard
/// sharing the same `card_def_id`. Ordered by the immutable public decision witness.
fn graveyard_instance_index(state: &GameState, player: PlayerIndex, obj_id: ObjectId, normalized: &crate::public_projection::JointPublicNormalization) -> usize {
    let card_id = state.objects[&obj_id].card_def_id;
    let mut siblings: Vec<ObjectId> = state.players[player]
        .graveyard
        .iter()
        .copied()
        .filter(|&id| state.is_card(id) && state.objects[&id].card_def_id == card_id)
        .collect();
    siblings.sort_by_key(|id| state.exact_object(*id).and_then(|exact| normalized.exact_to_coordinate.get(&exact)).copied());
    siblings.iter().position(|&id| id == obj_id).unwrap_or(0)
}

/// Find the N-th instance of `card_id` in a player's graveyard.
fn find_in_graveyard_by_index(
    state: &GameState,
    player: PlayerIndex,
    card_id: CardId,
    instance_index: usize, normalized: &crate::public_projection::JointPublicNormalization) -> Option<ObjectId> {
    let mut matches: Vec<ObjectId> = state.players[player]
        .graveyard
        .iter()
        .copied()
        .filter(|&id| state.is_card(id) && state.objects[&id].card_def_id == card_id)
        .collect();
    matches.sort_by_key(|id| state.exact_object(*id).and_then(|exact| normalized.exact_to_coordinate.get(&exact)).copied());
    matches.get(instance_index).copied()
}

/// Find a card in a player's command zone by card_id.
fn find_in_command_zone(
    state: &GameState,
    player: PlayerIndex,
    card_id: CardId,
) -> Option<ObjectId> {
    state.players[player]
        .command_zone
        .iter()
        .copied()
        .find(|&id| state.is_card(id) && state.objects[&id].card_def_id == card_id)
}

/// Find the N-th instance of `card_id` controlled by `controller` on the battlefield.
fn find_on_battlefield_by_controller(
    state: &GameState,
    card_id: CardId,
    controller: PlayerIndex,
    instance_index: usize,
    normalized: &crate::public_projection::JointPublicNormalization) -> Option<ObjectId> {
    let mut matches: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            let inst = &state.objects[&id];
            inst.card_def_id == card_id && inst.controller == controller
        })
        .collect();
    matches.sort_by_key(|id| state.exact_object(*id).and_then(|exact| normalized.exact_to_coordinate.get(&exact)).copied());
    matches.get(instance_index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::legal_actions;
    use crate::card::sample;
    use crate::card::ZoneType;
    use crate::game::Phase;
    use std::sync::Arc;

    /// Helper: create a basic game state with cards for testing.
    fn setup_test_state() -> GameState {
        let db = sample::build_sample_db();
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));

        // Libraries so no one decks out
        for _ in 0..20 {
            state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Library);
            state.create_card_in_zone(sample::ids::FOREST, 1, ZoneType::Library);
        }

        state
    }

    #[test]
    fn test_canonicalize_pass_priority() {
        let state = setup_test_state();
        let action = Action::PassPriority;
        let canonical = canonicalize(&action, &state).unwrap();
        assert_eq!(canonical, CanonicalAction::PassPriority);

        let resolved = resolve(&canonical, &state, 0).unwrap().unwrap();
        assert_eq!(resolved, action);
    }

    #[test]
    fn test_canonicalize_concede() {
        let state = setup_test_state();
        let action = Action::Concede;
        let canonical = canonicalize(&action, &state).unwrap();
        assert_eq!(canonical, CanonicalAction::Concede);

        let resolved = resolve(&canonical, &state, 0).unwrap().unwrap();
        assert_eq!(resolved, action);
    }

    #[test]
    fn test_canonicalize_play_land_roundtrip() {
        let mut state = setup_test_state();
        let land_id =
            state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand);

        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::PreCombatMain;

        let action = Action::PlayLand { object_id: land_id };
        let canonical = canonicalize(&action, &state).unwrap();
        let resolved = resolve(&canonical, &state, 0).unwrap().unwrap();
        assert_eq!(resolved, action);
    }

    #[test]
    fn test_canonicalize_cast_spell_roundtrip() {
        let mut state = setup_test_state();

        // Put a Lightning Bolt in hand with a Mountain on the battlefield
        let bolt_id =
            state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
        let _mountain =
            state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);

        // Put a creature on the battlefield as a target
        let bear_id =
            state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 1, ZoneType::Battlefield);

        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::PreCombatMain;

        let action = Action::CastSpell {
            object_id: bolt_id,
            targets: vec![Target::Object(bear_id)],
        };
        let canonical = canonicalize(&action, &state).unwrap();
        let resolved = resolve(&canonical, &state, 0).unwrap().unwrap();
        assert_eq!(resolved, action);
    }

    #[test]
    fn test_canonicalize_cast_spell_targeting_player_roundtrip() {
        let mut state = setup_test_state();
        let bolt_id =
            state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);
        let _mountain =
            state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);

        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::PreCombatMain;

        let action = Action::CastSpell {
            object_id: bolt_id,
            targets: vec![Target::Player(1)],
        };
        let canonical = canonicalize(&action, &state).unwrap();
        let resolved = resolve(&canonical, &state, 0).unwrap().unwrap();
        assert_eq!(resolved, action);
    }

    #[test]
    fn test_canonicalize_discard_roundtrip() {
        let mut state = setup_test_state();
        let card_id =
            state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand);

        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::Cleanup;

        let action = Action::Discard {
            object_id: card_id,
        };
        let canonical = canonicalize(&action, &state).unwrap();
        let resolved = resolve(&canonical, &state, 0).unwrap().unwrap();
        assert_eq!(resolved, action);
    }

    #[test]
    fn test_canonicalize_declare_attackers_roundtrip() {
        let mut state = setup_test_state();

        let bear1 =
            state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Battlefield);
        let bear2 =
            state.create_card_in_zone(sample::ids::GREY_OGRE, 0, ZoneType::Battlefield);
        for &id in &[bear1, bear2] {
            if let Some(inst) = state.objects.get_mut(&id) {
                inst.tapped = false;
                inst.summoning_sick = false;
            }
        }

        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::DeclareAttackers;

        let action = Action::DeclareAttackers {
            attackers: vec![bear1, bear2],
        };
        let canonical = canonicalize(&action, &state).unwrap();
        let resolved = resolve(&canonical, &state, 0).unwrap().unwrap();
        // Attackers may be reordered due to sorting in canonical form,
        // but the sets should be equal
        if let Action::DeclareAttackers { attackers: resolved_attackers } = &resolved {
            let mut expected = vec![bear1, bear2];
            let mut actual = resolved_attackers.clone();
            expected.sort();
            actual.sort();
            assert_eq!(expected, actual);
        } else {
            panic!("Expected DeclareAttackers");
        }
    }

    #[test]
    fn test_canonicalize_all_legal_actions_roundtrip() {
        // The core acceptance criterion: canonicalize round-trips for ALL legal actions.
        let mut state = setup_test_state();

        // Set up a rich game state with various possible actions
        // Player 0: hand with land, creature, bolt; battlefield with lands
        state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Hand);
        state.create_card_in_zone(sample::ids::GREY_OGRE, 0, ZoneType::Hand);
        state.create_card_in_zone(sample::ids::LIGHTNING_BOLT, 0, ZoneType::Hand);

        let f1 = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
        let f2 = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
        let f3 = state.create_card_in_zone(sample::ids::MOUNTAIN, 0, ZoneType::Battlefield);
        for &id in &[f1, f2, f3] {
            if let Some(inst) = state.objects.get_mut(&id) {
                inst.tapped = false;
                inst.summoning_sick = false;
            }
        }

        // Player 1: a creature on the battlefield (target for bolt)
        let _target = state.create_card_in_zone(
            sample::ids::GRIZZLY_BEARS,
            1,
            ZoneType::Battlefield,
        );

        state.active_player = 0;
        state.priority_player = 0;
        state.phase = Phase::PreCombatMain;
        state.turn_number = 2;

        let actions = legal_actions(&state);
        assert!(!actions.is_empty(), "Should have legal actions");

        for action in &actions {
            let canonical = canonicalize(action, &state).unwrap();
            let resolved = resolve(&canonical, &state, 0).unwrap();
            assert!(
                resolved.is_some(),
                "Failed to resolve canonical action {:?} (from {:?})",
                canonical, action
            );
            let resolved = resolved.unwrap();
            // For most actions, direct equality holds. For DeclareAttackers/DeclareBlockers,
            // the order may differ but set equality should hold.
            match (&resolved, action) {
                (
                    Action::DeclareAttackers { attackers: a },
                    Action::DeclareAttackers { attackers: b },
                ) => {
                    let mut a_sorted = a.clone();
                    let mut b_sorted = b.clone();
                    a_sorted.sort();
                    b_sorted.sort();
                    assert_eq!(a_sorted, b_sorted,
                        "DeclareAttackers mismatch: resolved {:?} vs original {:?}", a, b);
                }
                _ => {
                    assert_eq!(
                        resolved, *action,
                        "Round-trip failed for action {:?}",
                        action
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod retained_normalization_reuse_tests {
    use super::*;
    use crate::card::{CardDef, CardType, Effect, TriggerCondition, TriggeredAbility, ZoneType};
    use crate::game::{CardDatabase, Phase};
    use crate::info_set::InformationSet;
    use crate::rules::transitions::{transition_batch, ExactObjectRef, MovementKind, TransitionRequest,
        NORMALIZATION_CALLS};
    #[test]
    fn seven_hundred_twenty_actions_and_observation_normalize_once() {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 999_290, card_types: vec![CardType::Creature],
            power: Some(2), toughness: Some(2), triggered_abilities: vec![TriggeredAbility {
                trigger: TriggerCondition::LeavesBattlefield, effect: Effect::GainLife { amount: 1 },
                description: String::new(),
            }], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(std::sync::Arc::new(db));
        state.phase = Phase::PreCombatMain;
        let requests: Vec<_> = (0..6).map(|_| {
            let id = state.create_card_in_zone(999_290, 0, ZoneType::Battlefield);
            TransitionRequest { object: ExactObjectRef { id, generation: 0 },
                from: ZoneType::Battlefield, to: ZoneType::Exile, kind: MovementKind::Put }
        }).collect();
        transition_batch(&mut state, &requests).unwrap();
        crate::rules::check_state_based_actions(&mut state);
        let actions: Vec<_> = crate::action::legal_actions(&state).into_iter()
            .filter(|a| matches!(a, Action::OrderTriggerOccurrences { .. })).collect();
        assert_eq!(actions.len(), 720);
        let view = state.visible_state(0);
        NORMALIZATION_CALLS.with(|calls| calls.set(0));
        let normalized = InformationSet::normalize_retained_view(&view).unwrap();
        let keys = canonicalize_actions(&actions, &state, &normalized).unwrap();
        let info = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized).unwrap();
        NORMALIZATION_CALLS.with(|calls| assert_eq!(calls.get(), 1));
        assert_eq!(keys.len(), 720);
        assert_eq!(keys.iter().collect::<std::collections::HashSet<_>>().len(), 720);
        assert_eq!(info.zone_normalization, normalized.encoding);
        println!("720 actions + information set: calls=1, nodes={}, rounds={}, cells={:?}, components={:?}, encoding_bytes={}, elapsed_ns={}",
            normalized.stats.search_nodes, normalized.stats.refinement_rounds,
            normalized.stats.tied_cell_sizes, normalized.stats.component_sizes,
            normalized.encoding.len(), normalized.stats.elapsed_nanos);
    }
}

fn coordinate(state: &GameState, normalized: &crate::public_projection::JointPublicNormalization, id: ObjectId)
    -> Result<crate::public_projection::SemanticCoordinate, crate::simulation::TerminationReason> {
    let exact = state.exact_object(id).ok_or(crate::simulation::TerminationReason::StateEncoding)?;
    normalized.exact_to_coordinate.get(&exact).copied().ok_or(crate::simulation::TerminationReason::StateEncoding)
}
fn resolve_coordinate(state: &GameState, normalized: &crate::public_projection::JointPublicNormalization,
    definition: CardId, coordinate: crate::public_projection::SemanticCoordinate) -> Option<ObjectId> {
    let exact = normalized.coordinate_to_exact.get(&coordinate)?;
    let object = state.objects.get(&exact.id)?;
    (state.battlefield.contains(&exact.id) && object.zone_change_count == exact.generation
        && object.card_def_id == definition).then_some(exact.id)
}
fn validate_action_references(action: &Action, state: &GameState, normalized: &crate::public_projection::JointPublicNormalization) -> Result<(), crate::simulation::TerminationReason> {
    // Serialize only this finite action enum to inventory its explicit runtime references.
    // Each branch is handled below; no arbitrary GameState traversal occurs.
    let objects: Vec<ObjectId> = match action {
        Action::PlayLand { object_id } | Action::Discard { object_id } |
        Action::MulliganBottomCard { object_id } | Action::PlayLandFromGraveyard { object_id } => vec![*object_id],
        Action::CastSpell { object_id, .. } | Action::CastCommander { object_id, .. } |
        Action::ActivateAbility { object_id, .. } | Action::ActivateManaAbility { object_id, .. } |
        Action::ActivateLoyalty { object_id, .. } | Action::CastFromGraveyard { object_id, .. } => vec![*object_id],
        Action::Equip { equipment_id, target_id } => vec![*equipment_id, *target_id],
        Action::DeclareAttackers { attackers } => attackers.clone(),
        Action::DeclareBlockers { blocks } => blocks.iter().flat_map(|&(a,b)| [a,b]).collect(),
        Action::OrderDamageAssignment { attacker, assignment } => std::iter::once(*attacker).chain(assignment.iter().map(|(id,_)| *id)).collect(),
        Action::ChooseReplacementOrder { ordering } => ordering.iter().map(|(id,_)| *id).collect(),
        _ => vec![],
    };
    let targets = match action {
        Action::CastSpell { targets, .. } | Action::CastCommander { targets, .. } | Action::ActivateAbility { targets, .. } | Action::CastFromGraveyard { targets, .. } => targets.as_slice(), _ => &[],
    };
    if objects.iter().chain(targets.iter().filter_map(|target| if let Target::Object(id) = target { Some(id) } else { None })).any(|id| !normalized.current_ids.contains(id)) {
        return Err(crate::simulation::TerminationReason::StateEncoding);
    }
    if let Action::Equip { equipment_id, target_id } = action {
        if !state.battlefield.contains(equipment_id) || !state.battlefield.contains(target_id) { return Err(crate::simulation::TerminationReason::StateEncoding); }
    }
    Ok(())
}
