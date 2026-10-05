//! Milestone 2B.1: exact, owned transition evidence and occurrence selection.
use std::sync::Arc;

use mtg_gto::action::{
    canonical::{canonicalize, resolve},
    legal_actions, legal_actions_abstracted, Action,
};
use mtg_gto::card::{
    ActivatedAbility, CardDef, CardType, DynamicValue, Effect, KeywordAbility, Subtype, TargetSpec,
    TriggerCondition, TriggeredAbility, ZoneType,
};
use mtg_gto::events::{GameEvent, Zone};
use mtg_gto::game::{CardDatabase, GameState, Phase, Target};
use mtg_gto::info_set::{
    BucketedAbstraction, CardAwareBucketedAbstraction, InfoSetAbstraction, InformationSet,
};
use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
use mtg_gto::mana::{Color, ManaCost};
use mtg_gto::rules::{
    self,
    transitions::{
        transition_batch, AttachmentLki, ExactObjectRef, MovementKind, TransitionError,
        TransitionRequest,
    },
};

const SUBJECT: u64 = 999_200;
const WATCHER: u64 = 999_201;
const SPELL: u64 = 999_202;
const OTHER: u64 = 999_203;
const DEATH_WATCHER: u64 = 999_204;
const DIES_SUBJECT: u64 = 999_205;
const LAND: u64 = 999_206;
const ACTIVATOR: u64 = 999_207;

fn database() -> Arc<CardDatabase> {
    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: SUBJECT,
        name: "Event subject".into(),
        card_types: vec![CardType::Creature],
        subtypes: vec![Subtype("Goblin".into())],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::LeavesBattlefield,
            effect: Effect::GainLife { amount: 1 },
            description: "leave".into(),
        }],
        ..Default::default()
    });
    db.insert(CardDef {
        id: WATCHER,
        name: "Departure observer".into(),
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::APermanentLeaves,
            effect: Effect::GainLife { amount: 2 },
            description: "observe leave".into(),
        }],
        ..Default::default()
    });
    db.insert(CardDef {
        id: OTHER,
        name: "Other subject".into(),
        card_types: vec![CardType::Creature],
        power: Some(1),
        toughness: Some(1),
        ..Default::default()
    });
    db.insert(CardDef {
        id: SPELL,
        name: "Exile test".into(),
        card_types: vec![CardType::Instant],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(Effect::ExileTarget {
            target: TargetSpec::AnyCreature,
        }),
        ..Default::default()
    });
    db.insert(CardDef {
        id: DEATH_WATCHER,
        name: "Death observer".into(),
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::ACreatureDies,
            effect: Effect::GainLife { amount: 4 },
            description: "observe death".into(),
        }],
        ..Default::default()
    });
    db.insert(CardDef {
        id: DIES_SUBJECT,
        name: "Self death observer".into(),
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        triggered_abilities: vec![TriggeredAbility {
            trigger: TriggerCondition::Dies,
            effect: Effect::GainLife { amount: 5 },
            description: "self death".into(),
        }],
        ..Default::default()
    });
    db.insert(CardDef {
        id: LAND,
        name: "Guard land".into(),
        card_types: vec![CardType::Land],
        ..Default::default()
    });
    db.insert(CardDef {
        id: ACTIVATOR,
        name: "Guard activator".into(),
        card_types: vec![CardType::Creature],
        power: Some(1),
        toughness: Some(1),
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(),
            requires_tap: false,
            sacrifice_cost: None,
            life_cost: 0,
            effect: Effect::GainLife { amount: 1 },
            description: "Gain life".into(),
        }],
        ..Default::default()
    });
    Arc::new(db)
}

fn game() -> GameState {
    let mut state = GameState::new(2);
    state.card_db = Some(database());
    state.phase = Phase::PreCombatMain;
    state.active_player = 0;
    state.priority_player = 0;
    state
}

fn request(state: &GameState, id: u64, to: ZoneType) -> TransitionRequest {
    TransitionRequest {
        object: ExactObjectRef {
            id,
            generation: state.objects[&id].zone_change_count,
        },
        from: ZoneType::Battlefield,
        to,
        kind: MovementKind::Put,
    }
}

#[test]
fn valid_exile_owns_lki_and_emits_only_committed_departure() {
    let mut state = game();
    let subject = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    state.objects.get_mut(&subject).unwrap().controller = 1;
    state.objects.get_mut(&subject).unwrap().plus_counters = 1;
    state.objects.get_mut(&subject).unwrap().tapped = true;
    state.objects.get_mut(&subject).unwrap().attached_to = Some(999_999);
    let _ = state.effective_power(subject); // warm cache
    let requested = request(&state, subject, ZoneType::Exile);
    let batch = transition_batch(&mut state, &[requested]).unwrap();
    let moved = &batch.transitions[0];
    assert_eq!(moved.before.object.id, subject);
    assert_eq!(moved.before.owner, 0);
    assert_eq!(moved.before.controller, 1);
    assert_eq!(moved.before.power, 3);
    assert!(moved.before.tapped);
    assert_eq!(moved.before.attachment, AttachmentLki::UnverifiedLegacyLink);
    assert_eq!(moved.destination.zone, ZoneType::Exile);
    assert!(!moved.creature_died());
    assert!(state.players[0].exile.contains(&subject));
    assert_eq!(
        state
            .pending_events
            .iter()
            .filter(|event| matches!(event,
        GameEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Exile }
            if *object == subject))
            .count(),
        1
    );
    assert_eq!(state.pending_triggers.len(), 1);
    let ctx = state.pending_triggers[0]
        .context
        .zone_transition
        .as_ref()
        .unwrap();
    assert!(ctx.source_was_subject);
    assert_eq!(ctx.subject.before.controller, 1);
    assert!(state.stack.is_empty()); // 2A owns placement
}

#[test]
fn invalid_requests_reject_without_partial_mutation_or_cache_effect() {
    let mut state = game();
    let id = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let valid_peer = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let other = state.create_card_in_zone(OTHER, 0, ZoneType::Hand);
    let good = request(&state, valid_peer, ZoneType::Exile);
    let bad_base = request(&state, id, ZoneType::Exile);
    let cases = [
        (
            TransitionRequest {
                object: ExactObjectRef { id, generation: 99 },
                ..bad_base
            },
            TransitionError::StaleIncarnation(id),
        ),
        (
            TransitionRequest {
                object: ExactObjectRef {
                    id: other,
                    generation: 0,
                },
                ..bad_base
            },
            TransitionError::WrongZone(other),
        ),
        (
            TransitionRequest {
                object: ExactObjectRef {
                    id: 987_654,
                    generation: 0,
                },
                ..bad_base
            },
            TransitionError::MissingObject(987_654),
        ),
    ];
    for (bad, expected) in cases {
        let before = serde_json::to_vec(&state).unwrap();
        let events = state.pending_events.clone();
        assert_eq!(
            transition_batch(&mut state, &[good, bad]).unwrap_err(),
            expected
        );
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert_eq!(state.pending_events, events);
    }
    let before_unsupported = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(
            &mut state,
            &[
                good,
                TransitionRequest {
                    to: ZoneType::Stack,
                    ..bad_base
                }
            ]
        )
        .unwrap_err(),
        TransitionError::UnsupportedPath
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before_unsupported);
    let before = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(&mut state, &[good, good]).unwrap_err(),
        TransitionError::DuplicateSubject(valid_peer)
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn missing_database_rejects_without_mutation() {
    let mut state = game();
    let id = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let req = request(&state, id, ZoneType::Exile);
    state.card_db = None;
    let before = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(&mut state, &[req]).unwrap_err(),
        TransitionError::MissingDatabase
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn one_batch_observes_all_departures_including_its_own_watcher() {
    let mut state = game();
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let a = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let requests = [
        request(&state, a, ZoneType::Exile),
        request(&state, b, ZoneType::Exile),
        request(&state, watcher, ZoneType::Exile),
    ];
    let batch = transition_batch(&mut state, &requests).unwrap();
    assert_eq!(batch.transitions.len(), 3);
    assert_eq!(
        state
            .pending_triggers
            .iter()
            .filter(|t| t.source_id == watcher)
            .count(),
        3
    );
    assert!(state.pending_triggers.iter().all(|t| t
        .context
        .zone_transition
        .as_ref()
        .unwrap()
        .subject
        .before
        .object
        .generation
        == 0));
    assert!(state.stack.is_empty());
    let subjects: Vec<_> = state
        .pending_triggers
        .iter()
        .filter(|t| t.source_id == watcher)
        .map(|t| {
            t.context
                .zone_transition
                .as_ref()
                .unwrap()
                .subject
                .before
                .object
                .id
        })
        .collect();
    assert!(subjects.contains(&watcher) && subjects.contains(&a) && subjects.contains(&b));
}

#[test]
fn successive_events_do_not_reuse_departed_observers() {
    let mut state = game();
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let a = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let first_request = request(&state, watcher, ZoneType::Exile);
    let first = transition_batch(&mut state, &[first_request]).unwrap();
    let first_count = state.pending_triggers.len();
    let second_request = request(&state, a, ZoneType::Exile);
    let second = transition_batch(&mut state, &[second_request]).unwrap();
    assert_eq!(first.transitions.len(), 1);
    assert_eq!(second.transitions.len(), 1);
    assert_eq!(
        state
            .pending_triggers
            .iter()
            .filter(|t| t.source_id == watcher)
            .count(),
        1
    );
    assert_eq!(state.pending_triggers.len(), first_count + 1); // a's own leave
    assert!(state.stack.is_empty());
}

#[test]
fn token_bridge_owns_event_before_legacy_purge() {
    let mut state = game();
    let id = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    state.objects.get_mut(&id).unwrap().is_token = true;
    let req = request(&state, id, ZoneType::Exile);
    let batch = transition_batch(&mut state, &[req]).unwrap();
    assert!(batch.transitions[0].before.is_token);
    assert_eq!(batch.transitions[0].destination.zone, ZoneType::Exile);
    assert!(
        state.pending_triggers[0]
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .subject
            .before
            .is_token
    );
    assert!(!state.objects.contains_key(&id));
    assert!(!state.players[0].exile.contains(&id));
    // The immediate purge is temporary legacy behavior, not 2C cessation.
}

#[test]
fn layered_facts_and_reused_object_id_do_not_rewrite_lki() {
    let mut state = game();
    let id = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let original_generation = state.objects[&id].zone_change_count;
    for modification in [
        LayerModification::ChangeController(1),
        LayerModification::AddSubtype(Subtype("Wizard".into())),
        LayerModification::AddColor(Color::Blue),
        LayerModification::AddKeyword(KeywordAbility::Flying),
    ] {
        let timestamp = state.new_timestamp();
        state.continuous_effects.push(ContinuousEffect {
            source_id: id,
            controller: 0,
            timestamp,
            duration: Duration::UntilEndOfTurn,
            affected: AffectedObjects::SpecificIncarnation {
                object_id: id,
                zone_change_count: original_generation,
            },
            modification,
        });
    }
    state.invalidate_characteristics_cache();
    let _ = state.effective_power(id);
    let requested = request(&state, id, ZoneType::Exile);
    let batch = transition_batch(&mut state, &[requested]).unwrap();
    let before = &batch.transitions[0].before;
    assert_eq!(before.controller, 1);
    assert!(before.subtypes.iter().any(|subtype| subtype.0 == "Wizard"));
    assert!(before.keywords.contains(&KeywordAbility::Flying));
    assert!(before.colors.contains(&Color::Blue));
    state.move_object(id, ZoneType::Exile, ZoneType::Battlefield);
    assert_ne!(
        state.objects[&id].zone_change_count,
        before.object.generation
    );
    let context = state.pending_triggers[0]
        .context
        .zone_transition
        .as_ref()
        .unwrap();
    assert_eq!(context.subject.before.controller, 1);
    assert!(
        !context
            .public_info(Some(state.objects[&id].zone_change_count))
            .same_incarnation_now
    );
}

#[test]
fn subject_occurrences_order_and_round_trip_without_raw_id_keys() {
    let mut state = game();
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let a = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let requests = [
        request(&state, a, ZoneType::Exile),
        request(&state, b, ZoneType::Exile),
    ];
    transition_batch(&mut state, &requests).unwrap();
    assert_eq!(
        state
            .pending_triggers
            .iter()
            .filter(|t| t.source_id == watcher)
            .count(),
        2
    );
    assert!(state.pending_triggers.iter().any(|t| t
        .context
        .zone_transition
        .as_ref()
        .is_some_and(|ctx| ctx.subject.before.card_id == SUBJECT)));
    let before_hash =
        InformationSet::from_view(&state.visible_state(0), state.card_db()).hash_value();
    let snapshot = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let bin = bincode::serialize(&state).unwrap();
    let mut variants = [
        state.clone(),
        serde_json::from_slice(&json).unwrap(),
        bincode::deserialize(&bin).unwrap(),
    ];
    state.pending_triggers.clear();
    state.restore(snapshot);
    variants[0] = state.clone();
    for restored in &mut variants {
        restored.card_db = Some(database());
        assert_eq!(
            InformationSet::from_view(&restored.visible_state(0), restored.card_db()).hash_value(),
            before_hash
        );
        assert_eq!(restored.pending_triggers.len(), 3); // own leave + two observer occurrences
        restored.priority_player = 0;
        let choices: Vec<_> = legal_actions(restored)
            .into_iter()
            .filter(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
            .collect();
        assert!(!choices.is_empty());
        let choice = choices
            .iter()
            .find(|choice| {
                matches!(choice,
            Action::OrderTriggerOccurrences { ordering } if ordering[0] != 0)
            })
            .unwrap();
        let canonical = canonicalize(choice, restored);
        let canonical_json = serde_json::to_vec(&canonical).unwrap();
        let canonical_binary = bincode::serialize(&canonical).unwrap();
        assert_eq!(
            serde_json::from_slice::<mtg_gto::action::canonical::CanonicalAction>(&canonical_json)
                .unwrap(),
            canonical
        );
        assert_eq!(
            bincode::deserialize::<mtg_gto::action::canonical::CanonicalAction>(&canonical_binary)
                .unwrap(),
            canonical
        );
        assert_eq!(resolve(&canonical, restored, 0), Some(choice.clone()));
        rules::apply_action(restored, choice);
        assert!(restored.pending_triggers.is_empty());
        assert_eq!(restored.stack.len(), 3);
    }
}

#[test]
fn real_exile_spell_uses_kernel_once_without_dies() {
    let mut state = game();
    let id = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let spell = state.create_card_in_zone(SPELL, 0, ZoneType::Hand);
    let cast = Action::CastSpell {
        object_id: spell,
        targets: vec![Target::Object(id)],
    };
    assert!(legal_actions(&state).contains(&cast));
    rules::apply_action(&mut state, &cast);
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert!(state.players[1].exile.contains(&id));
    assert_eq!(
        state
            .pending_events
            .iter()
            .filter(|event| matches!(event,
        GameEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Exile }
            if *object == id))
            .count(),
        1
    );
    assert_eq!(
        state.pending_triggers.len()
            + state
                .stack
                .iter()
                .filter(|entry| matches!(
                    entry.source,
                    mtg_gto::game::StackSource::TriggeredAbility { .. }
                ))
                .count(),
        1
    );
    assert!(!state.pending_triggers.iter().any(|t| t
        .context
        .zone_transition
        .as_ref()
        .is_some_and(|ctx| ctx.subject.creature_died())));
}

#[test]
fn actual_destination_controls_death_and_simultaneous_watcher_capture() {
    let mut state = game();
    let watcher = state.create_card_in_zone(DEATH_WATCHER, 0, ZoneType::Battlefield);
    let subject = state.create_card_in_zone(DIES_SUBJECT, 1, ZoneType::Battlefield);
    let requests = [
        request(&state, subject, ZoneType::Graveyard),
        request(&state, watcher, ZoneType::Graveyard),
    ];
    let batch = transition_batch(&mut state, &requests).unwrap();
    assert!(batch
        .transitions
        .iter()
        .all(|member| member.creature_died()));
    assert_eq!(
        state
            .pending_triggers
            .iter()
            .filter(|trigger| trigger.source_id == watcher)
            .count(),
        2
    );
    assert_eq!(
        state
            .pending_triggers
            .iter()
            .filter(|trigger| trigger.source_id == subject)
            .count(),
        1
    );
    assert_eq!(batch.transitions[0].destination.player, 1); // owner, not watcher/controller

    let mut exile = game();
    let death_watcher = exile.create_card_in_zone(DEATH_WATCHER, 0, ZoneType::Battlefield);
    let victim = exile.create_card_in_zone(DIES_SUBJECT, 0, ZoneType::Battlefield);
    let moved = request(&exile, victim, ZoneType::Exile);
    let batch = transition_batch(&mut exile, &[moved]).unwrap();
    assert!(!batch.transitions[0].creature_died());
    assert!(!exile
        .pending_triggers
        .iter()
        .any(|trigger| trigger.source_id == death_watcher));
    assert!(!exile
        .pending_triggers
        .iter()
        .any(|trigger| trigger.source_id == victim));
}

#[test]
fn batch_permutation_changes_storage_order_not_occurrence_multiset() {
    let mut a = game();
    let watcher = a.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = a.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let second = a.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let mut b = a.clone();
    let requests = [
        request(&a, first, ZoneType::Exile),
        request(&a, second, ZoneType::Exile),
    ];
    transition_batch(&mut a, &requests).unwrap();
    transition_batch(&mut b, &[requests[1], requests[0]]).unwrap();
    let descriptions = |state: &GameState| {
        let mut values: Vec<_> = state
            .pending_triggers
            .iter()
            .filter(|t| t.source_id == watcher)
            .map(|t| {
                format!(
                    "{:?}",
                    t.context.zone_transition.as_ref().unwrap().public_info(
                        t.context.zone_transition.as_ref().and_then(|ctx| state
                            .objects
                            .get(&ctx.subject.after.id)
                            .map(|inst| inst.zone_change_count))
                    )
                )
            })
            .collect();
        values.sort();
        values
    };
    assert_eq!(descriptions(&a), descriptions(&b));
    assert_eq!(
        InformationSet::from_view(&a.visible_state(0), a.card_db()).hash_value(),
        InformationSet::from_view(&b.visible_state(0), b.card_db()).hash_value()
    );
}

fn subject_occurrence_game(with_purged_prefix: bool) -> GameState {
    let mut state = game();
    if with_purged_prefix {
        let token = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
        state.objects.get_mut(&token).unwrap().is_token = true;
        let req = request(&state, token, ZoneType::Exile);
        transition_batch(&mut state, &[req]).unwrap();
        state.pending_events.clear();
    }
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let second = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let requests = [
        request(&state, first, ZoneType::Exile),
        request(&state, second, ZoneType::Exile),
    ];
    transition_batch(&mut state, &requests).unwrap();
    state
}

#[test]
fn equivalent_runtime_id_allocations_have_same_canonical_choices_and_information() {
    let a = subject_occurrence_game(false);
    let b = subject_occurrence_game(true);
    assert_ne!(
        a.pending_triggers[0].source_id,
        b.pending_triggers[0].source_id
    );
    assert_eq!(
        InformationSet::from_view(&a.visible_state(0), a.card_db()).hash_value(),
        InformationSet::from_view(&b.visible_state(0), b.card_db()).hash_value()
    );
    let choices_a: Vec<_> = legal_actions(&a)
        .into_iter()
        .filter(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .map(|action| canonicalize(&action, &a))
        .collect();
    let choices_b: Vec<_> = legal_actions(&b)
        .into_iter()
        .filter(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .map(|action| canonicalize(&action, &b))
        .collect();
    assert_eq!(choices_a, choices_b);
    for canonical in choices_a {
        assert!(resolve(&canonical, &b, 0).is_some());
    }
}

#[test]
fn invalid_direct_occurrence_order_cannot_drop_or_duplicate_triggers() {
    let state = subject_occurrence_game(false);
    let mut altered = state.clone();
    let before = serde_json::to_vec(&altered).unwrap();
    rules::apply_action(
        &mut altered,
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 0, 1],
        },
    );
    assert_eq!(serde_json::to_vec(&altered).unwrap(), before);
    rules::apply_action(
        &mut altered,
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
    );
    assert_eq!(serde_json::to_vec(&altered).unwrap(), before);
    rules::apply_action(
        &mut altered,
        &Action::OrderTriggers {
            ordering: vec![(state.pending_triggers[0].source_id, 0); 3],
        },
    );
    assert_eq!(serde_json::to_vec(&altered).unwrap(), before);
}

#[test]
fn seven_subject_occurrences_keep_existing_fifo_limit() {
    let mut state = game();
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let ids: Vec<_> = (0..7)
        .map(|_| state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield))
        .collect();
    let requests: Vec<_> = ids
        .iter()
        .map(|&id| request(&state, id, ZoneType::Exile))
        .collect();
    transition_batch(&mut state, &requests).unwrap();
    assert_eq!(state.pending_triggers.len(), 7);
    assert_eq!(
        legal_actions(&state)
            .into_iter()
            .filter(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
            .count(),
        1
    );
}

#[test]
fn occurrence_retains_controller_and_instruction_after_source_departure_and_control_change() {
    let mut state = game();
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let first = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[first]).unwrap();
    let original = state.pending_triggers[0].clone();
    state.objects.get_mut(&watcher).unwrap().controller = 1;
    state.invalidate_characteristics_cache();
    let second = request(&state, watcher, ZoneType::Exile);
    transition_batch(&mut state, &[second]).unwrap();
    assert_eq!(original.controller, 0);
    assert_eq!(original.context.source_card_id, WATCHER);
    assert!(matches!(
        original.context.effect,
        Effect::GainLife { amount: 2 }
    ));
    assert_eq!(
        original
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .subject
            .before
            .card_id,
        OTHER
    );
    assert_eq!(state.pending_triggers[0].controller, 0);
}

