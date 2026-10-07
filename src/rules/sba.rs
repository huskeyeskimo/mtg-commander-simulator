//! Common-pass CR 704 stabilization. Planning is immutable and synchronous;
//! each pass owns one mixed-cause movement event and frozen nonmovement work.
//! The simulator applies a pinned deterministic semantic legend-retention policy
//! instead of exposing the player's legal choice. Strategic legend-choice
//! quality is not claimed. Unproved ties invalidate the run.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use super::transitions::{self, ExactObjectRef, PreparedSbaMovements, SbaCause, TransitionError};
use crate::card::{CardType, ObjectId, Supertype};
use crate::game::{GameState, StackSource, Target};
use crate::layers::AffectedObjects;

/// A stopped external boundary, never a suspended planner. Existing consumers
/// classify this as INVALID and cannot resume ordinary gameplay from a save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreparedPassFailure {
    Transition(TransitionError),
    UnprovedLegendTie,
    PreventedMandatoryMovement,
}

/// Version 1 public semantic legend-retention key. Numeric signed facts compare
/// numerically; sorted enum tags and typed linked summaries compare lexically.
/// Runtime identities, generations, allocation counters and storage order are
/// excluded. This does not add missing copy/name/supertype semantics.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LegendKey {
    version: u16,
    definition: u64,
    name: String,
    owner: usize,
    controller: usize,
    types: Vec<u8>,
    subtypes: Vec<String>,
    colors: Vec<u8>,
    keywords: Vec<u8>,
    abilities_removed: bool,
    power: i32,
    toughness: i32,
    plus: i32,
    minus: i32,
    damage: u32,
    tapped: bool,
    summoning_sick: bool,
    loyalty: u32,
    loyalty_activated: bool,
    temp_power: i32,
    temp_toughness: i32,
    temp_keywords: Vec<u8>,
    token: bool,
    commander: bool,
    linked_exiles: Vec<Vec<u8>>,
}

fn legend_key(state: &GameState, id: ObjectId) -> LegendKey {
    let inst = &state.objects[&id];
    let live = transitions::capture_live_source(state, id).expect("validated legend");
    let mut temp_keywords: Vec<_> = inst.temp_keywords.iter().map(|k| *k as u8).collect();
    temp_keywords.sort_unstable();
    LegendKey {
        version: 1,
        definition: inst.card_def_id,
        name: state.card_db().get(inst.card_def_id).unwrap().name.clone(),
        owner: inst.owner,
        controller: live.controller,
        types: live.card_types.iter().map(|v| *v as u8).collect(),
        subtypes: live.subtypes.iter().map(|v| v.0.clone()).collect(),
        colors: live.colors.iter().map(|v| *v as u8).collect(),
        keywords: live.keywords.iter().map(|v| *v as u8).collect(),
        abilities_removed: live.abilities_removed,
        power: live.power,
        toughness: live.toughness,
        plus: inst.plus_counters,
        minus: inst.minus_counters,
        damage: inst.damage_marked,
        tapped: inst.tapped,
        summoning_sick: inst.summoning_sick,
        loyalty: inst.loyalty_counters,
        loyalty_activated: inst.loyalty_activated_this_turn,
        temp_power: inst.temp_power_mod,
        temp_toughness: inst.temp_toughness_mod,
        temp_keywords,
        token: inst.is_token,
        commander: state.is_commander(id),
        linked_exiles: live
            .linked_exiles
            .iter()
            .map(transitions::typed_bytes)
            .collect(),
    }
}

fn target_mentions(target: &Target, ids: &HashSet<ObjectId>) -> bool {
    matches!(target, Target::Object(id) if ids.contains(id))
}

fn context_mentions(context: &crate::game::TriggerContext, ids: &HashSet<ObjectId>) -> bool {
    context
        .cast_spell
        .as_ref()
        .is_some_and(|cast| cast.targets.iter().any(|t| target_mentions(t, ids)))
        || context.zone_transition.as_ref().is_some_and(|zone| {
            ids.contains(&zone.source_before.object.id)
                || ids.contains(&zone.subject.before.object.id)
                || ids.contains(&zone.subject.after.id)
        })
}

