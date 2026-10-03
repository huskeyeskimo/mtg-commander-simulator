//! Moxfield deck importer.
//!
//! Fetches a deck from the Moxfield public API by URL or deck ID, converts it
//! to our internal format, and saves it to the `decks/` directory so it can be
//! used like a built-in preset.
//!
//! # Usage
//!
//! ```bash
//! cargo run --features scryfall --bin moxfield_import -- https://moxfield.com/decks/JHjwO92ZUEyNdPzE7D5d7A
//! cargo run --release --bin goldfish -- --preset my-deck-name
//! ```
//!
//! The importer saves two files into `decks/`:
//! - `{name}.txt`        — deck list in our standard text format
//! - `{name}.cards.json` — serialized `Vec<CardDef>` for cards not in the sample DB

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::card::{CardDef, CardId};
use crate::game::CardDatabase;
use crate::scryfall::ScryfallFetcher;

// ---------------------------------------------------------------------------
// Moxfield API response types
// ---------------------------------------------------------------------------

/// A card slot in a Moxfield deck section.
#[derive(Debug, Clone, Deserialize)]
pub struct MoxfieldCardEntry {
    pub quantity: u32,
    pub card: MoxfieldCard,
}

/// Minimal card data returned by the Moxfield API.
#[derive(Debug, Clone, Deserialize)]
pub struct MoxfieldCard {
    pub name: String,
}

/// Top-level Moxfield deck response from `/v3/decks/all/{id}`.
#[derive(Debug, Clone, Deserialize)]
pub struct MoxfieldDeckResponse {
    pub name: String,
    #[serde(default)]
    pub commanders: HashMap<String, MoxfieldCardEntry>,
    #[serde(default)]
    pub mainboard: HashMap<String, MoxfieldCardEntry>,
    // sideboard intentionally omitted — we ignore it
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum MoxfieldError {
    InvalidUrl(String),
    Network(String),
    Parse(String),
    Scryfall(crate::scryfall::ScryfallError),
    Io(std::io::Error),
}

impl std::fmt::Display for MoxfieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoxfieldError::InvalidUrl(s) => write!(f, "invalid Moxfield URL or ID: {s}"),
            MoxfieldError::Network(s) => write!(f, "network error: {s}"),
            MoxfieldError::Parse(s) => write!(f, "JSON parse error: {s}"),
            MoxfieldError::Scryfall(e) => write!(f, "Scryfall error: {e}"),
            MoxfieldError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for MoxfieldError {}

impl From<std::io::Error> for MoxfieldError {
    fn from(e: std::io::Error) -> Self {
        MoxfieldError::Io(e)
    }
}

impl From<crate::scryfall::ScryfallError> for MoxfieldError {
    fn from(e: crate::scryfall::ScryfallError) -> Self {
        MoxfieldError::Scryfall(e)
    }
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/// The result of a successful Moxfield import.
pub struct MoxfieldImportResult {
    /// Human-readable deck name (from Moxfield).
    pub deck_name: String,
    /// Filesystem-safe version of the deck name used for filenames.
    pub file_stem: String,
    /// Merged card database: sample DB cards + Scryfall-fetched cards.
    pub db: CardDatabase,
    /// Flat deck list (99 main + commander included in list).
    pub deck: Vec<CardId>,
    /// Commander card ID.
    pub commander_id: Option<CardId>,
    /// Cards that could not be fetched (name + reason).
    pub errors: Vec<String>,
    /// Path where the deck list was saved.
    pub deck_txt_path: String,
    /// Path where the extra card definitions were saved (may be empty if all
    /// cards were found in the sample DB).
    pub cards_json_path: Option<String>,
}

// ---------------------------------------------------------------------------
// URL / ID parsing
// ---------------------------------------------------------------------------

/// Extract a Moxfield deck ID from a URL or return the string as-is if it
/// looks like a bare ID already.
///
/// Handles:
/// - `https://moxfield.com/decks/JHjwO92ZUEyNdPzE7D5d7A`
/// - `https://www.moxfield.com/decks/JHjwO92ZUEyNdPzE7D5d7A`
/// - `JHjwO92ZUEyNdPzE7D5d7A` (bare ID)
pub fn extract_deck_id(url_or_id: &str) -> Result<String, MoxfieldError> {
    let trimmed = url_or_id.trim();
    if trimmed.is_empty() {
        return Err(MoxfieldError::InvalidUrl("empty input".to_string()));
    }

    // If it contains "moxfield.com/decks/", extract the segment after that.
    if let Some(pos) = trimmed.find("moxfield.com/decks/") {
        let after = &trimmed[pos + "moxfield.com/decks/".len()..];
        // The ID ends at the next '/', '?', or '#'.
        let id = after.split('/').next().unwrap_or("").trim();
        // Strip query-string (?utm_source=...) or fragment (#comments).
        let id = id.split(&['?', '#'] as &[char]).next().unwrap_or("").trim();
        if id.is_empty() {
            return Err(MoxfieldError::InvalidUrl(format!(
                "could not extract deck ID from URL: {trimmed}"
            )));
        }
        return Ok(id.to_string());
    }

    // Otherwise assume it's a bare ID.
    Ok(trimmed.to_string())
}

// ---------------------------------------------------------------------------
// API fetch
// ---------------------------------------------------------------------------

/// Fetch a deck from the Moxfield API.
pub fn fetch_moxfield_deck(deck_id: &str) -> Result<MoxfieldDeckResponse, MoxfieldError> {
    let url = format!("https://api2.moxfield.com/v3/decks/all/{deck_id}");
    let response = ureq::get(&url)
        .set(
            "User-Agent",
            "mtg-gto/0.1 (Rust game engine; moxfield importer)",
        )
        .call()
        .map_err(|e| MoxfieldError::Network(e.to_string()))?;

    response
        .into_json::<MoxfieldDeckResponse>()
        .map_err(|e| MoxfieldError::Parse(e.to_string()))
}

// ---------------------------------------------------------------------------
// Name sanitisation
// ---------------------------------------------------------------------------

/// Convert a deck name to a filesystem-safe stem (lowercase, spaces → hyphens,
/// non-alphanumeric non-hyphen characters dropped).
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            'A'..='Z' => c.to_ascii_lowercase(),
            ' ' | '_' | '-' => '-',
            _ => '-',
        })
        .collect::<String>()
        // Collapse runs of hyphens.
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// ---------------------------------------------------------------------------
// Deck text generation
// ---------------------------------------------------------------------------

