//! Shared targeting contract for untargeted and single-target spells.
use crate::card::{CardDef, CardType, Effect, KeywordAbility, TargetSpec};
use crate::game::{GameState, PlayerIndex, Target};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpellTargeting {
    Untargeted,
    Single(TargetSpec),
    /// Requires target allocation not supported by the single-target engine.
    Unsupported,
}

pub fn spell_targeting(def: &CardDef) -> SpellTargeting {
    def.spell_effect
        .as_ref()
        .map(effect_targeting)
        .unwrap_or(SpellTargeting::Untargeted)
}

pub(crate) fn effect_targeting(effect: &Effect) -> SpellTargeting {
    use Effect::*;
    match effect {
        #[cfg(test)]
        TestCopyBatch { copies: _ } => SpellTargeting::Untargeted,
        Multiple(effects) => {
            // Multiple executes instructions on one shared target vector. Repeated
            // identical restrictions describe that same target, not extra targets.
            let mut contract = SpellTargeting::Untargeted;
            for effect in effects {
                match effect_targeting(effect) {
                    SpellTargeting::Untargeted => (),
                    next if contract == SpellTargeting::Untargeted || contract == next => {
                        contract = next
                    }
                    _ => return SpellTargeting::Unsupported,
                }
            }
            contract
        }
        Buff { .. } | Debuff { .. } => SpellTargeting::Single(TargetSpec::AnyCreature),
        Fight { .. } => SpellTargeting::Unsupported,
        // Extracting a selector must not enable battlefield targeting for
        // graveyard effects. Their target-zone model is outside Milestone 1.
        ReturnToTopOfLibrary { target }
        | ReturnFromGraveyardToBattlefield { target }
        | ReturnFromGraveyardToHand { target }
        | ExileFromGraveyard { target } => match target {
            TargetSpec::NoTarget | TargetSpec::Controller | TargetSpec::EachCreature => {
                SpellTargeting::Untargeted
            }
            _ => SpellTargeting::Unsupported,
        },
        _ => match effect_recipients(effect) {
            EffectRecipients::Declared(
                TargetSpec::NoTarget | TargetSpec::Controller | TargetSpec::EachCreature,
            )
            | EffectRecipients::Independent => SpellTargeting::Untargeted,
            EffectRecipients::Declared(target) => SpellTargeting::Single(target.clone()),
            // Preserve specialized ability selection; do not enable it for spells.
            EffectRecipients::SelectedObjects => SpellTargeting::Unsupported,
            // Other child containers retain their existing behavior. No new
            // modal/conditional targeting support is introduced here.
            EffectRecipients::Children => SpellTargeting::Untargeted,
        },
    }
}

/// Recipient routing is separate from the spell forms supported by this milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffectRecipients<'a> {
    Declared(&'a TargetSpec),
    SelectedObjects,
    Children,
    Independent,
}

/// Exhaustive by design: adding a variant or a field requires reconsidering
/// recipient routing. Do not add a wildcard arm or `..` field patterns here.
pub(crate) fn effect_recipients(effect: &Effect) -> EffectRecipients<'_> {
    use Effect::*;
    use EffectRecipients::*;
    match effect {
        #[cfg(test)]
        TestCopyBatch { copies: _ } => Independent,
        DealDamage { amount: _, target }
        | LoseLife { amount: _, target }
        | DestroyTarget { target }
        | ExileTarget { target }
        | BounceTo { zone: _, target }
        | DiscardCards { count: _, target }
        | Counter { target }
        | PutCounters { count: _, target }
        | MillCards { count: _, target }
        | SacrificeCreatures { count: _, target }
        | LoseDynamicLife { amount: _, target }
        | ReturnToTopOfLibrary { target }
        | UntapTarget { target }
        | ReturnFromGraveyardToBattlefield { target }
        | ReturnFromGraveyardToHand { target }
        | ExileFromGraveyard { target }
        | ShuffleIntoLibrary { target }
        | PutOnBottomOfLibrary { target }
        | GainKeywordUntilEOT { keyword: _, target }
        | SetPowerToughness {
            power: _,
            toughness: _,
            until_eot: _,
            target,
        }
        | GainControlUntilEOT { target }
        | Fight { target }
        | TapTarget { target }
        | DrawThenDiscard {
            draw: _,
            discard: _,
            target,
        }
        | DoublePowerUntilEOT { target }
        | DealDynamicDamage { amount: _, target } => Declared(target),
        Buff {
            power: _,
            toughness: _,
            until_eot: _,
        }
        | Debuff {
            power: _,
            toughness: _,
            until_eot: _,
        }
        | ExileFromHandLinked
        | ReturnLinkedExileToHand => SelectedObjects,
        Multiple(_)
        | Modal {
            choices: _,
            choose_count: _,
        }
        | Conditional {
            condition: _,
            if_true: _,
            if_false: _,
        }
        | ForEach {
            count: _,
            effect: _,
        } => Children,
        GainLife { amount: _ }
        | DrawCards { count: _ }
        | DestroyAll
        | CreateToken(_)
        | CreateTokens { token: _, count: _ }
        | PreventCombatDamage
        | AddMana {
            color: _,
            amount: _,
        }
        | AddDynamicMana { color: _, count: _ }
        | ExtraTurn
        | SkipPhase(_)
        | SearchLibrary {
            destination: _,
            subtype_filter: _,
        }
        | BounceAllNonlandOpponents
        | EachOpponentLosesLife { amount: _ }
        | EachOpponentDiscards { count: _ }
        | EachOpponentSacrifices { count: _ }
        | GainDynamicLife { amount: _ }
        | CreatePredefinedToken {
            token_type: _,
            count: _,
        }
        | Scry { count: _ }
        | Proliferate
        | BuffOtherSubtype {
            subtype: _,
            amount: _,
            until_eot: _,
        }
        | ExtraLandDrop
        | Surveil { count: _ }
        | AddManaOfAnyColor { amount: _ }
        | CreateTokenCopyOfSource
        | CreateTokenFromDef { card_def_id: _ }
        | CopyCastSpellForOtherCreatures
        | ReturnWithDeathKeyword { keyword: _ }
        | Unimplemented(_) => Independent,
    }
}

