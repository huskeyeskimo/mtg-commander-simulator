use mtg_gto::{action::{legal_actions, Action}, game::GameState, rules};

/// Adapt interaction assertions formerly expressed as one controller's complete
/// order. Every occurrence is now submitted through the real incremental API.
pub fn finish_controller(state: &mut GameState, first: &Action) {
    let controller = state.priority_player;
    match first {
        Action::OrderTriggerOccurrences { ordering } => {
            // Capture identity before vector slots shift or the window freezes.
            let selected: Vec<_> = ordering.iter().map(|&slot| state.pending_triggers[slot].clone()).collect();
            rules::check_state_based_actions(state);
            for wanted in selected {
                let trigger = state.pending_triggers.iter().find(|trigger|
                    trigger.source_id == wanted.source_id && trigger.ability_index == wanted.ability_index
                    && trigger.controller == wanted.controller && trigger.targets == wanted.targets
                    && bincode::serialize(&trigger.context).unwrap() == bincode::serialize(&wanted.context).unwrap()).expect("requested historical occurrence must remain pending");
                let action = Action::ChooseNextTrigger { handle: trigger.placement.expect("frozen occurrence") };
                assert!(legal_actions(state).contains(&action));
                rules::apply_action(state, &action);
            }
            return;
        }
        Action::ChooseNextTrigger { .. } => rules::apply_action(state, first),
        _ => { rules::apply_action(state, first); return; }
    }
    while state.trigger_placement.is_some() && state.priority_player == controller && !state.gameplay_stopped() {
        let action = legal_actions(state).into_iter().find(|a| matches!(a, Action::ChooseNextTrigger { .. })).unwrap();
        rules::apply_action(state, &action);
    }
}
