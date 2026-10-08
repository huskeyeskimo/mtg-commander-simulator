use mtg_gto::{
    action::{canonical, legal_actions},
    card::{CardDef, CardType, Subtype, ZoneType},
    game::{CardDatabase, GameState, Phase},
    info_set::{
        BucketedAbstraction, CardAwareBucketedAbstraction, InfoSetAbstraction, InformationSet,
    },
    layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification},
    mana::ManaCost,
    public_projection::{
        normalize_graph, EdgeKind, JointPublicNormalization, PublicGraph, VertexKind,
    },
};
use std::{collections::HashSet, sync::Arc};
fn graph_permutation(graph: &PublicGraph, permutation: &[usize]) -> PublicGraph {
    let mut result = PublicGraph::default();
    result.vertices = graph.vertices.clone();
    for (old, &new) in permutation.iter().enumerate() {
        result.vertices[new] = graph.vertices[old].clone();
    }
    result.edges = graph
        .edges
        .iter()
        .rev()
        .map(|edge| {
            let mut edge = edge.clone();
            edge.source = permutation[edge.source];
            edge.target = permutation[edge.target];
            edge
        })
        .collect();
    result
}
fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn visit(values: &mut [usize], slot: usize, results: &mut Vec<Vec<usize>>) {
        if slot == values.len() {
            results.push(values.to_vec());
            return;
        }
        for i in slot..values.len() {
            values.swap(slot, i);
            visit(values, slot + 1, results);
            values.swap(slot, i);
        }
    }
    let mut result = vec![];
    visit(&mut (0..n).collect::<Vec<_>>(), 0, &mut result);
    result
}
#[test]
fn complete_tree_bundles_centers_loops_and_multiplicity_are_invariant() {
    for n in 1..=6 {
        let mut graph = PublicGraph::default();
        for _ in 0..n {
            graph.vertex(VertexKind::CurrentObject, &0u8).unwrap();
        }
        for i in 1..n {
            graph
                .edge(i - 1, i, EdgeKind::LiveAttached, &(i % 2))
                .unwrap();
            graph.edge(i, i - 1, EdgeKind::EffectSource, &1u8).unwrap();
            graph.edge(i - 1, i, EdgeKind::EffectTarget, &2u8).unwrap();
        }
        graph
            .edge(0, 0, EdgeKind::StackTarget, &(0usize, 1usize))
            .unwrap();
        let base = normalize_graph(&graph).unwrap();
        assert_eq!(base.diagnostics.exact_search_nodes, 0);
        assert!(base.components.iter().all(|c| c.is_tree));
        for permutation in permutations(n) {
            assert_eq!(
                base.encoding,
                normalize_graph(&graph_permutation(&graph, &permutation))
                    .unwrap()
                    .encoding
                    .clone()
            );
        }
        let mut changed = graph.clone();
        changed.edges.pop();
        assert_ne!(
            base.encoding,
            normalize_graph(&changed).unwrap().encoding.clone()
        );
    }
}
fn regular(kind: bool) -> PublicGraph {
    let mut graph = PublicGraph::default();
    for _ in 0..6 {
        graph.vertex(VertexKind::CurrentObject, &0u8).unwrap();
    }
    let edges = if kind {
        vec![
            (0, 1),
            (1, 2),
            (2, 0),
            (3, 4),
            (4, 5),
            (5, 3),
            (0, 3),
            (1, 4),
            (2, 5),
        ]
    } else {
        (0..3).flat_map(|a| (3..6).map(move |b| (a, b))).collect()
    };
    for (a, b) in edges {
        graph.edge(a, b, EdgeKind::EffectTarget, &()).unwrap();
        graph.edge(b, a, EdgeKind::EffectTarget, &()).unwrap();
    }
    graph
}
#[test]
fn cyclic_refinement_adversaries_remain_exact() {
    let prism = regular(true);
    let bipartite = regular(false);
    let a = normalize_graph(&prism).unwrap();
    let b = normalize_graph(&bipartite).unwrap();
    assert_ne!(a.encoding, b.encoding);
    assert!(a.diagnostics.exact_search_nodes > 0);
    assert!(b.diagnostics.exact_search_nodes > 0);
    assert!(a.diagnostics.verified_automorphisms > 0);
    assert!(b.diagnostics.verified_automorphisms > 0);
    for permutation in permutations(6) {
        assert_eq!(
            a.encoding,
            normalize_graph(&graph_permutation(&prism, &permutation))
                .unwrap()
                .encoding
                .clone()
        );
        assert_eq!(
            b.encoding,
            normalize_graph(&graph_permutation(&bipartite, &permutation))
                .unwrap()
                .encoding
                .clone()
        );
    }
}
#[test]
fn joint_live_effect_history_occurrence_components_preserve_pairings() {
    let mut graph = PublicGraph::default();
    let source = graph.vertex(VertexKind::CurrentObject, &0u8).unwrap();
    let target = graph.vertex(VertexKind::CurrentObject, &1u8).unwrap();
    let history = graph.vertex(VertexKind::HistoricalFrame, &2u8).unwrap();
    let occurrence = graph.vertex(VertexKind::Occurrence, &3u8).unwrap();
    let effect = graph.vertex(VertexKind::ContinuousEffect, &4u8).unwrap();
    graph
        .edge(source, target, EdgeKind::LiveAttached, &())
        .unwrap();
    graph.edge(history, source, EdgeKind::FrameOf, &()).unwrap();
    graph
        .edge(history, target, EdgeKind::HistoricalAttached, &())
        .unwrap();
    graph
        .edge(effect, source, EdgeKind::EffectSource, &())
        .unwrap();
    graph
        .edge(effect, target, EdgeKind::EffectTarget, &())
        .unwrap();
    graph
        .edge(occurrence, history, EdgeKind::OccurrenceSource, &())
        .unwrap();
    graph
        .edge(occurrence, target, EdgeKind::OccurrenceSubject, &())
        .unwrap();
    let base = normalize_graph(&graph).unwrap();
    assert_eq!(base.components.len(), 1);
    for permutation in permutations(5) {
        assert_eq!(
            base.encoding,
            normalize_graph(&graph_permutation(&graph, &permutation))
                .unwrap()
                .encoding
                .clone()
        );
    }
    let mut changed = graph.clone();
    changed.edges.last_mut().unwrap().target = source;
    assert_ne!(
        base.encoding,
        normalize_graph(&changed).unwrap().encoding.clone()
    );
}
fn state(offset: u64, generation: u32, reverse: bool, pairing: bool) -> GameState {
    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: 1,
        name: "Body".into(),
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        ..Default::default()
    });
    db.insert(CardDef {
        id: 2,
        name: "Equipment".into(),
        card_types: vec![CardType::Artifact],
        subtypes: vec![Subtype("Equipment".into())],
        equip_cost: Some(ManaCost::zero()),
        ..Default::default()
    });
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    state.next_object_id = offset;
    let mut bodies = vec![];
    for _ in 0..6 {
        bodies.push(state.create_card_in_zone(1, 0, ZoneType::Battlefield));
    }
    let equipment: Vec<_> = (0..2)
        .map(|_| state.create_card_in_zone(2, 0, ZoneType::Battlefield))
        .collect();
    for instance in state.objects.values_mut() {
        instance.zone_change_count = generation;
        instance.summoning_sick = false;
    }
    for (source, target) in [
        (bodies[4], bodies[0]),
        (bodies[4], bodies[1]),
        (bodies[5], bodies[2]),
        (bodies[5], bodies[3]),
    ] {
        state.continuous_effects.push(ContinuousEffect {
            source_id: source,
            controller: 0,
            timestamp: 10,
            duration: Duration::Permanent,
            affected: AffectedObjects::Specific(target),
            modification: LayerModification::ModifyPT(0, 0),
        });
    }
    for (source, target) in [
        (equipment[0], bodies[0]),
        (equipment[1], bodies[if pairing { 2 } else { 1 }]),
    ] {
        let prepared = state
            .prepare_attach(
                state.exact_object(source).unwrap(),
                state.exact_object(target).unwrap(),
                mtg_gto::card::AttachmentContext::Established,
            )
            .unwrap();
        state.commit_attach(prepared).unwrap();
    }
    if reverse {
        state.battlefield.reverse();
        state.continuous_effects.reverse();
    }
    state
}
#[test]
fn complete_action_sets_reconstruct_with_shared_witness_across_allocations() {
    for pairing in [false, true] {
        let a = state(1, 0, false, pairing);
        let b = state(700, 40, true, pairing);
        let na = JointPublicNormalization::for_state(&a, 0).unwrap();
        let nb = JointPublicNormalization::for_state(&b, 0).unwrap();
        assert_eq!(na.encoding, nb.encoding);
        let ia =
            InformationSet::from_view_with_normalization(&a.visible_state(0), a.card_db(), &na)
                .unwrap();
        let ib =
            InformationSet::from_view_with_normalization(&b.visible_state(0), b.card_db(), &nb)
                .unwrap();
        assert_eq!(ia.hash_value(), ib.hash_value());
        assert_eq!(
            BucketedAbstraction.abstract_info_set(&ia),
            BucketedAbstraction.abstract_info_set(&ib)
        );
        assert_eq!(
            CardAwareBucketedAbstraction {
                card_db: a.card_db()
            }
            .abstract_info_set(&ia),
            CardAwareBucketedAbstraction {
                card_db: b.card_db()
            }
            .abstract_info_set(&ib)
        );
        let aa = legal_actions(&a);
        let ab = legal_actions(&b);
        let ka = canonical::canonicalize_actions(&aa, &a, &na).unwrap();
        let kb = canonical::canonicalize_actions(&ab, &b, &nb).unwrap();
        assert_eq!(
            ka.iter().cloned().collect::<HashSet<_>>(),
            kb.iter().cloned().collect()
        );
        assert_eq!(ka.len(), kb.len());
        for (action, key) in aa.iter().zip(&ka) {
            assert_eq!(
                canonical::resolve_with_normalization(key, &a, 0, &na)
                    .unwrap()
                    .as_ref(),
                Some(action)
            );
            let concrete = canonical::resolve_with_normalization(key, &b, 0, &nb)
                .unwrap()
                .unwrap();
            assert_eq!(
                canonical::canonicalize_actions(&[concrete], &b, &nb).unwrap()[0],
                *key
            );
        }
        let pairs = ka
            .iter()
            .filter(|key| matches!(key, canonical::CanonicalAction::Equip { .. }))
            .count();
        assert_eq!(pairs, 12);
    }
    let a = state(1, 0, false, false);
    let b = state(1, 0, false, true);
    assert_ne!(
        InformationSet::from_view(&a.visible_state(0), a.card_db())
            .unwrap()
            .hash_value(),
        InformationSet::from_view(&b.visible_state(0), b.card_db())
            .unwrap()
            .hash_value()
    );
}
#[test]
fn hidden_current_identities_do_not_join_owned_history_or_opaque_handles() {
    let a = state(1, 0, false, false);
    let mut b = a.clone();
    let hidden = b.create_card_in_zone(1, 1, ZoneType::Hand);
    b.continuous_effects.push(ContinuousEffect {
        source_id: hidden,
        controller: 1,
        timestamp: 20,
        duration: Duration::Permanent,
        affected: AffectedObjects::Specific(hidden),
        modification: LayerModification::ModifyPT(0, 0),
    });
    let first = JointPublicNormalization::for_state(&b, 0).unwrap();
    assert!(!first
        .exact_to_coordinate
        .contains_key(&b.exact_object(hidden).unwrap()));
    let bytes = first.encoding.clone();
    drop(first);
    b.objects.get_mut(&hidden).unwrap().card_def_id = 2;
    b.objects.get_mut(&hidden).unwrap().zone_change_count = 77;
    b.invalidate_characteristics_cache();
    assert_eq!(
        bytes,
        JointPublicNormalization::for_state(&b, 0)
            .unwrap()
            .encoding
            .clone()
    );
    let own = JointPublicNormalization::for_state(&b, 1).unwrap();
    assert!(own
        .exact_to_coordinate
        .contains_key(&b.exact_object(hidden).unwrap()));
}
#[test]
fn unsupported_schema_is_a_result_error_without_graph_mutation() {
    let mut graph = PublicGraph::default();
    graph.schema = 999;
    assert!(normalize_graph(&graph).is_err());
    assert_eq!(graph.schema, 999);
}
fn report_performance(case: &str, n: usize, state: &GameState) {
    let normalization = JointPublicNormalization::for_state(state, 0).unwrap();
    let d = &normalization.diagnostics;
    let started = std::time::Instant::now();
    let info = InformationSet::from_view_with_normalization(
        &state.visible_state(0),
        state.card_db(),
        &normalization,
    )
    .unwrap();
    std::hint::black_box(info.hash_value());
    let info_ns = started.elapsed().as_nanos();
    let actions = legal_actions(state);
    let started = std::time::Instant::now();
    let keys = canonical::canonicalize_actions(&actions, state, &normalization).unwrap();
    let action_ns = started.elapsed().as_nanos();
    let started = std::time::Instant::now();
    for key in keys.iter().take(32) {
        std::hint::black_box(
            canonical::resolve_with_normalization(key, state, 0, &normalization).unwrap(),
        );
    }
    let reconstruct_ns = started.elapsed().as_nanos();
    println!("{{\"case\":\"{case}\",\"n\":{n},\"vertices\":{},\"edges\":\"{:?}\",\"components\":{},\"rounds\":{},\"cells\":\"{:?}\",\"nodes\":{},\"candidates\":{},\"inventory_ns\":{},\"components_ns\":{},\"tree_ns\":{},\"cyclic_ns\":{},\"witness_ns\":{},\"supplement_ns\":{},\"total_ns\":{},\"info_ns\":{info_ns},\"actions\":{},\"action_ns\":{action_ns},\"reconstruct_first_32_ns\":{reconstruct_ns}}}", d.vertices,d.edges_by_type,normalization.components.len(),d.refinement_rounds,d.unresolved_cell_sizes,d.exact_search_nodes,d.candidate_encodings,d.inventory_nanos,d.component_nanos,d.tree_nanos,d.cyclic_nanos,d.witness_nanos,d.supplement_nanos,d.total_nanos, keys.len());
}
fn performance_state(n: usize, shared: bool, modifier: bool) -> (GameState, Vec<u64>) {
    let mut state = state(1, 0, false, false);
    state.continuous_effects.clear();
    let originals = state.battlefield.clone();
    for id in originals {
        state.move_object(id, ZoneType::Battlefield, ZoneType::Exile);
    }
    let target = state.create_card_in_zone(1, 0, ZoneType::Battlefield);
    let mut targets = vec![];
    for i in 0..n {
        let source = state.create_card_in_zone(2, 0, ZoneType::Battlefield);
        let current = if shared {
            target
        } else {
            state.create_card_in_zone(1, 0, ZoneType::Battlefield)
        };
        if i % 2 == 0 {
            state.objects.get_mut(&current).unwrap().is_token = true;
        }
        let prepared = state
            .prepare_attach(
                state.exact_object(source).unwrap(),
                state.exact_object(current).unwrap(),
                mtg_gto::card::AttachmentContext::ExistingEquip,
            )
            .unwrap();
        state.commit_attach(prepared).unwrap();
        if modifier {
            state.continuous_effects.push(ContinuousEffect {
                source_id: source,
                controller: 0,
                timestamp: 10,
                duration: Duration::Permanent,
                affected: AffectedObjects::AttachedTo,
                modification: LayerModification::ModifyPT(1, 0),
            });
        }
        targets.push(current);
    }
    state.invalidate_characteristics_cache();
    (state, targets)
}
#[test]
#[ignore = "implemented production diagnostics; run explicitly with --ignored --nocapture"]
fn production_performance_diagnostics() {
    for n in [1, 5, 10, 20, 50] {
        for shared in [true, false] {
            let (state, _) = performance_state(n, shared, false);
            report_performance(
                if shared {
                    "shared_star_token_board"
                } else {
                    "distinct_targets_token_board"
                },
                n,
                &state,
            );
        }
    }
    for n in [1, 5, 10, 20] {
        for shared in [true, false] {
            let (state, _) = performance_state(n, shared, true);
            report_performance(
                if shared {
                    "cyclic_attached_modifier_shared"
                } else {
                    "cyclic_attached_modifier_distinct"
                },
                n,
                &state,
            );
        }
    }
    for pairs in [2, 3, 4, 5, 6, 8, 10] {
        let (mut state, targets) = performance_state(pairs, false, false);
        let hub = state.create_card_in_zone(1, 0, ZoneType::Battlefield);
        for target in targets {
            state.continuous_effects.push(ContinuousEffect {
                source_id: hub,
                controller: 0,
                timestamp: 10,
                duration: Duration::Permanent,
                affected: AffectedObjects::Specific(target),
                modification: LayerModification::ModifyPT(0, 0),
            });
        }
        state.invalidate_characteristics_cache();
        report_performance("production_coupled_tree", pairs, &state);
        let normal = JointPublicNormalization::for_state(&state, 0).unwrap();
        assert_eq!(normal.diagnostics.exact_search_nodes, 0);
    }
    for n in [1, 5, 10, 20] {
        let (mut state, targets) = performance_state(n, false, true);
        let mut db = (*state.card_db.as_ref().unwrap().as_ref()).clone();
        let mut equipment = db.get(2).unwrap().clone();
        equipment
            .triggered_abilities
            .push(mtg_gto::card::TriggeredAbility {
                trigger: mtg_gto::card::TriggerCondition::EquippedCreatureDies,
                effect: mtg_gto::card::Effect::DrawCards { count: 1 },
                description: "synthetic".into(),
            });
        db.insert(equipment);
        state.card_db = Some(Arc::new(db));
        let requests: Vec<_> = targets
            .iter()
            .map(|&id| mtg_gto::rules::transitions::TransitionRequest {
                object: state.exact_object(id).unwrap(),
                from: ZoneType::Battlefield,
                to: ZoneType::Graveyard,
                kind: mtg_gto::rules::transitions::MovementKind::Put,
            })
            .collect();
        mtg_gto::rules::transitions::transition_batch(&mut state, &requests).unwrap();
        report_performance("simultaneous_owned_history", n, &state);
    }
    for n in [2, 3, 4, 5, 6] {
        let (mut state, targets) = performance_state(n, false, false);
        for index in 0..n {
            for target in [targets[index], targets[(index + 1) % n]] {
                state.continuous_effects.push(ContinuousEffect {
                    source_id: targets[index],
                    controller: 0,
                    timestamp: 10,
                    duration: Duration::Permanent,
                    affected: AffectedObjects::Specific(target),
                    modification: LayerModification::ModifyPT(0, 0),
                });
            }
        }
        state.invalidate_characteristics_cache();
        report_performance("overlapping_cyclic_effects", n, &state);
    }
    for kind in [true, false] {
        let result = normalize_graph(&regular(kind)).unwrap();
        println!("cyclic_prism={kind} diagnostics={:?}", result.diagnostics);
    }
}

