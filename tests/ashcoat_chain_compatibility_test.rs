use std::sync::Arc;
use mtg_gto::{action::Action, card::{sample, CardDef, CardType, KeywordAbility, ZoneType},
    events::{GameEvent, Zone}, game::{GameState, Phase, StackEntry, StackSource, Target}, rules};

const SUBJECT: u64 = 999_501;
fn fixture() -> (GameState, u64, u64, Vec<u64>) {
    let mut db = sample::build_sample_db();
    db.insert(CardDef { id: SUBJECT, name: "Chain target".into(), card_types: vec![CardType::Creature],
        power: Some(3), toughness: Some(3), ..Default::default() });
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db)); state.phase = Phase::PreCombatMain;
    let commander = state.create_card_in_zone(sample::ids::ASHCOAT_OF_THE_SHADOW_SWARM, 0, ZoneType::Command).unwrap();
    state.designate_commander(commander, 0).unwrap();
    let subject = state.create_card_in_zone(SUBJECT, 1, ZoneType::Battlefield).unwrap();
    let cards = (0..3).map(|_| state.create_card_in_zone(sample::ids::SWAMP, 0, ZoneType::Library).unwrap()).collect();
    let spell = state.create_card_in_zone(sample::ids::CHAIN_ASSASSINATION, 0, ZoneType::Stack).unwrap();
    state.stack.push(StackEntry { id: 1, source: StackSource::Spell(spell), controller: 0,
        targets: vec![Target::Object(subject)], target_generations: vec![Some(0)] });
    state.next_stack_id = 2; state.drain_events();
    (state, subject, spell, cards)
}
fn resolve(state: &mut GameState) {for _ in 0..state.players.len() {rules::apply_action(state, &Action::PassPriority);}}

#[test]
fn chain_destroys_then_draws_one_and_finishes_once() {
    let (mut state, subject, spell, cards) = fixture();
    state.objects.get_mut(&subject).unwrap().damage_marked = 1;
    resolve(&mut state);
    assert!(!state.gameplay_stopped(), "ordinary synchronous Chain must remain supported");
    assert!(state.players[1].graveyard.contains(&subject));
    assert_eq!(state.players[0].hand, vec![cards[0]]);
    assert_eq!(state.players[0].library, cards[1..]);
    assert!(state.players[0].graveyard.contains(&spell)); assert!(state.stack.is_empty());
    assert!(state.pending_movement.is_none()); assert!(state.trigger_order_resume.is_none());
    assert_eq!(state.priority_player, state.active_player);
    assert_eq!(state.objects[&subject].zone_change_count, 1); assert_eq!(state.objects[&spell].zone_change_count, 1);
    assert_eq!(state.next_zone_event_group_id, 1);
    assert_eq!((state.players[0].life, state.players[1].life), (40, 40));
    let events = state.drain_events();
    assert_eq!(events.iter().filter(|event| matches!(event, GameEvent::CardDrawn { player: 0, object } if *object == cards[0])).count(), 1);
    for (id, from, to) in [(subject, Zone::Battlefield, Zone::Graveyard), (cards[0], Zone::Library, Zone::Hand), (spell, Zone::Stack, Zone::Graveyard)] {
        assert_eq!(events.iter().filter(|event| matches!(event, GameEvent::ZoneChange { object, from: f, to: t } if *object == id && *f == from && *t == to)).count(), 1);
    }
    let death = events.iter().position(|event| matches!(event, GameEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Graveyard } if *object == subject)).unwrap();
    let draw = events.iter().position(|event| matches!(event, GameEvent::CardDrawn { object, .. } if *object == cards[0])).unwrap();
    let cleanup = events.iter().position(|event| matches!(event, GameEvent::ZoneChange { object, from: Zone::Stack, to: Zone::Graveyard } if *object == spell)).unwrap();
    assert!(death < draw && draw < cleanup);
}

