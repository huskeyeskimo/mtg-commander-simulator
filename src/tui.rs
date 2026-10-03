//! TUI rendering and application state for the goldfish pilot mode.
//!
//! This module contains the shared logic used by both the interactive TUI
//! binary and the headless snapshot generator. The rendering functions work
//! with any ratatui `Backend`, including `TestBackend` for offline rendering.

use std::collections::HashMap;
use std::sync::Arc;

use ratatui::{
    backend::TestBackend,
    prelude::*,
    widgets::*,
};

use crate::action::{legal_actions, Action};
use crate::card::{sample, CardDef, CardType, ObjectId};
use crate::game::{CardDatabase, GameState, Target};
use crate::rules;
use crate::strategy::{GoldfishStrategy, Strategy};

// ---------------------------------------------------------------------------
// Zone navigation model
// ---------------------------------------------------------------------------

/// The zones the player can navigate between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zone {
    Hand,
    Battlefield,
    CommandZone,
    Graveyard,
    Exile,
    Stack,
    Actions,
}

pub const ZONE_ORDER: &[Zone] = &[
    Zone::Hand,
    Zone::Battlefield,
    Zone::CommandZone,
    Zone::Stack,
    Zone::Graveyard,
    Zone::Exile,
    Zone::Actions,
];

/// The current UI interaction mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiMode {
    /// Normal browsing — navigate zones and cards.
    Browse,
    /// Picking an action from the action list.
    ActionSelect,
    /// Game is over.
    GameOver,
}

// ---------------------------------------------------------------------------
// Startup menu state
// ---------------------------------------------------------------------------

/// A selectable deck entry for the startup menu.
#[derive(Debug, Clone)]
pub struct DeckOption {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const DECK_OPTIONS: &[DeckOption] = &[
    DeckOption {
        key: "kinnan",
        label: "Kinnan, Bonder Prodigy",
        description: "Simic infinite mana combo",
    },
    DeckOption {
        key: "ashcoat",
        label: "Ashcoat of the Shadow Swarm",
        description: "Mono-black rats",
    },
    DeckOption {
        key: "brimaz",
        label: "Brimaz, King of Oreskos",
        description: "Mono-white tokens",
    },
    DeckOption {
        key: "flubs",
        label: "Flubs, the Fool",
        description: "Mono-red storm",
    },
    DeckOption {
        key: "thrun",
        label: "Thrun, Last Troll",
        description: "Mono-green voltron",
    },
];

/// Persistent state for the startup menu.
pub struct MenuState {
    pub cursor: usize,
}

impl MenuState {
    pub fn new() -> Self {
        MenuState { cursor: 0 }
    }

    pub fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < DECK_OPTIONS.len() {
            self.cursor += 1;
        }
    }

    pub fn selected_preset(&self) -> &'static str {
        DECK_OPTIONS[self.cursor].key
    }
}

/// TUI application state.
pub struct App {
    pub state: GameState,
    pub db: CardDatabase,
    pub goldfish: GoldfishStrategy,
    pub actions_taken: u32,
    pub undo_stack: Vec<GameState>,
    pub action_log: Vec<String>,

    // UI state
    pub mode: UiMode,
    pub active_zone: Zone,
    /// Per-zone selection index.
    pub zone_cursors: HashMap<Zone, usize>,
    /// Cached legal actions for current game state.
    pub cached_actions: Vec<Action>,
    /// Log messages shown in the status area.
    pub status_log: Vec<String>,

    pub should_quit: bool,
}

const MAX_TURNS: u32 = 20;
const MAX_ACTIONS: u32 = 10_000;

impl App {
    pub fn new(state: GameState, db: CardDatabase) -> Self {
        let cached_actions = legal_actions(&state);
        let mut zone_cursors = HashMap::new();
        for &z in ZONE_ORDER {
            zone_cursors.insert(z, 0);
        }
        App {
            state,
            db,
            goldfish: GoldfishStrategy,
            actions_taken: 0,
            undo_stack: Vec::new(),
            action_log: Vec::new(),

            mode: UiMode::Browse,
            active_zone: Zone::Hand,
            zone_cursors,
            cached_actions,
            status_log: vec!["Game started. WASD to navigate, Space to act, Q to quit.".into()],

            should_quit: false,
        }
    }

    /// Number of items in the given zone.
    pub fn zone_len(&self, zone: Zone) -> usize {
        match zone {
            Zone::Hand => self.state.players[0].hand.len(),
            Zone::Battlefield => self.state.permanents_controlled_by(0).len(),
            Zone::CommandZone => self.state.players[0].command_zone.len(),
            Zone::Graveyard => self.state.players[0].graveyard.len(),
            Zone::Exile => self.state.players[0].exile.len(),
            Zone::Stack => self.state.stack.len(),
            Zone::Actions => self.cached_actions.len(),
        }
    }

    pub fn cursor(&self) -> usize {
        *self.zone_cursors.get(&self.active_zone).unwrap_or(&0)
    }

    pub fn set_cursor(&mut self, val: usize) {
        let len = self.zone_len(self.active_zone);
        let clamped = if len == 0 { 0 } else { val.min(len - 1) };
        self.zone_cursors.insert(self.active_zone, clamped);
    }

