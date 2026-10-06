//! TUI Goldfish Mode — ratatui-based terminal UI for goldfish games.
//!
//! Usage:
//!   cargo run --release --features tui --bin tui
//!   cargo run --release --features tui --bin tui -- --preset kinnan
//!   cargo run --release --features tui --bin tui -- --deck decks/kinnan.txt
//!
//! Controls:
//!   W/S     Move between zones (up/down)
//!   A/D     Move between cards within a zone (left/right)
//!   Space   Activate selected card / confirm action
//!   Escape  Cancel current action / deselect
//!   U       Undo last action
//!   Q       Quit

use std::io::{self, stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;

use mtg_gto::tui::{self, App, MenuState, UiMode, Zone};

/// Whether we are in the startup menu or playing a game.
enum Phase {
    Menu(MenuState),
    Game(App),
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // If --preset or --deck is given on CLI, skip the menu.
    let has_preset = args.iter().any(|a| a == "--preset" || a == "--deck");

    let mut phase = if has_preset {
        let (state, db) = tui::load_game(&args);
        let mut app = App::new(state, db);
        app.auto_advance();
        Phase::Game(app)
    } else {
        Phase::Menu(MenuState::new())
    };

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut should_quit = false;

    loop {
        match &phase {
            Phase::Menu(menu) => {
                terminal.draw(|f| tui::render_menu(f, menu))?;
            }
            Phase::Game(app) => {
                terminal.draw(|f| tui::ui(f, app))?;
                if app.should_quit {
                    should_quit = true;
                }
            }
        }

        if should_quit {
            break;
        }

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match &mut phase {
                    Phase::Menu(menu) => {
                        match key.code {
                            KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                                menu.move_up();
                            }
                            KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Down => {
                                menu.move_down();
                            }
                            KeyCode::Char(' ') | KeyCode::Enter => {
                                let preset = menu.selected_preset();
                                let (state, db) = tui::load_preset(preset);
                                let mut app = App::new(state, db);
                                app.auto_advance();
                                phase = Phase::Game(app);
                            }
                            KeyCode::Char('q') | KeyCode::Char('Q') => {
                                should_quit = true;
                            }
                            _ => {}
                        }
                    }
                    Phase::Game(app) => {
                        handle_key(app, key.code);
                    }
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    // Print action log on exit
    if let Phase::Game(app) = &phase {
        if !app.action_log.is_empty() {
            println!("\n=== Action Log ({} actions) ===", app.action_log.len());
            for (i, entry) in app.action_log.iter().enumerate() {
                println!("  {:>3}. {}", i + 1, entry);
            }
        }

        if app.state.unsupported_continuing_elimination() {
            println!("\nINVALID reason=unsupported_continuing_elimination");
        } else if app.state.game_over {
            match app.state.winner {
                Some(0) => println!("\nYOU WIN on turn {}!", app.state.turn_number),
                Some(_) => println!("\nYou lost on turn {}.", app.state.turn_number),
                None => println!("\nRULES DRAW."),
            }
            println!(
                "Final life: You={} Goldfish={}",
                app.state.players[0].life, app.state.players[1].life
            );
        }
    }

    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode) {
    match app.mode {
        UiMode::GameOver => {
            if code == KeyCode::Char('q') || code == KeyCode::Char('Q') {
                app.should_quit = true;
            }
        }
        UiMode::ActionSelect => match code {
            KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                let cur = app.cursor();
                if cur > 0 {
                    app.set_cursor(cur - 1);
                }
            }
            KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Down => {
                let cur = app.cursor();
                app.set_cursor(cur + 1);
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                let idx = app.cursor();
                app.execute_action(idx);
            }
            KeyCode::Esc => {
                app.mode = UiMode::Browse;
                app.active_zone = Zone::Hand;
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                app.should_quit = true;
            }
            _ => {}
        },
        UiMode::Browse => match code {
            KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                let idx = tui::ZONE_ORDER.iter().position(|&z| z == app.active_zone).unwrap_or(0);
                if idx > 0 {
                    app.active_zone = tui::ZONE_ORDER[idx - 1];
                }
            }
            KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Down => {
                let idx = tui::ZONE_ORDER.iter().position(|&z| z == app.active_zone).unwrap_or(0);
                if idx + 1 < tui::ZONE_ORDER.len() {
                    app.active_zone = tui::ZONE_ORDER[idx + 1];
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Left => {
                let cur = app.cursor();
                if cur > 0 {
                    app.set_cursor(cur - 1);
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Right => {
                let cur = app.cursor();
                app.set_cursor(cur + 1);
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if app.active_zone == Zone::Actions {
                    let idx = app.cursor();
                    app.execute_action(idx);
                } else {
                    let relevant = app.actions_for_selected();
                    if relevant.len() == 1 {
                        app.execute_action(relevant[0]);
                    } else if relevant.len() > 1 {
                        app.mode = UiMode::ActionSelect;
                        app.active_zone = Zone::Actions;
                        app.zone_cursors.insert(Zone::Actions, relevant[0]);
                    } else {
                        app.status_log.push("No actions available for this card.".into());
                    }
                }
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                app.undo();
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                app.should_quit = true;
            }
            KeyCode::Esc => {
                app.zone_cursors.insert(app.active_zone, 0);
            }
            _ => {}
        },
    }
}
