//! Exact destination arrivals and the bounded Commander SBA decision window.
use super::*;
use crate::card::ExactObjectRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommanderArrival {
    pub designation: CommanderDesignation,
    pub object: ExactObjectRef,
    pub zone: ZoneType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommanderSbaChoice {
    pub version: u32,
    pub active_player: PlayerIndex,
    pub resume_priority: PlayerIndex,
    /// Narrow cleanup-step continuation: prior ordinary SBA activity.
    pub cleanup_resume: Option<bool>,
    /// Frozen APNAP eligibility. Answers are aligned with this vector.
    pub offers: Vec<CommanderArrival>,
    pub answers: Vec<Option<bool>>,
}

/// Public decision facts only; exact object identities remain in GameState.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicCommanderSbaChoice {
    pub active_player: PlayerIndex,
    pub chooser: Option<PlayerIndex>,
    pub resume_priority: PlayerIndex,
    pub cleanup_resume: Option<bool>,
    pub offers: Vec<(CommanderDesignation, ZoneType, Option<bool>)>,
}

impl From<&CommanderSbaChoice> for PublicCommanderSbaChoice {
    fn from(operation: &CommanderSbaChoice) -> Self {
        Self {
            active_player: operation.active_player,
            chooser: operation.chooser(),
            resume_priority: operation.resume_priority,
            cleanup_resume: operation.cleanup_resume,
            offers: operation.offers.iter().zip(&operation.answers)
                .map(|(offer, answer)| (offer.designation, offer.zone, *answer)).collect(),
        }
    }
}

impl CommanderSbaChoice {
    pub fn chooser(&self) -> Option<PlayerIndex> {
        self.offers.iter().zip(&self.answers)
            .find_map(|(offer, answer)| answer.is_none().then_some(offer.designation.owner))
    }
}

impl GameState {
    pub(crate) fn note_commander_arrival(&mut self, object: ObjectId, from: ZoneType, to: ZoneType,
        was_resident: bool) {
        if !was_resident || from == to { return; }
        let Some(designation) = self.commander_for_object(object).map(|c| c.designation) else { return; };
        self.commander_arrivals.retain(|arrival| arrival.designation != designation);
        self.consumed_commander_arrivals.retain(|arrival| arrival.designation != designation);
        if matches!(to, ZoneType::Graveyard | ZoneType::Exile) {
            if let Some(exact) = self.exact_object(object) {
                self.commander_arrivals.push(CommanderArrival { designation, object: exact, zone: to });
            }
        }
    }

    pub(crate) fn arrival_is_current(&self, arrival: &CommanderArrival) -> bool {
        self.commander_for_object(arrival.object.id).is_some_and(|c| c.designation == arrival.designation)
            && matches!(arrival.zone, ZoneType::Graveyard | ZoneType::Exile)
            && self.objects.get(&arrival.object.id).is_some_and(|inst|
                inst.owner == arrival.designation.owner && inst.zone_change_count == arrival.object.generation)
            && self.players.get(arrival.designation.owner).is_some_and(|seat| match arrival.zone {
                ZoneType::Graveyard => seat.graveyard.contains(&arrival.object.id),
                ZoneType::Exile => seat.exile.contains(&arrival.object.id),
                _ => false,
            })
    }

    pub fn validate_commander_choices(&self) -> Result<(), &'static str> {
        if self.commander_arrivals.is_empty() && self.consumed_commander_arrivals.is_empty()
            && self.pending_commander_sba.is_none() { return Ok(()); }
        if !self.is_commander_format() || self.active_player >= self.players.len() {
            return Err("invalid commander choice format or active player");
        }
        let mut seen = HashSet::new();
        for arrival in &self.commander_arrivals {
            if !seen.insert(arrival.designation) || !self.arrival_is_current(arrival) {
                return Err("invalid commander arrival");
            }
        }
        for arrival in &self.consumed_commander_arrivals {
            if !seen.insert(arrival.designation) || !self.arrival_is_current(arrival) {
                return Err("invalid consumed commander opportunity");
            }
        }
        if let Some(op) = &self.pending_commander_sba {
            if op.version != 1 || op.active_player != self.active_player
                || op.resume_priority >= self.players.len() || op.offers.is_empty()
                || op.offers.len() != op.answers.len() || !self.commander_arrivals.is_empty()
                || self.pending_copy_order.is_some() || self.trigger_placement.is_some()
                || op.cleanup_resume.is_some() && (self.phase != Phase::Cleanup || self.cleanup_discard_in_progress)
                || self.players.iter().any(|p| p.has_lost) && self.unsupported_continuing_elimination()
            { return Err("invalid commander SBA window"); }
            let mut previous_rank = 0;
            let mut unanswered_rank = None;
            for (offer, answer) in op.offers.iter().zip(&op.answers) {
                let owner = offer.designation.owner;
                if owner >= self.players.len() { return Err("invalid commander SBA owner"); }
                let rank = (owner + self.players.len() - op.active_player) % self.players.len();
                if !seen.insert(offer.designation) || !self.arrival_is_current(offer)
                    || (self.players[owner].has_lost && !self.gameplay_stopped()) || rank < previous_rank
                    || (answer.is_some() && unanswered_rank.is_some_and(|first| rank > first)) {
                    return Err("invalid commander SBA offer or answer");
                }
                previous_rank = rank;
                if answer.is_none() { unanswered_rank.get_or_insert(rank); }
            }
            if unanswered_rank.is_none() || op.chooser() != Some(self.priority_player) {
                return Err("invalid commander SBA chooser");
            }
        }
        Ok(())
    }
}
