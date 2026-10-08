//! Synthetic tests for independent spell copies; no card definition depends on Zada.
use mtg_gto::action::{
    canonical::{canonicalize, resolve},
    legal_actions, Action,
};
use mtg_gto::card::effects::{Condition, DynamicValue};
use mtg_gto::card::{CardDef, CardType, Effect, KeywordAbility, TargetSpec, ZoneType};
use mtg_gto::game::{CardDatabase, GameState, Phase, StackEntry, StackSource, Target};
use mtg_gto::info_set::InformationSet;
use mtg_gto::mana::ManaCost;
use mtg_gto::rules::{self, copy_stack_spell, CopyError, CopyTargetPolicy};
use std::sync::Arc;

const CREATURE: u64 = 970001;
const DRAW: u64 = 970002;
const HASTE_DRAW: u64 = 970003;
const COUNTER: u64 = 970004;
const MANA_DRAW: u64 = 970005;

fn state() -> GameState {
    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: CREATURE,
        name: "Test creature".into(),
        card_types: vec![CardType::Creature],
        mana_cost: Some(ManaCost::zero()),
        power: Some(1),
        toughness: Some(1),
        ..Default::default()
    });
    for (id, effect) in [
        (DRAW, Effect::DrawCards { count: 1 }),
        (
            HASTE_DRAW,
            Effect::Multiple(vec![
                Effect::GainKeywordUntilEOT {
                    keyword: KeywordAbility::Haste,
                    target: TargetSpec::AnyCreature,
                },
                Effect::DrawCards { count: 1 },
            ]),
        ),
        (
            COUNTER,
            Effect::Counter {
                target: TargetSpec::AnySpell,
            },
        ),
        (
            MANA_DRAW,
            Effect::Multiple(vec![
                Effect::AddMana {
                    color: None,
                    amount: 2,
                },
                Effect::DrawCards { count: 1 },
            ]),
        ),
    ] {
        let cost = if id == DRAW {
            ManaCost::new(1, 0, 0, 0, 0, 0)
        } else {
            ManaCost::zero()
        };
        db.insert(CardDef {
            id,
            name: format!("Test spell {id}"),
            card_types: vec![CardType::Instant],
            mana_cost: Some(cost),
            spell_effect: Some(effect),
            ..Default::default()
        });
    }
    let mut game = GameState::new(2);
    game.card_db = Some(Arc::new(db));
    game.phase = Phase::PreCombatMain;
    game.players[0].mana_pool.colorless = 10;
    for _ in 0..8 {
        game.create_card_in_zone(CREATURE, 0, ZoneType::Library);
    }
    game
}

fn cast(state: &mut GameState, card: u64, targets: Vec<Target>) -> u64 {
    let object = state.create_card_in_zone(card, 0, ZoneType::Hand);
    let action = Action::CastSpell {
        object_id: object,
        targets,
    };
    assert!(legal_actions(state).contains(&action));
    rules::apply_action(state, &action);
    object
}

fn resolve_top(state: &mut GameState) {
    rules::apply_action(state, &Action::PassPriority);
    rules::apply_action(state, &Action::PassPriority);
}

fn copy(state: &mut GameState, id: u64) -> u64 {
    copy_stack_spell(state, id, 0, CopyTargetPolicy::Preserve).unwrap()
}

#[test]
fn independent_identity_no_cast_side_effects_or_objects() {
    let mut state = state();
    let original = cast(&mut state, DRAW, vec![]);
    let id = state.stack[0].id;
    state.drain_events();
    let objects = state.objects.len();
    let next_object = state.next_object_id;
    let cast_count = state.spells_cast_this_turn;
    let mana = state.players[0].mana_pool.clone();
    let life = state.players[0].life;
    let hand = state.players[0].hand.clone();
    let first = copy(&mut state, id);
    let second = copy(&mut state, id);
    assert_ne!(id, first);
    assert_ne!(first, second);
    assert_eq!(state.objects.len(), objects);
    assert_eq!(state.next_object_id, next_object);
    assert_eq!(state.spells_cast_this_turn, cast_count);
    assert_eq!(state.players[0].mana_pool, mana);
    assert_eq!(state.players[0].life, life);
    assert_eq!(state.players[0].hand, hand);
    assert!(state.drain_events().is_empty());
    assert!(!state.players[0].graveyard.contains(&original));
}

