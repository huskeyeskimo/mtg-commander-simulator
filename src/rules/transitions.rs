//! Validated, synchronous zone transitions. Only `ExileTarget` uses this
//! kernel in production in 2B.1; other movement families remain legacy.
//! A batch is one simultaneous event, independent of the later 2A trigger
//! placement window. The batch itself is transient; occurrences own history.

use std::collections::{HashMap, HashSet};

use bincode::Options;
use serde::{Deserialize, Serialize};

use crate::card::{
    CardId, CardType, KeywordAbility, ObjectId, Subtype, TriggerCondition, ZoneType,
};
use crate::events::GameEvent;
use crate::game::{GameState, PendingTrigger, PlayerIndex, Target, TriggerContext};
use crate::mana::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExactObjectRef {
    pub id: ObjectId,
    pub generation: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZoneLocation {
    pub zone: ZoneType,
    pub player: PlayerIndex,
}

/// A raw legacy attachment ID is insufficient to prove a wearer incarnation.
/// 2D will add exact links after attachment state itself becomes versioned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttachmentLki {
    Unattached,
    UnverifiedLegacyLink,
}

/// Owned pre-event public subject facts. `card_id` refers to the stable
/// definition in this game's database; no CardDef or GameState is cloned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastKnownObject {
    pub object: ExactObjectRef,
    pub card_id: CardId,
    pub owner: PlayerIndex,
    pub controller: PlayerIndex,
    pub location: ZoneLocation,
    pub is_token: bool,
    pub card_types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub colors: Vec<Color>,
    pub keywords: Vec<KeywordAbility>,
    pub power: i32,
    pub toughness: i32,
    pub plus_counters: i32,
    pub minus_counters: i32,
    pub tapped: bool,
    pub attachment: AttachmentLki,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionRequest {
    pub object: ExactObjectRef,
    pub from: ZoneType,
    pub to: ZoneType,
    pub kind: MovementKind,
}

/// Semantic actions whose rules differ beyond endpoints. Only `Put` is
/// accepted by the first battlefield-departure adapter; later complete
/// operation families will opt in to their own kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MovementKind {
    Put,
    Destroy,
    Sacrifice { player: PlayerIndex },
    Discard { player: PlayerIndex },
    Draw { player: PlayerIndex },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommittedTransition {
    pub before: LastKnownObject,
    pub after: ExactObjectRef,
    pub destination: ZoneLocation,
    pub kind: MovementKind,
}

impl CommittedTransition {
    pub fn left_battlefield(&self) -> bool {
        self.before.location.zone == ZoneType::Battlefield
            && self.destination.zone != ZoneType::Battlefield
    }

    pub fn creature_died(&self) -> bool {
        self.before.card_types.contains(&CardType::Creature)
            && self.before.location.zone == ZoneType::Battlefield
            && self.destination.zone == ZoneType::Graveyard
    }
}

