//! Deserialization wire mirrors GameState, then validates cross-field Commander bindings.
use super::*;

#[derive(Deserialize)]
#[serde(remote = "GameState")]
struct GameStateWire {
    #[serde(deserialize_with = "deserialize_game_state_format")]
    state_format: u32,
    #[serde(skip)]
    pub card_db: Option<Arc<CardDatabase>>,

    #[serde(skip)]
    pub combo_registry: Option<Arc<ComboRegistry>>,

    pub format: GameFormat,

    pub objects: HashMap<ObjectId, CardInstance>,

    pub battlefield: Vec<ObjectId>,

    pub stack: Vec<StackEntry>,

    pub players: Vec<PlayerState>,
    pub commanders: Vec<CommanderRecord>,
    pub commander_arrivals: Vec<CommanderArrival>,
    pub consumed_commander_arrivals: Vec<CommanderArrival>,
    pub pending_commander_sba: Option<CommanderSbaChoice>,
    pub pending_movement: Option<PendingMovement>,
    pub pending_chittering: Option<PendingChittering>,
    pub next_movement_window: u64,

    pub active_player: PlayerIndex,

    pub phase: Phase,

    pub priority_player: PlayerIndex,

    pub turn_number: u32,

    pub consecutive_passes: u32,

    pub combat: CombatState,

    pub next_object_id: ObjectId,

    pub next_stack_id: StackId,

    pub next_zone_event_group_id: u64,

    pub pending_triggers: Vec<PendingTrigger>,

    #[serde(default)]
    pub trigger_order_resume: Option<TriggerOrderResume>,
    #[serde(deserialize_with = "deserialize_trigger_placement")]
    pub trigger_placement: Option<TriggerPlacement>,
    pub next_trigger_window: u64,

    #[serde(default)]
    pub cleanup_needs_repeat: bool,

    #[serde(default)]
    pub cleanup_discard_in_progress: bool,

    pub pending_copy_order: Option<PendingCopyOrder>,

    #[serde(skip)]
    pub(crate) trigger_placement_deferred: bool,

    #[serde(skip)]
    pub(crate) effect_terminal_position: Option<bool>,

    pub continuous_effects: Vec<ContinuousEffect>,

    pub next_timestamp: u32,

    pub replacement_effects: Vec<ReplacementEffect>,

    pub pending_tutor: Option<PendingTutor>,

    pub extra_turns: VecDeque<PlayerIndex>,

    pub spells_cast_this_turn: u32,

    pub skip_phases: HashSet<Phase>,

    pub game_over: bool,

    pub winner: Option<PlayerIndex>,

    #[serde(skip)]
    pub pending_events: Vec<GameEvent>,

    #[serde(skip)]
    pub characteristics_cache: CharacteristicsCache,

    #[serde(default)]
    pub loss_boundary: LossBoundary,

    #[serde(skip)]
    pub(crate) loss_action_in_progress: bool,

    #[serde(default)]
    pub sba_failure: Option<crate::rules::sba::PreparedPassFailure>,
}

impl<'de> Deserialize<'de> for GameState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut state = GameStateWire::deserialize(deserializer)?;
        if crate::rules::chittering::resolving_entry(&state).is_some()
            || state.pending_movement.as_ref().is_some_and(|op| op.resolution_owned
                || crate::rules::movement_choice::private_draw_operation(op)) {
            state.trigger_placement_deferred = true;
        }
        state.validate_commanders().map_err(serde::de::Error::custom)?;
        state.validate_commander_choices().map_err(serde::de::Error::custom)?;
        state.validate_movement_choices().map_err(serde::de::Error::custom)?;
        crate::rules::chittering::validate_structure(&state).map_err(serde::de::Error::custom)?;
        crate::rules::reclamation::validate_structure(&state).map_err(serde::de::Error::custom)?;
        Ok(state)
    }
}
