use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;

use crate::action::Action;
use crate::card::{ObjectId, ZoneType};
use crate::game::{GameState, Phase};

/// Advance the mulligan state machine after a keep or bottom-card decision.
///
/// Flow:
/// 1. If current player kept but still has cards to bottom → stay (bottom actions generated)
/// 2. If current player is done → advance to next player who hasn't decided
/// 3. If all players are done deciding and bottoming → transition to Turn 1
pub(super) fn advance_mulligan(state: &mut GameState) {
    if state.pending_copy_order.is_some() { return; }
    let player = state.priority_player;
    let ps = &state.players[player];

    // If this player kept but still needs to bottom cards, stay on them
    if ps.mulligan_decided {
        let target_hand_size = 7u32.saturating_sub(ps.mulligan_count) as usize;
        if ps.hand.len() > target_hand_size {
            return; // More bottom-card decisions needed
        }
    }

    // This player is fully done — find the next player who needs action
    let num_players = state.players.len();
    for offset in 1..=num_players {
        let next = (player + offset) % num_players;
        let nps = &state.players[next];
        if !nps.mulligan_decided {
            state.priority_player = next;
            return;
        }
        let target = 7u32.saturating_sub(nps.mulligan_count) as usize;
        if nps.hand.len() > target {
            state.priority_player = next;
            return;
        }
    }

    // All players done — transition to Turn 1
    state.phase = Phase::Untap;
    state.active_player = 0;
    state.priority_player = 0;
    state.turn_number = 1;
    super::phases::execute_phase_entry(state);
}

/// Set up a game from two decklists. Shuffles libraries and draws opening hands.
pub fn setup_game(
    state: &mut GameState,
    deck0: &[crate::card::CardId],
    deck1: &[crate::card::CardId],
) {
    if state.pending_copy_order.is_some() { return; }
    let mut rng = rand::thread_rng();

    // Create card instances for player 0
    let mut lib0: Vec<ObjectId> = deck0
        .iter()
        .map(|&card_id| state.create_card_in_zone(card_id, 0, ZoneType::Library))
        .collect();
    lib0.shuffle(&mut rng);
    state.players[0].library = lib0;

    // Create card instances for player 1
    let mut lib1: Vec<ObjectId> = deck1
        .iter()
        .map(|&card_id| state.create_card_in_zone(card_id, 1, ZoneType::Library))
        .collect();
    lib1.shuffle(&mut rng);
    state.players[1].library = lib1;

    // Draw opening hands (7 cards each)
    super::draw_cards(state, 0, 7);
    super::draw_cards(state, 1, 7);

    // Set starting state
    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::Untap;
    state.turn_number = 1;

    // Execute first untap step (which auto-advances to upkeep -> draw)
    super::phases::execute_phase_entry(state);
}

/// Set up a game with a fixed random seed for deterministic replay.
///
/// Given the same seed, the same deck order and opening hands are produced
/// every time, allowing games to be replayed for debugging.
pub fn setup_game_seeded(
    state: &mut GameState,
    deck0: &[crate::card::CardId],
    deck1: &[crate::card::CardId],
    seed: u64,
) {
    if state.pending_copy_order.is_some() { return; }
    let mut rng = StdRng::seed_from_u64(seed);

    let mut lib0: Vec<ObjectId> = deck0
        .iter()
        .map(|&card_id| state.create_card_in_zone(card_id, 0, ZoneType::Library))
        .collect();
    lib0.shuffle(&mut rng);
    state.players[0].library = lib0;

    let mut lib1: Vec<ObjectId> = deck1
        .iter()
        .map(|&card_id| state.create_card_in_zone(card_id, 1, ZoneType::Library))
        .collect();
    lib1.shuffle(&mut rng);
    state.players[1].library = lib1;

    super::draw_cards(state, 0, 7);
    super::draw_cards(state, 1, 7);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::Untap;
    state.turn_number = 1;
    super::phases::execute_phase_entry(state);
}

