mod combat;
mod effects;
mod mana;
mod phases;
mod resolution;
mod sba;
mod setup;
mod tokens;
mod triggers;

use rand::Rng;

use crate::action::Action;
use crate::card::{CardType, KeywordAbility, ManaAbility, ObjectId, SacrificeCost, TriggerCondition, ZoneType};
use crate::events::{GameEvent, Zone};
use crate::game::{GameState, Phase, PlayerIndex, StackEntry, StackSource};
use crate::mana::Color;

// Public API re-exports
pub use mana::{total_cost_reduction, apply_cost_reduction, auto_tap_lands, spell_cost_reduction, total_cost_increase};
pub use sba::check_state_based_actions;
pub use triggers::fire_triggers;
pub use setup::{setup_game, setup_game_seeded, setup_commander_game, setup_commander_game_seeded, setup_commander_game_with_partners, set_tutor_targets, reshuffle_opening_hand, validate_commander_deck, validate_commander_deck_with_partner};
pub(crate) use tokens::create_token_from_combo;

/// Apply an action to the game state, advancing it.
pub fn apply_action(state: &mut GameState, action: &Action) {
    match action {
        Action::PassPriority => {
            // If there's a pending tutor, passing means "fail to find" —
            // clear it and return without advancing priority normally.
            if state.pending_tutor.is_some() {
                state.pending_tutor = None;
                return;
            }

            if state.phase == Phase::Cleanup
                && state.players[state.active_player].hand.len() > 7
            {
                debug_assert!(
                    false,
                    "PassPriority during cleanup discard is illegal; choose a Discard action."
                );
                return;
            }
            state.consecutive_passes += 1;
            phases::handle_priority_pass(state);
        }

        Action::Discard { object_id } => {
            if state.phase != Phase::Cleanup {
                return;
            }
            if state.priority_player != state.active_player {
                return;
            }
            let player = state.active_player;
            if !state.players[player].hand.contains(object_id) {
                return;
            }
            if state.players[player].hand.len() <= 7 {
                return;
            }

            state.move_object(*object_id, ZoneType::Hand, ZoneType::Graveyard);
            state.consecutive_passes = 0;
            state.priority_player = player;

            // Fire discard triggers (e.g., Monument to Endurance)
            triggers::check_triggers(state, TriggerCondition::YouDiscardACard, None);
            let _ = triggers::flush_triggers(state);

            if state.players[player].hand.len() <= 7 {
                phases::finalize_cleanup(state);
            }
        }

        Action::PlayLand { object_id } => {
            let obj_id = *object_id;
            let player = state.priority_player;
            state.players[player].land_plays_remaining -= 1;
            state.move_object(obj_id, ZoneType::Hand, ZoneType::Battlefield);
            // Lands enter untapped by default (we'd check for "enters tapped" later)
            if let Some(inst) = state.objects.get_mut(&obj_id) {
                inst.tapped = false;
                inst.summoning_sick = false; // lands don't have summoning sickness
            }
            state.refresh_continuous_effects();
            // Fire ETB triggers on the land itself (e.g., Mystic Sanctuary)
            let _ = triggers::fire_triggers(state, TriggerCondition::EntersBattlefield, Some(obj_id));
            // Fire landfall triggers on all permanents
            triggers::check_triggers(state, TriggerCondition::ALandYouControlEnters, None);
            // Fire "whenever you play a land" triggers
            triggers::check_triggers(state, TriggerCondition::YouPlayALand, None);
            let _ = triggers::flush_triggers(state);
            state.consecutive_passes = 0;
        }

        Action::PlayLandFromGraveyard { object_id } => {
            let obj_id = *object_id;
            let player = state.priority_player;
            state.players[player].land_plays_remaining -= 1;
            state.move_object(obj_id, ZoneType::Graveyard, ZoneType::Battlefield);
            if let Some(inst) = state.objects.get_mut(&obj_id) {
                inst.tapped = false;
                inst.summoning_sick = false;
            }
            state.refresh_continuous_effects();
            // Fire ETB triggers on the land itself
            let _ = triggers::fire_triggers(state, TriggerCondition::EntersBattlefield, Some(obj_id));
            // Fire landfall triggers on all permanents
            triggers::check_triggers(state, TriggerCondition::ALandYouControlEnters, None);
            // Fire "whenever you play a land" triggers
            triggers::check_triggers(state, TriggerCondition::YouPlayALand, None);
            let _ = triggers::flush_triggers(state);
            state.consecutive_passes = 0;
        }

        Action::CastSpell { object_id, targets } => {
            let obj_id = *object_id;
            let player = state.priority_player;
            let db = state.card_db();
            let inst = &state.objects[&obj_id];
            let def = db.get(inst.card_def_id).unwrap().clone();
            let is_creature = def.is_creature();

            // Pay mana cost (with cost reduction, spell keywords, and tax effects)
            let card_def_id = {
                state.objects[&obj_id].card_def_id
            };
            if let Some(ref cost) = def.mana_cost {
                let reduction = mana::total_cost_reduction(state, player, is_creature);
                let spell_reduction = mana::spell_cost_reduction(state, player, card_def_id);
                let tax = mana::total_cost_increase(state, player, is_creature);
                let mut reduced_cost = mana::apply_cost_reduction(cost, reduction + spell_reduction);
                reduced_cost.generic += tax;
                // First, auto-tap lands to generate mana if pool is insufficient
                mana::auto_tap_lands(state, player, &reduced_cost);
                // Then pay from pool — if payment fails, abort the cast
                if !state.players[player].mana_pool.pay(&reduced_cost) {
                    return;
                }
            }

            // Move to stack
            let stack_id = state.new_stack_id();
            state.stack.push(StackEntry {
                id: stack_id,
                source: StackSource::Spell(obj_id),
                controller: player,
                targets: targets.clone(),
            });
            // Remove from hand (but don't put in a zone yet — it's on the stack)
            state.players[player].hand.retain(|&id| id != obj_id);

            state.emit_event(GameEvent::SpellCast {
                object: obj_id,
                controller: player,
            });
            state.emit_event(GameEvent::ZoneChange {
                object: obj_id,
                from: Zone::Hand,
                to: Zone::Stack,
            });

            // Fire spell-cast triggers (YouCastSpell, OpponentCastsSpell, etc.)
            triggers::fire_spell_cast_triggers(state, player, is_creature);

            state.consecutive_passes = 0;
        }

        Action::ActivateManaAbility {
            object_id,
            ability_index,
        } => {
            let obj_id = *object_id;
            let idx = *ability_index;
            let player = state.priority_player;

            if !state.can_pay_tap_cost(obj_id) {
                return;
            }

            // Read mana ability and source properties in one borrow scope
            let (ma, source_is_nonland, source_is_swamp) = {
                let db = state.card_db();
                let inst = &state.objects[&obj_id];
                let def = db.get(inst.card_def_id).unwrap();
                (
                    def.mana_abilities.get(idx).cloned(),
                    !def.card_types.contains(&CardType::Land),
                    def.subtypes.iter().any(|s| s.0 == "Swamp"),
                )
            };

            if let Some(ma) = ma {
                match &ma {
                    ManaAbility::TapForColor(color) => {
                        state.players[player].mana_pool.add_color(*color, 1);
                    }
                    ManaAbility::TapForColorless => {
                        state.players[player].mana_pool.colorless += 1;
                    }
                    ManaAbility::TapForAny => {
                        state.players[player].mana_pool.colorless += 1;
                    }
                    ManaAbility::TapForLegendaryColors => {
                        // Mox Amber: add one mana of any color among
                        // legendary creatures/planeswalkers you control.
                        let leg_colors = mana::legendary_colors(state, player);
                        if let Some(&c) = leg_colors.first() {
                            state.players[player].mana_pool.add_color(c, 1);
                        }
                        // If no legendary creature/planeswalker, produces nothing
                    }
                    ManaAbility::TapForChoice(colors) => {
                        if let Some(&color) = colors.first() {
                            state.players[player].mana_pool.add_color(color, 1);
                        }
                    }
                    ManaAbility::TapForColorlessAmount(n) => {
                        state.players[player].mana_pool.colorless += n;
                    }
                }

                // Check for ManaFromNonlandBonus (e.g., Kinnan, Bonder Prodigy)
                if source_is_nonland {
                    let bonus = triggers::mana_from_nonland_bonus_count(state, player);
                    if bonus > 0 {
                        state.players[player].mana_pool.colorless += bonus;
                    }
                }

                // Check for ManaFromSwampBonus (e.g., Nirkana Revenant, Crypt Ghast)
                if source_is_swamp {
                    let bonus = triggers::mana_from_swamp_bonus_count(state, player);
                    if bonus > 0 {
                        state.players[player].mana_pool.add_color(Color::Black, bonus);
                    }
                }
            }

            // Tap the permanent
            if let Some(inst) = state.objects.get_mut(&obj_id) {
                inst.tapped = true;
            }
        }

        Action::ActivateAbility {
            object_id,
            ability_index,
            targets,
        } => {
            let obj_id = *object_id;
            let idx = *ability_index;
            let player = state.priority_player;

            // Clone ability info to avoid borrow conflict
            let ability = {
                let db = state.card_db();
                let inst = &state.objects[&obj_id];
                let def = db.get(inst.card_def_id).unwrap();
                def.activated_abilities.get(idx).cloned()
            };

            if let Some(ability) = ability {
                if ability.requires_tap && !state.can_pay_tap_cost(obj_id) {
                    return;
                }
                // Pay mana cost
                mana::auto_tap_lands_excluding(
                    state, player, &ability.cost, ability.requires_tap.then_some(obj_id),
                );
                if !state.players[player].mana_pool.pay(&ability.cost) {
                    return;
                }

                // Pay life cost (e.g., fetch lands pay 1 life)
                if ability.life_cost > 0 {
                    state.players[player].life -= ability.life_cost as i32;
                }

                // Tap if required
                if ability.requires_tap {
                    if let Some(inst) = state.objects.get_mut(&obj_id) {
                        inst.tapped = true;
                    }
                }

                // Pay sacrifice cost
                match &ability.sacrifice_cost {
                    Some(SacrificeCost::SelfSacrifice) => {
                        // Sacrifice the source permanent itself (e.g., fetch lands)
                        state.move_object(obj_id, ZoneType::Battlefield, ZoneType::Graveyard);
                    }
                    Some(SacrificeCost::AnyCreature) | Some(SacrificeCost::CreatureWithSubtype(_)) => {
                        // Sacrifice another creature — for now, this is handled
                        // by the combo discovery engine. Full implementation would
                        // require choosing a sacrifice target from legal_actions.
                    }
                    None => {}
                }

                let stack_id = state.new_stack_id();
                state.stack.push(StackEntry {
                    id: stack_id,
                    source: StackSource::ActivatedAbility {
                        source_id: obj_id,
                        ability_index: idx,
                    },
                    controller: player,
                    targets: targets.clone(),
                });
            }
            state.consecutive_passes = 0;
        }

        Action::DeclareAttackers { attackers } => {
            state.combat.clear();
            state.combat.attackers = attackers.clone();

            // Collect which attackers need tapping (those without vigilance)
            let to_tap: Vec<ObjectId> = attackers
                .iter()
                .filter(|&&id| !state.has_keyword(id, KeywordAbility::Vigilance))
                .copied()
                .collect();
            for id in to_tap {
                if let Some(inst) = state.objects.get_mut(&id) {
                    inst.tapped = true;
                }
            }

            state.consecutive_passes = 0;
            if attackers.is_empty() {
                // No attackers — skip combat entirely
                phases::transition_to_phase(state, Phase::EndOfCombat);
                state.priority_player = state.active_player;
            } else {
                // Exalted: if exactly one creature attacks, each permanent with
                // Exalted gives it +1/+1 until EOT.
                if attackers.len() == 1 {
                    triggers::apply_exalted(state, attackers[0]);
                }

                // Annihilator: for each attacker with Annihilator N, the defending
                // player sacrifices N permanents (simplified: random selection).
                triggers::apply_annihilator(state, attackers);

                // Batch-check attack triggers for all attackers before flushing
                for &attacker_id in attackers {
                    triggers::check_triggers(state, TriggerCondition::Attacks, Some(attacker_id));
                }
                let flushed = triggers::flush_triggers(state);

                if flushed {
                    phases::transition_to_phase(state, Phase::DeclareBlockers);
                    state.priority_player = state.next_player(state.active_player);
                }
            }
        }

        Action::DeclareBlockers { blocks } => {
            state.combat.blockers.clear();
            state.combat.attacker_blockers.clear();

            for &(blocker, attacker) in blocks {
                state.combat.blockers.insert(blocker, attacker);
                state
                    .combat
                    .attacker_blockers
                    .entry(attacker)
                    .or_default()
                    .push(blocker);
            }

            // Advance to combat damage (skip first strike if not applicable)
            state.consecutive_passes = 0;
            phases::transition_to_phase(state, Phase::FirstStrikeDamage);
            // Execute the first strike damage step entry (which may skip to CombatDamage)
            phases::execute_phase_entry_public(state);
        }

        Action::OrderDamageAssignment {
            attacker,
            assignment,
        } => {
            state
                .combat
                .damage_assignment
                .insert(*attacker, assignment.clone());
            state.consecutive_passes = 0;
        }

        Action::OrderTriggers { ordering } => {
            let player = state.priority_player;

            // Place this player's triggers on the stack in the chosen order.
            for &(source_id, ability_index) in ordering {
                if let Some(pos) = state.pending_triggers.iter().position(|t| {
                    t.controller == player
                        && t.source_id == source_id
                        && t.ability_index == ability_index
                }) {
                    let trigger = state.pending_triggers.remove(pos);
                    triggers::push_trigger_to_stack(state, &trigger);
                }
            }

            // Continue flushing remaining triggers (the other player's).
            let _ = triggers::flush_triggers(state);
        }

        Action::CastCommander { object_id, targets } => {
            let obj_id = *object_id;
            let player = state.priority_player;
            let db = state.card_db();
            let inst = &state.objects[&obj_id];
            let def = db.get(inst.card_def_id).unwrap().clone();
            let is_creature = def.is_creature();

            // Determine if this is the partner commander (separate tax tracking)
            let is_partner = state.players[player].partner_commander_object_id == Some(obj_id);

            // Pay mana cost with commander tax (and cost reduction)
            if let Some(ref cost) = def.mana_cost {
                let tax = if is_partner {
                    state.players[player].partner_commander_tax
                } else {
                    state.players[player].commander_tax
                };
                let reduction = mana::total_cost_reduction(state, player, is_creature);
                let mut taxed_cost = cost.clone();
                taxed_cost.generic += tax * 2;
                let final_cost = mana::apply_cost_reduction(&taxed_cost, reduction);
                mana::auto_tap_lands(state, player, &final_cost);
                if !state.players[player].mana_pool.pay(&final_cost) {
                    return;
                }
            }

            // Increment commander tax for next cast (separate tracking per partner)
            if is_partner {
                state.players[player].partner_commander_tax += 1;
            } else {
                state.players[player].commander_tax += 1;
            }

            // Move to stack
            let stack_id = state.new_stack_id();
            state.stack.push(StackEntry {
                id: stack_id,
                source: StackSource::Spell(obj_id),
                controller: player,
                targets: targets.clone(),
            });
            // Remove from command zone
            state.players[player].command_zone.retain(|&id| id != obj_id);

            state.emit_event(GameEvent::SpellCast {
                object: obj_id,
                controller: player,
            });
            state.emit_event(GameEvent::ZoneChange {
                object: obj_id,
                from: Zone::Command,
                to: Zone::Stack,
            });

            // Fire spell-cast triggers (YouCastSpell, OpponentCastsSpell, etc.)
            triggers::fire_spell_cast_triggers(state, player, is_creature);

            state.consecutive_passes = 0;
        }

        Action::Equip { equipment_id, target_id } => {
            let eq_id = *equipment_id;
            let tgt_id = *target_id;
            let player = state.priority_player;

            // Pay equip cost
            let equip_cost = {
                let db = state.card_db();
                let inst = &state.objects[&eq_id];
                let def = db.get(inst.card_def_id).unwrap();
                def.equip_cost.clone()
            };
            if let Some(cost) = equip_cost {
                mana::auto_tap_lands(state, player, &cost);
                if !state.players[player].mana_pool.pay(&cost) {
                    return;
                }
            }

            // Detach from previous creature (if any)
            if let Some(old_target) = state.objects.get(&eq_id).and_then(|i| i.attached_to) {
                if let Some(old_inst) = state.objects.get_mut(&old_target) {
                    old_inst.attachments.retain(|&id| id != eq_id);
                }
            }

            // Attach to new creature
            if let Some(eq_inst) = state.objects.get_mut(&eq_id) {
                eq_inst.attached_to = Some(tgt_id);
            }
            if let Some(tgt_inst) = state.objects.get_mut(&tgt_id) {
                if !tgt_inst.attachments.contains(&eq_id) {
                    tgt_inst.attachments.push(eq_id);
                }
            }

            state.refresh_continuous_effects();
            state.consecutive_passes = 0;
        }

        Action::ActivateLoyalty { object_id, ability_index } => {
            let obj_id = *object_id;
            let ab_idx = *ability_index;

            // Read the loyalty ability info
            let (loyalty_cost, _effect, controller) = {
                let db = state.card_db();
                let inst = &state.objects[&obj_id];
                let def = db.get(inst.card_def_id).unwrap();
                let la = &def.loyalty_abilities[ab_idx];
                (la.cost, la.effect.clone(), inst.controller)
            };

            // Adjust loyalty counters
            if loyalty_cost >= 0 {
                // +N: add counters
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    inst.loyalty_counters += loyalty_cost as u32;
                    inst.loyalty_activated_this_turn = true;
                }
            } else {
                // -N: remove counters
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    inst.loyalty_counters = inst.loyalty_counters.saturating_sub((-loyalty_cost) as u32);
                    inst.loyalty_activated_this_turn = true;
                }
            }

            // Put the ability on the stack
            let stack_id = state.new_stack_id();
            state.stack.push(crate::game::StackEntry {
                id: stack_id,
                source: crate::game::StackSource::ActivatedAbility {
                    source_id: obj_id,
                    ability_index: ab_idx,
                },
                controller,
                targets: Vec::new(),
            });
            state.consecutive_passes = 0;
        }

        Action::CastFromGraveyard { object_id, targets } => {
            let obj_id = *object_id;
            let player = state.priority_player;
            let (def, is_flashback, escape_exile_count) = {
                let db = state.card_db();
                let inst = &state.objects[&obj_id];
                let def = db.get(inst.card_def_id).unwrap().clone();
                let is_flashback = def.flashback_cost.is_some();
                let escape_count = def.escape_exile_count;
                (def, is_flashback, escape_count)
            };
            let is_creature = def.is_creature();

            // Pay the appropriate cost
            if is_flashback {
                if let Some(ref fb_cost) = def.flashback_cost {
                    let reduction = mana::total_cost_reduction(state, player, is_creature);
                    let reduced_cost = mana::apply_cost_reduction(fb_cost, reduction);
                    mana::auto_tap_lands(state, player, &reduced_cost);
                    if !state.players[player].mana_pool.pay(&reduced_cost) {
                        return;
                    }
                }
            } else if let Some(exile_count) = escape_exile_count {
                // Escape: pay regular mana cost + exile N cards from graveyard
                if let Some(ref cost) = def.mana_cost {
                    let reduction = mana::total_cost_reduction(state, player, is_creature);
                    let reduced_cost = mana::apply_cost_reduction(cost, reduction);
                    mana::auto_tap_lands(state, player, &reduced_cost);
                    if !state.players[player].mana_pool.pay(&reduced_cost) {
                        return;
                    }
                }
                // Exile N other cards from graveyard as additional cost
                let mut exiled = 0u32;
                let gy: Vec<ObjectId> = state.players[player].graveyard.clone();
                for &gy_id in &gy {
                    if exiled >= exile_count {
                        break;
                    }
                    if gy_id != obj_id {
                        state.move_object(gy_id, ZoneType::Graveyard, ZoneType::Exile);
                        exiled += 1;
                    }
                }
            }

            // Move to stack from graveyard
            let stack_id = state.new_stack_id();
            state.stack.push(StackEntry {
                id: stack_id,
                source: StackSource::Spell(obj_id),
                controller: player,
                targets: targets.clone(),
            });
            state.players[player].graveyard.retain(|&id| id != obj_id);

            state.emit_event(GameEvent::SpellCast {
                object: obj_id,
                controller: player,
            });
            state.emit_event(GameEvent::ZoneChange {
                object: obj_id,
                from: Zone::Graveyard,
                to: Zone::Stack,
            });

            triggers::fire_spell_cast_triggers(state, player, is_creature);
            state.consecutive_passes = 0;
        }

        Action::ChooseReplacementOrder { ordering } => {
            let _ = ordering;
            state.consecutive_passes = 0;
        }

        Action::MulliganKeep => {
            let player = state.priority_player;
            state.players[player].mulligan_decided = true;
            setup::advance_mulligan(state);
        }

        Action::MulliganMulligan => {
            let player = state.priority_player;
            debug_assert!(
                state.players[player].mulligan_count < crate::action::MAX_MULLIGANS,
                "MulliganMulligan applied but player {} already at max mulligans ({})",
                player,
                state.players[player].mulligan_count,
            );
            // Shuffle hand back into library
            let hand: Vec<crate::card::ObjectId> = state.players[player].hand.drain(..).collect();
            for obj_id in hand {
                state.players[player].library.push(obj_id);
            }
            {
                use rand::seq::SliceRandom;
                let mut rng = rand::thread_rng();
                state.players[player].library.shuffle(&mut rng);
            }
            // Draw 7 new cards
            draw_cards(state, player, 7);
            state.players[player].mulligan_count += 1;
            // Stay on the same player for another keep/mulligan decision
        }

        Action::MulliganBottomCard { object_id } => {
            let player = state.priority_player;
            // Move the card from hand to bottom of library
            if let Some(pos) = state.players[player].hand.iter().position(|&id| id == *object_id) {
                state.players[player].hand.remove(pos);
                state.players[player].library.push(*object_id);
            }
            setup::advance_mulligan(state);
        }

        Action::ChooseTutorTarget { card_id } => {
            if let Some(pending) = state.pending_tutor.take() {
                let player = pending.controller;
                let destination = pending.destination;
                // Find the first instance of this card in the library
                if let Some(pos) = state.players[player]
                    .library
                    .iter()
                    .position(|&obj_id| state.objects[&obj_id].card_def_id == *card_id)
                {
                    let obj_id = state.players[player].library.remove(pos);
                    state.move_object(obj_id, ZoneType::Library, destination);
                }
            }
        }

        Action::Concede => {
            let player = state.priority_player;
            state.players[player].has_lost = true;
            // Check if game ends: only 1 active player left = game over
            let active_count = state.active_player_count();
            if active_count <= 1 {
                state.game_over = true;
                state.winner = (0..state.players.len()).find(|&i| !state.players[i].has_lost);
            }
        }

        Action::ActivateMacro { combo_id } => {
            let player = state.priority_player;
            // Look up the combo from the registry and apply its effect.
            let combo = state
                .combo_registry
                .as_ref()
                .and_then(|reg| reg.get(*combo_id).cloned());
            if let Some(combo) = combo {
                crate::combo::apply_combo_effect(state, player, &combo);
                state.consecutive_passes = 0;
                // Immediately check if the combo ended the game (e.g., infinite
                // damage killed the opponent). Without this, the game would keep
                // offering actions until the next scheduled SBA check.
                sba::check_state_based_actions(state);
            }
        }

        Action::EndTurn => {
            fast_forward_end_of_turn(state);
        }

    }
}

