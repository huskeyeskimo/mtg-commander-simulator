//! Offline deck resolution. Identity, executable behavior and certification are separate.
use crate::card::{
    sample, CardDef, CardId, CardType, DeckEntry, Decklist, Effect, Subtype, Supertype,
};
use crate::deck_import::import_deck_from_file;
use crate::game::CardDatabase;
use crate::mana::{Color, ManaCost};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub const GOBLIN_STORM_CONTRACT_VERSION: &str = "goblin-storm-v1-draft-runtime-blocked";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Capability {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportStatus {
    Supported,
    Partial,
    Unsupported,
    Unreviewed,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CardSupport {
    pub origin: DefinitionOrigin,
    pub status: SupportStatus,
    pub blocking_capabilities: Vec<String>,
    pub reviewed_at: Option<String>,
    pub evidence: Vec<String>,
    pub behavior_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionOrigin {
    Authored,
    IdentityOnly,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CatalogCard {
    pub id: CardId,
    pub name: String,
    pub mana_cost: Option<ManaCost>,
    pub printed_colors: Vec<Color>,
    pub color_identity: Vec<Color>,
    pub card_types: Vec<CardType>,
    pub supertypes: Vec<Supertype>,
    pub subtypes: Vec<String>,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub oracle_text: String,
    pub identity_source: String,
    pub support: CardSupport,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FixedCatalog {
    pub schema_version: u32,
    pub deck: String,
    pub identity_note: String,
    pub source_audit_sha256: String,
    pub official_decklist_url: String,
    pub capabilities: Vec<Capability>,
    pub commander: String,
    pub expected_mainboard: HashMap<String, u32>,
    pub cards: Vec<CatalogCard>,
}

impl FixedCatalog {
    pub fn parse(data: &str) -> Result<Self, String> {
        let catalog: Self =
            serde_json::from_str(data).map_err(|e| format!("invalid fixed catalog JSON: {e}"))?;
        if catalog.schema_version != 1
            || catalog.deck != "goblin_storm"
            || catalog.cards.len() != 79
            || catalog.source_audit_sha256.len() != 64
            || !catalog
                .official_decklist_url
                .starts_with("https://magic.wizards.com/")
        {
            return Err("unsupported fixed catalog schema, deck, or card count".into());
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let mut caps = HashSet::new();
        for cap in &catalog.capabilities {
            if cap.id.is_empty() || cap.description.is_empty() || !caps.insert(cap.id.as_str()) {
                return Err(format!("duplicate or invalid capability '{}'", cap.id));
            }
        }
        for card in &catalog.cards {
            let name = normalized(&card.name);
            if !ids.insert(card.id) || !names.insert(name.clone()) {
                return Err(format!(
                    "duplicate catalog identity '{}' (ID {})",
                    card.name, card.id
                ));
            }
            if card.name.trim().is_empty()
                || card.card_types.is_empty()
                || card.identity_source.trim().is_empty()
                || card.oracle_text.trim().is_empty()
                || card.card_types.contains(&CardType::Creature) != card.power.is_some()
                || card.card_types.contains(&CardType::Creature) != card.toughness.is_some()
                || (card.card_types.contains(&CardType::Land) && card.mana_cost.is_some())
            {
                return Err(format!(
                    "incomplete basic identity metadata for '{}'",
                    card.name
                ));
            }
            for gap in &card.support.blocking_capabilities {
                if !caps.contains(gap.as_str()) {
                    return Err(format!("unknown capability '{gap}' on '{}'", card.name));
                }
            }
            if card.support.status == SupportStatus::Supported
                && positive_review_issue(card).is_some()
            {
                return Err(format!(
                    "'{}' lacks positive support certification",
                    card.name
                ));
            }
            if card.support.origin == DefinitionOrigin::IdentityOnly
                && card.support.status == SupportStatus::Supported
            {
                return Err(format!(
                    "identity-only '{}' cannot be rules-supported",
                    card.name
                ));
            }
            if !card
                .printed_colors
                .iter()
                .all(|c| card.color_identity.contains(c))
                || card.card_types.contains(&CardType::Land) && !card.printed_colors.is_empty()
            {
                return Err(format!(
                    "invalid printed colors or color identity for '{}'",
                    card.name
                ));
            }
            if card.mana_cost.as_ref().is_some_and(|cost| {
                cost.colors()
                    .iter()
                    .any(|color| !card.printed_colors.contains(color))
            }) {
                return Err(format!(
                    "mana cost and printed colors disagree for '{}'",
                    card.name
                ));
            }
        }
        if catalog.expected_mainboard.values().sum::<u32>() != 99
            || catalog.expected_mainboard.len() != 78
            || !names.contains(&normalized(&catalog.commander))
            || catalog.expected_mainboard.contains_key(&catalog.commander)
        {
            return Err("catalog fixed-list declaration is invalid".into());
        }
        for name in catalog.expected_mainboard.keys() {
            if !names.contains(&normalized(name)) {
                return Err(format!("expected card '{name}' has no identity"));
            }
        }
        Ok(catalog)
    }

    pub fn from_path(path: &Path) -> Result<Self, String> {
        let data = fs::read_to_string(path)
            .map_err(|e| format!("cannot read catalog '{}': {e}", path.display()))?;
        Self::parse(&data)
    }

    pub fn impacted_by(&self, capability: &str) -> Vec<&CatalogCard> {
        self.cards
            .iter()
            .filter(|c| {
                c.support
                    .blocking_capabilities
                    .iter()
                    .any(|b| b == capability)
            })
            .collect()
    }
}

fn normalized(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Stable non-cryptographic revision marker; this is not an authenticity check.
pub fn stable_fingerprint(data: &[u8]) -> String {
    let mut value = 0xcbf29ce484222325u64;
    for byte in data {
        value = (value ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{value:016x}")
}

fn effect_suspicion(effect: &Effect) -> Option<&'static str> {
    match effect {
        Effect::Unimplemented(_) => Some("nested Unimplemented effect"),
        Effect::Multiple(items) if items.is_empty() => Some("empty effect container"),
        Effect::Multiple(items) => items.iter().find_map(effect_suspicion),
        Effect::Modal {
            choices,
            choose_count,
        } if choices.is_empty() || *choose_count == 0 => Some("empty effect container"),
        Effect::Modal { choices, .. } => choices.iter().find_map(effect_suspicion),
        Effect::Conditional {
            if_true, if_false, ..
        } => effect_suspicion(if_true).or_else(|| if_false.as_deref().and_then(effect_suspicion)),
        Effect::ForEach { effect, .. } => effect_suspicion(effect),
        _ => None,
    }
}

fn implementation_suspicion(def: &CardDef) -> Option<&'static str> {
    if let Some(issue) = def
        .spell_effect
        .as_ref()
        .and_then(effect_suspicion)
        .or_else(|| {
            def.activated_abilities
                .iter()
                .find_map(|a| effect_suspicion(&a.effect))
        })
        .or_else(|| {
            def.triggered_abilities
                .iter()
                .find_map(|a| effect_suspicion(&a.effect))
        })
        .or_else(|| {
            def.loyalty_abilities
                .iter()
                .find_map(|a| effect_suspicion(&a.effect))
        })
    {
        return Some(issue);
    }
    if !def.is_land()
        && def.spell_effect.is_none()
        && def.activated_abilities.is_empty()
        && def.triggered_abilities.is_empty()
        && def.static_abilities.is_empty()
        && def.mana_abilities.is_empty()
        && def.keywords.is_empty()
        && !def.oracle_text.trim().is_empty()
    {
        return Some("printed abilities absent from definition");
    }
    None
}

fn positive_review_issue(card: &CatalogCard) -> Option<&'static str> {
    if card.support.origin != DefinitionOrigin::Authored {
        return Some("identity-only definition cannot be certified");
    }
    if !card.support.blocking_capabilities.is_empty() {
        return Some("blocking capabilities remain");
    }
    if card
        .support
        .reviewed_at
        .as_deref()
        .is_none_or(|date| date.trim().is_empty())
    {
        return Some("missing substantive review date or identifier");
    }
    if card.support.evidence.is_empty()
        || card
            .support
            .evidence
            .iter()
            .any(|entry| entry.trim().is_empty())
    {
        return Some("missing substantive review evidence");
    }
    if card
        .support
        .behavior_fingerprint
        .as_deref()
        .is_none_or(|fingerprint| fingerprint.trim().is_empty())
    {
        return Some("missing behavior fingerprint");
    }
    None
}

/// Positive certification is independent of whether blockers have been resolved.
pub fn certification_issue(card: &CatalogCard, def: &CardDef) -> Option<String> {
    if card.support.status != SupportStatus::Supported {
        return Some(format!(
            "{:?}: {}",
            card.support.status,
            card.support.blocking_capabilities.join(", ")
        ));
    }
    if let Some(issue) = positive_review_issue(card) {
        return Some(issue.into());
    }
    if let Some(issue) = implementation_suspicion(def) {
        return Some(issue.into());
    }
    let encoded = match serde_json::to_vec(def) {
        Ok(encoded) => encoded,
        Err(_) => return Some("cannot fingerprint executable behavior".into()),
    };
    let actual = stable_fingerprint(&encoded);
    if card.support.behavior_fingerprint.as_deref() != Some(actual.as_str()) {
        return Some("stale behavior certification".into());
    }
    None
}

pub struct ResolvedDeck {
    pub db: CardDatabase,
    pub decklist: Decklist,
    pub commander: CardId,
    pub mainboard: Vec<CardId>,
    pub setup_cards: Vec<CardId>,
    pub tutor_targets: Vec<CardId>,
    pub catalog: Option<FixedCatalog>,
    pub catalog_fingerprint: Option<String>,
    pub readiness_reasons: Vec<String>,
}

impl ResolvedDeck {
    pub fn benchmark_ready(&self) -> bool {
        self.readiness_reasons.is_empty()
    }

    /// The fixed catalog retains color identity separately from executable
    /// mana abilities, including on lands whose behavior is unsupported.
    pub fn color_identity(&self, card_id: CardId) -> Option<Vec<Color>> {
        let def = self.db.get(card_id)?;
        if let Some(catalog) = &self.catalog {
            if let Some(entry) = catalog
                .cards
                .iter()
                .find(|c| normalized(&c.name) == normalized(&def.name))
            {
                return Some(entry.color_identity.clone());
            }
        }
        Some(def.color_identity())
    }
}

pub fn fixed_catalog_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("decks/goblin_storm.catalog.json")
}

/// Structural opponent fixture only. Its commander is never voluntarily cast;
/// forced relevance of that commander's uncertified behavior invalidates a run.
pub fn passive_opponent_setup_cards() -> (Vec<CardId>, CardId) {
    let commander = sample::ids::MIKU_LOST_BUT_SINGING;
    let mut cards = vec![sample::ids::FOREST; 99];
    cards.push(commander);
    (cards, commander)
}

pub fn resolve_preset(name: &str) -> Result<ResolvedDeck, String> {
    let db = sample::build_sample_db();
    let (setup_cards, commander, tutor_targets) = match name {
        "kinnan" => sample::kinnan_commander_deck(),
        "brimaz" => {
            let (d, c) = sample::brimaz_commander_deck();
            (d, c, Vec::new())
        }
        "ashcoat" => {
            let (d, c) = sample::ashcoat_commander_deck();
            (d, c, Vec::new())
        }
        "flubs" => {
            let (d, c) = sample::flubs_commander_deck();
            (d, c, Vec::new())
        }
        "thrun" => {
            let (d, c) = sample::thrun_commander_deck();
            (d, c, Vec::new())
        }
        _ => {
            let path = Path::new("decks").join(format!("{name}.txt"));
            if !path.is_file() {
                return Err(format!("unknown preset '{name}'"));
            }
            return resolve_file(&path);
        }
    };
    let mut mainboard = setup_cards.clone();
    let commander_copies = mainboard.iter().filter(|&&id| id == commander).count();
    match (setup_cards.len(), commander_copies) {
        (100, 1) => {
            let index = mainboard.iter().position(|id| *id == commander).unwrap();
            mainboard.remove(index);
        }
        (99, 0) => {} // Legacy Flubs builder returns mainboard only.
        _ => {
            return Err(format!(
                "builtin preset '{name}' has invalid commander inclusion"
            ))
        }
    }
    let mut setup_cards = mainboard.clone();
    setup_cards.push(commander);
    let mut quantities: std::collections::BTreeMap<CardId, u32> = std::collections::BTreeMap::new();
    for &id in &mainboard {
        *quantities.entry(id).or_default() += 1;
    }
    let decklist = Decklist {
        name: name.into(),
        cards: quantities
            .into_iter()
            .map(|(card_id, quantity)| DeckEntry { card_id, quantity })
            .collect(),
        commanders: vec![DeckEntry {
            card_id: commander,
            quantity: 1,
        }],
        tutor_targets: tutor_targets.clone(),
    };
    Ok(ResolvedDeck {
        db,
        decklist,
        commander,
        mainboard,
        setup_cards,
        tutor_targets,
        catalog: None,
        catalog_fingerprint: None,
        readiness_reasons: Vec::new(),
    })
}

fn enrich_from_catalog(db: &mut CardDatabase, catalog: &FixedCatalog) -> Result<(), String> {
    for card in &catalog.cards {
        let existing_by_name = db.find_by_name(&card.name);
        if (existing_by_name.is_some()) != (card.support.origin == DefinitionOrigin::Authored) {
            return Err(format!("definition origin mismatch for '{}'", card.name));
        }
        let id = existing_by_name.unwrap_or(card.id);
        if let Some(other) = db.get(card.id) {
            if normalized(&other.name) != normalized(&card.name) {
                return Err(format!(
                    "catalog ID {} for '{}' collides with '{}'",
                    card.id, card.name, other.name
                ));
            }
        }
        if existing_by_name.is_none() && db.get(id).is_some() {
            return Err(format!("catalog ID collision: {id}"));
        }
        let mut def = db.get(id).cloned().unwrap_or_default();
        def.id = id;
        def.name = card.name.clone();
        def.mana_cost = card.mana_cost.clone();
        def.colors = Some(card.printed_colors.clone());
        def.card_types = card.card_types.clone();
        def.supertypes = card.supertypes.clone();
        def.subtypes = card.subtypes.iter().cloned().map(Subtype).collect();
        def.power = card.power;
        def.toughness = card.toughness;
        def.oracle_text = card.oracle_text.clone();
        // No executable effects are inferred from text. Existing hand-authored
        // behavior survives, even when its ID differs from the catalog's ID.
        db.insert(def);
    }
    Ok(())
}

pub fn resolve_file(path: &Path) -> Result<ResolvedDeck, String> {
    let catalog_path = fixed_catalog_path();
    let fixed = path.file_stem().and_then(|s| s.to_str()) == Some("goblin_storm")
        || matches_fixed_list_by_identity(path, &catalog_path)?;
    resolve_file_with_catalog(
        path,
        if fixed {
            Some(catalog_path.as_path())
        } else {
            None
        },
    )
}

/// Probe the exact printed multiset using only the reviewed local catalog.
/// This reads only commander and mainboard names/counts. Auxiliary tutor
/// metadata is validated by the authoritative loader after classification,
/// so an unknown sidecar-only tutor cannot hide an otherwise exact deck.
fn matches_fixed_list_by_identity(path: &Path, catalog_path: &Path) -> Result<bool, String> {
    let catalog = FixedCatalog::from_path(catalog_path)?;
    let contents = fs::read_to_string(path)
        .map_err(|e| format!("cannot read deck '{}': {e}", path.display()))?;
    enum Section {
        Mainboard,
        Commander,
        Auxiliary,
    }
    let mut section = Section::Mainboard;
    let mut commanders: HashMap<String, u32> = HashMap::new();
    let mut mainboard: HashMap<String, u32> = HashMap::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("~~") && line.ends_with("~~") && line.len() > 4 {
            section = match line[2..line.len() - 2].to_ascii_lowercase().as_str() {
                "commanders" | "commander" => Section::Commander,
                "mainboard" | "main" | "maindeck" => Section::Mainboard,
                "tutor targets" | "tutortargets" | "tutor_targets" | "targets" => {
                    Section::Auxiliary
                }
                _ => return Ok(false), // The normal importer reports the unknown section.
            };
            continue;
        }
        let counts = match section {
            Section::Mainboard => &mut mainboard,
            Section::Commander => &mut commanders,
            Section::Auxiliary => continue,
        };
        let Some(separator) = line.find(char::is_whitespace) else {
            return Ok(false);
        };
        let Ok(quantity) = line[..separator].parse::<u32>() else {
            return Ok(false);
        };
        let name = normalized(&line[separator..]);
        if quantity == 0 || quantity > 100 || name.is_empty() {
            return Ok(false);
        }
        *counts.entry(name).or_default() += quantity;
    }
    let expected_mainboard: HashMap<String, u32> = catalog
        .expected_mainboard
        .iter()
        .map(|(name, &quantity)| (normalized(name), quantity))
        .collect();
    Ok(
        commanders == HashMap::from([(normalized(&catalog.commander), 1)])
            && mainboard == expected_mainboard,
    )
}

pub fn resolve_file_with_catalog(
    path: &Path,
    fixed_catalog_path: Option<&Path>,
) -> Result<ResolvedDeck, String> {
    let mut db = sample::build_sample_db();
    let mut catalog = None;
    let mut catalog_fingerprint = None;
    if let Some(catalog_path) = fixed_catalog_path {
        let data = fs::read_to_string(catalog_path)
            .map_err(|e| format!("cannot read catalog '{}': {e}", catalog_path.display()))?;
        let parsed = FixedCatalog::parse(&data)?;
        enrich_from_catalog(&mut db, &parsed)?;
        catalog_fingerprint = Some(stable_fingerprint(data.as_bytes()));
        catalog = Some(parsed);
    } else {
        let sidecar = path.with_extension("cards.json");
        if sidecar.exists() {
            let data = fs::read_to_string(&sidecar)
                .map_err(|e| format!("cannot read sidecar '{}': {e}", sidecar.display()))?;
            let defs: Vec<CardDef> = serde_json::from_str(&data)
                .map_err(|e| format!("invalid sidecar '{}': {e}", sidecar.display()))?;
            let mut sidecar_names = HashSet::new();
            let mut sidecar_ids = HashSet::new();
            for def in defs {
                if !sidecar_names.insert(normalized(&def.name)) || !sidecar_ids.insert(def.id) {
                    return Err(format!(
                        "duplicate sidecar identity '{}' ({})",
                        def.name, def.id
                    ));
                }
                if let Some(existing) = db.get(def.id) {
                    if normalized(&existing.name) != normalized(&def.name) {
                        return Err(format!("sidecar ID collision: {}", def.id));
                    }
                }
                if db.find_by_name(&def.name).is_none() {
                    db.insert(def);
                }
            }
        }
    }
    let decklist =
        import_deck_from_file(path, &db).map_err(|e| format!("{}: {e}", path.display()))?;
    let commander = validate_commander_inclusion(&decklist, &db)?;
    let mainboard = decklist.expand();
    let mut setup_cards = mainboard.clone();
    setup_cards.push(commander);
    let mut readiness_reasons = Vec::new();
    if let Some(catalog) = &catalog {
        validate_fixed_list(&decklist, &db, catalog)?;
        for card in &catalog.cards {
            let def = db.get(db.find_by_name(&card.name).unwrap()).unwrap();
            if let Some(issue) = certification_issue(card, def) {
                readiness_reasons.push(format!("{}: {issue}", card.name));
            }
        }
        readiness_reasons
            .push("V1 benchmark runner/RNG/hidden-information contract is not implemented".into());
    }
    Ok(ResolvedDeck {
        db,
        decklist: decklist.clone(),
        commander,
        mainboard,
        setup_cards,
        tutor_targets: decklist.tutor_targets,
        catalog,
        catalog_fingerprint,
        readiness_reasons,
    })
}

pub fn validate_commander_inclusion(
    decklist: &Decklist,
    db: &CardDatabase,
) -> Result<CardId, String> {
    if decklist.commanders.len() != 1 || decklist.commanders[0].quantity != 1 {
        return Err("deck must declare exactly one commander with quantity one".into());
    }
    let cmd = decklist.commanders[0].card_id;
    if decklist.cards.iter().any(|entry| entry.card_id == cmd) {
        return Err("commander also appears in the mainboard".into());
    }
    if decklist.total_cards() != 99 {
        return Err(format!(
            "mainboard must have 99 cards, got {}",
            decklist.total_cards()
        ));
    }
    let mut setup = decklist.expand();
    setup.push(cmd);
    crate::rules::validate_commander_deck(db, &setup, cmd)?;
    Ok(cmd)
}

fn validate_fixed_list(
    decklist: &Decklist,
    db: &CardDatabase,
    catalog: &FixedCatalog,
) -> Result<(), String> {
    if !decklist.tutor_targets.is_empty() {
        return Err("fixed benchmark deck may not restrict tutor targets".into());
    }
    let commander_name = &db
        .get(decklist.commanders[0].card_id)
        .ok_or("commander identity missing")?
        .name;
    if normalized(commander_name) != normalized(&catalog.commander) {
        return Err("fixed deck commander differs from catalog".into());
    }
    let commander_identity: HashSet<Color> = catalog
        .cards
        .iter()
        .find(|card| normalized(&card.name) == normalized(&catalog.commander))
        .ok_or("commander missing from catalog")?
        .color_identity
        .iter()
        .copied()
        .collect();
    for card in &catalog.cards {
        if card
            .color_identity
            .iter()
            .any(|color| !commander_identity.contains(color))
        {
            return Err(format!(
                "'{}' exceeds fixed commander's color identity",
                card.name
            ));
        }
    }
    let mut counts: HashMap<String, u32> = HashMap::new();
    for entry in &decklist.cards {
        let name = db
            .get(entry.card_id)
            .ok_or("deck identity missing")?
            .name
            .clone();
        *counts.entry(name).or_default() += entry.quantity;
    }
    if counts != catalog.expected_mainboard {
        return Err("fixed deck differs from the catalog's exact 99-card mainboard".into());
    }
    Ok(())
}
