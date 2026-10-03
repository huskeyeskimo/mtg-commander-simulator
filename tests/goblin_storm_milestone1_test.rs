use mtg_gto::card::effects::Condition;
use mtg_gto::card::{sample, CardType, DynamicValue, Effect};
use mtg_gto::deck_loader::{
    self, certification_issue, stable_fingerprint, FixedCatalog, SupportStatus,
};
use mtg_gto::game::GameState;
use mtg_gto::rules;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

fn fixture(contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mtg-m1-{}-{}",
        std::process::id(),
        NEXT_CASE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("goblin_storm.txt");
    std::fs::write(&path, contents).unwrap();
    path
}

fn official_text() -> String {
    std::fs::read_to_string("decks/goblin_storm.txt").unwrap()
}

fn resolve(path: &Path) -> deck_loader::ResolvedDeck {
    deck_loader::resolve_file(path).unwrap()
}

fn renamed_official_deck(with_sidecar: bool) -> PathBuf {
    let original = fixture(&official_text());
    let renamed = original.with_file_name("renamed.txt");
    std::fs::rename(original, &renamed).unwrap();
    if with_sidecar {
        let audit: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string("decks/goblin_storm.audit.json").unwrap(),
        )
        .unwrap();
        let mut definitions: Vec<serde_json::Value> = audit
            .into_iter()
            .map(|record| record["definition"].clone())
            .collect();
        let pashalik = definitions
            .iter_mut()
            .find(|definition| definition["name"] == "Pashalik Mons")
            .unwrap();
        pashalik["oracle_text"] = serde_json::json!("Contradictory sidecar text");
        std::fs::write(
            renamed.with_extension("cards.json"),
            serde_json::to_vec(&definitions).unwrap(),
        )
        .unwrap();
    }
    renamed
}

fn assert_blocked_output(output: std::process::Output) {
    assert_eq!(output.status.code(), Some(2));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.to_lowercase().contains("blocked"), "{combined}");
    assert!(!combined.contains("Win rate:"), "{combined}");
    assert!(!combined.contains("Result:"), "{combined}");
    assert!(!combined.contains("Simulating"), "{combined}");
}

#[test]
fn renamed_exact_deck_uses_authoritative_catalog_with_or_without_sidecar() {
    for with_sidecar in [false, true] {
        let path = renamed_official_deck(with_sidecar);
        let resolved = resolve(&path);
        assert_eq!(resolved.setup_cards.len(), 100);
        assert_eq!(resolved.mainboard.len(), 99);
        assert!(!resolved.benchmark_ready());
        assert!(resolved.catalog.is_some());
        let pashalik = resolved
            .db
            .get(resolved.db.find_by_name("Pashalik Mons").unwrap())
            .unwrap();
        assert_ne!(pashalik.oracle_text, "Contradictory sidecar text");
        assert!(pashalik.activated_abilities.is_empty());
        let direct = resolve(Path::new("decks/goblin_storm.txt"));
        assert_eq!(resolved.setup_cards, direct.setup_cards);
        assert_eq!(resolved.catalog_fingerprint, direct.catalog_fingerprint);
        for name in ["Zada, Hedron Grinder", "Expedite"] {
            let actual = resolved
                .db
                .get(resolved.db.find_by_name(name).unwrap())
                .unwrap();
            let authored = sample::build_sample_db();
            let expected = authored.get(authored.find_by_name(name).unwrap()).unwrap();
            assert_eq!(actual.spell_effect, expected.spell_effect);
            assert_eq!(actual.triggered_abilities, expected.triggered_abilities);
        }
        assert_blocked_output(
            std::process::Command::new(env!("CARGO_BIN_EXE_goldfish"))
                .args(["--deck", path.to_str().unwrap(), "--games", "1"])
                .output()
                .unwrap(),
        );
        #[cfg(feature = "tui")]
        {
            let args = vec!["tui".into(), "--deck".into(), path.display().to_string()];
            let tui = mtg_gto::tui::resolve_deck_args(&args).unwrap();
            assert_eq!(tui.setup_cards, direct.setup_cards);
            assert!(!tui.benchmark_ready());
            assert_blocked_output(
                std::process::Command::new(env!("CARGO_BIN_EXE_tui"))
                    .args(["--deck", path.to_str().unwrap()])
                    .output()
                    .unwrap(),
            );
        }
    }
}

