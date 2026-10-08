use rand::seq::SliceRandom;

use crate::action::canonical::canonicalize_actions;
use crate::action::{legal_actions, legal_actions_abstracted, Action};
use crate::card::{Effect, KeywordAbility, ObjectId};
use crate::game::{GameState, PlayerIndex, Target};
use crate::info_set::InformationSet;
use crate::solver::{sample_from_distribution, RegretTable};

/// A strategy decides what action to take given a game state.
/// This is the interface the GTO solver will optimize over.
pub trait Strategy: Send + Sync {
    fn choose_action(&self, state: &GameState, player: PlayerIndex) -> Result<Action, crate::simulation::TerminationReason>;
    fn name(&self) -> &str;
}

/// Random strategy: picks uniformly at random from legal actions.
/// Useful as a baseline and for Monte Carlo rollouts.
pub struct RandomStrategy;

impl Strategy for RandomStrategy {
    fn choose_action(&self, state: &GameState, _player: PlayerIndex) -> Result<Action, crate::simulation::TerminationReason> {
        Ok((|| -> Action {
        let mut rng = rand::thread_rng();
        let actions = legal_actions(state);
        actions
            .choose(&mut rng)
            .cloned()
            .unwrap_or(Action::PassPriority)

        })())
    }

    fn name(&self) -> &str {
        "Random"
    }
}

/// Greedy heuristic strategy: plays lands, casts biggest spell possible,
/// attacks with everything, blocks favorably.
/// Better than random, serves as a reasonable default opponent.
pub struct GreedyStrategy;

/// Direction of an effect's value to its selected target. Composite effects
/// with conflicting directions stay neutral; unrelated children are ignored.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetValue {
    Harmful,
    Helpful,
    Neutral,
    Mixed,
}

fn effect_target_value(effect: &Effect) -> TargetValue {
    use TargetValue::*;
    if let Effect::Multiple(children) = effect {
        return children.iter().fold(Neutral, |value, child| {
            let next = effect_target_value(child);
            match (value, next) {
                (Mixed, _) | (_, Mixed) => Mixed,
                (Neutral, other) => other,
                (other, Neutral) => other,
                (left, right) if left == right => left,
                _ => Mixed,
            }
        });
    }

    // Only effects that actually use the spell's selected target contribute.
    if !matches!(crate::targeting::effect_targeting(effect),
        crate::targeting::SpellTargeting::Single(_)) {
        return Neutral;
    }

    match effect {
        Effect::DealDamage { .. }
        | Effect::DealDynamicDamage { .. }
        | Effect::LoseLife { .. }
        | Effect::LoseDynamicLife { .. }
        | Effect::DestroyTarget { .. }
        | Effect::ExileTarget { .. }
        | Effect::BounceTo { .. }
        | Effect::Counter { .. }
        | Effect::GainControlUntilEOT { .. }
        | Effect::TapTarget { .. }
        | Effect::DiscardCards { .. }
        | Effect::SacrificeCreatures { .. } => Harmful,
        Effect::Buff { power, toughness, .. } => {
            if *power >= 0 && *toughness >= 0 && (*power > 0 || *toughness > 0) {
                Helpful
            } else if *power <= 0 && *toughness <= 0 && (*power < 0 || *toughness < 0) {
                Harmful
            } else {
                Neutral
            }
        }
        Effect::Debuff { power, toughness, .. } if *power > 0 || *toughness > 0 => Harmful,
        Effect::PutCounters { count, .. } if *count > 0 => Helpful,
        Effect::PutCounters { count, .. } if *count < 0 => Harmful,
        Effect::DoublePowerUntilEOT { .. } | Effect::UntapTarget { .. } => Helpful,
        Effect::GainKeywordUntilEOT { keyword, .. } if matches!(keyword,
            KeywordAbility::Haste | KeywordAbility::Flying | KeywordAbility::Vigilance
            | KeywordAbility::Trample | KeywordAbility::Indestructible
            | KeywordAbility::Hexproof) => Helpful,
        _ => Neutral,
    }
}