#[test]
fn copy_bypasses_prowess_extort_and_cast_triggers() {
    use mtg_gto::card::{TriggerCondition, TriggeredAbility};
    let mut state = state();
    let mut db = state.card_db().clone();
    db.insert(CardDef {
        id: 970006,
        name: "Cast watcher".into(),
        card_types: vec![CardType::Creature],
        power: Some(1),
        toughness: Some(1),
        keywords: vec![KeywordAbility::Prowess, KeywordAbility::Extort],
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::YouCastSpell,
            effect: Effect::GainLife { amount: 1 },
            description: "cast".into(),
        }],
        ..Default::default()
    });
    state.card_db = Some(Arc::new(db));
    let watcher = state.create_card_in_zone(970006, 0, ZoneType::Battlefield);
    cast(&mut state, DRAW, vec![]);
    let original = state
        .stack
        .iter()
        .find(|e| matches!(e.source, StackSource::Spell(_)))
        .unwrap()
        .id;
    state.drain_events();
    let before = serde_json::to_value(&state).unwrap();
    copy(&mut state, original);
    let mut after = serde_json::to_value(&state).unwrap();
    let mut prior = before;
    after.as_object_mut().unwrap().remove("stack");
    after.as_object_mut().unwrap().remove("next_stack_id");
    prior.as_object_mut().unwrap().remove("stack");
    prior.as_object_mut().unwrap().remove("next_stack_id");
    assert_eq!(after, prior);
    assert!(state.drain_events().is_empty());
    assert!(state.battlefield.contains(&watcher));
}

#[test]
fn targets_generations_and_controller_sensitive_replacement() {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    let mut state = state();
    let selected = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let replacement = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    let original = cast(&mut state, HASTE_DRAW, vec![Target::Object(selected)]);
    let id = state.stack[0].id;
    let old_generations = state.stack[0].target_generations.clone();
    state.move_object(selected, ZoneType::Battlefield, ZoneType::Hand);
    state.move_object(selected, ZoneType::Hand, ZoneType::Battlefield);
    let stale = copy(&mut state, id);
    assert_eq!(
        state.stack.last().unwrap().targets,
        vec![Target::Object(selected)]
    );
    assert_eq!(
        state.stack.last().unwrap().target_generations,
        old_generations
    );
    let chosen = copy_stack_spell(
        &mut state,
        id,
        1,
        CopyTargetPolicy::Replace(vec![Target::Object(replacement)]),
    )
    .unwrap();
    assert_eq!(state.stack.last().unwrap().id, chosen);
    assert_eq!(
        state.stack.last().unwrap().target_generations,
        vec![Some(state.objects[&replacement].zone_change_count)]
    );
    assert_eq!(state.stack[0].targets, vec![Target::Object(selected)]);
    assert_eq!(state.stack[0].target_generations, old_generations);
    let stamp = state.new_timestamp();
    state.continuous_effects.push(ContinuousEffect {
        source_id: replacement,
        controller: 1,
        timestamp: stamp,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::Specific(replacement),
        modification: LayerModification::AddKeyword(KeywordAbility::Hexproof),
    });
    state.invalidate_characteristics_cache();
    assert_eq!(
        copy_stack_spell(
            &mut state,
            id,
            0,
            CopyTargetPolicy::Replace(vec![Target::Object(replacement)])
        ),
        Err(CopyError::InvalidTargets)
    );
    assert!(copy_stack_spell(
        &mut state,
        id,
        1,
        CopyTargetPolicy::Replace(vec![Target::Object(replacement)])
    )
    .is_ok());
    assert_ne!(stale, original);
}

