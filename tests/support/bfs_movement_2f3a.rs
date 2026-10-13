//! Phase 3A search consumer regressions, kept separate from existing test bodies.
use super::*;
use mtg_gto::{card::{CardDef, CardType, Effect, TargetSpec, ZoneType},
    game::{CardDatabase, StackEntry, StackSource, Target}};

fn opponent_window() -> GameState {
    let mut database = CardDatabase::new();
    database.insert(CardDef { id: 998_391, name: "Opposing commander".into(),
        card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2), ..Default::default() });
    database.insert(CardDef { id: 998_392, name: "Owned resolution".into(), card_types: vec![CardType::Instant],
        spell_effect: Some(Effect::Multiple(vec![
            Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent },
            Effect::LoseLife { amount: 40, target: TargetSpec::Controller },
        ])), ..Default::default() });
    let mut state = GameState::new_commander(2); state.card_db = Some(Arc::new(database));
    state.phase = Phase::PreCombatMain;
    let commander = state.create_card_in_zone(998_391, 1, ZoneType::Battlefield).unwrap();
    state.designate_commander(commander, 0).unwrap(); state.objects.get_mut(&commander).unwrap().controller = 0;
    let spell = state.create_card_in_zone(998_392, 0, ZoneType::Stack).unwrap();
    state.stack.push(StackEntry { id: 1, source: StackSource::Spell(spell), controller: 0,
        targets: vec![Target::Object(commander)], target_generations: vec![Some(0)] });
    state.next_stack_id = 2;
    for _ in 0..2 { rules::apply_action(&mut state, &Action::PassPriority); }
    assert!(state.pending_movement.is_some()); assert_eq!(state.priority_player, 1); assert_eq!(state.active_player, 0);
    state
}
fn search(state: &mut GameState) -> SearchStats {
    let mut best_turn = 10; let mut best = Vec::new(); let mut current = Vec::new();
    let mut stats = SearchStats::new(); let mut visited = HashMap::new();
    let limits = SearchLimits { deadline: Instant::now() + std::time::Duration::from_secs(2),
        max_states: 10, max_visited: 10, max_depth: 10, trace: false };
    dfs_search(state, &mut best_turn, &mut best, &mut current, &kinnan_config(), &mut stats, &mut visited, &limits);
    stats
}

#[test]
fn opponent_owned_movement_is_answered_before_ordinary_auto_pass() {
    let mut state = opponent_window(); let before = bincode::serialize(&state).unwrap();
    // Answering the opponent's window completes the represented controller
    // life-loss suffix and reaches a terminal loss for the pilot. Auto-passing
    // that window would instead produce the old stalled-action outcome.
    let stats = search(&mut state);
    assert!(!stats.stalled); assert!(!stats.invalid); assert!(!stats.hit_state_cap);
    assert!(!stats.hit_depth_cap); assert!(!stats.timed_out);
    assert_eq!(classify_search(None, &stats), SearchOutcome::ExhaustedNoWin);
    assert_eq!(bincode::serialize(&state).unwrap(), before, "search retains its caller's suspended frame");
}

#[test]
fn malformed_opponent_window_disposes_search_as_state_encoding() {
    let mut state = opponent_window(); state.pending_movement.as_mut().unwrap().history[0].resulting_revision = 99;
    let before = bincode::serialize(&state).unwrap(); let stats = search(&mut state);
    assert!(stats.invalid); assert_eq!(stats.invalid_reason, "state_encoding"); assert!(!stats.stalled);
    assert_eq!(classify_search(None, &stats), SearchOutcome::Invalid("state_encoding"));
    assert_eq!(bincode::serialize(&state).unwrap(), before);
}

#[test]
fn reclamation_pruning_retains_public_graveyard_activations_as_meaningful_actions() {
    let mut state = GameState::new(2); state.card_db = Some(Arc::new(sample::build_sample_db())); state.phase = Phase::PreCombatMain;
    let source = state.create_card_in_zone(ids::PHYREXIAN_RECLAMATION, 0, ZoneType::Battlefield).unwrap();
    let target = state.create_card_in_zone(ids::ASHCOAT_OF_THE_SHADOW_SWARM, 0, ZoneType::Graveyard).unwrap();
    state.players[0].mana_pool.black = 1; state.players[0].mana_pool.colorless = 1;
    let action = Action::ActivateReclamation {source: state.exact_object(source).unwrap(), target: state.exact_object(target).unwrap()};
    let all = legal_actions(&state); assert!(all.contains(&action));
    let kept = prune_actions(&state, &all, &ashcoat_config()).unwrap(); assert!(kept.contains(&action));
    assert!(!kept.contains(&Action::EndTurn));
    let mut filtered = ashcoat_config(); filtered.filtered_abilities.insert(ids::PHYREXIAN_RECLAMATION);
    assert!(!prune_actions(&state, &all, &filtered).unwrap().contains(&action));
}