pub fn target_is_legal(
    state: &GameState,
    controller: PlayerIndex,
    spec: &TargetSpec,
    target: &Target,
) -> bool {
    match target {
        Target::Player(player) => {
            *player < state.players.len()
                && !state.players[*player].has_lost
                && match spec {
                    TargetSpec::AnyPlayer | TargetSpec::CreatureOrPlayer => true,
                    TargetSpec::Opponent => *player != controller,
                    _ => false,
                }
        }
        Target::StackEntry(id) => matches!(spec, TargetSpec::AnySpell)
            && state.stack.iter().any(|entry| entry.id == *id && entry.source.is_spell()),
        Target::Object(id) => {
            if !state.objects.contains_key(id) {
                return false;
            }
            if matches!(spec, TargetSpec::CardInHand) {
                return state.is_card(*id) && state.players[controller].hand.contains(id);
            }
            if !state.battlefield.contains(id) {
                return false;
            }
            let Some(chars) = state.get_characteristics(*id) else {
                return false;
            };
            if chars.keywords.contains(&KeywordAbility::Shroud)
                || (chars.controller != controller
                    && chars.keywords.contains(&KeywordAbility::Hexproof))
            {
                return false;
            }
            match spec {
                TargetSpec::AnyCreature | TargetSpec::CreatureOrPlayer => {
                    chars.card_types.contains(&CardType::Creature)
                }
                TargetSpec::CreatureOrPlaneswalker => {
                    chars.card_types.contains(&CardType::Creature)
                        || chars.card_types.contains(&CardType::Planeswalker)
                }
                TargetSpec::AnyNonlandPermanent => !chars.card_types.contains(&CardType::Land),
                TargetSpec::AnyPermanent => true,
                _ => false,
            }
        }
    }
}

/// Equip uses computed control and type, and the shared represented targeting restrictions.
pub(crate) fn equip_target_is_legal(state: &GameState, controller: PlayerIndex, id: crate::card::ObjectId) -> bool {
    state.get_characteristics(id).is_some_and(|chars| chars.controller == controller)
        && target_is_legal(state, controller, &TargetSpec::AnyCreature, &Target::Object(id))
}

/// Equip's sorcery timing and mandatory-choice context.
pub(crate) fn equip_activation_context(state: &GameState) -> bool {
    let player = state.priority_player;
    player < state.players.len() && !state.players[player].has_lost
        && player == state.active_player && state.phase.is_main_phase()
        && state.stack.is_empty() && state.pending_tutor.is_none()
        && state.pending_copy_order.is_none() && !state.cleanup_discard_in_progress
        && state.pending_triggers.is_empty()
}

/// Source eligibility before candidate-target enumeration. Animated Equipment
/// retains its represented Equip ability independently of attachment feasibility.
pub(crate) fn equip_source_activation_cost(state: &GameState, source: crate::card::ObjectId)
    -> Option<crate::mana::ManaCost> {
    if !equip_activation_context(state) { return None; }
    let inst = state.objects.get(&source)?;
    let def = state.card_db().get(inst.card_def_id)?;
    if !def.is_equipment() || def.equip_cost.is_none() || !state.battlefield.contains(&source) { return None; }
    let chars = state.get_characteristics(source)?;
    if chars.controller != state.priority_player || chars.abilities_removed { return None; }
    def.equip_cost.clone()
}

