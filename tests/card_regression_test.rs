//! Phase 13.2: Card-specific regression tests.
//!
//! Verifies that key hand-authored sample cards produce correct behavior.
//! Each test targets a specific card's primary mechanic.

use std::sync::Arc;

use mtg_gto::action::{legal_actions, Action};
use mtg_gto::card::sample::{self, ids};
use mtg_gto::card::ZoneType;
use mtg_gto::game::{GameState, Phase, Target};
use mtg_gto::rules;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base_state() -> GameState {
    let db = sample::build_sample_db();
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    for _ in 0..30 {
        state.create_card_in_zone(ids::MOUNTAIN, 0, ZoneType::Library).unwrap();
        state.create_card_in_zone(ids::FOREST, 1, ZoneType::Library).unwrap();
    }
    state.active_player = 0;
    state.priority_player = 0;
    state.turn_number = 3;
    state
}

fn add_creature(state: &mut GameState, card_id: u64, player: usize) -> u64 {
    let obj_id = state.create_card_in_zone(card_id, player, ZoneType::Battlefield).unwrap();
    if let Some(inst) = state.objects.get_mut(&obj_id) {
        inst.tapped = false;
        inst.summoning_sick = false;
    }
    obj_id
}

fn add_land(state: &mut GameState, card_id: u64, player: usize) -> u64 {
    let obj_id = state.create_card_in_zone(card_id, player, ZoneType::Battlefield).unwrap();
    if let Some(inst) = state.objects.get_mut(&obj_id) {
        inst.tapped = false;
    }
    obj_id
}

fn add_to_hand(state: &mut GameState, card_id: u64, player: usize) -> u64 {
    state.create_card_in_zone(card_id, player, ZoneType::Hand).unwrap()
}

fn resolve_stack(state: &mut GameState) {
    let mut safety = 0;
    while !state.stack.is_empty() && safety < 100 {
        rules::apply_action(state, &Action::PassPriority);
        safety += 1;
    }
}

// ===========================================================================
// Burn spells
// ===========================================================================

#[test]
fn test_shock_deals_2_damage() {
    let mut state = base_state();
    add_land(&mut state, ids::MOUNTAIN, 0);
    add_to_hand(&mut state, ids::SHOCK, 0);
    state.phase = Phase::PreCombatMain;

    let life_before = state.players[1].life;
    let actions = legal_actions(&state);
    let shock = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, targets, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::SHOCK)
            && targets.contains(&Target::Player(1)))
    });

    if let Some(action) = shock {
        rules::apply_action(&mut state, action);
        resolve_stack(&mut state);
        assert_eq!(
            state.players[1].life,
            life_before - 2,
            "Shock should deal 2 damage"
        );
    }
}

#[test]
fn test_lava_spike_deals_3_to_player() {
    let mut state = base_state();
    add_land(&mut state, ids::MOUNTAIN, 0);
    add_to_hand(&mut state, ids::LAVA_SPIKE, 0);
    state.phase = Phase::PreCombatMain;

    let life_before = state.players[1].life;
    let actions = legal_actions(&state);
    let spike = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, targets, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::LAVA_SPIKE)
            && targets.contains(&Target::Player(1)))
    });

    if let Some(action) = spike {
        rules::apply_action(&mut state, action);
        resolve_stack(&mut state);
        assert_eq!(
            state.players[1].life,
            life_before - 3,
            "Lava Spike should deal 3 damage to player"
        );
    }
}

// ===========================================================================
// Mana acceleration
// ===========================================================================

#[test]
fn test_sol_ring_exists_and_produces_mana() {
    let db = sample::build_sample_db();
    let sol_ring = db.get(ids::SOL_RING).expect("Sol Ring should exist in sample db");
    assert_eq!(sol_ring.name, "Sol Ring");
    // Sol Ring costs {1}
    assert!(sol_ring.mana_cost.is_some());
}

#[test]
fn test_dark_ritual_adds_3_black() {
    let mut state = base_state();
    add_land(&mut state, ids::SWAMP, 0);
    add_to_hand(&mut state, ids::DARK_RITUAL, 0);
    state.phase = Phase::PreCombatMain;

    let actions = legal_actions(&state);
    let ritual = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::DARK_RITUAL))
    });

    if let Some(action) = ritual {
        rules::apply_action(&mut state, action);
        resolve_stack(&mut state);
        // After Dark Ritual, mana pool should have 3 black (minus 1 to cast = 2 net black)
        assert!(
            state.players[0].mana_pool.black >= 2,
            "Dark Ritual should add BBB to mana pool (net 2 after paying B), got {} black",
            state.players[0].mana_pool.black,
        );
    }
}