#[test]
fn two_controllers_order_one_window_in_outgoing_apnap_groups() {
    let mut state = game();
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
    let a = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let requested = [
        request(&state, a, ZoneType::Exile),
        request(&state, b, ZoneType::Exile),
    ];
    transition_batch(&mut state, &requested).unwrap();
    assert_eq!(state.pending_triggers.len(), 4);
    assert!(state.stack.is_empty());
    rules::check_state_based_actions(&mut state);
    assert_eq!(state.priority_player, 0);
    let first = legal_actions(&state)
        .into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    rules::apply_action(&mut state, &first);
    assert_eq!(
        state
            .stack
            .iter()
            .map(|entry| entry.controller)
            .collect::<Vec<_>>(),
        vec![0, 0]
    );
    assert_eq!(state.priority_player, 1);
    let second = legal_actions(&state)
        .into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    rules::apply_action(&mut state, &second);
    assert_eq!(
        state
            .stack
            .iter()
            .map(|entry| entry.controller)
            .collect::<Vec<_>>(),
        vec![0, 0, 1, 1]
    );
    assert!(state.pending_triggers.is_empty());
}

#[test]
fn production_exile_of_token_captures_history_and_trigger_survives_purge() {
    let mut state = game();
    let id = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    state.objects.get_mut(&id).unwrap().is_token = true;
    let spell = state.create_card_in_zone(SPELL, 0, ZoneType::Hand);
    rules::apply_action(
        &mut state,
        &Action::CastSpell {
            object_id: spell,
            targets: vec![Target::Object(id)],
        },
    );
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert!(!state.objects.contains_key(&id));
    assert!(state.stack.iter().any(|entry| matches!(&entry.source,
        mtg_gto::game::StackSource::TriggeredAbility { context, .. }
            if context.zone_transition.as_ref().is_some_and(|ctx| ctx.subject.before.is_token))));
    let before_life = state.players[1].life;
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(state.players[1].life, before_life + 1);
}

#[test]
fn missing_definition_and_generation_exhaustion_reject_before_mutation() {
    let mut state = game();
    let id = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let missing = 9_999_991;
    state.objects.get_mut(&id).unwrap().card_def_id = missing;
    let req = request(&state, id, ZoneType::Exile);
    let before = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(&mut state, &[req]).unwrap_err(),
        TransitionError::MissingDefinition(missing)
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    state.objects.get_mut(&id).unwrap().card_def_id = SUBJECT;
    state.objects.get_mut(&id).unwrap().zone_change_count = u32::MAX;
    let req = request(&state, id, ZoneType::Exile);
    let before = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(&mut state, &[req]).unwrap_err(),
        TransitionError::GenerationExhausted(id)
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn invalid_owner_or_linked_followup_cannot_partially_commit_batch() {
    let mut state = game();
    let first = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let second = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    state.objects.get_mut(&second).unwrap().owner = state.players.len();
    let requests = [
        request(&state, first, ZoneType::Exile),
        request(&state, second, ZoneType::Exile),
    ];
    let before = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(&mut state, &requests).unwrap_err(),
        TransitionError::InvalidPlayer(second)
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);

    state.objects.get_mut(&second).unwrap().owner = 0;
    let linked = state.create_card_in_zone(OTHER, 0, ZoneType::Exile);
    state.objects.get_mut(&linked).unwrap().exiled_by = Some(second);
    state.objects.get_mut(&linked).unwrap().zone_change_count = u32::MAX;
    let before = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(&mut state, &requests).unwrap_err(),
        TransitionError::InvalidLinkedState(linked)
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn ability_removal_pre_event_suppresses_departure_watcher() {
    let mut state = game();
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let timestamp = state.new_timestamp();
    state.continuous_effects.push(ContinuousEffect {
        source_id: watcher,
        controller: 0,
        timestamp,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::SpecificIncarnation {
            object_id: watcher,
            zone_change_count: state.objects[&watcher].zone_change_count,
        },
        modification: LayerModification::RemoveAllAbilities,
    });
    state.invalidate_characteristics_cache();
    let requested = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[requested]).unwrap();
    assert!(state.pending_triggers.is_empty());
}

#[test]
fn successive_exiles_share_placement_window_without_becoming_one_event() {
    let mut state = game();
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let a = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    let first = request(&state, a, ZoneType::Exile);
    let first_batch = transition_batch(&mut state, &[first]).unwrap();
    let second = request(&state, b, ZoneType::Exile);
    let second_batch = transition_batch(&mut state, &[second]).unwrap();
    assert_eq!(first_batch.transitions.len(), 1);
    assert_eq!(second_batch.transitions.len(), 1);
    assert_eq!(
        state
            .pending_triggers
            .iter()
            .filter(|trigger| trigger.source_id == watcher)
            .count(),
        2
    );
    assert!(state.stack.is_empty());
    rules::check_state_based_actions(&mut state);
    let choice = legal_actions(&state)
        .into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    rules::apply_action(&mut state, &choice);
    assert_eq!(state.stack.len(), 3); // two watcher occurrences and self leave
}

#[test]
fn linked_exile_followup_waits_until_primary_batch_is_complete() {
    let mut state = game();
    let a = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let linked = state.create_card_in_zone(OTHER, 0, ZoneType::Exile);
    state.objects.get_mut(&linked).unwrap().exiled_by = Some(a);
    let requested = [
        request(&state, a, ZoneType::Exile),
        request(&state, b, ZoneType::Exile),
    ];
    let batch = transition_batch(&mut state, &requested).unwrap();
    assert_eq!(batch.transitions.len(), 2);
    assert!(state.players[0].graveyard.contains(&linked));
    let events: Vec<_> = state
        .pending_events
        .iter()
        .filter_map(|event| {
            if let GameEvent::ZoneChange { object, from, to } = event {
                Some((*object, *from, *to))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        events,
        vec![
            (a, Zone::Battlefield, Zone::Exile),
            (b, Zone::Battlefield, Zone::Exile),
            (linked, Zone::Exile, Zone::Graveyard)
        ]
    );
}

#[test]
fn pending_information_distinguishes_live_source_from_older_incarnation() {
    let state = subject_occurrence_game(false);
    let mut old = state.clone();
    let watcher = old
        .pending_triggers
        .iter()
        .position(|trigger| trigger.context.source_card_id == WATCHER)
        .unwrap();
    old.pending_triggers[watcher].context.source_generation = u32::MAX;
    let current = InformationSet::from_view(&state.visible_state(0), state.card_db());
    let older = InformationSet::from_view(&old.visible_state(0), old.card_db());
    assert_ne!(current.hash_value(), older.hash_value());
    assert_ne!(
        BucketedAbstraction.abstract_info_set(&current),
        BucketedAbstraction.abstract_info_set(&older)
    );
    let card_aware = CardAwareBucketedAbstraction {
        card_db: state.card_db(),
    };
    assert_ne!(
        card_aware.abstract_info_set(&current),
        card_aware.abstract_info_set(&older)
    );
    let current_choice = legal_actions(&state)
        .into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    let older_choice = legal_actions(&old)
        .into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    assert_ne!(
        canonicalize(&current_choice, &state),
        canonicalize(&older_choice, &old)
    );
}

#[test]
fn supported_commander_redirect_is_recorded_as_actual_command_destination() {
    let mut state = GameState::new_commander(2);
    state.card_db = Some(database());
    let id = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield);
    state.players[0].commander_object_id = Some(id);
    let requested = request(&state, id, ZoneType::Exile);
    let batch = transition_batch(&mut state, &[requested]).unwrap();
    assert_eq!(batch.transitions[0].destination.zone, ZoneType::Command);
    assert!(state.players[0].command_zone.contains(&id));
    assert!(!state.players[0].exile.contains(&id));
    assert!(state.pending_events.iter().any(|event| matches!(event,
        GameEvent::ZoneChange { object, to: Zone::Command, .. } if *object == id)));
    assert!(!batch.transitions[0].creature_died());
    // The current direct redirect remains legacy; graveyard-then-SBA choice is 2F.
}

#[test]
fn new_departure_predicate_appends_after_existing_serialized_tags() {
    assert_eq!(
        bincode::serialize(&TriggerCondition::YouCastInstantOrSorceryTargetingOnlySelf).unwrap(),
        22u32.to_le_bytes()
    );
    assert_eq!(
        bincode::serialize(&TriggerCondition::APermanentLeaves).unwrap(),
        23u32.to_le_bytes()
    );
}

#[test]
fn linked_primary_member_is_moved_only_once() {
    let mut state = game();
    let a = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&b).unwrap().exiled_by = Some(a);
    state.objects.get_mut(&b).unwrap().zone_change_count = u32::MAX - 1;
    let requests = [
        request(&state, a, ZoneType::Exile),
        request(&state, b, ZoneType::Exile),
    ];
    let batch = transition_batch(&mut state, &requests).unwrap();
    assert_eq!(batch.transitions.len(), 2);
    assert_eq!(state.objects[&b].zone_change_count, u32::MAX);
    assert_eq!(
        state.players[0].exile.iter().filter(|&&id| id == b).count(),
        1
    );
    assert_eq!(
        state
            .pending_events
            .iter()
            .filter(|event| matches!(event,
        GameEvent::ZoneChange { object, .. } if *object == b))
            .count(),
        1
    );
}

#[test]
fn pending_source_relationship_changes_information_hash() {
    let mut state = game();
    let a = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&a).unwrap().plus_counters = 1;
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    let mut other = state.clone();
    state
        .pending_triggers
        .retain(|trigger| trigger.source_id == a);
    other
        .pending_triggers
        .retain(|trigger| trigger.source_id == b);
    let hash = |state: &GameState| {
        InformationSet::from_view(&state.visible_state(0), state.card_db()).hash_value()
    };
    assert_ne!(hash(&state), hash(&other));
}

