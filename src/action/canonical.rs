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
        equipment_instance_index: usize,
        target_card_id: CardId,
        target_instance_index: usize,
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
pub fn canonicalize(action: &Action, state: &GameState) -> CanonicalAction {
    let normalized = matches!(action, Action::OrderTriggerOccurrences { .. })
        .then(|| normalize_player_retained(state, state.priority_player));
    canonicalize_with_normalization(action, state, normalized.as_ref())
}

/// Reuse one immutable decision result, preserving every action and its order.
pub fn canonicalize_actions(
    actions: &[Action], state: &GameState,
    normalized: &crate::rules::transitions::RetainedNormalization,
) -> Vec<CanonicalAction> {
    actions.iter().map(|action| canonicalize_with_normalization(action, state, Some(normalized))).collect()
}

fn canonicalize_with_normalization(
    action: &Action, state: &GameState,
    normalized: Option<&crate::rules::transitions::RetainedNormalization>,
) -> CanonicalAction {
    match action {
        Action::ChooseNextCopy { item_index } => CanonicalAction::ChooseNextCopy { item_index: *item_index },
        Action::PassPriority => CanonicalAction::PassPriority,
        Action::Concede => CanonicalAction::Concede,

        Action::PlayLand { object_id } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let hand_index = hand_instance_index(state, inst.owner, *object_id);
            CanonicalAction::PlayLand { card_id, hand_index }
        }

        Action::CastSpell { object_id, targets } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let hand_index = hand_instance_index(state, inst.owner, *object_id);
            let canonical_targets: Vec<CanonicalTarget> = targets
                .iter()
                .map(|t| canonicalize_target(t, state))
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
            let instance_index = battlefield_instance_index(state, *object_id);
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
            let instance_index = battlefield_instance_index(state, *object_id);
            let canonical_targets: Vec<CanonicalTarget> = targets
                .iter()
                .map(|t| canonicalize_target(t, state))
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
                    let idx = battlefield_instance_index(state, obj_id);
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
                    let b_idx = battlefield_instance_index(state, blocker_id);
                    let a_card = state.objects[&attacker_id].card_def_id;
                    let a_idx = battlefield_instance_index(state, attacker_id);
                    (b_card, b_idx, a_card, a_idx)
                })
                .collect();
            assignments.sort();
            CanonicalAction::DeclareBlockers { assignments }
        }

        Action::Discard { object_id } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let hand_index = hand_instance_index(state, inst.owner, *object_id);
            CanonicalAction::Discard { card_id, hand_index }
        }

        Action::OrderTriggers { ordering } => {
            let source_card_ids: Vec<(CardId, usize, usize)> = ordering
                .iter()
                .map(|&(source_id, ability_index)| {
                    let trigger = state.pending_triggers.iter()
                        .find(|t| t.source_id == source_id && t.ability_index == ability_index)
                        .expect("ordered trigger must still be pending");
                    let card_id = trigger.context.source_card_id;
                    let instance_index = pending_source_index(state, source_id, card_id);
                    (card_id, instance_index, ability_index)
                })
                .collect();
            CanonicalAction::OrderTriggers { source_card_ids }
        }

        Action::OrderTriggerOccurrences { ordering } => {
            let normalized = normalized.expect("occurrence ordering requires one coherent witness");
            let occurrences = ordering.iter().map(|&slot| {
                canonical_pending_occurrence(state, slot, normalized)
                    .expect("ordered occurrence must still be pending")
            }).collect();
            CanonicalAction::OrderTriggerOccurrences { occurrences }
        }

        Action::OrderDamageAssignment {
            attacker,
            assignment,
        } => {
            let a_card = state.objects[attacker].card_def_id;
            let a_idx = battlefield_instance_index(state, *attacker);
            let canonical_assignment: Vec<(CardId, usize, u32)> = assignment
                .iter()
                .map(|&(blocker_id, damage)| {
                    let b_card = state.objects[&blocker_id].card_def_id;
                    let b_idx = battlefield_instance_index(state, blocker_id);
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
                .map(|t| canonicalize_target(t, state))
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
                    let instance_index = battlefield_instance_index(state, source_id);
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
            let instance_index = battlefield_instance_index(state, *object_id);
            CanonicalAction::ActivateLoyalty {
                source_card_id: card_id,
                source_instance_index: instance_index,
                ability_index: *ability_index,
            }
        }

        Action::Equip { equipment_id, target_id } => {
            let eq_inst = &state.objects[equipment_id];
            let eq_card_id = eq_inst.card_def_id;
            let eq_idx = battlefield_instance_index(state, *equipment_id);
            let tgt_inst = &state.objects[target_id];
            let tgt_card_id = tgt_inst.card_def_id;
            let tgt_idx = battlefield_instance_index(state, *target_id);
            CanonicalAction::Equip {
                equipment_card_id: eq_card_id,
                equipment_instance_index: eq_idx,
                target_card_id: tgt_card_id,
                target_instance_index: tgt_idx,
            }
        }

        Action::CastFromGraveyard { object_id, targets } => {
            let inst = &state.objects[object_id];
            let card_id = inst.card_def_id;
            let graveyard_index = graveyard_instance_index(state, inst.owner, *object_id);
            let canonical_targets: Vec<CanonicalTarget> = targets
                .iter()
                .map(|t| canonicalize_target(t, state))
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
            let graveyard_index = graveyard_instance_index(state, inst.owner, *object_id);
            CanonicalAction::PlayLandFromGraveyard {
                card_id,
                graveyard_index,
            }
        }
    }
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
) -> Option<Action> {
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
            let obj_id = find_in_hand_by_index(state, player, *card_id, *hand_index)?;
            Some(Action::PlayLand { object_id: obj_id })
        }

        CanonicalAction::CastSpell { card_id, hand_index, targets } => {
            let obj_id = find_in_hand_by_index(state, player, *card_id, *hand_index)?;
            let concrete_targets: Vec<Target> = targets
                .iter()
                .filter_map(|t| resolve_target(t, state))
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
                find_on_battlefield_by_index(state, *source_card_id, *source_instance_index)?;
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
                find_on_battlefield_by_index(state, *source_card_id, *source_instance_index)?;
            let concrete_targets: Vec<Target> = targets
                .iter()
                .filter_map(|t| resolve_target(t, state))
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
                let obj_id = find_on_battlefield_by_index(state, card_id, idx)?;
                attackers.push(obj_id);
            }
            Some(Action::DeclareAttackers { attackers })
        }

        CanonicalAction::DeclareBlockers { assignments } => {
            let mut blocks = Vec::with_capacity(assignments.len());
            for &(b_card, b_idx, a_card, a_idx) in assignments {
                let blocker = find_on_battlefield_by_index(state, b_card, b_idx)?;
                let attacker = find_on_battlefield_by_index(state, a_card, a_idx)?;
                blocks.push((blocker, attacker));
            }
            Some(Action::DeclareBlockers { blocks })
        }

        CanonicalAction::Discard { card_id, hand_index } => {
            let obj_id = find_in_hand_by_index(state, player, *card_id, *hand_index)?;
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
                let target_obj_id = find_pending_source_by_index(state, card_id, instance_index);
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
            let normalized = normalize_player_retained(state, player);
            let mut ordering = Vec::with_capacity(occurrences.len());
            let mut used = vec![false; state.pending_triggers.len()];
            for wanted in occurrences {
                let slot = state.pending_triggers.iter().enumerate()
                    .find(|(slot, trigger)| !used[*slot] && trigger.controller == player
                        && canonical_pending_occurrence(state, *slot, &normalized).as_ref() == Some(wanted))
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
                find_on_battlefield_by_index(state, *attacker_card_id, *attacker_instance_index)?;
            let mut concrete: Vec<(ObjectId, u32)> = Vec::with_capacity(assignment.len());
            for &(b_card, b_idx, damage) in assignment {
                let blocker = find_on_battlefield_by_index(state, b_card, b_idx)?;
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
                .filter_map(|t| resolve_target(t, state))
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
                let obj_id = find_on_battlefield_by_index(state, card_id, instance_index)?;
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
            let obj_id = find_in_hand_by_index(state, player, *card_id, 0)?;
            Some(Action::MulliganBottomCard { object_id: obj_id })
        }

        CanonicalAction::ActivateLoyalty {
            source_card_id,
            source_instance_index,
            ability_index,
        } => {
            let obj_id =
                find_on_battlefield_by_index(state, *source_card_id, *source_instance_index)?;
            Some(Action::ActivateLoyalty {
                object_id: obj_id,
                ability_index: *ability_index,
            })
        }

        CanonicalAction::Equip {
            equipment_card_id,
            equipment_instance_index,
            target_card_id,
            target_instance_index,
        } => {
            let equipment_id = find_on_battlefield_by_index(state, *equipment_card_id, *equipment_instance_index)?;
            let target_id = find_on_battlefield_by_index(state, *target_card_id, *target_instance_index)?;
            Some(Action::Equip { equipment_id, target_id })
        }

        CanonicalAction::CastFromGraveyard {
            card_id,
            graveyard_index,
            targets,
        } => {
            let obj_id = find_in_graveyard_by_index(state, player, *card_id, *graveyard_index)?;
            let concrete_targets: Vec<Target> = targets
                .iter()
                .filter_map(|ct| resolve_target(ct, state))
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
            let obj_id = find_in_graveyard_by_index(state, player, *card_id, *graveyard_index)?;
            Some(Action::PlayLandFromGraveyard { object_id: obj_id })
        }
    }
}

