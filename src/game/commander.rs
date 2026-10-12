//! Immutable designation identity and public histories, independent of incarnation.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CommanderDesignation {
    pub owner: PlayerIndex,
    /// Existing setup roles: primary = 0, partner = 1.
    pub role: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommanderRecord {
    pub designation: CommanderDesignation,
    pub object: ObjectId,
    /// Public designation at setup, not current copied characteristics.
    pub initial_card: CardId,
    pub casts_by_player: Vec<u32>,
    pub damage_to_player: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PublicCommander {
    pub designation: CommanderDesignation,
    pub initial_card: CardId,
    pub casts_by_player: Vec<u32>,
    pub damage_to_player: Vec<u32>,
}

impl GameState {
    pub fn designate_commander(&mut self, object: ObjectId, role: u8) -> Result<(), &'static str> {
        self.validate_commanders()?;
        let inst = self.objects.get(&object).ok_or("missing commander object")?;
        if !self.is_commander_format() || !self.is_card(object) || role > 1 || inst.owner >= self.players.len() {
            return Err("invalid commander binding");
        }
        let designation = CommanderDesignation { owner: inst.owner, role };
        if self.commanders.iter().any(|c| c.object == object || c.designation == designation) {
            return Err("duplicate commander designation");
        }
        self.commanders.push(CommanderRecord { designation, object, initial_card: inst.card_def_id,
            casts_by_player: vec![0; self.players.len()], damage_to_player: vec![0; self.players.len()] });
        if let Err(error) = self.validate_commanders() { self.commanders.pop(); return Err(error); }
        Ok(())
    }

    pub fn commander_for_object(&self, object: ObjectId) -> Option<&CommanderRecord> {
        if !self.is_commander_format() { return None; }
        self.commanders.iter().find(|c| c.object == object)
    }

    pub fn commander(&self, owner: usize, role: u8) -> Option<&CommanderRecord> {
        self.commanders.iter().find(|c| c.designation == CommanderDesignation { owner, role })
    }

    pub fn commander_mut(&mut self, owner: usize, role: u8) -> Option<&mut CommanderRecord> {
        self.commanders.iter_mut().find(|c| c.designation == CommanderDesignation { owner, role })
    }

    pub fn commander_casts(&self, owner: usize, role: u8, caster: usize) -> u32 {
        self.commander(owner, role).and_then(|c| c.casts_by_player.get(caster)).copied().unwrap_or(0)
    }

    pub fn public_commanders(&self) -> Vec<PublicCommander> {
        let mut result: Vec<_> = self.commanders.iter().map(|c| PublicCommander {
            designation: c.designation, initial_card: c.initial_card,
            casts_by_player: c.casts_by_player.clone(), damage_to_player: c.damage_to_player.clone(),
        }).collect();
        result.sort();
        result
    }

    pub fn validate_commanders(&self) -> Result<(), &'static str> {
        if !self.is_commander_format() && !self.commanders.is_empty() { return Err("designation outside Commander format"); }
        let mut bindings = HashSet::new();
        let mut designations = HashSet::new();
        for c in &self.commanders {
            let Some(inst) = self.objects.get(&c.object) else { return Err("missing commander object"); };
            if c.designation.owner >= self.players.len() || c.designation.role > 1
                || inst.owner != c.designation.owner || inst.object_id != c.object || !self.is_card(c.object)
                || !bindings.insert(c.object) || !designations.insert(c.designation)
                || c.casts_by_player.len() != self.players.len() || c.damage_to_player.len() != self.players.len()
                || c.casts_by_player.iter().any(|n| n.checked_mul(2).is_none()) {
                return Err("invalid commander identity or ledger");
            }
            let residence = self.battlefield.iter().filter(|&&id| id == c.object).count()
                + usize::from(self.pending_copy_order.as_ref().and_then(PendingCopyOrder::resolving_entry)
                    .is_some_and(|entry| entry.source.physical_object() == Some(c.object)))
                + self.stack.iter().filter(|entry| entry.source.physical_object() == Some(c.object)).count()
                + self.players.iter().map(|p| [&p.library, &p.hand, &p.graveyard, &p.exile, &p.command_zone]
                    .iter().map(|zone| zone.iter().filter(|&&id| id == c.object).count()).sum::<usize>()).sum::<usize>();
            if self.card_db.as_ref().is_some_and(|db| db.get(c.initial_card).is_none()) {
                return Err("unknown initial commander definition");
            }
            if residence != 1 { return Err("invalid commander residence"); }
            for (seat, p) in self.players.iter().enumerate() {
                if seat != inst.owner && [&p.library, &p.hand, &p.graveyard, &p.exile, &p.command_zone]
                    .iter().any(|zone| zone.contains(&c.object)) { return Err("commander in nonowner zone"); }
            }
        }
        // Designation belongs to a physical card, so every retained incarnation
        // must agree with the registry, even after that card becomes hidden.
        // This validation never exports the current object's private facts.
        let contexts = self.pending_triggers.iter().map(|t| &t.context)
            .chain(self.trigger_placement.iter().flat_map(|op| op.occurrences.iter().map(|t| &t.context)))
            .chain(self.stack.iter().filter_map(|entry| match &entry.source {
                StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()), _ => None,
            }))
            .chain(self.pending_copy_order.as_ref().and_then(PendingCopyOrder::resolving_entry)
                .and_then(|entry| match &entry.source {
                    StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()), _ => None,
                }));
        for context in contexts {
            if let Some(zone) = &context.zone_transition {
                for historical in [&zone.source_before, &zone.subject.before] {
                    if historical.commander != self.commander_for_object(historical.object.id).map(|c| c.designation) {
                        return Err("invalid historical commander designation");
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod historical_designation_tests {
    use super::*;
    use crate::{action::{legal_actions, Action}, card::{sample, ZoneType}, info_set::InformationSet,
        rules::{self, transitions::{transition_batch, TransitionRequest, MovementKind}}};

    fn history() -> GameState {
        let mut s = GameState::new_commander(2);
        let mut db = sample::build_sample_db();
        db.cards.get_mut(&sample::ids::BLOOD_ARTIST).unwrap().triggered_abilities = vec![crate::card::TriggeredAbility {
            trigger: crate::card::TriggerCondition::APermanentLeaves,
            effect: crate::card::Effect::GainLife { amount: 1 }, description: "Historical observer".into(),
        }];
        s.card_db = Some(Arc::new(db));
        s.phase = Phase::PreCombatMain;
        let watcher = s.create_card_in_zone(sample::ids::BLOOD_ARTIST, 0, ZoneType::Battlefield).unwrap();
        let subject = s.create_card_in_zone(sample::ids::LLANOWAR_ELVES, 0, ZoneType::Battlefield).unwrap();
        s.designate_commander(watcher, 0).unwrap();
        s.designate_commander(subject, 1).unwrap();
        let exact = s.exact_object(subject).unwrap();
        transition_batch(&mut s, &[TransitionRequest { object: exact, from: ZoneType::Battlefield,
            to: ZoneType::Graveyard, kind: MovementKind::Put }]).unwrap();
        assert_eq!(s.pending_triggers.len(), 1);
        s
    }

    #[test]
    fn historical_designations_reject_forgery_in_every_retained_context() {
        for location in 0..4 {
            let mut valid = history();
            let trigger = valid.pending_triggers[0].clone();
            let entry = StackEntry { id: valid.next_stack_id, controller: 0, targets: trigger.targets.clone(),
                target_generations: crate::targeting::target_generations(&valid, &trigger.targets),
                source: StackSource::TriggeredAbility { source_id: trigger.source_id,
                    ability_index: trigger.ability_index, context: Box::new(trigger.context.clone()) } };
            match location {
                0 => {},
                1 => { valid.pending_triggers.clear(); valid.stack.push(entry); valid.next_stack_id += 1; },
                2 => { valid.pending_triggers.push(trigger); rules::check_state_based_actions(&mut valid);
                    assert!(valid.pending_commander_sba.is_some());
                    rules::apply_action(&mut valid, &Action::ChooseCommanderSba {
                        designation: CommanderDesignation { owner: 0, role: 1 }, to_command: false,
                    });
                    assert!(valid.trigger_placement.is_some(), "pending choice {:?}, triggers {}, stack {}, failure {:?}, actions {:?}",
                        valid.pending_commander_sba, valid.pending_triggers.len(), valid.stack.len(),
                        valid.sba_failure, legal_actions(&valid)); },
                _ => { valid.pending_triggers.clear(); valid.pending_copy_order = Some(PendingCopyOrder {
                    controller: 0, items: vec![], selected_order: vec![], expected_stack_len: 0,
                    expected_next_stack_id: valid.next_stack_id, resolving_entry: Some(Box::new(entry)),
                    resolving_source_generation: None }); },
            }
            assert!(valid.validate_commanders().is_ok());
            assert!(serde_json::from_slice::<GameState>(&serde_json::to_vec(&valid).unwrap()).is_ok());
            assert!(bincode::deserialize::<GameState>(&bincode::serialize(&valid).unwrap()).is_ok());
            for subject in [false, true] { for fault in 0..5 {
                let mut bad = valid.clone();
                let context = match location {
                    0 => &mut bad.pending_triggers[0].context,
                    2 => &mut bad.trigger_placement.as_mut().unwrap().occurrences[0].context,
                    _ => {
                        let entry = if location == 1 { &mut bad.stack[0] }
                            else { bad.pending_copy_order.as_mut().unwrap().resolving_entry.as_mut().unwrap() };
                        let StackSource::TriggeredAbility { context, .. } = &mut entry.source else { unreachable!() };
                        context.as_mut()
                    }
                };
                let zone = context.zone_transition.as_mut().unwrap();
                let frame = if subject { &mut zone.subject.before } else { &mut zone.source_before };
                frame.commander = match fault {
                    0 => None,
                    1 => Some(CommanderDesignation { owner: usize::MAX, role: 0 }),
                    2 => Some(CommanderDesignation { owner: 0, role: 2 }),
                    3 => Some(CommanderDesignation { owner: 1, role: 0 }),
                    _ => Some(CommanderDesignation { owner: 0, role: if subject { 0 } else { 1 } }),
                };
                assert!(bad.validate_commanders().is_err(), "location {location}, subject {subject}, fault {fault}");
                assert!(serde_json::from_slice::<GameState>(&serde_json::to_vec(&bad).unwrap()).is_err());
                assert!(bincode::deserialize::<GameState>(&bincode::serialize(&bad).unwrap()).is_err());
                assert!(valid.clone().restore(bad.snapshot()).is_err());
                assert!(legal_actions(&bad).is_empty());
                assert!(InformationSet::from_view(&bad.visible_state(0), bad.card_db()).is_err());
                rules::apply_action(&mut bad, &Action::PassPriority);
                assert!(bad.sba_failure.is_some());
            } }
        }
    }

    #[test]
    fn valid_historical_designation_survives_hidden_reentry_and_restoration() {
        let mut s = history();
        let source = s.commander(0, 0).unwrap().object;
        let generation = s.objects[&source].zone_change_count;
        s.move_object(source, ZoneType::Battlefield, ZoneType::Hand);
        assert!(s.objects[&source].zone_change_count > generation);
        assert!(s.validate_commanders().is_ok());
        let expected = InformationSet::from_view(&s.visible_state(1), s.card_db()).unwrap().hash_value();
        let mut snapshot = s.clone(); snapshot.restore(s.snapshot()).unwrap();
        for mut restored in [s.clone(), snapshot, serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap(),
            bincode::deserialize(&bincode::serialize(&s).unwrap()).unwrap()] {
            restored.card_db = s.card_db.clone();
            assert!(restored.validate_commanders().is_ok());
            assert_eq!(InformationSet::from_view(&restored.visible_state(1), restored.card_db()).unwrap().hash_value(), expected);
        }
    }
}