fn reverse_runtime_ids(state: &GameState) -> GameState {
    use mtg_gto::game::{StackSource, Target};
    let mut result = state.clone();
    let mut ids: Vec<_> = state.objects.keys().copied().collect();
    ids.sort_unstable();
    let replacements: std::collections::HashMap<_, _> = ids
        .iter()
        .copied()
        .zip(ids.iter().rev().map(|id| id + 500))
        .collect();
    result.objects.clear();
    for (&old, instance) in &state.objects {
        let mut instance = instance.clone();
        instance.object_id = replacements[&old];
        instance.exiled_by = instance.exiled_by.map(|id| replacements[&id]);
        result.objects.insert(instance.object_id, instance);
    }
    for (&old, instance) in &state.objects {
        if let Some(mut link) = instance.attachment_link() {
            link.target.id = replacements[&link.target.id];
            result.set_malformed_attachment_fixture(replacements[&old], Some(link));
        }
    }
    for id in &mut result.battlefield {
        *id = replacements[id];
    }
    result.battlefield.reverse();
    for seat in &mut result.players {
        for zone in [
            &mut seat.hand,
            &mut seat.library,
            &mut seat.graveyard,
            &mut seat.exile,
            &mut seat.command_zone,
        ] {
            for id in zone {
                *id = replacements[id];
            }
        }
    }
    for effect in &mut result.continuous_effects {
        effect.source_id = replacements[&effect.source_id];
        if let AffectedObjects::Specific(ref mut id) = effect.affected {
            *id = replacements[id];
        }
    }
    result.continuous_effects.reverse();
    for entry in &mut result.stack {
        match &mut entry.source {
            StackSource::Spell(id) | StackSource::ActivatedAbility { source_id: id, .. } => {
                *id = replacements[id]
            }
            _ => panic!("fixture has no historical stack source"),
        }
        for target in &mut entry.targets {
            if let Target::Object(id) = target {
                *id = replacements[id];
            }
        }
    }
    result.combat.attackers = result
        .combat
        .attackers
        .iter()
        .map(|id| replacements[id])
        .collect();
    result.combat.blockers = result
        .combat
        .blockers
        .iter()
        .map(|(a, b)| (replacements[a], replacements[b]))
        .collect();
    result.combat.attacker_blockers = result
        .combat
        .attacker_blockers
        .iter()
        .map(|(a, bs)| {
            (
                replacements[a],
                bs.iter().map(|b| replacements[b]).collect(),
            )
        })
        .collect();
    result.combat.damage_assignment = result
        .combat
        .damage_assignment
        .iter()
        .map(|(a, bs)| {
            (
                replacements[a],
                bs.iter().map(|(b, n)| (replacements[b], *n)).collect(),
            )
        })
        .collect();
    result.invalidate_characteristics_cache();
    result
}
#[test]
fn reversed_source_target_ids_preserve_complete_action_sets_and_anchor_incidence() {
    use mtg_gto::game::{StackEntry, StackSource, Target};
    for anchored in [false, true] {
        let mut a = state(1, 0, false, false);
        if anchored {
            let wearer = a.battlefield[0];
            let other = a.battlefield[2];
            let equipment = a.battlefield[6];
            a.stack.push(StackEntry {
                id: 17,
                source: StackSource::ActivatedAbility {
                    source_id: equipment,
                    ability_index: 0,
                },
                controller: 0,
                targets: vec![Target::Object(other)],
                target_generations: vec![Some(a.objects[&other].zone_change_count)],
            });
            a.combat.attackers.push(wearer);
            a.combat.blockers.insert(other, wearer);
            a.combat.attacker_blockers.insert(wearer, vec![other]);
            a.combat.damage_assignment.insert(wearer, vec![(other, 1)]);
            let exile = a.create_card_in_zone(1, 0, ZoneType::Exile);
            a.objects.get_mut(&exile).unwrap().exiled_by = Some(equipment);
        }
        let b = reverse_runtime_ids(&a);
        let na = JointPublicNormalization::for_state(&a, 0).unwrap();
        let nb = JointPublicNormalization::for_state(&b, 0).unwrap();
        assert_eq!(na.encoding, nb.encoding);
        let keys_a = canonical::canonicalize_actions(&legal_actions(&a), &a, &na).unwrap();
        let keys_b = canonical::canonicalize_actions(&legal_actions(&b), &b, &nb).unwrap();
        assert_eq!(
            keys_a.iter().cloned().collect::<HashSet<_>>(),
            keys_b.iter().cloned().collect()
        );
        assert_eq!(keys_a.len(), keys_b.len());
        for key in keys_a {
            let concrete = canonical::resolve_with_normalization(&key, &b, 0, &nb)
                .unwrap()
                .unwrap();
            assert_eq!(
                canonical::canonicalize_actions(&[concrete], &b, &nb).unwrap()[0],
                key
            );
        }
        if anchored {
            for kind in [
                EdgeKind::StackSource,
                EdgeKind::StackTarget,
                EdgeKind::Blocks,
                EdgeKind::DamageAssignment,
                EdgeKind::LinkedExile,
            ] {
                assert!(na.diagnostics.edges_by_type.contains_key(&kind));
            }
        }
    }
}