#[test]
fn canonical_occurrence_order_survives_pending_reordering() {
    let mut state = game();
    let a = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&a).unwrap().plus_counters = 1;
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    let mut reversed = state.clone();
    reversed.pending_triggers.reverse();
    let canonical = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &state,
    );
    let resolved = resolve(&canonical, &reversed, 0).unwrap();
    rules::apply_action(
        &mut state,
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
    );
    rules::apply_action(&mut reversed, &resolved);
    let source = |state: &GameState| match &state.stack.last().unwrap().source {
        mtg_gto::game::StackSource::TriggeredAbility { source_id, .. } => *source_id,
        _ => panic!("expected trigger"),
    };
    assert_eq!(source(&state), b);
    assert_eq!(source(&reversed), b);
}

#[test]
fn simultaneous_and_successive_occurrences_retain_partition() {
    let mut simultaneous = game();
    simultaneous.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let a = simultaneous.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let b = simultaneous.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let mut successive = simultaneous.clone();
    let ra = request(&simultaneous, a, ZoneType::Exile);
    let rb = request(&simultaneous, b, ZoneType::Exile);
    transition_batch(&mut simultaneous, &[ra, rb]).unwrap();
    transition_batch(&mut successive, &[ra]).unwrap();
    transition_batch(&mut successive, &[rb]).unwrap();
    assert_ne!(
        serde_json::to_value(&simultaneous.pending_triggers).unwrap(),
        serde_json::to_value(&successive.pending_triggers).unwrap()
    );
}

#[test]
fn direct_pass_cannot_bypass_mandatory_occurrence_order() {
    let mut state = subject_occurrence_game(false);
    rules::check_state_based_actions(&mut state);
    assert!(legal_actions(&state)
        .iter()
        .any(|action| matches!(action, Action::OrderTriggerOccurrences { .. })));
    let before = serde_json::to_vec(&state).unwrap();
    rules::apply_action(&mut state, &Action::PassPriority);
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn linked_followup_plan_rejects_invalid_pre_event_entries_atomically() {
    let mut state = game();
    let source = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let linked = state.create_card_in_zone(OTHER, 0, ZoneType::Exile);
    state.objects.get_mut(&linked).unwrap().exiled_by = Some(source);
    let req = request(&state, source, ZoneType::Exile);
    state.objects.get_mut(&linked).unwrap().zone_change_count = u32::MAX;
    let before = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(&mut state, &[req]).unwrap_err(),
        TransitionError::InvalidLinkedState(linked)
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);

    state.objects.get_mut(&linked).unwrap().zone_change_count = 0;
    state.players[1].exile.push(linked); // impossible duplicate occupancy
    let before = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(&mut state, &[req]).unwrap_err(),
        TransitionError::InvalidLinkedState(linked)
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);

    state.players[1].exile.clear();
    state.next_zone_event_group_id = u64::MAX;
    let before = serde_json::to_vec(&state).unwrap();
    assert_eq!(
        transition_batch(&mut state, &[req]).unwrap_err(),
        TransitionError::GroupIdExhausted
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn newly_exiled_linked_token_has_one_event_then_purges() {
    let mut state = game();
    let source = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let token = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&token).unwrap().is_token = true;
    state.objects.get_mut(&token).unwrap().exiled_by = Some(source);
    let requests = [
        request(&state, source, ZoneType::Exile),
        request(&state, token, ZoneType::Exile),
    ];
    let batch = transition_batch(&mut state, &requests).unwrap();
    assert_eq!(batch.transitions.len(), 2);
    assert!(batch.transitions[1].before.is_token);
    assert!(!state.objects.contains_key(&token));
    assert_eq!(
        state
            .pending_events
            .iter()
            .filter(|event| matches!(event,
        GameEvent::ZoneChange { object, .. } if *object == token))
            .count(),
        1
    );
}

#[test]
fn mandatory_trigger_order_rejects_unrelated_direct_actions_atomically() {
    let mut state = subject_occurrence_game(false);
    let spell = state.create_card_in_zone(SPELL, 0, ZoneType::Hand);
    let land = state.create_card_in_zone(LAND, 0, ZoneType::Hand);
    let source = state.battlefield[0];
    let activator = state.create_card_in_zone(ACTIVATOR, 0, ZoneType::Battlefield);
    rules::check_state_based_actions(&mut state);
    let before = serde_json::to_vec(&state).unwrap();
    let attempts = [
        Action::PassPriority,
        Action::CastSpell {
            object_id: spell,
            targets: vec![Target::Object(source)],
        },
        Action::PlayLand { object_id: land },
        Action::ActivateAbility {
            object_id: activator,
            ability_index: 0,
            targets: vec![],
        },
        Action::EndTurn,
        Action::OrderTriggers { ordering: vec![] },
        Action::OrderTriggerOccurrences {
            ordering: vec![0, 0, 1],
        },
    ];
    for action in attempts {
        rules::apply_action(&mut state, &action);
        assert_eq!(serde_json::to_vec(&state).unwrap(), before, "{action:?}");
    }
    let valid = legal_actions(&state)
        .into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    rules::apply_action(&mut state, &valid);
    assert!(state.pending_triggers.is_empty());
}

#[test]
fn historical_source_key_distinguishes_different_continuations() {
    let mut state = game();
    let mut db = CardDatabase::new();
    let mut watcher = database().get(WATCHER).unwrap().clone();
    watcher.subtypes = vec![Subtype("Goblin".into())];
    watcher.triggered_abilities[0].effect = Effect::BuffOtherSubtype {
        subtype: "Goblin".into(),
        amount: DynamicValue::Fixed(1),
        until_eot: true,
    };
    db.insert(watcher);
    db.insert(database().get(OTHER).unwrap().clone());
    state.card_db = Some(Arc::new(db));
    let a = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&a).unwrap().plus_counters = 1;
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    let mut other = state.clone();
    state
        .pending_triggers
        .retain(|trigger| trigger.source_id == a);
    other
        .pending_triggers
        .retain(|trigger| trigger.source_id == b);
    let hash = |game: &GameState| {
        InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
    };
    assert_ne!(hash(&state), hash(&other));
    for game in [&mut state, &mut other] {
        rules::check_state_based_actions(game);
        rules::apply_action(game, &Action::PassPriority);
        rules::apply_action(game, &Action::PassPriority);
    }
    assert_ne!(
        (state.effective_power(a), state.effective_power(b)),
        (other.effective_power(a), other.effective_power(b))
    );
}

fn source_relationship_game(extra_allocation: bool) -> GameState {
    let mut state = game();
    if extra_allocation {
        let token = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
        state.objects.get_mut(&token).unwrap().is_token = true;
        let req = request(&state, token, ZoneType::Exile);
        transition_batch(&mut state, &[req]).unwrap();
        state.pending_events.clear();
    }
    let first = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&first).unwrap().plus_counters = 1;
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    state
}

#[test]
fn source_keys_survive_raw_id_shift_and_pending_storage_permutation() {
    let a = source_relationship_game(false);
    let mut b = source_relationship_game(true);
    assert_ne!(
        a.pending_triggers[0].source_id,
        b.pending_triggers[0].source_id
    );
    b.pending_triggers.reverse();
    let info = |game: &GameState| InformationSet::from_view(&game.visible_state(0), game.card_db());
    assert_eq!(info(&a).hash_value(), info(&b).hash_value());
    let canonical = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &a,
    );
    let resolved = resolve(&canonical, &b, 0).unwrap();
    let mut a = a;
    rules::apply_action(
        &mut a,
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
    );
    rules::apply_action(&mut b, &resolved);
    let order = |game: &GameState| {
        game.stack
            .iter()
            .map(|entry| match &entry.source {
                mtg_gto::game::StackSource::TriggeredAbility { context, .. } => {
                    context
                        .zone_transition
                        .as_ref()
                        .unwrap()
                        .source_before
                        .plus_counters
                }
                _ => panic!("expected trigger"),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(order(&a), vec![1, 0]);
    assert_eq!(order(&a), order(&b));
}

#[test]
fn departed_source_key_keeps_pre_event_facts_after_mutation() {
    let mut state = game();
    let source = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&source).unwrap().plus_counters = 1;
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let requests = [
        request(&state, source, ZoneType::Exile),
        request(&state, victim, ZoneType::Exile),
    ];
    transition_batch(&mut state, &requests).unwrap();
    let ctx = state
        .pending_triggers
        .iter()
        .find(|trigger| trigger.source_id == source)
        .unwrap()
        .context
        .zone_transition
        .as_ref()
        .unwrap();
    assert_eq!(ctx.source_before.plus_counters, 1);
    let before = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &state,
    );
    state.objects.get_mut(&source).unwrap().plus_counters = 9;
    let after = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &state,
    );
    assert_eq!(before, after);
    assert!(state
        .pending_triggers
        .iter()
        .filter(|trigger| trigger.source_id == source)
        .all(|trigger| trigger
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .source_public_info(None)
            .plus_counters
            == 1));
}

fn grouped_state(simultaneous: bool, extra_allocation: bool) -> GameState {
    let mut state = game();
    if extra_allocation {
        let token = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
        state.objects.get_mut(&token).unwrap().is_token = true;
        let req = request(&state, token, ZoneType::Exile);
        transition_batch(&mut state, &[req]).unwrap();
    }
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let a = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let ra = request(&state, a, ZoneType::Exile);
    let rb = request(&state, b, ZoneType::Exile);
    if simultaneous {
        transition_batch(&mut state, &[ra, rb]).unwrap();
    } else {
        transition_batch(&mut state, &[ra]).unwrap();
        transition_batch(&mut state, &[rb]).unwrap();
    }
    state
}

#[test]
fn event_group_partition_is_owned_normalized_and_persistent() {
    let simultaneous = grouped_state(true, false);
    let mut shifted = grouped_state(true, true);
    let successive = grouped_state(false, false);
    let groups = |game: &GameState| {
        game.pending_triggers
            .iter()
            .map(|trigger| trigger.context.zone_transition.as_ref().unwrap().group_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(groups(&simultaneous), vec![0, 0]);
    assert_eq!(groups(&successive), vec![0, 1]);
    assert_eq!(groups(&shifted), vec![1, 1]);
    shifted.pending_triggers.reverse();
    let hash = |game: &GameState| {
        InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
    };
    assert_eq!(hash(&simultaneous), hash(&shifted));
    assert_ne!(hash(&simultaneous), hash(&successive));

    let json = serde_json::to_vec(&successive).unwrap();
    let binary = bincode::serialize(&successive).unwrap();
    let snapshot = successive.snapshot();
    let mut variants: Vec<GameState> = vec![
        successive.clone(),
        serde_json::from_slice(&json).unwrap(),
        bincode::deserialize(&binary).unwrap(),
    ];
    let mut restored = successive.clone();
    restored.pending_triggers.clear();
    restored.restore(snapshot);
    variants.push(restored);
    for game in &mut variants {
        game.card_db = Some(database());
        assert_eq!(groups(game), vec![0, 1]);
        assert_eq!(hash(game), hash(&successive));
        rules::check_state_based_actions(game);
        let order = legal_actions(game)
            .into_iter()
            .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
            .unwrap();
        rules::apply_action(game, &order);
        let stack_groups: Vec<_> = game
            .stack
            .iter()
            .map(|entry| match &entry.source {
                mtg_gto::game::StackSource::TriggeredAbility { context, .. } => {
                    context.zone_transition.as_ref().unwrap().group_id
                }
                _ => panic!("expected trigger"),
            })
            .collect();
        assert_eq!(stack_groups, vec![0, 1]);
        assert!(game.pending_triggers.is_empty());
    }
}

#[test]
fn event_group_context_expires_with_last_occurrence() {
    let mut state = grouped_state(true, false);
    rules::check_state_based_actions(&mut state);
    let order = legal_actions(&state)
        .into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    rules::apply_action(&mut state, &order);
    assert_eq!(
        mtg_gto::rules::transitions::live_group_ranks(&state.pending_triggers, &state.stack)
            .get(&0),
        Some(&0)
    );
    while !state.stack.is_empty() {
        rules::apply_action(&mut state, &Action::PassPriority);
        rules::apply_action(&mut state, &Action::PassPriority);
    }
    assert!(
        mtg_gto::rules::transitions::live_group_ranks(&state.pending_triggers, &state.stack)
            .is_empty()
    );
    let next = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, next, ZoneType::Exile);
    let batch = transition_batch(&mut state, &[req]).unwrap();
    assert_eq!(batch.group_id, 1);
    assert_eq!(
        mtg_gto::rules::transitions::live_group_ranks(&state.pending_triggers, &state.stack)
            .get(&1),
        Some(&0)
    );
}

fn finish_occurrence_resolution(
    game: &mut GameState,
    canonical: &mtg_gto::action::canonical::CanonicalAction,
) {
    let action = resolve(canonical, game, game.priority_player).unwrap();
    rules::apply_action(game, &action);
    assert!(game.pending_triggers.is_empty());
    for _ in 0..20 {
        if game.stack.is_empty() {
            break;
        }
        rules::apply_action(game, &Action::PassPriority);
        rules::apply_action(game, &Action::PassPriority);
    }
    assert!(game.stack.is_empty());
    rules::check_state_based_actions(game);
    assert!(game.pending_triggers.is_empty());
    assert!(game.trigger_order_resume.is_none());
}

fn assert_final_continuation_matches(mut state: GameState, expected_life_gain: i32) {
    rules::check_state_based_actions(&mut state);
    state.pending_events.clear();
    let initial_life = state.players[0].life;
    let action = legal_actions(&state)
        .into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    let canonical = canonicalize(&action, &state);
    let snapshot = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let mut variants: Vec<GameState> = vec![
        state.clone(),
        serde_json::from_slice(&json).unwrap(),
        bincode::deserialize(&binary).unwrap(),
    ];
    state.pending_triggers.clear();
    state.restore(snapshot);
    variants.push(state);
    let mut final_state = None;
    for game in &mut variants {
        game.card_db = Some(database());
        finish_occurrence_resolution(game, &canonical);
        assert_eq!(game.players[0].life, initial_life + expected_life_gain);
        let observable = serde_json::to_value(&*game).unwrap();
        if let Some(expected) = &final_state {
            assert_eq!(&observable, expected);
        } else {
            final_state = Some(observable);
        }
    }
}

#[test]
fn restored_occurrences_finish_resolution_and_settlement_identically() {
    assert_final_continuation_matches(subject_occurrence_game(false), 5);
}

#[test]
fn restored_token_occurrences_finish_after_subject_purge() {
    let mut state = game();
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let token = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&token).unwrap().is_token = true;
    let req = request(&state, token, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    assert!(!state.objects.contains_key(&token));
    assert_final_continuation_matches(state, 4);
}

#[test]
fn rereview_shared_source_partition_changes_result() {
    let mut state = game();
    let mut db = CardDatabase::new();
    let mut watcher = database().get(WATCHER).unwrap().clone();
    watcher.triggered_abilities[0].effect = Effect::DoublePowerUntilEOT {
        target: TargetSpec::NoTarget,
    };
    db.insert(watcher);
    db.insert(database().get(OTHER).unwrap().clone());
    state.card_db = Some(Arc::new(db));
    let a = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let x = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let y = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let mut split = state.clone();
    let fire = |game: &mut GameState, subject: u64, suppressed: u64| {
        let timestamp = game.new_timestamp();
        game.continuous_effects.push(ContinuousEffect {
            source_id: suppressed,
            controller: 0,
            timestamp,
            duration: Duration::UntilEndOfTurn,
            affected: AffectedObjects::SpecificIncarnation {
                object_id: suppressed,
                zone_change_count: game.objects[&suppressed].zone_change_count,
            },
            modification: LayerModification::RemoveAllAbilities,
        });
        game.invalidate_characteristics_cache();
        let req = request(game, subject, ZoneType::Exile);
        transition_batch(game, &[req]).unwrap();
        game.continuous_effects.clear();
        game.invalidate_characteristics_cache();
    };
    fire(&mut state, x, b);
    fire(&mut state, y, b);
    fire(&mut split, x, b);
    fire(&mut split, y, a);
    assert_eq!(state.pending_triggers.len(), 2);
    assert_eq!(split.pending_triggers.len(), 2);
    let hash = |game: &GameState| {
        InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
    };
    assert_ne!(hash(&state), hash(&split));
    let same_order = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &state,
    );
    let split_order = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &split,
    );
    assert_ne!(same_order, split_order);
    for game in [&mut state, &mut split] {
        rules::check_state_based_actions(game);
        let choice = legal_actions(game)
            .into_iter()
            .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. }))
            .unwrap();
        rules::apply_action(game, &choice);
        for _ in 0..2 {
            rules::apply_action(game, &Action::PassPriority);
            rules::apply_action(game, &Action::PassPriority);
        }
        game.invalidate_characteristics_cache();
    }
    assert_eq!((state.effective_power(a), state.effective_power(b)), (8, 2));
    assert_eq!((split.effective_power(a), split.effective_power(b)), (4, 4));
}