#[test]
fn chain_still_draws_when_legal_creature_is_indestructible() {
    let (mut state, subject, spell, cards) = fixture();
    let mut definition = state.card_db().get(SUBJECT).unwrap().clone();
    definition.keywords.push(KeywordAbility::Indestructible);
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(definition); state.invalidate_characteristics_cache();
    resolve(&mut state);
    assert!(!state.gameplay_stopped(), "indestructible target does not stop untargeted draw");
    assert!(state.battlefield.contains(&subject)); assert_eq!(state.objects[&subject].zone_change_count, 0);
    assert_eq!(state.players[0].hand, vec![cards[0]]); assert!(state.players[0].graveyard.contains(&spell));
    assert_eq!(state.next_zone_event_group_id, 0);
    assert!(!state.drain_events().iter().any(|event| matches!(event, GameEvent::ZoneChange { object, .. } if *object == subject)));
}

fn assert_rejected_without_prefix(state: &mut GameState, subject: u64, spell: u64) {
    let objects = serde_json::to_value(&state.objects).unwrap();
    let players = serde_json::to_value(&state.players).unwrap();
    let stack = serde_json::to_value(&state.stack).unwrap();
    let group = state.next_zone_event_group_id; let timestamp = state.next_timestamp;
    state.drain_events(); resolve(state);
    assert!(state.gameplay_stopped()); assert!(state.pending_movement.is_none());
    assert!(state.battlefield.contains(&subject));
    assert!(state.stack.iter().any(|entry| entry.source.physical_object() == Some(spell)));
    assert_eq!(serde_json::to_value(&state.objects).unwrap(), objects);
    assert_eq!(serde_json::to_value(&state.players).unwrap(), players);
    assert_eq!(serde_json::to_value(&state.stack).unwrap(), stack);
    assert_eq!(state.next_zone_event_group_id, group); assert_eq!(state.next_timestamp, timestamp);
    assert!(state.drain_events().is_empty());
}
fn information(state: &GameState, observer: usize) -> mtg_gto::info_set::InformationSet {
    mtg_gto::info_set::InformationSet::from_view(&state.visible_state(observer), state.card_db()).unwrap()
}

#[test]
fn hidden_commander_hand_and_library_are_indistinguishable_and_both_reject_before_prefix() {
    let (base, subject, spell, cards) = fixture(); let commander = base.commanders[0].object;
    let mut hand = base.clone(); let mut library = base;
    hand.players[0].command_zone.clear(); hand.players[0].hand.push(commander);
    library.players[0].command_zone.clear(); library.players[0].library[0] = commander;
    library.players[0].hand.push(cards[0]);
    assert!(hand.validate_commanders().is_ok()); assert!(library.validate_commanders().is_ok());
    for after in [false, true] {
        if after {
            assert_rejected_without_prefix(&mut hand, subject, spell);
            assert_rejected_without_prefix(&mut library, subject, spell);
        }
        assert_eq!(information(&hand, 1).zone_normalization, information(&library, 1).zone_normalization);
        assert_eq!(information(&hand, 1).hash_value(), information(&library, 1).hash_value());
        assert_ne!(information(&hand, 0).hash_value(), information(&library, 0).hash_value());
    }
}

#[test]
fn a_library_commander_below_the_nominal_draw_and_each_rotated_position_reject() {
    let (mut base, subject, spell, _) = fixture(); let commander = base.commanders[0].object;
    base.players[0].command_zone.clear(); base.players[0].library.push(commander);
    let library = base.players[0].library.clone(); let before = information(&base, 1).hash_value();
    let mut stopped = None;
    for index in 0..library.len() {
        let mut state = base.clone(); state.players[0].library.rotate_left(index);
        assert_eq!(information(&state, 1).hash_value(), before);
        assert_rejected_without_prefix(&mut state, subject, spell);
        let encoding = information(&state, 1).zone_normalization;
        if let Some(expected) = &stopped {assert_eq!(&encoding, expected);} else {stopped = Some(encoding);}
    }
}