/// Build our standard deck text from a Moxfield deck response.
fn build_deck_text(deck: &MoxfieldDeckResponse) -> String {
    let mut out = String::new();

    // Commanders section
    if !deck.commanders.is_empty() {
        out.push_str("~~Commanders~~\n");
        let mut commanders: Vec<_> = deck.commanders.values().collect();
        commanders.sort_by(|a, b| a.card.name.cmp(&b.card.name));
        for entry in commanders {
            out.push_str(&format!("{} {}\n", entry.quantity, entry.card.name));
        }
        out.push('\n');
    }

    // Mainboard section
    if !deck.mainboard.is_empty() {
        out.push_str("~~Mainboard~~\n");
        let mut mainboard: Vec<_> = deck.mainboard.values().collect();
        mainboard.sort_by(|a, b| a.card.name.cmp(&b.card.name));
        for entry in mainboard {
            out.push_str(&format!("{} {}\n", entry.quantity, entry.card.name));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Full import pipeline
// ---------------------------------------------------------------------------

/// Import a Moxfield deck end-to-end.
///
/// Steps:
/// 1. Extract deck ID from URL/ID string.
/// 2. Fetch deck from Moxfield API.
/// 3. Build a merged `CardDatabase`: sample DB first, Scryfall for unknowns.
/// 4. Save `decks/{name}.txt` and, if extra card defs were needed,
///    `decks/{name}.cards.json`.
///
/// The saved files can then be used by the goldfish binary via `--preset`.
pub fn import_moxfield_deck(
    url_or_id: &str,
    decks_dir: &str,
    scryfall_cache_dir: &str,
) -> Result<MoxfieldImportResult, MoxfieldError> {
    // Step 1: extract deck ID
    let deck_id = extract_deck_id(url_or_id)?;
    eprintln!("Fetching Moxfield deck ID: {deck_id}");

    // Step 2: fetch from Moxfield
    let mox_deck = fetch_moxfield_deck(&deck_id)?;
    let deck_name = mox_deck.name.clone();
    let mut file_stem = sanitize_name(&deck_name);
    if file_stem.is_empty() {
        // Fall back to the raw deck ID so we always have a valid filename.
        file_stem = deck_id.clone();
    }
    eprintln!("Deck: {deck_name} → files: decks/{file_stem}.txt");

    // Step 3: collect all card names (commanders + mainboard, ignore sideboard)
    // We keep duplicates out by tracking names we've already queued.
    let mut all_cards: Vec<(String, u32, bool)> = Vec::new(); // (name, qty, is_commander)
    {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        let mut commanders: Vec<_> = mox_deck.commanders.values().collect();
        commanders.sort_by(|a, b| a.card.name.cmp(&b.card.name));
        for entry in &commanders {
            let name = entry.card.name.clone();
            if seen.insert(name.clone()) {
                all_cards.push((name, entry.quantity, true));
            }
        }

        let mut mainboard: Vec<_> = mox_deck.mainboard.values().collect();
        mainboard.sort_by(|a, b| a.card.name.cmp(&b.card.name));
        for entry in &mainboard {
            let name = entry.card.name.clone();
            if seen.insert(name.clone()) {
                all_cards.push((name, entry.quantity, false));
            }
        }
    }

    // Step 4: build merged CardDatabase via Scryfall fetcher
    let sample_db = crate::card::sample::build_sample_db();
    let mut fetcher = ScryfallFetcher::new(scryfall_cache_dir);

    let mut merged_db = sample_db.clone();
    // Extra CardDefs that are NOT in the sample DB (need to be saved to JSON).
    let mut extra_defs: Vec<CardDef> = Vec::new();
    let mut name_to_id: HashMap<String, CardId> = HashMap::new();
    let mut deck_flat: Vec<CardId> = Vec::new();
    let mut commander_id: Option<CardId> = None;
    let mut errors: Vec<String> = Vec::new();

    let total = all_cards.len();
    for (idx, (card_name, qty, is_commander)) in all_cards.iter().enumerate() {
        eprintln!("  [{}/{}] {}", idx + 1, total, card_name);

        // Handle "Front // Back" DFC names: look up by front face name only.
        let lookup_name: &str = if card_name.contains(" // ") {
            card_name.split(" // ").next().unwrap_or(card_name.as_str())
        } else {
            card_name.as_str()
        };

        let card_id = if let Some(&existing) = name_to_id.get(lookup_name) {
            existing
        } else {
            // Check sample DB first (Tier 1).
            if let Some(id) = sample_db.find_by_name(lookup_name) {
                name_to_id.insert(lookup_name.to_string(), id);
                id
            } else {
                // Tier 2: fetch from Scryfall.
                match fetcher.fetch_card_def(lookup_name) {
                    Ok(def) => {
                        let id = def.id;
                        // Ensure it's in the merged DB.
                        if merged_db.get(id).is_none() {
                            extra_defs.push(def.clone());
                            merged_db.insert(def);
                        }
                        name_to_id.insert(lookup_name.to_string(), id);
                        id
                    }
                    Err(e) => {
                        errors.push(format!("{card_name}: {e}"));
                        continue;
                    }
                }
            }
        };

        for _ in 0..*qty {
            deck_flat.push(card_id);
        }

        if *is_commander {
            commander_id = Some(card_id);
        }
    }

    // Step 5: write deck text file.
    std::fs::create_dir_all(decks_dir)?;
    let deck_text = build_deck_text(&mox_deck);
    let txt_path = format!("{decks_dir}/{file_stem}.txt");
    std::fs::write(&txt_path, &deck_text)?;
    eprintln!("Saved deck list → {txt_path}");

    // Step 6: write extra card defs JSON (only if any cards weren't in sample DB).
    let cards_json_path = if extra_defs.is_empty() {
        None
    } else {
        let json_path = format!("{decks_dir}/{file_stem}.cards.json");
        let json = serde_json::to_string_pretty(&extra_defs)
            .map_err(|e| MoxfieldError::Parse(e.to_string()))?;
        std::fs::write(&json_path, &json)?;
        eprintln!(
            "Saved {} extra card definition(s) → {json_path}",
            extra_defs.len()
        );
        Some(json_path)
    };

    Ok(MoxfieldImportResult {
        deck_name,
        file_stem,
        db: merged_db,
        deck: deck_flat,
        commander_id,
        errors,
        deck_txt_path: txt_path,
        cards_json_path,
    })
}

// ---------------------------------------------------------------------------
// Helpers for loading a saved Moxfield preset at goldfish time
// ---------------------------------------------------------------------------

/// Load the extra card definitions saved by a previous `moxfield_import` run.
///
/// Delegates to `deck_import::load_extra_card_defs` — the canonical implementation
/// that lives outside the `scryfall` feature gate so `goldfish` can use it too.
pub fn load_extra_card_defs(json_path: &Path) -> Result<Vec<CardDef>, crate::deck_import::DeckImportError> {
    crate::deck_import::load_extra_card_defs(json_path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_deck_id ---

    #[test]
    fn test_extract_deck_id_full_url() {
        let id = extract_deck_id("https://moxfield.com/decks/JHjwO92ZUEyNdPzE7D5d7A").unwrap();
        assert_eq!(id, "JHjwO92ZUEyNdPzE7D5d7A");
    }

    #[test]
    fn test_extract_deck_id_www_url() {
        let id =
            extract_deck_id("https://www.moxfield.com/decks/JHjwO92ZUEyNdPzE7D5d7A").unwrap();
        assert_eq!(id, "JHjwO92ZUEyNdPzE7D5d7A");
    }

    #[test]
    fn test_extract_deck_id_url_with_query_params() {
        let id = extract_deck_id(
            "https://moxfield.com/decks/JHjwO92ZUEyNdPzE7D5d7A?utm_source=share",
        )
        .unwrap();
        assert_eq!(id, "JHjwO92ZUEyNdPzE7D5d7A");
    }

    #[test]
    fn test_extract_deck_id_url_with_fragment() {
        let id =
            extract_deck_id("https://moxfield.com/decks/JHjwO92ZUEyNdPzE7D5d7A#comments")
                .unwrap();
        assert_eq!(id, "JHjwO92ZUEyNdPzE7D5d7A");
    }

    #[test]
    fn test_extract_deck_id_bare_id() {
        let id = extract_deck_id("JHjwO92ZUEyNdPzE7D5d7A").unwrap();
        assert_eq!(id, "JHjwO92ZUEyNdPzE7D5d7A");
    }

    #[test]
    fn test_extract_deck_id_empty() {
        assert!(extract_deck_id("").is_err());
        assert!(extract_deck_id("   ").is_err());
    }

    #[test]
    fn test_extract_deck_id_url_missing_id() {
        assert!(extract_deck_id("https://moxfield.com/decks/").is_err());
    }

    // --- sanitize_name ---

    #[test]
    fn test_sanitize_name_spaces_become_hyphens() {
        assert_eq!(sanitize_name("My Cool Deck"), "my-cool-deck");
    }

    #[test]
    fn test_sanitize_name_uppercase_lowercased() {
        assert_eq!(sanitize_name("Kinnan GoodStuff"), "kinnan-goodstuff");
    }

    #[test]
    fn test_sanitize_name_special_chars_dropped() {
        assert_eq!(sanitize_name("Deck: The Gathering!"), "deck-the-gathering");
    }

    #[test]
    fn test_sanitize_name_consecutive_separators_collapsed() {
        assert_eq!(sanitize_name("a  --  b"), "a-b");
    }

    #[test]
    fn test_sanitize_name_all_special_returns_empty() {
        assert_eq!(sanitize_name("!!!"), "");
    }

    #[test]
    fn test_sanitize_name_alphanumeric_preserved() {
        assert_eq!(sanitize_name("deck42"), "deck42");
    }
}
