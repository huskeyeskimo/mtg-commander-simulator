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
                return state.players[controller].hand.contains(id);
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
