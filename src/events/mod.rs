//! Phase 1A.2 — Event System Skeleton
//!
//! Game events are transient notifications that fire during `apply_action()`
//! and `resolve_effect()`. They are NOT part of `GameState` and do NOT affect
//! `Clone`/snapshot cost. This preserves the cheap-clone invariant required
//! by MCCFR traversal.
//!
//! # Design
//!
//! Events flow through an `EventBus` that holds a function-pointer table
//! (not per-game-state dynamic dispatch). Handlers receive
//! `(&mut GameState, &GameEvent)` and can inspect or mutate state in
//! response to events.
//!
//! The event bus is intentionally kept outside `GameState`:
//! - It is not cloned when `GameState::clone()` is called
//! - It is not serialized
//! - It is reconstructed/configured at the engine level, not per game
//!
//! # Usage
//!
//! ```ignore
//! let mut bus = EventBus::new();
//! bus.subscribe(|state, event| {
//!     if let GameEvent::ZoneChange { .. } = event {
//!         // react to zone changes
//!     }
//! });
//! bus.emit(&mut state, &GameEvent::DamageDealt { ... });
//! ```

use crate::card::ObjectId;
use crate::game::{GameState, PlayerIndex, Target};

/// A counter type placed on permanents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CounterType {
    PlusOnePlusOne,
    MinusOneMinusOne,
    Loyalty,
    Charge,
    // Extensible — add more as needed.
}

/// The zone a card resides in (mirrors `card::ZoneType` but avoids
/// a circular dependency and exists specifically for event payloads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zone {
    Library,
    Hand,
    Battlefield,
    Graveyard,
    Exile,
    Stack,
    Command,
}

impl From<crate::card::ZoneType> for Zone {
    fn from(z: crate::card::ZoneType) -> Self {
        match z {
            crate::card::ZoneType::Library => Zone::Library,
            crate::card::ZoneType::Hand => Zone::Hand,
            crate::card::ZoneType::Battlefield => Zone::Battlefield,
            crate::card::ZoneType::Graveyard => Zone::Graveyard,
            crate::card::ZoneType::Exile => Zone::Exile,
            crate::card::ZoneType::Stack => Zone::Stack,
            crate::card::ZoneType::Command => Zone::Command,
        }
    }
}

/// A game event that just happened. Events are transient — they fire,
/// handlers process them, and they're discarded. They are NOT part of
/// `GameState` and do NOT affect `Clone`/snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    /// Damage was dealt to a target.
    DamageDealt {
        source: ObjectId,
        target: Target,
        amount: u32,
        is_combat: bool,
    },

    /// A card instance moved between zones.
    ZoneChange {
        object: ObjectId,
        from: Zone,
        to: Zone,
    },

    /// A player's life total changed.
    LifeChanged {
        player: PlayerIndex,
        old: i32,
        new: i32,
    },

    /// A spell was cast (put on the stack).
    SpellCast {
        object: ObjectId,
        controller: PlayerIndex,
    },

    /// A triggered ability was put on the stack.
    AbilityTriggered {
        source: ObjectId,
        ability_index: usize,
    },

    /// A counter was placed on or removed from a permanent.
    CounterChanged {
        object: ObjectId,
        counter_type: CounterType,
        old_count: i32,
        new_count: i32,
    },

    /// A permanent was tapped.
    Tapped {
        object: ObjectId,
    },

    /// A permanent was untapped.
    Untapped {
        object: ObjectId,
    },

    /// A player drew a card.
    CardDrawn {
        player: PlayerIndex,
        object: ObjectId,
    },

    /// A card was revealed to every player by an effect.
    CardRevealed {
        player: PlayerIndex,
        object: ObjectId,
    },

    /// A player's library was shuffled by an effect.
    LibraryShuffled { player: PlayerIndex },

    /// A turn started.
    TurnStarted {
        active_player: PlayerIndex,
        turn_number: u32,
    },
}

/// Handler function type: receives a mutable game state reference and
/// a reference to the event that fired.
pub type EventHandler = fn(&mut GameState, &GameEvent);

/// The event bus — a simple function-pointer table for event dispatch.
///
/// This lives outside `GameState` and is configured at the engine level.
/// It is NOT cloned, serialized, or part of the game snapshot.
///
/// Handlers are called synchronously in registration order when `emit()`
/// is invoked. They may mutate `GameState` (e.g., to queue triggers in
/// response to events).
pub struct EventBus {
    handlers: Vec<EventHandler>,
}