#[test]
fn rereview_later_live_source_change_survives_queue_reversal() {
    let mut state = game();
    let a = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    state.objects.get_mut(&a).unwrap().temp_power_mod = 1;
    state.invalidate_characteristics_cache();
    let mut reversed = state.clone();
    reversed.pending_triggers.reverse();
    let canonical = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &state,
    );
    let action = resolve(&canonical, &reversed, 0).unwrap();
    rules::apply_action(&mut reversed, &action);
    let source = match reversed.stack.last().unwrap().source {
        mtg_gto::game::StackSource::TriggeredAbility { source_id, .. } => source_id,
        _ => panic!("expected trigger"),
    };
    assert_eq!(source, b);
}

fn shared_source_pattern(pattern: [usize; 3], shift_ids: bool) -> GameState {
    let mut state = game();
    if shift_ids {
        let token = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
        state.objects.get_mut(&token).unwrap().is_token = true;
        let req = request(&state, token, ZoneType::Exile);
        transition_batch(&mut state, &[req]).unwrap();
    }
    let sources: Vec<_> = (0..3)
        .map(|_| state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield))
        .collect();
    let subjects: Vec<_> = (0..3)
        .map(|_| state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield))
        .collect();
    for (event, &selected) in pattern.iter().enumerate() {
        for (index, &source) in sources.iter().enumerate() {
            if index == selected {
                continue;
            }
            let timestamp = state.new_timestamp();
            state.continuous_effects.push(ContinuousEffect {
                source_id: source,
                controller: 0,
                timestamp,
                duration: Duration::UntilEndOfTurn,
                affected: AffectedObjects::SpecificIncarnation {
                    object_id: source,
                    zone_change_count: state.objects[&source].zone_change_count,
                },
                modification: LayerModification::RemoveAllAbilities,
            });
        }
        state.invalidate_characteristics_cache();
        let req = request(&state, subjects[event], ZoneType::Exile);
        transition_batch(&mut state, &[req]).unwrap();
        state.continuous_effects.clear();
        state.invalidate_characteristics_cache();
    }
    assert_eq!(state.pending_triggers.len(), 3);
    state
}

#[test]
fn exact_source_partitions_survive_raw_ids_and_pending_permutations() {
    let patterns = [[0, 0, 0], [0, 0, 1], [0, 1, 2]];
    let mut hashes = Vec::new();
    for pattern in patterns {
        let state = shared_source_pattern(pattern, false);
        let mut shifted = shared_source_pattern(pattern, true);
        shifted.pending_triggers.rotate_left(1);
        let info =
            |game: &GameState| InformationSet::from_view(&game.visible_state(0), game.card_db());
        assert_eq!(info(&state).hash_value(), info(&shifted).hash_value());
        let partition = |game: &GameState| {
            let mut ranks: Vec<_> = info(game)
                .pending_zone_triggers
                .iter()
                .map(|occurrence| occurrence.source_class_rank.unwrap())
                .collect();
            ranks.sort();
            ranks
        };
        assert_eq!(partition(&state), partition(&shifted));
        let mut counts = std::collections::HashMap::new();
        for rank in partition(&state) {
            *counts.entry(rank).or_insert(0usize) += 1;
        }
        let mut sizes: Vec<_> = counts.into_values().collect();
        sizes.sort();
        assert_eq!(
            sizes,
            match pattern {
                [0, 0, 0] => vec![3],
                [0, 0, 1] => vec![1, 2],
                _ => vec![1, 1, 1],
            }
        );
        hashes.push(info(&state).hash_value());

        let canonical = canonicalize(
            &Action::OrderTriggerOccurrences {
                ordering: vec![0, 1, 2],
            },
            &state,
        );
        let resolved = resolve(&canonical, &shifted, 0).unwrap();
        let resolved_canonical = canonicalize(&resolved, &shifted);
        assert_eq!(canonical, resolved_canonical);
    }
    assert_ne!(hashes[0], hashes[1]);
    assert_ne!(hashes[1], hashes[2]);
    assert_ne!(hashes[0], hashes[2]);
}

#[test]
fn current_live_source_facts_change_owned_occurrence_description() {
    let mut state = game();
    let source = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    let choice = Action::OrderTriggerOccurrences { ordering: vec![0] };
    let baseline = canonicalize(&choice, &state);
    let mut changed = state.clone();
    changed.objects.get_mut(&source).unwrap().temp_power_mod = 1;
    changed.invalidate_characteristics_cache();
    let modified = canonicalize(&choice, &changed);
    assert_ne!(baseline, modified);
    assert_ne!(
        InformationSet::from_view(&state.visible_state(0), state.card_db()).hash_value(),
        InformationSet::from_view(&changed.visible_state(0), changed.card_db()).hash_value()
    );

    let mut changed = state.clone();
    changed.objects.get_mut(&source).unwrap().plus_counters = 1;
    changed.invalidate_characteristics_cache();
    assert_ne!(baseline, canonicalize(&choice, &changed));

    let mut changed = state.clone();
    changed.objects.get_mut(&source).unwrap().tapped = true;
    assert_ne!(baseline, canonicalize(&choice, &changed));

    let mut changed = state.clone();
    let timestamp = changed.new_timestamp();
    changed.continuous_effects.push(ContinuousEffect {
        source_id: source,
        controller: 1,
        timestamp,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::SpecificIncarnation {
            object_id: source,
            zone_change_count: changed.objects[&source].zone_change_count,
        },
        modification: LayerModification::ChangeController(1),
    });
    changed.invalidate_characteristics_cache();
    assert_ne!(baseline, canonicalize(&choice, &changed));
}

#[test]
fn source_and_event_equality_relations_are_independent() {
    let one_source_two_events = grouped_state(false, false);
    let one_source_one_event = grouped_state(true, false);
    let mut two_sources_one_event = game();
    two_sources_one_event.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    two_sources_one_event.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let victim = two_sources_one_event.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&two_sources_one_event, victim, ZoneType::Exile);
    transition_batch(&mut two_sources_one_event, &[req]).unwrap();
    let partition = |game: &GameState| {
        let info = InformationSet::from_view(&game.visible_state(0), game.card_db());
        let groups: std::collections::HashSet<_> = info
            .pending_zone_triggers
            .iter()
            .map(|occurrence| occurrence.group_rank.unwrap())
            .collect();
        let sources: std::collections::HashSet<_> = info
            .pending_zone_triggers
            .iter()
            .map(|occurrence| occurrence.source_class_rank.unwrap())
            .collect();
        (groups.len(), sources.len())
    };
    assert_eq!(partition(&one_source_one_event), (1, 1));
    assert_eq!(partition(&one_source_two_events), (2, 1));
    assert_eq!(partition(&two_sources_one_event), (1, 2));
}

#[test]
fn shared_source_partition_continues_identically_after_restore() {
    assert_final_continuation_matches(shared_source_pattern([0, 0, 1], false), 6);
}

#[test]
fn finalreview_raw_tie_is_not_symmetric_for_existing_linked_exile_relationship() {
    use mtg_gto::card::effects::Condition;
    let make = |reverse_ids: bool| {
        let mut state = game();
        let mut db = CardDatabase::new();
        let mut watcher = database().get(WATCHER).unwrap().clone();
        watcher.subtypes = vec![Subtype("Goblin".into())];
        watcher.triggered_abilities[0].effect = Effect::Multiple(vec![
            Effect::BuffOtherSubtype {
                subtype: "Goblin".into(),
                amount: DynamicValue::Fixed(2),
                until_eot: true,
            },
            Effect::Conditional {
                condition: Condition::ControlNOrMore {
                    count: 2,
                    card_type: CardType::Creature,
                },
                if_true: Box::new(Effect::DealDamage {
                    amount: 2,
                    target: TargetSpec::EachCreature,
                }),
                if_false: None,
            },
        ]);
        db.insert(watcher);
        db.insert(database().get(OTHER).unwrap().clone());
        state.card_db = Some(Arc::new(db));
        let first = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
        let second = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
        let (a, b) = if reverse_ids {
            (second, first)
        } else {
            (first, second)
        };
        let linked = state.create_card_in_zone(OTHER, 0, ZoneType::Exile);
        state.objects.get_mut(&linked).unwrap().exiled_by = Some(a);
        let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
        state.battlefield = vec![a, b, victim];
        let req = request(&state, victim, ZoneType::Exile);
        transition_batch(&mut state, &[req]).unwrap();
        rules::check_state_based_actions(&mut state);
        (state, a, b, linked)
    };
    let (mut original, a, b, linked) = make(false);
    let (mut renamed, ra, rb, rlinked) = make(true);
    let canonical = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &original,
    );
    let action = resolve(&canonical, &renamed, 0).unwrap();
    rules::apply_action(
        &mut original,
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
    );
    rules::apply_action(&mut renamed, &action);
    for state in [&mut original, &mut renamed] {
        for _ in 0..2 {
            rules::apply_action(state, &Action::PassPriority);
            rules::apply_action(state, &Action::PassPriority);
        }
        assert!(state.pending_triggers.is_empty());
        assert!(state.stack.is_empty());
    }
    let result = (
        original.battlefield.contains(&a),
        original.battlefield.contains(&b),
        original.players[0].exile.contains(&linked),
    );
    let renamed_result = (
        renamed.battlefield.contains(&ra),
        renamed.battlefield.contains(&rb),
        renamed.players[0].exile.contains(&rlinked),
    );
    println!(
        "semantic (A alive,B alive,linked card in exile): {result:?} versus {renamed_result:?}"
    );
    assert_eq!(
        result, renamed_result,
        "raw-ID tie-break changed a complete continuation across equivalent renumbered states"
    );
}

#[test]
fn finalreview_source_subject_cross_relationship_is_preserved() {
    let mut s = game();
    let a = s.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let b = s.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let c = s.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let timestamp = s.new_timestamp();
    s.continuous_effects.push(ContinuousEffect {
        source_id: c,
        controller: 0,
        timestamp,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::SpecificIncarnation {
            object_id: c,
            zone_change_count: 0,
        },
        modification: LayerModification::RemoveAllAbilities,
    });
    s.invalidate_characteristics_cache();
    let reqs = [
        request(&s, a, ZoneType::Exile),
        request(&s, b, ZoneType::Exile),
        request(&s, c, ZoneType::Exile),
    ];
    transition_batch(&mut s, &reqs).unwrap();
    assert_eq!(s.pending_triggers.len(), 6);
    assert_eq!(s.pending_triggers[1].source_id, a);
    assert_eq!(
        s.pending_triggers[1]
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .subject
            .before
            .object
            .id,
        b
    );
    assert_eq!(
        s.pending_triggers[2]
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .subject
            .before
            .object
            .id,
        c
    );
    let first = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1, 2, 3, 4, 5],
        },
        &s,
    );
    let second = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 2, 1, 3, 4, 5],
        },
        &s,
    );
    assert_ne!(first,second,"subject B is another retained source, subject C is not; this cross-relationship disappeared");
}

fn linked_source_choice_game(
    reverse_sources: bool,
    reverse_link_allocation: bool,
    linked_cards: [Option<u64>; 2],
    shift_ids: bool,
) -> (GameState, [u64; 2]) {
    let mut state = game();
    if shift_ids {
        let token = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
        state.objects.get_mut(&token).unwrap().is_token = true;
        let req = request(&state, token, ZoneType::Exile);
        transition_batch(&mut state, &[req]).unwrap();
    }
    let first = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let second = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let sources = if reverse_sources {
        [second, first]
    } else {
        [first, second]
    };
    let order = if reverse_link_allocation {
        [1, 0]
    } else {
        [0, 1]
    };
    for index in order {
        if let Some(card_id) = linked_cards[index] {
            let linked = state.create_card_in_zone(card_id, 0, ZoneType::Exile);
            state.objects.get_mut(&linked).unwrap().exiled_by = Some(sources[index]);
        }
    }
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    (state, sources)
}

#[test]
fn linked_source_profiles_are_canonical_across_source_and_target_allocation() {
    for linked_cards in [
        [Some(OTHER), None],
        [None, Some(OTHER)],
        [Some(OTHER), Some(OTHER)],
        [Some(OTHER), Some(SPELL)],
    ] {
        let (base, sources) = linked_source_choice_game(false, false, linked_cards, false);
        let hash = InformationSet::from_view(&base.visible_state(0), base.card_db()).hash_value();
        let order: Vec<_> = sources
            .iter()
            .map(|source| {
                base.pending_triggers
                    .iter()
                    .position(|trigger| trigger.source_id == *source)
                    .unwrap()
            })
            .collect();
        let key = canonicalize(&Action::OrderTriggerOccurrences { ordering: order }, &base);
        for reverse_sources in [false, true] {
            for reverse_links in [false, true] {
                let (mut variant, variant_sources) =
                    linked_source_choice_game(reverse_sources, reverse_links, linked_cards, true);
                variant.pending_triggers.reverse();
                assert_eq!(
                    hash,
                    InformationSet::from_view(&variant.visible_state(0), variant.card_db())
                        .hash_value()
                );
                let action = resolve(&key, &variant, 0).unwrap();
                assert_eq!(key, canonicalize(&action, &variant));
                rules::apply_action(&mut variant, &action);
                let stack_sources: Vec<_> = variant
                    .stack
                    .iter()
                    .map(|entry| match &entry.source {
                        mtg_gto::game::StackSource::TriggeredAbility { source_id, .. } => {
                            *source_id
                        }
                        _ => panic!("expected trigger"),
                    })
                    .collect();
                if linked_cards[0] != linked_cards[1] {
                    assert_eq!(stack_sources, variant_sources);
                }
            }
        }
    }
}

#[test]
fn linked_target_public_state_distinguishes_otherwise_identical_sources() {
    let (mut state, sources) =
        linked_source_choice_game(false, false, [Some(OTHER), Some(OTHER)], false);
    let initial = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &state,
    );
    let linked = state.players[0]
        .exile
        .iter()
        .copied()
        .find(|id| state.objects[id].exiled_by == Some(sources[0]))
        .unwrap();
    state.objects.get_mut(&linked).unwrap().plus_counters = 1;
    let changed = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 1],
        },
        &state,
    );
    assert_ne!(initial, changed);
}

#[test]
fn historical_lki_omits_live_only_damage_and_sickness() {
    let mut state = game();
    let source = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&source).unwrap().damage_marked = 1;
    state.objects.get_mut(&source).unwrap().summoning_sick = false;
    let victim = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, victim, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    let historical = serde_json::to_value(
        &state.pending_triggers[0]
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .source_before,
    )
    .unwrap();
    assert!(historical.get("damage_marked").is_none());
    assert!(historical.get("summoning_sick").is_none());
    let info = InformationSet::from_view(&state.visible_state(0), state.card_db());
    let live = &info.pending_zone_triggers[0].source.as_ref().unwrap().live;
    assert_eq!(live.as_ref().unwrap().damage_marked, 1);
    assert!(!live.as_ref().unwrap().summoning_sick);
}

