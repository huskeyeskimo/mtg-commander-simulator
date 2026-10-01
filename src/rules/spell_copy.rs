//! Direct creation of independent instant and sorcery spell copies.
use crate::card::effects::{Condition, DynamicValue};
use crate::card::{CardType, Effect};
use crate::game::{GameState, PlayerIndex, StackEntry, StackId, StackSource, Target};
use crate::targeting::{self, SpellTargeting};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyError {
    InvalidController,
    MissingSource,
    SourceIsAbility,
    MissingDefinition,
    UnsupportedSpell,
    UnsupportedEffect,
    InvalidTargets,
    StackIdExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyTargetPolicy {
    Preserve,
    Replace(Vec<Target>),
}

/// Put a copy directly onto the stack. Every fallible check precedes mutation.
pub fn copy_stack_spell(
    state: &mut GameState,
    source_stack_id: StackId,
    copy_controller: PlayerIndex,
    target_policy: CopyTargetPolicy,
) -> Result<StackId, CopyError> {
    if copy_controller >= state.players.len() || state.players[copy_controller].has_lost {
        return Err(CopyError::InvalidController);
    }
    let source = state
        .stack
        .iter()
        .find(|entry| entry.id == source_stack_id)
        .ok_or(CopyError::MissingSource)?;
    let definition = match &source.source {
        StackSource::Spell(object_id) => {
            let instance = state
                .objects
                .get(object_id)
                .ok_or(CopyError::MissingDefinition)?;
            state
                .card_db
                .as_ref()
                .and_then(|db| db.get(instance.card_def_id))
                .ok_or(CopyError::MissingDefinition)?
                .clone()
        }
        StackSource::SpellCopy { definition } => (**definition).clone(),
        StackSource::ActivatedAbility { .. } | StackSource::TriggeredAbility { .. } => {
            return Err(CopyError::SourceIsAbility);
        }
    };
    if !definition
        .card_types
        .iter()
        .any(|kind| matches!(kind, CardType::Instant | CardType::Sorcery))
        || definition.card_types.iter().any(|kind| {
            matches!(
                kind,
                CardType::Creature
                    | CardType::Artifact
                    | CardType::Enchantment
                    | CardType::Planeswalker
                    | CardType::Land
            )
        })
    {
        return Err(CopyError::UnsupportedSpell);
    }
    if definition
        .spell_effect
        .as_ref()
        .is_some_and(|effect| !copy_safe(effect))
    {
        return Err(CopyError::UnsupportedEffect);
    }
    let targeting = targeting::spell_targeting(&definition);
    if matches!(targeting, SpellTargeting::Unsupported) {
        return Err(CopyError::UnsupportedSpell);
    }
    let (targets, target_generations) = match target_policy {
        CopyTargetPolicy::Preserve => {
            let shape_ok = match targeting {
                SpellTargeting::Untargeted => source.targets.is_empty(),
                SpellTargeting::Single(_) => source.targets.len() == 1,
                SpellTargeting::Unsupported => false,
            };
            if !shape_ok
                || source.target_generations.len() != source.targets.len()
                || source.targets.iter().zip(&source.target_generations).any(
                    |(target, generation)| {
                        let generation_shape_invalid = match target {
                            Target::Object(_) => generation.is_none(),
                            Target::Player(_) | Target::StackEntry(_) => generation.is_some(),
                        };
                        let target_kind_invalid = match &targeting {
                            SpellTargeting::Single(spec) => !target_kind_matches(spec, target),
                            SpellTargeting::Untargeted | SpellTargeting::Unsupported => true,
                        };
                        generation_shape_invalid || target_kind_invalid
                    },
                )
            {
                return Err(CopyError::InvalidTargets);
            }
            (source.targets.clone(), source.target_generations.clone())
        }
        CopyTargetPolicy::Replace(targets) => {
            if !targeting::valid_spell_targets(state, copy_controller, &definition, &targets) {
                return Err(CopyError::InvalidTargets);
            }
            let generations = targeting::target_generations(state, &targets);
            (targets, generations)
        }
    };
    // Reserve one ID without allowing the increment to overflow on the next call.
    let next_id = state
        .next_stack_id
        .checked_add(1)
        .ok_or(CopyError::StackIdExhausted)?;
    let id = state.next_stack_id;
    state.next_stack_id = next_id;
    state.stack.push(StackEntry {
        id,
        source: StackSource::SpellCopy {
            definition: Box::new(definition),
        },
        controller: copy_controller,
        targets,
        target_generations,
    });
    Ok(id)
}

/// Validate only the representation permitted by a target specification.
/// Preserve deliberately does not check current existence or targeting legality.
fn target_kind_matches(spec: &crate::card::TargetSpec, target: &Target) -> bool {
    use crate::card::TargetSpec;
    match (spec, target) {
        (TargetSpec::AnySpell, Target::StackEntry(_)) => true,
        (TargetSpec::AnyPlayer | TargetSpec::Opponent, Target::Player(_)) => true,
        (TargetSpec::CreatureOrPlayer, Target::Object(_) | Target::Player(_)) => true,
        (
            TargetSpec::AnyCreature
            | TargetSpec::CreatureOrPlaneswalker
            | TargetSpec::AnyNonlandPermanent
            | TargetSpec::AnyPermanent,
            Target::Object(_),
        ) => true,
        _ => false,
    }
}

fn dynamic_safe(value: &DynamicValue) -> bool {
    match value {
        DynamicValue::ChargeCountersOnSource => false,
        DynamicValue::CardsInHand
        | DynamicValue::CreaturesControlled
        | DynamicValue::TotalPowerControlled
        | DynamicValue::TappedCreaturesControlled
        | DynamicValue::LandsControlled
        | DynamicValue::CreaturesInGraveyard
        | DynamicValue::AllPermanentsWithSubtype(_)
        | DynamicValue::PermanentsWithSubtype(_)
        | DynamicValue::CreaturesWithSubtype(_)
        | DynamicValue::DevotionTo(_)
        | DynamicValue::SwampsControlled
        | DynamicValue::Fixed(_)
        | DynamicValue::CardTypesInGraveyards => true,
    }
}

fn condition_safe(condition: &Condition) -> bool {
    match condition {
        Condition::SourceHasCounters => false,
        Condition::ControlCreatures
        | Condition::LifeAtOrAbove(_)
        | Condition::LifeAtOrBelow(_)
        | Condition::IsYourTurn
        | Condition::ControlNOrMore {
            count: _,
            card_type: _,
        }
        | Condition::Always
        | Condition::HandIsEmpty
        | Condition::ControlNOrMorePermanents { count: _ } => true,
    }
}

// Deliberately exhaustive: a new effect must be classified before copies can use it.
fn copy_safe(effect: &Effect) -> bool {
    use Effect::*;
    match effect {
        ExileFromHandLinked | ReturnLinkedExileToHand | CreateTokenCopyOfSource => false,
        DoublePowerUntilEOT { target: _ }
        | BuffOtherSubtype {
            subtype: _,
            amount: _,
            until_eot: _,
        } => false,
        Multiple(children) => children.iter().all(copy_safe),
        Conditional {
            condition,
            if_true,
            if_false,
        } => {
            condition_safe(condition)
                && matches!(
                    targeting::effect_targeting(if_true),
                    SpellTargeting::Untargeted
                )
                && if_false.as_ref().is_none_or(|effect| {
                    matches!(
                        targeting::effect_targeting(effect),
                        SpellTargeting::Untargeted
                    )
                })
                && copy_safe(if_true)
                && if_false.as_ref().is_none_or(|effect| copy_safe(effect))
        }
        ForEach { count, effect } => {
            dynamic_safe(count)
                && matches!(
                    targeting::effect_targeting(effect),
                    SpellTargeting::Untargeted
                )
                && copy_safe(effect)
        }
        // The existing engine has no recorded mode choice to carry onto a copy.
        Modal {
            choices: _,
            choose_count: _,
        } => false,
        CreateTokens { token: _, count } | AddDynamicMana { color: _, count } => {
            dynamic_safe(count)
        }
        LoseDynamicLife { amount, target: _ }
        | DealDynamicDamage { amount, target: _ }
        | GainDynamicLife { amount } => dynamic_safe(amount),
        DealDamage {
            amount: _,
            target: _,
        }
        | GainLife { amount: _ }
        | LoseLife {
            amount: _,
            target: _,
        }
        | DrawCards { count: _ }
        | DestroyTarget { target: _ }
        | ExileTarget { target: _ }
        | DestroyAll
        | BounceTo { zone: _, target: _ }
        | Buff {
            power: _,
            toughness: _,
            until_eot: _,
        }
        | Debuff {
            power: _,
            toughness: _,
            until_eot: _,
        }
        | DiscardCards {
            count: _,
            target: _,
        }
        | CreateToken(_)
        | Counter { target: _ }
        | PutCounters {
            count: _,
            target: _,
        }
        | MillCards {
            count: _,
            target: _,
        }
        | SacrificeCreatures {
            count: _,
            target: _,
        }
        | PreventCombatDamage
        | AddMana {
            color: _,
            amount: _,
        }
        | ExtraTurn
        | SkipPhase(_)
        | SearchLibrary {
            destination: _,
            subtype_filter: _,
        }
        | BounceAllNonlandOpponents
        | ReturnToTopOfLibrary { target: _ }
        | UntapTarget { target: _ }
        | ReturnFromGraveyardToBattlefield { target: _ }
        | ReturnFromGraveyardToHand { target: _ }
        | ExileFromGraveyard { target: _ }
        | ShuffleIntoLibrary { target: _ }
        | PutOnBottomOfLibrary { target: _ }
        | GainKeywordUntilEOT {
            keyword: _,
            target: _,
        }
        | SetPowerToughness {
            power: _,
            toughness: _,
            until_eot: _,
            target: _,
        }
        | GainControlUntilEOT { target: _ }
        | Fight { target: _ }
        | TapTarget { target: _ }
        | EachOpponentLosesLife { amount: _ }
        | EachOpponentDiscards { count: _ }
        | EachOpponentSacrifices { count: _ }
        | DrawThenDiscard {
            draw: _,
            discard: _,
            target: _,
        }
        | CreatePredefinedToken {
            token_type: _,
            count: _,
        }
        | Scry { count: _ }
        | Proliferate
        | ExtraLandDrop
        | Surveil { count: _ }
        | AddManaOfAnyColor { amount: _ }
        | CreateTokenFromDef { card_def_id: _ }
        | Unimplemented(_) => true,
    }
}
