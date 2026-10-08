//! One source-bound authority for currently represented attachments.
//! Reverse inventories and departure plans are transient and never serialized.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{CardId, CardType, ExactObjectRef, ObjectId};
use crate::game::GameState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttachmentKind {
    Equipment,
    Aura,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttachmentLink {
    pub source_generation: u32,
    pub target: ExactObjectRef,
    pub kind: AttachmentKind,
    pub timestamp: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentContext {
    /// Already-established synthetic/API relationship; assign its represented
    /// attachment-source timestamp. Full Equip stack semantics are deferred.
    Established,
    /// Preserve the existing immediate Equip timestamp behavior until 2D.2.
    ExistingEquip,
    /// Mechanical conversion of the existing Aura resolution path, not new
    /// enchant metadata or a new target-validity rule.
    ExistingAuraResolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentError {
    Source,
    Target,
    Kind,
    Generation,
    Membership,
    Timestamp,
}

#[derive(Debug)]
pub struct PreparedAttachment {
    source: ExactObjectRef,
    target: ExactObjectRef,
    old_link: Option<AttachmentLink>,
    kind: AttachmentKind,
    context: AttachmentContext,
}

/// Owned public evidence, independent of any newer or hidden current object.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttachmentEvidence {
    pub source: ExactObjectRef,
    pub target: ExactObjectRef,
    pub kind: AttachmentKind,
    pub source_card_id: CardId,
    pub source_owner: usize,
    pub source_controller: usize,
    pub source_is_token: bool,
    pub target_card_id: CardId,
    pub target_owner: usize,
    pub target_controller: usize,
    pub target_is_token: bool,
}

#[derive(Debug, Default)]
pub(crate) struct AttachmentInventory {
    pub evidence: Vec<AttachmentEvidence>,
    links: HashMap<ExactObjectRef, Vec<(ObjectId, AttachmentLink)>>,
}

impl GameState {
    pub fn exact_object(&self, id: ObjectId) -> Option<ExactObjectRef> {
        self.objects.get(&id).map(|inst| ExactObjectRef {
            id,
            generation: inst.zone_change_count,
        })
    }

    pub fn attachment_target(&self, source: ExactObjectRef) -> Option<ExactObjectRef> {
        let inst = self.objects.get(&source.id)?;
        let link = inst.attachment?;
        (inst.zone_change_count == source.generation
            && link.source_generation == source.generation
            && self.battlefield.contains(&source.id)
            && self.battlefield.contains(&link.target.id)
            && self.exact_object(link.target.id) == Some(link.target))
        .then_some(link.target)
    }

    pub fn attachments_of(&self, target: ExactObjectRef) -> Vec<ExactObjectRef> {
        let mut incoming: Vec<_> = self
            .objects
            .values()
            .filter_map(|inst| {
                let source = ExactObjectRef {
                    id: inst.object_id,
                    generation: inst.zone_change_count,
                };
                (self.attachment_target(source) == Some(target)).then_some(source)
            })
            .collect();
        incoming.sort_by_key(|source| (source.id, source.generation));
        incoming
    }

    pub fn prepare_attach(
        &self,
        source: ExactObjectRef,
        target: ExactObjectRef,
        context: AttachmentContext,
    ) -> Result<PreparedAttachment, AttachmentError> {
        if self.exact_object(source.id) != Some(source) || !self.battlefield.contains(&source.id) {
            return Err(AttachmentError::Source);
        }
        if self.exact_object(target.id) != Some(target)
            || !self.battlefield.contains(&target.id)
            || source == target
        {
            return Err(AttachmentError::Target);
        }
        let inst = &self.objects[&source.id];
        if self
            .battlefield
            .iter()
            .filter(|&&id| id == source.id)
            .count()
            != 1
            || self
                .battlefield
                .iter()
                .filter(|&&id| id == target.id)
                .count()
                != 1
        {
            return Err(AttachmentError::Membership);
        }
        if inst
            .attachment
            .is_some_and(|link| link.source_generation != source.generation)
        {
            return Err(AttachmentError::Generation);
        }
        let def = self
            .card_db
            .as_ref()
            .and_then(|db| db.get(inst.card_def_id))
            .ok_or(AttachmentError::Kind)?;
        let kind = if def.is_equipment() {
            if !self.is_creature(target.id) {
                return Err(AttachmentError::Target);
            }
            AttachmentKind::Equipment
        } else if def.is_aura() {
            AttachmentKind::Aura
        } else {
            return Err(AttachmentError::Kind);
        };
        if let Some(link) = inst.attachment {
            if link.kind != kind || link.target.id == source.id {
                return Err(AttachmentError::Kind);
            }
            if let Some(old_target) = self.objects.get(&link.target.id) {
                if old_target.zone_change_count < link.target.generation {
                    return Err(AttachmentError::Generation);
                }
                if old_target.zone_change_count == link.target.generation
                    && self
                        .battlefield
                        .iter()
                        .filter(|&&id| id == old_target.object_id)
                        .count()
                        != 1
                {
                    return Err(AttachmentError::Target);
                }
            }
        }
        if context == AttachmentContext::Established && self.next_timestamp == u32::MAX {
            return Err(AttachmentError::Timestamp);
        }
        Ok(PreparedAttachment {
            source,
            target,
            old_link: inst.attachment,
            kind,
            context,
        })
    }

    pub fn commit_attach(&mut self, prepared: PreparedAttachment) -> Result<(), AttachmentError> {
        let current = self.prepare_attach(prepared.source, prepared.target, prepared.context)?;
        if current.old_link != prepared.old_link || current.kind != prepared.kind {
            return Err(AttachmentError::Source);
        }
        let timestamp = if prepared.context == AttachmentContext::Established {
            self.new_timestamp()
        } else {
            prepared
                .old_link
                .map_or(self.next_timestamp, |link| link.timestamp)
        };
        self.objects
            .get_mut(&prepared.source.id)
            .ok_or(AttachmentError::Source)?
            .attachment = Some(AttachmentLink {
            source_generation: prepared.source.generation,
            target: prepared.target,
            kind: prepared.kind,
            timestamp,
        });
        if prepared.context == AttachmentContext::Established {
            for effect in &mut self.continuous_effects {
                if effect.source_id == prepared.source.id
                    && effect.affected == crate::layers::AffectedObjects::AttachedTo
                {
                    effect.timestamp = timestamp;
                }
            }
        }
        self.refresh_continuous_effects();
        self.invalidate_characteristics_cache();
        Ok(())
    }

    pub fn detach_exact(
        &mut self,
        source: ExactObjectRef,
        expected_target: ExactObjectRef,
    ) -> bool {
        let Some(inst) = self.objects.get_mut(&source.id) else {
            return false;
        };
        if inst.zone_change_count != source.generation
            || !inst.attachment.is_some_and(|link| {
                link.source_generation == source.generation && link.target == expected_target
            })
        {
            return false;
        }
        inst.attachment = None;
        self.refresh_continuous_effects();
        self.invalidate_characteristics_cache();
        true
    }

    pub(crate) fn capture_attachment_inventory(&self) -> AttachmentInventory {
        let mut inventory = AttachmentInventory::default();
        for inst in self.objects.values() {
            let source = ExactObjectRef {
                id: inst.object_id,
                generation: inst.zone_change_count,
            };
            let Some(link) = inst.attachment else {
                continue;
            };
            // Cleanup includes a source's orphan link; history asserts only
            // relationships whose exact endpoints really were on Battlefield.
            inventory
                .links
                .entry(source)
                .or_default()
                .push((source.id, link));
            inventory
                .links
                .entry(link.target)
                .or_default()
                .push((source.id, link));
            if self.attachment_target(source) != Some(link.target) {
                continue;
            }
            let Some(target) = self.objects.get(&link.target.id) else {
                continue;
            };
            inventory.evidence.push(AttachmentEvidence {
                source,
                target: link.target,
                kind: link.kind,
                source_card_id: inst.card_def_id,
                source_owner: inst.owner,
                source_controller: self
                    .get_characteristics(source.id)
                    .map_or(inst.controller, |c| c.controller),
                source_is_token: inst.is_token,
                target_card_id: target.card_def_id,
                target_owner: target.owner,
                target_controller: self
                    .get_characteristics(target.object_id)
                    .map_or(target.controller, |c| c.controller),
                target_is_token: target.is_token,
            });
        }
        inventory
    }

    pub(crate) fn cleanup_attachment_departure(
        &mut self,
        departed: ExactObjectRef,
        inventory: &AttachmentInventory,
    ) {
        if let Some(links) = inventory.links.get(&departed) {
            for &(source, expected) in links {
                if let Some(inst) = self.objects.get_mut(&source) {
                    if inst.attachment == Some(expected) {
                        inst.attachment = None;
                    }
                }
            }
        }
        self.invalidate_characteristics_cache();
    }

    /// Read-only structural validation of the finite forward authority.
    /// Missing/older targets are orphan corrections; contradictory current
    /// endpoints cannot be silently repaired by a frozen SBA detach plan.
    pub(crate) fn validate_attachment_structure(&self) -> Result<(), AttachmentError> {
        for inst in self.objects.values() {
            let Some(link) = inst.attachment else {
                continue;
            };
            if link.source_generation != inst.zone_change_count {
                return Err(AttachmentError::Generation);
            }
            if self
                .battlefield
                .iter()
                .filter(|&&id| id == inst.object_id)
                .count()
                != 1
            {
                return Err(AttachmentError::Source);
            }
            let definition = self
                .card_db
                .as_ref()
                .and_then(|db| db.get(inst.card_def_id))
                .ok_or(AttachmentError::Kind)?;
            if link.target.id == inst.object_id
                || match link.kind {
                    AttachmentKind::Equipment => !definition.is_equipment(),
                    AttachmentKind::Aura => !definition.is_aura(),
                }
            {
                return Err(AttachmentError::Kind);
            }
            if let Some(target) = self.objects.get(&link.target.id) {
                if target.zone_change_count < link.target.generation {
                    return Err(AttachmentError::Generation);
                }
                if target.zone_change_count == link.target.generation
                    && !self.battlefield.contains(&target.object_id)
                {
                    return Err(AttachmentError::Target);
                }
            }
        }
        Ok(())
    }

    /// Validate at a mutable state-entry boundary. Only missing/older targets
    /// can undergo approved orphan cleanup; impossible structural links reject.
    pub fn validate_attachment_state(
        &mut self,
        clean_orphans: bool,
    ) -> Result<(), AttachmentError> {
        self.validate_attachment_structure()?;
        let mut membership = std::collections::HashSet::new();
        for id in self
            .battlefield
            .iter()
            .chain(self.players.iter().flat_map(|seat| {
                seat.hand
                    .iter()
                    .chain(&seat.library)
                    .chain(&seat.graveyard)
                    .chain(&seat.exile)
                    .chain(&seat.command_zone)
            }))
        {
            if !membership.insert(*id) || !self.objects.contains_key(id) {
                return Err(AttachmentError::Membership);
            }
        }
        for entry in &self.stack {
            if let crate::game::StackSource::Spell(id) = entry.source {
                if !membership.insert(id) || !self.objects.contains_key(&id) {
                    return Err(AttachmentError::Membership);
                }
            }
        }
        let mut orphaned = Vec::new();
        for inst in self.objects.values() {
            let Some(link) = inst.attachment else {
                continue;
            };
            let definition = self
                .card_db
                .as_ref()
                .and_then(|db| db.get(inst.card_def_id))
                .ok_or(AttachmentError::Kind)?;
            if link.target.id == inst.object_id
                || match link.kind {
                    AttachmentKind::Equipment => !definition.is_equipment(),
                    AttachmentKind::Aura => !definition.is_aura(),
                }
            {
                return Err(AttachmentError::Kind);
            }
            if link.source_generation != inst.zone_change_count {
                return Err(AttachmentError::Generation);
            }
            if self
                .battlefield
                .iter()
                .filter(|&&id| id == inst.object_id)
                .count()
                != 1
            {
                return Err(AttachmentError::Source);
            }
            match self.objects.get(&link.target.id) {
                Some(target) if target.zone_change_count < link.target.generation => {
                    return Err(AttachmentError::Generation)
                }
                Some(target) if target.zone_change_count == link.target.generation => {
                    if self
                        .battlefield
                        .iter()
                        .filter(|&&id| id == target.object_id)
                        .count()
                        != 1
                    {
                        return Err(AttachmentError::Target);
                    }
                }
                _ if clean_orphans => orphaned.push((
                    self.exact_object(inst.object_id)
                        .ok_or(AttachmentError::Source)?,
                    link.target,
                )),
                _ => return Err(AttachmentError::Target),
            }
        }
        for (source, target) in orphaned {
            self.detach_exact(source, target);
        }
        Ok(())
    }

    /// Frozen represented Equipment legality. Shroud and controller changes
    /// govern targeting/activation, not an already established relationship.
    pub(crate) fn illegal_equipment_attachment(&self, source: ExactObjectRef) -> bool {
        let Some(inst) = self.objects.get(&source.id) else {
            return false;
        };
        let Some(link) = inst.attachment else {
            return false;
        };
        link.kind == AttachmentKind::Equipment
            && (self.is_creature(source.id)
                || self.attachment_target(source).is_none()
                || self
                    .get_characteristics(link.target.id)
                    .is_none_or(|chars| !chars.card_types.contains(&CardType::Creature)))
    }

    /// Explicit malformed-fixture ingress. Generations are supplied, never
    /// recovered from current objects to reconstruct missing historical data.
    #[doc(hidden)]
    pub fn set_malformed_attachment_fixture(
        &mut self,
        source: ObjectId,
        link: Option<AttachmentLink>,
    ) {
        if let Some(inst) = self.objects.get_mut(&source) {
            inst.attachment = link;
        }
        self.invalidate_characteristics_cache();
    }
}

impl super::CardInstance {
    pub fn attachment_link(&self) -> Option<AttachmentLink> {
        self.attachment
    }
}
