//! Owned bounded movement operation. Proposals and decisions are not zone changes.
use serde::{Deserialize, Serialize};
use crate::{card::{Effect, ExactObjectRef, ZoneType}, game::{CommanderDesignation, GameState, PlayerIndex, StackEntry, Target},
    replacement::{ReplacementAction, ReplacementEventKind}, rules::transitions::{MovementKind, TransitionRequest}};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MovementReplacementCandidate {
    Commander(CommanderDesignation),
    Ordinary { source: ExactObjectRef, controller: PlayerIndex, predicate: ReplacementEventKind,
        action: ReplacementAction, self_replacement: bool, occurrence: usize },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LibraryPlacement { Top, Bottom, Shuffle }
/// Closed Slice A instruction language. No arbitrary suspended Effect is owned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundedMovementInstruction {
    Bounce { zone: ZoneType, target: crate::card::TargetSpec },
    BounceOpponents, Shuffle { target: crate::card::TargetSpec }, Bottom { target: crate::card::TargetSpec },
    GraveTop { target: crate::card::TargetSpec }, GraveHand { target: crate::card::TargetSpec },
    LinkedHand, Destroy { target: crate::card::TargetSpec }, DestroyAll,
    Sacrifice { count: u32, target: crate::card::TargetSpec }, OpponentSacrifice { count: u32 },
    GainLife { amount: u32 }, LoseLife { amount: u32, target: crate::card::TargetSpec },
    AddMana { color: Option<crate::mana::Color>, amount: u32 }, Untap { target: crate::card::TargetSpec },
    ChitteringHandTop,
    Draw { count: u32 },
    ExpediteHaste,
}
impl BoundedMovementInstruction {
    pub(crate) fn from_effect(effect: &Effect) -> Option<Self> {
        Some(match effect {
            Effect::ChitteringRats => Self::ChitteringHandTop,
            Effect::BounceTo { zone, target } => Self::Bounce { zone: *zone, target: target.clone() },
            Effect::BounceAllNonlandOpponents => Self::BounceOpponents,
            Effect::ShuffleIntoLibrary { target } => Self::Shuffle { target: target.clone() },
            Effect::PutOnBottomOfLibrary { target } => Self::Bottom { target: target.clone() },
            Effect::ReturnToTopOfLibrary { target } => Self::GraveTop { target: target.clone() },
            Effect::ReturnFromGraveyardToHand { target } => Self::GraveHand { target: target.clone() },
            Effect::ReturnLinkedExileToHand => Self::LinkedHand,
            Effect::DestroyTarget { target } => Self::Destroy { target: target.clone() },
            Effect::DestroyAll => Self::DestroyAll,
            Effect::SacrificeCreatures { count, target } => Self::Sacrifice { count: *count, target: target.clone() },
            Effect::EachOpponentSacrifices { count } => Self::OpponentSacrifice { count: *count },
            Effect::GainLife { amount } => Self::GainLife { amount: *amount },
            Effect::LoseLife { amount, target } => Self::LoseLife { amount: *amount, target: target.clone() },
            Effect::AddMana { color, amount } => Self::AddMana { color: *color, amount: *amount },
            Effect::UntapTarget { target } => Self::Untap { target: target.clone() },
            Effect::DrawCards { count } => Self::Draw { count: *count },
            Effect::GainKeywordUntilEOT { keyword: crate::card::KeywordAbility::Haste,
                target: crate::card::TargetSpec::AnyCreature } => Self::ExpediteHaste,
            _ => return None,
        })
    }
    pub(crate) fn effect(&self) -> Effect {
        match self {
            Self::ChitteringHandTop => Effect::ChitteringRats,
            Self::Bounce { zone, target } => Effect::BounceTo { zone: *zone, target: target.clone() },
            Self::BounceOpponents => Effect::BounceAllNonlandOpponents,
            Self::Shuffle { target } => Effect::ShuffleIntoLibrary { target: target.clone() },
            Self::Bottom { target } => Effect::PutOnBottomOfLibrary { target: target.clone() },
            Self::GraveTop { target } => Effect::ReturnToTopOfLibrary { target: target.clone() },
            Self::GraveHand { target } => Effect::ReturnFromGraveyardToHand { target: target.clone() },
            Self::LinkedHand => Effect::ReturnLinkedExileToHand,
            Self::Destroy { target } => Effect::DestroyTarget { target: target.clone() },
            Self::DestroyAll => Effect::DestroyAll,
            Self::Sacrifice { count, target } => Effect::SacrificeCreatures { count: *count, target: target.clone() },
            Self::OpponentSacrifice { count } => Effect::EachOpponentSacrifices { count: *count },
            Self::GainLife { amount } => Effect::GainLife { amount: *amount },
            Self::LoseLife { amount, target } => Effect::LoseLife { amount: *amount, target: target.clone() },
            Self::AddMana { color, amount } => Effect::AddMana { color: *color, amount: *amount },
            Self::Untap { target } => Effect::UntapTarget { target: target.clone() },
            Self::Draw { count } => Effect::DrawCards { count: *count },
            Self::ExpediteHaste => Effect::GainKeywordUntilEOT {
                keyword: crate::card::KeywordAbility::Haste, target: crate::card::TargetSpec::AnyCreature },
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovementProposal {
    pub request: TransitionRequest,
    pub owner: PlayerIndex,
    pub affected_controller: PlayerIndex,
    pub commander: Option<CommanderDesignation>,
    pub is_creature: bool,
    pub is_token: bool,
    pub canceled: bool,
    pub placement: LibraryPlacement,
    pub proposed: ZoneType,
    pub consumed: Vec<MovementReplacementCandidate>,
    /// A decline applies to this proposal revision only, not the event lineage.
    pub declined_revision: Option<usize>,
    pub revision: usize,
    pub commander_selected: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MovementDecision { Select(MovementReplacementCandidate), Commander(bool), LibraryNext }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovementDecisionRecord {
    pub object: ExactObjectRef,
    pub chooser: PlayerIndex,
    pub automatic: bool,
    pub decision: MovementDecision,
    pub resulting_destination: ZoneType,
    pub resulting_revision: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMovement {
    pub window: u64,
    pub version: u32,
    pub active_player: PlayerIndex,
    pub instruction_controller: PlayerIndex,
    pub resume_priority: PlayerIndex,
    pub proposals: Vec<MovementProposal>,
    /// Complete pre-event catalog, including currently inapplicable occurrences.
    pub catalog: Vec<MovementReplacementCandidate>,
    pub history: Vec<MovementDecisionRecord>,
    pub library_order: Vec<ExactObjectRef>,
    /// Instruction obligation survives redirection and now-impossible movers (701.24c).
    pub shuffle_owners: Vec<PlayerIndex>,
    pub instruction: BoundedMovementInstruction,
    pub envelope: Vec<BoundedMovementInstruction>,
    pub instruction_index: usize,
    pub targets: Vec<Target>,
    pub target_generations: Vec<Option<u32>>,
    pub source: Option<crate::card::ObjectId>,
    pub resolving_entry: Option<Box<StackEntry>>,
    pub resolving_source: Option<ExactObjectRef>,
    pub resolution_owned: bool,
    /// Number of individual draws left after the proposed current draw.
    pub draws_remaining: u32,
    /// Drawable library cards at the start of this Draw instruction.
    pub draw_start_library_count: usize,
    /// A draw-step action has no resolving stack entry but still owes settlement.
    pub turn_draw: bool,
    /// Original trigger-placement deferral for a draw caller.
    pub draw_resume_deferred: bool,
}
impl PendingMovement {
    pub fn candidates(&self, p: &MovementProposal) -> Vec<MovementReplacementCandidate> {
        if p.commander_selected || p.canceled { return Vec::new(); }
        let mut candidates: Vec<_> = self.catalog.iter().filter(|candidate| match candidate {
            MovementReplacementCandidate::Ordinary { predicate, .. } => !p.consumed.contains(candidate)
                && match predicate {
                    ReplacementEventKind::WouldMoveTo { destination, .. } => p.proposed == destination.zone(),
                    ReplacementEventKind::WouldDie => p.request.from == ZoneType::Battlefield
                        && p.is_creature && p.proposed == ZoneType::Graveyard
                        && matches!(p.request.kind, MovementKind::Destroy | MovementKind::Sacrifice { .. }),
                    _ => false,
                },
            _ => false,
        }).cloned().collect();
        if candidates.iter().any(|candidate| matches!(candidate,
            MovementReplacementCandidate::Ordinary { self_replacement: true, .. })) {
            candidates.retain(|candidate| matches!(candidate,
                MovementReplacementCandidate::Ordinary { self_replacement: true, .. }));
        } else if p.declined_revision != Some(p.revision)
            && matches!(p.proposed, ZoneType::Hand | ZoneType::Library) {
            if let Some(commander) = p.commander { candidates.push(MovementReplacementCandidate::Commander(commander)); }
        }
        candidates
    }
    pub fn replacement_chooser(p: &MovementProposal) -> PlayerIndex {
        if p.request.from == ZoneType::Battlefield { p.affected_controller } else { p.owner }
    }
    pub fn chooser(&self, players: usize) -> Option<PlayerIndex> {
        let mut choosers: Vec<_> = self.proposals.iter().filter_map(|p| {
            if p.commander_selected { Some(p.owner) }
            else if !self.candidates(p).is_empty() { Some(Self::replacement_chooser(p)) } else { None }
        }).collect();
        if choosers.is_empty() {
            for p in &self.proposals {
                if !p.is_token && !p.canceled && p.proposed == ZoneType::Library && p.placement != LibraryPlacement::Shuffle
                    && !self.library_order.contains(&p.request.object)
                    && self.proposals.iter().filter(|other| other.owner == p.owner
                        && !other.is_token && !other.canceled && other.proposed == ZoneType::Library && other.placement == p.placement).count() > 1 {
                    choosers.push(p.owner);
                }
            }
        }
        choosers.into_iter().min_by_key(|&player| (player + players - self.active_player) % players)
    }
}
impl GameState {
    /// Wire structure can be validated without the shared database. A resumed
    /// decision must bind definitions and recompute all reached subject facts.
    pub fn validate_movement_play(&self) -> Result<(), &'static str> {
        if self.pending_movement.is_some() && self.card_db.is_none() {
            return Err("movement database must be attached before play");
        }
        self.validate_movement_choices()
    }
    pub fn validate_movement_choices(&self) -> Result<(), &'static str> {
        for effect in &self.replacement_effects {
            if let ReplacementEventKind::WouldMoveTo { source_generation, .. } = effect.applies_to {
                if !self.battlefield.contains(&effect.source_id)
                    || self.exact_object(effect.source_id) != Some(ExactObjectRef { id: effect.source_id, generation: source_generation })
                    || effect.controller >= self.players.len() { return Err("invalid exact movement replacement source"); }
            }
        }
        let Some(op) = &self.pending_movement else { return Ok(()); };
        crate::rules::movement_choice::validate_operation(self, op)
    }
}

/// Owns only the first, private selection stage of the closed ETB resolution.
/// Handoff transfers its original public trigger entry to PendingMovement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingChittering {
    pub version: u32,
    pub opponent: PlayerIndex,
    pub resume_priority: PlayerIndex,
    pub resolving_entry: Box<StackEntry>,
}