impl EventBus {
    /// Create an empty event bus with no handlers.
    pub fn new() -> Self {
        EventBus {
            handlers: Vec::new(),
        }
    }

    /// Register a handler to be called for every emitted event.
    pub fn subscribe(&mut self, handler: EventHandler) {
        self.handlers.push(handler);
    }

    /// Emit an event, calling all registered handlers in order.
    pub fn emit(&self, state: &mut GameState, event: &GameEvent) {
        for handler in &self.handlers {
            handler(state, event);
        }
    }

    /// Returns the number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// A log-only event collector that records events for testing and debugging.
/// Does not mutate game state — purely observational.
#[derive(Debug, Default)]
pub struct EventLog {
    pub events: Vec<GameEvent>,
}

impl EventLog {
    pub fn new() -> Self {
        EventLog { events: Vec::new() }
    }

    /// Record an event.
    pub fn record(&mut self, event: GameEvent) {
        self.events.push(event);
    }

    /// Get all events of a specific type.
    pub fn damage_events(&self) -> Vec<&GameEvent> {
        self.events
            .iter()
            .filter(|e| matches!(e, GameEvent::DamageDealt { .. }))
            .collect()
    }

    pub fn zone_change_events(&self) -> Vec<&GameEvent> {
        self.events
            .iter()
            .filter(|e| matches!(e, GameEvent::ZoneChange { .. }))
            .collect()
    }

    pub fn life_change_events(&self) -> Vec<&GameEvent> {
        self.events
            .iter()
            .filter(|e| matches!(e, GameEvent::LifeChanged { .. }))
            .collect()
    }

    /// Clear all recorded events.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::GameState;
    use std::sync::Arc;
    use crate::card::sample;

    #[test]
    fn test_event_bus_creation() {
        let bus = EventBus::new();
        assert_eq!(bus.handler_count(), 0);
    }

    #[test]
    fn test_event_bus_subscribe_and_emit() {
        let mut bus = EventBus::new();

        // A handler that increments player 0's life when damage is dealt
        fn damage_handler(state: &mut GameState, event: &GameEvent) {
            if let GameEvent::LifeChanged { player, .. } = event {
                // Just verify we can access state — mark the player's mana pool
                // as a side-effect indicator (harmless for testing)
                let _ = state.players[*player].life;
            }
        }

        bus.subscribe(damage_handler);
        assert_eq!(bus.handler_count(), 1);

        let db = sample::build_sample_db();
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));

        // Emit an event — should not panic
        bus.emit(
            &mut state,
            &GameEvent::LifeChanged {
                player: 0,
                old: 20,
                new: 17,
            },
        );
    }

    #[test]
    fn test_event_log() {
        let mut log = EventLog::new();

        log.record(GameEvent::DamageDealt {
            source: 1,
            target: Target::Player(0),
            amount: 3,
            is_combat: false,
        });
        log.record(GameEvent::LifeChanged {
            player: 0,
            old: 20,
            new: 17,
        });
        log.record(GameEvent::ZoneChange {
            object: 5,
            from: Zone::Hand,
            to: Zone::Stack,
        });

        assert_eq!(log.events.len(), 3);
        assert_eq!(log.damage_events().len(), 1);
        assert_eq!(log.life_change_events().len(), 1);
        assert_eq!(log.zone_change_events().len(), 1);

        log.clear();
        assert_eq!(log.events.len(), 0);
    }

    #[test]
    fn test_zone_from_zone_type() {
        use crate::card::ZoneType;

        assert_eq!(Zone::from(ZoneType::Library), Zone::Library);
        assert_eq!(Zone::from(ZoneType::Hand), Zone::Hand);
        assert_eq!(Zone::from(ZoneType::Battlefield), Zone::Battlefield);
        assert_eq!(Zone::from(ZoneType::Graveyard), Zone::Graveyard);
        assert_eq!(Zone::from(ZoneType::Exile), Zone::Exile);
        assert_eq!(Zone::from(ZoneType::Stack), Zone::Stack);
        assert_eq!(Zone::from(ZoneType::Command), Zone::Command);
    }
}