pub(crate) fn equip_activation_cost(state: &GameState, source: crate::card::ObjectId,
    target: crate::card::ObjectId) -> Option<crate::mana::ManaCost> {
    let cost = equip_source_activation_cost(state, source)?;
    equip_target_is_legal(state, state.priority_player, target).then_some(cost)
}

/// Authoritative residence lists, excluding ability/copy references to objects.
/// Missing proposed endpoints may be ordinarily illegal; contradictory membership
/// is encoding failure and must not be repaired as part of an Equip action.
fn validate_equip_endpoint_membership(state: &GameState, id: crate::card::ObjectId)
    -> Result<(), crate::simulation::TerminationReason> {
    use crate::{game::StackSource, simulation::TerminationReason::StateEncoding};
    let mut residents = state.battlefield.iter().filter(|&&object| object == id).count();
    for player in &state.players {
        for zone in [&player.hand, &player.library, &player.graveyard, &player.exile, &player.command_zone] {
            residents += zone.iter().filter(|&&object| object == id).count();
        }
    }
    residents += state.stack.iter().filter(|entry| matches!(entry.source, StackSource::Spell(object) if object == id)).count();
    if state.pending_copy_order.as_ref().and_then(crate::game::PendingCopyOrder::resolving_entry)
        .is_some_and(|entry| matches!(entry.source, StackSource::Spell(object) if object == id)) { residents += 1; }
    if residents != usize::from(state.objects.contains_key(&id)) { return Err(StateEncoding); }
    Ok(())
}

/// Read-only endpoint preflight, performed before payment and accepted-action accounting.
pub(crate) fn validate_equip_activation_structure(state: &GameState,
    source: crate::card::ObjectId, target: crate::card::ObjectId,
) -> Result<(), crate::simulation::TerminationReason> {
    use crate::simulation::TerminationReason::StateEncoding;
    state.validate_attachment_structure().map_err(|_| StateEncoding)?;
    for id in [source, target] {
        validate_equip_endpoint_membership(state, id)?;
        if state.objects.get(&id).is_some_and(|inst| inst.object_id != id
                || inst.owner >= state.players.len() || inst.controller >= state.players.len()
                || state.card_db().get(inst.card_def_id).is_none()) { return Err(StateEncoding); }
    }
    if state.next_stack_id == u64::MAX { return Err(StateEncoding); }
    Ok(())
}

/// Owned Equip shape and identity validation. Only supplied public/current objects
/// are consulted; callers must not expose a hidden newer incarnation.
pub(crate) fn equip_entry_target<'a>(entry: &crate::game::StackEntry,
    db: &crate::game::CardDatabase, players: usize,
    object: impl Fn(crate::card::ObjectId) -> Option<&'a crate::card::CardInstance>,
) -> Result<Option<crate::card::ExactObjectRef>, crate::simulation::TerminationReason> {
    use crate::{card::ExactObjectRef, game::StackSource, simulation::TerminationReason::StateEncoding};
    let StackSource::EquipAbility { source, source_card_id, target_card_id } = &entry.source else { return Ok(None); };
    let ([Target::Object(id)], [Some(generation)]) = (entry.targets.as_slice(), entry.target_generations.as_slice()) else { return Err(StateEncoding); };
    if entry.controller >= players || db.get(*source_card_id).is_none_or(|def| !def.is_equipment())
        || db.get(*target_card_id).is_none() { return Err(StateEncoding); }
    let target = ExactObjectRef { id: *id, generation: *generation };
    if source.id == target.id && (source.generation != target.generation || source_card_id != target_card_id) { return Err(StateEncoding); }
    for (exact, definition) in [(*source, *source_card_id), (target, *target_card_id)] {
        if let Some(inst) = object(exact.id) {
            if inst.object_id != exact.id || inst.zone_change_count < exact.generation
                || (inst.zone_change_count == exact.generation && (inst.card_def_id != definition
                    || inst.owner >= players || inst.controller >= players)) {
                return Err(StateEncoding);
            }
        }
    }
    Ok(Some(target))
}