fn spell_target_value(state: &GameState, controller: PlayerIndex, effect: &Effect, targets: &[Target]) -> i32 {
    let direction = match effect_target_value(effect) {
        TargetValue::Helpful => 1,
        TargetValue::Harmful => -1,
        TargetValue::Neutral | TargetValue::Mixed => return 0,
    };
    let target_controller = match targets {
        [Target::Player(player)] => Some(*player),
        [Target::Object(id)] if state.battlefield.contains(id) => {
            state.objects.get(id).map(|object| object.controller)
        }
        [Target::StackEntry(id)] => state.stack.iter().find(|entry| entry.id == *id)
            .map(|entry| entry.controller),
        _ => None,
    };
    match target_controller {
        Some(player) if player == controller => direction,
        Some(_) => -direction,
        None => 0,
    }
}

impl Strategy for GreedyStrategy {
    fn choose_action(&self, state: &GameState, _player: PlayerIndex) -> Result<Action, crate::simulation::TerminationReason> {
        Ok((|| -> Action {
        let actions = legal_actions(state);
        if state.pending_copy_order.is_some() { return actions[0].clone(); }
        let db = state.card_db();

        // Mulligan heuristic: count "effective mana sources" — lands count
        // as 1.0, cheap mana rocks/dorks (CMC ≤ 2 with mana abilities) count
        // as 0.7. Keep if effective sources ≥ 1.5, ensuring at least 1 real
        // land + 1 mana producer (or 2 lands). This helps combo decks keep
        // hands like "1 land + Sol Ring + Mox".
        if state.phase == crate::game::Phase::Mulligan {
            let player = state.priority_player;
            let ps = &state.players[player];
            if !ps.mulligan_decided {
                let mut mana_sources = 0.0f64;
                for &obj_id in &ps.hand {
                    let inst = &state.objects[&obj_id];
                    if let Some(def) = db.get(inst.card_def_id) {
                        if def.is_land() {
                            mana_sources += 1.0;
                        } else if !def.mana_abilities.is_empty() && def.cmc() <= 2 {
                            mana_sources += 0.7;
                        }
                    }
                }
                // Keep if 1.5-6.0 effective sources, or mulliganed twice
                if (1.5..=6.0).contains(&mana_sources) || ps.mulligan_count >= 2 {
                    return Action::MulliganKeep;
                }
                return Action::MulliganMulligan;
            }
            // Bottoming: bottom targeted spells first (dead in goldfish),
            // then highest-CMC non-land, non-mana-producing cards.
            let mut worst_card = None;
            let mut worst_score = -1i32;
            for action in &actions {
                if let Action::MulliganBottomCard { object_id } = action {
                    let inst = &state.objects[object_id];
                    let def = db.get(inst.card_def_id);
                    let score = if def.map_or(false, |d| d.is_land()) {
                        0 // keep lands
                    } else if def.map_or(false, |d| !d.mana_abilities.is_empty()) {
                        1 // keep mana producers
                    } else if def.map_or(false, |d| {
                        crate::action::spell_requires_target(d)
                    }) {
                        100 // bottom targeted spells first (dead in goldfish)
                    } else {
                        def.map_or(5, |d| d.cmc() as i32 + 2)
                    };
                    if score > worst_score {
                        worst_score = score;
                        worst_card = Some(action.clone());
                    }
                }
            }
            return worst_card.unwrap_or(actions[0].clone());
        }

        // Priority 0: If we must order triggers or replacement effects, pick
        // the first ordering (FIFO). A real MCCFR solver would evaluate all
        // orderings; greedy just uses FIFO.
        for action in &actions {
            if matches!(action, Action::OrderTriggers { .. } | Action::OrderTriggerOccurrences { .. }
                | Action::ChooseReplacementOrder { .. }) {
                return action.clone();
            }
        }

        // Priority 0a: Activate combo macro-action.
        // Always activate combos that deal damage (instant win).
        // Only activate mana-only combos if we don't already have massive mana
        // (prevents infinite loop: ActivateMacro → untap → ActivateMacro → ...).
        {
            let pool_total = state.players[_player].mana_pool.total();
            for action in &actions {
                if let Action::ActivateMacro { combo_id } = action {
                    if let Some(ref registry) = state.combo_registry {
                        if let Some(combo) = registry.get(*combo_id) {
                            let deals_damage = combo.categories.contains(
                                &crate::combo::ComboCategory::InfiniteDamage,
                            );
                            if deals_damage || pool_total < 1000 {
                                return action.clone();
                            }
                        }
                    } else if pool_total < 1000 {
                        return action.clone();
                    }
                }
            }
        }

        // Priority 0b: Resolve pending tutor — pick the first target offered.
        // Tutor targets are ordered by strategic priority (e.g., Basalt Monolith
        // first for Kinnan combo), so the first is the best default choice.
        for action in &actions {
            if let Action::ChooseTutorTarget { .. } = action {
                return action.clone();
            }
        }

        // Priority 1: Play a land if we can (from hand or graveyard)
        for action in &actions {
            if matches!(action, Action::PlayLand { .. } | Action::PlayLandFromGraveyard { .. }) {
                return action.clone();
            }
        }

        // Priority 1.5: When we have infinite mana, cast win-condition
        // artifacts (Walking Ballista) first to enable 3-piece combo kill.
        if state.players[_player].mana_pool.total() >= 1000 {
            for action in &actions {
                let obj_id = match action {
                    Action::CastSpell { object_id, .. } => Some(object_id),
                    _ => None,
                };
                if let Some(object_id) = obj_id {
                    let inst = &state.objects[object_id];
                    if let Some(def) = db.get(inst.card_def_id) {
                        // Cast any artifact with a damage-dealing ability
                        let is_artifact = def.card_types.iter().any(|t| {
                            matches!(t, crate::card::CardType::Artifact)
                        });
                        if is_artifact {
                            let has_damage = def.activated_abilities.iter().any(|a| {
                                fn effect_deals_damage(e: &crate::card::Effect) -> bool {
                                    match e {
                                        crate::card::Effect::DealDamage { .. } => true,
                                        crate::card::Effect::Multiple(subs) => {
                                            subs.iter().any(effect_deals_damage)
                                        }
                                        _ => false,
                                    }
                                }
                                effect_deals_damage(&a.effect)
                            });
                            if has_damage {
                                return action.clone();
                            }
                        }
                    }
                }
            }
        }

        // Priority 2: Cast the most expensive spell we can afford.
        // For equally expensive casts, prefer the target favored by the
        // spell's effect from the current player's perspective.
        // (includes casting commander from command zone)
        let mut best_spell: Option<&Action> = None;
        let mut best_score = (0, i32::MIN);
        for action in &actions {
            let obj_id = match action {
                Action::CastSpell { object_id, .. } => Some(object_id),
                Action::CastCommander { object_id, .. } => Some(object_id),
                _ => None,
            };
            if let Some(object_id) = obj_id {
                let inst = &state.objects[object_id];
                if let Some(def) = db.get(inst.card_def_id) {
                    let target_value = match action {
                        Action::CastSpell { targets, .. } | Action::CastCommander { targets, .. } =>
                            def.spell_effect.as_ref().map_or(0, |effect|
                                spell_target_value(state, _player, effect, targets)),
                        _ => 0,
                    };
                    let score = (def.cmc(), target_value);
                    if score >= best_score {
                        best_score = score;
                        best_spell = Some(action);
                    }
                }
            }
        }
        if let Some(spell) = best_spell {
            return spell.clone();
        }

        // Priority 2.5: Activate non-mana abilities we can afford.
        // legal_actions() only offers these when the cost is payable.
        // Prefer the most expensive ability first (Kinnan's 7-mana search
        // over Basalt's 3-mana untap) to use mana on win conditions before
        // looping for more.
        {
            let mut best_ability: Option<&Action> = None;
            let mut best_cost = 0u32;
            for action in &actions {
                if let Action::ActivateAbility { object_id, ability_index, .. } = action {
                    let inst = &state.objects[object_id];
                    if let Some(def) = db.get(inst.card_def_id) {
                        if let Some(ability) = def.activated_abilities.get(*ability_index) {
                            let cost = ability.cost.cmc();
                            if cost >= best_cost || best_ability.is_none() {
                                best_cost = cost;
                                best_ability = Some(action);
                            }
                        }
                    }
                }
            }
            if let Some(ability) = best_ability {
                return ability.clone();
            }
        }

        // Priority 2.6: Tap mana sources when a mana doubler is on the
        // field. This enables combo loops: tap Basalt Monolith for 4
        // (with Kinnan), spend 3 to untap, net +1 per cycle. Without a
        // doubler, tapping without a spell to cast is pointless.
        {
            let has_mana_doubler = state.battlefield.iter().any(|&bid| {
                let binst = &state.objects[&bid];
                if binst.controller != _player { return false; }
                if let Some(bdef) = db.get(binst.card_def_id) {
                    bdef.static_abilities.iter().any(|sa| {
                        matches!(sa, crate::layers::StaticAbility::ManaFromNonlandBonus
                            | crate::layers::StaticAbility::ManaFromSwampBonus)
                    })
                } else {
                    false
                }
            });
            if has_mana_doubler {
                for action in &actions {
                    if let Action::ActivateManaAbility { .. } = action {
                        return action.clone();
                    }
                }
            }
        }

        // Priority 3: Attack with all eligible creatures
        for action in &actions {
            if let Action::DeclareAttackers { attackers } = action {
                if !attackers.is_empty() {
                    // Find the action that attacks with the most creatures
                    let mut best_attack: Option<&Action> = None;
                    let mut max_attackers = 0;
                    for a in &actions {
                        if let Action::DeclareAttackers { attackers } = a {
                            if attackers.len() > max_attackers {
                                max_attackers = attackers.len();
                                best_attack = Some(a);
                            }
                        }
                    }
                    if let Some(attack) = best_attack {
                        return attack.clone();
                    }
                }
            }
        }

        // Priority 4: Block with favorable trades
        for action in &actions {
            if let Action::DeclareBlockers { blocks } = action {
                if !blocks.is_empty() {
                    // Simple heuristic: block if our creature's toughness > attacker's power
                    // (i.e., we survive the block)
                    let mut best_block: Option<&Action> = None;
                    let mut best_score: i32 = 0;

                    for a in &actions {
                        if let Action::DeclareBlockers { blocks } = a {
                            let score = evaluate_blocks(state, blocks);
                            if score > best_score || best_block.is_none() {
                                best_score = score;
                                best_block = Some(a);
                            }
                        }
                    }
                    if best_score > 0 {
                        if let Some(block) = best_block {
                            return block.clone();
                        }
                    }
                }
            }
        }

        // Priority 5: Cleanup discard if forced
        for action in &actions {
            if let Action::Discard { .. } = action {
                return action.clone();
            }
        }

        // Prefer EndTurn over PassPriority to reduce search depth.
        // EndTurn is only offered post-combat by legal_actions(), so this is safe.
        if actions.contains(&Action::EndTurn) {
            return Action::EndTurn;
        }

        // Default: pass priority
        Action::PassPriority

        })())
    }