/// Check if a player controls a permanent with a given static ability.
fn has_static_ability_on_battlefield(
    state: &GameState,
    player: PlayerIndex,
    target_ability: &crate::layers::StaticAbility,
) -> bool {
    let db = state.card_db();
    state.battlefield.iter().any(|&obj_id| {
        state.objects.get(&obj_id).map_or(false, |inst| {
            if inst.controller != player {
                return false;
            }
            db.get(inst.card_def_id).map_or(false, |def| {
                def.static_abilities.iter().any(|sa| {
                    std::mem::discriminant(sa) == std::mem::discriminant(target_ability)
                })
            })
        })
    })
}

/// Draw cards for a player, applying draw replacement effects.
///
/// Replacement effect priority (only one applies per would-draw):
/// 1. Renfield/Eruth: exile top 2 instead of drawing (simplified as put 2 in hand)
/// 2. Abundance: reveal until land, put in hand, rest on bottom
/// 3. Phial of Galadriel: draw 2 instead of 1 when hand is empty
/// If none apply, normal draw.
pub fn draw_cards(state: &mut GameState, player: PlayerIndex, count: usize) {
    // Check for draw replacement effects on the battlefield
    let has_renfield = has_static_ability_on_battlefield(
        state, player, &crate::layers::StaticAbility::RenfieldDrawReplacement,
    );
    let has_abundance = has_static_ability_on_battlefield(
        state, player, &crate::layers::StaticAbility::AbundanceReplacement,
    );
    let has_phial = has_static_ability_on_battlefield(
        state, player, &crate::layers::StaticAbility::PhialDrawDoubler,
    );

    for _ in 0..count {
        if state.players[player].library.is_empty() {
            // Player loses for drawing from empty library
            state.players[player].has_lost = true;
            return;
        }

        let hand_was_empty = state.players[player].hand.is_empty();

        // Determine how many actual cards to put in hand for this single draw.
        // Phial doubles the draw (draw 2 instead of 1) as a true replacement
        // when hand was empty. It does NOT stack with other replacements --
        // only one replacement effect applies per would-draw event.
        if has_renfield {
            // Renfield replacement: exile top 2 to hand instead of drawing 1.
            // Simplified: put 2 cards in hand (in practice they'd be exiled and
            // playable this turn). This is a replacement, so no CardDrawn event.
            for _ in 0..2 {
                if state.players[player].library.is_empty() {
                    break;
                }
                let card_id = state.players[player].library.remove(0);
                state.players[player].hand.push(card_id);
                state.emit_event(GameEvent::ZoneChange {
                    object: card_id,
                    from: Zone::Library,
                    to: Zone::Hand,
                });
            }
        } else if has_abundance {
            // Abundance replacement: reveal cards from the top until you find a land,
            // put it in hand, then put the revealed non-land cards on the bottom
            // in any order. This is a draw replacement, so no CardDrawn event.
            let land_idx = {
                let db = state.card_db();
                state.players[player].library.iter().position(|&id| {
                    state.objects.get(&id)
                        .and_then(|inst| db.get(inst.card_def_id))
                        .map_or(false, |def| def.is_land())
                })
            };
            if let Some(idx) = land_idx {
                // Remove revealed non-land cards (indices 0..idx) and put on bottom
                let revealed: Vec<ObjectId> = state.players[player].library.drain(0..idx).collect();
                // Now the land is at index 0; remove it and put in hand
                let card_id = state.players[player].library.remove(0);
                state.players[player].hand.push(card_id);
                // Put revealed non-lands on the bottom of the library
                state.players[player].library.extend(revealed);
                state.emit_event(GameEvent::ZoneChange {
                    object: card_id,
                    from: Zone::Library,
                    to: Zone::Hand,
                });
            } else {
                // No lands left: reveal entire library, put all on bottom (no card drawn).
                // Abundance still replaces the draw even if nothing is found.
            }
        } else {
            // Normal draw, possibly doubled by Phial
            let draws = if has_phial && hand_was_empty { 2 } else { 1 };
            for _ in 0..draws {
                if state.players[player].library.is_empty() {
                    break;
                }
                let card_id = state.players[player].library.remove(0);
                state.players[player].hand.push(card_id);
                state.emit_event(GameEvent::CardDrawn {
                    player,
                    object: card_id,
                });
                state.emit_event(GameEvent::ZoneChange {
                    object: card_id,
                    from: Zone::Library,
                    to: Zone::Hand,
                });
            }
        }

        // Fire OpponentDrawsCard triggers (e.g. Consecrated Sphinx)
        triggers::fire_card_draw_triggers(state, player);
    }
}

