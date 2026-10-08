use std::sync::Arc;

use mtg_gto::action::canonical::{canonicalize, resolve};
use mtg_gto::action::{legal_actions, Action};
use mtg_gto::card::{CardDef, CardType, Effect, KeywordAbility, TargetSpec, ZoneType};
use mtg_gto::game::{CardDatabase, GameState, Phase, StackEntry, StackSource, Target};
use mtg_gto::mana::ManaCost;
use mtg_gto::solver::RegretTable;
use mtg_gto::strategy::{GreedyStrategy, McfrStrategy, Strategy};

const SPELL: u64 = 985_001;
const CREATURE: u64 = 985_002;
const OTHER_CREATURE: u64 = 985_003;

fn game(effect: Effect, player: usize) -> (GameState, u64) {
    let mut db = CardDatabase::new();
    db.insert(CardDef {
        id: SPELL,
        name: "Synthetic targeted spell".into(),
        card_types: vec![CardType::Instant],
        mana_cost: Some(ManaCost::zero()),
        spell_effect: Some(effect),
        ..Default::default()
    });
    db.insert(CardDef {
        id: CREATURE,
        name: "Synthetic creature".into(),
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        ..Default::default()
    });
    db.insert(CardDef {
        id: OTHER_CREATURE,
        name: "Other synthetic creature".into(),
        card_types: vec![CardType::Creature],
        power: Some(2),
        toughness: Some(2),
        ..Default::default()
    });
    let mut state = GameState::new(2);
    state.card_db = Some(Arc::new(db));
    state.active_player = player;
    state.priority_player = player;
    state.phase = Phase::PreCombatMain;
    state.turn_number = 2;
    let spell = state.create_card_in_zone(SPELL, player, ZoneType::Hand);
    (state, spell)
}

fn assert_greedy_cast(state: &GameState, spell: u64, player: usize, target: Target) {
    let expected = Action::CastSpell { object_id: spell, targets: vec![target] };
    assert!(legal_actions(state).contains(&expected));
    let chosen = GreedyStrategy.choose_action(state, player).unwrap();
    assert_eq!(chosen, expected);
    assert_eq!(resolve(&canonicalize(&chosen, state).unwrap(), state, player).unwrap(), Some(chosen));
}

#[test]
fn damage_targets_opponent_from_either_seat_and_fallback_agrees() {
    for player in 0..2 {
        let (state, spell) = game(Effect::DealDamage {
            amount: 2, target: TargetSpec::AnyPlayer,
        }, player);
        let opponent = 1 - player;
        let expected = Action::CastSpell {
            object_id: spell, targets: vec![Target::Player(opponent)],
        };
        assert_greedy_cast(&state, spell, player, Target::Player(opponent));
        assert_eq!(McfrStrategy::new(RegretTable::new()).choose_action(&state, player).unwrap(), expected);
    }
}

#[test]
fn destructive_effect_prefers_opponents_currently_controlled_permanent() {
    for player in 0..2 {
        let (mut state, spell) = game(Effect::DestroyTarget {
            target: TargetSpec::AnyCreature,
        }, player);
        let opponent = 1 - player;
        // The own creature is last, so enumeration order alone selects it.
        let enemy = state.create_card_in_zone(CREATURE, opponent, ZoneType::Battlefield);
        state.create_card_in_zone(OTHER_CREATURE, player, ZoneType::Battlefield);
        assert_greedy_cast(&state, spell, player, Target::Object(enemy));
    }
}

#[test]
fn useful_keyword_prefers_own_permanent_even_with_untargeted_composite_child() {
    for player in 0..2 {
        let effect = Effect::Multiple(vec![
            Effect::GainKeywordUntilEOT {
                keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature,
            },
            Effect::DealDamage { amount: 1, target: TargetSpec::EachCreature },
        ]);
        let (mut state, spell) = game(effect, player);
        let own = state.create_card_in_zone(CREATURE, player, ZoneType::Battlefield);
        state.create_card_in_zone(OTHER_CREATURE, 1 - player, ZoneType::Battlefield);
        assert_greedy_cast(&state, spell, player, Target::Object(own));
    }
}

#[test]
fn counter_prefers_opponents_stack_entry() {
    for player in 0..2 {
        let (mut state, spell) = game(Effect::Counter { target: TargetSpec::AnySpell }, player);
        let definition = Box::new(CardDef {
            id: 985_004,
            name: "Synthetic stack spell".into(),
            card_types: vec![CardType::Instant],
            spell_effect: Some(Effect::DrawCards { count: 1 }),
            ..Default::default()
        });
        let opponent_spell = state.new_stack_id();
        state.stack.push(StackEntry { id: opponent_spell,
            source: StackSource::SpellCopy { definition: definition.clone() },
            controller: 1 - player, targets: vec![], target_generations: vec![] });
        let own_spell = state.new_stack_id();
        state.stack.push(StackEntry { id: own_spell,
            source: StackSource::SpellCopy { definition },
            controller: player, targets: vec![], target_generations: vec![] });
        assert_greedy_cast(&state, spell, player, Target::StackEntry(opponent_spell));
    }
}

#[test]
fn ambiguous_targeted_effect_remains_legal_and_canonical() {
    let (mut state, _) = game(Effect::Multiple(vec![
        Effect::DealDamage { amount: 1, target: TargetSpec::AnyCreature },
        Effect::GainKeywordUntilEOT {
            keyword: KeywordAbility::Haste, target: TargetSpec::AnyCreature,
        },
    ]), 1);
    state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state.create_card_in_zone(OTHER_CREATURE, 1, ZoneType::Battlefield);
    let chosen = GreedyStrategy.choose_action(&state, 1).unwrap();
    assert!(matches!(chosen, Action::CastSpell { .. }));
    assert!(legal_actions(&state).contains(&chosen));
    assert_eq!(resolve(&canonicalize(&chosen, &state).unwrap(), &state, 1).unwrap(), Some(chosen));
}