#[derive(Debug, Clone)]
pub struct CommittedTransitionBatch {
    pub group_id: u64,
    /// Every member was captured from one pre-event view and committed before
    /// any trigger is matched. Vector order is storage order, not event order.
    pub transitions: Vec<CommittedTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneTriggerContext {
    pub subject: CommittedTransition,
    pub source_before: LastKnownObject,
    pub source_was_subject: bool,
    /// Internal event-group identity. Canonical and information-set views use
    /// only its rank among currently live groups, never this raw value.
    pub group_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LiveSourceInfo {
    pub controller: PlayerIndex,
    pub card_id: CardId,
    pub tapped: bool,
    pub damage_marked: u32,
    pub summoning_sick: bool,
    pub plus_counters: i32,
    pub minus_counters: i32,
    pub temp_power_mod: i32,
    pub temp_toughness_mod: i32,
    pub card_types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub colors: Vec<Color>,
    pub keywords: Vec<KeywordAbility>,
    pub power: i32,
    pub toughness: i32,
    pub abilities_removed: bool,
    /// Current legacy `exiled_by` links, described without raw object IDs.
    /// These are present relationships, not proof of the exiled objects'
    /// original source incarnation. They matter when this source departs.
    pub linked_exiles: Vec<LinkedExileInfo>,
}

/// Observable state of an object currently exiled by a retained source. The
/// existing follow-up moves these objects to their owners' graveyards when
/// that source departs. No historical incarnation is inferred from the raw
/// legacy `exiled_by` link.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkedExileInfo {
    pub card_id: CardId,
    pub owner: PlayerIndex,
    pub controller: PlayerIndex,
    pub is_token: bool,
    pub tapped: bool,
    pub damage_marked: u32,
    pub summoning_sick: bool,
    pub plus_counters: i32,
    pub minus_counters: i32,
    pub temp_power_mod: i32,
    pub temp_toughness_mod: i32,
    pub temp_keywords: Vec<KeywordAbility>,
    pub loyalty_counters: u32,
    pub loyalty_activated_this_turn: bool,
    pub has_legacy_attachment: bool,
    pub generation_exhausted: bool,
}

fn linked_exile_info(inst: &crate::card::CardInstance) -> LinkedExileInfo {
    let mut temp_keywords = inst.temp_keywords.clone();
    temp_keywords.sort_by_key(|value| *value as u8);
    LinkedExileInfo {
        card_id: inst.card_def_id,
        owner: inst.owner,
        controller: inst.controller,
        is_token: inst.is_token,
        tapped: inst.tapped,
        damage_marked: inst.damage_marked,
        summoning_sick: inst.summoning_sick,
        plus_counters: inst.plus_counters,
        minus_counters: inst.minus_counters,
        temp_power_mod: inst.temp_power_mod,
        temp_toughness_mod: inst.temp_toughness_mod,
        temp_keywords,
        loyalty_counters: inst.loyalty_counters,
        loyalty_activated_this_turn: inst.loyalty_activated_this_turn,
        has_legacy_attachment: inst.attached_to.is_some() || !inst.attachments.is_empty(),
        generation_exhausted: inst.zone_change_count == u32::MAX,
    }
}

/// Evaluate only currently relevant trigger sources, not the battlefield.
pub(crate) fn capture_live_source(state: &GameState, id: ObjectId) -> Option<LiveSourceInfo> {
    if !state.battlefield.contains(&id) {
        return None;
    }
    let inst = state.objects.get(&id)?;
    let chars = state.get_characteristics(id)?;
    let mut card_types = chars.card_types;
    let mut subtypes = chars.subtypes;
    let mut colors = chars.colors;
    let mut keywords = chars.keywords;
    card_types.sort_by_key(|value| *value as u8);
    subtypes.sort_by(|left, right| left.0.cmp(&right.0));
    colors.sort_by_key(|value| *value as u8);
    keywords.sort_by_key(|value| *value as u8);
    let mut linked_exiles: Vec<_> = state
        .players
        .iter()
        .flat_map(|player| {
            player.exile.iter().filter_map(|&exiled_id| {
                let exiled = state.objects.get(&exiled_id)?;
                (exiled.exiled_by == Some(id)).then(|| linked_exile_info(exiled))
            })
        })
        .collect();
    linked_exiles.sort_by_cached_key(typed_bytes);
    Some(LiveSourceInfo {
        controller: chars.controller,
        card_id: inst.card_def_id,
        tapped: inst.tapped,
        damage_marked: inst.damage_marked,
        summoning_sick: inst.summoning_sick,
        plus_counters: inst.plus_counters,
        minus_counters: inst.minus_counters,
        temp_power_mod: inst.temp_power_mod,
        temp_toughness_mod: inst.temp_toughness_mod,
        card_types,
        subtypes,
        colors,
        keywords,
        power: chars.power,
        toughness: chars.toughness,
        abilities_removed: chars.abilities_removed,
        linked_exiles,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZoneSourceInfo {
    pub card_id: CardId,
    pub owner: PlayerIndex,
    pub controller_before: PlayerIndex,
    pub is_token: bool,
    pub card_types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub colors: Vec<Color>,
    pub keywords: Vec<KeywordAbility>,
    pub power: i32,
    pub toughness: i32,
    pub plus_counters: i32,
    pub minus_counters: i32,
    pub tapped: bool,
    pub attachment: AttachmentLki,
    pub live: Option<LiveSourceInfo>,
}

/// The same source/subject/group description is used by canonical mandatory
/// actions and information sets. Runtime identities remain in TriggerContext.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZoneOccurrenceInfo {
    pub source_card_id: CardId,
    pub controller: PlayerIndex,
    pub ability_index: usize,
    /// Typed ordered effect, including child order and multiplicity.
    pub effect_encoding: Vec<u8>,
    pub source: Option<ZoneSourceInfo>,
    pub subject: Option<ZoneSubjectInfo>,
    pub group_rank: Option<usize>,
    /// Local equivalence-class rank of this exact source among live event
    /// occurrences; equal ranks mean the same source incarnation.
    pub source_class_rank: Option<usize>,
    /// Relation to exact sources represented among currently retained zone
    /// occurrences. Independent of source sharing and simultaneous event ID.
    pub subject_source_relation: Option<SubjectSourceRelation>,
    /// Only for legacy occurrences without owned transition context.
    pub legacy_source_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubjectSourceRelation {
    SelfSource,
    OtherRetainedSource(usize),
    External,
}

pub fn public_occurrence_info(
    context: &TriggerContext,
    ability_index: usize,
    controller: PlayerIndex,
    source_info: Option<ZoneSourceInfo>,
    subject_generation: Option<u32>,
    group_rank: Option<usize>,
    source_class_rank: Option<usize>,
    subject_source_rank: Option<usize>,
    legacy_source_index: Option<usize>,
) -> ZoneOccurrenceInfo {
    let zone = context.zone_transition.as_ref();
    ZoneOccurrenceInfo {
        source_card_id: context.source_card_id,
        controller,
        ability_index,
        effect_encoding: typed_bytes(&context.effect),
        source: if zone.is_some() { source_info } else { None },
        subject: zone.map(|ctx| ctx.public_info(subject_generation)),
        group_rank: zone.and(group_rank),
        source_class_rank: zone.and(source_class_rank),
        subject_source_relation: zone.map(|ctx| {
            if ctx.source_was_subject {
                SubjectSourceRelation::SelfSource
            } else if let Some(rank) = subject_source_rank {
                SubjectSourceRelation::OtherRetainedSource(rank)
            } else {
                SubjectSourceRelation::External
            }
        }),
        legacy_source_index: if zone.is_none() {
            legacy_source_index
        } else {
            None
        },
    }
}

/// Public, allocation-independent description for ordering and information
/// sets. Numeric runtime IDs/generations remain only in the owned rules data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZoneSubjectInfo {
    pub card_id: CardId,
    pub owner: PlayerIndex,
    pub controller_before: PlayerIndex,
    pub from: ZoneType,
    pub to: ZoneType,
    pub is_token: bool,
    pub card_types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub colors: Vec<Color>,
    pub keywords: Vec<KeywordAbility>,
    pub power: i32,
    pub toughness: i32,
    pub plus_counters: i32,
    pub minus_counters: i32,
    pub tapped: bool,
    pub source_was_subject: bool,
    /// True only when the viewing player can currently confirm that this
    /// exact post-transition incarnation is in a visible zone. False also
    /// covers private zones; it does not assert that the object ceased to exist.
    pub same_incarnation_now: bool,
}

impl ZoneTriggerContext {
    pub fn source_public_info(&self, live: Option<&LiveSourceInfo>) -> ZoneSourceInfo {
        let before = &self.source_before;
        let mut card_types = before.card_types.clone();
        let mut subtypes = before.subtypes.clone();
        let mut colors = before.colors.clone();
        let mut keywords = before.keywords.clone();
        card_types.sort_by_key(|value| *value as u8);
        subtypes.sort_by(|left, right| left.0.cmp(&right.0));
        colors.sort_by_key(|value| *value as u8);
        keywords.sort_by_key(|value| *value as u8);
        ZoneSourceInfo {
            card_id: before.card_id,
            owner: before.owner,
            controller_before: before.controller,
            is_token: before.is_token,
            card_types,
            subtypes,
            colors,
            keywords,
            power: before.power,
            toughness: before.toughness,
            plus_counters: before.plus_counters,
            minus_counters: before.minus_counters,
            tapped: before.tapped,
            attachment: before.attachment,
            live: live.cloned(),
        }
    }

    pub fn public_info(&self, current_generation: Option<u32>) -> ZoneSubjectInfo {
        let before = &self.subject.before;
        let mut card_types = before.card_types.clone();
        let mut subtypes = before.subtypes.clone();
        let mut colors = before.colors.clone();
        let mut keywords = before.keywords.clone();
        card_types.sort_by_key(|value| *value as u8);
        subtypes.sort_by(|left, right| left.0.cmp(&right.0));
        colors.sort_by_key(|value| *value as u8);
        keywords.sort_by_key(|value| *value as u8);
        ZoneSubjectInfo {
            card_id: before.card_id,
            owner: before.owner,
            controller_before: before.controller,
            from: before.location.zone,
            to: self.subject.destination.zone,
            is_token: before.is_token,
            card_types,
            subtypes,
            colors,
            keywords,
            power: before.power,
            toughness: before.toughness,
            plus_counters: before.plus_counters,
            minus_counters: before.minus_counters,
            tapped: before.tapped,
            source_was_subject: self.source_was_subject,
            same_incarnation_now: current_generation == Some(self.subject.after.generation),
        }
    }
}

/// Local ordinals encode the partition of currently live occurrences without
/// exposing the allocator's raw event IDs to strategy/canonical state.
pub fn live_group_ranks(
    pending: &[PendingTrigger],
    stack: &[crate::game::StackEntry],
) -> HashMap<u64, usize> {
    let mut groups: Vec<u64> = pending
        .iter()
        .filter_map(|trigger| {
            trigger
                .context
                .zone_transition
                .as_ref()
                .map(|ctx| ctx.group_id)
        })
        .collect();
    groups.extend(stack.iter().filter_map(|entry| match &entry.source {
        crate::game::StackSource::TriggeredAbility { context, .. } => {
            context.zone_transition.as_ref().map(|ctx| ctx.group_id)
        }
        _ => None,
    }));
    groups.sort_unstable();
    groups.dedup();
    groups
        .into_iter()
        .enumerate()
        .map(|(rank, id)| (id, rank))
        .collect()
}

/// Typed, version-local deterministic encoding. Fixed-width big-endian integers,
/// enum tags, collection lengths, and ordered fields are supplied by serde's
/// data model and bincode; Debug output and runtime identities are never keys.
pub(crate) fn typed_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_big_endian()
        .serialize(value)
        .expect("retained occurrence encoding must serialize")
}

#[derive(Debug, Clone, Default)]
pub struct NormalizationStats {
    pub normalization_calls: usize,
    pub search_nodes: usize,
    pub refinement_rounds: usize,
    pub tied_cell_sizes: Vec<usize>,
    pub component_sizes: Vec<usize>,
    pub encoded_candidates: usize,
    pub elapsed_nanos: u128,
}

/// Ephemeral result for ONE immutable decision. The encoding and witness are
/// inseparable: all action and observation coordinates use this same map.
/// No field is persisted in GameState or a save file.
#[derive(Debug, Clone)]
pub struct RetainedNormalization {
    pub encoding: Vec<u8>,
    pub source_ranks: HashMap<ExactObjectRef, usize>,
    pub group_ranks: HashMap<u64, usize>,
    pub stats: NormalizationStats,
    /// Captured once, labeled with this witness, indexed only for lookup in
    /// this immutable decision. Storage positions never enter the encoding.
    pub pending_occurrences: Vec<Option<ZoneOccurrenceInfo>>,
    pub stack_occurrences: Vec<Option<ZoneOccurrenceInfo>>,
}

#[derive(Clone)]
struct ProjectedOccurrence {
    source: usize,
    related_source: Option<usize>,
    info: ZoneOccurrenceInfo,
    /// Position in the complete stack, including non-zone entries. None is
    /// pending multiset membership; pending storage order is not an anchor.
    stack_position: Option<usize>,
    pending_slot: Option<usize>,
}

struct RetainedProjection {
    sources: Vec<ExactObjectRef>,
    occurrences: Vec<ProjectedOccurrence>,
    groups: HashMap<u64, usize>,
    stack_len: usize,
    pending_len: usize,
}

#[derive(Serialize)]
struct EncodedOccurrence {
    stack_position: Option<usize>,
    occurrence: ZoneOccurrenceInfo,
}

impl RetainedProjection {
    fn new(
        pending: &[PendingTrigger],
        stack: &[crate::game::StackEntry],
        source_info: impl Fn(&TriggerContext) -> ZoneSourceInfo,
        subject_generation: impl Fn(ObjectId) -> Option<u32>,
    ) -> Self {
        let groups = live_group_ranks(pending, stack);
        let mut sources = Vec::new();
        let mut indices = HashMap::new();
        for context in pending
            .iter()
            .map(|t| &t.context)
            .chain(stack.iter().filter_map(|entry| match &entry.source {
                crate::game::StackSource::TriggeredAbility { context, .. } => {
                    Some(context.as_ref())
                }
                _ => None,
            }))
        {
            if let Some(zone) = &context.zone_transition {
                let exact = zone.source_before.object;
                if !indices.contains_key(&exact) {
                    indices.insert(exact, sources.len());
                    sources.push(exact);
                }
            }
        }
        let mut occurrences = Vec::new();
        let mut record =
            |context: &TriggerContext, controller, ability_index, stack_position, pending_slot| {
                let Some(zone) = &context.zone_transition else {
                    return;
                };
                let source = indices[&zone.source_before.object];
                let subject = zone.subject.before.object;
                let related_source = (subject != zone.source_before.object)
                    .then(|| indices.get(&subject).copied())
                    .flatten();
                let info = public_occurrence_info(
                    context,
                    ability_index,
                    controller,
                    Some(source_info(context)),
                    subject_generation(zone.subject.after.id),
                    groups.get(&zone.group_id).copied(),
                    None,
                    // Zero is only a placeholder. Every candidate replaces it
                    // using its ONE joint source/subject labeling.
                    related_source.map(|_| 0),
                    None,
                );
                occurrences.push(ProjectedOccurrence {
                    source,
                    related_source,
                    info,
                    stack_position,
                    pending_slot,
                });
            };
        for (slot, trigger) in pending.iter().enumerate() {
            record(
                &trigger.context,
                trigger.controller,
                trigger.ability_index,
                None,
                Some(slot),
            );
        }
        for (position, entry) in stack.iter().enumerate() {
            if let crate::game::StackSource::TriggeredAbility {
                context,
                ability_index,
                ..
            } = &entry.source
            {
                record(
                    context,
                    entry.controller,
                    *ability_index,
                    Some(position),
                    None,
                );
            }
        }
        Self {
            sources,
            occurrences,
            groups,
            stack_len: stack.len(),
            pending_len: pending.len(),
        }
    }

    /// Only retained-source subject edges connect components. Event ranks and
    /// complete stack positions remain globally fixed anchors INSIDE every
    /// record, so shared groups are preserved without inventing graph edges.
    fn components(&self) -> Vec<Vec<usize>> {
        let mut adjacency = vec![Vec::new(); self.sources.len()];
        for record in &self.occurrences {
            if let Some(target) = record.related_source {
                adjacency[record.source].push(target);
                adjacency[target].push(record.source);
            }
        }
        let mut seen = vec![false; self.sources.len()];
        let mut components = Vec::new();
        for root in 0..self.sources.len() {
            if seen[root] {
                continue;
            }
            let mut component = Vec::new();
            let mut frontier = vec![root];
            seen[root] = true;
            while let Some(vertex) = frontier.pop() {
                component.push(vertex);
                for &neighbor in &adjacency[vertex] {
                    if !seen[neighbor] {
                        seen[neighbor] = true;
                        frontier.push(neighbor);
                    }
                }
            }
            components.push(component);
        }
        components
    }

    fn record_bytes(&self, record: &ProjectedOccurrence, ranks: &[usize]) -> Vec<u8> {
        let mut occurrence = record.info.clone();
        occurrence.source_class_rank = Some(ranks[record.source]);
        if let Some(target) = record.related_source {
            occurrence.subject_source_relation =
                Some(SubjectSourceRelation::OtherRetainedSource(ranks[target]));
        }
        typed_bytes(&EncodedOccurrence {
            stack_position: record.stack_position,
            occurrence,
        })
    }

    fn component_bytes(&self, component: &[usize], ordering: &[usize]) -> Vec<u8> {
        let mut ranks = vec![usize::MAX; self.sources.len()];
        for (rank, &vertex) in ordering.iter().enumerate() {
            ranks[vertex] = rank;
        }
        let mut records: Vec<_> = self
            .occurrences
            .iter()
            .filter(|r| ranks[r.source] != usize::MAX)
            .map(|r| {
                assert!(
                    r.related_source
                        .is_none_or(|target| ranks[target] != usize::MAX),
                    "a retained relationship cannot cross a decomposed component"
                );
                self.record_bytes(r, &ranks)
            })
            .collect();
        records.sort(); // ONLY the occurrence multiset, stack position is encoded.
        typed_bytes(&(component.len(), records))
    }

    /// Stable semantic partition. Refinement is a search aid, never proof that
    /// tied vertices are interchangeable. Incoming and outgoing labeled edges
    /// both contribute; historical facts remain per occurrence.
    fn refine(&self, marks: &[Option<usize>], stats: &mut NormalizationStats) -> Vec<usize> {
        let mut colors = vec![0; self.sources.len()];
        loop {
            stats.refinement_rounds += 1;
            let mut outgoing = vec![Vec::<Vec<u8>>::new(); self.sources.len()];
            let mut incoming = outgoing.clone();
            for record in &self.occurrences {
                let descriptor = typed_bytes(&(record.stack_position, &record.info));
                outgoing[record.source].push(typed_bytes(&(
                    &descriptor,
                    record.related_source.map(|target| colors[target]),
                )));
                if let Some(target) = record.related_source {
                    incoming[target].push(typed_bytes(&(&descriptor, colors[record.source])));
                }
            }
            let mut signatures: Vec<_> = (0..self.sources.len())
                .map(|vertex| {
                    outgoing[vertex].sort();
                    incoming[vertex].sort();
                    (
                        typed_bytes(&(
                            colors[vertex],
                            marks[vertex],
                            &outgoing[vertex],
                            &incoming[vertex],
                        )),
                        vertex,
                    )
                })
                .collect();
            signatures.sort_by(|left, right| left.0.cmp(&right.0));
            let mut next = vec![0; colors.len()];
            let mut color = 0;
            for index in 0..signatures.len() {
                if index > 0 && signatures[index - 1].0 != signatures[index].0 {
                    color += 1;
                }
                next[signatures[index].1] = color;
            }
            // Including the prior color prevents merging. Equality of the
            // partition (not a guessed round limit) establishes stability.
            let stable = (0..colors.len()).all(|a| {
                (0..colors.len()).all(|b| (colors[a] == colors[b]) == (next[a] == next[b]))
            });
            colors = next;
            if stable {
                return colors;
            }
        }
    }

    fn assemble(
        &self,
        mut components: Vec<(Vec<u8>, Vec<usize>)>,
        mut stats: NormalizationStats,
    ) -> RetainedNormalization {
        // Component-local coordinates are normative. The exhaustive oracle
        // localizes each GLOBAL joint candidate the same way before sorting
        // complete component encodings. Thus decomposition computes exactly
        // the oracle minimum, without independent maps breaking incidence.
        components.sort_by(|left, right| left.0.cmp(&right.0));
        let mut source_ranks = HashMap::new();
        let mut offset = 0;
        for (_, ordering) in &components {
            for (local, &vertex) in ordering.iter().enumerate() {
                source_ranks.insert(self.sources[vertex], offset + local);
            }
            offset += ordering.len();
        }
        assert_eq!(source_ranks.len(), self.sources.len());
        stats.normalization_calls = 1;
        let encoding = if self.sources.is_empty() {
            Vec::new()
        } else {
            // Each component is already a self-delimiting typed structure.
            // Do not add a byte-vector length before it: that would change
            // the normative ordering of complete component encodings.
            let mut bytes =
                typed_bytes(&(1u8, self.stack_len, self.groups.len(), components.len()));
            for (component, _) in &components {
                bytes.extend_from_slice(component);
            }
            bytes
        };
        let mut pending_occurrences = vec![None; self.pending_len];
        let mut stack_occurrences = vec![None; self.stack_len];
        for record in &self.occurrences {
            let mut occurrence = record.info.clone();
            occurrence.source_class_rank = Some(source_ranks[&self.sources[record.source]]);
            if let Some(target) = record.related_source {
                occurrence.subject_source_relation = Some(
                    SubjectSourceRelation::OtherRetainedSource(source_ranks[&self.sources[target]]),
                );
            }
            if let Some(slot) = record.pending_slot {
                pending_occurrences[slot] = Some(occurrence);
            } else {
                stack_occurrences[record.stack_position.unwrap()] = Some(occurrence);
            }
        }
        RetainedNormalization {
            encoding,
            source_ranks,
            group_ranks: self.groups.clone(),
            stats,
            pending_occurrences,
            stack_occurrences,
        }
    }
}

#[cfg(test)]
thread_local! { pub(crate) static NORMALIZATION_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

/// Exact retained-occurrence normalization. No raw-ID tie break, approximate
/// hash, size limit, timeout or fallback. Worst-case search remains factorial.
pub fn normalize_retained(
    pending: &[PendingTrigger],
    stack: &[crate::game::StackEntry],
    source_info: impl Fn(&TriggerContext) -> ZoneSourceInfo,
    subject_generation: impl Fn(ObjectId) -> Option<u32>,
) -> RetainedNormalization {
    #[cfg(test)]
    NORMALIZATION_CALLS.with(|calls| calls.set(calls.get() + 1));
    let started = std::time::Instant::now();
    let projection = RetainedProjection::new(pending, stack, source_info, subject_generation);
    let mut stats = NormalizationStats::default();
    let colors = projection.refine(&vec![None; projection.sources.len()], &mut stats);
    let components = projection.components();
    stats.component_sizes = components.iter().map(Vec::len).collect();
    let mut global_cells = std::collections::BTreeMap::<usize, usize>::new();
    for &color in &colors {
        *global_cells.entry(color).or_default() += 1;
    }
    stats.tied_cell_sizes = global_cells
        .into_values()
        .filter(|&size| size > 1)
        .collect();
    let mut normalized = Vec::new();
    for component in components {
        let mut cells = std::collections::BTreeMap::<usize, Vec<usize>>::new();
        for &vertex in &component {
            cells.entry(colors[vertex]).or_default().push(vertex);
        }
        let cells: Vec<_> = cells.into_values().collect();
        let identity: Vec<_> = cells.iter().flatten().copied().collect();
        let identity_encoding = projection.component_bytes(&component, &identity);
        let mut interchangeable = HashSet::new();
        // This is a COMPLETE-encoding automorphism proof, not equality of
        // refinement colors. A verified transposition fixes all other vertices
        // and therefore fixes every previously individualized vertex.
        for cell in &cells {
            for (index, &left) in cell.iter().enumerate() {
                for &right in &cell[index + 1..] {
                    let mut swapped = identity.clone();
                    let a = swapped.iter().position(|&v| v == left).unwrap();
                    let b = swapped.iter().position(|&v| v == right).unwrap();
                    swapped.swap(a, b);
                    stats.encoded_candidates += 1;
                    if projection.component_bytes(&component, &swapped) == identity_encoding {
                        interchangeable.insert((left.min(right), left.max(right)));
                    }
                }
            }
        }
        struct Search<'a> {
            projection: &'a RetainedProjection,
            component: &'a [usize],
            cells: &'a [Vec<usize>],
            interchangeable: &'a HashSet<(usize, usize)>,
            best: Option<(Vec<u8>, Vec<usize>)>,
        }
        impl Search<'_> {
            fn visit(
                &mut self,
                ordering: &mut Vec<usize>,
                marks: &mut [Option<usize>],
                stats: &mut NormalizationStats,
            ) {
                stats.search_nodes += 1;
                let refined = self.projection.refine(marks, stats);
                if ordering.len() == self.component.len() {
                    stats.encoded_candidates += 1;
                    let encoding = self.projection.component_bytes(self.component, ordering);
                    if self.best.as_ref().is_none_or(|(best, _)| encoding < *best) {
                        self.best = Some((encoding, ordering.clone()));
                    }
                    return;
                }
                // Stable initial cells define the normative candidate domain.
                // New refinement orders alternatives but does NOT silently
                // remove initial-cell permutations from the oracle domain.
                let cell = self
                    .cells
                    .iter()
                    .find(|cell| cell.iter().any(|&v| marks[v].is_none()))
                    .unwrap();
                let mut candidates: Vec<_> = cell
                    .iter()
                    .copied()
                    .filter(|&v| marks[v].is_none())
                    .collect();
                candidates.sort_by_key(|&vertex| refined[vertex]);
                let mut searched = Vec::new();
                for vertex in candidates {
                    if searched.iter().any(|&other| {
                        self.interchangeable
                            .contains(&(vertex.min(other), vertex.max(other)))
                    }) {
                        continue;
                    }
                    searched.push(vertex);
                    marks[vertex] = Some(ordering.len());
                    ordering.push(vertex);
                    self.visit(ordering, marks, stats);
                    ordering.pop();
                    marks[vertex] = None;
                }
            }
        }
        let mut search = Search {
            projection: &projection,
            component: &component,
            cells: &cells,
            interchangeable: &interchangeable,
            best: None,
        };
        search.visit(
            &mut Vec::new(),
            &mut vec![None; projection.sources.len()],
            &mut stats,
        );
        normalized.push(
            search
                .best
                .expect("finite exact labeling search must have a candidate"),
        );
    }
    let mut result = projection.assemble(normalized, stats);
    result.stats.elapsed_nanos = started.elapsed().as_nanos();
    result
}

/// Reference oracle, deliberately independent of the production search:
/// enumerate the Cartesian product of permutations of ALL tied cells, then
/// encode each complete joint candidate using the normative local coordinates.
/// This diagnostic API is only for bounded fixtures; no production caller uses it.
#[doc(hidden)]
pub fn exhaustive_retained_oracle(
    pending: &[PendingTrigger],
    stack: &[crate::game::StackEntry],
    source_info: impl Fn(&TriggerContext) -> ZoneSourceInfo,
    subject_generation: impl Fn(ObjectId) -> Option<u32>,
) -> RetainedNormalization {
    let started = std::time::Instant::now();
    let projection = RetainedProjection::new(pending, stack, source_info, subject_generation);
    let mut stats = NormalizationStats::default();
    let colors = projection.refine(&vec![None; projection.sources.len()], &mut stats);
    let mut cells = std::collections::BTreeMap::<usize, Vec<usize>>::new();
    for (vertex, color) in colors.into_iter().enumerate() {
        cells.entry(color).or_default().push(vertex);
    }
    let cells: Vec<_> = cells.into_values().collect();
    stats.tied_cell_sizes = cells
        .iter()
        .filter(|cell| cell.len() > 1)
        .map(Vec::len)
        .collect();
    let components = projection.components();
    stats.component_sizes = components.iter().map(Vec::len).collect();
    fn enumerate(cells: &[Vec<usize>], label: &mut Vec<usize>, f: &mut impl FnMut(&[usize])) {
        if cells.is_empty() {
            f(label);
            return;
        }
        fn permutations(
            left: &mut Vec<usize>,
            prefix: &mut Vec<usize>,
            f: &mut impl FnMut(&[usize]),
        ) {
            if left.is_empty() {
                f(prefix);
                return;
            }
            for index in 0..left.len() {
                let vertex = left.remove(index);
                prefix.push(vertex);
                permutations(left, prefix, f);
                prefix.pop();
                left.insert(index, vertex);
            }
        }
        permutations(&mut cells[0].clone(), &mut Vec::new(), &mut |permutation| {
            let old_len = label.len();
            label.extend_from_slice(permutation);
            enumerate(&cells[1..], label, f);
            label.truncate(old_len);
        });
    }
    let mut best: Option<RetainedNormalization> = None;
    enumerate(&cells, &mut Vec::new(), &mut |ordering| {
        stats.search_nodes += 1;
        stats.encoded_candidates += 1;
        let mut candidate_components = Vec::new();
        for component in &components {
            let local: Vec<_> = ordering
                .iter()
                .copied()
                .filter(|v| component.contains(v))
                .collect();
            candidate_components.push((projection.component_bytes(component, &local), local));
        }
        let candidate = projection.assemble(candidate_components, NormalizationStats::default());
        if best
            .as_ref()
            .is_none_or(|best| candidate.encoding < best.encoding)
        {
            best = Some(candidate);
        }
    });
    stats.elapsed_nanos = started.elapsed().as_nanos();
    stats.normalization_calls = 1;
    let mut result = best.expect("oracle must enumerate an empty or nonempty labeling");
    result.stats = stats;
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    MissingDatabase,
    MissingObject(ObjectId),
    WrongZone(ObjectId),
    StaleIncarnation(ObjectId),
    DuplicateSubject(ObjectId),
    MissingDefinition(CardId),
    GenerationExhausted(ObjectId),
    InvalidPlayer(ObjectId),
    InvalidLinkedState(ObjectId),
    CapacityExhausted,
    GroupIdExhausted,
    UnsupportedPath,
}

#[derive(Debug, Clone)]
struct Observer {
    source: ExactObjectRef,
    before: LastKnownObject,
    controller: PlayerIndex,
    abilities: Vec<(usize, crate::card::TriggeredAbility)>,
}

#[derive(Debug, Clone, Copy)]
struct LinkedExileFollowup {
    object: ExactObjectRef,
    source: ExactObjectRef,
    player: PlayerIndex,
}

fn actual_destination(state: &GameState, id: ObjectId, to: ZoneType) -> ZoneType {
    if state.is_commander_format()
        && state.is_commander(id)
        && matches!(to, ZoneType::Graveyard | ZoneType::Exile)
    {
        ZoneType::Command
    } else {
        to
    }
}

fn validate(
    state: &GameState,
    requests: &[TransitionRequest],
) -> Result<Vec<LinkedExileFollowup>, TransitionError> {
    let db = state
        .card_db
        .as_ref()
        .ok_or(TransitionError::MissingDatabase)?;
    let mut seen = HashSet::new();
    for request in requests {
        let id = request.object.id;
        if !seen.insert(id) {
            return Err(TransitionError::DuplicateSubject(id));
        }
        let inst = state
            .objects
            .get(&id)
            .ok_or(TransitionError::MissingObject(id))?;
        if inst.zone_change_count != request.object.generation {
            return Err(TransitionError::StaleIncarnation(id));
        }
        // This first adapter supports battlefield departures only. Other
        // locations enter the same data model in later complete families.
        if request.from != ZoneType::Battlefield
            || matches!(request.to, ZoneType::Battlefield | ZoneType::Stack)
        {
            return Err(TransitionError::UnsupportedPath);
        }
        if request.kind != MovementKind::Put {
            return Err(TransitionError::UnsupportedPath);
        }
        if !state.battlefield.contains(&id) {
            return Err(TransitionError::WrongZone(id));
        }
        if inst.zone_change_count == u32::MAX {
            return Err(TransitionError::GenerationExhausted(id));
        }
        if inst.owner >= state.players.len() || inst.controller >= state.players.len() {
            return Err(TransitionError::InvalidPlayer(id));
        }
        if db.get(inst.card_def_id).is_none() {
            return Err(TransitionError::MissingDefinition(inst.card_def_id));
        }
    }
    // Linked exiles are still a legacy secondary movement (2B.5). Capture
    // exactly those already in exile before the primary event; a batch member
    // newly entering exile must never join this follow-up plan.
    let subjects: std::collections::HashMap<_, _> =
        requests.iter().map(|r| (r.object.id, r.object)).collect();
    let mut linked_seen = HashSet::new();
    let mut followups = Vec::new();
    for (player_index, player) in state.players.iter().enumerate() {
        for &id in &player.exile {
            let Some(inst) = state.objects.get(&id) else {
                return Err(TransitionError::InvalidLinkedState(id));
            };
            let Some(source) = inst
                .exiled_by
                .and_then(|source| subjects.get(&source).copied())
            else {
                continue;
            };
            if !linked_seen.insert(id)
                || subjects.contains_key(&id)
                || inst.owner != player_index
                || inst.owner >= state.players.len()
                || inst.zone_change_count == u32::MAX
            {
                return Err(TransitionError::InvalidLinkedState(id));
            }
            followups.push(LinkedExileFollowup {
                object: ExactObjectRef {
                    id,
                    generation: inst.zone_change_count,
                },
                source,
                player: player_index,
            });
        }
    }
    state
        .pending_events
        .len()
        .checked_add(requests.len())
        .and_then(|count| count.checked_add(followups.len()))
        .ok_or(TransitionError::CapacityExhausted)?;
    let mut zone_additions: HashMap<(PlayerIndex, ZoneType), usize> = HashMap::new();
    for request in requests {
        let owner = state.objects[&request.object.id].owner;
        let destination = actual_destination(state, request.object.id, request.to);
        let count = zone_additions.entry((owner, destination)).or_default();
        *count = count
            .checked_add(1)
            .ok_or(TransitionError::CapacityExhausted)?;
    }
    for linked in &followups {
        let owner = state.objects[&linked.object.id].owner;
        let count = zone_additions
            .entry((owner, ZoneType::Graveyard))
            .or_default();
        *count = count
            .checked_add(1)
            .ok_or(TransitionError::CapacityExhausted)?;
    }
    for ((owner, zone), count) in zone_additions {
        let player = &state.players[owner];
        let occupied = match zone {
            ZoneType::Library => player.library.len(),
            ZoneType::Hand => player.hand.len(),
            ZoneType::Graveyard => player.graveyard.len(),
            ZoneType::Exile => player.exile.len(),
            ZoneType::Command => player.command_zone.len(),
            ZoneType::Stack | ZoneType::Battlefield => unreachable!("unsupported destination"),
        };
        occupied
            .checked_add(count)
            .ok_or(TransitionError::CapacityExhausted)?;
    }
    Ok(followups)
}

fn capture_subject(state: &GameState, id: ObjectId) -> LastKnownObject {
    let inst = &state.objects[&id];
    let chars = state
        .get_characteristics(id)
        .expect("validated battlefield definition");
    LastKnownObject {
        object: ExactObjectRef {
            id,
            generation: inst.zone_change_count,
        },
        card_id: inst.card_def_id,
        owner: inst.owner,
        controller: chars.controller,
        location: ZoneLocation {
            zone: ZoneType::Battlefield,
            player: chars.controller,
        },
        is_token: inst.is_token,
        card_types: chars.card_types,
        subtypes: chars.subtypes,
        colors: chars.colors,
        keywords: chars.keywords,
        power: chars.power,
        toughness: chars.toughness,
        plus_counters: inst.plus_counters,
        minus_counters: inst.minus_counters,
        tapped: inst.tapped,
        attachment: if inst.attached_to.is_some() || !inst.attachments.is_empty() {
            AttachmentLki::UnverifiedLegacyLink
        } else {
            AttachmentLki::Unattached
        },
    }
}

fn capture_observers(state: &GameState) -> Vec<Observer> {
    state
        .battlefield
        .iter()
        .filter_map(|&id| {
            let inst = state.objects.get(&id)?;
            let def = state.card_db().get(inst.card_def_id)?;
            let abilities: Vec<_> = def
                .triggered_abilities
                .iter()
                .enumerate()
                .filter(|(_, ability)| {
                    matches!(
                        ability.trigger,
                        TriggerCondition::LeavesBattlefield
                            | TriggerCondition::Dies
                            | TriggerCondition::ACreatureDies
                            | TriggerCondition::ACreatureYouControlDies
                            | TriggerCondition::APermanentLeaves
                    )
                })
                .map(|(index, ability)| (index, ability.clone()))
                .collect();
            if abilities.is_empty() {
                return None;
            }
            let chars = state.get_characteristics(id)?;
            if chars.abilities_removed {
                return None;
            }
            Some(Observer {
                source: ExactObjectRef {
                    id,
                    generation: inst.zone_change_count,
                },
                before: capture_subject(state, id),
                controller: chars.controller,
                abilities,
            })
        })
        .collect()
}

fn collect_occurrences(
    observers: &[Observer],
    batch: &CommittedTransitionBatch,
) -> Vec<PendingTrigger> {
    let mut found = Vec::new();
    for observer in observers {
        for (ability_index, ability) in &observer.abilities {
            for subject in &batch.transitions {
                let same = observer.source == subject.before.object;
                let matches = match ability.trigger {
                    TriggerCondition::LeavesBattlefield => same && subject.left_battlefield(),
                    TriggerCondition::Dies => same && subject.creature_died(),
                    TriggerCondition::ACreatureDies => subject.creature_died(),
                    TriggerCondition::ACreatureYouControlDies => {
                        subject.creature_died() && subject.before.controller == observer.controller
                    }
                    TriggerCondition::APermanentLeaves => subject.left_battlefield(),
                    _ => false,
                };
                if matches {
                    found.push(PendingTrigger {
                        source_id: observer.source.id,
                        ability_index: *ability_index,
                        controller: observer.controller,
                        targets: Vec::<Target>::new(),
                        context: TriggerContext {
                            source_card_id: observer.before.card_id,
                            source_generation: observer.source.generation,
                            effect: ability.effect.clone(),
                            cast_spell: None,
                            zone_transition: Some(ZoneTriggerContext {
                                subject: subject.clone(),
                                source_before: observer.before.clone(),
                                source_was_subject: same,
                                group_id: batch.group_id,
                            }),
                        },
                    });
                }
            }
        }
    }
    found
}

/// Commit one simultaneous departure event. All validation and pre-event
/// capture finish before any mutation. Occurrences enter 2A's pending queue;
/// this function never places triggers, grants priority, or advances time.
pub fn transition_batch(
    state: &mut GameState,
    requests: &[TransitionRequest],
) -> Result<CommittedTransitionBatch, TransitionError> {
    let followups = validate(state, requests)?;
    let group_id = state.next_zone_event_group_id;
    let next_group_id = group_id
        .checked_add(1)
        .ok_or(TransitionError::GroupIdExhausted)?;
    let subjects: Vec<_> = requests
        .iter()
        .map(|r| capture_subject(state, r.object.id))
        .collect();
    let observers = capture_observers(state);
    let destinations: Vec<_> = requests
        .iter()
        .map(|r| actual_destination(state, r.object.id, r.to))
        .collect();

    for (request, destination) in requests.iter().zip(&destinations) {
        state.move_object_for_transition(request.object.id, request.from, *destination);
    }
    state.refresh_continuous_effects();
    state.refresh_replacement_effects();
    let transitions = subjects
        .into_iter()
        .zip(destinations.iter())
        .zip(requests.iter())
        .map(|((before, &to), request)| {
            let id = before.object.id;
            CommittedTransition {
                after: ExactObjectRef {
                    id,
                    generation: before.object.generation + 1,
                },
                destination: ZoneLocation {
                    zone: to,
                    player: before.owner,
                },
                before,
                kind: request.kind,
            }
        })
        .collect();
    let batch = CommittedTransitionBatch {
        group_id,
        transitions,
    };
    state.next_zone_event_group_id = next_group_id;

    // The complete post-event view exists before any notification/matching.
    for transition in &batch.transitions {
        state.emit_event(GameEvent::ZoneChange {
            object: transition.before.object.id,
            from: crate::events::Zone::from(transition.before.location.zone),
            to: crate::events::Zone::from(transition.destination.zone),
        });
    }
    state
        .pending_triggers
        .extend(collect_occurrences(&observers, &batch));

    // Temporary 2B.1 bridge: purge only after event and occurrence ownership.
    // Correct token residence/cessation at SBA time remains 2C.
    for transition in &batch.transitions {
        state.purge_transitioned_token(transition.before.object.id, transition.destination.zone);
    }
    // Existing linked-exile movement is a separate legacy follow-up event.
    // It must not run between members of the primary simultaneous batch.
    let linked: Vec<_> = followups
        .iter()
        .map(|item| {
            debug_assert_eq!(
                state.objects[&item.object.id].zone_change_count,
                item.object.generation
            );
            debug_assert!(requests.iter().any(|request| request.object == item.source));
            (item.object.id, item.player)
        })
        .collect();
    state.move_prevalidated_linked_exiles(&linked);
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::card::{CardDef, Effect, TargetSpec};
    use crate::game::CardDatabase;

    #[test]
    fn exile_effect_does_not_bind_to_blinked_target_in_same_resolution() {
        let mut db = CardDatabase::new();
        db.insert(CardDef {
            id: 992_001,
            name: "Incarnation".into(),
            card_types: vec![CardType::Creature],
            power: Some(2),
            toughness: Some(2),
            ..Default::default()
        });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        let id = state.create_card_in_zone(992_001, 0, ZoneType::Battlefield);
        let original = state.objects[&id].zone_change_count;
        state.move_object(id, ZoneType::Battlefield, ZoneType::Exile);
        state.move_object(id, ZoneType::Exile, ZoneType::Battlefield);
        state.pending_events.clear();
        super::super::effects::resolve_effect(
            &mut state,
            &Effect::ExileTarget {
                target: TargetSpec::AnyCreature,
            },
            0,
            &[Target::Object(id)],
            &[Some(original)],
            None,
        );
        assert!(state.battlefield.contains(&id));
        assert!(!state.players[0].exile.contains(&id));
        assert!(state.pending_events.is_empty());
        assert!(state.pending_triggers.is_empty());
    }
}