    fn name(&self) -> &str {
        "Greedy"
    }
}

/// MCCFR-trained strategy: selects actions according to the average
/// strategy computed by the MCCFR solver.
///
/// After training, the average strategy (not the current strategy) converges
/// to a Nash equilibrium in two-player zero-sum games. This implementation
/// looks up the current info set in the regret table, canonicalizes the
/// available actions, and samples from the average strategy distribution.
///
/// Falls back to GreedyStrategy when the info set hasn't been visited
/// during training.
pub struct McfrStrategy {
    /// Trained regret table (read-only during play).
    policy: RegretTable,
}

impl McfrStrategy {
    /// Create a new McfrStrategy from a trained regret table.
    pub fn new(policy: RegretTable) -> Self {
        McfrStrategy { policy }
    }
}

impl Strategy for McfrStrategy {
    fn choose_action(&self, state: &GameState, player: PlayerIndex) -> Result<Action, crate::simulation::TerminationReason> {
        self.policy.ensure_usable()?;
        if player >= state.players.len() || state.players.len() < 2 { return Err(crate::simulation::TerminationReason::StateEncoding); }
        let view = state.visible_state(player);
        let normalized = InformationSet::normalize_retained_view(&view)?;
        let actions = legal_actions_abstracted(state);
        if actions.is_empty() {
            return Ok(Action::PassPriority);
        }
        if actions.len() == 1 {
            return Ok(actions[0].clone());
        }

        // Canonicalize actions for stable regret table lookup
        let canonical_actions = canonicalize_actions(&actions, state, &normalized)?;
        let info_set = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized)?;
        let info_hash = info_set.hash_value();