/// Narrow proof for isolated twins only. No relationships refer to either
/// identity, no identity-bearing continuation is suspended, and the remaining
/// object facts agree. Selecting a storage representative after this proof is
/// semantically unobservable, not an ID/battlefield-order preference.
fn isolated_twins(state: &GameState, candidates: &[ObjectId]) -> bool {
    let ids: HashSet<_> = candidates.iter().copied().collect();
    let generation = state.objects[&candidates[0]].zone_change_count;
    let stored_controller = state.objects[&candidates[0]].controller;
    if state.pending_copy_order.is_some() || state.pending_tutor.is_some() {
        return false;
    }
    if candidates.iter().any(|id| {
        state.is_commander(*id)
                || state.objects[id].zone_change_count != generation
                // Effective control can hide a difference which reappears
                // after a global temporary control effect expires.
                || state.objects[id].controller != stored_controller
    }) {
        return false;
    }
    // Private-zone membership is an identity-bearing relationship too,
    // including a malformed candidate which would otherwise be the keeper.
    if state.players.iter().any(|player| {
        [
            &player.library,
            &player.hand,
            &player.graveyard,
            &player.exile,
            &player.command_zone,
        ]
        .iter()
        .any(|zone| zone.iter().any(|id| ids.contains(id)))
    }) {
        return false;
    }
    if state.objects.values().any(|inst| {
        (ids.contains(&inst.object_id)
            && (inst.attached_to.is_some()
                || !inst.attachments.is_empty()
                || inst.exiled_by.is_some()))
            || inst.attached_to.is_some_and(|id| ids.contains(&id))
            || inst.attachments.iter().any(|id| ids.contains(id))
            || inst.exiled_by.is_some_and(|id| ids.contains(&id))
    }) {
        return false;
    }
    if state.continuous_effects.iter().any(|e| {
        ids.contains(&e.source_id)
            || match e.affected {
                AffectedObjects::Specific(id) => ids.contains(&id),
                AffectedObjects::SpecificIncarnation { object_id, .. } => ids.contains(&object_id),
                _ => false,
            }
    }) || state
        .replacement_effects
        .iter()
        .any(|e| ids.contains(&e.source_id))
    {
        return false;
    }
    let combat = &state.combat;
    if combat.attackers.iter().any(|id| ids.contains(id))
        || combat
            .blockers
            .iter()
            .any(|(a, b)| ids.contains(a) || ids.contains(b))
        || combat
            .attacker_blockers
            .iter()
            .any(|(a, bs)| ids.contains(a) || bs.iter().any(|b| ids.contains(b)))
        || combat
            .damage_assignment
            .iter()
            .any(|(a, bs)| ids.contains(a) || bs.iter().any(|(b, _)| ids.contains(b)))
    {
        return false;
    }
    if state.pending_triggers.iter().any(|t| {
        ids.contains(&t.source_id)
            || t.targets.iter().any(|target| target_mentions(target, &ids))
            || context_mentions(&t.context, &ids)
    }) {
        return false;
    }
    !state.stack.iter().any(|entry| {
        entry
            .targets
            .iter()
            .any(|target| target_mentions(target, &ids))
            || match &entry.source {
                StackSource::Spell(id) | StackSource::ActivatedAbility { source_id: id, .. } => {
                    ids.contains(id)
                }
                StackSource::TriggeredAbility {
                    source_id, context, ..
                } => ids.contains(source_id) || context_mentions(context, &ids),
                StackSource::SpellCopy { .. } => false,
            }
    })
}

struct PreparedPass {
    movements: PreparedSbaMovements,
    cancellations: Vec<(ExactObjectRef, i32)>,
    orphan_equipment: Vec<ExactObjectRef>,
}

