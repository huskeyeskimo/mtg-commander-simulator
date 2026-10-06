//! Phase 13.1: Rules assertion framework.
//!
//! Targeted tests for individual rules: combat keywords, state-based actions,
//! mana payment, triggered abilities, and stack resolution. Each test sets up
//! a minimal game state, applies one or more actions, and verifies the correct
//! rule is enforced.

use std::collections::HashSet;
use std::sync::Arc;

use mtg_gto::action::{legal_actions, Action};
use mtg_gto::card::sample::{self, ids};
use mtg_gto::card::ZoneType;
use mtg_gto::game::{GameState, Phase, Target};
use mtg_gto::rules;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a minimal 2-player state with sample database and libraries so
/// neither player decks out.
fn base_state() -> GameState {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    for _ in 0..30 {
        state.create_card_in_zone(ids::MOUNTAIN, 0, ZoneType::Library);
        state.create_card_in_zone(ids::FOREST, 1, ZoneType::Library);
    }
    state.active_player = 0;
    state.priority_player = 0;
    state.turn_number = 3; // past summoning sickness
    state
}

/// Put a creature on the battlefield for a player, ready to attack (untapped,
/// not summoning sick).
fn add_creature(state: &mut GameState, card_id: u64, player: usize) -> u64 {
    let obj_id = state.create_card_in_zone(card_id, player, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&obj_id) {
        inst.tapped = false;
        inst.summoning_sick = false;
    }
    obj_id
}

/// Put a land on the battlefield for a player, untapped.
fn add_land(state: &mut GameState, card_id: u64, player: usize) -> u64 {
    let obj_id = state.create_card_in_zone(card_id, player, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&obj_id) {
        inst.tapped = false;
    }
    obj_id
}

/// Put a card in a player's hand.
fn add_to_hand(state: &mut GameState, card_id: u64, player: usize) -> u64 {
    state.create_card_in_zone(card_id, player, ZoneType::Hand)
}

/// Advance through priority passes until the stack is empty.
fn resolve_stack(state: &mut GameState) {
    let mut safety = 0;
    while !state.stack.is_empty() && safety < 100 {
        rules::apply_action(state, &Action::PassPriority);
        safety += 1;
    }
}

// ===========================================================================
// Combat keyword tests
// ===========================================================================

#[test]
fn test_first_strike_kills_before_normal_damage() {
    let mut state = base_state();
    // White Knight: 2/2 first strike
    let wk = add_creature(&mut state, ids::WHITE_KNIGHT, 0);
    // Grizzly Bears: 2/2 no first strike
    let bears = add_creature(&mut state, ids::GRIZZLY_BEARS, 1);

    state.phase = Phase::DeclareAttackers;

    rules::apply_action(
        &mut state,
        &Action::DeclareAttackers {
            attackers: vec![wk],
        },
    );

    rules::apply_action(
        &mut state,
        &Action::DeclareBlockers {
            blocks: vec![(bears, wk)],
        },
    );

    let mut safety = 0;
    while state.phase != Phase::PostCombatMain && !state.game_over && safety < 50 {
        rules::apply_action(&mut state, &Action::PassPriority);
        safety += 1;
    }

    // White Knight (first strike) should survive; Bears should be dead
    assert!(
        state.battlefield.contains(&wk),
        "First strike creature should survive"
    );
    assert!(
        !state.battlefield.contains(&bears),
        "Normal creature should die to first strike before dealing damage"
    );
}

#[test]
fn test_deathtouch_creature_deals_combat_damage() {
    // Verify that a deathtouch creature deals damage normally when unblocked.
    let mut state = base_state();
    // Typhoid Rats: 1/1 deathtouch (no flying)
    let rats = add_creature(&mut state, ids::TYPHOID_RATS, 0);

    let p1_life_initial = state.players[1].life;

    state.phase = Phase::DeclareAttackers;
    rules::apply_action(
        &mut state,
        &Action::DeclareAttackers {
            attackers: vec![rats],
        },
    );

    // No blockers
    rules::apply_action(
        &mut state,
        &Action::DeclareBlockers {
            blocks: vec![],
        },
    );

    let mut safety = 0;
    while state.phase != Phase::PostCombatMain && !state.game_over && safety < 50 {
        rules::apply_action(&mut state, &Action::PassPriority);
        safety += 1;
    }

    // Rats deal 1 combat damage to player
    assert!(
        state.players[1].life < p1_life_initial,
        "Deathtouch creature should deal combat damage to player"
    );
}

