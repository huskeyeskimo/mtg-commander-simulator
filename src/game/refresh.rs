//! Transient read-only preparation of the existing continuous refresh policy.
//! Allocation order (including statics producing no records) is unchanged.
use super::*;
use crate::{
    card::ExactObjectRef,
    layers::{AffectedObjects, Duration, LayerModification, StaticAbility},
    rules::sba::PreparedPassFailure,
};

#[derive(Debug)]
pub(crate) struct PreparedContinuousRefresh {
    effects: Vec<ContinuousEffect>,
    next_timestamp: u32,
}
impl PreparedContinuousRefresh {
    pub(crate) fn commit(self, state: &mut GameState) {
        state.continuous_effects = self.effects;
        state.next_timestamp = self.next_timestamp;
        state.invalidate_characteristics_cache();
    }
}

/// Only prospective inputs read by refresh; never a mutable/rollback GameState.
pub(crate) struct RefreshProjection {
    pub(crate) battlefield: Vec<ObjectId>,
    pub(crate) graveyards: Vec<Vec<ObjectId>>,
    pub(crate) effects: Vec<ContinuousEffect>,
    pub(crate) next_timestamp: u32,
    overrides: HashMap<ObjectId, CardInstance>,
    removed: HashSet<ObjectId>,
}
impl RefreshProjection {
    pub(crate) fn new(state: &GameState) -> Self {
        Self {
            battlefield: state.battlefield.clone(),
            graveyards: state.players.iter().map(|p| p.graveyard.clone()).collect(),
            effects: state.continuous_effects.clone(),
            next_timestamp: state.next_timestamp,
            overrides: HashMap::new(),
            removed: HashSet::new(),
        }
    }
    pub(crate) fn object<'a>(
        &'a self,
        state: &'a GameState,
        id: ObjectId,
    ) -> Option<&'a CardInstance> {
        if self.removed.contains(&id) {
            None
        } else {
            self.overrides.get(&id).or_else(|| state.objects.get(&id))
        }
    }
    pub(crate) fn put(&mut self, inst: CardInstance) {
        self.overrides.insert(inst.object_id, inst);
    }
    pub(crate) fn depart(
        &mut self,
        state: &GameState,
        id: ObjectId,
        to: ZoneType,
    ) -> Result<(), PreparedPassFailure> {
        let mut inst = self
            .object(state, id)
            .ok_or(PreparedPassFailure::StateEncoding)?
            .clone();
        inst.zone_change_count = inst
            .zone_change_count
            .checked_add(1)
            .ok_or(PreparedPassFailure::StateEncoding)?;
        inst.project_attachment_departure(ExactObjectRef {
            id,
            generation: inst.zone_change_count - 1,
        });
        self.battlefield.retain(|&object| object != id);
        for gy in &mut self.graveyards {
            gy.retain(|&object| object != id);
        }
        if to == ZoneType::Graveyard {
            self.graveyards
                .get_mut(inst.owner)
                .ok_or(PreparedPassFailure::StateEncoding)?
                .push(id);
        }
        // Real departure clears live links on all other sources as well.
        for source in self.battlefield.clone() {
            let mut other = self
                .object(state, source)
                .ok_or(PreparedPassFailure::StateEncoding)?
                .clone();
            if other.attachment_link().is_some_and(|link| {
                link.target.id == id && link.target.generation == inst.zone_change_count - 1
            }) {
                other.project_attachment_departure(ExactObjectRef {
                    id,
                    generation: inst.zone_change_count - 1,
                });
                self.put(other);
            }
        }
        self.put(inst);
        Ok(())
    }
    pub(crate) fn detach(
        &mut self,
        state: &GameState,
        source: ExactObjectRef,
        target: ExactObjectRef,
    ) -> bool {
        let Some(inst) = self.object(state, source.id) else {
            return false;
        };
        let mut inst = inst.clone();
        if !inst.project_detachment(source, target) {
            return false;
        }
        self.put(inst);
        true
    }
    pub(crate) fn cease(&mut self, id: ObjectId) {
        self.removed.insert(id);
        for gy in &mut self.graveyards {
            gy.retain(|&object| object != id);
        }
    }
    pub(crate) fn refresh(
        &mut self,
        state: &GameState,
    ) -> Result<PreparedContinuousRefresh, PreparedPassFailure> {
        let invalid = PreparedPassFailure::StateEncoding;
        let db = state.card_db.as_ref().ok_or(invalid.clone())?;
        let bf: HashSet<_> = self.battlefield.iter().copied().collect();
        if bf.len() != self.battlefield.len() {
            return Err(invalid);
        }
        let gy: HashSet<_> = self.graveyards.iter().flatten().copied().collect();
        let mut effects: Vec<_> = self
            .effects
            .iter()
            .filter(|e| match e.duration {
                Duration::WhileSourceOnBattlefield => bf.contains(&e.source_id),
                Duration::WhileSourceInGraveyard => gy.contains(&e.source_id),
                _ => true,
            })
            .cloned()
            .collect();
        let existing: HashSet<_> = effects
            .iter()
            .filter(|e| e.duration == Duration::WhileSourceOnBattlefield)
            .map(|e| e.source_id)
            .collect();
        let mut ts = self.next_timestamp;
        for &id in &self.battlefield {
            let inst = self.object(state, id).ok_or(invalid.clone())?;
            if inst.object_id != id
                || inst.owner >= state.players.len()
                || inst.controller >= state.players.len()
            {
                return Err(invalid);
            }
            if existing.contains(&id) {
                continue;
            }
            let Some(def) = db.get(inst.card_def_id) else {
                continue;
            };
            for ability in &def.static_abilities {
                let stamp = ts;
                ts = ts.checked_add(1).ok_or(invalid.clone())?;
                let generated = ability.to_continuous_effects(id, inst.controller, stamp);
                effects
                    .len()
                    .checked_add(generated.len())
                    .ok_or(invalid.clone())?;
                effects.extend(generated);
            }
        }
        let existing: HashSet<_> = effects
            .iter()
            .filter(|e| e.duration == Duration::WhileSourceInGraveyard)
            .map(|e| e.source_id)
            .collect();
        for (player, graveyard) in self.graveyards.iter().enumerate() {
            for &id in graveyard {
                if existing.contains(&id) {
                    continue;
                }
                let Some(inst) = self.object(state, id) else {
                    continue;
                };
                let Some(def) = db.get(inst.card_def_id) else {
                    continue;
                };
                for ability in &def.static_abilities {
                    if matches!(ability, StaticAbility::WonderInGraveyard) {
                        let stamp = ts;
                        ts = ts.checked_add(1).ok_or(invalid.clone())?;
                        effects.len().checked_add(1).ok_or(invalid.clone())?;
                        effects.push(ContinuousEffect {
                            source_id: id,
                            controller: player,
                            timestamp: stamp,
                            duration: Duration::WhileSourceInGraveyard,
                            affected: AffectedObjects::CreaturesControlledBy(player),
                            modification: LayerModification::AddKeyword(
                                crate::card::KeywordAbility::Flying,
                            ),
                        });
                    }
                }
            }
        }
        for effect in &effects {
            if effect.controller >= state.players.len()
                || matches!(effect.modification, LayerModification::ChangeController(player) if player >= state.players.len())
            {
                return Err(invalid);
            }
            if matches!(
                effect.duration,
                Duration::WhileSourceOnBattlefield | Duration::WhileSourceInGraveyard
            ) && self.object(state, effect.source_id).is_none()
            {
                return Err(invalid);
            }
            if let AffectedObjects::SpecificIncarnation {
                object_id,
                zone_change_count,
            } = effect.affected
            {
                if self
                    .object(state, object_id)
                    .is_some_and(|inst| inst.zone_change_count < zone_change_count)
                {
                    return Err(invalid);
                }
            }
        }
        self.next_timestamp = ts;
        self.effects = effects.clone();
        Ok(PreparedContinuousRefresh {
            effects,
            next_timestamp: ts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Mechanical frozen-before-correction reference; only successful capacity
    // cases exercise it. This is not a production alternate refresh authority.
    impl GameState {
        fn frozen_refresh_policy(&mut self) {
            self.invalidate_characteristics_cache();

            // Remove effects whose source has left the relevant zone
            let bf: HashSet<ObjectId> = self.battlefield.iter().copied().collect();
            let gy: HashSet<ObjectId> = self
                .players
                .iter()
                .flat_map(|p| p.graveyard.iter().copied())
                .collect();
            self.continuous_effects.retain(|e| {
                match e.duration {
                    crate::layers::Duration::WhileSourceOnBattlefield => bf.contains(&e.source_id),
                    crate::layers::Duration::WhileSourceInGraveyard => gy.contains(&e.source_id),
                    _ => true, // UntilEndOfTurn and Permanent effects persist
                }
            });

            // Phase 1: Read — collect new effects to add
            let existing_static_sources: std::collections::HashSet<ObjectId> = self
                .continuous_effects
                .iter()
                .filter(|e| e.duration == crate::layers::Duration::WhileSourceOnBattlefield)
                .map(|e| e.source_id)
                .collect();

            let mut new_effects = Vec::new();
            let mut ts = self.next_timestamp;
            {
                let db = self.card_db();
                for &obj_id in &self.battlefield {
                    if existing_static_sources.contains(&obj_id) {
                        continue;
                    }
                    let inst = match self.objects.get(&obj_id) {
                        Some(i) => i,
                        None => continue,
                    };
                    let def = match db.get(inst.card_def_id) {
                        Some(d) => d,
                        None => continue,
                    };
                    for sa in &def.static_abilities {
                        let generated = sa.to_continuous_effects(obj_id, inst.controller, ts);
                        ts += 1;
                        new_effects.extend(generated);
                    }
                }
            }

            // Phase 1b: Check graveyards for WonderInGraveyard static abilities.
            // These generate flying grants while the source is in a graveyard.
            let existing_gy_sources: std::collections::HashSet<ObjectId> = self
                .continuous_effects
                .iter()
                .filter(|e| e.duration == crate::layers::Duration::WhileSourceInGraveyard)
                .map(|e| e.source_id)
                .collect();
            {
                let db = self.card_db();
                for (pi, player) in self.players.iter().enumerate() {
                    for &obj_id in &player.graveyard {
                        if existing_gy_sources.contains(&obj_id) {
                            continue;
                        }
                        let inst = match self.objects.get(&obj_id) {
                            Some(i) => i,
                            None => continue,
                        };
                        let def = match db.get(inst.card_def_id) {
                            Some(d) => d,
                            None => continue,
                        };
                        for sa in &def.static_abilities {
                            if matches!(sa, crate::layers::StaticAbility::WonderInGraveyard) {
                                // Check if controller has an Island (or all lands are Islands
                                // via Prismatic Omen / Dryad). Simplified: always grant flying
                                // in this deck since Prismatic Omen makes all lands Islands.
                                new_effects.push(crate::layers::ContinuousEffect {
                                    source_id: obj_id,
                                    controller: pi,
                                    timestamp: ts,
                                    duration: crate::layers::Duration::WhileSourceInGraveyard,
                                    affected: crate::layers::AffectedObjects::CreaturesControlledBy(
                                        pi,
                                    ),
                                    modification: crate::layers::LayerModification::AddKeyword(
                                        crate::card::KeywordAbility::Flying,
                                    ),
                                });
                                ts += 1;
                            }
                        }
                    }
                }
            }

            // Phase 2: Write — apply collected effects
            self.next_timestamp = ts;
            self.continuous_effects.extend(new_effects);
        }
    }
    #[test]
    fn successful_refresh_matches_frozen_effect_and_timestamp_order() {
        use crate::card::{CardType, KeywordAbility};
        let mut db = CardDatabase::new();
        db.insert(CardDef {
            id: 979701,
            card_types: vec![CardType::Creature],
            power: Some(2),
            toughness: Some(2),
            static_abilities: vec![
                StaticAbility::Anthem {
                    power: 1,
                    toughness: 1,
                    affected: AffectedObjects::AllCreatures,
                },
                StaticAbility::GrantKeyword {
                    keyword: KeywordAbility::Flying,
                    affected: AffectedObjects::AllCreatures,
                },
            ],
            ..Default::default()
        });
        db.insert(CardDef {
            id: 979702,
            card_types: vec![CardType::Creature],
            power: Some(2),
            toughness: Some(2),
            static_abilities: vec![StaticAbility::ExtraLandDrops { count: 1 }; 3],
            ..Default::default()
        });
        db.insert(CardDef {
            id: 979703,
            static_abilities: vec![StaticAbility::WonderInGraveyard],
            ..Default::default()
        });
        for reversed in [false, true] {
            let mut checked = GameState::new(2);
            checked.card_db = Some(Arc::new(db.clone()));
            let a = checked
                .create_card_in_zone(979701, 0, ZoneType::Battlefield)
                .unwrap();
            checked
                .create_card_in_zone(979702, 1, ZoneType::Battlefield)
                .unwrap();
            checked
                .create_card_in_zone(979703, 1, ZoneType::Graveyard)
                .unwrap();
            if reversed {
                checked.battlefield.reverse();
            }
            checked.continuous_effects.push(ContinuousEffect {
                source_id: a,
                controller: 0,
                timestamp: 4,
                duration: Duration::UntilEndOfTurn,
                affected: AffectedObjects::Specific(a),
                modification: LayerModification::ModifyPT(2, 2),
            });
            checked.next_timestamp = 9;
            let mut frozen = checked.clone();
            for step in 0..4 {
                checked.refresh_continuous_effects();
                frozen.frozen_refresh_policy();
                assert!(checked.sba_failure.is_none());
                assert_eq!(checked.continuous_effects, frozen.continuous_effects);
                assert_eq!(checked.next_timestamp, frozen.next_timestamp);
                assert_eq!(
                    bincode::serialize(&checked).unwrap(),
                    bincode::serialize(&frozen).unwrap()
                );
                if step == 1 {
                    for state in [&mut checked, &mut frozen] {
                        // Frozen fixture zone update, exercising record removal
                        // and the unchanged next refresh allocation policy.
                        state.battlefield.retain(|&id| id != a);
                        state.players[0].graveyard.push(a);
                        state.objects.get_mut(&a).unwrap().zone_change_count += 1;
                    }
                }
            }
        }
    }
    #[test]
    fn linked_raw_departure_matches_frozen_refresh_after_actual_storage_order() {
        use crate::card::CardType;
        for (linked_owner, commander) in [(1, false), (0, false), (1, true)] {
            let mut db = CardDatabase::new();
            db.insert(CardDef {
                id: 979704,
                card_types: vec![CardType::Creature],
                power: Some(2),
                toughness: Some(2),
                static_abilities: vec![StaticAbility::WonderInGraveyard],
                ..Default::default()
            });
            let mut checked = GameState::new(2);
            checked.card_db = Some(Arc::new(db));
            let primary = checked
                .create_card_in_zone(979704, 1, ZoneType::Battlefield)
                .unwrap();
            let linked = checked
                .create_card_in_zone(979704, linked_owner, ZoneType::Exile)
                .unwrap();
            checked.objects.get_mut(&linked).unwrap().exiled_by = Some(primary);
            if commander {
                checked.format = GameFormat::Commander;
                checked.designate_commander(primary, 0).unwrap();

            }
            let stamp = checked.next_timestamp;
            let before_effects = checked.continuous_effects.clone();
            checked.move_object(primary, ZoneType::Battlefield, ZoneType::Graveyard);
            assert!(checked.sba_failure.is_none());
            assert_eq!(
                checked.players[linked_owner].graveyard.first(),
                Some(&linked)
            );
            if linked_owner == 1 && !commander {
                assert_eq!(checked.players[1].graveyard, vec![linked, primary]);
            }
            if commander {
                assert_eq!(checked.players[1].command_zone, vec![primary]);
            }
            let mut frozen = checked.clone();
            frozen.continuous_effects = before_effects;
            frozen.next_timestamp = stamp;
            frozen.frozen_refresh_policy();
            assert_eq!(checked.continuous_effects, frozen.continuous_effects);
            assert_eq!(checked.next_timestamp, frozen.next_timestamp);
            assert_eq!(
                bincode::serialize(&checked).unwrap(),
                bincode::serialize(&frozen).unwrap()
            );
        }
    }
}