        let distribution = match self.policy.get(info_hash)? {
            Some(data) => data.average_strategy(&canonical_actions),
            // Greedy chooses from this state's concrete legal actions; no
            // stored canonical action is reconstructed for an unseen state.
            None => return GreedyStrategy.choose_action(state, player),
        };

        let mut rng = rand::thread_rng();
        let idx = sample_from_distribution(&distribution, &mut rng);
        Ok(actions[idx].clone())
    }

    fn name(&self) -> &str {
        "MCCFR"
    }
}

/// MCCFR-trained strategy with information set abstraction (Phase 2B).
///
/// Like `McfrStrategy`, but uses an `InfoSetAbstraction` to hash the info set
/// the same way the training did. This is essential when training uses
/// `BucketedAbstraction` — the play-time strategy must use the same abstraction
/// or it will never find matching entries in the regret table.
pub struct AbstractedMcfrStrategy {
    /// Trained regret table (read-only during play).
    policy: RegretTable,
    /// Abstraction used during training (must match).
    abstraction: Box<dyn crate::info_set::InfoSetAbstraction>,
}

impl AbstractedMcfrStrategy {
    /// Create a new strategy from a trained regret table and matching abstraction.
    pub fn new(
        policy: RegretTable,
        abstraction: Box<dyn crate::info_set::InfoSetAbstraction>,
    ) -> Self {
        AbstractedMcfrStrategy { policy, abstraction }
    }
}