#[test]
fn test_lifelink_gains_life_on_combat_damage() {
    let mut state = base_state();
    // Vampire Nighthawk: 2/3 flying deathtouch lifelink
    let hawk = add_creature(&mut state, ids::VAMPIRE_NIGHTHAWK, 0);

    let life_before = state.players[0].life;

    state.phase = Phase::DeclareAttackers;
    rules::apply_action(
        &mut state,
        &Action::DeclareAttackers {
            attackers: vec![hawk],
        },
    );
    rules::apply_action(
        &mut state,
        &Action::DeclareBlockers {
            blocks: vec![],
        },
    );

    let mut safety = 0;
    while state.phase != Phase::PostCombatMain && !state.game_over && safety < 50 {
        rules::apply_action(&mut state, &Action::PassPriority);
        safety += 1;
    }

    assert_eq!(
        state.players[0].life,
        life_before + 2,
        "Lifelink should gain life equal to damage dealt"
    );
}

#[test]
fn test_trample_deals_excess_damage_to_player() {
    let mut state = base_state();
    // Rancor Beast should have trample
    let trampler = add_creature(&mut state, ids::RANCOR_BEAST, 0);
    let blocker = add_creature(&mut state, ids::SAVANNAH_LIONS, 1); // 2/1

    let p1_life_before = state.players[1].life;

    state.phase = Phase::DeclareAttackers;
    rules::apply_action(
        &mut state,
        &Action::DeclareAttackers {
            attackers: vec![trampler],
        },
    );

    rules::apply_action(
        &mut state,
        &Action::DeclareBlockers {
            blocks: vec![(blocker, trampler)],
        },
    );

    let mut safety = 0;
    while state.phase != Phase::PostCombatMain && !state.game_over && safety < 50 {
        rules::apply_action(&mut state, &Action::PassPriority);
        safety += 1;
    }

    let power = {
        let db = state.card_db();
        let def = db.get(ids::RANCOR_BEAST).unwrap();
        def.power.unwrap_or(0)
    };
    let blocker_toughness = 1; // Savannah Lions is 2/1

    if power > blocker_toughness {
        let expected_trample = power - blocker_toughness;
        assert!(
            state.players[1].life <= p1_life_before - expected_trample as i32,
            "Trample should deal {} excess damage to player, life went from {} to {}",
            expected_trample,
            p1_life_before,
            state.players[1].life
        );
    }
}

#[test]
fn test_flying_cannot_be_blocked_by_non_flyer() {
    let mut state = base_state();
    // Serra Angel: 4/4 flying vigilance
    let flyer = add_creature(&mut state, ids::SERRA_ANGEL, 0);
    // Grizzly Bears: 2/2 no flying
    let ground = add_creature(&mut state, ids::GRIZZLY_BEARS, 1);

    state.phase = Phase::DeclareAttackers;
    rules::apply_action(
        &mut state,
        &Action::DeclareAttackers {
            attackers: vec![flyer],
        },
    );

    // Check that legal actions don't allow ground creature to block flyer
    let actions = legal_actions(&state);
    let block_actions: Vec<&Action> = actions
        .iter()
        .filter(|a| matches!(a, Action::DeclareBlockers { blocks } if !blocks.is_empty()))
        .collect();

    // Non-flyer should NOT be able to block a flyer
    for action in &block_actions {
        if let Action::DeclareBlockers { blocks } = action {
            for &(blocker_id, attacker_id) in blocks {
                assert!(
                    !(blocker_id == ground && attacker_id == flyer),
                    "Non-flying creature should not be able to block a flyer"
                );
            }
        }
    }
}