#[test]
fn invalid_sources_effects_and_targets_are_atomic() {
    let mut state = state();
    let creature = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let original = cast(&mut state, HASTE_DRAW, vec![Target::Object(creature)]);
    let id = state.stack[0].id;
    let ability_id = state.new_stack_id();
    state.stack.push(StackEntry {
        id: ability_id,
        source: StackSource::ActivatedAbility {
            source_id: creature,
            ability_index: 0,
        },
        controller: 0,
        targets: vec![],
        target_generations: vec![],
    });
    state.drain_events();
    for (source, controller, policy) in [
        (u64::MAX, 0, CopyTargetPolicy::Preserve),
        (ability_id, 0, CopyTargetPolicy::Preserve),
        (id, 2, CopyTargetPolicy::Preserve),
        (id, 0, CopyTargetPolicy::Replace(vec![])),
        (id, 0, CopyTargetPolicy::Replace(vec![Target::Player(1)])),
    ] {
        let before = serde_json::to_value(&state).unwrap();
        assert!(copy_stack_spell(&mut state, source, controller, policy).is_err());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
    let before = serde_json::to_value(&state).unwrap();
    state.next_stack_id = u64::MAX;
    assert_eq!(
        copy_stack_spell(&mut state, id, 0, CopyTargetPolicy::Preserve),
        Err(CopyError::StackIdExhausted)
    );
    state.next_stack_id = before["next_stack_id"].as_u64().unwrap();
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    assert!(state.drain_events().is_empty());
    assert!(state.objects.contains_key(&original));
}

#[test]
fn source_dependent_effects_rejected_recursively() {
    let mut state = state();
    let id = cast(&mut state, DRAW, vec![]);
    let stack_id = state.stack[0].id;
    for effect in [
        Effect::ExileFromHandLinked,
        Effect::ReturnLinkedExileToHand,
        Effect::BuffOtherSubtype {
            subtype: "Goblin".into(),
            amount: DynamicValue::Fixed(1),
            until_eot: true,
        },
        Effect::DoublePowerUntilEOT {
            target: TargetSpec::NoTarget,
        },
        Effect::CreateTokenCopyOfSource,
        Effect::AddDynamicMana {
            color: mtg_gto::mana::Color::Red,
            count: DynamicValue::ChargeCountersOnSource,
        },
        Effect::Conditional {
            condition: Condition::SourceHasCounters,
            if_true: Box::new(Effect::DrawCards { count: 1 }),
            if_false: None,
        },
        Effect::Multiple(vec![
            Effect::DrawCards { count: 1 },
            Effect::ExileFromHandLinked,
        ]),
        Effect::ForEach {
            count: DynamicValue::Fixed(1),
            effect: Box::new(Effect::CreateTokenCopyOfSource),
        },
        Effect::Modal {
            choices: vec![Effect::CreateTokenCopyOfSource],
            choose_count: 1,
        },
    ] {
        let mut db = state.card_db().clone();
        let mut def = db.get(DRAW).unwrap().clone();
        def.spell_effect = Some(effect);
        db.insert(def);
        state.card_db = Some(Arc::new(db));
        let before = serde_json::to_value(&state).unwrap();
        assert_eq!(
            copy_stack_spell(&mut state, stack_id, 0, CopyTargetPolicy::Preserve),
            Err(CopyError::UnsupportedEffect)
        );
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
    assert!(state.objects.contains_key(&id));
}

#[test]
fn independent_resolution_and_original_departure() {
    let mut state = state();
    let original = cast(&mut state, DRAW, vec![]);
    let source = state.stack[0].id;
    let first = copy(&mut state, source);
    let second = copy(&mut state, first);
    state.move_object(original, ZoneType::Stack, ZoneType::Hand);
    assert_eq!(state.stack.len(), 2);
    assert!(format!("{:?}", state.stack[0]).contains("SpellCopy"));
    assert_eq!(state.stack[0].id, first);
    assert_eq!(state.stack[1].id, second);
    let before = state.players[0].library.len();
    resolve_top(&mut state);
    resolve_top(&mut state);
    assert_eq!(state.players[0].library.len(), before - 2);
    assert!(state.stack.is_empty());
    assert!(state.players[0].hand.contains(&original));
    assert!(!state.players[0].graveyard.contains(&original));
}

#[test]
fn original_can_resolve_before_copy() {
    let mut state = state();
    let original = cast(&mut state, DRAW, vec![]);
    let source = state.stack[0].id;
    let copy_id = copy(&mut state, source);
    state.stack.swap(0, 1);
    let before = state.players[0].library.len();
    resolve_top(&mut state);
    assert_eq!(state.players[0].graveyard, vec![original]);
    assert_eq!(state.stack[0].id, copy_id);
    resolve_top(&mut state);
    assert_eq!(state.players[0].library.len(), before - 2);
    assert_eq!(state.players[0].graveyard, vec![original]);
}

#[test]
fn counters_original_or_copy_without_collateral_cleanup() {
    for target_copy in [false, true] {
        let mut state = state();
        let original = cast(&mut state, DRAW, vec![]);
        let physical = state.stack[0].id;
        let sibling = copy(&mut state, physical);
        let chosen = if target_copy { sibling } else { physical };
        let counter = cast(&mut state, COUNTER, vec![Target::StackEntry(chosen)]);
        resolve_top(&mut state);
        assert_eq!(state.stack.len(), 1);
        assert_eq!(
            state.stack[0].id,
            if target_copy { physical } else { sibling }
        );
        assert_eq!(state.players[0].graveyard.contains(&original), !target_copy);
        assert_eq!(
            state.players[0]
                .graveyard
                .iter()
                .filter(|&&id| id == counter)
                .count(),
            1
        );
    }
}

#[test]
fn stale_permanent_target_skips_entire_composite() {
    let mut state = state();
    let creature = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let original = cast(&mut state, HASTE_DRAW, vec![Target::Object(creature)]);
    let source = state.stack[0].id;
    let copy_id = copy(&mut state, source);
    state.move_object(original, ZoneType::Stack, ZoneType::Hand);
    state.move_object(creature, ZoneType::Battlefield, ZoneType::Hand);
    state.move_object(creature, ZoneType::Hand, ZoneType::Battlefield);
    let library = state.players[0].library.clone();
    resolve_top(&mut state);
    assert!(state.stack.iter().all(|e| e.id != copy_id));
    assert_eq!(state.players[0].library, library);
    assert!(!state.has_keyword(creature, KeywordAbility::Haste));
}

#[test]
fn copied_counter_preserves_replaces_and_stales_stack_ids() {
    let mut state = state();
    let first = cast(&mut state, DRAW, vec![]);
    let lower = state.stack[0].id;
    cast(&mut state, DRAW, vec![]);
    let upper = state.stack[1].id;
    let counter = cast(&mut state, COUNTER, vec![Target::StackEntry(lower)]);
    let counter_id = state.stack[2].id;
    let preserve = copy(&mut state, counter_id);
    assert_eq!(
        state.stack.last().unwrap().targets,
        vec![Target::StackEntry(lower)]
    );
    assert_eq!(state.stack.last().unwrap().target_generations, vec![None]);
    let replacement = copy_stack_spell(
        &mut state,
        counter_id,
        0,
        CopyTargetPolicy::Replace(vec![Target::StackEntry(upper)]),
    )
    .unwrap();
    assert_eq!(
        state.stack.last().unwrap().targets,
        vec![Target::StackEntry(upper)]
    );
    assert_eq!(state.stack.last().unwrap().target_generations, vec![None]);
    state.move_object(first, ZoneType::Stack, ZoneType::Hand);
    assert_eq!(state.stack.len(), 4);
    let cards = state.players[0].graveyard.len();
    resolve_top(&mut state); // replacement removes upper
    assert!(!state.stack.iter().any(|e| e.id == upper));
    assert_eq!(state.stack.len(), 2);
    resolve_top(&mut state); // preserved target stale, no fallback to the original counter
    assert!(!state.stack.iter().any(|e| e.id == preserve));
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].id, counter_id);
    assert_eq!(state.players[0].graveyard.len(), cards + 1);
    assert!(!state.players[0].graveyard.contains(&counter));
    assert_ne!(preserve, replacement);
}

