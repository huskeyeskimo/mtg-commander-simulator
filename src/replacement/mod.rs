//! Phase 1A.3 — Replacement Effect Framework
//!
//! Replacement effects modify or replace game events as they happen (CR 614).
//! When multiple replacement effects could apply to the same event, the affected
//! player (or controller of the affected object) chooses the order in which they
//! apply. This ordering decision is surfaced as `Action::ChooseReplacementOrder`
//! so MCCFR can observe and optimize it.
//!
//! # Design Constraints
//!
//! - Replacement effects are checked *before* an event happens, not after
//! - Each replacement effect can only apply once to a given event (CR 614.5)
//! - Self-replacement effects (e.g., "enters tapped") are applied first without
//!   player choice (CR 614.16a)
//! - The ordering decision only arises when multiple non-self replacement effects
//!   could apply
//!
//! # Examples
//!
//! - "If a creature would die, exile it instead" (Rest in Peace)
//! - "If you would draw a card, instead draw two and put one back" (Sylvan Library)
//! - "If damage would be dealt to you, prevent 1 of it" (damage prevention)
//! - "This creature enters the battlefield tapped" (self-replacement)

use serde::{Deserialize, Serialize};

use crate::card::ObjectId;
use crate::game::PlayerIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MovementDestination { Hand, Library, Command }
impl MovementDestination {
    pub fn zone(self) -> crate::card::ZoneType {
        match self { Self::Hand => crate::card::ZoneType::Hand, Self::Library => crate::card::ZoneType::Library,
            Self::Command => crate::card::ZoneType::Command }
    }
}

/// Identifies what kind of game event a replacement effect can modify.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReplacementEventKind {
    /// The current proposed destination of a bounded movement event.
    WouldMoveTo { destination: MovementDestination, source_generation: u32 },
    /// A permanent would enter the battlefield.
    EntersBattlefield,
    /// Damage would be dealt to a player or permanent.
    DamageDealt,
    /// A card would be drawn.
    CardDraw,
    /// A permanent would die (move from battlefield to graveyard).
    WouldDie,
    /// A player would gain life.
    LifeGain,
    /// A player would lose life.
    LifeLoss,
    /// Counters would be placed on a permanent.
    CounterPlacement,
}

/// What a replacement effect does when it applies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReplacementAction {
    /// Instead of the original event, do nothing (prevent).
    Prevent,
    /// Redirect the event to a different zone (e.g., exile instead of graveyard).
    RedirectToZone(crate::card::ZoneType),
    /// Modify the amount (e.g., prevent 1 damage, gain extra life).
    ModifyAmount { delta: i32 },
    /// The permanent enters with a modification (e.g., enters tapped,
    /// enters with counters).
    EntersModified {
        enters_tapped: bool,
        extra_counters: i32,
    },
    /// Custom replacement — deferred to card-specific logic.
    /// The string identifies which card/effect implements the replacement.
    Custom(String),
}

/// A replacement effect definition, registered from a card's static ability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacementEffect {
    /// The permanent that generates this replacement effect.
    pub source_id: ObjectId,
    /// The player who controls the source.
    pub controller: PlayerIndex,
    /// What kind of event this effect can replace.
    pub applies_to: ReplacementEventKind,
    /// What the replacement does.
    pub action: ReplacementAction,
    /// Whether this is a self-replacement effect (CR 614.16a).
    /// Self-replacement effects are applied first without player choice.
    pub is_self_replacement: bool,
    /// Human-readable description for debugging and display.
    pub description: String,
}

/// A pending replacement decision: which replacement effects apply to
/// a specific event, requiring the affected player to choose the order.
#[derive(Debug, Clone)]
pub struct PendingReplacementChoice {
    /// The player who must choose the application order.
    pub chooser: PlayerIndex,
    /// The replacement effects that could apply (indices into a registry
    /// or direct references). Each entry is (source_id, effect_index).
    pub applicable_effects: Vec<(ObjectId, usize)>,
    /// The event being replaced (for context).
    pub event_kind: ReplacementEventKind,
}

/// Check which replacement effects apply to a given event kind.
///
/// Returns the list of applicable effects. Self-replacement effects
/// are separated from player-choice effects.
pub fn find_applicable_replacements(
    effects: &[ReplacementEffect],
    event_kind: &ReplacementEventKind,
    _affected_player: PlayerIndex,
) -> (Vec<usize>, Vec<usize>) {
    let mut self_replacements = Vec::new();
    let mut player_choice = Vec::new();

    for (i, effect) in effects.iter().enumerate() {
        if effect.applies_to == *event_kind {
            if effect.is_self_replacement {
                self_replacements.push(i);
            } else {
                player_choice.push(i);
            }
        }
    }

    (self_replacements, player_choice)
}