#[test]
fn test_vigilance_does_not_tap_when_attacking() {
    let mut state = base_state();
    // Serra Angel: 4/4 flying vigilance
    let angel = add_creature(&mut state, ids::SERRA_ANGEL, 0);

    state.phase = Phase::DeclareAttackers;
    rules::apply_action(
        &mut state,
        &Action::DeclareAttackers {
            attackers: vec![angel],
        },
    );

    assert!(
        !state.objects[&angel].tapped,
        "Creature with vigilance should not tap when attacking"
    );
}

// ===========================================================================
// State-based action tests
// ===========================================================================

#[test]
fn test_sba_creature_zero_toughness_dies() {
    let mut state = base_state();
    let bears = add_creature(&mut state, ids::GRIZZLY_BEARS, 0);

    state.objects.get_mut(&bears).unwrap().damage_marked = 2;

    rules::check_state_based_actions(&mut state);

    assert!(
        !state.battlefield.contains(&bears),
        "Creature with lethal damage should be removed by SBA"
    );
    assert!(
        state.players[0].graveyard.contains(&bears),
        "Dead creature should be in graveyard"
    );
}

#[test]
fn test_sba_player_zero_life_loses() {
    let mut state = base_state();
    state.players[1].life = 0;

    rules::check_state_based_actions(&mut state);

    assert!(state.game_over, "Game should be over when a player is at 0 life");
    assert_eq!(
        state.winner,
        Some(0),
        "Player 0 should win when player 1 is at 0 life"
    );
}

#[test]
fn test_sba_legendary_rule() {
    let mut state = base_state();
    let thrun1 = add_creature(&mut state, ids::THRUN_LAST_TROLL, 0);
    let thrun2 = add_creature(&mut state, ids::THRUN_LAST_TROLL, 0);

    rules::check_state_based_actions(&mut state);

    let on_battlefield = [thrun1, thrun2]
        .iter()
        .filter(|id| state.battlefield.contains(id))
        .count();
    assert_eq!(
        on_battlefield, 1,
        "Legendary rule: only one copy of a legendary creature should survive"
    );
}

// ===========================================================================
// Stack resolution tests
// ===========================================================================

#[test]
fn test_lightning_bolt_deals_3_damage() {
    let mut state = base_state();
    add_land(&mut state, ids::MOUNTAIN, 0);
    add_to_hand(&mut state, ids::LIGHTNING_BOLT, 0);

    state.phase = Phase::PreCombatMain;

    let p1_life_before = state.players[1].life;

    let actions = legal_actions(&state);
    let bolt_action = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, targets, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::LIGHTNING_BOLT)
            && targets.contains(&Target::Player(1)))
    });

    if let Some(action) = bolt_action {
        rules::apply_action(&mut state, action);
        resolve_stack(&mut state);

        assert_eq!(
            state.players[1].life,
            p1_life_before - 3,
            "Lightning Bolt should deal 3 damage to target"
        );
    } else {
        panic!("Should be able to cast Lightning Bolt with a Mountain");
    }
}

#[test]
fn test_wrath_of_god_destroys_all_creatures() {
    let mut state = base_state();
    for _ in 0..4 {
        add_land(&mut state, ids::PLAINS, 0);
    }
    add_to_hand(&mut state, ids::WRATH_OF_GOD, 0);

    let bear0 = add_creature(&mut state, ids::GRIZZLY_BEARS, 0);
    let ogre1 = add_creature(&mut state, ids::GREY_OGRE, 1);

    state.phase = Phase::PreCombatMain;

    let actions = legal_actions(&state);
    let wrath_action = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::WRATH_OF_GOD))
    });

    if let Some(action) = wrath_action {
        rules::apply_action(&mut state, action);
        resolve_stack(&mut state);

        assert!(
            !state.battlefield.contains(&bear0),
            "Wrath should destroy P0's creatures"
        );
        assert!(
            !state.battlefield.contains(&ogre1),
            "Wrath should destroy P1's creatures"
        );
    } else {
        panic!("Should be able to cast Wrath of God with 4 Plains");
    }
}

// ===========================================================================
// Mana payment tests
// ===========================================================================