#[test]
fn commander_death_and_linked_exile_arrival_offer_sba_only_after_the_draw() {
    for linked in [false, true] {
        let (mut state, subject, spell, cards) = fixture();
        let commander = if linked {
            let id = state.commanders[0].object; state.players[0].command_zone.clear(); state.players[0].exile.push(id);
            state.objects.get_mut(&id).unwrap().exiled_by = Some(subject); id
        } else {state.designate_commander(subject, 0).unwrap(); subject};
        resolve(&mut state);
        assert!(!state.gameplay_stopped()); assert!(state.pending_movement.is_none());
        assert!(state.pending_commander_sba.is_some());
        assert!(state.players[state.objects[&commander].owner].graveyard.contains(&commander));
        assert_eq!(state.players[0].hand, vec![cards[0]]); assert!(state.players[0].graveyard.contains(&spell));
        assert!(state.stack.is_empty()); assert_eq!(state.objects[&commander].zone_change_count, 1);
        let events = state.drain_events();
        assert_eq!(events.iter().filter(|event| matches!(event, GameEvent::CardDrawn { object, .. } if *object == cards[0])).count(), 1);
        let declaration = state.commander_for_object(commander).unwrap().designation;
        for accept in [false, true] {
            let mut branch = state.clone(); rules::apply_action(&mut branch, &Action::ChooseCommanderSba { designation: declaration, to_command: accept });
            assert_eq!(branch.players[0].hand, vec![cards[0]]); assert!(branch.pending_commander_sba.is_none());
            assert_eq!(branch.players[declaration.owner].command_zone.contains(&commander), accept);
            assert!(!branch.drain_events().iter().any(|event| matches!(event, GameEvent::CardDrawn { .. })));
        }
    }
}

#[test]
fn relevant_catalog_and_death_created_library_commander_reject_before_prefix() {
    use mtg_gto::replacement::{MovementDestination, ReplacementAction, ReplacementEffect, ReplacementEventKind};
    for predicate in [ReplacementEventKind::WouldDie, ReplacementEventKind::WouldMoveTo { destination: MovementDestination::Command, source_generation: 0 }, ReplacementEventKind::CardDraw] {
        let (mut state, subject, spell, _) = fixture(); state.designate_commander(subject, 0).unwrap();
        let source = state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield).unwrap();
        state.replacement_effects.push(ReplacementEffect { source_id: source, controller: 0,
            applies_to: predicate, action: ReplacementAction::RedirectToZone(ZoneType::Library),
            is_self_replacement: true, description: "would introduce or reconsider a replacement".into() });
        assert_rejected_without_prefix(&mut state, subject, spell);
    }
}

#[test]
fn ordinary_bounce_still_owns_the_required_commander_hand_decision() {
    use mtg_gto::card::{Effect, TargetSpec};
    let (mut state, subject, spell, cards) = fixture(); state.designate_commander(subject, 0).unwrap();
    let mut definition = state.card_db().get(sample::ids::CHAIN_ASSASSINATION).unwrap().clone();
    definition.spell_effect = Some(Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyCreature });
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(definition); resolve(&mut state);
    assert!(state.pending_movement.is_some()); assert!(state.battlefield.contains(&subject));
    assert_eq!(state.players[0].library, cards); assert!(state.players[0].hand.is_empty());
    assert!(state.objects[&spell].zone_change_count == 0); assert_eq!(state.priority_player, 1);
    let decline = mtg_gto::action::legal_actions(&state).into_iter().find(|action| matches!(action, Action::ChooseCommanderMovement { to_command: false, .. })).unwrap();
    rules::apply_action(&mut state, &decline); assert!(state.players[1].hand.contains(&subject));
}

