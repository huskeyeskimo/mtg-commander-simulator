use crate::card::{KeywordAbility, ObjectId};
use crate::events::GameEvent;
use crate::game::{GameState, PlayerIndex, Target};

/// Check if any creature in combat has first strike or double strike.
pub(super) fn has_first_strike_creatures(state: &GameState) -> bool {
    let check = |id: &ObjectId| -> bool {
        state.has_keyword(*id, KeywordAbility::FirstStrike)
            || state.has_keyword(*id, KeywordAbility::DoubleStrike)
    };

    state.combat.attackers.iter().any(check)
        || state.combat.blockers.keys().any(check)
}

/// A pending damage application collected during combat resolution.
struct DamageEvent {
    /// The attacker/source dealing the damage.
    source_id: ObjectId,
    target_object: Option<ObjectId>,
    target_player: Option<PlayerIndex>,
    amount: u32,
    lifelink_for: Option<PlayerIndex>,
    /// Whether this damage should be applied as -1/-1 counters (Wither/Infect).
    wither: bool,
    /// Whether this damage to a player should be applied as poison counters (Infect).
    infect: bool,
}

/// Resolve combat damage. Collects all damage events first (read phase), then applies them.
pub(super) fn resolve_combat_damage(state: &mut GameState, first_strike_only: bool) {
    if state.pending_copy_order.is_some() || state.movement_or_hand_pending() { return; }
    let defending_player = state.next_player(state.active_player);
    let mut damage_events: Vec<DamageEvent> = Vec::new();

    // Apply flanking before damage (when blockers are declared, creatures blocking
    // a creature with flanking that don't have flanking get -1/-1 until EOT).
    // We apply this in the first damage step only.
    if first_strike_only || !has_first_strike_creatures(state) {
        apply_flanking(state);
    }

    // Phase 1: Collect all damage events (immutable — uses layer engine)
    {
        let attackers = state.combat.attackers.clone();

        for &attacker_id in &attackers {
            if state.objects.get(&attacker_id).is_none() {
                continue;
            }

            let has_fs = state.has_keyword(attacker_id, KeywordAbility::FirstStrike);
            let has_ds = state.has_keyword(attacker_id, KeywordAbility::DoubleStrike);

            let deals_damage = if first_strike_only {
                has_fs || has_ds
            } else {
                !has_fs || has_ds
            };

            if !deals_damage {
                continue;
            }

            let power = state.effective_power(attacker_id).max(0) as u32;
            if power == 0 {
                continue;
            }

            let lifelink = if state.has_keyword(attacker_id, KeywordAbility::Lifelink) {
                Some(state.objects[&attacker_id].controller)
            } else {
                None
            };

            let has_wither = state.has_keyword(attacker_id, KeywordAbility::Wither);
            let has_infect = state.has_keyword(attacker_id, KeywordAbility::Infect);

            let blockers = state
                .combat
                .attacker_blockers
                .get(&attacker_id)
                .cloned()
                .unwrap_or_default();

            if blockers.is_empty() {
                // Unblocked — damage to defending player
                damage_events.push(DamageEvent {
                    source_id: attacker_id,
                    target_object: None,
                    target_player: Some(defending_player),
                    amount: power,
                    lifelink_for: lifelink,
                    wither: false, // wither only affects creatures
                    infect: has_infect,
                });
            } else {
                let has_trample = state.has_keyword(attacker_id, KeywordAbility::Trample);
                let has_deathtouch = state.has_keyword(attacker_id, KeywordAbility::Deathtouch);

                let mut remaining = power;

                for &blocker_id in &blockers {
                    if remaining == 0 {
                        break;
                    }
                    let blocker_inst = match state.objects.get(&blocker_id) {
                        Some(i) => i,
                        None => continue,
                    };

                    let eff_toughness = state.effective_toughness(blocker_id);
                    let remaining_tough =
                        (eff_toughness - blocker_inst.damage_marked as i32).max(0) as u32;
                    let damage = if has_deathtouch {
                        1.min(remaining)
                    } else {
                        remaining_tough.min(remaining)
                    };

                    damage_events.push(DamageEvent {
                        source_id: attacker_id,
                        target_object: Some(blocker_id),
                        target_player: None,
                        amount: damage,
                        lifelink_for: lifelink,
                        wither: has_wither || has_infect,
                        infect: false,
                    });
                    remaining -= damage;
                }

                // Trample: excess to defending player
                if has_trample && remaining > 0 {
                    damage_events.push(DamageEvent {
                        source_id: attacker_id,
                        target_object: None,
                        target_player: Some(defending_player),
                        amount: remaining,
                        lifelink_for: lifelink,
                        wither: false,
                        infect: has_infect,
                    });
                }

                // Blockers deal damage to attacker
                for &blocker_id in &blockers {
                    if state.objects.get(&blocker_id).is_none() {
                        continue;
                    }

                    let blocker_has_fs =
                        state.has_keyword(blocker_id, KeywordAbility::FirstStrike);
                    let blocker_has_ds =
                        state.has_keyword(blocker_id, KeywordAbility::DoubleStrike);

                    let blocker_deals = if first_strike_only {
                        blocker_has_fs || blocker_has_ds
                    } else {
                        !blocker_has_fs || blocker_has_ds
                    };

                    if blocker_deals {
                        let blocker_power =
                            state.effective_power(blocker_id).max(0) as u32;
                        let blocker_lifelink =
                            if state.has_keyword(blocker_id, KeywordAbility::Lifelink) {
                                Some(state.objects[&blocker_id].controller)
                            } else {
                                None
                            };
                        let blocker_wither = state.has_keyword(blocker_id, KeywordAbility::Wither)
                            || state.has_keyword(blocker_id, KeywordAbility::Infect);

                        damage_events.push(DamageEvent {
                            source_id: blocker_id,
                            target_object: Some(attacker_id),
                            target_player: None,
                            amount: blocker_power,
                            lifelink_for: blocker_lifelink,
                            wither: blocker_wither,
                            infect: false,
                        });
                    }
                }
            }
        }
    }

    // The represented automatic subset needs no replacement decision frame.
    // Reject other damage replacements rather than silently ignoring them.
    if damage_events.iter().any(|event| event.amount > 0) && state.replacement_effects.iter().any(|effect|
        effect.applies_to == crate::replacement::ReplacementEventKind::DamageDealt
            && (!effect.is_self_replacement || !matches!(effect.action,
                crate::replacement::ReplacementAction::Prevent | crate::replacement::ReplacementAction::ModifyAmount { .. }))) {
        state.sba_failure = Some(super::sba::PreparedPassFailure::Transition(super::transitions::TransitionError::UnsupportedPath));
        return;
    }
    // Resolve the represented automatic replacements before changing results.
    for event in &mut damage_events {
        let target = event.target_object.map(Target::Object).or_else(|| event.target_player.map(Target::Player));
        if let Some(target) = target { event.amount = state.deal_damage_with_replacement(event.amount, &target); }
    }
    // Capacity errors discard this damage batch before any life/counter/ledger
    // mutation. This does not introduce replacement choices or continuations.
    if damage_capacity(state, &damage_events).is_none() {
        state.sba_failure = Some(super::sba::PreparedPassFailure::StateEncoding); return;
    }
    for event in damage_events {
        if event.amount == 0 { continue; }
        if let Some(player) = event.target_player {
            if let Some(record) = state.commanders.iter_mut().find(|c| c.object == event.source_id) {
                let Some(total) = record.damage_to_player[player].checked_add(event.amount) else {
                    state.sba_failure = Some(super::sba::PreparedPassFailure::StateEncoding); return;
                };
                record.damage_to_player[player] = total;
            }
        }
        if let Some(obj_id) = event.target_object {
            if event.wither {
                // Wither/Infect: damage to creatures as -1/-1 counters
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    inst.minus_counters += event.amount as i32;
                }
            } else {
                // Normal damage
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    inst.damage_marked += event.amount;
                }
            }
            state.emit_event(GameEvent::DamageDealt {
                source: event.source_id,
                target: Target::Object(obj_id),
                amount: event.amount,
                is_combat: true,
            });
        }
        if let Some(player) = event.target_player {
            if event.infect {
                // Infect: damage to players as poison counters
                state.players[player].poison_counters += event.amount;
            } else {
                let old_life = state.players[player].life;
                state.players[player].life -= event.amount as i32;

                state.emit_event(GameEvent::LifeChanged {
                    player,
                    old: old_life,
                    new: state.players[player].life,
                });
            }

            // Toxic: when this creature deals combat damage to a player, add poison counters
            // (regardless of infect — toxic and infect are separate mechanics)
            if !event.infect {
                // Check toxic on the source creature
                // Toxic N gives N poison counters (simplified: we treat Toxic as giving 1 poison)
                if state.has_keyword(event.source_id, KeywordAbility::Toxic) {
                    state.players[player].poison_counters += 1;
                }
            }

            state.emit_event(GameEvent::DamageDealt {
                source: event.source_id,
                target: Target::Player(player),
                amount: event.amount,
                is_combat: true,
            });
        }
        if let Some(lifelink_player) = event.lifelink_for {
            let old_life = state.players[lifelink_player].life;
            state.players[lifelink_player].life += event.amount as i32;
            state.emit_event(GameEvent::LifeChanged {
                player: lifelink_player,
                old: old_life,
                new: state.players[lifelink_player].life,
            });
        }
    }
}