#[test]
fn test_cannot_cast_spell_without_enough_mana() {
    let mut state = base_state();
    add_land(&mut state, ids::MOUNTAIN, 0);
    add_to_hand(&mut state, ids::SERRA_ANGEL, 0);

    state.phase = Phase::PreCombatMain;

    let actions = legal_actions(&state);
    let angel_action = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::SERRA_ANGEL))
    });

    assert!(
        angel_action.is_none(),
        "Should not be able to cast Serra Angel with only 1 Mountain"
    );
}

#[test]
fn test_sol_ring_provides_2_colorless() {
    let mut state = base_state();
    add_land(&mut state, ids::MOUNTAIN, 0);
    let sr = add_creature(&mut state, ids::SOL_RING, 0);
    if let Some(inst) = state.objects.get_mut(&sr) {
        inst.tapped = false;
        inst.summoning_sick = false;
    }
    add_to_hand(&mut state, ids::GREY_OGRE, 0);

    state.phase = Phase::PreCombatMain;

    let actions = legal_actions(&state);
    let ogre_action = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::GREY_OGRE))
    });

    assert!(
        ogre_action.is_some(),
        "Sol Ring + Mountain should pay for Grey Ogre (2R)"
    );
}

// ===========================================================================
// Trigger tests
// ===========================================================================

#[test]
fn test_etb_trigger_draws_card() {
    let mut state = base_state();
    for _ in 0..2 {
        add_land(&mut state, ids::FOREST, 0);
    }
    add_to_hand(&mut state, ids::ELVISH_VISIONARY, 0);

    state.phase = Phase::PreCombatMain;

    let hand_size_before = state.players[0].hand.len();

    let actions = legal_actions(&state);
    let vis_action = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::ELVISH_VISIONARY))
    });

    if let Some(action) = vis_action {
        rules::apply_action(&mut state, action);
        resolve_stack(&mut state);

        assert!(
            state.players[0].hand.len() >= hand_size_before,
            "Elvish Visionary ETB should draw a card"
        );
    }
}

#[test]
fn test_undying_returns_with_counter() {
    use mtg_gto::card::{CardDef, CardType, KeywordAbility};
    let mut state = base_state();
    Arc::make_mut(state.card_db.as_mut().unwrap()).insert(CardDef { id: 996_801,
        name: "Executable Undying fixture".into(), card_types: vec![CardType::Creature],
        power: Some(2), toughness: Some(1), keywords: vec![KeywordAbility::Undying],
        ..Default::default() });
    let subject = add_creature(&mut state, 996_801, 0);
    let generation = state.objects[&subject].zone_change_count;
    state.objects.get_mut(&subject).unwrap().damage_marked = 1;
    rules::check_state_based_actions(&mut state);
    assert!(state.players[0].graveyard.contains(&subject));
    assert_eq!(state.stack.len(), 1, "ordinary Undying trigger must be placed");
    resolve_stack(&mut state);
    assert!(state.battlefield.contains(&subject));
    assert_eq!(state.objects[&subject].plus_counters, 1);
    assert_eq!(state.objects[&subject].zone_change_count, generation + 2);
}

// ===========================================================================
// Turn structure tests
// ===========================================================================

#[test]
fn test_land_play_limited_to_one_per_turn() {
    let mut state = base_state();
    add_to_hand(&mut state, ids::MOUNTAIN, 0);
    add_to_hand(&mut state, ids::MOUNTAIN, 0);

    state.phase = Phase::PreCombatMain;
    state.players[0].land_plays_remaining = 1;

    let actions = legal_actions(&state);
    let land_plays: Vec<&Action> = actions
        .iter()
        .filter(|a| matches!(a, Action::PlayLand { .. }))
        .collect();

    assert!(
        !land_plays.is_empty(),
        "Should be able to play a land when land_plays_remaining > 0"
    );

    rules::apply_action(&mut state, land_plays[0]);

    let actions2 = legal_actions(&state);
    let land_plays2: Vec<&Action> = actions2
        .iter()
        .filter(|a| matches!(a, Action::PlayLand { .. }))
        .collect();

    assert!(
        land_plays2.is_empty(),
        "Should not be able to play a second land in the same turn"
    );
}