#[test]
fn invalid_or_stale_sole_target_fizzles_the_entire_chain_without_drawing() {
    for fault in 0..4 {
        let (mut state, subject, spell, cards) = fixture();
        match fault {
            0 => {state.move_object(subject, ZoneType::Battlefield, ZoneType::Graveyard);}
            1 => {state.move_object(subject, ZoneType::Battlefield, ZoneType::Graveyard); state.move_object(subject, ZoneType::Graveyard, ZoneType::Battlefield);}
            2 => {state.stack[0].targets = vec![Target::Player(1)]; state.stack[0].target_generations = vec![None];}
            _ => {let mut definition = state.card_db().get(SUBJECT).unwrap().clone(); definition.card_types = vec![CardType::Land]; Arc::make_mut(state.card_db.as_mut().unwrap()).insert(definition); state.invalidate_characteristics_cache();}
        }
        let generation = state.objects[&subject].zone_change_count; state.drain_events(); resolve(&mut state);
        assert!(!state.gameplay_stopped()); assert!(state.stack.is_empty()); assert!(state.players[0].graveyard.contains(&spell));
        assert!(state.players[0].hand.is_empty()); assert_eq!(state.players[0].library, cards);
        assert_eq!(state.objects[&subject].zone_change_count, generation);
        assert!(!state.drain_events().iter().any(|event| matches!(event, GameEvent::CardDrawn { .. })));
    }
}

#[test]
fn prefix_resources_and_linked_state_fail_before_destruction_or_draw() {
    for fault in 0..5 {
        let (mut state, subject, spell, _) = fixture(); state.players[0].life = 47;
        match fault {
            0 => state.next_zone_event_group_id = u64::MAX,
            1 => {state.objects.get_mut(&subject).unwrap().zone_change_count = u32::MAX; state.stack[0].target_generations[0] = Some(u32::MAX);}
            2 => state.objects.get_mut(&spell).unwrap().zone_change_count = u32::MAX,
            3 => {let linked = state.create_card_in_zone(SUBJECT, 0, ZoneType::Exile).unwrap(); let instance = state.objects.get_mut(&linked).unwrap(); instance.exiled_by = Some(subject); instance.zone_change_count = u32::MAX;}
            _ => {let mut definition = state.card_db().get(SUBJECT).unwrap().clone(); definition.static_abilities = vec![mtg_gto::layers::StaticAbility::Anthem { power: 1, toughness: 1, affected: mtg_gto::layers::AffectedObjects::CreaturesControlledBy(0) }]; Arc::make_mut(state.card_db.as_mut().unwrap()).insert(definition); state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield).unwrap(); state.next_timestamp = u32::MAX; state.invalidate_characteristics_cache();}
        }
        assert_rejected_without_prefix(&mut state, subject, spell); assert_eq!(state.players[0].life, 47);
    }
}

#[test]
fn suffix_rechecks_draw_statics_after_destruction_without_new_continuation() {
    use mtg_gto::layers::StaticAbility;
    for remove_source in [false, true] {
        let (mut state, subject, spell, cards) = fixture();
        let mut definition = state.card_db().get(SUBJECT).unwrap().clone(); definition.static_abilities = vec![StaticAbility::PhialDrawDoubler];
        Arc::make_mut(state.card_db.as_mut().unwrap()).insert(definition);
        state.objects.get_mut(&subject).unwrap().controller = 0;
        if !remove_source {state.create_card_in_zone(SUBJECT, 0, ZoneType::Battlefield).unwrap();}
        state.invalidate_characteristics_cache(); resolve(&mut state);
        assert!(!state.gameplay_stopped()); assert!(state.players[0].graveyard.contains(&spell));
        let draws = if remove_source {1} else {2}; assert_eq!(state.players[0].hand, cards[..draws]);
        assert_eq!(state.players[0].library, cards[draws..]); assert!(state.pending_movement.is_none());
    }
}

#[test]
fn empty_library_failed_draw_settles_after_destruction_and_physical_spell_cleanup() {
    let (mut state, subject, spell, _) = fixture();
    let library = std::mem::take(&mut state.players[0].library); state.players[0].graveyard.extend(library);
    resolve(&mut state);
    assert!(state.game_over); assert_eq!(state.winner, Some(1));
    assert!(state.players[1].graveyard.contains(&subject)); assert!(state.players[0].graveyard.contains(&spell));
    assert!(state.stack.is_empty()); assert!(state.sba_failure.is_none());
    assert!(state.pending_movement.is_none()); assert!(state.loss_boundary.terminal.is_some());
}