    pub fn refresh_actions(&mut self) {
        self.cached_actions = legal_actions(&self.state);
        let len = self.cached_actions.len();
        if len > 0 {
            let cur = *self.zone_cursors.get(&Zone::Actions).unwrap_or(&0);
            self.zone_cursors.insert(Zone::Actions, cur.min(len - 1));
        } else {
            self.zone_cursors.insert(Zone::Actions, 0);
        }
    }

    /// Auto-advance: handle goldfish turns and auto-pass situations.
    pub fn auto_advance(&mut self) {
        let mut passes = 0;
        while !self.state.game_over
            && self.state.turn_number <= MAX_TURNS
            && self.actions_taken < MAX_ACTIONS
            && passes < 200
        {
            // Fast-forward entire opponent turn without calling legal_actions().
            if self.state.active_player != 0
                && self.state.pending_copy_order.as_ref().is_none_or(|pending| pending.controller() != 0) {
                let ff = rules::fast_forward_goldfish_turn_until_copy_choice(&mut self.state, 0);
                self.actions_taken += ff;
                passes += ff as usize;
                continue;
            }

            let player = self.state.priority_player;
            let actions = legal_actions(&self.state);

            if actions.is_empty()
                || (actions.len() == 1 && actions[0] == Action::PassPriority)
            {
                rules::apply_action(&mut self.state, &Action::PassPriority);
                self.actions_taken += 1;
                passes += 1;
                continue;
            }

            if player != 0 {
                let action = self.goldfish.choose_action(&self.state, player);
                rules::apply_action(&mut self.state, &action);
                self.actions_taken += 1;
                passes += 1;
                continue;
            }

            break;
        }

        self.refresh_actions();

        if self.state.game_over {
            self.mode = UiMode::GameOver;
            let msg = match self.state.winner {
                Some(0) => format!("YOU WIN on turn {}!", self.state.turn_number),
                Some(_) => format!("You lost on turn {}.", self.state.turn_number),
                None => "Draw (turn limit reached).".into(),
            };
            self.status_log.push(msg);
        }
    }

    /// Execute the chosen action index.
    pub fn execute_action(&mut self, idx: usize) {
        if idx >= self.cached_actions.len() {
            return;
        }

        let action = self.cached_actions[idx].clone();
        let desc = format_action_rich(&self.state, &action, &self.db);

        self.undo_stack.push(self.state.clone());

        self.action_log.push(format!(
            "T{} {:?}: {}",
            self.state.turn_number, self.state.phase, desc
        ));

        self.status_log.push(format!("> {}", desc));

        rules::apply_action(&mut self.state, &action);
        self.actions_taken += 1;

        rules::check_state_based_actions(&mut self.state);

        self.auto_advance();

        self.mode = UiMode::Browse;
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.state = prev;
            if let Some(last) = self.action_log.pop() {
                self.status_log.push(format!("Undid: {}", last));
            }
            self.actions_taken = self.actions_taken.saturating_sub(1);
            self.auto_advance();
        } else {
            self.status_log.push("Nothing to undo.".into());
        }
    }

    /// Find actions relevant to the currently selected object in the current zone.
    pub fn actions_for_selected(&self) -> Vec<usize> {
        let zone = self.active_zone;
        let cursor = self.cursor();

        let obj_id: Option<ObjectId> = match zone {
            Zone::Hand => self.state.players[0].hand.get(cursor).copied(),
            Zone::Battlefield => {
                let perms = self.state.permanents_controlled_by(0);
                perms.get(cursor).copied()
            }
            Zone::CommandZone => self.state.players[0].command_zone.get(cursor).copied(),
            Zone::Graveyard => self.state.players[0].graveyard.get(cursor).copied(),
            _ => None,
        };

        let Some(oid) = obj_id else {
            return Vec::new();
        };

        self.cached_actions
            .iter()
            .enumerate()
            .filter(|(_, a)| action_involves_object(a, oid))
            .map(|(i, _)| i)
            .collect()
    }
}