fn retained_subject_relationship_game() -> GameState {
    let mut state = game();
    let a = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let external = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let timestamp = state.new_timestamp();
    state.continuous_effects.push(ContinuousEffect {
        source_id: external,
        controller: 0,
        timestamp,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::SpecificIncarnation {
            object_id: external,
            zone_change_count: state.objects[&external].zone_change_count,
        },
        modification: LayerModification::RemoveAllAbilities,
    });
    state.invalidate_characteristics_cache();
    let requests = [
        request(&state, a, ZoneType::Exile),
        request(&state, b, ZoneType::Exile),
        request(&state, external, ZoneType::Exile),
    ];
    transition_batch(&mut state, &requests).unwrap();
    assert_eq!(state.pending_triggers.len(), 6);
    state
}

#[test]
fn self_other_retained_and_external_subjects_are_distinct_and_persistent() {
    use mtg_gto::action::canonical::CanonicalAction;
    use mtg_gto::rules::transitions::SubjectSourceRelation;
    let state = retained_subject_relationship_game();
    let ordering: Vec<_> = (0..6).collect();
    let key = canonicalize(&Action::OrderTriggerOccurrences { ordering }, &state);
    let CanonicalAction::OrderTriggerOccurrences { occurrences } = &key else {
        panic!("expected occurrence ordering");
    };
    let relations: Vec<_> = occurrences
        .iter()
        .map(|item| item.subject_source_relation.unwrap())
        .collect();
    assert_eq!(relations[0], SubjectSourceRelation::SelfSource);
    assert!(matches!(
        relations[1],
        SubjectSourceRelation::OtherRetainedSource(_)
    ));
    assert_eq!(relations[2], SubjectSourceRelation::External);
    assert!(matches!(
        relations[3],
        SubjectSourceRelation::OtherRetainedSource(_)
    ));
    assert_eq!(relations[4], SubjectSourceRelation::SelfSource);
    assert_eq!(relations[5], SubjectSourceRelation::External);
    assert!(occurrences.iter().all(|item| item.group_rank == Some(0)));
    let swap = canonicalize(
        &Action::OrderTriggerOccurrences {
            ordering: vec![0, 2, 1, 3, 4, 5],
        },
        &state,
    );
    assert_ne!(key, swap);

    let mut reversed = state.clone();
    reversed.pending_triggers.reverse();
    let restored_action = resolve(&key, &reversed, 0).unwrap();
    assert_eq!(key, canonicalize(&restored_action, &reversed));
    let info = |game: &GameState| {
        InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
    };
    assert_eq!(info(&state), info(&reversed));
    assert_final_continuation_matches(state, 12);
}

#[test]
fn linked_asymmetry_preserves_all_three_source_sharing_partitions() {
    let mut hashes = Vec::new();
    for pattern in [[0, 0, 0], [0, 0, 1], [0, 1, 2]] {
        let mut base = shared_source_pattern(pattern, false);
        let mut shifted = shared_source_pattern(pattern, true);
        for state in [&mut base, &mut shifted] {
            let first_source = state.battlefield[0];
            let linked = state.create_card_in_zone(OTHER, 0, ZoneType::Exile);
            state.objects.get_mut(&linked).unwrap().exiled_by = Some(first_source);
        }
        shifted.pending_triggers.rotate_left(1);
        let hash = |game: &GameState| {
            InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
        };
        assert_eq!(hash(&base), hash(&shifted));
        let key = canonicalize(
            &Action::OrderTriggerOccurrences {
                ordering: vec![0, 1, 2],
            },
            &base,
        );
        let reconstructed = resolve(&key, &shifted, 0).unwrap();
        assert_eq!(key, canonicalize(&reconstructed, &shifted));
        hashes.push(hash(&base));
    }
    assert_ne!(hashes[0], hashes[1]);
    assert_ne!(hashes[1], hashes[2]);
    assert_ne!(hashes[0], hashes[2]);
}

#[test]
fn two_abilities_from_one_source_for_one_subject_keep_one_relationship() {
    use mtg_gto::rules::transitions::SubjectSourceRelation;
    let mut state = game();
    let mut db = CardDatabase::new();
    let mut watcher = database().get(WATCHER).unwrap().clone();
    watcher
        .triggered_abilities
        .push(watcher.triggered_abilities[0].clone());
    db.insert(watcher);
    db.insert(database().get(OTHER).unwrap().clone());
    state.card_db = Some(Arc::new(db));
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let subject = state.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&state, subject, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    let info = InformationSet::from_view(&state.visible_state(0), state.card_db());
    assert_eq!(info.pending_zone_triggers.len(), 2);
    assert_eq!(
        info.pending_zone_triggers[0].source_class_rank,
        info.pending_zone_triggers[1].source_class_rank
    );
    assert_eq!(
        info.pending_zone_triggers[0].group_rank,
        info.pending_zone_triggers[1].group_rank
    );
    assert!(info
        .pending_zone_triggers
        .iter()
        .all(|item| item.subject_source_relation == Some(SubjectSourceRelation::External)));
    assert_ne!(
        info.pending_zone_triggers[0].ability_index,
        info.pending_zone_triggers[1].ability_index
    );
}

#[test]
fn linked_source_relationship_survives_clone_snapshot_json_and_bincode() {
    for linked_cards in [[Some(OTHER), None], [Some(OTHER), Some(SPELL)]] {
        let (state, _) = linked_source_choice_game(false, false, linked_cards, false);
        assert_final_continuation_matches(state, 4);
    }
}

#[test]
fn finalreview_all_three_occurrence_permutations_and_departed_patterns() {
    let permutations = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for pattern in [[0, 0, 0], [0, 0, 1], [0, 1, 2]] {
        for departed in [false, true] {
            let mut base = shared_source_pattern(pattern, false);
            let mut shifted = shared_source_pattern(pattern, true);
            if departed {
                for s in [&mut base, &mut shifted] {
                    let ids = s.battlefield.clone();
                    for id in ids {
                        s.move_object(id, ZoneType::Battlefield, ZoneType::Exile);
                    }
                }
            }
            let hash = |s: &GameState| {
                InformationSet::from_view(&s.visible_state(0), s.card_db()).hash_value()
            };
            for order in permutations {
                let mut variant = shifted.clone();
                variant.pending_triggers = order
                    .iter()
                    .map(|&i| shifted.pending_triggers[i].clone())
                    .collect();
                assert_eq!(hash(&base), hash(&variant));
                for action_order in permutations {
                    let canonical = canonicalize(
                        &Action::OrderTriggerOccurrences {
                            ordering: action_order.to_vec(),
                        },
                        &base,
                    );
                    let action = resolve(&canonical, &variant, 0).unwrap();
                    assert_eq!(canonical, canonicalize(&action, &variant));
                }
            }
        }
    }
}

#[test]
fn finalreview_old_and_blinked_source_remain_separate_through_restore() {
    let mut s = game();
    let source = s.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let x = s.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&s, x, ZoneType::Exile);
    transition_batch(&mut s, &[req]).unwrap();
    s.move_object(source, ZoneType::Battlefield, ZoneType::Exile);
    s.move_object(source, ZoneType::Exile, ZoneType::Battlefield);
    let y = s.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
    let req = request(&s, y, ZoneType::Exile);
    transition_batch(&mut s, &[req]).unwrap();
    let info = InformationSet::from_view(&s.visible_state(0), s.card_db());
    assert_eq!(
        info.pending_zone_triggers
            .iter()
            .filter(|o| o.source.as_ref().unwrap().live.is_some())
            .count(),
        1
    );
    let ranks: std::collections::HashSet<_> = info
        .pending_zone_triggers
        .iter()
        .map(|o| o.source_class_rank)
        .collect();
    assert_eq!(ranks.len(), 2);
    assert_final_continuation_matches(s, 4);
}

fn incoming_stacked_subject_game(
    reverse: bool,
    inverse_stack: bool,
    distinguish_a: bool,
    shift_ids: bool,
) -> (GameState, u64, u64) {
    let mut s = game();
    if shift_ids {
        let token = s.create_card_in_zone(OTHER, 0, ZoneType::Battlefield);
        s.objects.get_mut(&token).unwrap().is_token = true;
        let req = request(&s, token, ZoneType::Exile);
        transition_batch(&mut s, &[req]).unwrap();
    }
    let watcher = s.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = s.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let second = s.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let (a, b) = if reverse {
        (second, first)
    } else {
        (first, second)
    };
    if distinguish_a {
        s.objects.get_mut(&a).unwrap().plus_counters = 1;
    }
    s.battlefield = vec![watcher, a, b];
    let reqs = [
        request(&s, a, ZoneType::Exile),
        request(&s, b, ZoneType::Exile),
    ];
    transition_batch(&mut s, &reqs).unwrap();
    rules::check_state_based_actions(&mut s);
    assert_eq!(s.priority_player, 0);
    let stack_subjects = if inverse_stack { [b, a] } else { [a, b] };
    let order = stack_subjects
        .iter()
        .map(|&subject| {
            s.pending_triggers
                .iter()
                .position(|t| {
                    t.source_id == watcher
                        && t.context
                            .zone_transition
                            .as_ref()
                            .unwrap()
                            .subject
                            .before
                            .object
                            .id
                            == subject
                })
                .unwrap()
        })
        .collect();
    rules::apply_action(&mut s, &Action::OrderTriggerOccurrences { ordering: order });
    assert_eq!(s.priority_player, 1);
    assert_eq!(s.pending_triggers.len(), 2);
    assert_eq!(s.stack.len(), 2);
    (s, a, b)
}

#[test]
fn closure_incoming_stacked_subject_edges_break_raw_id_symmetry() {
    let (base, a, b) = incoming_stacked_subject_game(false, false, false, false);
    let (other, oa, ob) = incoming_stacked_subject_game(true, false, false, true);
    let hash =
        |s: &GameState| InformationSet::from_view(&s.visible_state(0), s.card_db()).hash_value();
    let key_for = |s: &GameState, a: u64, b: u64| {
        canonicalize(
            &Action::OrderTriggerOccurrences {
                ordering: vec![a, b]
                    .iter()
                    .map(|id| {
                        s.pending_triggers
                            .iter()
                            .position(|t| t.source_id == *id)
                            .unwrap()
                    })
                    .collect(),
            },
            s,
        )
    };
    let key = key_for(&base, a, b);
    let action = resolve(&key, &other, 1).unwrap();
    let Action::OrderTriggerOccurrences { ordering } = action else {
        panic!()
    };
    let reconstructed: Vec<_> = ordering
        .iter()
        .map(|&i| other.pending_triggers[i].source_id)
        .collect();
    assert_eq!(hash(&base),hash(&other),"raw-ID renumbering changed information with identical represented incoming subject/stack relationships");
    assert_eq!(reconstructed, vec![oa, ob]);

    let mut reordered = other.clone();
    reordered.pending_triggers.reverse();
    reordered.stack[0].id += 1000;
    reordered.stack[1].id += 1000;
    assert_eq!(hash(&base), hash(&reordered));
    let reordered_action = resolve(&key, &reordered, 1).unwrap();
    assert_eq!(key, canonicalize(&reordered_action, &reordered));
    for (mut state, expected, action) in [
        (base.clone(), vec![a, b], resolve(&key, &base, 1).unwrap()),
        (
            other,
            vec![oa, ob],
            Action::OrderTriggerOccurrences { ordering },
        ),
        (reordered, vec![oa, ob], reordered_action),
    ] {
        let initial_life = (state.players[0].life, state.players[1].life);
        let Action::OrderTriggerOccurrences { ordering } = &action else {
            panic!()
        };
        let sources: Vec<_> = ordering
            .iter()
            .map(|&slot| state.pending_triggers[slot].source_id)
            .collect();
        assert_eq!(sources, expected);
        rules::apply_action(&mut state, &action);
        assert!(state.pending_triggers.is_empty());
        assert_eq!(state.stack.len(), 4);
        for _ in 0..4 {
            rules::apply_action(&mut state, &Action::PassPriority);
            rules::apply_action(&mut state, &Action::PassPriority);
        }
        assert!(state.stack.is_empty());
        assert_eq!(
            (state.players[0].life, state.players[1].life),
            (initial_life.0 + 4, initial_life.1 + 2)
        );
    }
    assert_final_continuation_matches(base, 4);
}

#[test]
fn incoming_stack_order_and_raw_stack_ids_are_independent() {
    let hash = |game: &GameState| {
        InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
    };
    let mut order_hashes = Vec::new();
    for inverse in [false, true] {
        let (base, a, b) = incoming_stacked_subject_game(false, inverse, false, false);
        let (mut shifted, sa, sb) = incoming_stacked_subject_game(true, inverse, false, true);
        shifted.pending_triggers.reverse();
        for (index, entry) in shifted.stack.iter_mut().enumerate() {
            entry.id = 1000 + index as u64;
        }
        assert_eq!(hash(&base), hash(&shifted));
        let order: Vec<_> = [a, b]
            .iter()
            .map(|source| {
                base.pending_triggers
                    .iter()
                    .position(|trigger| trigger.source_id == *source)
                    .unwrap()
            })
            .collect();
        let key = canonicalize(&Action::OrderTriggerOccurrences { ordering: order }, &base);
        let reconstructed = resolve(&key, &shifted, 1).unwrap();
        assert_eq!(key, canonicalize(&reconstructed, &shifted));
        let Action::OrderTriggerOccurrences { ordering } = reconstructed else {
            panic!()
        };
        assert_eq!(
            ordering
                .iter()
                .map(|&slot| shifted.pending_triggers[slot].source_id)
                .collect::<Vec<_>>(),
            vec![sa, sb]
        );
        order_hashes.push(hash(&base));
    }
    // A and B are symmetric here, so reversing both stacked subjects does
    // not create an observable difference. A real pre-event distinction does.
    assert_eq!(order_hashes[0], order_hashes[1]);
    let anchored = incoming_stacked_subject_game(false, false, true, false).0;
    let anchored_inverse = incoming_stacked_subject_game(false, true, true, false).0;
    assert_ne!(hash(&anchored), hash(&anchored_inverse));
}

fn incoming_pending_subject_game(reverse: bool) -> (GameState, u64, u64) {
    let mut state = game();
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let second = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let (a, b) = if reverse {
        (second, first)
    } else {
        (first, second)
    };
    state.battlefield = vec![state.battlefield[0], a, b];
    let requests = [
        request(&state, a, ZoneType::Exile),
        request(&state, b, ZoneType::Exile),
    ];
    transition_batch(&mut state, &requests).unwrap();
    assert_eq!(state.pending_triggers.len(), 4);
    assert!(state.stack.is_empty());
    (state, a, b)
}

#[test]
fn incoming_pending_edges_are_an_unordered_multiset() {
    let (base, _, _) = incoming_pending_subject_game(false);
    let (other, _, _) = incoming_pending_subject_game(true);
    let hash = |game: &GameState| {
        InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
    };
    let base_hash = hash(&base);
    assert_eq!(base_hash, hash(&other));
    for order in [[0, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2], [2, 0, 3, 1]] {
        let mut permuted = other.clone();
        permuted.pending_triggers = order
            .iter()
            .map(|&index| other.pending_triggers[index].clone())
            .collect();
        assert_eq!(base_hash, hash(&permuted));
    }
}