/// Check the exact sequence of represented damage results without mutating it.
fn damage_capacity(state: &GameState, events: &[DamageEvent]) -> Option<()> {
    use std::collections::HashMap;
    let mut life: Vec<_> = state.players.iter().map(|p| p.life).collect();
    let mut poison: Vec<_> = state.players.iter().map(|p| p.poison_counters).collect();
    let mut marked = HashMap::new();
    let mut counters = HashMap::new();
    let mut commander_damage: Vec<_> = state.commanders.iter().map(|c| c.damage_to_player.clone()).collect();
    for event in events {
        if event.amount == 0 { continue; }
        let signed = i32::try_from(event.amount).ok()?;
        if let Some(id) = event.target_object {
            let inst = state.objects.get(&id)?;
            if event.wither {
                let value = counters.entry(id).or_insert(inst.minus_counters);
                *value = value.checked_add(signed)?;
            } else {
                let value = marked.entry(id).or_insert(inst.damage_marked);
                *value = value.checked_add(event.amount)?;
            }
        }
        if let Some(player) = event.target_player {
            if let Some(index) = state.commanders.iter().position(|c| c.object == event.source_id) {
                let value = commander_damage.get_mut(index)?.get_mut(player)?;
                *value = value.checked_add(event.amount)?;
            }
            if event.infect { *poison.get_mut(player)? = poison[player].checked_add(event.amount)?; }
            else { *life.get_mut(player)? = life[player].checked_sub(signed)?; }
            if !event.infect && state.has_keyword(event.source_id, KeywordAbility::Toxic) {
                *poison.get_mut(player)? = poison[player].checked_add(1)?;
            }
        }
        if let Some(player) = event.lifelink_for { *life.get_mut(player)? = life[player].checked_add(signed)?; }
    }
    Some(())
}

