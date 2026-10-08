use std::sync::Arc;

use mtg_gto::action::canonical::{canonicalize, resolve};
use mtg_gto::action::{legal_actions, legal_actions_abstracted, Action};
use mtg_gto::card::sample;
use mtg_gto::card::{CardDef, CardType, Effect, ZoneType};
use mtg_gto::game::{CardDatabase, GameState, Phase};
use mtg_gto::info_set::InformationSet;
use mtg_gto::rules::{self, begin_terminal_copy_batch, prepare_spell_copy, snapshot_stack_spell, CopyBatchOutcome, CopyTargetPolicy};
use mtg_gto::solver::RegretTable;
use mtg_gto::strategy::{GreedyStrategy, McfrStrategy, Strategy};

fn land_choice(card_id: u64) -> (GameState, u64) {
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(sample::build_sample_db()));
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;
    let land = state.create_card_in_zone(card_id, 0, ZoneType::Hand);
    (state, land)
}

#[test]
fn unseen_state_uses_greedy_useful_play_across_card_types() {
    // Both states have multiple legal actions. The old uniform fallback could
    // pass or end the turn instead of making the useful play.
    for card_id in [sample::ids::MOUNTAIN, sample::ids::FOREST] {
        let (state, land) = land_choice(card_id);
        let expected = Action::PlayLand { object_id: land };
        assert!(legal_actions(&state).len() > 1);
        assert_eq!(GreedyStrategy.choose_action(&state, 0).unwrap(), expected);
        let strategy = McfrStrategy::new(RegretTable::new());
        for _ in 0..32 {
            let chosen = strategy.choose_action(&state, 0).unwrap();
            assert_eq!(chosen, expected);
            assert!(legal_actions(&state).contains(&chosen));
            assert!(legal_actions_abstracted(&state).contains(&chosen));
            assert_eq!(resolve(&canonicalize(&chosen, &state).unwrap(), &state, 0).unwrap(), Some(chosen));
        }
    }
}

#[test]
fn unseen_state_casts_affordable_spell_as_current_concrete_action() {
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(sample::build_sample_db()));
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;
    let spell = state.create_card_in_zone(sample::ids::GRIZZLY_BEARS, 0, ZoneType::Hand);
    for _ in 0..2 {
        state.create_card_in_zone(sample::ids::FOREST, 0, ZoneType::Battlefield);
    }
    let expected = Action::CastSpell { object_id: spell, targets: vec![] };
    assert!(legal_actions(&state).contains(&expected));
    assert_eq!(GreedyStrategy.choose_action(&state, 0).unwrap(), expected);
    let chosen = McfrStrategy::new(RegretTable::new()).choose_action(&state, 0).unwrap();
    assert_eq!(chosen, expected);
    assert!(legal_actions(&state).contains(&chosen));
    assert_eq!(resolve(&canonicalize(&chosen, &state).unwrap(), &state, 0).unwrap(), Some(chosen));
}

#[test]
fn trained_entry_uses_stored_average_strategy_instead_of_greedy() {
    let (state, _) = land_choice(sample::ids::MOUNTAIN);
    assert!(matches!(GreedyStrategy.choose_action(&state, 0).unwrap(), Action::PlayLand { .. }));
    let info_hash = InformationSet::from_view(&state.visible_state(0), state.card_db()).unwrap().hash_value();
    let mut table = RegretTable::new();
    let pass = canonicalize(&Action::PassPriority, &state).unwrap();
    table.get_or_create(info_hash).unwrap().get_or_create_action(&pass).cumulative_strategy = 1.0;
    let strategy = McfrStrategy::new(table);
    for _ in 0..32 {
        assert_eq!(strategy.choose_action(&state, 0).unwrap(), Action::PassPriority);
    }
}

#[test]
fn unseen_pending_copy_order_returns_only_mandatory_legal_choice() {
    const DRAW: u64 = 984_001;
    let mut db = CardDatabase::new();
    db.insert(CardDef { id: DRAW, name: "Synthetic draw".into(),
        card_types: vec![CardType::Instant], spell_effect: Some(Effect::DrawCards { count: 1 }),
        ..Default::default() });
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    let object_id = state.create_card_in_zone(DRAW, 0, ZoneType::Hand);
    rules::apply_action(&mut state, &Action::CastSpell { object_id, targets: vec![] });
    let snapshot = snapshot_stack_spell(&state, state.stack.last().unwrap().id).unwrap();
    let copy = prepare_spell_copy(&state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
    assert_eq!(begin_terminal_copy_batch(&mut state, vec![copy.clone(), copy], true),
        Ok(CopyBatchOutcome::Pending));
    let legal = legal_actions(&state);
    assert_eq!(legal, vec![Action::ChooseNextCopy { item_index: 0 },
        Action::ChooseNextCopy { item_index: 1 }]);
    let strategy = McfrStrategy::new(RegretTable::new());
    let chosen = strategy.choose_action(&state, 0).unwrap();
    assert!(legal.contains(&chosen));
    assert_eq!(resolve(&canonicalize(&chosen, &state).unwrap(), &state, 0).unwrap(), Some(chosen.clone()));
    rules::apply_action(&mut state, &chosen);
    assert!(state.pending_copy_order.is_none());
}