fn prepare_pass(state: &GameState) -> Result<PreparedPass, PreparedPassFailure> {
    let fail = PreparedPassFailure::Transition;
    let db = state
        .card_db
        .as_ref()
        .ok_or_else(|| fail(TransitionError::MissingDatabase))?;
    let mut nominal = BTreeMap::<ObjectId, (ExactObjectRef, Vec<SbaCause>)>::new();
    let mut cancellations = Vec::new();
    let mut orphan_equipment = Vec::new();
    let mut legends = BTreeMap::<(usize, String), Vec<ObjectId>>::new();
    let mut seen = HashSet::new();
    for &id in &state.battlefield {
        if !seen.insert(id) {
            return Err(fail(TransitionError::DuplicateSubject(id)));
        }
        let inst = state
            .objects
            .get(&id)
            .ok_or_else(|| fail(TransitionError::MissingObject(id)))?;
        if inst.object_id != id {
            return Err(fail(TransitionError::StaleIncarnation(id)));
        }
        let def = db
            .get(inst.card_def_id)
            .ok_or_else(|| fail(TransitionError::MissingDefinition(inst.card_def_id)))?;
        let chars = state
            .get_characteristics(id)
            .ok_or_else(|| fail(TransitionError::MissingObject(id)))?;
        if inst.owner >= state.players.len()
            || inst.controller >= state.players.len()
            || chars.controller >= state.players.len()
        {
            return Err(fail(TransitionError::InvalidPlayer(id)));
        }
        let object = ExactObjectRef {
            id,
            generation: inst.zone_change_count,
        };
        let mut causes = Vec::new();
        if chars.card_types.contains(&CardType::Creature) {
            if chars.toughness <= 0 {
                causes.push(SbaCause::ZeroToughness);
            }
            // Only the currently represented marked-damage condition. No new
            // regeneration or deathtouch state is introduced.
            if chars.toughness > 0 && i64::from(inst.damage_marked) >= i64::from(chars.toughness) {
                causes.push(SbaCause::LethalDamage);
            }
        }
        if chars.card_types.contains(&CardType::Planeswalker) && inst.loyalty_counters == 0 {
            causes.push(SbaCause::ZeroLoyalty);
        }
        if def.is_aura()
            && inst
                .attached_to
                .is_none_or(|target| !state.battlefield.contains(&target))
        {
            causes.push(SbaCause::OrphanAura);
        }
        if !causes.is_empty() {
            nominal.insert(id, (object, causes));
        }
        if def.supertypes.contains(&Supertype::Legendary) {
            legends
                .entry((chars.controller, def.name.clone()))
                .or_default()
                .push(id);
        }
        if inst.plus_counters > 0 && inst.minus_counters > 0 {
            cancellations.push((object, inst.plus_counters.min(inst.minus_counters)));
        }
        if def.is_equipment()
            && inst
                .attached_to
                .is_some_and(|target| !state.battlefield.contains(&target))
        {
            orphan_equipment.push(object);
        }
    }
    for ids in legends.values().filter(|ids| ids.len() > 1) {
        let keyed: Vec<_> = ids.iter().map(|&id| (id, legend_key(state, id))).collect();
        let minimum = keyed.iter().map(|(_, key)| key).min().unwrap();
        let minima: Vec<_> = keyed
            .iter()
            .filter(|(_, key)| key == minimum)
            .map(|(id, _)| *id)
            .collect();
        if minima.len() > 1 && !isolated_twins(state, &minima) {
            return Err(PreparedPassFailure::UnprovedLegendTie);
        }
        let keep = minima[0];
        for &id in ids.iter().filter(|&&id| id != keep) {
            let object = ExactObjectRef {
                id,
                generation: state.objects[&id].zone_change_count,
            };
            nominal
                .entry(id)
                .or_insert_with(|| (object, Vec::new()))
                .1
                .push(SbaCause::Legend);
        }
    }
    // Coalesce subjects across movement and frozen correction families before
    // any prevention or commit. Correction-only subjects need the same exact,
    // exclusive battlefield incarnation, but allocate no movement capacity.
    let mut subjects: BTreeMap<_, _> = nominal
        .values()
        .map(|(object, _)| (object.id, *object))
        .collect();
    for object in cancellations
        .iter()
        .map(|(object, _)| object)
        .chain(&orphan_equipment)
    {
        subjects.insert(object.id, *object);
    }
    transitions::validate_sba_subjects(state, &subjects.into_values().collect::<Vec<_>>())
        .map_err(fail)?;
    let movements =
        transitions::prepare_sba_movements(state, &nominal.into_values().collect::<Vec<_>>())
            .map_err(fail)?;
    // Counter cancellation preserves net P/T, and the existing orphan
    // correction refers only to absent targets. Neither can resolve a blocked
    // mandatory mover when no departure can change the next pass's view.
    if !movements.has_movers() && movements.prevented_mandatory {
        return Err(PreparedPassFailure::PreventedMandatoryMovement);
    }
    Ok(PreparedPass {
        movements,
        cancellations,
        orphan_equipment,
    })
}