#[test]
fn sibling_copies_canonical_info_and_display() {
    let mut state = state();
    let original = cast(&mut state, DRAW, vec![]);
    let original_id = state.stack[0].id;
    let a = copy(&mut state, original_id);
    let b = copy(&mut state, original_id);
    let counter = state.create_card_in_zone(COUNTER, 0, ZoneType::Hand);
    let mut equivalent = state.clone();
    for entry in &mut equivalent.stack {
        entry.id += 1000;
    }
    equivalent.next_stack_id += 1000;
    let actions = legal_actions(&state);
    for (index, target) in [original_id, a, b].into_iter().enumerate() {
        let action = Action::CastSpell {
            object_id: counter,
            targets: vec![Target::StackEntry(target)],
        };
        assert!(actions.contains(&action));
        let canonical = canonicalize(&action, &state).unwrap();
        assert_eq!(resolve(&canonical, &state, 0).unwrap(), Some(action));
        assert_eq!(
            resolve(&canonical, &equivalent, 0).unwrap(),
            Some(Action::CastSpell {
                object_id: counter,
                targets: vec![Target::StackEntry(equivalent.stack[index].id)]
            })
        );
    }
    let info = InformationSet::from_view(&state.visible_state(0), state.card_db()).unwrap();
    assert!(!info.stack_entries[0].is_spell_copy);
    assert!(info.stack_entries[1].is_spell_copy && info.stack_entries[2].is_spell_copy);
    state.move_object(original, ZoneType::Stack, ZoneType::Hand);
    assert_eq!(state.stack.len(), 2);
    #[cfg(feature = "tui")]
    {
        assert!(
            mtg_gto::tui::format_stack_entry(&state, &state.stack[0], state.card_db())
                .contains("copy")
        );
        assert!(
            mtg_gto::tui::format_targets(&state, &[Target::StackEntry(b)], state.card_db())
                .contains("copy")
        );
    }
}