#[test]
fn test_summoning_sickness_prevents_attacking() {
    let mut state = base_state();
    let bears = add_creature(&mut state, ids::GRIZZLY_BEARS, 0);
    state.objects.get_mut(&bears).unwrap().summoning_sick = true;

    state.phase = Phase::DeclareAttackers;

    let actions = legal_actions(&state);
    let attack_with_bears = actions.iter().any(|a| {
        if let Action::DeclareAttackers { attackers } = a {
            attackers.contains(&bears)
        } else {
            false
        }
    });

    assert!(
        !attack_with_bears,
        "Summoning-sick creature should not be able to attack"
    );
}

#[test]
fn test_haste_ignores_summoning_sickness() {
    let mut state = base_state();
    let guide = add_creature(&mut state, ids::GOBLIN_GUIDE, 0);
    state.objects.get_mut(&guide).unwrap().summoning_sick = true;

    state.phase = Phase::DeclareAttackers;

    let actions = legal_actions(&state);
    let attack_with_guide = actions.iter().any(|a| {
        if let Action::DeclareAttackers { attackers } = a {
            attackers.contains(&guide)
        } else {
            false
        }
    });

    assert!(
        attack_with_guide,
        "Creature with haste should be able to attack despite summoning sickness"
    );
}

// ===========================================================================
// Anthem / static ability tests
// ===========================================================================

#[test]
fn test_glorious_anthem_pumps_creatures() {
    let mut state = base_state();
    let _anthem = add_creature(&mut state, ids::GLORIOUS_ANTHEM, 0);
    let bears = add_creature(&mut state, ids::GRIZZLY_BEARS, 0);

    // Register the anthem's static abilities as continuous effects
    state.refresh_continuous_effects();

    let bf_set: HashSet<u64> = state.battlefield.iter().copied().collect();
    let chars = mtg_gto::layers::compute_characteristics(
        bears,
        &state.continuous_effects,
        &state.objects,
        &bf_set,
        state.card_db.as_ref().unwrap(),
    )
    .expect("Should compute characteristics for creature");

    assert!(
        chars.power >= 3,
        "Glorious Anthem should pump creature to at least 3 power, got {}",
        chars.power
    );
    assert!(
        chars.toughness >= 3,
        "Glorious Anthem should pump creature to at least 3 toughness, got {}",
        chars.toughness
    );
}

// Loss-boundary controls use represented thresholds without changing ordinary
// creature SBA, Commander identity, or leaving-game semantics.
#[test]
fn test_terminal_loss_boundary_threshold_controls_do_not_stop_ordinary_play() {
    let mut state = base_state();
    state.format = mtg_gto::game::GameFormat::Commander;
    state.players[0].life = 1;
    state.players[0].poison_counters = 9;
    state.players[0].commander_damage_received = vec![20, 20];
    let victim = add_creature(&mut state, ids::GRIZZLY_BEARS, 0);
    state.objects.get_mut(&victim).unwrap().damage_marked = 2;
    rules::check_state_based_actions(&mut state);
    assert!(!state.gameplay_stopped());
    assert!(state.players[0].graveyard.contains(&victim));
    assert!(state.loss_boundary.terminal.is_none());
    assert!(state.loss_boundary.unsupported.is_none());
    assert!(!legal_actions(&state).is_empty());
}

#[test]
fn test_terminal_loss_boundary_supported_all_lost_draw_is_stable() {
    let mut state = base_state();
    state.players[0].life = 0;
    state.players[1].life = 0;
    rules::check_state_based_actions(&mut state);
    assert!(state.game_over);
    assert_eq!(state.winner, None);
    assert_eq!(state.loss_boundary.terminal.as_ref().unwrap().losses.len(), 2);
    let settled = bincode::serialize(&state).unwrap();
    rules::apply_action(&mut state, &Action::PassPriority);
    rules::apply_action(&mut state, &Action::EndTurn);
    rules::check_state_based_actions(&mut state);
    assert_eq!(bincode::serialize(&state).unwrap(), settled);
    assert!(legal_actions(&state).is_empty());
}
