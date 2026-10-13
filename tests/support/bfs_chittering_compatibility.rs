//! Closed opponent hand selection must precede BFS's ordinary opponent pass.
use super::*;
#[test]
fn chittering_opponent_private_selection_does_not_stall_search() {
    use mtg_gto::{card::ZoneType, game::{StackEntry,StackSource,TriggerContext,Target}};
    let mut state=GameState::new_commander(2);state.card_db=Some(Arc::new(sample::build_sample_db()));state.phase=Phase::PreCombatMain;
    let source=state.create_card_in_zone(ids::CHITTERING_RATS,0,ZoneType::Battlefield).unwrap();
    state.create_card_in_zone(ids::SWAMP,1,ZoneType::Hand).unwrap();
    let id=state.new_stack_id();state.stack.push(StackEntry {id,controller:0,source:StackSource::TriggeredAbility {source_id:source,ability_index:0,
        context:Box::new(TriggerContext {source_card_id:ids::CHITTERING_RATS,source_generation:0,effect:mtg_gto::card::Effect::ChitteringRats,cast_spell:None,zone_transition:None})},targets:vec![Target::Player(1)],target_generations:vec![None]});
    rules::apply_action(&mut state,&Action::PassPriority);rules::apply_action(&mut state,&Action::PassPriority);assert!(state.pending_chittering.is_some());
    let before=bincode::serialize(&state).unwrap();let mut best_turn=10;let mut best=Vec::new();let mut current=Vec::new();let mut stats=SearchStats::new();let mut visited=HashMap::new();
    let limits=SearchLimits {deadline:Instant::now()+std::time::Duration::from_secs(2),max_states:10,max_visited:10,max_depth:1,trace:false};
    dfs_search(&mut state,&mut best_turn,&mut best,&mut current,&ashcoat_config(),&mut stats,&mut visited,&limits);
    assert!(!stats.stalled);assert!(!stats.invalid);assert!(stats.states_explored>0);assert_eq!(bincode::serialize(&state).unwrap(),before);
}

#[test]
fn opponent_chittering_target_placement_precedes_bfs_auto_pass() {
    use mtg_gto::{card::{ZoneType,TriggerCondition},game::StackSource};
    let mut state=GameState::new_commander(3);state.card_db=Some(Arc::new(sample::build_sample_db()));state.phase=Phase::PreCombatMain;
    let source=state.create_card_in_zone(ids::CHITTERING_RATS,1,ZoneType::Battlefield).unwrap();
    state.create_card_in_zone(ids::SWAMP,0,ZoneType::Hand).unwrap();state.create_card_in_zone(ids::SOL_RING,2,ZoneType::Hand).unwrap();
    let _=rules::fire_triggers(&mut state,TriggerCondition::EntersBattlefield,Some(source));
    assert!(state.trigger_placement.is_some());assert_eq!(state.active_player,0);assert_eq!(state.priority_player,1);
    let legal=legal_actions(&state);let selected=mtg_gto::strategy::Strategy::choose_action(&mtg_gto::strategy::GoldfishStrategy,&state,1).unwrap();assert!(legal.contains(&selected));assert!(matches!(selected,Action::PlaceChitteringTrigger {..}));
    let mut witness=state.clone();rules::apply_action(&mut witness,&selected);assert!(witness.trigger_placement.is_none());assert_eq!(witness.stack.len(),1);assert!(matches!(witness.stack[0].source,StackSource::TriggeredAbility {..}));
    let before=bincode::serialize(&state).unwrap();let mut best_turn=10;let mut best=Vec::new();let mut current=Vec::new();let mut stats=SearchStats::new();let mut visited=HashMap::new();
    // Stop after the first pilot branch; later empty-library multiplayer
    // elimination is an unrelated deliberately unsupported engine boundary.
    let limits=SearchLimits {deadline:Instant::now()+std::time::Duration::from_secs(2),max_states:10,max_visited:10,max_depth:0,trace:false};
    dfs_search(&mut state,&mut best_turn,&mut best,&mut current,&ashcoat_config(),&mut stats,&mut visited,&limits);
    assert!(!stats.stalled,"opponent target placement must precede ordinary auto-pass");assert!(!stats.invalid);assert!(stats.states_explored>0);assert_eq!(bincode::serialize(&state).unwrap(),before);
}