#[test]
fn copies_survive_clone_snapshot_json_and_bincode() {
    let mut state = state();
    let original = cast(&mut state, DRAW, vec![]);
    let physical = state.stack[0].id;
    let copy_id = copy(&mut state, physical);
    let saved = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let mut states = vec![
        state.clone(),
        serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap(),
    ];
    state.move_object(original, ZoneType::Stack, ZoneType::Hand);
    state.restore(saved).unwrap();
    states.push(state);
    for mut restored in states {
        restored.card_db = Some(restored.card_db.clone().unwrap_or_else(|| state_db()));
        assert_eq!(restored.stack[1].id, copy_id);
        assert_eq!(restored.stack[1].targets, Vec::<Target>::new());
        assert!(matches!(
            restored.stack[1].source,
            StackSource::SpellCopy { .. }
        ));
        assert!(restored.new_stack_id() > copy_id);
        restored.move_object(original, ZoneType::Stack, ZoneType::Hand);
        resolve_top(&mut restored);
        assert!(restored.stack.is_empty());
    }
}

fn state_db() -> Arc<CardDatabase> {
    state().card_db.unwrap()
}

#[test]
fn copy_owns_definition_after_original_definition_changes() {
    let mut state = state();
    let original = cast(&mut state, DRAW, vec![]);
    let physical = state.stack[0].id;
    let copy_id = copy(&mut state, physical);
    let mut db = state.card_db().clone();
    let mut changed = db.get(DRAW).unwrap().clone();
    changed.spell_effect = Some(Effect::DrawCards { count: 3 });
    db.insert(changed);
    state.card_db = Some(Arc::new(db));
    state.move_object(original, ZoneType::Stack, ZoneType::Hand);
    let before = state.players[0].library.len();
    resolve_top(&mut state);
    assert_eq!(state.players[0].library.len(), before - 1);
    assert!(!state.stack.iter().any(|entry| entry.id == copy_id));
}

