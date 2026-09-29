use std::sync::Arc;

use crate::card::{CardDef, CardType, TokenDef, TriggerCondition, ZoneType};
use crate::game::{GameState, PlayerIndex};

/// Compute a stable CardId for a token type based on its properties.
/// Uses the high bit to avoid collisions with regular card IDs.
/// Uses FNV-1a hash for stability across Rust versions (DefaultHasher is not
/// guaranteed to be stable).
fn token_card_id(token_def: &TokenDef) -> u64 {
    // FNV-1a 64-bit constants
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in token_def.name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for byte in token_def.power.to_le_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for byte in token_def.toughness.to_le_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for color in &token_def.colors {
        let disc = *color as u8;
        hash ^= disc as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for kw in &token_def.keywords {
        let disc = *kw as u8;
        hash ^= disc as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash | (1u64 << 63)
}

/// Convert a TokenDef into a CardDef suitable for the card database.
fn token_to_card_def(token_def: &TokenDef, card_id: u64) -> CardDef {
    CardDef {
        id: card_id,
        name: token_def.name.clone(),
        colors: Some(token_def.colors.clone()),
        card_types: vec![CardType::Creature],
        subtypes: token_def.subtypes.clone(),
        keywords: token_def.keywords.clone(),
        power: Some(token_def.power as i32),
        toughness: Some(token_def.toughness as i32),
        oracle_text: format!("{}/{} {} Token", token_def.power, token_def.toughness, token_def.name),
        ..Default::default()
    }
}

/// Build a DynamicContext for the given controller, used when evaluating
/// DynamicValue variants that need hand/graveyard information.
pub(super) fn build_dynamic_context(state: &GameState, controller: PlayerIndex) -> crate::card::DynamicContext {
    let hand_size = state.players.get(controller)
        .map(|p| p.hand.len())
        .unwrap_or(0);
    let db = state.card_db();
    let mut graveyard_card_types = Vec::new();
    let mut creatures_in_graveyard = 0usize;
    if let Some(player) = state.players.get(controller) {
        for &gid in &player.graveyard {
            if let Some(gi) = state.objects.get(&gid) {
                if let Some(gdef) = db.get(gi.card_def_id) {
                    graveyard_card_types.push(gdef.card_types.clone());
                    if gdef.is_creature() {
                        creatures_in_graveyard += 1;
                    }
                }
            }
        }
    }
    crate::card::DynamicContext {
        hand_size,
        graveyard_card_types,
        creatures_in_graveyard,
    }
}

/// Create a token on the battlefield (CR 111.1).
///
/// Registers the token's CardDef in the card database (via Arc::make_mut,
/// which clones only if needed) and creates a CardInstance marked as a token.
/// Fires ETB triggers for the token.
pub(super) fn create_token(state: &mut GameState, token_def: &TokenDef, controller: PlayerIndex) {
    let card_id = token_card_id(token_def);

    // Register token CardDef in the database if not already present
    let needs_registration = state.card_db().get(card_id).is_none();
    if needs_registration {
        let def = token_to_card_def(token_def, card_id);
        if let Some(ref mut arc) = state.card_db {
            let db = Arc::make_mut(arc);
            if db.get(card_id).is_none() {
                db.insert(def);
            }
        }
    }

    // Create the token instance on the battlefield
    let obj_id = state.create_card_in_zone(card_id, controller, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&obj_id) {
        inst.controller = controller;
        inst.is_token = true;
        inst.summoning_sick = true;
    }

    // Refresh continuous effects for any static abilities on the token
    state.refresh_continuous_effects();

    // Fire ETB triggers (self-ETB for the token, and watcher ETBs for other permanents)
    let _ = super::triggers::fire_triggers(state, TriggerCondition::EntersBattlefield, Some(obj_id));
    // Fire "whenever a creature enters" watcher triggers on other permanents
    super::triggers::check_triggers(state, TriggerCondition::ACreatureEnters, None);
    let _ = super::triggers::flush_triggers(state);
}

/// Create a token on the battlefield from a combo macro-action.
///
/// Lightweight variant of `create_token` that skips ETB triggers and
/// continuous effect refresh — suitable for batch token creation where
/// firing triggers per-token would be prohibitively expensive.
pub(crate) fn create_token_from_combo(
    state: &mut GameState,
    token_def: &TokenDef,
    controller: PlayerIndex,
) {
    let card_id = token_card_id(token_def);

    let needs_registration = state.card_db().get(card_id).is_none();
    if needs_registration {
        let def = token_to_card_def(token_def, card_id);
        if let Some(ref mut arc) = state.card_db {
            let db = Arc::make_mut(arc);
            if db.get(card_id).is_none() {
                db.insert(def);
            }
        }
    }

    let obj_id = state.create_card_in_zone(card_id, controller, ZoneType::Battlefield);
    if let Some(inst) = state.objects.get_mut(&obj_id) {
        inst.controller = controller;
        inst.is_token = true;
        inst.summoning_sick = true;
    }
}