fn commit_pass(state: &mut GameState, pass: PreparedPass) -> Result<(), TransitionError> {
    // Revalidate and capture from S before frozen nonmovement corrections.
    pass.movements.commit(state)?;
    for (object, cancel) in pass.cancellations {
        if let Some(inst) = state
            .objects
            .get_mut(&object.id)
            .filter(|inst| inst.zone_change_count == object.generation)
        {
            inst.plus_counters -= cancel;
            inst.minus_counters -= cancel;
        }
    }
    for object in pass.orphan_equipment {
        if let Some(inst) = state
            .objects
            .get_mut(&object.id)
            .filter(|inst| inst.zone_change_count == object.generation)
        {
            inst.attached_to = None;
        }
    }
    state.refresh_continuous_effects();
    state.refresh_replacement_effects();
    state.invalidate_characteristics_cache();
    Ok(())
}

/// Stabilize through distinct frozen passes before ordinary 2A placement.
/// The bool retains the legacy activity contract; preparation failures are
/// explicitly latched at the stopped boundary and never reported as stability.
pub fn check_state_based_actions(state: &mut GameState) -> bool {
    if state.game_over || state.sba_failure.is_some() || state.loss_boundary.unsupported.is_some() {
        return false;
    }
    if state.unsupported_continuing_elimination() {
        return super::loss::adjudicate(state, None);
    }
    if state.pending_copy_order.is_some() {
        return false;
    }
    let caller_deferred = std::mem::replace(&mut state.trigger_placement_deferred, true);
    let mut performed = false;
    loop {
        // The checkpointed loss set reads this same coherent pre-pass state.
        // Supported terminal and unsupported continuing loss stop before any
        // ordinary cleanup/departure, preserving that authoritative contract.
        if super::loss::adjudicate(state, None) {
            performed = true;
            break;
        }
        let pass = match prepare_pass(state) {
            Ok(pass) => pass,
            Err(error) => {
                state.sba_failure = Some(error);
                break;
            }
        };
        let progress = pass.movements.has_movers()
            || !pass.cancellations.is_empty()
            || !pass.orphan_equipment.is_empty();
        if !progress {
            if pass.movements.prevented_mandatory {
                state.sba_failure = Some(PreparedPassFailure::PreventedMandatoryMovement);
            }
            break;
        }
        if let Err(error) = commit_pass(state, pass) {
            state.sba_failure = Some(PreparedPassFailure::Transition(error));
            break;
        }
        performed = true;
    }
    state.trigger_placement_deferred = caller_deferred;
    if !state.gameplay_stopped()
        && !caller_deferred
        && !state.cleanup_discard_in_progress
        && !state.pending_triggers.is_empty()
    {
        let _ = super::triggers::flush_triggers(state);
    }
    performed
}

