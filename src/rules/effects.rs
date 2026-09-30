use crate::card::{Effect, KeywordAbility, ObjectId, TriggerCondition, ZoneType};
use crate::events::GameEvent;
use crate::game::{GameState, PlayerIndex, StackSource, Target};

/// Resolve an effect.
/// `source_id` is the ObjectId of the permanent that generated this effect
/// (e.g., the creature whose triggered ability fired). Used by effects like
/// `BuffOtherSubtype` that need to exclude "self" from the buff.
pub(super) fn resolve_effect(
    state: &mut GameState,
    effect: &Effect,
    controller: PlayerIndex,
    targets: &[Target],
    source_id: Option<ObjectId>,
) {
    // Spell targets and untargeted recipients are different. Interpret each
    // leaf's declaration, including leaves nested inside Multiple, before its
    // handler runs. NoTarget retains the handler's existing untargeted behavior.
    use crate::targeting::EffectRecipients;
    let recipients = match crate::targeting::effect_recipients(effect) {
        EffectRecipients::Declared(crate::card::TargetSpec::Controller) => Some(vec![Target::Player(controller)]),
        EffectRecipients::Declared(crate::card::TargetSpec::EachCreature) => Some(state.battlefield.iter().copied()
            .filter(|&id| state.is_creature(id)).map(Target::Object).collect()),
        EffectRecipients::Declared(crate::card::TargetSpec::NoTarget)
        | EffectRecipients::Independent => Some(Vec::new()),
        EffectRecipients::Declared(_)
        | EffectRecipients::SelectedObjects
        | EffectRecipients::Children => None,
    };
    let targets = recipients.as_deref().unwrap_or(targets);

    match effect {
        Effect::DealDamage { amount, target: target_spec } => {
            // For untargeted effects, auto-generate targets from the spec.
            let effective_targets: Vec<Target> = if targets.is_empty() {
                match target_spec {
                    crate::card::TargetSpec::NoTarget => {
                        // "Each player" — deal damage to all players
                        (0..state.players.len())
                            .map(|i| Target::Player(i))
                            .collect()
                    }
                    crate::card::TargetSpec::EachCreature => {
                        // "Each creature" — deal damage to all creatures on the battlefield
                        state.battlefield.iter().copied()
                            .filter(|&id| state.is_creature(id))
                            .map(Target::Object)
                            .collect()
                    }
                    crate::card::TargetSpec::Opponent => {
                        // "Target opponent" — deal damage to the opponent
                        let opp = state.opponent(controller);
                        vec![Target::Player(opp)]
                    }
                    crate::card::TargetSpec::CreatureOrPlayer
                    | crate::card::TargetSpec::AnyPlayer => {
                        // Default to targeting opponent when no explicit target
                        let opp = state.opponent(controller);
                        vec![Target::Player(opp)]
                    }
                    _ => vec![],
                }
            } else {
                targets.to_vec()
            };

            for target in &effective_targets {
                // Apply replacement effects to damage (CR 614)
                let actual_damage = state.deal_damage_with_replacement(*amount, target);
                if actual_damage == 0 {
                    continue;
                }

                match target {
                    Target::Player(p) => {
                        let old_life = state.players[*p].life;
                        state.players[*p].life -= actual_damage as i32;
                        state.emit_event(GameEvent::LifeChanged {
                            player: *p,
                            old: old_life,
                            new: state.players[*p].life,
                        });
                        state.emit_event(GameEvent::DamageDealt {
                            source: 0,
                            target: target.clone(),
                            amount: actual_damage,
                            is_combat: false,
                        });
                    }
                    Target::Object(id) => {
                        if let Some(inst) = state.objects.get_mut(id) {
                            inst.damage_marked += actual_damage;
                        }
                        state.emit_event(GameEvent::DamageDealt {
                            source: 0,
                            target: target.clone(),
                            amount: actual_damage,
                            is_combat: false,
                        });
                    }
                }
            }
        }

        Effect::GainLife { amount } => {
            let old_life = state.players[controller].life;
            state.players[controller].life += *amount as i32;
            state.emit_event(GameEvent::LifeChanged {
                player: controller,
                old: old_life,
                new: state.players[controller].life,
            });
        }

        Effect::LoseLife { amount, .. } => {
            for target in targets {
                if let Target::Player(p) = target {
                    let old_life = state.players[*p].life;
                    state.players[*p].life -= *amount as i32;
                    state.emit_event(GameEvent::LifeChanged {
                        player: *p,
                        old: old_life,
                        new: state.players[*p].life,
                    });
                }
            }
        }

        Effect::DrawCards { count } => {
            super::draw_cards(state, controller, *count as usize);
        }

        Effect::DestroyTarget { .. } => {
            // Collect which objects are destroyable (read phase)
            let destroyable: Vec<ObjectId> = targets
                .iter()
                .filter_map(|target| {
                    if let Target::Object(id) = target {
                        let indestructible =
                            state.has_keyword(*id, KeywordAbility::Indestructible);
                        if !indestructible { Some(*id) } else { None }
                    } else {
                        None
                    }
                })
                .collect();

            for &id in &destroyable {
                state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
            }
            if !destroyable.is_empty() {
                state.refresh_continuous_effects();
            }
            // Fire dies triggers for destroyed creatures.
            for &id in &destroyable {
                super::triggers::check_triggers(state, TriggerCondition::Dies, Some(id));
            }
            if !destroyable.is_empty() {
                let _ = super::triggers::flush_triggers(state);
            }
        }

        Effect::BounceTo { zone, .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    state.move_object(*id, ZoneType::Battlefield, *zone);
                }
            }
        }

        Effect::Buff {
            power,
            toughness,
            until_eot,
        } => {
            use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
            for target in targets {
                if let Target::Object(id) = target {
                    if *until_eot {
                        // Create a continuous effect that lasts until end of turn
                        let ts = state.new_timestamp();
                        state.continuous_effects.push(ContinuousEffect {
                            source_id: *id,
                            controller,
                            timestamp: ts,
                            duration: Duration::UntilEndOfTurn,
                            affected: AffectedObjects::Specific(*id),
                            modification: LayerModification::ModifyPT(*power, *toughness),
                        });
                    } else {
                        // Permanent buff via +1/+1 counters
                        if let Some(inst) = state.objects.get_mut(id) {
                            let counters = (*power).min(*toughness);
                            inst.plus_counters += counters;
                            // Any asymmetric remainder as a permanent continuous effect
                            if *power != *toughness {
                                let ts = state.new_timestamp();
                                state.continuous_effects.push(ContinuousEffect {
                                    source_id: *id,
                                    controller,
                                    timestamp: ts,
                                    duration: Duration::Permanent,
                                    affected: AffectedObjects::Specific(*id),
                                    modification: LayerModification::ModifyPT(
                                        power - counters,
                                        toughness - counters,
                                    ),
                                });
                            }
                        }
                    }
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::DiscardCards { count, .. } => {
            for target in targets {
                if let Target::Player(p) = target {
                    super::discard_random(state, *p, *count as usize);
                }
            }
        }

        Effect::CreateToken(token_def) => {
            super::tokens::create_token(state, token_def, controller);
        }

        Effect::CreateTokens { token, count } => {
            let n = state.evaluate_dynamic_value(count, controller, None);
            for _ in 0..n.max(0) {
                super::tokens::create_token(state, token, controller);
            }
        }

        Effect::Counter { .. } => {
            // Find the targeted spell on the stack and counter it
            let target_obj_id = targets.iter().find_map(|t| {
                if let Target::Object(id) = t { Some(*id) } else { None }
            });

            if let Some(target_id) = target_obj_id {
                // Find and remove the targeted spell from the stack
                if let Some(idx) = state.stack.iter().position(|entry| {
                    matches!(&entry.source, StackSource::Spell(id) if *id == target_id)
                }) {
                    let countered = state.stack.remove(idx);
                    if let StackSource::Spell(obj_id) = countered.source {
                        state.move_object(obj_id, ZoneType::Stack, ZoneType::Graveyard);
                    }
                }
            } else {
                // Fallback: counter top spell on stack if no target specified
                if let Some(countered) = state.stack.pop() {
                    if let StackSource::Spell(obj_id) = countered.source {
                        state.move_object(obj_id, ZoneType::Stack, ZoneType::Graveyard);
                    }
                }
            }
        }

        Effect::ExileTarget { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if state.battlefield.contains(id) {
                        state.move_object(*id, ZoneType::Battlefield, ZoneType::Exile);
                    }
                }
            }
            state.refresh_continuous_effects();
            state.refresh_replacement_effects();
        }

        Effect::DestroyAll => {
            // Destroy all creatures on the battlefield (e.g., Wrath of God)
            let creatures: Vec<ObjectId> = state
                .battlefield
                .iter()
                .copied()
                .filter(|&id| state.is_creature(id))
                .filter(|&id| !state.has_keyword(id, KeywordAbility::Indestructible))
                .collect();

            for &id in &creatures {
                let dest_zone = state.death_replacement_zone(id);
                if dest_zone != ZoneType::Battlefield {
                    state.move_object(id, ZoneType::Battlefield, dest_zone);
                }
            }
            if !creatures.is_empty() {
                state.refresh_continuous_effects();
                state.refresh_replacement_effects();
            }
            // Fire dies triggers
            for &id in &creatures {
                super::triggers::check_triggers(state, TriggerCondition::Dies, Some(id));
            }
            if !creatures.is_empty() {
                let _ = super::triggers::flush_triggers(state);
            }
        }

        Effect::Debuff {
            power,
            toughness,
            until_eot,
        } => {
            use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
            for target in targets {
                if let Target::Object(id) = target {
                    let duration = if *until_eot {
                        Duration::UntilEndOfTurn
                    } else {
                        Duration::Permanent
                    };
                    let ts = state.new_timestamp();
                    state.continuous_effects.push(ContinuousEffect {
                        source_id: *id,
                        controller,
                        timestamp: ts,
                        duration,
                        affected: AffectedObjects::Specific(*id),
                        modification: LayerModification::ModifyPT(-power, -toughness),
                    });
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::PutCounters { count, .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if let Some(inst) = state.objects.get_mut(id) {
                        if *count > 0 {
                            inst.plus_counters += count;
                        } else {
                            inst.minus_counters += count.abs();
                        }
                    }
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::MillCards { count, .. } => {
            for target in targets {
                if let Target::Player(p) = target {
                    for _ in 0..*count {
                        if !state.players[*p].library.is_empty() {
                            let card_id = state.players[*p].library.remove(0);
                            state.move_object(card_id, ZoneType::Library, ZoneType::Graveyard);
                        }
                    }
                }
            }
        }

        Effect::SacrificeCreatures { count, .. } => {
            for target in targets {
                if let Target::Player(p) = target {
                    let mut creatures = state.creatures_controlled_by(*p);
                    // Sort by effective power ascending so the weakest are
                    // sacrificed first — a reasonable heuristic standing in
                    // for actual player choice until we surface a UI action.
                    creatures.sort_by_key(|&id| state.effective_power(id));
                    for &id in creatures.iter().take(*count as usize) {
                        state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
                    }
                }
            }
            state.refresh_continuous_effects();
        }

        Effect::PreventCombatDamage => {
            // Not yet implemented — no cards in the current pool use this effect.
        }

        Effect::AddMana { color, amount } => {
            for _ in 0..*amount {
                match color {
                    Some(c) => state.players[controller].mana_pool.add_color(*c, 1),
                    None => state.players[controller].mana_pool.colorless += 1,
                }
            }
        }

        Effect::AddDynamicMana { color, count } => {
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let n = state.evaluate_dynamic_value(count, controller, Some(&ctx));
            for _ in 0..n.max(0) {
                state.players[controller].mana_pool.add_color(*color, 1);
            }
        }

        Effect::LoseDynamicLife { amount, .. } => {
            let db_ref = state.card_db.clone();
            let db = db_ref.as_ref().expect("card_db required");
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let n = amount.evaluate(
                controller,
                &state.objects,
                &state.battlefield,
                &|id| db.get(id),
                Some(&ctx),
            );
            if n > 0 {
                for target in targets {
                    if let Target::Player(p) = target {
                        let old_life = state.players[*p].life;
                        state.players[*p].life -= n;
                        state.emit_event(GameEvent::LifeChanged {
                            player: *p,
                            old: old_life,
                            new: state.players[*p].life,
                        });
                    }
                }
                // If no explicit targets, apply to controller
                if targets.is_empty() {
                    let old_life = state.players[controller].life;
                    state.players[controller].life -= n;
                    state.emit_event(GameEvent::LifeChanged {
                        player: controller,
                        old: old_life,
                        new: state.players[controller].life,
                    });
                }
            }
        }

        Effect::ExtraTurn => {
            state.extra_turns.push_back(controller);
        }

        Effect::SkipPhase(phase) => {
            state.skip_phases.insert(*phase);
        }

        Effect::Multiple(effects) => {
            for e in effects {
                resolve_effect(state, e, controller, targets, source_id);
            }
        }

        Effect::SearchLibrary { destination, subtype_filter } => {
            if !subtype_filter.is_empty() {
                // Fetch-land style: filter by subtype. Always present a choice
                // (or fail to find if no matching cards in library).
                state.pending_tutor = Some(crate::game::PendingTutor {
                    controller,
                    destination: *destination,
                    subtype_filter: subtype_filter.clone(),
                });
            } else if state.players[controller].tutor_targets.is_empty() {
                // Legacy behavior: no tutor targets configured, take top card.
                if !state.players[controller].library.is_empty() {
                    let card_obj = state.players[controller].library.remove(0);
                    state.move_object(card_obj, ZoneType::Library, *destination);
                }
            } else {
                // Set pending tutor — the player will choose via ChooseTutorTarget.
                state.pending_tutor = Some(crate::game::PendingTutor {
                    controller,
                    destination: *destination,
                    subtype_filter: vec![],
                });
            }
        }

        Effect::BounceAllNonlandOpponents => {
            // Bounce all nonland permanents opponents control to their owners' hands.
            let db = state.card_db();
            let to_bounce: Vec<ObjectId> = state
                .battlefield
                .iter()
                .copied()
                .filter(|&id| {
                    if let Some(inst) = state.objects.get(&id) {
                        if inst.controller == controller {
                            return false; // skip own permanents
                        }
                        if let Some(def) = db.get(inst.card_def_id) {
                            return !def.is_land();
                        }
                    }
                    false
                })
                .collect();
            for id in to_bounce {
                state.move_object(id, ZoneType::Battlefield, ZoneType::Hand);
            }
        }

        Effect::ReturnToTopOfLibrary { .. } => {
            // Put target card from graveyard on top of owner's library.
            for target in targets {
                match target {
                    Target::Object(obj_id) => {
                        if let Some(inst) = state.objects.get(obj_id) {
                            let owner = inst.owner;
                            state.move_object(*obj_id, ZoneType::Graveyard, ZoneType::Library);
                            // Move to front (top) of library
                            if let Some(pos) = state.players[owner].library.iter().position(|&id| id == *obj_id) {
                                let id = state.players[owner].library.remove(pos);
                                state.players[owner].library.insert(0, id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Effect::UntapTarget { .. } => {
            for target in targets {
                match target {
                    Target::Object(obj_id) => {
                        if let Some(inst) = state.objects.get_mut(obj_id) {
                            inst.tapped = false;
                        }
                    }
                    _ => {}
                }
            }
        }

        // --- Zone manipulation effects ---

        Effect::ReturnFromGraveyardToBattlefield { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if state.players.iter().any(|p| p.graveyard.contains(id)) {
                        state.move_object(*id, ZoneType::Graveyard, ZoneType::Battlefield);
                        if let Some(inst) = state.objects.get_mut(id) {
                            inst.controller = controller;
                        }
                        state.refresh_continuous_effects();
                        let _ = super::triggers::fire_triggers(state, TriggerCondition::EntersBattlefield, Some(*id));
                    }
                }
            }
        }

        Effect::ReturnFromGraveyardToHand { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if state.players.iter().any(|p| p.graveyard.contains(id)) {
                        state.move_object(*id, ZoneType::Graveyard, ZoneType::Hand);
                    }
                }
            }
        }

        Effect::ExileFromGraveyard { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if state.players.iter().any(|p| p.graveyard.contains(id)) {
                        state.move_object(*id, ZoneType::Graveyard, ZoneType::Exile);
                    }
                }
            }
        }

        Effect::ExileFromHandLinked => {
            // Exile a card from controller's hand, linked to the source permanent.
            if let Some(source) = source_id {
                for target in targets {
                    if let Target::Object(id) = target {
                        let controller = state.objects.get(&source)
                            .map(|i| i.controller)
                            .unwrap_or(0);
                        if state.players[controller].hand.contains(id) {
                            state.move_object(*id, ZoneType::Hand, ZoneType::Exile);
                            if let Some(inst) = state.objects.get_mut(id) {
                                inst.exiled_by = Some(source);
                            }
                        }
                    }
                }
            }
        }

        Effect::ReturnLinkedExileToHand => {
            // Return a card exiled by the source permanent to its owner's hand.
            if let Some(source) = source_id {
                for target in targets {
                    if let Target::Object(id) = target {
                        let owner = state.objects.get(id).map(|i| i.owner).unwrap_or(0);
                        if state.players[owner].exile.contains(id)
                            && state.objects.get(id).and_then(|i| i.exiled_by) == Some(source)
                        {
                            if let Some(inst) = state.objects.get_mut(id) {
                                inst.exiled_by = None;
                            }
                            state.move_object(*id, ZoneType::Exile, ZoneType::Hand);
                        }
                    }
                }
            }
        }

        Effect::ShuffleIntoLibrary { .. } => {
            use rand::seq::SliceRandom;
            for target in targets {
                if let Target::Object(id) = target {
                    if state.battlefield.contains(id) {
                        let owner = state.objects.get(id).map(|i| i.owner).unwrap_or(0);
                        state.move_object(*id, ZoneType::Battlefield, ZoneType::Library);
                        let mut rng = rand::thread_rng();
                        state.players[owner].library.shuffle(&mut rng);
                    }
                }
            }
        }

        Effect::PutOnBottomOfLibrary { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if state.battlefield.contains(id) {
                        let owner = state.objects.get(id).map(|i| i.owner).unwrap_or(0);
                        state.move_object(*id, ZoneType::Battlefield, ZoneType::Library);
                        // move_object puts it at the end (bottom) by default via push, which is correct
                        // but we need to ensure it's at the end, not the front
                        if let Some(pos) = state.players[owner].library.iter().position(|&x| x == *id) {
                            let removed = state.players[owner].library.remove(pos);
                            state.players[owner].library.push(removed);
                        }
                    }
                }
            }
        }

        // --- Creature/permanent manipulation ---

        Effect::GainKeywordUntilEOT { keyword, .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if let Some(inst) = state.objects.get_mut(id) {
                        if !inst.temp_keywords.contains(keyword) {
                            inst.temp_keywords.push(*keyword);
                        }
                    }
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::SetPowerToughness { power, toughness, until_eot, .. } => {
            use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
            for target in targets {
                if let Target::Object(id) = target {
                    let duration = if *until_eot {
                        Duration::UntilEndOfTurn
                    } else {
                        Duration::Permanent
                    };
                    let ts = state.new_timestamp();
                    state.continuous_effects.push(ContinuousEffect {
                        source_id: *id,
                        controller,
                        timestamp: ts,
                        duration,
                        affected: AffectedObjects::Specific(*id),
                        modification: LayerModification::SetPT(*power, *toughness),
                    });
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::GainControlUntilEOT { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if let Some(inst) = state.objects.get_mut(id) {
                        inst.controller = controller;
                        // Grant haste so the creature can attack/tap this turn
                        if !inst.temp_keywords.contains(&KeywordAbility::Haste) {
                            inst.temp_keywords.push(KeywordAbility::Haste);
                        }
                    }
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::Fight { .. } => {
            // Both targets deal damage equal to their power to each other
            if targets.len() >= 2 {
                if let (Target::Object(a), Target::Object(b)) = (&targets[0], &targets[1]) {
                    let power_a = state.effective_power(*a).max(0) as u32;
                    let power_b = state.effective_power(*b).max(0) as u32;
                    if let Some(inst) = state.objects.get_mut(b) {
                        inst.damage_marked += power_a;
                    }
                    if let Some(inst) = state.objects.get_mut(a) {
                        inst.damage_marked += power_b;
                    }
                }
            }
        }

        Effect::TapTarget { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if let Some(inst) = state.objects.get_mut(id) {
                        inst.tapped = true;
                    }
                }
            }
        }

        // --- Player-targeted effects ---

        Effect::EachOpponentLosesLife { amount } => {
            let opponents: Vec<usize> = (0..state.players.len())
                .filter(|&i| i != controller)
                .collect();
            for opp in opponents {
                let old_life = state.players[opp].life;
                state.players[opp].life -= *amount as i32;
                state.emit_event(GameEvent::LifeChanged {
                    player: opp,
                    old: old_life,
                    new: state.players[opp].life,
                });
            }
        }

        Effect::EachOpponentDiscards { count } => {
            let opponents: Vec<usize> = (0..state.players.len())
                .filter(|&i| i != controller)
                .collect();
            for opp in opponents {
                super::discard_random(state, opp, *count as usize);
            }
        }

        Effect::EachOpponentSacrifices { count } => {
            let opponents: Vec<usize> = (0..state.players.len())
                .filter(|&i| i != controller)
                .collect();
            for opp in opponents {
                let mut creatures = state.creatures_controlled_by(opp);
                creatures.sort_by_key(|&id| state.effective_power(id));
                for &id in creatures.iter().take(*count as usize) {
                    state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
                }
            }
            state.refresh_continuous_effects();
        }

        Effect::DrawThenDiscard { draw, discard, .. } => {
            // Apply to the targeted player, or controller if no target
            let player = targets.iter().find_map(|t| {
                if let Target::Player(p) = t { Some(*p) } else { None }
            }).unwrap_or(controller);
            super::draw_cards(state, player, *draw as usize);
            super::discard_random(state, player, *discard as usize);
        }

        Effect::GainDynamicLife { amount } => {
            let db_ref = state.card_db.clone();
            let db = db_ref.as_ref().expect("card_db required");
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let n = amount.evaluate(
                controller,
                &state.objects,
                &state.battlefield,
                &|id| db.get(id),
                Some(&ctx),
            );
            if n > 0 {
                let old_life = state.players[controller].life;
                state.players[controller].life += n;
                state.emit_event(GameEvent::LifeChanged {
                    player: controller,
                    old: old_life,
                    new: state.players[controller].life,
                });
            }
        }

        // --- Conditional/modal effects ---

        Effect::Modal { choices, choose_count } => {
            // Simplified: for goldfish/AI, always choose the first N choices
            for effect in choices.iter().take(*choose_count as usize) {
                resolve_effect(state, effect, controller, targets, source_id);
            }
        }

        Effect::Conditional { condition, if_true, if_false } => {
            let met = evaluate_condition(state, condition, controller);
            if met {
                resolve_effect(state, if_true, controller, targets, source_id);
            } else if let Some(else_effect) = if_false {
                resolve_effect(state, else_effect, controller, targets, source_id);
            }
        }

        Effect::ForEach { count, effect } => {
            let db_ref = state.card_db.clone();
            let db = db_ref.as_ref().expect("card_db required");
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let n = count.evaluate(
                controller,
                &state.objects,
                &state.battlefield,
                &|id| db.get(id),
                Some(&ctx),
            );
            for _ in 0..n.max(0) {
                resolve_effect(state, effect, controller, targets, source_id);
            }
        }

        // --- Predefined tokens ---

        Effect::CreatePredefinedToken { token_type, count } => {
            let token_def = token_type.to_token_def();
            for _ in 0..*count {
                super::tokens::create_token(state, &token_def, controller);
            }
        }

        // --- Library manipulation ---

        Effect::Scry { count } => {
            // Simplified scry: for now, leave top cards in place (proper implementation
            // would need player choices about which to put on bottom).
            // In goldfish mode this is a no-op since the player can't make informed
            // choices without seeing the cards.
            let _ = count;
        }

        Effect::Proliferate => {
            // Add one counter of each type already present on each permanent/player
            // that the controller chooses. Simplified: proliferate all permanents
            // with +1/+1 counters (add another) and all players with poison counters.
            let bf = state.battlefield.clone();
            for &obj_id in &bf {
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    if inst.plus_counters > 0 {
                        inst.plus_counters += 1;
                    }
                    if inst.minus_counters > 0 {
                        inst.minus_counters += 1;
                    }
                    if inst.loyalty_counters > 0 {
                        inst.loyalty_counters += 1;
                    }
                }
            }
            for p in &mut state.players {
                if p.poison_counters > 0 {
                    p.poison_counters += 1;
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::BuffOtherSubtype {
            subtype,
            amount,
            until_eot,
        } => {
            use crate::layers::{
                AffectedObjects, ContinuousEffect, Duration, LayerModification,
            };
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let card_db_arc = state.card_db.as_ref().expect("card_db required").clone();
            let val = amount.evaluate(
                controller,
                &state.objects,
                &state.battlefield,
                &|id| card_db_arc.get(id),
                Some(&ctx),
            );
            if val > 0 {
                let ts = state.new_timestamp();
                let duration = if *until_eot {
                    Duration::UntilEndOfTurn
                } else {
                    Duration::Permanent
                };
                state.continuous_effects.push(ContinuousEffect {
                    source_id: source_id.unwrap_or(0),
                    controller,
                    timestamp: ts,
                    duration,
                    affected: AffectedObjects::OtherCreaturesWithSubtypeControlledBy(
                        subtype.clone(),
                        controller,
                    ),
                    modification: LayerModification::ModifyPT(val, val),
                });
                state.invalidate_characteristics_cache();
            }
        }

        Effect::Unimplemented(_) => {
            // Can't resolve unimplemented effects
        }

        Effect::ExtraLandDrop => {
            state.players[controller].land_plays_remaining += 1;
        }

        Effect::Surveil { count } => {
            // Simplified surveil: mill N cards (put top N into graveyard).
            // Full surveil would let you choose which go to GY vs stay on top.
            let n = (*count).min(state.players[controller].library.len() as u32);
            for _ in 0..n {
                if !state.players[controller].library.is_empty() {
                    let card_id = state.players[controller].library.remove(0);
                    state.players[controller].graveyard.push(card_id);
                }
            }
        }

        Effect::AddManaOfAnyColor { amount } => {
            // In goldfish/solver context, add green mana as default for "any color"
            state.players[controller].mana_pool.green += *amount;
        }

        Effect::DoublePowerUntilEOT { target: _ } => {
            // Double the source creature's power until EOT
            if let Some(sid) = source_id {
                let card_def_id = state.objects.get(&sid).map(|i| i.card_def_id);
                let base_power = card_def_id.and_then(|cid| {
                    let db = state.card_db();
                    db.get(cid).and_then(|d| d.power)
                }).unwrap_or(0);
                if let Some(inst) = state.objects.get_mut(&sid) {
                    let current_power = base_power + inst.temp_power_mod;
                    inst.temp_power_mod += current_power;
                }
            }
        }

        Effect::DealDynamicDamage { amount: _, target: _ } => {
            // Evaluated with full context in the card-specific handlers
            // For now, this is a no-op placeholder
        }

        Effect::CreateTokenCopyOfSource => {
            // Create a token that is a copy of the source permanent (same card_def_id).
            // The token inherits all abilities (e.g., Scute Swarm copies get landfall).
            if let Some(sid) = source_id {
                let card_def_id = state.objects.get(&sid).map(|i| i.card_def_id);
                if let Some(cid) = card_def_id {
                    let obj_id = state.create_card_in_zone(cid, controller, ZoneType::Battlefield);
                    if let Some(inst) = state.objects.get_mut(&obj_id) {
                        inst.controller = controller;
                        inst.is_token = true;
                        inst.summoning_sick = true;
                    }
                    state.refresh_continuous_effects();
                    let _ = super::triggers::fire_triggers(
                        state,
                        TriggerCondition::EntersBattlefield,
                        Some(obj_id),
                    );
                    super::triggers::check_triggers(
                        state,
                        TriggerCondition::ACreatureEnters,
                        None,
                    );
                    let _ = super::triggers::flush_triggers(state);
                }
            }
        }

        Effect::CreateTokenFromDef { card_def_id } => {
            // Create a token from a pre-registered CardDef in the database.
            let obj_id = state.create_card_in_zone(*card_def_id, controller, ZoneType::Battlefield);
            if let Some(inst) = state.objects.get_mut(&obj_id) {
                inst.controller = controller;
                inst.is_token = true;
                inst.summoning_sick = true;
            }
            state.refresh_continuous_effects();
            let _ = super::triggers::fire_triggers(
                state,
                TriggerCondition::EntersBattlefield,
                Some(obj_id),
            );
            super::triggers::check_triggers(state, TriggerCondition::ACreatureEnters, None);
            let _ = super::triggers::flush_triggers(state);
        }
    }
}

/// Evaluate a condition in the current game state.
fn evaluate_condition(
    state: &GameState,
    condition: &crate::card::effects::Condition,
    controller: PlayerIndex,
) -> bool {
    use crate::card::effects::Condition;
    match condition {
        Condition::ControlCreatures => {
            !state.creatures_controlled_by(controller).is_empty()
        }
        Condition::LifeAtOrAbove(n) => {
            state.players[controller].life >= *n
        }
        Condition::LifeAtOrBelow(n) => {
            state.players[controller].life <= *n
        }
        Condition::IsYourTurn => {
            state.active_player == controller
        }
        Condition::SourceHasCounters => {
            // Check if any target has +1/+1 counters (simplified)
            false
        }
        Condition::ControlNOrMore { count, card_type } => {
            let db = state.card_db();
            let matching = state.battlefield.iter().filter(|&&id| {
                if let Some(inst) = state.objects.get(&id) {
                    if inst.controller != controller {
                        return false;
                    }
                    if let Some(def) = db.get(inst.card_def_id) {
                        return def.card_types.contains(card_type);
                    }
                }
                false
            }).count();
            matching >= *count as usize
        }
        Condition::Always => true,
        Condition::HandIsEmpty => {
            state.players[controller].hand.is_empty()
        }
        Condition::ControlNOrMorePermanents { count } => {
            let matching = state.battlefield.iter().filter(|&&id| {
                state.objects.get(&id).map_or(false, |inst| inst.controller == controller)
            }).count();
            matching >= *count as usize
        }
    }
}