/// Pending abilities retain their source identity after the source leaves.
/// Use occurrence among pending sources rather than battlefield position.
fn pending_sources_with_card(state: &GameState, card_id: CardId) -> Vec<ObjectId> {
    let mut sources = Vec::new();
    for trigger in &state.pending_triggers {
        if trigger.context.source_card_id == card_id && !sources.contains(&trigger.source_id) {
            sources.push(trigger.source_id);
        }
    }
    sources
}

fn pending_source_index(state: &GameState, source_id: ObjectId, card_id: CardId) -> usize {
    pending_sources_with_card(state, card_id).iter().position(|&id| id == source_id).unwrap_or(0)
}

fn find_pending_source_by_index(state: &GameState, card_id: CardId, index: usize) -> Option<ObjectId> {
    pending_sources_with_card(state, card_id).get(index).copied()
}

fn canonical_pending_occurrence(
    state: &GameState, slot: usize,
    normalized: &crate::rules::transitions::RetainedNormalization,
) -> Option<crate::rules::transitions::ZoneOccurrenceInfo> {
    let trigger = state.pending_triggers.get(slot)?;
    if trigger.context.zone_transition.is_some() {
        return normalized.pending_occurrences.get(slot)?.clone();
    }
    Some(crate::rules::transitions::public_occurrence_info(
        &trigger.context, trigger.ability_index, trigger.controller, None,
        None, None, None, None,
        Some(pending_source_index(state, trigger.source_id, trigger.context.source_card_id)),
    ))
}