/// Set up a Commander game with a fixed random seed for deterministic replay.
pub fn setup_commander_game_seeded(
    state: &mut GameState,
    deck0: &[crate::card::CardId],
    deck1: &[crate::card::CardId],
    commander0: crate::card::CardId,
    commander1: crate::card::CardId,
    seed: u64,
) {
    if state.pending_copy_order.is_some() { return; }
    let mut rng = StdRng::seed_from_u64(seed);

    state.players[0].commander_card_id = Some(commander0);
    state.players[1].commander_card_id = Some(commander1);

    let mut lib0 = Vec::new();
    let mut found_commander0 = false;
    for &card_id in deck0 {
        if card_id == commander0 && !found_commander0 {
            let obj_id = state.create_card_in_zone(card_id, 0, ZoneType::Command);
            state.players[0].commander_object_id = Some(obj_id);
            found_commander0 = true;
        } else {
            lib0.push(state.create_card_in_zone(card_id, 0, ZoneType::Library));
        }
    }
    lib0.shuffle(&mut rng);
    state.players[0].library = lib0;

    let mut lib1 = Vec::new();
    let mut found_commander1 = false;
    for &card_id in deck1 {
        if card_id == commander1 && !found_commander1 {
            let obj_id = state.create_card_in_zone(card_id, 1, ZoneType::Command);
            state.players[1].commander_object_id = Some(obj_id);
            found_commander1 = true;
        } else {
            lib1.push(state.create_card_in_zone(card_id, 1, ZoneType::Library));
        }
    }
    lib1.shuffle(&mut rng);
    state.players[1].library = lib1;

    super::draw_cards(state, 0, 7);
    super::draw_cards(state, 1, 7);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::Untap;
    state.turn_number = 1;
    super::phases::execute_phase_entry(state);
}

/// Set up a Commander game from two decklists. The commander card ID is
/// specified separately — it starts in the command zone rather than the
/// library.
///
/// `deck0`/`deck1` should contain all 100 cards including the commander.
/// The commander is extracted from the list and placed in the command zone.
pub fn setup_commander_game(
    state: &mut GameState,
    deck0: &[crate::card::CardId],
    deck1: &[crate::card::CardId],
    commander0: crate::card::CardId,
    commander1: crate::card::CardId,
) {
    if state.pending_copy_order.is_some() { return; }
    let mut rng = rand::thread_rng();

    // Record commander designations
    state.players[0].commander_card_id = Some(commander0);
    state.players[1].commander_card_id = Some(commander1);

    // Player 0: create all cards, put commander in command zone, rest in library
    let mut lib0 = Vec::new();
    let mut found_commander0 = false;
    for &card_id in deck0 {
        if card_id == commander0 && !found_commander0 {
            let obj_id = state.create_card_in_zone(card_id, 0, ZoneType::Command);
            state.players[0].commander_object_id = Some(obj_id);
            found_commander0 = true;
        } else {
            lib0.push(state.create_card_in_zone(card_id, 0, ZoneType::Library));
        }
    }
    lib0.shuffle(&mut rng);
    state.players[0].library = lib0;

    // Player 1: same
    let mut lib1 = Vec::new();
    let mut found_commander1 = false;
    for &card_id in deck1 {
        if card_id == commander1 && !found_commander1 {
            let obj_id = state.create_card_in_zone(card_id, 1, ZoneType::Command);
            state.players[1].commander_object_id = Some(obj_id);
            found_commander1 = true;
        } else {
            lib1.push(state.create_card_in_zone(card_id, 1, ZoneType::Library));
        }
    }
    lib1.shuffle(&mut rng);
    state.players[1].library = lib1;

    // Draw opening hands (7 cards each)
    super::draw_cards(state, 0, 7);
    super::draw_cards(state, 1, 7);

    // Set starting state — begin in Mulligan phase so players can decide
    // whether to keep or mulligan before the game starts.
    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::Mulligan;
    state.turn_number = 1;
}