impl Strategy for AbstractedMcfrStrategy {
    fn choose_action(&self, state: &GameState, player: PlayerIndex) -> Result<Action, crate::simulation::TerminationReason> {
        self.policy.ensure_usable()?;
        if player >= state.players.len() || state.players.len() < 2 { return Err(crate::simulation::TerminationReason::StateEncoding); }
        let view = state.visible_state(player);
        let normalized = InformationSet::normalize_retained_view(&view)?;
        let actions = legal_actions_abstracted(state);
        if actions.is_empty() {
            return Ok(Action::PassPriority);
        }
        if actions.len() == 1 {
            return Ok(actions[0].clone());
        }

        let canonical_actions = canonicalize_actions(&actions, state, &normalized)?;
        let info_set = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized)?;
        let info_hash = self.abstraction.abstract_info_set(&info_set);

        let distribution = match self.policy.get(info_hash)? {
            Some(data) => data.average_strategy(&canonical_actions),
            None => {
                let n = actions.len();
                vec![1.0 / n as f64; n]
            }
        };

        let mut rng = rand::thread_rng();
        let idx = sample_from_distribution(&distribution, &mut rng);
        Ok(actions[idx].clone())
    }

    fn name(&self) -> &str {
        "MCCFR-Abstracted"
    }
}

/// Goldfish strategy: simulates a passive opponent who takes no actions.
///
/// In MTG, "goldfishing" means playing solitaire against an opponent who does
/// nothing — no blocking, no attacking, no spells. This measures the fastest
/// possible clock (kill turn) for a deck. The goldfish:
///
/// - Always passes priority (never casts spells or activates abilities)
/// - Never attacks
/// - Never blocks
/// - Handles mandatory actions minimally (trigger ordering, forced discard)
///
/// Because the goldfish makes no meaningful decisions, simulations run faster
/// and have near-zero branching on the opponent's side, allowing convergence
/// to the deck's theoretical best-case win speed.
pub struct GoldfishStrategy;