#[cfg(test)]
mod common_pass_tests {
    use super::*;
    use crate::card::{
        CardDef, Effect, KeywordAbility, Subtype, TriggerCondition, TriggeredAbility, ZoneType,
    };
    use crate::game::{CardDatabase, Phase};
    use crate::layers::{ContinuousEffect, Duration, LayerModification};
    use std::sync::Arc;
    fn fixture() -> GameState {
        let mut db = CardDatabase::new();
        db.insert(CardDef {
            id: 1,
            name: "Subject".into(),
            card_types: vec![CardType::Creature],
            power: Some(2),
            toughness: Some(2),
            triggered_abilities: vec![TriggeredAbility {
                trigger: TriggerCondition::LeavesBattlefield,
                effect: Effect::GainLife { amount: 1 },
                description: "leave".into(),
            }],
            ..Default::default()
        });
        for (id, name, ty, subtype) in [
            (2, "Aura", CardType::Enchantment, "Aura"),
            (3, "Equipment", CardType::Artifact, "Equipment"),
        ] {
            db.insert(CardDef {
                id,
                name: name.into(),
                card_types: vec![ty],
                subtypes: vec![Subtype(subtype.into())],
                ..Default::default()
            });
        }
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::PreCombatMain;
        state
    }
    #[test]
    fn private_one_pass_freezes_aura_equipment_and_counter_membership() {
        let mut s = fixture();
        let subject = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
        s.objects.get_mut(&subject).unwrap().damage_marked = 2;
        let orphan = s.create_card_in_zone(2, 0, ZoneType::Battlefield);
        let aura = s.create_card_in_zone(2, 0, ZoneType::Battlefield);
        s.objects.get_mut(&aura).unwrap().attached_to = Some(subject);
        let existing = s.create_card_in_zone(3, 0, ZoneType::Battlefield);
        s.objects.get_mut(&existing).unwrap().attached_to = Some(900);
        let equip = s.create_card_in_zone(3, 0, ZoneType::Battlefield);
        s.objects.get_mut(&equip).unwrap().attached_to = Some(subject);
        let counter = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
        s.objects.get_mut(&counter).unwrap().plus_counters = 2;
        s.objects.get_mut(&counter).unwrap().minus_counters = 1;
        let pass = prepare_pass(&s).unwrap();
        assert_eq!(pass.orphan_equipment.len(), 1);
        assert_eq!(pass.cancellations.len(), 1);
        commit_pass(&mut s, pass).unwrap();
        assert!(s.players[0].graveyard.contains(&orphan));
        assert!(s.players[0].graveyard.contains(&subject));
        assert!(s.battlefield.contains(&aura));
        assert_eq!(s.objects[&existing].attached_to, None);
        assert_eq!(s.objects[&equip].attached_to, Some(subject));
        assert_eq!(s.objects[&counter].plus_counters, 1);
        let later = prepare_pass(&s).unwrap();
        assert_eq!(later.orphan_equipment.len(), 1);
        commit_pass(&mut s, later).unwrap();
        assert!(s.players[0].graveyard.contains(&aura));
        assert_eq!(s.objects[&equip].attached_to, None);
        assert_eq!(s.next_zone_event_group_id, 2);
        assert!(s.stack.is_empty());
    }
    #[test]
    fn private_stabilization_before_placement_and_restore_has_no_planner_state() {
        let mut s = fixture();
        for _ in 0..2 {
            let id = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
            s.objects.get_mut(&id).unwrap().damage_marked = 2;
        }
        s.trigger_placement_deferred = true;
        assert!(check_state_based_actions(&mut s));
        assert!(s.stack.is_empty());
        assert_eq!(s.pending_triggers.len(), 2);
        assert!(s.trigger_placement_deferred);
        let json = serde_json::to_vec(&s).unwrap();
        assert!(!String::from_utf8_lossy(&json).contains("movements"));
        let mut restored: GameState = serde_json::from_slice(&json).unwrap();
        restored.card_db = s.card_db.clone();
        assert!(!check_state_based_actions(&mut restored));
        assert_eq!(restored.pending_triggers.len(), 2);
        assert!(restored.trigger_order_resume.is_some());
    }
    #[test]
    fn private_three_pass_layered_cascade_recomputes_only_between_passes() {
        let mut s = fixture();
        let first = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
        let second = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
        let a = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
        let b = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
        s.objects.get_mut(&first).unwrap().damage_marked = 2;
        for id in [second, a, b] {
            s.objects.get_mut(&id).unwrap().temp_toughness_mod = -2;
        }
        s.continuous_effects.push(ContinuousEffect {
            source_id: first,
            controller: 0,
            timestamp: 1,
            duration: Duration::WhileSourceOnBattlefield,
            affected: AffectedObjects::Specific(second),
            modification: LayerModification::ModifyPT(0, 1),
        });
        s.continuous_effects.push(ContinuousEffect {
            source_id: second,
            controller: 0,
            timestamp: 2,
            duration: Duration::WhileSourceOnBattlefield,
            affected: AffectedObjects::OtherCreatures,
            modification: LayerModification::ModifyPT(0, 1),
        });
        s.objects.get_mut(&first).unwrap().damage_marked = 3;
        s.invalidate_characteristics_cache();
        check_state_based_actions(&mut s);
        let group = |id| {
            s.pending_triggers
                .iter()
                .find(|t| t.source_id == id)
                .unwrap()
                .context
                .zone_transition
                .as_ref()
                .unwrap()
                .group_id
        };
        assert_ne!(group(first), group(second));
        assert_ne!(group(second), group(a));
        assert_eq!(group(a), group(b));
        assert_eq!(s.next_zone_event_group_id, 3);
    }
    #[test]
    fn private_historical_keyword_lki_precedes_cancellation_without_duplicates() {
        let mut s = fixture();
        let id = s.create_card_in_zone(1, 0, ZoneType::Battlefield);
        let inst = s.objects.get_mut(&id).unwrap();
        inst.plus_counters = 1;
        inst.minus_counters = 1;
        inst.damage_marked = 2;
        inst.temp_keywords = vec![KeywordAbility::Undying, KeywordAbility::Persist];
        s.trigger_placement_deferred = true;
        check_state_based_actions(&mut s);
        assert_eq!(s.pending_triggers.len(), 1);
        let before = &s.pending_triggers[0]
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .subject
            .before;
        assert_eq!((before.plus_counters, before.minus_counters), (1, 1));
    }
}