fn multiple_incoming_sources_game(split_subjects: bool) -> GameState {
    let mut state = game();
    let c = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let d = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let a = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let b = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let requests = [
        request(&state, a, ZoneType::Exile),
        request(&state, b, ZoneType::Exile),
    ];
    transition_batch(&mut state, &requests).unwrap();
    // Select a synthetic subset of the real occurrences to model two
    // independently matching observer predicates on one simultaneous event.
    state.pending_triggers.retain(|trigger| {
        if trigger.source_id == c {
            trigger
                .context
                .zone_transition
                .as_ref()
                .unwrap()
                .subject
                .before
                .object
                .id
                == a
        } else if trigger.source_id == d {
            trigger
                .context
                .zone_transition
                .as_ref()
                .unwrap()
                .subject
                .before
                .object
                .id
                == if split_subjects { b } else { a }
        } else {
            true
        }
    });
    assert_eq!(state.pending_triggers.len(), 4);
    state
}

#[test]
fn incoming_subject_partition_distinguishes_two_to_one_from_one_to_each() {
    let together = multiple_incoming_sources_game(false);
    let split = multiple_incoming_sources_game(true);
    let hash = |game: &GameState| {
        InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
    };
    assert_ne!(hash(&together), hash(&split));
    let mut reversed = together.clone();
    reversed.pending_triggers.reverse();
    assert_eq!(hash(&together), hash(&reversed));
    let mut reversed = split.clone();
    reversed.pending_triggers.reverse();
    assert_eq!(hash(&split), hash(&reversed));
}

fn incoming_successive_subject_game(reverse: bool) -> (GameState, u64, u64) {
    let mut state = game();
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let second = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let (a, b) = if reverse {
        (second, first)
    } else {
        (first, second)
    };
    state.battlefield = vec![watcher, a, b];
    for subject in [a, b] {
        let req = request(&state, subject, ZoneType::Exile);
        transition_batch(&mut state, &[req]).unwrap();
    }
    rules::check_state_based_actions(&mut state);
    let ordering: Vec<_> = [a, b]
        .iter()
        .map(|subject| {
            state
                .pending_triggers
                .iter()
                .position(|trigger| {
                    trigger.source_id == watcher
                        && trigger
                            .context
                            .zone_transition
                            .as_ref()
                            .unwrap()
                            .subject
                            .before
                            .object
                            .id
                            == *subject
                })
                .unwrap()
        })
        .collect();
    rules::apply_action(&mut state, &Action::OrderTriggerOccurrences { ordering });
    assert_eq!(state.stack.len(), 2);
    assert_eq!(state.pending_triggers.len(), 2);
    (state, a, b)
}

#[test]
fn incoming_edges_preserve_successive_event_groups_independently() {
    let (simultaneous, _, _) = incoming_stacked_subject_game(false, false, false, false);
    let (successive, a, b) = incoming_successive_subject_game(false);
    let (mut renamed, ra, rb) = incoming_successive_subject_game(true);
    renamed.pending_triggers.reverse();
    let hash = |game: &GameState| {
        InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
    };
    assert_ne!(hash(&simultaneous), hash(&successive));
    assert_eq!(hash(&successive), hash(&renamed));
    let ordering: Vec<_> = [a, b]
        .iter()
        .map(|source| {
            successive
                .pending_triggers
                .iter()
                .position(|trigger| trigger.source_id == *source)
                .unwrap()
        })
        .collect();
    let key = canonicalize(&Action::OrderTriggerOccurrences { ordering }, &successive);
    let action = resolve(&key, &renamed, 1).unwrap();
    let Action::OrderTriggerOccurrences { ordering } = action else {
        panic!()
    };
    assert_eq!(
        ordering
            .iter()
            .map(|&slot| renamed.pending_triggers[slot].source_id)
            .collect::<Vec<_>>(),
        vec![ra, rb]
    );
}

#[test]
fn incoming_edges_coexist_with_current_links_and_blinked_origin() {
    let hash = |game: &GameState| {
        InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value()
    };
    let (mut base, a, b) = incoming_stacked_subject_game(false, false, false, false);
    let (mut renamed, ra, rb) = incoming_stacked_subject_game(true, false, false, true);
    let unlinked_hash = hash(&base);
    for game in [&mut base, &mut renamed] {
        let watcher = game.battlefield[0];
        let linked = game.create_card_in_zone(OTHER, 0, ZoneType::Exile);
        game.objects.get_mut(&linked).unwrap().exiled_by = Some(watcher);
    }
    assert_eq!(hash(&base), hash(&renamed));
    assert_ne!(hash(&base), unlinked_hash);
    let ordering: Vec<_> = [a, b]
        .iter()
        .map(|source| {
            base.pending_triggers
                .iter()
                .position(|trigger| trigger.source_id == *source)
                .unwrap()
        })
        .collect();
    let key = canonicalize(&Action::OrderTriggerOccurrences { ordering }, &base);
    let action = resolve(&key, &renamed, 1).unwrap();
    let Action::OrderTriggerOccurrences { ordering } = action else {
        panic!()
    };
    assert_eq!(
        ordering
            .iter()
            .map(|&slot| renamed.pending_triggers[slot].source_id)
            .collect::<Vec<_>>(),
        vec![ra, rb]
    );

    for game in [&mut base, &mut renamed] {
        let watcher = game.battlefield[0];
        game.move_object(watcher, ZoneType::Battlefield, ZoneType::Exile);
        game.move_object(watcher, ZoneType::Exile, ZoneType::Battlefield);
    }
    assert_eq!(hash(&base), hash(&renamed));
    let action = resolve(&key, &renamed, 1).unwrap();
    assert_eq!(key, canonicalize(&action, &renamed));
    assert_final_continuation_matches(base, 4);
    assert_final_continuation_matches(renamed, 4);
}

#[test]
fn incoming_closure_paired_symmetric_sources_survive_independent_id_swap() {
    let make = |reverse_subject_ids: bool| {
        let mut s = game();
        let c = s.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
        let d = s.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
        let first = s.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
        let second = s.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
        let (a, b) = if reverse_subject_ids {
            (second, first)
        } else {
            (first, second)
        };
        s.battlefield = vec![c, d, a, b];
        let reqs = [
            request(&s, a, ZoneType::Exile),
            request(&s, b, ZoneType::Exile),
        ];
        transition_batch(&mut s, &reqs).unwrap();
        // Same represented synthetic matching pattern as the permanent
        // multiple_incoming_sources_game(true) fixture: C->A, D->B.
        s.pending_triggers.retain(|t| {
            let subject = t
                .context
                .zone_transition
                .as_ref()
                .unwrap()
                .subject
                .before
                .object
                .id;
            if t.source_id == c {
                subject == a
            } else if t.source_id == d {
                subject == b
            } else {
                true
            }
        });
        rules::check_state_based_actions(&mut s);
        assert_eq!(s.pending_triggers.len(), 4);
        assert_eq!(s.priority_player, 0);
        (s, c, d)
    };
    let (base, c, d) = make(false);
    let (mut renamed, _, _) = make(true);
    renamed.pending_triggers.reverse();
    let hash =
        |s: &GameState| InformationSet::from_view(&s.visible_state(0), s.card_db()).hash_value();
    let order = vec![c, d]
        .iter()
        .map(|id| {
            base.pending_triggers
                .iter()
                .position(|t| t.source_id == *id)
                .unwrap()
        })
        .collect();
    let key = canonicalize(&Action::OrderTriggerOccurrences { ordering: order }, &base);
    let reconstructed = resolve(&key, &renamed, 0);
    println!(
        "paired graph hashes={} vs {}; canonical resolves={}",
        hash(&base),
        hash(&renamed),
        reconstructed.is_some()
    );
    assert!(reconstructed.is_some(),"valid canonical choice could not reconstruct across a pure ID swap in the represented C->A,D->B fixture");
    assert_eq!(hash(&base), hash(&renamed));
}

// Exact normalization fixtures use only the approved retained source/subject
// domain. Every record originates from a committed synthetic departure batch.
fn exact_graph(edges: &[(usize, usize)], allocation: &[usize]) -> GameState {
    let mut state = game();
    let n = allocation.len();
    let mut ids = vec![0; n];
    for &semantic in allocation {
        ids[semantic] = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    }
    let requests: Vec<_> = ids
        .iter()
        .map(|&id| request(&state, id, ZoneType::Exile))
        .collect();
    transition_batch(&mut state, &requests).unwrap();
    state.pending_triggers.retain(|trigger| {
        let zone = trigger.context.zone_transition.as_ref().unwrap();
        let source = ids
            .iter()
            .position(|&id| id == zone.source_before.object.id)
            .unwrap();
        let subject = ids
            .iter()
            .position(|&id| id == zone.subject.before.object.id)
            .unwrap();
        edges.contains(&(source, subject))
    });
    state.pending_events.clear();
    state
}

fn exact_normalization(
    state: &GameState,
    oracle: bool,
) -> mtg_gto::rules::transitions::RetainedNormalization {
    use mtg_gto::rules::transitions::{exhaustive_retained_oracle, normalize_retained};
    let view = state.visible_state(0);
    let source = |context: &mtg_gto::game::TriggerContext| {
        let zone = context.zone_transition.as_ref().unwrap();
        let exact = zone.source_before.object;
        let live = (context.source_generation == exact.generation
            && state
                .objects
                .get(&exact.id)
                .is_some_and(|inst| inst.zone_change_count == exact.generation))
        .then(|| view.zone_live_sources.get(&exact.id))
        .flatten();
        zone.source_public_info(live)
    };
    let generation = |id| state.objects.get(&id).map(|inst| inst.zone_change_count);
    if oracle {
        exhaustive_retained_oracle(&state.pending_triggers, &state.stack, source, generation)
    } else {
        normalize_retained(&state.pending_triggers, &state.stack, source, generation)
    }
}

#[test]
fn production_destroy_watcher_normalizer_diagnostics() {
    use mtg_gto::rules::transitions::{destroy_batch, ExactObjectRef};
    // Distinct authored-like watcher identities, with the same generic death
    // predicate, remain on the battlefield through one actual mass event.
    for (watchers, deaths) in [(1, 10), (4, 10), (8, 10), (11, 10), (11, 30)] {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 997_000, name: "Plain victim".into(),
            card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
            ..Default::default() });
        for index in 0..watchers {
            db.insert(CardDef { id: 997_100 + index as u64,
                name: format!("Distinct death observer {index}"),
                card_types: vec![CardType::Creature],
                keywords: vec![KeywordAbility::Indestructible],
                power: Some(2), toughness: Some(2),
                triggered_abilities: vec![TriggeredAbility {
                    trigger: TriggerCondition::ACreatureDies,
                    effect: Effect::GainLife { amount: 1 },
                    description: "death".into(),
                }], ..Default::default() });
        }
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        for index in 0..watchers {
            state.create_card_in_zone(997_100 + index as u64, 0, ZoneType::Battlefield);
        }
        let subjects: Vec<_> = (0..deaths).map(|_| {
            let id = state.create_card_in_zone(997_000, 1, ZoneType::Battlefield);
            ExactObjectRef { id, generation: state.objects[&id].zone_change_count }
        }).collect();
        let batch = destroy_batch(&mut state, &subjects).unwrap().unwrap();
        assert_eq!(batch.transitions.len(), deaths);
        assert_eq!(state.pending_triggers.len(), watchers * deaths);
        let result = exact_normalization(&state, false);
        println!("production destroy watchers={watchers} deaths={deaths} sources={} occurrences={} components={:?} tied={:?} nodes={} calls={} candidates={} elapsed_ns={}",
            result.source_ranks.len(), state.pending_triggers.len(),
            result.stats.component_sizes, result.stats.tied_cell_sizes,
            result.stats.search_nodes, result.stats.normalization_calls,
            result.stats.encoded_candidates, result.stats.elapsed_nanos);
    }
}

#[test]
fn production_destroy_member_order_and_raw_ids_keep_retained_encoding() {
    use mtg_gto::rules::transitions::{destroy_batch, ExactObjectRef};
    fn run(shift_ids: bool, reverse_members: bool) -> GameState {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 997_300, name: "Victim".into(),
            card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
            ..Default::default() });
        db.insert(CardDef { id: 997_301, name: "Watcher".into(),
            card_types: vec![CardType::Creature],
            keywords: vec![KeywordAbility::Indestructible],
            power: Some(2), toughness: Some(2),
            triggered_abilities: vec![TriggeredAbility {
                trigger: TriggerCondition::ACreatureDies,
                effect: Effect::GainLife { amount: 1 }, description: "death".into(),
            }], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        if shift_ids {
            let extra = state.create_card_in_zone(997_300, 1, ZoneType::Battlefield);
            state.objects.get_mut(&extra).unwrap().is_token = true;
            state.move_object(extra, ZoneType::Battlefield, ZoneType::Exile);
            state.pending_events.clear();
        }
        state.create_card_in_zone(997_301, 0, ZoneType::Battlefield);
        let mut subjects: Vec<_> = (0..3).map(|_| {
            let id = state.create_card_in_zone(997_300, 1, ZoneType::Battlefield);
            ExactObjectRef { id, generation: state.objects[&id].zone_change_count }
        }).collect();
        if reverse_members { subjects.reverse(); }
        destroy_batch(&mut state, &subjects).unwrap().unwrap();
        if reverse_members { state.pending_triggers.reverse(); }
        rules::check_state_based_actions(&mut state);
        state
    }
    let a = run(false, false);
    let b = run(true, true);
    assert_eq!(exact_normalization(&a, false).encoding,
        exact_normalization(&b, false).encoding);
    let information = |state: &GameState| InformationSet::from_view(
        &state.visible_state(0), state.card_db()).hash_value();
    assert_eq!(information(&a), information(&b));
    let choice = legal_actions(&a).into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. })).unwrap();
    let key = canonicalize(&choice, &a);
    let reconstructed = resolve(&key, &b, 0).unwrap();
    assert_eq!(canonicalize(&reconstructed, &b), key);
}

#[test]
#[ignore = "known synthetic tied eight-source control; run separately with an external diagnostic time budget"]
fn synthetic_tied_eight_source_normalizer_control() {
    let edges: Vec<_> = (0..8).map(|index| (index, (index + 1) % 8)).collect();
    let state = exact_graph(&edges, &(0..8).collect::<Vec<_>>());
    let result = exact_normalization(&state, false);
    println!("synthetic tied cycle: sources={} occurrences={} components={:?} tied={:?} nodes={} calls={} candidates={} elapsed_ns={}",
        result.source_ranks.len(), state.pending_triggers.len(),
        result.stats.component_sizes, result.stats.tied_cell_sizes,
        result.stats.search_nodes, result.stats.normalization_calls,
        result.stats.encoded_candidates, result.stats.elapsed_nanos);
}

fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn visit(prefix: &mut Vec<usize>, remaining: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if remaining.is_empty() {
            out.push(prefix.clone());
            return;
        }
        for index in 0..remaining.len() {
            let vertex = remaining.remove(index);
            prefix.push(vertex);
            visit(prefix, remaining, out);
            prefix.pop();
            remaining.insert(index, vertex);
        }
    }
    let mut out = Vec::new();
    visit(&mut Vec::new(), &mut (0..n).collect(), &mut out);
    out
}

#[test]
fn exact_normalizer_matches_independent_oracle_for_generated_retained_graphs() {
    // Deterministic fixture generator, independent of production RNG.
    let mut seed = 19u64;
    for n in 1..=5 {
        for case in 0..24 {
            let mut edges = Vec::new();
            for source in 0..n {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                edges.push((source, (seed >> 32) as usize % n));
                if case % 4 == 0 {
                    let other = (source + 1) % n;
                    if !edges.contains(&(source, other)) {
                        edges.push((source, other));
                    }
                }
            }
            let mut state = exact_graph(&edges, &(0..n).collect::<Vec<_>>());
            if case % 3 == 0 {
                // Ordered effect shape is part of BOTH refinement and the
                // complete encoding, never an unencoded label-only fact.
                state.pending_triggers[0].context.effect = Effect::Multiple(vec![
                    Effect::GainLife { amount: 1 },
                    Effect::GainLife { amount: 2 },
                ]);
            }
            if case % 5 == 0 {
                state.pending_triggers[0].ability_index = 1;
            }
            let production = exact_normalization(&state, false);
            let oracle = exact_normalization(&state, true);
            assert_eq!(
                production.encoding, oracle.encoding,
                "n={n} case={case} edges={edges:?}"
            );
            state.pending_triggers.reverse();
            assert_eq!(
                production.encoding,
                exact_normalization(&state, false).encoding
            );
            assert_eq!(production.source_ranks.len(), n);
        }
    }
}