/// Apply flanking: when a creature with flanking becomes blocked by a creature
/// without flanking, the blocking creature gets -1/-1 until end of turn.
fn apply_flanking(state: &mut GameState) {
    use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};

    let mut debuffs: Vec<(ObjectId, PlayerIndex)> = Vec::new();

    for &attacker_id in &state.combat.attackers {
        if !state.has_keyword(attacker_id, KeywordAbility::Flanking) {
            continue;
        }
        let controller = match state.objects.get(&attacker_id) {
            Some(inst) => inst.controller,
            None => continue,
        };
        if let Some(blocker_ids) = state.combat.attacker_blockers.get(&attacker_id) {
            for &blocker_id in blocker_ids {
                if !state.has_keyword(blocker_id, KeywordAbility::Flanking) {
                    debuffs.push((blocker_id, controller));
                }
            }
        }
    }

    for (blocker_id, controller) in debuffs {
        let ts = state.new_timestamp();
        state.continuous_effects.push(ContinuousEffect {
            source_id: blocker_id,
            controller,
            timestamp: ts,
            duration: Duration::UntilEndOfTurn,
            affected: AffectedObjects::Specific(blocker_id),
            modification: LayerModification::ModifyPT(-1, -1),
        });
    }
    state.invalidate_characteristics_cache();
}