/// Set up a Commander game with Partner commanders.
///
/// Each player can have one or two commanders. If a partner is provided, both
/// start in the command zone and each has independent commander tax.
/// The combined color identity of both commanders is used for deck validation.
pub fn setup_commander_game_with_partners(
    state: &mut GameState,
    deck0: &[crate::card::CardId],
    deck1: &[crate::card::CardId],
    commander0: crate::card::CardId,
    partner0: Option<crate::card::CardId>,
    commander1: crate::card::CardId,
    partner1: Option<crate::card::CardId>,
) {
    if state.pending_copy_order.is_some() { return; }
    let mut rng = rand::thread_rng();

    // Record commander designations
    state.players[0].commander_card_id = Some(commander0);
    state.players[1].commander_card_id = Some(commander1);
    if let Some(p) = partner0 {
        state.players[0].partner_commander_card_id = Some(p);
    }
    if let Some(p) = partner1 {
        state.players[1].partner_commander_card_id = Some(p);
    }

    // Player 0: create all cards, put commanders in command zone, rest in library
    let mut lib0 = Vec::new();
    let mut found_commander0 = false;
    let mut found_partner0 = false;
    for &card_id in deck0 {
        if card_id == commander0 && !found_commander0 {
            let obj_id = state.create_card_in_zone(card_id, 0, ZoneType::Command);
            state.players[0].commander_object_id = Some(obj_id);
            found_commander0 = true;
        } else if partner0 == Some(card_id) && !found_partner0 {
            let obj_id = state.create_card_in_zone(card_id, 0, ZoneType::Command);
            state.players[0].partner_commander_object_id = Some(obj_id);
            found_partner0 = true;
        } else {
            lib0.push(state.create_card_in_zone(card_id, 0, ZoneType::Library));
        }
    }
    lib0.shuffle(&mut rng);
    state.players[0].library = lib0;

    // Player 1: same
    let mut lib1 = Vec::new();
    let mut found_commander1 = false;
    let mut found_partner1 = false;
    for &card_id in deck1 {
        if card_id == commander1 && !found_commander1 {
            let obj_id = state.create_card_in_zone(card_id, 1, ZoneType::Command);
            state.players[1].commander_object_id = Some(obj_id);
            found_commander1 = true;
        } else if partner1 == Some(card_id) && !found_partner1 {
            let obj_id = state.create_card_in_zone(card_id, 1, ZoneType::Command);
            state.players[1].partner_commander_object_id = Some(obj_id);
            found_partner1 = true;
        } else {
            lib1.push(state.create_card_in_zone(card_id, 1, ZoneType::Library));
        }
    }
    lib1.shuffle(&mut rng);
    state.players[1].library = lib1;

    // Draw opening hands (7 cards each)
    super::draw_cards(state, 0, 7);
    super::draw_cards(state, 1, 7);

    state.active_player = 0;
    state.priority_player = 0;
    state.phase = Phase::Mulligan;
    state.turn_number = 1;
}

/// Configure tutor targets for a player.
///
/// When tutor targets are set, `SearchLibrary` effects present the player with
/// a choice (`ChooseTutorTarget`) restricted to this set instead of automatically
/// taking the top card. This allows MCCFR to learn which card to tutor for in
/// each game state.
///
/// Call this after `setup_game` or `setup_commander_game` but before the game
/// loop begins.
pub fn set_tutor_targets(
    state: &mut GameState,
    player: crate::game::PlayerIndex,
    targets: &[crate::card::CardId],
) {
    if state.pending_copy_order.is_some() { return; }
    state.players[player].tutor_targets = targets.to_vec();
}