#[test]
fn exact_paired_components_all_id_swaps_and_pending_permutations_preserve_action_sets() {
    use std::collections::HashSet;
    let edges = [(0, 0), (1, 1), (2, 0), (3, 1)];
    let mut base = exact_graph(&edges, &[0, 1, 2, 3]);
    // Two distinct semantic source cells, correlated by incoming edges.
    for trigger in &mut base.pending_triggers {
        if !trigger
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .source_was_subject
        {
            trigger.context.source_card_id = SUBJECT;
            trigger
                .context
                .zone_transition
                .as_mut()
                .unwrap()
                .source_before
                .card_id = SUBJECT;
        }
    }
    rules::check_state_based_actions(&mut base);
    let baseline = exact_normalization(&base, false);
    let actions: Vec<_> = legal_actions(&base)
        .into_iter()
        .filter(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
        .collect();
    assert_eq!(actions.len(), 24);
    let keyset: HashSet<_> =
        mtg_gto::action::canonical::canonicalize_actions(&actions, &base, &baseline)
            .into_iter()
            .collect();
    for allocation in [
        &[0, 1, 2, 3][..],
        &[1, 0, 2, 3],
        &[0, 1, 3, 2],
        &[1, 0, 3, 2],
    ] {
        let mut renamed = exact_graph(&edges, allocation);
        for trigger in &mut renamed.pending_triggers {
            if !trigger
                .context
                .zone_transition
                .as_ref()
                .unwrap()
                .source_was_subject
            {
                trigger.context.source_card_id = SUBJECT;
                trigger
                    .context
                    .zone_transition
                    .as_mut()
                    .unwrap()
                    .source_before
                    .card_id = SUBJECT;
            }
        }
        rules::check_state_based_actions(&mut renamed);
        let original = renamed.pending_triggers.clone();
        for order in permutations(4) {
            renamed.pending_triggers = order.iter().map(|&i| original[i].clone()).collect();
            let normalized = exact_normalization(&renamed, false);
            assert_eq!(normalized.encoding, baseline.encoding);
            assert_eq!(
                normalized.encoding,
                exact_normalization(&renamed, true).encoding
            );
            assert_eq!(
                keyset,
                mtg_gto::action::canonical::canonicalize_actions(
                    &legal_actions(&renamed)
                        .into_iter()
                        .filter(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
                        .collect::<Vec<_>>(),
                    &renamed,
                    &normalized
                )
                .into_iter()
                .collect()
            );
            for key in &keyset {
                let action = resolve(key, &renamed, 0).expect("joint witness must reconstruct");
                assert_eq!(canonicalize(&action, &renamed), *key);
            }
        }
    }
}

#[test]
fn exact_matching_crossed_pairs_cycles_and_fan_graphs_keep_complete_incidence() {
    let paired = exact_graph(&[(0, 0), (1, 1), (2, 0), (3, 1)], &[0, 1, 2, 3]);
    let crossed = exact_graph(&[(0, 0), (1, 1), (2, 1), (3, 0)], &[0, 1, 2, 3]);
    let fan_in = exact_graph(&[(0, 0), (1, 1), (2, 0), (3, 0)], &[0, 1, 2, 3]);
    assert_eq!(
        exact_normalization(&paired, false).encoding,
        exact_normalization(&crossed, false).encoding
    );
    assert_ne!(
        exact_normalization(&paired, false).encoding,
        exact_normalization(&fan_in, false).encoding
    );
    let fan_out = exact_graph(&[(0, 0), (1, 1), (2, 0), (2, 1), (3, 3)], &[0, 1, 2, 3]);
    assert_ne!(
        exact_normalization(&paired, false).encoding,
        exact_normalization(&fan_out, false).encoding
    );
    for edges in [
        vec![(0, 1), (1, 2), (2, 0)],
        vec![(0, 0), (1, 1), (2, 2), (3, 0), (4, 1), (5, 2)],
    ] {
        let n = edges.len();
        let base = exact_graph(&edges, &(0..n).collect::<Vec<_>>());
        let expected = exact_normalization(&base, true).encoding;
        for allocation in permutations(n) {
            let renamed = exact_graph(&edges, &allocation);
            assert_eq!(expected, exact_normalization(&renamed, false).encoding);
        }
    }
}

#[test]
fn exact_encoding_retains_effect_order_multiplicity_and_shared_group_anchors() {
    let base = exact_graph(&[(0, 0), (1, 1)], &[0, 1]);
    let mut changed = base.clone();
    changed.pending_triggers[0].context.effect = Effect::Multiple(vec![
        Effect::GainLife { amount: 1 },
        Effect::GainLife { amount: 2 },
    ]);
    let ordered = exact_normalization(&changed, false).encoding;
    changed.pending_triggers[0].context.effect = Effect::Multiple(vec![
        Effect::GainLife { amount: 2 },
        Effect::GainLife { amount: 1 },
    ]);
    assert_ne!(ordered, exact_normalization(&changed, false).encoding);
    changed.pending_triggers[0].context.effect = Effect::Multiple(vec![
        Effect::GainLife { amount: 1 },
        Effect::GainLife { amount: 2 },
        Effect::GainLife { amount: 2 },
    ]);
    assert_ne!(ordered, exact_normalization(&changed, false).encoding);
    let same_group = exact_normalization(&base, false);
    assert_eq!(same_group.stats.component_sizes, vec![1, 1]);
    changed = base.clone();
    changed.pending_triggers[1]
        .context
        .zone_transition
        .as_mut()
        .unwrap()
        .group_id += 1;
    assert_ne!(
        same_group.encoding,
        exact_normalization(&changed, false).encoding
    );
    for trigger in &mut changed.pending_triggers {
        trigger.context.zone_transition.as_mut().unwrap().group_id += 2000;
    }
    assert_eq!(
        exact_normalization(&changed, false).encoding,
        exact_normalization(&changed, true).encoding
    );
}

#[test]
fn exact_normalization_diagnostics_report_search_and_component_cost_without_limits() {
    let cases = [
        ("2!*2!", vec![(0, 0), (1, 1), (2, 0), (3, 1)]),
        ("3!", vec![(0, 1), (1, 2), (2, 0)]),
        (
            "3!*3!",
            vec![(0, 0), (1, 1), (2, 2), (3, 0), (4, 1), (5, 2)],
        ),
        ("6!", (0..6).map(|i| (i, i)).collect()),
        (">6 sources", (0..8).map(|i| (i, i)).collect()),
    ];
    for (name, edges) in cases {
        let n = edges.len();
        let mut state = exact_graph(&edges, &(0..n).collect::<Vec<_>>());
        if name.contains("*3!") || name == "2!*2!" {
            for trigger in &mut state.pending_triggers {
                if !trigger
                    .context
                    .zone_transition
                    .as_ref()
                    .unwrap()
                    .source_was_subject
                {
                    trigger.context.source_card_id = SUBJECT;
                    trigger
                        .context
                        .zone_transition
                        .as_mut()
                        .unwrap()
                        .source_before
                        .card_id = SUBJECT;
                }
            }
        }
        let result = exact_normalization(&state, false);
        println!("{name}: calls={}, nodes={}, refinements={}, cells={:?}, components={:?}, candidates={}, encoding_bytes={}, elapsed_ns={}",
            result.stats.normalization_calls, result.stats.search_nodes, result.stats.refinement_rounds,
            result.stats.tied_cell_sizes, result.stats.component_sizes, result.stats.encoded_candidates,
            result.encoding.len(), result.stats.elapsed_nanos);
        assert_eq!(result.source_ranks.len(), n);
        if n <= 6 {
            assert_eq!(result.encoding, exact_normalization(&state, true).encoding);
        }
        if n > 6 {
            state.pending_triggers.reverse();
            assert_eq!(result.encoding, exact_normalization(&state, false).encoding);
            // No assertion that the existing >6 FIFO action subset is invariant.
        }
    }
}

#[test]
fn exact_symmetry_witness_survives_all_restores_and_actual_final_resolution() {
    let mut base = exact_graph(&[(0, 1), (1, 2), (2, 0)], &[0, 1, 2]);
    rules::check_state_based_actions(&mut base);
    let mut renamed = exact_graph(&[(0, 1), (1, 2), (2, 0)], &[2, 0, 1]);
    renamed.pending_triggers.reverse();
    rules::check_state_based_actions(&mut renamed);
    let encoding = exact_normalization(&base, false).encoding;
    let action = legal_actions(&base)
        .into_iter()
        .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    let key = canonicalize(&action, &base);
    let snapshot = renamed.snapshot();
    let json = serde_json::to_vec(&renamed).unwrap();
    let binary = bincode::serialize(&renamed).unwrap();
    let mut variants = vec![
        base.clone(),
        renamed.clone(),
        serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap(),
    ];
    renamed.pending_triggers.clear();
    renamed.restore(snapshot);
    variants.push(renamed);
    for state in &mut variants {
        state.card_db = Some(database());
        assert_eq!(encoding, exact_normalization(state, false).encoding);
        assert_eq!(encoding, exact_normalization(state, true).encoding);
        let action = resolve(&key, state, 0).unwrap();
        assert_eq!(canonicalize(&action, state), key);
        finish_occurrence_resolution(state, &key);
        assert_eq!(state.players[0].life, 26);
        assert!(exact_normalization(state, false).encoding.is_empty());
    }
}

#[test]
fn exact_oracle_covers_live_links_departed_blinked_groups_and_stack_anchors() {
    let mut fixtures = vec![retained_subject_relationship_game()];
    for linked in [[Some(OTHER), None], [Some(OTHER), Some(SUBJECT)]] {
        fixtures.push(linked_source_choice_game(false, false, linked, false).0);
        fixtures.push(linked_source_choice_game(true, true, linked, true).0);
    }
    for inverse in [false, true] {
        fixtures.push(incoming_stacked_subject_game(false, inverse, false, false).0);
        fixtures.push(incoming_stacked_subject_game(true, inverse, false, true).0);
    }
    fixtures.push(incoming_successive_subject_game(false).0);
    let mut blinked = game();
    let source = blinked.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let subject = blinked.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let req = request(&blinked, subject, ZoneType::Exile);
    transition_batch(&mut blinked, &[req]).unwrap();
    blinked.move_object(source, ZoneType::Battlefield, ZoneType::Exile);
    blinked.move_object(source, ZoneType::Exile, ZoneType::Battlefield);
    let subject = blinked.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let req = request(&blinked, subject, ZoneType::Exile);
    transition_batch(&mut blinked, &[req]).unwrap();
    fixtures.push(blinked);
    for fixture in fixtures {
        let expected = exact_normalization(&fixture, true).encoding;
        assert_eq!(expected, exact_normalization(&fixture, false).encoding);
        let mut reversed = fixture.clone();
        reversed.pending_triggers.reverse();
        for entry in &mut reversed.stack {
            entry.id += 5000;
        }
        assert_eq!(expected, exact_normalization(&reversed, false).encoding);
        let mut changed = fixture.clone();
        changed.pending_triggers[0].controller = 1 - changed.pending_triggers[0].controller;
        assert_ne!(expected, exact_normalization(&changed, false).encoding);
        assert_eq!(
            exact_normalization(&changed, false).encoding,
            exact_normalization(&changed, true).encoding
        );
        let snapshot = fixture.snapshot();
        let mut variants = vec![
            fixture.clone(),
            serde_json::from_slice::<GameState>(&serde_json::to_vec(&fixture).unwrap()).unwrap(),
            bincode::deserialize::<GameState>(&bincode::serialize(&fixture).unwrap()).unwrap(),
        ];
        changed.restore(snapshot);
        variants.push(changed);
        for restored in &mut variants {
            restored.card_db = Some(database());
            assert_eq!(expected, exact_normalization(restored, false).encoding);
        }
    }
}

#[test]
fn exact_pending_to_stack_and_restored_resolution_recompute_without_persisted_labels() {
    let mut state = exact_graph(&[(0, 1), (1, 2), (2, 0)], &[0, 1, 2]);
    rules::check_state_based_actions(&mut state);
    let pending_encoding = exact_normalization(&state, false).encoding;
    let action = legal_actions(&state)
        .into_iter()
        .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    rules::apply_action(&mut state, &action);
    assert_eq!(state.stack.len(), 3);
    let encoding = exact_normalization(&state, false).encoding;
    assert_ne!(encoding, pending_encoding); // Complete fixed stack positions.
    assert_eq!(encoding, exact_normalization(&state, true).encoding);
    let snapshot = state.snapshot();
    let mut variants = vec![
        state.clone(),
        serde_json::from_slice::<GameState>(&serde_json::to_vec(&state).unwrap()).unwrap(),
        bincode::deserialize::<GameState>(&bincode::serialize(&state).unwrap()).unwrap(),
    ];
    state.stack.clear();
    state.restore(snapshot);
    variants.push(state);
    for restored in &mut variants {
        restored.card_db = Some(database());
        for entry in &mut restored.stack {
            entry.id += 700;
        }
        assert_eq!(encoding, exact_normalization(restored, false).encoding);
        for _ in 0..3 {
            rules::apply_action(restored, &Action::PassPriority);
            rules::apply_action(restored, &Action::PassPriority);
        }
        assert!(restored.stack.is_empty());
        assert!(restored.pending_triggers.is_empty());
        assert_eq!(restored.players[0].life, 26);
        assert!(exact_normalization(restored, false).encoding.is_empty());
    }
}

#[test]
fn exact_duplicate_occurrences_keep_multiplicity_without_action_orbit_merging() {
    let mut state = exact_graph(&[(0, 0)], &[0]);
    let occurrence = state.pending_triggers[0].clone();
    state
        .pending_triggers
        .extend([occurrence.clone(), occurrence]);
    rules::check_state_based_actions(&mut state);
    let normalized = exact_normalization(&state, false);
    assert_eq!(
        normalized.encoding,
        exact_normalization(&state, true).encoding
    );
    let actions: Vec<_> = legal_actions(&state)
        .into_iter()
        .filter(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
        .collect();
    assert_eq!(actions.len(), 6);
    let keys = mtg_gto::action::canonical::canonicalize_actions(&actions, &state, &normalized);
    assert_eq!(keys.len(), actions.len());
    for key in &keys {
        let resolved = resolve(key, &state, 0).unwrap();
        assert_eq!(canonicalize(&resolved, &state), *key);
    }
    let key = keys[0].clone();
    finish_occurrence_resolution(&mut state, &key);
    assert_eq!(state.players[0].life, 26);
}

#[test]
fn hidden_hand_view_keys_reconstruct_from_same_player_projection() {
    let mut state = game();
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = state.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let second = state.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let requests = [request(&state, first, ZoneType::Hand),
        request(&state, second, ZoneType::Hand)];
    transition_batch(&mut state, &requests).unwrap();
    rules::check_state_based_actions(&mut state);
    let actions: Vec<_> = legal_actions_abstracted(&state).into_iter().filter(|action|
        matches!(action, Action::OrderTriggerOccurrences { .. })).collect();
    assert_eq!(actions.len(), 2);
    let view = state.visible_state(0);
    let normalized = InformationSet::normalize_retained_view(&view);
    let info = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized);
    assert_eq!(info.pending_zone_triggers.len(), 2);
    assert!(info.pending_zone_triggers.iter().all(|occurrence|
        !occurrence.subject.as_ref().unwrap().same_incarnation_now));
    let keys = mtg_gto::action::canonical::canonicalize_actions(&actions, &state, &normalized);
    for (action, key) in actions.iter().zip(&keys) {
        assert_eq!(*key, canonicalize(action, &state));
        let reconstructed = resolve(key, &state, 0).expect("a view-derived key must reconstruct");
        assert_eq!(canonicalize(&reconstructed, &state), *key);
    }
    // The two identical hidden subjects have equivalent continuations. Their
    // equal semantic keys must still retain both concrete mandatory actions.
    assert_eq!(keys[0], keys[1]);
    let mut outcomes = Vec::new();
    for key in &keys {
        let mut continued = state.clone();
        finish_occurrence_resolution(&mut continued, key);
        outcomes.push((continued.players[0].life, continued.stack.len(), continued.pending_triggers.len()));
    }
    assert_eq!(outcomes, vec![(24, 0, 0); 2]);
    assert_final_continuation_matches(state, 4);
}

fn hidden_hand_occurrences(id_start: u64, reverse: bool) -> GameState {
    let mut state = game();
    state.next_object_id = id_start;
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = state.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let second = state.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let mut requests = [request(&state, first, ZoneType::Hand),
        request(&state, second, ZoneType::Hand)];
    if reverse { requests.reverse(); }
    transition_batch(&mut state, &requests).unwrap();
    if reverse { state.pending_triggers.reverse(); }
    rules::check_state_based_actions(&mut state);
    state
}

fn view_coordinates(state: &GameState, player: usize) -> (Vec<u8>, u64, Vec<Vec<u8>>) {
    let view = state.visible_state(player);
    let normalized = InformationSet::normalize_retained_view(&view);
    let info = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized);
    let actions = legal_actions_abstracted(state);
    let mut keys: Vec<_> = mtg_gto::action::canonical::canonicalize_actions(&actions, state, &normalized)
        .iter().map(|key| bincode::serialize(key).unwrap()).collect();
    keys.sort();
    (normalized.encoding, info.hash_value(), keys)
}

#[test]
fn hidden_subject_ids_allocation_and_pending_order_do_not_change_view_coordinates() {
    let base = hidden_hand_occurrences(1_000_000_010, false);
    let renamed = hidden_hand_occurrences(2_000_000_020, true);
    assert_eq!(view_coordinates(&base, 0), view_coordinates(&renamed, 0));
    let view = base.visible_state(0);
    let normalized = InformationSet::normalize_retained_view(&view);
    let actions = legal_actions_abstracted(&base);
    let keys = mtg_gto::action::canonical::canonicalize_actions(&actions, &base, &normalized);
    for (action, key) in actions.iter().zip(&keys) {
        assert_eq!(canonicalize(action, &base), *key);
        assert_eq!(canonicalize(&resolve(key, &base, 0).unwrap(), &base), *key);
        assert_eq!(canonicalize(&resolve(key, &renamed, 0).unwrap(), &renamed), *key);
    }
    let bytes = bincode::serialize(&keys).unwrap();
    let info = InformationSet::from_view(&base.visible_state(0), base.card_db());
    let info_debug = format!("{info:?}");
    let keys_debug = format!("{keys:?}");
    for hidden_id in base.players[1].hand.iter().chain(renamed.players[1].hand.iter()) {
        assert!(!bytes.windows(8).any(|chunk| chunk == hidden_id.to_le_bytes()),
            "canonical action leaked hidden runtime ObjectId {hidden_id}");
        assert!(!info_debug.contains(&hidden_id.to_string()));
        assert!(!keys_debug.contains(&hidden_id.to_string()));
    }
    assert_final_continuation_matches(base, 4);
    assert_final_continuation_matches(renamed, 4);
}

#[test]
fn hidden_pending_self_source_does_not_reveal_current_private_incarnation() {
    let mut state = game();
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let second = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield);
    let requests = [request(&state, first, ZoneType::Hand),
        request(&state, second, ZoneType::Hand)];
    transition_batch(&mut state, &requests).unwrap();
    rules::check_state_based_actions(&mut state);
    let view = state.visible_state(0);
    // Pending ability sources may be retained for resolution, even after they
    // enter a private zone. They are not evidence of current visibility.
    assert!(view.objects.contains_key(&first));
    let normalized = InformationSet::normalize_retained_view(&view);
    assert_eq!(normalized.pending_occurrences.len(), 4);
    for occurrence in normalized.pending_occurrences.iter().flatten() {
        assert!(!occurrence.subject.as_ref().unwrap().same_incarnation_now);
        if occurrence.source_card_id == SUBJECT {
            assert!(occurrence.source.as_ref().unwrap().live.is_none());
        }
    }
    let before = view_coordinates(&state, 0);
    for id in [first, second] {
        let instance = state.objects.get_mut(&id).unwrap();
        instance.zone_change_count += 7;
        instance.tapped = !instance.tapped;
        instance.damage_marked += 3;
    }
    state.players[1].hand.reverse();
    assert_eq!(view_coordinates(&state, 0), before);
}