impl Strategy for GoldfishStrategy {
    fn choose_action(&self, state: &GameState, player: PlayerIndex) -> Result<Action, crate::simulation::TerminationReason> {
        Ok((|| -> Action {
        let actions = legal_actions(state);
        if state.pending_copy_order.is_some() { return actions[0].clone(); }

        // Mulligan: always keep; bottom first card if forced (shouldn't happen
        // since keeping at mulligan_count=0 means no bottoming, but handle it
        // defensively so GoldfishStrategy is safe to use in any context).
        if state.phase == crate::game::Phase::Mulligan {
            for action in &actions {
                if matches!(action, Action::MulliganBottomCard { .. }) {
                    return action.clone();
                }
            }
            return Action::MulliganKeep;
        }

        // Handle mandatory actions that can't be skipped

        // Trigger ordering, replacement ordering, and damage assignment:
        // pick the first option offered (FIFO / default ordering).
        for action in &actions {
            if matches!(
                action,
                Action::OrderTriggers { .. }
                    | Action::OrderTriggerOccurrences { .. }
                    | Action::ChooseReplacementOrder { .. }
                    | Action::OrderDamageAssignment { .. }
            ) {
                return action.clone();
            }
        }

        // Forced discard during cleanup: discard the first card
        for action in &actions {
            if let Action::Discard { .. } = action {
                return action.clone();
            }
        }

        // Declare attackers: never attack (pick the empty attacker set)
        if matches!(state.phase, crate::game::Phase::DeclareAttackers) && player == state.active_player {
            for action in &actions {
                if let Action::DeclareAttackers { attackers } = action {
                    if attackers.is_empty() {
                        return action.clone();
                    }
                }
            }
        }

        // Declare blockers: never block (pick the empty block set)
        if matches!(state.phase, crate::game::Phase::DeclareBlockers) && player != state.active_player {
            for action in &actions {
                if let Action::DeclareBlockers { blocks } = action {
                    if blocks.is_empty() {
                        return action.clone();
                    }
                }
            }
        }

        // Default: always pass priority
        Action::PassPriority

        })())
    }

    fn name(&self) -> &str {
        "Goldfish"
    }
}

/// Evaluate a blocking assignment. Positive = good for the defender.
fn evaluate_blocks(state: &GameState, blocks: &[(ObjectId, ObjectId)]) -> i32 {
    let db = state.card_db();
    let mut score: i32 = 0;

    for &(blocker_id, attacker_id) in blocks {
        let blocker_inst = &state.objects[&blocker_id];
        let attacker_inst = &state.objects[&attacker_id];

        let blocker_def = match db.get(blocker_inst.card_def_id) {
            Some(d) => d,
            None => continue,
        };
        let attacker_def = match db.get(attacker_inst.card_def_id) {
            Some(d) => d,
            None => continue,
        };

        let attacker_power = state.effective_power(attacker_id);
        let attacker_toughness = state.effective_toughness(attacker_id);
        let blocker_power = state.effective_power(blocker_id);
        let blocker_toughness = state.effective_toughness(blocker_id);

        let blocker_dies = attacker_power >= blocker_toughness;
        let attacker_dies = blocker_power >= attacker_toughness;

        // Prevented damage (the attacker's power that won't hit the player)
        score += attacker_power;

        if attacker_dies && !blocker_dies {
            // We kill their creature and ours survives — great trade
            score += attacker_def.cmc() as i32 * 2;
        } else if attacker_dies && blocker_dies {
            // Trade — good if their creature is more expensive
            score += (attacker_def.cmc() as i32) - (blocker_def.cmc() as i32);
        } else if !attacker_dies && blocker_dies {
            // We lose our creature, they keep theirs — bad unless preventing lethal
            score -= blocker_def.cmc() as i32;
        }
    }

    score
}