/// Reshuffle opening hands for goldfish training iterations.
///
/// Returns all hand cards to each player's library, shuffles both libraries,
/// and redraws 7-card opening hands. Resets turn/phase/mana state so the
/// game starts cleanly from Turn 1.
pub fn reshuffle_opening_hand(state: &mut GameState) {
    if state.pending_copy_order.is_some() { return; }
    let mut rng = rand::thread_rng();

    for player in 0..state.players.len() {
        // Move hand cards back to library
        let hand: Vec<crate::card::ObjectId> = state.players[player].hand.drain(..).collect();
        for obj_id in hand {
            state.players[player].library.push(obj_id);
        }

        // Shuffle library
        state.players[player].library.shuffle(&mut rng);

        // Reset per-player state
        state.players[player].land_plays_remaining = 1;
        state.players[player].mana_pool.drain();
        state.players[player].has_drawn_for_turn = false;
        state.players[player].mulligan_count = 0;
        state.players[player].mulligan_decided = false;
    }

    // Redraw opening hands (7 cards each)
    super::draw_cards(state, 0, 7);
    super::draw_cards(state, 1, 7);

    // Reset game state
    state.active_player = 0;
    state.priority_player = 0;
    state.turn_number = 1;

    // Commander games start in Mulligan phase; non-commander skip straight to Turn 1
    if state.is_commander_format() {
        state.phase = Phase::Mulligan;
        resolve_mulligans_with_heuristic(state);
    } else {
        state.phase = Phase::Untap;
        super::phases::execute_phase_entry(state);
    }
}

/// Resolve the Mulligan phase using a simple land-count heuristic.
///
/// Uses the same logic as `GreedyStrategy`: count effective mana sources
/// (lands = 1.0, cheap mana rocks/dorks = 0.7), keep if 1.5-6.0 effective
/// sources, mulligan otherwise (up to 2 mulligans), bottom targeted spells
/// first then highest-CMC non-mana-producing cards.
fn resolve_mulligans_with_heuristic(state: &mut GameState) {
    while state.phase == Phase::Mulligan {
        let player = state.priority_player;
        let ps = &state.players[player];

        if !ps.mulligan_decided {
            // Count effective mana sources: lands + cheap mana producers
            let mut mana_sources = 0.0f64;
            for &obj_id in &ps.hand {
                if let Some(inst) = state.objects.get(&obj_id) {
                    if let Some(def) = state.card_db().get(inst.card_def_id) {
                        if def.is_land() {
                            mana_sources += 1.0;
                        } else if !def.mana_abilities.is_empty() && def.cmc() <= 2 {
                            mana_sources += 0.7;
                        }
                    }
                }
            }

            let action = if (1.5..=6.0).contains(&mana_sources) || ps.mulligan_count >= 2 {
                Action::MulliganKeep
            } else {
                Action::MulliganMulligan
            };
            super::apply_action(state, &action);
            continue;
        }

        // Bottom-card decision: bottom targeted spells first (dead in
        // goldfish), then highest-CMC non-land, non-mana-producing cards.
        let target_hand_size = 7u32.saturating_sub(ps.mulligan_count) as usize;
        if ps.hand.len() > target_hand_size {
            let mut worst_obj = ps.hand[0];
            let mut worst_score = -1i32;
            for &obj_id in &ps.hand {
                if let Some(inst) = state.objects.get(&obj_id) {
                    let def = state.card_db().get(inst.card_def_id);
                    let score = if def.map_or(false, |d| d.is_land()) {
                        0 // keep lands
                    } else if def.map_or(false, |d| !d.mana_abilities.is_empty()) {
                        1 // keep mana producers
                    } else if def.map_or(false, |d| {
                        crate::action::spell_requires_target(d)
                    }) {
                        100 // bottom targeted spells (dead in goldfish)
                    } else {
                        def.map_or(5, |d| d.cmc() as i32 + 2)
                    };
                    if score > worst_score {
                        worst_score = score;
                        worst_obj = obj_id;
                    }
                }
            }
            super::apply_action(state, &Action::MulliganBottomCard { object_id: worst_obj });
        } else {
            // Should not happen — advance_mulligan transitions out
            break;
        }
    }
}