/// Public action keys use the same player-relative projection as information
/// sets. Raw object lookups are used only after these coordinates are fixed.
pub fn normalize_player_retained(
    state: &GameState, player: PlayerIndex,
) -> crate::rules::transitions::RetainedNormalization {
    crate::info_set::InformationSet::normalize_retained_view(&state.visible_state(player))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Canonicalize a concrete `Target` to a `CanonicalTarget`.
fn canonicalize_target(target: &Target, state: &GameState) -> CanonicalTarget {
    match target {
        Target::StackEntry(id) => CanonicalTarget::StackEntry {
            stack_index: state.stack.iter().position(|entry| entry.id == *id && entry.source.is_spell()),
        },
        Target::Player(idx) => CanonicalTarget::Player(*idx),
        Target::Object(obj_id) => {
            let inst = &state.objects[obj_id];
            let card_id = inst.card_def_id;
            let controller = inst.controller;
            let instance_index = battlefield_instance_index(state, *obj_id);
            CanonicalTarget::Object {
                card_id,
                controller,
                instance_index,
            }
        }
    }
}

/// Resolve a `CanonicalTarget` back to a concrete `Target`.
fn resolve_target(target: &CanonicalTarget, state: &GameState) -> Option<Target> {
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
            )?;
            Some(Target::Object(obj_id))
        }
    }
}

/// Compute the instance index of `obj_id` among all battlefield permanents
/// sharing the same `card_def_id`. Ordered by ObjectId for determinism.
fn battlefield_instance_index(state: &GameState, obj_id: ObjectId) -> usize {
    let card_id = state.objects[&obj_id].card_def_id;
    let mut siblings: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects[&id].card_def_id == card_id)
        .collect();
    siblings.sort();
    siblings.iter().position(|&id| id == obj_id).unwrap_or(0)
}

