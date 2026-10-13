use std::sync::Arc;
use crate::{card::{sample, CardDef, CardType, Effect, TargetSpec, ZoneType}, game::{GameState, Target},
    rules::{self, sba::PreparedPassFailure, transitions::TransitionError}};

#[test]
fn synchronous_compatibility_cannot_resume_stopped_or_mandatory_owned_state() {
    for fault in 0..4 {
        let mut db = sample::build_sample_db();
        db.insert(CardDef { id: 999_511, name: "Unmoved target".into(), card_types: vec![CardType::Creature],
            power: Some(3), toughness: Some(3), ..Default::default() });
        let mut state = GameState::new_commander(2); state.card_db = Some(Arc::new(db));
        let commander = state.create_card_in_zone(sample::ids::ASHCOAT_OF_THE_SHADOW_SWARM, 0, ZoneType::Command).unwrap();
        state.designate_commander(commander, 0).unwrap();
        let subject = state.create_card_in_zone(999_511, 1, ZoneType::Battlefield).unwrap();
        state.create_card_in_zone(sample::ids::SWAMP, 0, ZoneType::Library).unwrap();
        match fault {
            0 => state.sba_failure = Some(PreparedPassFailure::StateEncoding),
            1 => state.sba_failure = Some(PreparedPassFailure::Transition(TransitionError::UnsupportedPath)),
            2 => state.game_over = true,
            _ => {state.move_object(commander, ZoneType::Command, ZoneType::Battlefield); state.move_object(commander, ZoneType::Battlefield, ZoneType::Graveyard); rules::check_state_based_actions(&mut state); assert!(state.pending_commander_sba.is_some());}
        }
        state.drain_events(); let before = bincode::serialize(&state).unwrap();
        super::resolve_effect(&mut state, &Effect::Multiple(vec![Effect::DestroyTarget { target: TargetSpec::AnyCreature },
            Effect::DrawCards { count: 1 }]), 0, &[Target::Object(subject)], &[Some(0)], None);
        assert_eq!(bincode::serialize(&state).unwrap(), before, "stopped/mandatory context {fault}");
        assert!(state.pending_events.is_empty());
    }
}