#[test]
fn renamed_exact_deck_with_tutor_metadata_is_rejected_before_gameplay() {
    for with_sidecar in [false, true] {
        let path = renamed_official_deck(with_sidecar);
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("\n~~Tutor Targets~~\n1 Mountain\n");
        std::fs::write(&path, text).unwrap();
        let err = match deck_loader::resolve_file(&path) {
            Ok(_) => panic!("accepted tutor restriction on fixed deck"),
            Err(err) => err,
        };
        assert!(
            err.contains("fixed benchmark deck may not restrict tutor targets"),
            "{err}"
        );
        let cli = std::process::Command::new(env!("CARGO_BIN_EXE_goldfish"))
            .args(["--deck", path.to_str().unwrap(), "--games", "1"])
            .output()
            .unwrap();
        assert_eq!(cli.status.code(), Some(1));
        let cli_text = format!(
            "{}{}",
            String::from_utf8_lossy(&cli.stdout),
            String::from_utf8_lossy(&cli.stderr)
        );
        assert!(cli_text.contains("fixed benchmark deck may not restrict tutor targets"));
        assert!(!cli_text.contains("Win rate:") && !cli_text.contains("Simulating"));
        #[cfg(feature = "tui")]
        {
            let args = vec!["tui".into(), "--deck".into(), path.display().to_string()];
            let tui_error = match mtg_gto::tui::resolve_deck_args(&args) {
                Ok(_) => panic!("TUI accepted tutor restriction on fixed deck"),
                Err(err) => err,
            };
            assert!(tui_error.contains("fixed benchmark deck may not restrict tutor targets"));
            let tui = std::process::Command::new(env!("CARGO_BIN_EXE_tui"))
                .args(["--deck", path.to_str().unwrap()])
                .output()
                .unwrap();
            assert_eq!(tui.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&tui.stderr)
                .contains("fixed benchmark deck may not restrict tutor targets"));
        }
    }
}