#[test]
fn physical_commander_spell_and_nonflat_or_longer_shapes_remain_excluded() {
    use mtg_gto::card::{Effect, TargetSpec};
    for fault in 0..4 {
        let (mut state, subject, spell, _) = fixture();
        if fault == 0 {state.designate_commander(spell, 1).unwrap();} else {
            let pair = vec![Effect::DestroyTarget { target: TargetSpec::AnyCreature }, Effect::DrawCards { count: 1 }];
            let effect = match fault {
                1 => Effect::Multiple(vec![Effect::Multiple(pair)]),
                2 => Effect::Multiple(vec![Effect::GainLife { amount: 7 }, Effect::DestroyTarget { target: TargetSpec::AnyCreature }, Effect::DrawCards { count: 1 }]),
                _ => Effect::Multiple(vec![Effect::DestroyTarget { target: TargetSpec::AnyCreature }, Effect::DrawCards { count: 2 }]),
            };
            let mut definition = state.card_db().get(sample::ids::CHAIN_ASSASSINATION).unwrap().clone(); definition.spell_effect = Some(effect); Arc::make_mut(state.card_db.as_mut().unwrap()).insert(definition);
        }
        assert_rejected_without_prefix(&mut state, subject, spell);
    }
}

#[test]
fn spell_copy_activated_and_triggered_entries_use_the_same_exact_synchronous_route() {
    use mtg_gto::{card::{ActivatedAbility, TriggerCondition, TriggeredAbility}, game::TriggerContext, mana::ManaCost};
    for route in 0..3 {
        let (mut state, subject, spell, cards) = fixture();
        let definition = state.card_db().get(sample::ids::CHAIN_ASSASSINATION).unwrap().clone();
        state.players[0].graveyard.push(spell);
        state.stack[0].source = if route == 0 {StackSource::SpellCopy { definition: Box::new(definition.clone()) }} else {
            let source_definition = 999_503; let effect = definition.spell_effect.clone().unwrap();
            Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: source_definition, name: "Synchronous ability source".into(), card_types: vec![CardType::Artifact],
                activated_abilities: vec![ActivatedAbility { cost: ManaCost::zero(), requires_tap: false, sacrifice_cost: None, life_cost: 0, effect: effect.clone(), description: "fixture".into() }],
                triggered_abilities: vec![TriggeredAbility { trigger: TriggerCondition::BeginningOfUpkeep, effect: effect.clone(), description: "fixture".into() }], ..Default::default() });
            let source = state.create_card_in_zone(source_definition, 0, ZoneType::Battlefield).unwrap();
            if route == 1 {StackSource::ActivatedAbility { source_id: source, ability_index: 0 }} else {StackSource::TriggeredAbility { source_id: source, ability_index: 0,
                context: Box::new(TriggerContext { source_card_id: source_definition, source_generation: 0, effect, cast_spell: None, zone_transition: None }) }}
        };
        state.drain_events(); resolve(&mut state);
        assert!(!state.gameplay_stopped()); assert!(state.players[1].graveyard.contains(&subject));
        assert_eq!(state.players[0].hand, vec![cards[0]]); assert!(state.stack.is_empty());
        assert_eq!(state.players[0].graveyard.iter().filter(|id| **id == spell).count(), 1);
        assert_eq!(state.objects[&spell].zone_change_count, 0, "copied/ability entry cannot settle an unrelated physical card");
    }
}