/// Validate pending Equip without consuming it or changing gameplay state.
pub(crate) fn validate_pending_equips(state: &GameState) -> Result<(), crate::simulation::TerminationReason> {
    use crate::{game::StackSource, simulation::TerminationReason::StateEncoding};
    if !state.stack.iter().any(|entry| matches!(entry.source, StackSource::EquipAbility { .. })) { return Ok(()); }
    let mut ids = std::collections::HashSet::new();
    let mut definitions = std::collections::HashMap::new();
    for entry in &state.stack {
        if !ids.insert(entry.id) { return Err(StateEncoding); }
        if let Some(target) = equip_entry_target(entry, state.card_db(), state.players.len(), |id| state.objects.get(&id))? {
            let StackSource::EquipAbility { source, source_card_id, target_card_id } = &entry.source else { unreachable!() };
            for (endpoint, definition) in [(*source, *source_card_id), (target, *target_card_id)] {
                if definitions.insert(endpoint, definition).is_some_and(|old| old != definition) { return Err(StateEncoding); }
                // Departed references own their facts. Do not inspect residence
                // or bind facts from a newer, potentially hidden incarnation.
                if state.exact_object(endpoint.id).is_none_or(|current| current == endpoint) {
                    validate_equip_endpoint_membership(state, endpoint.id)?;
                }
            }
        }
    }
    Ok(())
}

pub fn valid_spell_targets(
    state: &GameState,
    controller: PlayerIndex,
    def: &CardDef,
    targets: &[Target],
) -> bool {
    match spell_targeting(def) {
        SpellTargeting::Untargeted => targets.is_empty(),
        SpellTargeting::Single(spec) => {
            targets.len() == 1 && target_is_legal(state, controller, &spec, &targets[0])
        }
        SpellTargeting::Unsupported => false,
    }
}

/// Resolution-entry legality for destruction and 2B.2b targeted departures in
/// direct abilities and compatible Multiple trees. The classifier does not describe target contracts
/// inside Conditional, Modal, or ForEach, so those retain their prior behavior.
/// Children of Multiple share the entry decision; none is rechecked later.
pub(crate) fn valid_transition_ability_targets(
    state: &GameState,
    controller: PlayerIndex,
    effect: &Effect,
    targets: &[Target],
    generations: &[Option<u32>],
) -> bool {
    fn has_classified_departure(effect: &Effect) -> bool {
        match effect {
            Effect::DestroyTarget { .. } | Effect::BounceTo { .. }
                | Effect::ShuffleIntoLibrary { .. } | Effect::PutOnBottomOfLibrary { .. } => true,
            Effect::Multiple(children) => children.iter().any(has_classified_departure),
            _ => false,
        }
    }
    if !has_classified_departure(effect) { return true; }
    let SpellTargeting::Single(spec) = effect_targeting(effect) else {
        return true;
    };
    target_generations(state, targets) == generations
        && targets.len() == 1
        && target_is_legal(state, controller, &spec, &targets[0])
}

pub fn enumerate_spell_targets(
    state: &GameState,
    controller: PlayerIndex,
    def: &CardDef,
) -> Vec<Target> {
    let SpellTargeting::Single(spec) = spell_targeting(def) else {
        return vec![];
    };
    let mut candidates: Vec<_> = state
        .battlefield
        .iter()
        .copied()
        .map(Target::Object)
        .collect();
    candidates.extend((0..state.players.len()).map(Target::Player));
    candidates.extend(state.stack.iter().filter(|entry| entry.source.is_spell())
        .map(|entry| Target::StackEntry(entry.id)));
    candidates.extend(
        state.players[controller]
            .hand
            .iter()
            .copied()
            .map(Target::Object),
    );
    candidates
        .into_iter()
        .filter(|target| target_is_legal(state, controller, &spec, target))
        .collect()
}