#[test]
fn sidecar_only_tutor_identity_cannot_hide_the_fixed_deck() {
    let path = renamed_official_deck(true);
    let sidecar = path.with_extension("cards.json");
    let mut definitions: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
    let mut extra = definitions
        .iter()
        .find(|definition| definition["name"] == "Pashalik Mons")
        .unwrap()
        .clone();
    extra["id"] = serde_json::json!(990003);
    extra["name"] = serde_json::json!("Sidecar Only Tutor");
    definitions.push(extra);
    std::fs::write(&sidecar, serde_json::to_vec(&definitions).unwrap()).unwrap();
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n~~Tutor Targets~~\n1 Sidecar Only Tutor\n");
    std::fs::write(&path, text).unwrap();
    let err = match deck_loader::resolve_file(&path) {
        Ok(_) => panic!("sidecar-only tutor let exact fixed deck simulate"),
        Err(err) => err,
    };
    assert!(err.contains("unknown card"), "{err}");
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_goldfish"))
        .args(["--deck", path.to_str().unwrap(), "--games", "1"])
        .output()
        .unwrap();
    assert_eq!(cli.status.code(), Some(1));
    let cli_text = format!(
        "{}{}",
        String::from_utf8_lossy(&cli.stdout),
        String::from_utf8_lossy(&cli.stderr)
    );
    assert!(cli_text.contains("unknown card"));
    assert!(!cli_text.contains("Win rate:") && !cli_text.contains("Simulating"));
    #[cfg(feature = "tui")]
    {
        let tui = std::process::Command::new(env!("CARGO_BIN_EXE_tui"))
            .args(["--deck", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(tui.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&tui.stderr).contains("unknown card"));
    }
}

#[test]
fn cold_offline_exact_100_and_command_zone() {
    let result = resolve(Path::new("decks/goblin_storm.txt"));
    assert_eq!(result.catalog.as_ref().unwrap().cards.len(), 79);
    assert_eq!(
        result
            .catalog
            .as_ref()
            .unwrap()
            .cards
            .iter()
            .filter(|card| { card.support.origin == deck_loader::DefinitionOrigin::IdentityOnly })
            .count(),
        64
    );
    assert_eq!(result.mainboard.len(), 99);
    assert_eq!(result.setup_cards.len(), 100);
    assert_eq!(
        result
            .setup_cards
            .iter()
            .filter(|&&id| id == result.commander)
            .count(),
        1
    );
    assert_eq!(
        result
            .setup_cards
            .iter()
            .filter(|&&id| id == sample::ids::MOUNTAIN)
            .count(),
        22
    );
    assert_eq!(
        result
            .mainboard
            .iter()
            .filter(|&&id| { id != sample::ids::MOUNTAIN && result.db.get(id).unwrap().is_land() })
            .count(),
        14
    );
    assert!(!result.benchmark_ready());
    assert!(result
        .readiness_reasons
        .iter()
        .any(|reason| reason.contains("runner/RNG/hidden-information")));

    let (opp_deck, opp_commander) = deck_loader::passive_opponent_setup_cards();
    assert_eq!(opp_deck.len(), 100);
    assert_eq!(
        opp_deck
            .iter()
            .filter(|&&id| id == sample::ids::FOREST)
            .count(),
        99
    );
    let mut state = GameState::new_commander(2);
    state.card_db = Some(Arc::new(result.db.clone()));
    rules::setup_commander_game(
        &mut state,
        &result.setup_cards,
        &opp_deck,
        result.commander,
        opp_commander,
    );
    assert_eq!(state.players[0].life, 40);
    assert_eq!(state.players[1].life, 40);
    assert_eq!(state.players[0].library.len(), 92);
    assert_eq!(state.players[1].library.len(), 92);
    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(state.players[1].hand.len(), 7);
    assert!(state.battlefield.is_empty());
    assert_eq!(state.players[0].command_zone.len(), 1);
    assert_eq!(state.players[1].command_zone.len(), 1);
    assert_eq!(
        state.objects[&state.players[1].command_zone[0]].card_def_id,
        opp_commander
    );
}

#[test]
fn preset_and_file_share_exact_resolution() {
    let direct = resolve(Path::new("decks/goblin_storm.txt"));
    let preset = deck_loader::resolve_preset("goblin_storm").unwrap();
    assert_eq!(direct.setup_cards, preset.setup_cards);
    assert_eq!(direct.catalog_fingerprint, preset.catalog_fingerprint);
    assert_eq!(direct.readiness_reasons, preset.readiness_reasons);
    assert!(deck_loader::resolve_preset("goblin_strom").is_err());
    #[cfg(feature = "tui")]
    {
        let args = vec![
            "tui".into(),
            "--deck".into(),
            "decks/goblin_storm.txt".into(),
        ];
        let tui_file = mtg_gto::tui::resolve_deck_args(&args).unwrap();
        let args = vec!["tui".into(), "--preset".into(), "goblin_storm".into()];
        let tui_preset = mtg_gto::tui::resolve_deck_args(&args).unwrap();
        assert_eq!(direct.setup_cards, tui_file.setup_cards);
        assert_eq!(direct.setup_cards, tui_preset.setup_cards);
        assert!(!tui_file.benchmark_ready() && !tui_preset.benchmark_ready());
    }
}

#[test]
fn exact_list_and_commander_validation_rejects_mutations() {
    let original = official_text();
    for (changed, expected) in [
        (
            original.replace("1 Pashalik Mons", "1 Unknown Goblin"),
            "unknown card",
        ),
        (
            original.replace("1 Pashalik Mons", "1 Krenko, Mob Boss"),
            "non-basic",
        ),
        (original.replace("1 Pashalik Mons\n", ""), "99 cards"),
        (
            original.replace("1 Pashalik Mons", "1 Goblin Guide"),
            "exact 99-card mainboard",
        ),
        (
            original.replace("1 Pashalik Mons", "1 Zada, Hedron Grinder"),
            "commander also appears",
        ),
        (
            original.replace("1 Zada, Hedron Grinder", "2 Zada, Hedron Grinder"),
            "quantity one",
        ),
        (
            original.replace("~~Mainboard~~", "~~Surprise~~"),
            "unknown section",
        ),
        (
            original.replace("7 Mountain", "107 Mountain"),
            "quantity exceeds",
        ),
    ] {
        let path = fixture(&changed);
        let error = match deck_loader::resolve_file(&path) {
            Ok(_) => panic!("accepted invalid deck"),
            Err(e) => e,
        };
        assert!(error.contains(expected), "expected '{expected}' in {error}");
    }
    // Repeated Mountain rows are legal and aggregate to exactly 22.
    assert_eq!(
        resolve(Path::new("decks/goblin_storm.txt")).mainboard.len(),
        99
    );
}

#[test]
fn catalog_rejects_corruption_and_duplicate_identities() {
    let raw = std::fs::read_to_string(deck_loader::fixed_catalog_path()).unwrap();
    assert!(FixedCatalog::parse("{broken").is_err());
    let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    value["cards"][1]["id"] = value["cards"][0]["id"].clone();
    assert!(FixedCatalog::parse(&value.to_string())
        .unwrap_err()
        .contains("duplicate catalog identity"));
    let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    value["cards"][2]["support"]["blocking_capabilities"] = serde_json::json!(["unregistered_gap"]);
    assert!(FixedCatalog::parse(&value.to_string())
        .unwrap_err()
        .contains("unknown capability"));
}

#[test]
fn authored_behavior_wins_even_when_catalog_id_differs() {
    let raw = std::fs::read_to_string(deck_loader::fixed_catalog_path()).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let cards = value["cards"].as_array_mut().unwrap();
    for card in cards {
        if card["name"] == "Zada, Hedron Grinder" {
            card["id"] = serde_json::json!(990001);
        }
        if card["name"] == "Expedite" {
            card["id"] = serde_json::json!(990002);
        }
    }
    let dir = std::env::temp_dir().join(format!(
        "mtg-m1-catalog-{}",
        NEXT_CASE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let catalog_path = dir.join("catalog.json");
    std::fs::write(&catalog_path, value.to_string()).unwrap();
    let loaded = deck_loader::resolve_file_with_catalog(
        Path::new("decks/goblin_storm.txt"),
        Some(&catalog_path),
    )
    .unwrap();
    assert_eq!(loaded.commander, sample::ids::ZADA_HEDRON_GRINDER);
    let sample = sample::build_sample_db();
    for name in ["Zada, Hedron Grinder", "Expedite"] {
        let authored = sample.get(sample.find_by_name(name).unwrap()).unwrap();
        let resolved = loaded
            .db
            .get(loaded.db.find_by_name(name).unwrap())
            .unwrap();
        assert_eq!(authored.spell_effect, resolved.spell_effect);
        assert_eq!(authored.triggered_abilities, resolved.triggered_abilities);
    }
}

#[test]
fn support_is_positive_and_blockers_do_not_auto_certify() {
    let result = resolve(Path::new("decks/goblin_storm.txt"));
    let catalog = result.catalog.as_ref().unwrap();
    let by_name: HashMap<_, _> = catalog
        .cards
        .iter()
        .map(|card| (card.name.as_str(), card))
        .collect();
    assert!(catalog
        .impacted_by("death_events")
        .iter()
        .any(|card| card.name == "Skullclamp"));
    let mut skullclamp = by_name["Skullclamp"].clone();
    skullclamp.support.blocking_capabilities.clear();
    let def = result
        .db
        .get(result.db.find_by_name("Skullclamp").unwrap())
        .unwrap();
    assert_eq!(skullclamp.support.status, SupportStatus::Partial);
    assert!(certification_issue(&skullclamp, def).is_some());
    let mut throne = by_name["Roaming Throne"].clone();
    throne.support.status = SupportStatus::Supported;
    throne.support.blocking_capabilities.clear();
    throne.support.reviewed_at = Some("2026-10-03".into());
    throne.support.evidence.push("hypothetical test".into());
    let def = result
        .db
        .get(result.db.find_by_name("Roaming Throne").unwrap())
        .unwrap();
    throne.support.behavior_fingerprint =
        Some(stable_fingerprint(&serde_json::to_vec(def).unwrap()));
    assert!(certification_issue(&throne, def)
        .unwrap()
        .contains("printed abilities absent"));
    let mut expedite = result
        .db
        .get(result.db.find_by_name("Expedite").unwrap())
        .unwrap()
        .clone();
    expedite.spell_effect = Some(Effect::Multiple(vec![Effect::Unimplemented(
        "nested".into(),
    )]));
    let mut entry = by_name["Expedite"].clone();
    entry.support.status = SupportStatus::Supported;
    entry.support.blocking_capabilities.clear();
    entry.support.reviewed_at = Some("2026-10-03".into());
    entry.support.evidence.push("hypothetical test".into());
    entry.support.behavior_fingerprint =
        Some(stable_fingerprint(&serde_json::to_vec(&expedite).unwrap()));
    assert_eq!(
        certification_issue(&entry, &expedite).unwrap(),
        "nested Unimplemented effect"
    );
    for empty in [
        Effect::Multiple(vec![]),
        Effect::Multiple(vec![Effect::Multiple(vec![])]),
        Effect::Modal {
            choices: vec![],
            choose_count: 1,
        },
        Effect::Modal {
            choices: vec![Effect::Multiple(vec![])],
            choose_count: 1,
        },
        Effect::Conditional {
            condition: Condition::HandIsEmpty,
            if_true: Box::new(Effect::Multiple(vec![])),
            if_false: None,
        },
        Effect::ForEach {
            count: DynamicValue::Fixed(1),
            effect: Box::new(Effect::Modal {
                choices: vec![],
                choose_count: 1,
            }),
        },
    ] {
        expedite.spell_effect = Some(empty);
        entry.support.behavior_fingerprint =
            Some(stable_fingerprint(&serde_json::to_vec(&expedite).unwrap()));
        assert!(certification_issue(&entry, &expedite)
            .unwrap()
            .contains("empty effect"));
    }
    expedite.spell_effect = Some(Effect::DrawCards { count: 1 });
    assert_eq!(
        certification_issue(&entry, &expedite).unwrap(),
        "stale behavior certification"
    );
}

#[test]
fn positive_certification_requires_substantive_review_fields_at_both_boundaries() {
    let resolved = resolve(Path::new("decks/goblin_storm.txt"));
    let mut entry = resolved
        .catalog
        .as_ref()
        .unwrap()
        .cards
        .iter()
        .find(|card| card.name == "Expedite")
        .unwrap()
        .clone();
    let def = resolved
        .db
        .get(resolved.db.find_by_name("Expedite").unwrap())
        .unwrap();
    entry.support.status = SupportStatus::Supported;
    entry.support.blocking_capabilities.clear();
    entry.support.reviewed_at = Some("2026-10-03".into());
    entry.support.evidence = vec!["reviewed integration trace".into()];
    entry.support.behavior_fingerprint =
        Some(stable_fingerprint(&serde_json::to_vec(def).unwrap()));
    assert_eq!(certification_issue(&entry, def), None);

    let original: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(deck_loader::fixed_catalog_path()).unwrap())
            .unwrap();
    for (date, evidence) in [
        (None, vec!["reviewed integration trace"]),
        (Some(""), vec!["reviewed integration trace"]),
        (Some(" \t "), vec!["reviewed integration trace"]),
        (Some("2026-10-03"), vec![]),
        (Some("2026-10-03"), vec![""]),
        (Some("2026-10-03"), vec!["  \t"]),
        (Some("2026-10-03"), vec!["reviewed integration trace", "  "]),
    ] {
        let mut candidate = entry.clone();
        candidate.support.reviewed_at = date.map(str::to_owned);
        candidate.support.evidence = evidence.into_iter().map(str::to_owned).collect();
        assert!(certification_issue(&candidate, def).is_some());
        let mut json = original.clone();
        let card = json["cards"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|card| card["name"] == "Expedite")
            .unwrap();
        card["support"] = serde_json::to_value(&candidate.support).unwrap();
        assert!(FixedCatalog::parse(&json.to_string()).is_err());
    }
    let mut json = original;
    let card = json["cards"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|card| card["name"] == "Expedite")
        .unwrap();
    card["support"] = serde_json::to_value(&entry.support).unwrap();
    assert!(FixedCatalog::parse(&json.to_string()).is_ok());
}

#[test]
fn printed_metadata_and_binary_tags_are_distinct() {
    let result = resolve(Path::new("decks/goblin_storm.txt"));
    let catalog = result.catalog.as_ref().unwrap();
    let boggart = catalog
        .cards
        .iter()
        .find(|card| card.name == "Boggart Shenanigans")
        .unwrap();
    assert_eq!(
        boggart.card_types,
        vec![CardType::Kindred, CardType::Enchantment]
    );
    let siege = catalog
        .cards
        .iter()
        .find(|card| card.name == "Siege-Gang Commander")
        .unwrap();
    assert!(siege.oracle_text.contains("{1}{R}, Sacrifice a Goblin:"));
    assert!(siege.oracle_text.contains("2 damage to any target"));
    assert_eq!(siege.support.status, SupportStatus::Partial);
    let sample_db = sample::build_sample_db();
    let sample_siege = sample_db
        .get(sample_db.find_by_name("Siege-Gang Commander").unwrap())
        .unwrap();
    assert_eq!(sample_siege.oracle_text, siege.oracle_text);
    assert!(sample_siege.activated_abilities.is_empty());
    let throne = catalog
        .cards
        .iter()
        .find(|card| card.name == "Roaming Throne")
        .unwrap();
    assert_eq!(throne.subtypes, vec!["Golem"]);
    assert!(throne.oracle_text.contains("Ward {2}"));
    assert!(!throne.oracle_text.contains("Changeling"));
    let shinka = catalog
        .cards
        .iter()
        .find(|card| card.name == "Shinka, the Bloodsoaked Keep")
        .unwrap();
    assert!(shinka.printed_colors.is_empty());
    assert_eq!(shinka.color_identity, vec![mtg_gto::mana::Color::Red]);
    let shinka_id = result
        .db
        .find_by_name("Shinka, the Bloodsoaked Keep")
        .unwrap();
    assert_eq!(
        result.color_identity(shinka_id),
        Some(vec![mtg_gto::mana::Color::Red])
    );
    assert_eq!(
        bincode::serialize(&CardType::Land).unwrap(),
        vec![6, 0, 0, 0]
    );
    assert_eq!(
        bincode::serialize(&CardType::Kindred).unwrap(),
        vec![7, 0, 0, 0]
    );
}

#[test]
fn malformed_present_sidecar_is_not_silently_ignored() {
    let path = std::env::temp_dir().join(format!(
        "mtg-sidecar-{}.cards.json",
        NEXT_CASE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, "not-json").unwrap();
    assert!(mtg_gto::deck_import::load_extra_card_defs(&path).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn blocked_cli_never_reports_performance() {
    for flags in [
        vec!["--preset", "goblin_storm", "--games", "1"],
        vec!["--deck", "decks/goblin_storm.txt", "--trace"],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_goldfish"))
            .args(flags)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(combined.contains("BLOCKED"));
        assert!(!combined.contains("Win rate:"));
        assert!(!combined.contains("Result:"));
        assert!(!combined.contains("Simulating"));
    }
}

#[cfg(feature = "tui")]
#[test]
fn blocked_tui_exits_before_terminal_gameplay() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tui"))
        .args(["--deck", "decks/goblin_storm.txt"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("blocked"));
}