#[test]
fn current_subject_visibility_is_player_relative_and_public_exile_is_retained() {
    let mut hand = game();
    hand.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let subject = hand.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    hand.objects.get_mut(&subject).unwrap().controller = 0;
    let req = request(&hand, subject, ZoneType::Hand);
    transition_batch(&mut hand, &[req]).unwrap();
    let status = |state: &GameState, viewer| {
        let view = state.visible_state(viewer);
        InformationSet::normalize_retained_view(&view).pending_occurrences
            .into_iter().flatten().next().unwrap().subject.unwrap()
    };
    let opponent_view = status(&hand, 0);
    let owner_view = status(&hand, 1);
    assert!(!opponent_view.same_incarnation_now);
    assert!(owner_view.same_incarnation_now);
    assert_eq!(opponent_view.controller_before, 0);
    assert_eq!(opponent_view.owner, 1);
    assert_eq!(opponent_view.to, ZoneType::Hand);

    let mut library = game();
    library.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let subject = library.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let req = request(&library, subject, ZoneType::Library);
    transition_batch(&mut library, &[req]).unwrap();
    // A library is private even to its owner; neither view can confirm the
    // current incarnation from the zone's contents.
    assert!(!status(&library, 0).same_incarnation_now);
    assert!(!status(&library, 1).same_incarnation_now);

    let mut exile = game();
    exile.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    exile.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let subject = exile.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let req = request(&exile, subject, ZoneType::Exile);
    transition_batch(&mut exile, &[req]).unwrap();
    assert!(status(&exile, 0).same_incarnation_now);
    assert!(status(&exile, 1).same_incarnation_now);
    assert_final_continuation_matches(exile, 4);
}

#[test]
fn old_subject_occurrence_does_not_rebind_after_same_id_reenters() {
    let mut state = game();
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let subject = state.create_card_in_zone(OTHER, 1, ZoneType::Battlefield);
    let req = request(&state, subject, ZoneType::Hand);
    transition_batch(&mut state, &[req]).unwrap();
    state.move_object(subject, ZoneType::Hand, ZoneType::Battlefield);
    let req = request(&state, subject, ZoneType::Exile);
    transition_batch(&mut state, &[req]).unwrap();
    rules::check_state_based_actions(&mut state);
    let view = state.visible_state(0);
    let normalized = InformationSet::normalize_retained_view(&view);
    let statuses: Vec<_> = normalized.pending_occurrences.iter().flatten()
        .map(|occurrence| occurrence.subject.as_ref().unwrap().same_incarnation_now).collect();
    assert_eq!(statuses.iter().filter(|&&status| status).count(), 1);
    assert_eq!(statuses.iter().filter(|&&status| !status).count(), 1);
    let keys = mtg_gto::action::canonical::canonicalize_actions(
        &legal_actions_abstracted(&state), &state, &normalized);
    for key in &keys {
        assert_eq!(canonicalize(&resolve(key, &state, 0).unwrap(), &state), *key);
    }
    assert_final_continuation_matches(state, 4);
}

fn multiplayer_departure_game(players: usize, owner: usize, to: ZoneType) -> GameState {
    let mut state = GameState::new(players);
    state.card_db = Some(database());
    state.phase = Phase::PreCombatMain;
    state.active_player = 0;
    state.priority_player = 0;
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = state.create_card_in_zone(OTHER, owner, ZoneType::Battlefield);
    let second = state.create_card_in_zone(OTHER, owner, ZoneType::Battlefield);
    let requests = [request(&state, first, to), request(&state, second, to)];
    transition_batch(&mut state, &requests).unwrap();
    state
}

#[test]
fn later_opponent_public_exile_uses_same_current_subject_fact_as_direct_action_keys() {
    let mut state = multiplayer_departure_game(3, 2, ZoneType::Exile);
    rules::check_state_based_actions(&mut state);
    assert_eq!(state.players[2].exile.len(), 2);
    let view = state.visible_state(0);
    for subject in &state.players[2].exile {
        assert!(view.objects.contains_key(subject));
    }
    let normalized = InformationSet::normalize_retained_view(&view);
    assert_eq!(normalized.pending_occurrences.len(), 2);
    assert!(normalized.pending_occurrences.iter().flatten().all(|occurrence|
        occurrence.subject.as_ref().unwrap().same_incarnation_now));
    let actions: Vec<_> = legal_actions_abstracted(&state).into_iter().filter(|action|
        matches!(action, Action::OrderTriggerOccurrences { .. })).collect();
    let keys = mtg_gto::action::canonical::canonicalize_actions(&actions, &state, &normalized);
    for (action, key) in actions.iter().zip(&keys) {
        assert_eq!(canonicalize(action, &state), *key);
        let reconstructed = resolve(key, &state, 0).unwrap();
        assert_eq!(canonicalize(&reconstructed, &state), *key);
    }
    let info = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized);
    assert!(info.pending_zone_triggers.iter().all(|occurrence|
        occurrence.subject.as_ref().unwrap().same_incarnation_now));
}

fn multiplayer_subject_statuses(state: &GameState, viewer: usize) -> Vec<bool> {
    InformationSet::normalize_retained_view(&state.visible_state(viewer))
        .pending_occurrences.into_iter().flatten()
        .map(|occurrence| occurrence.subject.unwrap().same_incarnation_now).collect()
}

#[test]
fn rotated_viewers_see_every_public_destination_and_only_their_own_private_hand() {
    for players in [3, 4] {
        for owner in 0..players {
            for destination in [ZoneType::Exile, ZoneType::Graveyard, ZoneType::Command] {
                let state = multiplayer_departure_game(players, owner, destination);
                for viewer in 0..players {
                    assert_eq!(multiplayer_subject_statuses(&state, viewer), vec![true; 2],
                        "{players} seats, owner {owner}, viewer {viewer}, {destination:?}");
                }
            }
            for destination in [ZoneType::Hand, ZoneType::Library] {
                let state = multiplayer_departure_game(players, owner, destination);
                for viewer in 0..players {
                    assert_eq!(multiplayer_subject_statuses(&state, viewer),
                        vec![destination == ZoneType::Hand && viewer == owner; 2],
                        "{players} seats, owner {owner}, viewer {viewer}, {destination:?}");
                }
            }
        }
    }
}

#[test]
fn later_opponent_hidden_source_in_view_map_does_not_expose_current_private_facts() {
    let mut state = GameState::new(3);
    state.card_db = Some(database());
    state.phase = Phase::PreCombatMain;
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let subject = state.create_card_in_zone(SUBJECT, 2, ZoneType::Battlefield);
    let req = request(&state, subject, ZoneType::Hand);
    transition_batch(&mut state, &[req]).unwrap();
    let view = state.visible_state(0);
    assert!(view.objects.contains_key(&subject),
        "the departing source is retained for ability resolution");
    assert_eq!(multiplayer_subject_statuses(&state, 0), vec![false; 2]);
    assert_eq!(multiplayer_subject_statuses(&state, 1), vec![false; 2]);
    assert_eq!(multiplayer_subject_statuses(&state, 2), vec![true; 2]);
    let historical = InformationSet::normalize_retained_view(&view).pending_occurrences;
    assert!(historical.iter().flatten().all(|occurrence| {
        let subject = occurrence.subject.as_ref().unwrap();
        subject.card_id == SUBJECT && subject.controller_before == 2
            && subject.from == ZoneType::Battlefield && subject.to == ZoneType::Hand
    }));
    let before = view_coordinates(&state, 0);
    let inst = state.objects.get_mut(&subject).unwrap();
    inst.zone_change_count += 7;
    inst.tapped = true;
    inst.damage_marked += 3;
    assert_eq!(view_coordinates(&state, 0), before);
}

fn multiplayer_layout_game(to: ZoneType, id_start: u64, reverse: bool) -> GameState {
    let mut state = GameState::new(3);
    state.card_db = Some(database());
    state.phase = Phase::PreCombatMain;
    state.next_object_id = id_start;
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let first = state.create_card_in_zone(OTHER, 2, ZoneType::Battlefield);
    let second = state.create_card_in_zone(OTHER, 2, ZoneType::Battlefield);
    let mut requests = [request(&state, first, to), request(&state, second, to)];
    if reverse { requests.reverse(); }
    transition_batch(&mut state, &requests).unwrap();
    if reverse { state.pending_triggers.reverse(); }
    rules::check_state_based_actions(&mut state);
    state
}

#[test]
fn later_opponent_public_and_hidden_subjects_are_allocation_and_order_independent() {
    for destination in [ZoneType::Exile, ZoneType::Graveyard, ZoneType::Hand, ZoneType::Library] {
        let base = multiplayer_layout_game(destination, 1_000_001_000, false);
        let renamed = multiplayer_layout_game(destination, 2_000_002_000, true);
        assert_eq!(view_coordinates(&base, 0), view_coordinates(&renamed, 0),
            "{destination:?}");
        let visible = matches!(destination, ZoneType::Exile | ZoneType::Graveyard);
        assert_eq!(multiplayer_subject_statuses(&base, 0), vec![visible; 2]);
        let view = base.visible_state(0);
        let normalized = InformationSet::normalize_retained_view(&view);
        let actions = legal_actions_abstracted(&base);
        let keys = mtg_gto::action::canonical::canonicalize_actions(&actions, &base, &normalized);
        for (action, key) in actions.iter().zip(&keys) {
            assert_eq!(canonicalize(action, &base), *key);
            assert_eq!(canonicalize(&resolve(key, &base, 0).unwrap(), &base), *key);
            assert_eq!(canonicalize(&resolve(key, &renamed, 0).unwrap(), &renamed), *key);
        }
    }
}

#[test]
fn later_opponent_public_occurrences_continue_identically_after_all_restores() {
    let mut state = multiplayer_layout_game(ZoneType::Exile, 1_000_003_000, false);
    let initial_life = state.players[0].life;
    let action = legal_actions_abstracted(&state).into_iter()
        .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. })).unwrap();
    let key = canonicalize(&action, &state);
    let snapshot = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let mut variants = vec![state.clone(), serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap()];
    state.pending_triggers.clear();
    state.restore(snapshot);
    variants.push(state);
    let mut expected = None;
    for game in &mut variants {
        game.card_db = Some(database());
        assert_eq!(multiplayer_subject_statuses(game, 0), vec![true; 2]);
        let choice = resolve(&key, game, 0).unwrap();
        assert_eq!(canonicalize(&choice, game), key);
        rules::apply_action(game, &choice);
        for _ in 0..30 {
            if game.stack.is_empty() { break; }
            rules::apply_action(game, &Action::PassPriority);
        }
        assert!(game.stack.is_empty());
        assert!(game.pending_triggers.is_empty());
        assert_eq!(game.players[0].life, initial_life + 4);
        let finished = serde_json::to_value(&*game).unwrap();
        if let Some(ref expected) = expected { assert_eq!(&finished, expected); }
        else { expected = Some(finished); }
    }
}