/// Check if an action involves a specific object.
pub fn action_involves_object(action: &Action, oid: ObjectId) -> bool {
    match action {
        Action::PlayLand { object_id }
        | Action::CastSpell { object_id, .. }
        | Action::CastCommander { object_id, .. }
        | Action::ActivateManaAbility { object_id, .. }
        | Action::ActivateAbility { object_id, .. }
        | Action::ActivateLoyalty { object_id, .. }
        | Action::CastFromGraveyard { object_id, .. }
        | Action::Discard { object_id }
        | Action::MulliganBottomCard { object_id } => *object_id == oid,
        Action::DeclareAttackers { attackers } => attackers.contains(&oid),
        Action::Equip { equipment_id, target_id } => {
            *equipment_id == oid || *target_id == oid
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the startup menu for selecting mode and deck.
pub fn render_menu(f: &mut Frame, menu: &MenuState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // title
            Constraint::Length(3),  // mode
            Constraint::Length(1),  // spacer
            Constraint::Length(2),  // deck label
            Constraint::Min(5),    // deck list
        ])
        .margin(2)
        .split(outer[0]);

    // Title
    let title = Paragraph::new("MTG Commander Goldfish Simulator")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(title, inner[0]);

    // Mode (only goldfish for now)
    let mode = Paragraph::new("  Mode: Goldfish Pilot")
        .style(Style::default().fg(Color::Green));
    f.render_widget(mode, inner[1]);

    // Deck label
    let deck_label = Paragraph::new("  Select Commander Deck:")
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(deck_label, inner[3]);

    // Deck list
    let items: Vec<ListItem> = DECK_OPTIONS
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let selected = i == menu.cursor;
            let marker = if selected { "> " } else { "  " };
            let label_style = if selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let line = Line::from(vec![
                Span::styled(marker, Style::default().fg(Color::Cyan)),
                Span::styled(opt.label, label_style),
                Span::styled(format!("  ({})", opt.description), Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().padding(Padding::new(2, 0, 0, 0)));
    f.render_widget(list, inner[4]);

    // Footer
    let footer = Paragraph::new(" W/S: Navigate | Enter/Space: Select | Q: Quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(footer, outer[1]);
}

pub fn ui(f: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(6),
        ])
        .split(f.area());

    render_header(f, app, outer[0]);
    render_main(f, app, outer[1]);
    render_status(f, app, outer[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let phase_str = format!("{:?}", app.state.phase);
    let pool = &app.state.players[0].mana_pool;
    let mana_str = format_mana_pool(pool);

    let opp_life = app.state.players[1].life;

    let header_text = format!(
        " Turn {} | Phase: {} | Life: {} | Opp Life: {} | Mana: {} | Actions: {} ",
        app.state.turn_number,
        phase_str,
        app.state.players[0].life,
        opp_life,
        if mana_str.is_empty() { "empty".into() } else { mana_str },
        app.actions_taken,
    );

    let mode_str = match &app.mode {
        UiMode::Browse => "[BROWSE]",
        UiMode::ActionSelect => "[SELECT ACTION]",
        UiMode::GameOver => "[GAME OVER]",
    };

    let block = Block::default()
        .title(format!(" MTG Goldfish TUI {} ", mode_str))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let text = Paragraph::new(header_text)
        .block(block)
        .style(Style::default().fg(Color::White));

    f.render_widget(text, area);
}

fn render_main(f: &mut Frame, app: &App, area: Rect) {
    let main_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(area);

    render_zones(f, app, main_cols[0]);
    render_actions(f, app, main_cols[1]);
}

fn render_zones(f: &mut Frame, app: &App, area: Rect) {
    let zone_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(4),
            Constraint::Min(4),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    render_zone_hand(f, app, zone_layout[0]);
    render_zone_battlefield(f, app, zone_layout[1]);
    render_zone_command(f, app, zone_layout[2]);
    render_zone_stack(f, app, zone_layout[3]);
    render_zone_graveyard(f, app, zone_layout[4]);
    render_zone_exile(f, app, zone_layout[5]);
}

fn zone_border_style(app: &App, zone: Zone) -> Style {
    if app.active_zone == zone {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_zone_hand(f: &mut Frame, app: &App, area: Rect) {
    let hand = &app.state.players[0].hand;
    let cursor = if app.active_zone == Zone::Hand { Some(app.cursor()) } else { None };

    let items: Vec<Line> = hand.iter().enumerate().map(|(i, &oid)| {
        let text = format_card_in_hand(&app.state, &app.db, oid);
        let style = if cursor == Some(i) {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        Line::styled(text, style)
    }).collect();

    let block = Block::default()
        .title(format!(" Hand ({}) ", hand.len()))
        .borders(Borders::ALL)
        .border_style(zone_border_style(app, Zone::Hand));

    let para = Paragraph::new(items)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn render_zone_battlefield(f: &mut Frame, app: &App, area: Rect) {
    let perms = app.state.permanents_controlled_by(0);
    let cursor = if app.active_zone == Zone::Battlefield { Some(app.cursor()) } else { None };

    let items: Vec<Line> = perms.iter().enumerate().map(|(i, &oid)| {
        let text = format_card_on_battlefield(&app.state, &app.db, oid);
        let style = if cursor == Some(i) {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            let inst = &app.state.objects[&oid];
            if inst.tapped {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Green)
            }
        };
        Line::styled(text, style)
    }).collect();

    let block = Block::default()
        .title(format!(" Battlefield ({}) ", perms.len()))
        .borders(Borders::ALL)
        .border_style(zone_border_style(app, Zone::Battlefield));

    let para = Paragraph::new(items)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn render_zone_command(f: &mut Frame, app: &App, area: Rect) {
    let cz = &app.state.players[0].command_zone;
    let cursor = if app.active_zone == Zone::CommandZone { Some(app.cursor()) } else { None };

    let items: Vec<Line> = cz.iter().enumerate().map(|(i, &oid)| {
        let name = card_name(&app.state, oid, &app.db);
        let tax = app.state.players[0].commander_tax;
        let text = if tax > 0 {
            format!("{} (tax: {})", name, tax * 2)
        } else {
            name
        };
        let style = if cursor == Some(i) {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Magenta)
        };
        Line::styled(text, style)
    }).collect();

    let block = Block::default()
        .title(format!(" Command Zone ({}) ", cz.len()))
        .borders(Borders::ALL)
        .border_style(zone_border_style(app, Zone::CommandZone));

    let para = Paragraph::new(items)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn render_zone_stack(f: &mut Frame, app: &App, area: Rect) {
    let stack = &app.state.stack;
    let cursor = if app.active_zone == Zone::Stack { Some(app.cursor()) } else { None };

    let items: Vec<Line> = stack.iter().enumerate().rev().map(|(i, entry)| {
        let desc = format_stack_entry(&app.state, entry, &app.db);
        let style = if cursor == Some(i) {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };
        Line::styled(desc, style)
    }).collect();

    let block = Block::default()
        .title(format!(" Stack ({}) ", stack.len()))
        .borders(Borders::ALL)
        .border_style(zone_border_style(app, Zone::Stack));

    let para = Paragraph::new(items)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn render_zone_graveyard(f: &mut Frame, app: &App, area: Rect) {
    let gy = &app.state.players[0].graveyard;
    let cursor = if app.active_zone == Zone::Graveyard { Some(app.cursor()) } else { None };

    let items: Vec<Line> = gy.iter().enumerate().map(|(i, &oid)| {
        let name = card_name(&app.state, oid, &app.db);
        let style = if cursor == Some(i) {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Line::styled(name, style)
    }).collect();

    let block = Block::default()
        .title(format!(" Graveyard ({}) ", gy.len()))
        .borders(Borders::ALL)
        .border_style(zone_border_style(app, Zone::Graveyard));

    let para = Paragraph::new(items)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn render_zone_exile(f: &mut Frame, app: &App, area: Rect) {
    let ex = &app.state.players[0].exile;
    let cursor = if app.active_zone == Zone::Exile { Some(app.cursor()) } else { None };

    let items: Vec<Line> = ex.iter().enumerate().map(|(i, &oid)| {
        let name = card_name(&app.state, oid, &app.db);
        let style = if cursor == Some(i) {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        Line::styled(name, style)
    }).collect();

    let block = Block::default()
        .title(format!(" Exile ({}) ", ex.len()))
        .borders(Borders::ALL)
        .border_style(zone_border_style(app, Zone::Exile));

    let para = Paragraph::new(items)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn render_actions(f: &mut Frame, app: &App, area: Rect) {
    let action_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),
            Constraint::Length(8),
        ])
        .split(area);

    let cursor = if app.active_zone == Zone::Actions || app.mode == UiMode::ActionSelect {
        Some(*app.zone_cursors.get(&Zone::Actions).unwrap_or(&0))
    } else {
        None
    };

    let items: Vec<Line> = app.cached_actions.iter().enumerate().map(|(i, action)| {
        let desc = format_action_short(&app.state, action, &app.db);
        let style = if cursor == Some(i) {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        Line::styled(format!(" {} {}", if cursor == Some(i) { ">" } else { " " }, desc), style)
    }).collect();

    let action_border = if app.active_zone == Zone::Actions || app.mode == UiMode::ActionSelect {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(format!(" Actions ({}) ", app.cached_actions.len()))
        .borders(Borders::ALL)
        .border_style(action_border);

    let para = Paragraph::new(items)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((cursor.unwrap_or(0).saturating_sub(4) as u16, 0));

    f.render_widget(para, action_layout[0]);

    render_detail_panel(f, app, action_layout[1]);
}

fn render_detail_panel(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    let selected_oid = get_selected_object(app);
    if let Some(oid) = selected_oid {
        if let Some(inst) = app.state.objects.get(&oid) {
            if let Some(def) = app.db.get(inst.card_def_id) {
                lines.push(Line::styled(
                    def.name.clone(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ));
                let cost_str = def.mana_cost.as_ref()
                    .map(|c| format!("{}", c))
                    .unwrap_or_else(|| "—".into());
                lines.push(Line::styled(
                    format!("{} | {}", format_card_type(def), cost_str),
                    Style::default().fg(Color::Gray),
                ));
                if def.is_creature() {
                    let p = app.state.effective_power(oid);
                    let t = app.state.effective_toughness(oid);
                    lines.push(Line::styled(
                        format!("P/T: {}/{}", p, t),
                        Style::default().fg(Color::Green),
                    ));
                }
                if !def.oracle_text.is_empty() {
                    let text = if def.oracle_text.len() > 80 {
                        format!("{}...", &def.oracle_text[..77])
                    } else {
                        def.oracle_text.clone()
                    };
                    lines.push(Line::styled(text, Style::default().fg(Color::DarkGray)));
                }
            }
        }
    }

    lines.push(Line::styled(
        format!("Library: {} cards", app.state.players[0].library.len()),
        Style::default().fg(Color::Blue),
    ));

    let block = Block::default()
        .title(" Detail ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Log ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let max_lines = area.height.saturating_sub(2) as usize;
    let start = app.status_log.len().saturating_sub(max_lines);
    let items: Vec<Line> = app.status_log[start..].iter().map(|s| {
        Line::styled(s.clone(), Style::default().fg(Color::Gray))
    }).collect();

    let para = Paragraph::new(items)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn get_selected_object(app: &App) -> Option<ObjectId> {
    let cursor = app.cursor();
    match app.active_zone {
        Zone::Hand => app.state.players[0].hand.get(cursor).copied(),
        Zone::Battlefield => {
            let perms = app.state.permanents_controlled_by(0);
            perms.get(cursor).copied()
        }
        Zone::CommandZone => app.state.players[0].command_zone.get(cursor).copied(),
        Zone::Graveyard => app.state.players[0].graveyard.get(cursor).copied(),
        Zone::Exile => app.state.players[0].exile.get(cursor).copied(),
        _ => None,
    }
}

pub fn card_name(state: &GameState, obj_id: ObjectId, db: &CardDatabase) -> String {
    state.objects.get(&obj_id)
        .and_then(|inst| db.get(inst.card_def_id))
        .map(|d| d.name.clone())
        .unwrap_or_else(|| format!("obj#{}", obj_id))
}

pub fn format_card_type(def: &CardDef) -> String {
    def.card_types.iter().map(|t| match t {
        CardType::Creature => "Creature",
        CardType::Instant => "Instant",
        CardType::Sorcery => "Sorcery",
        CardType::Enchantment => "Enchantment",
        CardType::Artifact => "Artifact",
        CardType::Land => "Land",
        CardType::Kindred => "Kindred",
        CardType::Planeswalker => "Planeswalker",
    }).collect::<Vec<_>>().join(" ")
}

pub fn format_card_in_hand(state: &GameState, db: &CardDatabase, oid: ObjectId) -> String {
    let inst = &state.objects[&oid];
    let Some(def) = db.get(inst.card_def_id) else {
        return format!("obj#{}", oid);
    };

    let cost = def.mana_cost.as_ref()
        .map(|c| format!(" {}", c))
        .unwrap_or_default();

    let type_str = format_card_type(def);

    let stats = if def.is_creature() {
        format!(" {}/{}", def.power.unwrap_or(0), def.toughness.unwrap_or(0))
    } else {
        String::new()
    };

    format!("{}{} - {}{}", def.name, cost, type_str, stats)
}

pub fn format_card_on_battlefield(state: &GameState, db: &CardDatabase, oid: ObjectId) -> String {
    let inst = &state.objects[&oid];
    let Some(def) = db.get(inst.card_def_id) else {
        return format!("obj#{}", oid);
    };

    let mut parts = vec![def.name.clone()];

    if def.is_creature() {
        let p = state.effective_power(oid);
        let t = state.effective_toughness(oid);
        parts.push(format!("{}/{}", p, t));
        if inst.damage_marked > 0 {
            parts.push(format!("[{} dmg]", inst.damage_marked));
        }
        if inst.plus_counters != 0 {
            parts.push(format!("+{} counters", inst.plus_counters));
        }
    }

    if inst.tapped {
        parts.push("(T)".into());
    }
    if inst.summoning_sick && def.is_creature() {
        parts.push("(sick)".into());
    }

    parts.join(" ")
}

pub fn format_stack_entry(state: &GameState, entry: &crate::game::StackEntry, db: &CardDatabase) -> String {
    match &entry.source {
        crate::game::StackSource::Spell(obj_id) => {
            let name = card_name(state, *obj_id, db);
            format!("Spell: {}", name)
        }
        crate::game::StackSource::SpellCopy { definition } => {
            format!("Spell copy: {} (stack #{})", definition.name, entry.id)
        }
        crate::game::StackSource::ActivatedAbility { source_id, ability_index } => {
            let name = card_name(state, *source_id, db);
            format!("Ability: {} #{}", name, ability_index)
        }
        crate::game::StackSource::TriggeredAbility { ability_index, context, .. } => {
            let name = db.get(context.source_card_id)
                .map(|definition| definition.name.as_str()).unwrap_or("?");
            format!("Trigger: {} #{}", name, ability_index)
        }
    }
}

pub fn format_mana_pool(pool: &crate::mana::ManaPool) -> String {
    let mut parts = Vec::new();
    if pool.white > 0 { parts.push(format!("{}W", pool.white)); }
    if pool.blue > 0 { parts.push(format!("{}U", pool.blue)); }
    if pool.black > 0 { parts.push(format!("{}B", pool.black)); }
    if pool.red > 0 { parts.push(format!("{}R", pool.red)); }
    if pool.green > 0 { parts.push(format!("{}G", pool.green)); }
    if pool.colorless > 0 { parts.push(format!("{}C", pool.colorless)); }
    parts.join(" ")
}

pub fn format_action_short(state: &GameState, action: &Action, db: &CardDatabase) -> String {
    match action {
        Action::ChooseNextCopy { item_index } => {
            let item = &state.pending_copy_order.as_ref().unwrap().items()[*item_index];
            let target = format_copy_targets(state, item.targets(), db);
            format!("Place {} copy next (bottom to top){}", item.definition().name,
                if target.is_empty() { String::new() } else { format!(" -> {}", target) })
        }
        Action::PassPriority => "Pass priority".into(),
        Action::PlayLand { object_id } => {
            format!("Play land: {}", card_name(state, *object_id, db))
        }
        Action::CastSpell { object_id, targets } => {
            let name = card_name(state, *object_id, db);
            let tgt = format_targets(state, targets, db);
            if tgt.is_empty() {
                format!("Cast: {}", name)
            } else {
                format!("Cast: {} -> {}", name, tgt)
            }
        }
        Action::CastCommander { object_id, targets } => {
            let name = card_name(state, *object_id, db);
            let tgt = format_targets(state, targets, db);
            if tgt.is_empty() {
                format!("Cast cmdr: {}", name)
            } else {
                format!("Cast cmdr: {} -> {}", name, tgt)
            }
        }
        Action::ActivateManaAbility { object_id, ability_index } => {
            let name = card_name(state, *object_id, db);
            let inst = &state.objects[object_id];
            let mana_desc = db.get(inst.card_def_id)
                .and_then(|d| d.mana_abilities.get(*ability_index))
                .map(|ma| format!("{:?}", ma))
                .unwrap_or_else(|| "?".into());
            format!("Tap: {} ({})", name, mana_desc)
        }
        Action::ActivateAbility { object_id, ability_index, targets } => {
            let name = card_name(state, *object_id, db);
            let tgt = format_targets(state, targets, db);
            if tgt.is_empty() {
                format!("Activate: {} #{}", name, ability_index)
            } else {
                format!("Activate: {} #{} -> {}", name, ability_index, tgt)
            }
        }
        Action::DeclareAttackers { attackers } => {
            if attackers.is_empty() {
                "Attack: none".into()
            } else {
                let names: Vec<String> = attackers.iter().map(|&id| {
                    card_name(state, id, db)
                }).collect();
                format!("Attack: [{}]", names.join(", "))
            }
        }
        Action::DeclareBlockers { blocks } => {
            if blocks.is_empty() {
                "Block: none".into()
            } else {
                format!("Block with {} creatures", blocks.len())
            }
        }
        Action::Discard { object_id } => {
            format!("Discard: {}", card_name(state, *object_id, db))
        }
        Action::MulliganKeep => "Keep hand".into(),
        Action::MulliganMulligan => "Mulligan".into(),
        Action::MulliganBottomCard { object_id } => {
            format!("Bottom: {}", card_name(state, *object_id, db))
        }
        Action::ChooseTutorTarget { card_id } => {
            let name = db.get(*card_id)
                .map(|d| d.name.as_str())
                .unwrap_or("?");
            format!("Tutor: {}", name)
        }
        Action::ActivateLoyalty { object_id, ability_index } => {
            let name = card_name(state, *object_id, db);
            format!("Loyalty: {} #{}", name, ability_index)
        }
        Action::Equip { equipment_id, target_id } => {
            format!("Equip {} -> {}", card_name(state, *equipment_id, db), card_name(state, *target_id, db))
        }
        Action::CastFromGraveyard { object_id, .. } => {
            format!("Flashback: {}", card_name(state, *object_id, db))
        }
        Action::OrderTriggers { ordering } => format!("Order {} triggers", ordering.len()),
        Action::ChooseReplacementOrder { ordering } => format!("Order {} replacements", ordering.len()),
        Action::OrderDamageAssignment { .. } => "Assign damage".into(),
        Action::Concede => "Concede".into(),
        Action::ActivateMacro { combo_id } => format!("Combo #{}", combo_id),
        Action::EndTurn => "End turn".into(),
        Action::PlayLandFromGraveyard { object_id } => {
            let name = state.card_db().get(state.objects[object_id].card_def_id)
                .map(|d| d.name.as_str()).unwrap_or("?");
            format!("Play from GY: {}", name)
        }
    }
}

pub fn format_action_rich(state: &GameState, action: &Action, db: &CardDatabase) -> String {
    format_action_short(state, action, db)
}

pub fn format_targets(state: &GameState, targets: &[Target], db: &CardDatabase) -> String {
    if targets.is_empty() {
        return String::new();
    }
    let descs: Vec<String> = targets.iter().map(|t| match t {
        Target::Player(p) => {
            if *p == 0 { "You".into() } else { "Goldfish".into() }
        }
        Target::Object(id) => card_name(state, *id, db),
        Target::StackEntry(id) => state.stack.iter().find(|entry| entry.id == *id)
            .and_then(|entry| match entry.source {
                crate::game::StackSource::Spell(object) => Some(format!("{} (stack #{})", card_name(state, object, db), id)),
                crate::game::StackSource::SpellCopy { ref definition } => Some(format!("{} copy (stack #{})", definition.name, id)),
                _ => None,
            }).unwrap_or_else(|| format!("Stack #{} (gone)", id)),
    }).collect();
    descs.join(", ")
}

fn format_copy_targets(state: &GameState, targets: &[Target], db: &CardDatabase) -> String {
    targets.iter().map(|target| match target {
        Target::Object(id) => {
            let name = card_name(state, *id, db);
            let Some(inst) = state.objects.get(id) else { return format!("{name} (gone)"); };
            let mut details = vec![if inst.tapped { "tapped".to_string() } else { "untapped".to_string() },
                if inst.summoning_sick { "summoning sick".to_string() } else { "ready".to_string() }];
            if inst.damage_marked > 0 { details.push(format!("{} damage", inst.damage_marked)); }
            if inst.plus_counters != 0 { details.push(format!("{} +1/+1 counters", inst.plus_counters)); }
            if inst.minus_counters != 0 { details.push(format!("{} -1/-1 counters", inst.minus_counters)); }
            format!("{name} #{} (P{}, {})", id, inst.controller + 1, details.join(", "))
        }
        _ => format_targets(state, std::slice::from_ref(target), db),
    }).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod copy_target_label_tests {
    use super::*;
    use crate::card::{CardDef, CardType, ZoneType};

    #[test]
    fn same_name_targets_show_relevant_state() {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 983000, name: "Goblin".into(),
            card_types: vec![CardType::Creature], ..Default::default() });
        let mut state = GameState::new(2);
        let first = state.create_card_in_zone(983000, 0, ZoneType::Battlefield);
        let second = state.create_card_in_zone(983000, 0, ZoneType::Battlefield);
        state.objects.get_mut(&second).unwrap().tapped = true;
        state.objects.get_mut(&second).unwrap().damage_marked = 1;
        assert_ne!(format_copy_targets(&state, &[Target::Object(first)], &db),
            format_copy_targets(&state, &[Target::Object(second)], &db));
        state.objects.get_mut(&second).unwrap().tapped = false;
        state.objects.get_mut(&second).unwrap().damage_marked = 0;
        state.objects.get_mut(&second).unwrap().summoning_sick = false;
        let first_label = format_copy_targets(&state, &[Target::Object(first)], &db);
        let second_label = format_copy_targets(&state, &[Target::Object(second)], &db);
        assert!(first_label.contains("summoning sick"));
        assert!(second_label.contains("ready"));
        assert_ne!(first_label, second_label);
    }
}

// ---------------------------------------------------------------------------
// Deck loading
// ---------------------------------------------------------------------------

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

/// Build a GameState from one shared, validated Commander resolution.
fn build_commander_game(resolved: crate::deck_loader::ResolvedDeck) -> (GameState, CardDatabase) {
    let crate::deck_loader::ResolvedDeck { db, setup_cards, commander, .. } = resolved;
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(db.clone()));
    rules::setup_commander_game(&mut state, &setup_cards, &setup_cards, commander, commander);
    (state, db)
}

/// Load a menu preset. The menu only exposes recognized built-in names.
pub fn load_preset(preset: &str) -> (GameState, CardDatabase) {
    if preset == "red" || preset == "green" {
        let db = sample::build_sample_db();
        let deck = if preset == "red" { sample::red_aggro_deck() } else { sample::green_stompy_deck() };
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db.clone()));
        rules::setup_game(&mut state, &deck, &deck);
        return (state, db);
    }
    let resolved = crate::deck_loader::resolve_preset(preset)
        .unwrap_or_else(|e| { eprintln!("Error loading preset: {e}"); std::process::exit(1) });
    if !resolved.benchmark_ready() {
        eprintln!("Goblin Storm TUI goldfish is blocked: {} uncertified requirements. Use goldfish --coverage for reasons.", resolved.readiness_reasons.len());
        std::process::exit(2);
    }
    build_commander_game(resolved)
}

pub fn load_game(args: &[String]) -> (GameState, CardDatabase) {
    let preset = get_arg(args, "--preset").unwrap_or_else(|| "kinnan".to_string());
    if get_arg(args, "--deck").is_none() && (preset == "red" || preset == "green") { return load_preset(&preset); }
    let resolved = resolve_deck_args(args)
        .unwrap_or_else(|e| { eprintln!("Error loading deck: {e}"); std::process::exit(1) });
    if !resolved.benchmark_ready() {
        eprintln!("Goblin Storm TUI goldfish is blocked: {} uncertified requirements. Use goldfish --coverage for reasons.", resolved.readiness_reasons.len());
        std::process::exit(2);
    }
    build_commander_game(resolved)
}

/// Inspect the same resolved cards as the CLI without starting an uncertified game.
pub fn resolve_deck_args(args: &[String]) -> Result<crate::deck_loader::ResolvedDeck, String> {
    if let Some(path) = get_arg(args, "--deck") {
        crate::deck_loader::resolve_file(std::path::Path::new(&path))
    } else {
        let preset = get_arg(args, "--preset").unwrap_or_else(|| "kinnan".to_string());
        crate::deck_loader::resolve_preset(&preset)
    }
}

// ---------------------------------------------------------------------------
// SVG rendering from ratatui Buffer
// ---------------------------------------------------------------------------

/// Render a ratatui Buffer to an SVG string with colored cells.
pub fn buffer_to_svg(buf: &Buffer) -> String {
    let width = buf.area.width as usize;
    let height = buf.area.height as usize;

    let cell_w = 8; // px per character cell
    let cell_h = 16; // px per character cell
    let svg_w = width * cell_w;
    let svg_h = height * cell_h;

    let mut svg = String::new();
    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
        svg_w, svg_h, svg_w, svg_h
    ));
    svg.push('\n');

    // Background
    svg.push_str(&format!(
        "<rect width=\"{}\" height=\"{}\" fill=\"#1e1e2e\"/>",
        svg_w, svg_h
    ));
    svg.push('\n');

    // Style
    svg.push_str("<style>text { font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', 'Consolas', monospace; font-size: 13px; }</style>");
    svg.push('\n');

    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x as u16, y as u16)];
            let ch = cell.symbol();
            if ch == " " && cell.bg == Color::Reset {
                continue; // skip empty cells for smaller SVG
            }

            let px = x * cell_w;
            let py = y * cell_h;

            // Background rectangle if non-default
            let bg = color_to_css(cell.bg);
            if bg != "transparent" {
                svg.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
                    px, py, cell_w, cell_h, bg
                ));
                svg.push('\n');
            }

            // Character
            if ch != " " {
                let fg = color_to_css_fg(cell.fg);
                let bold = cell.modifier.contains(Modifier::BOLD);
                let weight = if bold { r#" font-weight="bold""# } else { "" };

                // Escape XML entities
                let escaped = match ch {
                    "<" => "&lt;",
                    ">" => "&gt;",
                    "&" => "&amp;",
                    "\"" => "&quot;",
                    "'" => "&apos;",
                    other => other,
                };

                svg.push_str(&format!(
                    r#"<text x="{}" y="{}" fill="{}"{} dominant-baseline="text-before-edge">{}</text>"#,
                    px, py, fg, weight, escaped
                ));
                svg.push('\n');
            }
        }
    }

    svg.push_str("</svg>\n");
    svg
}

fn color_to_css(c: Color) -> &'static str {
    match c {
        Color::Reset => "transparent",
        Color::Black => "#1e1e2e",
        Color::Red => "#f38ba8",
        Color::Green => "#a6e3a1",
        Color::Yellow => "#f9e2af",
        Color::Blue => "#89b4fa",
        Color::Magenta => "#cba6f7",
        Color::Cyan => "#94e2d5",
        Color::Gray => "#a6adc8",
        Color::DarkGray => "#585b70",
        Color::LightRed => "#f38ba8",
        Color::LightGreen => "#a6e3a1",
        Color::LightYellow => "#f9e2af",
        Color::LightBlue => "#89b4fa",
        Color::LightMagenta => "#cba6f7",
        Color::LightCyan => "#94e2d5",
        Color::White => "#cdd6f4",
        _ => "#cdd6f4",
    }
}

fn color_to_css_fg(c: Color) -> &'static str {
    match c {
        Color::Reset => "#cdd6f4",
        _ => color_to_css(c),
    }
}

// ---------------------------------------------------------------------------
// Snapshot generation
// ---------------------------------------------------------------------------

/// Render the current TUI state to an SVG string at the given terminal size.
pub fn render_snapshot(app: &App, cols: u16, rows: u16) -> String {
    let backend = TestBackend::new(cols, rows);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    buffer_to_svg(&buf)
}

/// Scenario descriptor for snapshot generation.
pub struct Scenario {
    pub name: String,
    pub description: String,
}

/// Generate snapshots for a series of game states using the greedy strategy
/// to auto-play actions. Returns (scenario_name, svg_content) pairs.
pub fn generate_snapshots(preset: &str, cols: u16, rows: u16) -> Vec<(Scenario, String)> {
    let args = vec![
        String::new(), // argv[0]
        "--preset".into(),
        preset.into(),
    ];
    let (state, db) = load_game(&args);
    let mut app = App::new(state, db);
    app.auto_advance();

    let mut snapshots = Vec::new();

    // Snapshot 1: Initial game state (opening hand)
    snapshots.push((
        Scenario {
            name: format!("{}_01_opening_hand", preset),
            description: "Opening hand after mulligan decisions".into(),
        },
        render_snapshot(&app, cols, rows),
    ));

    // Snapshot 2: Navigate to battlefield zone
    app.active_zone = Zone::Battlefield;
    snapshots.push((
        Scenario {
            name: format!("{}_02_battlefield_focus", preset),
            description: "Battlefield zone focused".into(),
        },
        render_snapshot(&app, cols, rows),
    ));

    // Snapshot 3: Navigate to actions zone
    app.active_zone = Zone::Actions;
    snapshots.push((
        Scenario {
            name: format!("{}_03_actions_panel", preset),
            description: "Actions panel focused".into(),
        },
        render_snapshot(&app, cols, rows),
    ));

    // Snapshot 4: Select first card in hand (highlight)
    app.active_zone = Zone::Hand;
    app.set_cursor(0);
    snapshots.push((
        Scenario {
            name: format!("{}_04_card_selected", preset),
            description: "First card in hand selected (highlighted)".into(),
        },
        render_snapshot(&app, cols, rows),
    ));

    // Snapshot 5: Play a few actions and show mid-game state
    // Execute up to 5 actions using a greedy approach
    let greedy = crate::strategy::GreedyStrategy;
    for _ in 0..5 {
        if app.state.game_over || app.cached_actions.is_empty() {
            break;
        }
        let action = greedy.choose_action(&app.state, 0);
        if let Some(idx) = app.cached_actions.iter().position(|a| *a == action) {
            app.execute_action(idx);
        } else if !app.cached_actions.is_empty() {
            app.execute_action(0);
        }
    }
    app.active_zone = Zone::Battlefield;
    app.set_cursor(0);
    snapshots.push((
        Scenario {
            name: format!("{}_05_mid_game", preset),
            description: "Mid-game state after several actions".into(),
        },
        render_snapshot(&app, cols, rows),
    ));

    // Snapshot 6: Command zone view (for commander decks)
    app.active_zone = Zone::CommandZone;
    snapshots.push((
        Scenario {
            name: format!("{}_06_command_zone", preset),
            description: "Command zone view".into(),
        },
        render_snapshot(&app, cols, rows),
    ));

    snapshots
}