/// Discard random cards from a player's hand.
fn discard_random(state: &mut GameState, player: PlayerIndex, count: usize) {
    let mut rng = rand::thread_rng();
    for _ in 0..count {
        if state.players[player].hand.is_empty() {
            break;
        }
        let idx = rng.gen_range(0..state.players[player].hand.len());
        let obj_id = state.players[player].hand.remove(idx);
        state.players[player].graveyard.push(obj_id);
        // Fire discard triggers (e.g., Monument to Endurance)
        triggers::check_triggers(state, TriggerCondition::YouDiscardACard, None);
        let _ = triggers::flush_triggers(state);
    }
}

/// Fast-forward the active player's turn from the current phase to completion.
///
/// Called when a player chooses `Action::EndTurn`. Advances through all
/// remaining phases without calling `legal_actions()`, executing phase
/// entries (triggers, combat damage, SBA) along the way. The stack is
/// auto-resolved and cleanup discard is handled randomly.
///
/// This collapses O(remaining_phases) priority passes into a single action,
/// cutting search depth when the optimal play is "do nothing more this turn."
fn fast_forward_end_of_turn(state: &mut GameState) {
    let turn_player = state.active_player;
    let initial_turn = state.turn_number;
    let mut safety = 0u32;
    const MAX_SAFETY: u32 = 200;

    while state.active_player == turn_player
        && state.turn_number == initial_turn
        && !state.game_over
        && safety < MAX_SAFETY
    {
        safety += 1;

        // Auto-order pending triggers (push in existing order)
        if !state.pending_triggers.is_empty() {
            let pending: Vec<_> = state.pending_triggers.drain(..).collect();
            for trigger in pending {
                triggers::push_trigger_to_stack(state, &trigger);
            }
        }

        // Auto fail-to-find any pending tutor
        if state.pending_tutor.is_some() {
            state.pending_tutor = None;
            continue;
        }

        // Resolve stack items
        if !state.stack.is_empty() {
            resolution::resolve_top_of_stack(state);
            sba::check_state_based_actions(state);
            continue;
        }

        // Handle phases with mandatory actions
        match state.phase {
            Phase::DeclareAttackers => {
                // Skip combat — declare no attackers
                state.combat.clear();
                state.consecutive_passes = 0;
                phases::advance_phase(state);
            }
            Phase::DeclareBlockers => {
                state.consecutive_passes = 0;
                phases::advance_phase(state);
            }
            Phase::Cleanup => {
                let hand_size = state.players[turn_player].hand.len();
                if hand_size > 7 {
                    let to_discard = hand_size - 7;
                    discard_random(state, turn_player, to_discard);
                }
                phases::finalize_cleanup(state);
            }
            _ => {
                // Normal phase — pass priority to advance
                state.consecutive_passes += 1;
                phases::handle_priority_pass(state);
            }
        }
    }
}