/// Compute the instance index of `obj_id` among cards in the player's hand
/// sharing the same `card_def_id`. Ordered by ObjectId for determinism.
fn hand_instance_index(state: &GameState, player: PlayerIndex, obj_id: ObjectId) -> usize {
    let card_id = state.objects[&obj_id].card_def_id;
    let mut siblings: Vec<ObjectId> = state.players[player]
        .hand
        .iter()
        .copied()
        .filter(|&id| state.objects[&id].card_def_id == card_id)
        .collect();
    siblings.sort();
    siblings.iter().position(|&id| id == obj_id).unwrap_or(0)
}

/// Find the N-th instance (by ObjectId order) of `card_id` in a player's hand.
fn find_in_hand_by_index(
    state: &GameState,
    player: PlayerIndex,
    card_id: CardId,
    instance_index: usize,
) -> Option<ObjectId> {
    let mut matches: Vec<ObjectId> = state.players[player]
        .hand
        .iter()
        .copied()
        .filter(|&id| state.objects[&id].card_def_id == card_id)
        .collect();
    matches.sort();
    matches.get(instance_index).copied()
}

/// Find the N-th instance (by ObjectId order) of `card_id` on the battlefield.
fn find_on_battlefield_by_index(
    state: &GameState,
    card_id: CardId,
    instance_index: usize,
) -> Option<ObjectId> {
    let mut matches: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects[&id].card_def_id == card_id)
        .collect();
    matches.sort();
    matches.get(instance_index).copied()
}

/// Compute the instance index of `obj_id` among cards in the player's graveyard
/// sharing the same `card_def_id`. Ordered by ObjectId for determinism.
fn graveyard_instance_index(state: &GameState, player: PlayerIndex, obj_id: ObjectId) -> usize {
    let card_id = state.objects[&obj_id].card_def_id;
    let mut siblings: Vec<ObjectId> = state.players[player]
        .graveyard
        .iter()
        .copied()
        .filter(|&id| state.objects[&id].card_def_id == card_id)
        .collect();
    siblings.sort();
    siblings.iter().position(|&id| id == obj_id).unwrap_or(0)
}

/// Find the N-th instance of `card_id` in a player's graveyard.
fn find_in_graveyard_by_index(
    state: &GameState,
    player: PlayerIndex,
    card_id: CardId,
    instance_index: usize,
) -> Option<ObjectId> {
    let mut matches: Vec<ObjectId> = state.players[player]
        .graveyard
        .iter()
        .copied()
        .filter(|&id| state.objects[&id].card_def_id == card_id)
        .collect();
    matches.sort();
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
        .find(|&id| state.objects[&id].card_def_id == card_id)
}

/// Find the N-th instance of `card_id` controlled by `controller` on the battlefield.
fn find_on_battlefield_by_controller(
    state: &GameState,
    card_id: CardId,
    controller: PlayerIndex,
    instance_index: usize,
) -> Option<ObjectId> {
    let mut matches: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            let inst = &state.objects[&id];
            inst.card_def_id == card_id && inst.controller == controller
        })
        .collect();
    matches.sort();
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
        let canonical = canonicalize(&action, &state);
        assert_eq!(canonical, CanonicalAction::PassPriority);

        let resolved = resolve(&canonical, &state, 0).unwrap();
        assert_eq!(resolved, action);
    }

    #[test]
    fn test_canonicalize_concede() {
        let state = setup_test_state();
        let action = Action::Concede;
        let canonical = canonicalize(&action, &state);
        assert_eq!(canonical, CanonicalAction::Concede);

        let resolved = resolve(&canonical, &state, 0).unwrap();
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
        let canonical = canonicalize(&action, &state);
        let resolved = resolve(&canonical, &state, 0).unwrap();
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
        let canonical = canonicalize(&action, &state);
        let resolved = resolve(&canonical, &state, 0).unwrap();
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
        let canonical = canonicalize(&action, &state);
        let resolved = resolve(&canonical, &state, 0).unwrap();
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
        let canonical = canonicalize(&action, &state);
        let resolved = resolve(&canonical, &state, 0).unwrap();
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
        let canonical = canonicalize(&action, &state);
        let resolved = resolve(&canonical, &state, 0).unwrap();
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
            let canonical = canonicalize(action, &state);
            let resolved = resolve(&canonical, &state, 0);
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
        let normalized = InformationSet::normalize_retained_view(&view);
        let keys = canonicalize_actions(&actions, &state, &normalized);
        let info = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized);
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