pub fn target_generations(state: &GameState, targets: &[Target]) -> Vec<Option<u32>> {
    targets
        .iter()
        .map(|target| match target {
            Target::Object(id) => state.objects.get(id).map(|inst| inst.zone_change_count),
            Target::Player(_) | Target::StackEntry(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{DynamicValue, ZoneType};

    #[test]
    fn test_targeting_classifier_preserves_all_explicit_selectors() {
        use Effect::*;
        let specs = [
            TargetSpec::AnyCreature,
            TargetSpec::AnyPlayer,
            TargetSpec::CreatureOrPlayer,
            TargetSpec::CreatureOrPlaneswalker,
            TargetSpec::Opponent,
            TargetSpec::Controller,
            TargetSpec::AnyNonlandPermanent,
            TargetSpec::AnyPermanent,
            TargetSpec::AnySpell,
            TargetSpec::NoTarget,
            TargetSpec::EachCreature,
            TargetSpec::CardInHand,
            TargetSpec::CardInExileBySource,
        ];
        for target in specs {
            let effects = [
                DealDamage {
                    amount: 1,
                    target: target.clone(),
                },
                LoseLife {
                    amount: 1,
                    target: target.clone(),
                },
                DestroyTarget {
                    target: target.clone(),
                },
                ExileTarget {
                    target: target.clone(),
                },
                BounceTo {
                    zone: ZoneType::Hand,
                    target: target.clone(),
                },
                DiscardCards {
                    count: 1,
                    target: target.clone(),
                },
                Counter {
                    target: target.clone(),
                },
                PutCounters {
                    count: 1,
                    target: target.clone(),
                },
                MillCards {
                    count: 1,
                    target: target.clone(),
                },
                SacrificeCreatures {
                    count: 1,
                    target: target.clone(),
                },
                LoseDynamicLife {
                    amount: DynamicValue::Fixed(1),
                    target: target.clone(),
                },
                ReturnToTopOfLibrary {
                    target: target.clone(),
                },
                UntapTarget {
                    target: target.clone(),
                },
                ReturnFromGraveyardToBattlefield {
                    target: target.clone(),
                },
                ReturnFromGraveyardToHand {
                    target: target.clone(),
                },
                ExileFromGraveyard {
                    target: target.clone(),
                },
                ShuffleIntoLibrary {
                    target: target.clone(),
                },
                PutOnBottomOfLibrary {
                    target: target.clone(),
                },
                GainKeywordUntilEOT {
                    keyword: KeywordAbility::Haste,
                    target: target.clone(),
                },
                SetPowerToughness {
                    power: 1,
                    toughness: 1,
                    until_eot: true,
                    target: target.clone(),
                },
                GainControlUntilEOT {
                    target: target.clone(),
                },
                Fight {
                    target: target.clone(),
                },
                TapTarget {
                    target: target.clone(),
                },
                DrawThenDiscard {
                    draw: 1,
                    discard: 1,
                    target: target.clone(),
                },
                DoublePowerUntilEOT {
                    target: target.clone(),
                },
                DealDynamicDamage {
                    amount: DynamicValue::Fixed(1),
                    target: target.clone(),
                },
            ];
            assert_eq!(effects.len(), 26);
            for effect in effects {
                assert_eq!(
                    effect_recipients(&effect),
                    EffectRecipients::Declared(&target),
                    "{effect:?}"
                );
            }
        }
    }

    #[test]
    fn test_targeting_classifier_distinguishes_non_declared_recipients() {
        use crate::card::effects::Condition;
        for effect in [
            Effect::Buff {
                power: 1,
                toughness: 1,
                until_eot: true,
            },
            Effect::Debuff {
                power: 1,
                toughness: 1,
                until_eot: true,
            },
            Effect::ExileFromHandLinked,
            Effect::ReturnLinkedExileToHand,
        ] {
            assert_eq!(
                effect_recipients(&effect),
                EffectRecipients::SelectedObjects
            );
        }
        for effect in [
            Effect::Multiple(vec![]),
            Effect::Modal {
                choices: vec![],
                choose_count: 1,
            },
            Effect::Conditional {
                condition: Condition::Always,
                if_true: Box::new(Effect::DrawCards { count: 1 }),
                if_false: None,
            },
            Effect::ForEach {
                count: DynamicValue::Fixed(1),
                effect: Box::new(Effect::DrawCards { count: 1 }),
            },
        ] {
            assert_eq!(effect_recipients(&effect), EffectRecipients::Children);
        }
        assert_eq!(
            effect_recipients(&Effect::DrawCards { count: 1 }),
            EffectRecipients::Independent
        );
    }

    #[test]
    fn test_targeting_graveyard_recognition_does_not_enable_targeting() {
        for effect in [
            Effect::ReturnToTopOfLibrary {
                target: TargetSpec::AnyCreature,
            },
            Effect::ReturnFromGraveyardToBattlefield {
                target: TargetSpec::AnyCreature,
            },
            Effect::ReturnFromGraveyardToHand {
                target: TargetSpec::AnyCreature,
            },
            Effect::ExileFromGraveyard {
                target: TargetSpec::AnyCreature,
            },
        ] {
            assert_eq!(effect_targeting(&effect), SpellTargeting::Unsupported);
        }
        assert_eq!(
            effect_targeting(&Effect::ReturnToTopOfLibrary {
                target: TargetSpec::NoTarget
            }),
            SpellTargeting::Untargeted
        );
        assert_eq!(
            effect_targeting(&Effect::Fight {
                target: TargetSpec::AnyCreature
            }),
            SpellTargeting::Unsupported
        );
    }
}