/// Apply replacement effects for a given event, following CR 614 ordering rules.
///
/// 1. Self-replacement effects (CR 614.16a) are applied first automatically
/// 2. If multiple non-self replacement effects apply, the affected player chooses
///    the order — returns a `PendingReplacementChoice` so the game can surface
///    an `Action::ChooseReplacementOrder`
/// 3. If only one non-self effect applies, it's applied automatically
///
/// Returns `Some(PendingReplacementChoice)` if player input is needed,
/// `None` if all effects were applied automatically.
pub fn apply_replacement_effects<'a>(
    effects: &'a [ReplacementEffect],
    event_kind: &ReplacementEventKind,
    affected_player: PlayerIndex,
) -> (Vec<&'a ReplacementEffect>, Option<PendingReplacementChoice>) {
    let (self_indices, player_indices) =
        find_applicable_replacements(effects, event_kind, affected_player);

    // Phase 1: Auto-apply self-replacement effects
    let auto_applied: Vec<&ReplacementEffect> = self_indices
        .iter()
        .map(|&i| &effects[i])
        .collect();

    // Phase 2: Handle player-choice effects
    if player_indices.len() <= 1 {
        // 0 or 1 player-choice effect — apply automatically
        let mut result = auto_applied;
        for &i in &player_indices {
            result.push(&effects[i]);
        }
        (result, None)
    } else {
        // Multiple player-choice effects — player must choose order
        let applicable = player_indices
            .iter()
            .map(|&i| (effects[i].source_id, i))
            .collect();
        let choice = PendingReplacementChoice {
            chooser: affected_player,
            applicable_effects: applicable,
            event_kind: event_kind.clone(),
        };
        (auto_applied, Some(choice))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::ZoneType;

    #[test]
    fn test_find_applicable_replacements_empty() {
        let effects: Vec<ReplacementEffect> = vec![];
        let (self_r, player_r) =
            find_applicable_replacements(&effects, &ReplacementEventKind::WouldDie, 0);
        assert!(self_r.is_empty());
        assert!(player_r.is_empty());
    }

    #[test]
    fn test_find_applicable_replacements_mixed() {
        let effects = vec![
            ReplacementEffect {
                source_id: 1,
                controller: 0,
                applies_to: ReplacementEventKind::WouldDie,
                action: ReplacementAction::RedirectToZone(ZoneType::Exile),
                is_self_replacement: false,
                description: "Exile instead of dying".into(),
            },
            ReplacementEffect {
                source_id: 2,
                controller: 0,
                applies_to: ReplacementEventKind::EntersBattlefield,
                action: ReplacementAction::EntersModified {
                    enters_tapped: true,
                    extra_counters: 0,
                },
                is_self_replacement: true,
                description: "Enters tapped".into(),
            },
            ReplacementEffect {
                source_id: 3,
                controller: 0,
                applies_to: ReplacementEventKind::WouldDie,
                action: ReplacementAction::Prevent,
                is_self_replacement: false,
                description: "Prevent death".into(),
            },
        ];

        // Check WouldDie: should find effects 0 and 2 as player-choice
        let (self_r, player_r) =
            find_applicable_replacements(&effects, &ReplacementEventKind::WouldDie, 0);
        assert!(self_r.is_empty());
        assert_eq!(player_r, vec![0, 2]);

        // Check EntersBattlefield: should find effect 1 as self-replacement
        let (self_r, player_r) =
            find_applicable_replacements(&effects, &ReplacementEventKind::EntersBattlefield, 0);
        assert_eq!(self_r, vec![1]);
        assert!(player_r.is_empty());

        // Check DamageDealt: nothing applies
        let (self_r, player_r) =
            find_applicable_replacements(&effects, &ReplacementEventKind::DamageDealt, 0);
        assert!(self_r.is_empty());
        assert!(player_r.is_empty());
    }

    #[test]
    fn test_replacement_effect_serde_roundtrip() {
        let effect = ReplacementEffect {
            source_id: 42,
            controller: 1,
            applies_to: ReplacementEventKind::DamageDealt,
            action: ReplacementAction::ModifyAmount { delta: -1 },
            is_self_replacement: false,
            description: "Prevent 1 damage".into(),
        };

        let json = serde_json::to_string(&effect).unwrap();
        let deserialized: ReplacementEffect = serde_json::from_str(&json).unwrap();
        assert_eq!(effect, deserialized);
    }
}

/// Public predicate facts exclude incarnation counters, represented by exact source edges.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PublicReplacementPredicate { Movement(MovementDestination), Other(ReplacementEventKind) }
pub(crate) fn public_predicate(predicate: &ReplacementEventKind) -> PublicReplacementPredicate {
    match predicate { ReplacementEventKind::WouldMoveTo { destination, .. } => PublicReplacementPredicate::Movement(*destination),
        other => PublicReplacementPredicate::Other(other.clone()) }
}