/// Validate a Commander deck:
/// - Exactly 100 cards (including commander(s))
/// - Singleton (max 1 copy of each non-basic-land card)
/// - Commander must be a legendary creature
/// - Optional partner commander must also be legendary with Partner keyword
/// - All cards must match the combined color identity
///
/// Returns Ok(()) or an error message.
pub fn validate_commander_deck(
    db: &crate::game::CardDatabase,
    deck: &[crate::card::CardId],
    commander: crate::card::CardId,
) -> Result<(), String> {
    validate_commander_deck_with_partner(db, deck, commander, None)
}

/// Validate a Commander deck with an optional partner commander.
pub fn validate_commander_deck_with_partner(
    db: &crate::game::CardDatabase,
    deck: &[crate::card::CardId],
    commander: crate::card::CardId,
    partner: Option<crate::card::CardId>,
) -> Result<(), String> {
    if deck.len() != 100 {
        return Err(format!(
            "Commander deck must be exactly 100 cards, got {}",
            deck.len()
        ));
    }

    // Commander must be in the deck
    if !deck.contains(&commander) {
        return Err("Commander card must be included in the deck".to_string());
    }

    // Commander must be a legendary creature
    if let Some(def) = db.get(commander) {
        let is_legendary = def.supertypes.contains(&crate::card::Supertype::Legendary);
        let is_creature = def.is_creature();
        if !is_legendary || !is_creature {
            return Err(format!(
                "Commander '{}' must be a legendary creature",
                def.name
            ));
        }
    } else {
        return Err(format!("Commander card ID {} not found in database", commander));
    }

    // Validate partner if provided
    if let Some(partner_id) = partner {
        if !deck.contains(&partner_id) {
            return Err("Partner commander must be included in the deck".to_string());
        }
        if let Some(def) = db.get(partner_id) {
            let is_legendary = def.supertypes.contains(&crate::card::Supertype::Legendary);
            let is_creature = def.is_creature();
            if !is_legendary || !is_creature {
                return Err(format!(
                    "Partner commander '{}' must be a legendary creature",
                    def.name
                ));
            }
            if !def.keywords.contains(&crate::card::KeywordAbility::Partner) {
                return Err(format!(
                    "Partner commander '{}' must have the Partner keyword",
                    def.name
                ));
            }
        } else {
            return Err(format!("Partner commander card ID {} not found in database", partner_id));
        }
        // Primary commander must also have Partner when using a partner
        if let Some(def) = db.get(commander) {
            if !def.keywords.contains(&crate::card::KeywordAbility::Partner) {
                return Err(format!(
                    "Commander '{}' must have the Partner keyword to use with a partner",
                    def.name
                ));
            }
        }
    }

    // Color identity check (CR 903.4) — combined identity for partner pairs
    let mut commander_identity: std::collections::HashSet<crate::mana::Color> = db
        .get(commander)
        .map(|d| d.color_identity().into_iter().collect())
        .unwrap_or_default();
    if let Some(partner_id) = partner {
        if let Some(def) = db.get(partner_id) {
            for color in def.color_identity() {
                commander_identity.insert(color);
            }
        }
    }
    for &card_id in deck {
        if let Some(def) = db.get(card_id) {
            let card_identity = def.color_identity();
            for color in &card_identity {
                if !commander_identity.contains(color) {
                    return Err(format!(
                        "'{}' has color {} which is outside the commander's color identity",
                        def.name, color
                    ));
                }
            }
        }
    }

    // Singleton check: no more than 1 copy of non-basic-land cards
    let mut counts: std::collections::HashMap<crate::card::CardId, u32> =
        std::collections::HashMap::new();
    for &card_id in deck {
        *counts.entry(card_id).or_insert(0) += 1;
    }
    for (&card_id, &count) in &counts {
        if count > 1 {
            if let Some(def) = db.get(card_id) {
                if !def.is_basic_land() {
                    return Err(format!(
                        "Commander decks allow only 1 copy of non-basic '{}', found {}",
                        def.name, count
                    ));
                }
            }
        }
    }

    Ok(())
}