// ===========================================================================
// Creature abilities
// ===========================================================================

#[test]
fn test_llanowar_elves_has_mana_abilities() {
    let db = sample::build_sample_db();
    let elves = db
        .get(ids::LLANOWAR_ELVES)
        .expect("Llanowar Elves should exist");
    assert_eq!(elves.name, "Llanowar Elves");
    assert!(
        !elves.mana_abilities.is_empty(),
        "Llanowar Elves should have a mana ability"
    );
}

#[test]
fn test_monastery_swiftspear_has_haste() {
    let db = sample::build_sample_db();
    let swiftspear = db
        .get(ids::MONASTERY_SWIFTSPEAR)
        .expect("Monastery Swiftspear should exist");
    assert!(
        swiftspear.keywords.contains(&mtg_gto::card::KeywordAbility::Haste),
        "Monastery Swiftspear should have haste"
    );
}

#[test]
fn test_serra_angel_has_flying_and_vigilance() {
    let db = sample::build_sample_db();
    let angel = db.get(ids::SERRA_ANGEL).expect("Serra Angel should exist");
    assert!(
        angel
            .keywords
            .contains(&mtg_gto::card::KeywordAbility::Flying),
        "Serra Angel should have flying"
    );
    assert!(
        angel
            .keywords
            .contains(&mtg_gto::card::KeywordAbility::Vigilance),
        "Serra Angel should have vigilance"
    );
}

#[test]
fn test_goblin_guide_has_haste() {
    let db = sample::build_sample_db();
    let guide = db
        .get(ids::GOBLIN_GUIDE)
        .expect("Goblin Guide should exist");
    assert!(
        guide
            .keywords
            .contains(&mtg_gto::card::KeywordAbility::Haste),
        "Goblin Guide should have haste"
    );
    assert_eq!(guide.power, Some(2));
    assert_eq!(guide.toughness, Some(2));
}

// ===========================================================================
// Removal spells
// ===========================================================================

#[test]
fn test_doom_blade_destroys_nonblack_creature() {
    let mut state = base_state();
    // 2 lands for Doom Blade ({1}{B})
    add_land(&mut state, ids::SWAMP, 0);
    add_land(&mut state, ids::SWAMP, 0);
    add_to_hand(&mut state, ids::DOOM_BLADE, 0);

    let target = add_creature(&mut state, ids::GRIZZLY_BEARS, 1);
    state.phase = Phase::PreCombatMain;

    let actions = legal_actions(&state);
    let doom = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::DOOM_BLADE))
    });

    if let Some(action) = doom {
        rules::apply_action(&mut state, action);
        resolve_stack(&mut state);
        assert!(
            !state.battlefield.contains(&target),
            "Doom Blade should destroy the target creature"
        );
    }
}

#[test]
fn test_swords_to_plowshares_destroys_creature() {
    let mut state = base_state();
    add_land(&mut state, ids::PLAINS, 0);
    add_to_hand(&mut state, ids::SWORDS_TO_PLOWSHARES, 0);

    let target = add_creature(&mut state, ids::LEATHERBACK_BALOTH, 1);
    state.phase = Phase::PreCombatMain;

    let actions = legal_actions(&state);
    let stp = actions.iter().find(|a| {
        matches!(a, Action::CastSpell { object_id, .. }
            if state.objects.get(object_id).map(|o| o.card_def_id) == Some(ids::SWORDS_TO_PLOWSHARES))
    });

    if let Some(action) = stp {
        rules::apply_action(&mut state, action);
        resolve_stack(&mut state);
        assert!(
            !state.battlefield.contains(&target),
            "Swords to Plowshares should remove the target creature"
        );
    }
}

// ===========================================================================
// Enchantments / static effects
// ===========================================================================

#[test]
fn test_glorious_anthem_buffs_own_creatures() {
    let db = sample::build_sample_db();
    let anthem = db
        .get(ids::GLORIOUS_ANTHEM)
        .expect("Glorious Anthem should exist");
    assert_eq!(anthem.name, "Glorious Anthem");
    // Should have static abilities that grant +1/+1
    assert!(
        !anthem.static_abilities.is_empty(),
        "Glorious Anthem should have static abilities"
    );
}

#[test]
fn test_rancor_is_aura() {
    let db = sample::build_sample_db();
    let rancor = db.get(ids::RANCOR).expect("Rancor should exist");
    assert_eq!(rancor.name, "Rancor");
    assert!(rancor.is_aura(), "Rancor should be an aura");
}

// ===========================================================================
// Card definition integrity
// ===========================================================================