#[test]
fn death_trigger_placement_and_event_order_follow_the_complete_chain() {
    use mtg_gto::card::{Effect, TriggerCondition, TriggeredAbility};
    let (mut state, subject, spell, cards) = fixture();
    let mut definition = state.card_db().get(SUBJECT).unwrap().clone();
    definition.triggered_abilities = vec![TriggeredAbility { trigger: TriggerCondition::Dies,
        effect: Effect::GainLife { amount: 7 }, description: "must wait for complete resolution".into() }];
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(definition); state.invalidate_characteristics_cache();
    resolve(&mut state);
    assert!(!state.gameplay_stopped()); assert_eq!(state.players[1].life, 40);
    assert_eq!(state.players[0].hand, vec![cards[0]]); assert!(state.players[0].graveyard.contains(&spell));
    assert_eq!(state.stack.len(), 1); assert!(matches!(state.stack[0].source, StackSource::TriggeredAbility { .. }));
    assert_eq!(state.priority_player, 0);
    let events = state.drain_events();
    let death = events.iter().position(|event| matches!(event, GameEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Graveyard } if *object == subject)).unwrap();
    let draw = events.iter().position(|event| matches!(event, GameEvent::CardDrawn { object, .. } if *object == cards[0])).unwrap();
    let cleanup = events.iter().position(|event| matches!(event, GameEvent::ZoneChange { object, from: Zone::Stack, to: Zone::Graveyard } if *object == spell)).unwrap();
    assert!(death < draw && draw < cleanup);
    resolve(&mut state); assert_eq!(state.players[1].life, 47); assert!(state.stack.is_empty());
    assert_eq!(state.players[0].hand, vec![cards[0]]);
    assert!(!state.drain_events().iter().any(|event| matches!(event, GameEvent::CardDrawn { .. })));
}

#[test]
fn a_completed_prior_resolution_survives_unsupported_chain_rejection() {
    use mtg_gto::card::Effect;
    let (mut state, subject, spell, _) = fixture();
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: 999_505, name: "Prior life instruction".into(),
        card_types: vec![CardType::Instant], spell_effect: Some(Effect::GainLife { amount: 7 }), ..Default::default() });
    let prior = state.create_card_in_zone(999_505, 0, ZoneType::Stack).unwrap();
    state.stack.push(StackEntry { id: 2, source: StackSource::Spell(prior), controller: 0, targets: vec![], target_generations: vec![] });
    state.next_stack_id = 3; resolve(&mut state);
    assert_eq!(state.players[0].life, 47); assert!(state.players[0].graveyard.contains(&prior));
    let commander = state.commanders[0].object; state.players[0].command_zone.clear(); state.players[0].library.push(commander);
    assert_rejected_without_prefix(&mut state, subject, spell);
    assert_eq!(state.players[0].life, 47); assert!(state.players[0].graveyard.contains(&prior));
    assert_eq!(state.objects[&prior].zone_change_count, 1);
}

#[test]
fn actual_cast_requires_exactly_one_legal_creature_with_affordable_mana() {
    let (mut base, subject, spell, cards) = fixture();
    // Fixture placement: the real physical Chain begins in hand for casting.
    base.stack.clear(); base.players[0].hand.push(spell);
    base.players[0].mana_pool.black = 1; base.players[0].mana_pool.colorless = 2;
    let land = base.create_card_in_zone(sample::ids::SWAMP, 1, ZoneType::Battlefield).unwrap();
    for targets in [vec![], vec![Target::Player(1)], vec![Target::Object(land)], vec![Target::Object(subject), Target::Object(land)]] {
        let mut state = base.clone(); let action = Action::CastSpell { object_id: spell, targets };
        assert!(!mtg_gto::action::legal_actions(&state).contains(&action));
        let before = bincode::serialize(&state).unwrap(); rules::apply_action(&mut state, &action);
        assert_eq!(bincode::serialize(&state).unwrap(), before); assert!(state.players[0].hand.contains(&spell));
        assert_eq!((state.players[0].mana_pool.black, state.players[0].mana_pool.colorless), (1, 2));
    }
    let cast = Action::CastSpell { object_id: spell, targets: vec![Target::Object(subject)] };
    assert!(mtg_gto::action::legal_actions(&base).contains(&cast)); rules::apply_action(&mut base, &cast);
    assert_eq!(base.stack.len(), 1); assert!(!base.players[0].hand.contains(&spell)); resolve(&mut base);
    assert!(!base.gameplay_stopped()); assert!(base.players[1].graveyard.contains(&subject));
    assert_eq!(base.players[0].hand, vec![cards[0]]); assert!(base.players[0].graveyard.contains(&spell));
}