/// Fast-forward through the goldfish player's entire turn.
///
/// In goldfish mode, the opponent (player 1) never casts spells, attacks,
/// or blocks. This function advances through all phases of their turn
/// without calling `legal_actions()` — a significant performance win since
/// `legal_actions()` is the most expensive function in the game loop.
///
/// Phase entries (untap, draw, upkeep/end-step triggers) are still executed
/// so the pilot's cards that trigger during the opponent's turn work correctly.
/// If triggers put items on the stack, they are auto-resolved (both players
/// pass priority). If the goldfish needs to discard in cleanup, random cards
/// are discarded.
///
/// Returns the number of internal actions taken (for action-count tracking).
pub fn fast_forward_goldfish_turn(state: &mut GameState) -> u32 {
    let goldfish_player = state.active_player;
    let mut actions = 0u32;
    let mut safety = 0u32;
    const MAX_SAFETY: u32 = 200;

    while state.active_player == goldfish_player && !state.game_over && safety < MAX_SAFETY {
        safety += 1;

        // Handle pending triggers that need ordering — auto-order them
        if !state.pending_triggers.is_empty() {
            // For each player's pending triggers, auto-push in existing order
            let triggers: Vec<_> = state.pending_triggers.drain(..).collect();
            for trigger in triggers {
                triggers::push_trigger_to_stack(state, &trigger);
            }
        }

        // Handle pending tutor — auto fail-to-find (goldfish doesn't search)
        if state.pending_tutor.is_some() {
            state.pending_tutor = None;
            actions += 1;
            continue;
        }

        // Resolve stack items (both players auto-pass)
        if !state.stack.is_empty() {
            resolution::resolve_top_of_stack(state);
            sba::check_state_based_actions(state);
            actions += 1;
            continue;
        }

        // Handle phases that need mandatory actions
        match state.phase {
            Phase::DeclareAttackers if state.priority_player == goldfish_player => {
                // Goldfish never attacks — declare empty attackers and advance
                state.combat.clear();
                state.consecutive_passes = 0;
                phases::advance_phase(state);
                actions += 1;
            }
            Phase::DeclareBlockers if state.priority_player != goldfish_player => {
                // Goldfish as defender never blocks — this shouldn't happen in
                // goldfish mode (pilot is attacking), but handle it gracefully
                state.consecutive_passes = 0;
                phases::advance_phase(state);
                actions += 1;
            }
            Phase::Cleanup => {
                // If goldfish needs to discard, do it randomly
                let hand_size = state.players[goldfish_player].hand.len();
                if hand_size > 7 {
                    let to_discard = hand_size - 7;
                    discard_random(state, goldfish_player, to_discard);
                }
                phases::finalize_cleanup(state);
                actions += 1;
            }
            _ => {
                // Normal phase — just pass priority to advance
                state.consecutive_passes += 1;
                phases::handle_priority_pass(state);
                actions += 1;
            }
        }
    }

    actions
}