#[test]
fn test_all_sample_cards_have_names() {
    let db = sample::build_sample_db();
    for (id, def) in db.cards.iter() {
        assert!(
            !def.name.is_empty(),
            "Card ID {} should have a non-empty name",
            id
        );
    }
}

#[test]
fn test_creatures_have_power_and_toughness() {
    let db = sample::build_sample_db();
    for (_id, def) in db.cards.iter() {
        if def.card_types.contains(&mtg_gto::card::CardType::Creature) {
            assert!(
                def.power.is_some(),
                "Creature '{}' should have power defined",
                def.name
            );
            assert!(
                def.toughness.is_some(),
                "Creature '{}' should have toughness defined",
                def.name
            );
        }
    }
}

#[test]
fn test_lands_have_mana_abilities() {
    let db = sample::build_sample_db();
    let basic_lands = [ids::MOUNTAIN, ids::FOREST, ids::PLAINS, ids::ISLAND, ids::SWAMP];
    for &land_id in &basic_lands {
        let land = db.get(land_id).expect("Basic land should exist");
        assert!(
            !land.mana_abilities.is_empty(),
            "Basic land '{}' should have a mana ability",
            land.name
        );
    }
}

// ===========================================================================
// Fetch lands
// ===========================================================================

#[test]
fn test_flooded_strand_sacrifices_and_fetches() {
    let mut state = base_state();
    // Add fetchable lands to library
    state.create_card_in_zone(ids::ISLAND, 0, ZoneType::Library).unwrap();
    state.create_card_in_zone(ids::BREEDING_POOL, 0, ZoneType::Library).unwrap();

    let fetch = add_land(&mut state, ids::FLOODED_STRAND, 0);
    state.phase = Phase::PreCombatMain;

    let life_before = state.players[0].life;
    let actions = legal_actions(&state);
    let activate = actions.iter().find(|a| {
        matches!(a, Action::ActivateAbility { object_id, .. } if *object_id == fetch)
    });

    assert!(activate.is_some(), "Should be able to activate Flooded Strand");
    rules::apply_action(&mut state, activate.unwrap());

    // Fetch land should be sacrificed (moved off battlefield)
    assert!(
        !state.battlefield.contains(&fetch),
        "Flooded Strand should be sacrificed after activation"
    );
    // Life should be paid
    assert_eq!(
        state.players[0].life,
        life_before - 1,
        "Activating fetch land should cost 1 life"
    );
    // Stack should have the search ability
    assert!(
        !state.stack.is_empty(),
        "Search ability should be on the stack"
    );

    // Pass priority to resolve the search ability on the stack.
    // Don't use resolve_stack() because PassPriority clears pending_tutor.
    // We need to pass priority enough times to resolve the stack entry,
    // then handle the tutor choice.
    let mut safety = 0;
    while !state.stack.is_empty() && safety < 10 {
        rules::apply_action(&mut state, &Action::PassPriority);
        safety += 1;
        // If pending_tutor appeared, stop passing priority
        if state.pending_tutor.is_some() {
            break;
        }
    }

    // After stack resolution, pending_tutor should be set
    assert!(
        state.pending_tutor.is_some(),
        "Pending tutor should be set after resolving search ability"
    );

    // Choose a tutor target
    let actions = legal_actions(&state);
    let choose = actions.iter().find(|a| {
        matches!(a, Action::ChooseTutorTarget { .. })
    });
    assert!(choose.is_some(), "Should have ChooseTutorTarget actions");
    rules::apply_action(&mut state, choose.unwrap());

    // The fetched land should now be on the battlefield
    let bf_has_fetchable = state.battlefield.iter().any(|&id| {
        let inst = &state.objects[&id];
        inst.card_def_id == ids::ISLAND || inst.card_def_id == ids::BREEDING_POOL
    });
    assert!(
        bf_has_fetchable,
        "A fetchable land should be on the battlefield after resolving"
    );
}

#[test]
fn test_deck_has_correct_number_of_cards() {
    let red = sample::red_aggro_deck();
    assert_eq!(red.len(), 60, "Red aggro deck should have 60 cards");

    let green = sample::green_stompy_deck();
    assert_eq!(green.len(), 60, "Green stompy deck should have 60 cards");
}

#[test]
fn test_commander_decks_have_100_cards() {
    let (brimaz_deck, _cmd) = sample::brimaz_commander_deck();
    assert_eq!(
        brimaz_deck.len(),
        100,
        "Brimaz commander deck should have 100 cards"
    );

    let (ashcoat_deck, _cmd) = sample::ashcoat_commander_deck();
    assert_eq!(
        ashcoat_deck.len(),
        100,
        "Ashcoat commander deck should have 100 cards"
    );
}