#[test]
fn unsupported_permanent_and_malformed_preserve_are_atomic() {
    let mut state = state();
    let creature = cast(&mut state, CREATURE, vec![]);
    let permanent_id = state.stack[0].id;
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(
        copy_stack_spell(&mut state, permanent_id, 0, CopyTargetPolicy::Preserve),
        Err(CopyError::UnsupportedSpell)
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    state.move_object(creature, ZoneType::Stack, ZoneType::Hand);
    cast(&mut state, DRAW, vec![]);
    let id = state.stack[0].id;
    state.stack[0].target_generations.push(None);
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(
        copy_stack_spell(&mut state, id, 0, CopyTargetPolicy::Preserve),
        Err(CopyError::InvalidTargets)
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn missing_definition_fails_without_mutation() {
    let mut state = state();
    cast(&mut state, DRAW, vec![]);
    let source = state.stack[0].id;
    state.card_db = None;
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(
        copy_stack_spell(&mut state, source, 0, CopyTargetPolicy::Preserve),
        Err(CopyError::MissingDefinition)
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn sibling_copies_counter_independently() {
    let mut state = state();
    let original = cast(&mut state, DRAW, vec![]);
    let physical = state.stack[0].id;
    let first = copy(&mut state, physical);
    let second = copy(&mut state, physical);
    cast(&mut state, COUNTER, vec![Target::StackEntry(first)]);
    resolve_top(&mut state);
    assert_eq!(
        state.stack.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![physical, second]
    );
    assert!(!state.players[0].graveyard.contains(&original));
    resolve_top(&mut state);
    assert_eq!(state.stack[0].id, physical);
}

#[test]
fn preserve_rejects_structurally_wrong_target_kinds_atomically() {
    let mut state = state();
    let creature = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    cast(&mut state, DRAW, vec![]);
    let draw_id = state.stack[0].id;
    let counter = cast(&mut state, COUNTER, vec![Target::StackEntry(draw_id)]);
    let counter_id = state
        .stack
        .iter()
        .find(|entry| matches!(entry.source, StackSource::Spell(id) if id == counter))
        .unwrap()
        .id;

    // Corrupt only the structural variant, retaining a shape-valid generation vector.
    let source = state
        .stack
        .iter_mut()
        .find(|entry| entry.id == counter_id)
        .unwrap();
    source.targets = vec![Target::Player(1)];
    source.target_generations = vec![None];
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(
        copy_stack_spell(&mut state, counter_id, 0, CopyTargetPolicy::Preserve),
        Err(CopyError::InvalidTargets)
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);

    // Creature spells likewise cannot preserve a player target.
    let haste = cast(&mut state, HASTE_DRAW, vec![Target::Object(creature)]);
    let haste_id = state
        .stack
        .iter()
        .find(|entry| matches!(entry.source, StackSource::Spell(id) if id == haste))
        .unwrap()
        .id;
    let source = state
        .stack
        .iter_mut()
        .find(|entry| entry.id == haste_id)
        .unwrap();
    source.targets = vec![Target::Player(1)];
    source.target_generations = vec![None];
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(
        copy_stack_spell(&mut state, haste_id, 0, CopyTargetPolicy::Preserve),
        Err(CopyError::InvalidTargets)
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn preserved_structural_targets_do_not_recheck_current_legality() {
    let mut permanent_state = state();
    let creature = permanent_state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    cast(
        &mut permanent_state,
        HASTE_DRAW,
        vec![Target::Object(creature)],
    );
    let source = permanent_state.stack[0].id;
    let generations = permanent_state.stack[0].target_generations.clone();
    permanent_state.move_object(creature, ZoneType::Battlefield, ZoneType::Hand);
    let copy_id = copy(&mut permanent_state, source);
    assert_eq!(permanent_state.stack.last().unwrap().id, copy_id);
    assert_eq!(
        permanent_state.stack.last().unwrap().targets,
        vec![Target::Object(creature)]
    );
    assert_eq!(
        permanent_state.stack.last().unwrap().target_generations,
        generations
    );

    // StackEntry identity is structural too; a now-absent StackId is retained verbatim.
    let mut stack_state = state();
    cast(&mut stack_state, DRAW, vec![]);
    let stale_id = stack_state.stack[0].id;
    let counter = cast(
        &mut stack_state,
        COUNTER,
        vec![Target::StackEntry(stale_id)],
    );
    let counter_id = stack_state
        .stack
        .iter()
        .find(|entry| matches!(entry.source, StackSource::Spell(id) if id == counter))
        .unwrap()
        .id;
    stack_state.stack.retain(|entry| entry.id != stale_id);
    let copy_id = copy(&mut stack_state, counter_id);
    assert_eq!(stack_state.stack.last().unwrap().id, copy_id);
    assert_eq!(
        stack_state.stack.last().unwrap().targets,
        vec![Target::StackEntry(stale_id)]
    );
    assert_eq!(
        stack_state.stack.last().unwrap().target_generations,
        vec![None]
    );
}

#[test]
fn targeted_copies_persist_object_and_stack_targets() {
    let mut permanent_state = state();
    let creature = permanent_state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    cast(
        &mut permanent_state,
        HASTE_DRAW,
        vec![Target::Object(creature)],
    );
    let source = permanent_state.stack[0].id;
    let copy_id =
        copy_stack_spell(&mut permanent_state, source, 1, CopyTargetPolicy::Preserve).unwrap();
    let generation = permanent_state.objects[&creature].zone_change_count;
    assert_targeted_copy_roundtrips(
        &mut permanent_state,
        copy_id,
        Target::Object(creature),
        Some(generation),
    );

    let mut stack_state = state();
    cast(&mut stack_state, DRAW, vec![]);
    let target_stack_id = stack_state.stack[0].id;
    let counter = cast(
        &mut stack_state,
        COUNTER,
        vec![Target::StackEntry(target_stack_id)],
    );
    let source = stack_state
        .stack
        .iter()
        .find(|entry| matches!(entry.source, StackSource::Spell(id) if id == counter))
        .unwrap()
        .id;
    let copy_id = copy(&mut stack_state, source);
    assert_targeted_copy_roundtrips(
        &mut stack_state,
        copy_id,
        Target::StackEntry(target_stack_id),
        None,
    );
}

fn assert_targeted_copy_roundtrips(
    state: &mut GameState,
    copy_id: u64,
    target: Target,
    generation: Option<u32>,
) {
    let expected_controller = state
        .stack
        .iter()
        .find(|entry| entry.id == copy_id)
        .unwrap()
        .controller;
    let expected_def = match &state
        .stack
        .iter()
        .find(|entry| entry.id == copy_id)
        .unwrap()
        .source
    {
        StackSource::SpellCopy { definition } => (**definition).clone(),
        _ => panic!("expected spell copy"),
    };
    let expected_def_json = serde_json::to_value(&expected_def).unwrap();
    let snapshot = state.snapshot();
    let json = serde_json::to_vec(state).unwrap();
    let binary = bincode::serialize(state).unwrap();
    let mut restored = vec![
        state.clone(),
        serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap(),
    ];
    state.restore(snapshot).unwrap();
    restored.push(state.clone());
    for restored in restored {
        let entry = restored
            .stack
            .iter()
            .find(|entry| entry.id == copy_id)
            .unwrap();
        assert_eq!(entry.controller, expected_controller);
        assert_eq!(entry.targets, vec![target.clone()]);
        assert_eq!(entry.target_generations, vec![generation]);
        match &entry.source {
            StackSource::SpellCopy { definition } => {
                assert_eq!(
                    serde_json::to_value(&**definition).unwrap(),
                    expected_def_json
                );
            }
            _ => panic!("restored copy lost its source definition"),
        }
        assert!(restored.next_stack_id > copy_id);
    }
}

#[test]
fn mixed_stack_canonicalization_counts_abilities_and_is_id_independent() {
    let mut state = state();
    let creature = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let original_card = cast(&mut state, DRAW, vec![]);
    let original_id = state.stack[0].id;
    let ability_id = state.new_stack_id();
    state.stack.push(StackEntry {
        id: ability_id,
        source: StackSource::ActivatedAbility {
            source_id: creature,
            ability_index: 0,
        },
        controller: 0,
        targets: vec![],
        target_generations: vec![],
    });
    let copy_id = copy(&mut state, original_id);
    assert_eq!(
        state.stack.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![original_id, ability_id, copy_id]
    );
    let counter = state.create_card_in_zone(COUNTER, 0, ZoneType::Hand);
    let original_action = Action::CastSpell {
        object_id: counter,
        targets: vec![Target::StackEntry(original_id)],
    };
    let copy_action = Action::CastSpell {
        object_id: counter,
        targets: vec![Target::StackEntry(copy_id)],
    };
    let canonical_original = canonicalize(&original_action, &state).unwrap();
    let canonical_copy = canonicalize(&copy_action, &state).unwrap();
    assert_ne!(canonical_original, canonical_copy);
    assert_eq!(
        resolve(&canonical_original, &state, 0).unwrap(),
        Some(original_action)
    );
    assert_eq!(resolve(&canonical_copy, &state, 0).unwrap(), Some(copy_action));

    let mut equivalent = state.clone();
    for entry in &mut equivalent.stack {
        entry.id += 10_000;
    }
    equivalent.next_stack_id += 10_000;
    let resolved_original = resolve(&canonical_original, &equivalent, 0).unwrap().unwrap();
    let resolved_copy = resolve(&canonical_copy, &equivalent, 0).unwrap().unwrap();
    assert_eq!(
        resolved_original,
        Action::CastSpell {
            object_id: counter,
            targets: vec![Target::StackEntry(equivalent.stack[0].id)]
        }
    );
    assert_eq!(
        resolved_copy,
        Action::CastSpell {
            object_id: counter,
            targets: vec![Target::StackEntry(equivalent.stack[2].id)]
        }
    );
    let info = InformationSet::from_view(&state.visible_state(0), state.card_db()).unwrap();
    let equivalent_info =
        InformationSet::from_view(&equivalent.visible_state(0), equivalent.card_db()).unwrap();
    assert_eq!(info.hash_value(), equivalent_info.hash_value());
    assert!(!info.stack_entries[0].is_spell_copy);
    assert!(!info.stack_entries[1].is_spell_copy);
    assert!(info.stack_entries[2].is_spell_copy);
    assert!(state.objects.contains_key(&original_card));
}
