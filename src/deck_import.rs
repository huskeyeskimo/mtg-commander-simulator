use crate::card::{CardDef, DeckEntry, Decklist};
use crate::game::CardDatabase;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub enum DeckImportError {
    Io(std::io::Error),
    MissingDeckName,
    InvalidLine {
        line_number: usize,
        line: String,
        message: String,
    },
    UnknownCard {
        line_number: usize,
        name: String,
    },
    InvalidSidecar { path: String, message: String },
}

impl fmt::Display for DeckImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeckImportError::Io(err) => write!(f, "failed to read deck file: {err}"),
            DeckImportError::MissingDeckName => write!(f, "deck filename is missing"),
            DeckImportError::InvalidLine {
                line_number,
                line,
                message,
            } => write!(
                f,
                "invalid line {line_number}: {message} (got: '{line}')"
            ),
            DeckImportError::UnknownCard { line_number, name } => {
                write!(f, "unknown card on line {line_number}: '{name}'")
            }
            DeckImportError::InvalidSidecar { path, message } => write!(f, "invalid sidecar '{path}': {message}"),
        }
    }
}

impl Error for DeckImportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DeckImportError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DeckImportError {
    fn from(err: std::io::Error) -> Self {
        DeckImportError::Io(err)
    }
}

/// Which section of a deck file we're currently parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeckSection {
    /// Default section (or ~~Mainboard~~) — cards go into the main deck.
    Mainboard,
    /// ~~Commanders~~ section — cards go into the commanders list.
    Commanders,
    /// ~~Tutor Targets~~ section — cards that tutors can search for.
    /// Restricts the search space so MCCFR can choose among a bounded
    /// set of targets rather than the entire library.
    TutorTargets,
}

/// Parse a single card line of the form "N Card Name" and return (card_id, quantity).
fn parse_card_line(
    trimmed: &str,
    raw_line: &str,
    line_number: usize,
    card_db: &CardDatabase,
) -> Result<(u64, u32), DeckImportError> {
    let first_space = trimmed
        .find(|c: char| c.is_whitespace())
        .ok_or_else(|| DeckImportError::InvalidLine {
            line_number,
            line: raw_line.to_string(),
            message: "missing card name after quantity".to_string(),
        })?;
    let (qty_str, rest) = trimmed.split_at(first_space);
    let quantity: u32 = qty_str.parse().map_err(|_| DeckImportError::InvalidLine {
        line_number,
        line: raw_line.to_string(),
        message: "quantity is not a number".to_string(),
    })?;
    if quantity == 0 {
        return Err(DeckImportError::InvalidLine {
            line_number,
            line: raw_line.to_string(),
            message: "quantity must be greater than zero".to_string(),
        });
    }
    if quantity > 100 {
        return Err(DeckImportError::InvalidLine {
            line_number,
            line: raw_line.to_string(),
            message: "quantity exceeds Commander deck size".to_string(),
        });
    }

    let name = rest.trim();
    if name.is_empty() {
        return Err(DeckImportError::InvalidLine {
            line_number,
            line: raw_line.to_string(),
            message: "card name is empty".to_string(),
        });
    }

    let card_id = card_db
        .find_by_name(name)
        .ok_or_else(|| DeckImportError::UnknownCard {
            line_number,
            name: name.to_string(),
        })?;

    Ok((card_id, quantity))
}

/// Insert or accumulate a card entry in the given list.
fn insert_entry(entries: &mut Vec<DeckEntry>, indices: &mut HashMap<u64, usize>, card_id: u64, quantity: u32) {
    if let Some(index) = indices.get(&card_id).copied() {
        entries[index].quantity += quantity;
    } else {
        indices.insert(card_id, entries.len());
        entries.push(DeckEntry { card_id, quantity });
    }
}

pub fn import_deck_from_file<P: AsRef<Path>>(
    path: P,
    card_db: &CardDatabase,
) -> Result<Decklist, DeckImportError> {
    let path = path.as_ref();
    let deck_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|name| !name.trim().is_empty())
        .ok_or(DeckImportError::MissingDeckName)?
        .to_string();

    let contents = fs::read_to_string(path)?;
    let mut main_entries: Vec<DeckEntry> = Vec::new();
    let mut main_indices: HashMap<u64, usize> = HashMap::new();
    let mut commander_entries: Vec<DeckEntry> = Vec::new();
    let mut commander_indices: HashMap<u64, usize> = HashMap::new();
    let mut tutor_target_ids: Vec<u64> = Vec::new();
    let mut tutor_target_seen: std::collections::HashSet<u64> = std::collections::HashSet::new();

    let mut section = DeckSection::Mainboard;

    for (line_number, raw_line) in contents.lines().enumerate() {
        let line_number = line_number + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check for section headers: ~~SectionName~~
        if trimmed.starts_with("~~") && trimmed.ends_with("~~") && trimmed.len() > 4 {
            let section_name = &trimmed[2..trimmed.len() - 2];
            match section_name.to_ascii_lowercase().as_str() {
                "commanders" | "commander" => {
                    section = DeckSection::Commanders;
                }
                "mainboard" | "main" | "maindeck" => {
                    section = DeckSection::Mainboard;
                }
                "tutor targets" | "tutortargets" | "tutor_targets" | "targets" => {
                    section = DeckSection::TutorTargets;
                }
                _ => {
                    return Err(DeckImportError::InvalidLine {
                        line_number,
                        line: raw_line.to_string(),
                        message: format!("unknown section '{section_name}'"),
                    });
                }
            }
            continue;
        }

        let (card_id, quantity) = parse_card_line(trimmed, raw_line, line_number, card_db)?;

        match section {
            DeckSection::Commanders => {
                insert_entry(&mut commander_entries, &mut commander_indices, card_id, quantity);
            }
            DeckSection::Mainboard => {
                insert_entry(&mut main_entries, &mut main_indices, card_id, quantity);
            }
            DeckSection::TutorTargets => {
                // For tutor targets, only the card identity matters (quantity is ignored).
                if tutor_target_seen.insert(card_id) {
                    tutor_target_ids.push(card_id);
                }
            }
        }
    }

    Ok(Decklist {
        name: deck_name,
        cards: main_entries,
        commanders: commander_entries,
        tutor_targets: tutor_target_ids,
    })
}

/// Load extra `CardDef`s saved by the Moxfield importer's `.cards.json` sidecar.
///
/// An absent optional sidecar is empty; malformed present sidecars are errors.
pub fn load_extra_card_defs(json_path: &Path) -> Result<Vec<CardDef>, DeckImportError> {
    match fs::read_to_string(json_path) {
        Ok(data) => serde_json::from_str::<Vec<CardDef>>(&data).map_err(|e| DeckImportError::InvalidSidecar {
            path: json_path.display().to_string(), message: e.to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(DeckImportError::Io(e)),
    }
}
