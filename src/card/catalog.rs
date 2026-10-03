//! Canonical card catalog and ID mapping.

use crate::card::CardId;
use crate::game::CardDatabase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardCatalogEntry {
    pub id: CardId,
    pub key: &'static str,
    pub name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardImplementationStatus {
    pub id: CardId,
    pub key: &'static str,
    pub name: &'static str,
    pub effects_implemented: bool,
}

pub mod ids {
    pub const MOUNTAIN: u64 = 1;
    pub const FOREST: u64 = 2;
    pub const PLAINS: u64 = 3;
    pub const ISLAND: u64 = 4;
    pub const SWAMP: u64 = 5;
    pub const LIGHTNING_BOLT: u64 = 100;
    pub const GIANT_GROWTH: u64 = 101;
    pub const GRIZZLY_BEARS: u64 = 102;
    pub const GREY_OGRE: u64 = 103;
    pub const SERRA_ANGEL: u64 = 104;
    pub const SHIVAN_DRAGON: u64 = 105;
    pub const LLANOWAR_ELVES: u64 = 106;
    pub const SWORDS_TO_PLOWSHARES: u64 = 107;
    pub const COUNTERSPELL: u64 = 108;
    pub const DARK_RITUAL: u64 = 109;
    pub const SAVANNAH_LIONS: u64 = 110;
    pub const SHOCK: u64 = 111;
    pub const GOBLIN_GUIDE: u64 = 112;
    pub const ELVISH_MYSTIC: u64 = 113;
    pub const KALONIAN_TUSKER: u64 = 114;
    pub const LEATHERBACK_BALOTH: u64 = 115;
    pub const MONASTERY_SWIFTSPEAR: u64 = 116;
    pub const LAVA_SPIKE: u64 = 117;
    pub const RIFT_BOLT: u64 = 118;
    pub const STOMPING_GROUND: u64 = 119;
    pub const ELVISH_VISIONARY: u64 = 120;
    pub const BLADE_SPLICER: u64 = 121;
    pub const SIEGE_GANG_COMMANDER: u64 = 122;
    pub const FIERY_CONCLUSION_ELEMENTAL: u64 = 200;
    pub const PYROCLASM_ELEMENTAL: u64 = 201;
    pub const MOTHER_OF_RUNES: u64 = 300;
    pub const ELITE_VANGUARD: u64 = 301;
    pub const WHITE_KNIGHT: u64 = 302;
    pub const LEONIN_SKYHUNTER: u64 = 303;
    pub const BANESLAYER_ANGEL: u64 = 304;
    pub const THALIA_GUARDIAN: u64 = 305;
    pub const BRIMAZ_KING: u64 = 306;
    pub const SOLDIER_OF_THE_PANTHEON: u64 = 307;
    pub const HERO_OF_BLADEHOLD: u64 = 308;
    pub const PRECINCT_CAPTAIN: u64 = 309;
    pub const PATH_TO_EXILE: u64 = 310;
    pub const WRATH_OF_GOD: u64 = 311;
    pub const DAY_OF_JUDGMENT: u64 = 312;
    pub const OBLIVION_RING: u64 = 313;
    pub const DISENCHANT: u64 = 314;
    pub const GLORIOUS_ANTHEM: u64 = 315;
    pub const HONOR_OF_THE_PURE: u64 = 316;
    pub const CRUSADE: u64 = 317;
    pub const HUMILITY: u64 = 318;
    pub const DELVER_OF_SECRETS: u64 = 320;
    pub const SNAPCASTER_MAGE: u64 = 321;
    pub const VENDILION_CLIQUE: u64 = 322;
    pub const MAN_O_WAR: u64 = 323;
    pub const SERENDIB_EFREET: u64 = 324;
    pub const PHANTASMAL_BEAR: u64 = 325;
    pub const MANA_LEAK: u64 = 326;
    pub const REMAND: u64 = 327;
    pub const BRAINSTORM: u64 = 328;
    pub const PONDER: u64 = 329;
    pub const UNSUMMON: u64 = 330;
    pub const DARK_CONFIDANT: u64 = 340;
    pub const HYPNOTIC_SPECTER: u64 = 341;
    pub const NANTUKO_SHADE: u64 = 342;
    pub const VAMPIRE_NIGHTHAWK: u64 = 343;
    pub const GATEKEEPER_OF_MALAKIR: u64 = 344;
    pub const BLOODGHAST: u64 = 345;
    pub const GERALF_MESSENGER: u64 = 346;
    pub const PHYREXIAN_OBLITERATOR: u64 = 347;
    pub const KNIGHT_OF_THE_EBON_LEGION: u64 = 348;
    pub const DOOM_BLADE: u64 = 350;
    pub const GO_FOR_THE_THROAT: u64 = 351;
    pub const THOUGHTSEIZE: u64 = 352;
    pub const HYMN_TO_TOURACH: u64 = 353;
    pub const DIABOLIC_EDICT: u64 = 354;
    pub const TRAGIC_SLIP: u64 = 355;
    pub const ASH_ZEALOT: u64 = 360;
    pub const EMBER_HAULER: u64 = 361;
    pub const HELLRIDER: u64 = 362;
    pub const JACKAL_PUP: u64 = 363;
    pub const KELDON_MARAUDERS: u64 = 364;
    pub const VEXING_DEVIL: u64 = 365;
    pub const EIDOLON_OF_GREAT_REVEL: u64 = 366;
    pub const YOUNG_PYROMANCER: u64 = 367;
    pub const GOBLIN_CHAINWHIRLER: u64 = 368;
    pub const CHAIN_LIGHTNING: u64 = 370;
    pub const SEARING_BLAZE: u64 = 371;
    pub const SKULLCRACK: u64 = 372;
    pub const FLAMES_OF_THE_BLOOD_HAND: u64 = 373;
    pub const SEARING_BLOOD: u64 = 374;
    pub const TARMOGOYF: u64 = 380;
    pub const SCAVENGING_OOZE: u64 = 381;
    pub const STRANGLEROOT_GEIST: u64 = 382;
    pub const WILD_NACATL: u64 = 383;
    pub const EXPERIMENT_ONE: u64 = 384;
    pub const DRYAD_MILITANT: u64 = 385;
    pub const THRUN_LAST_TROLL: u64 = 386;
    pub const RANCOR_BEAST: u64 = 387;
    pub const RANCOR: u64 = 390;
    pub const VINES_OF_VASTWOOD: u64 = 391;
    pub const COLLECTED_COMPANY: u64 = 392;
    pub const GAEA_ANTHEM: u64 = 393;
    pub const SOL_RING: u64 = 400;
    pub const SIGNAL_PEST: u64 = 401;
    pub const VAULT_SKIRGE: u64 = 402;
    pub const CRANIAL_PLATING: u64 = 403;
    pub const STEEL_OVERSEER: u64 = 404;
    pub const LIGHTNING_HELIX: u64 = 410;
    pub const TERMINATE: u64 = 411;
    pub const GEIST_OF_SAINT_TRAFT: u64 = 412;
    pub const FLEECEMANE_LION: u64 = 413;
    pub const TIDEHOLLOW_SCULLER: u64 = 414;
    pub const ANCIENT_TOMB: u64 = 500;
    pub const BOSEIJU_WHO_ENDURES: u64 = 501;
    pub const BREEDING_POOL: u64 = 502;
    pub const COMMAND_TOWER: u64 = 503;
    pub const FLOODED_STRAND: u64 = 504;
    pub const GAEAS_CRADLE: u64 = 505;
    pub const GEMSTONE_CAVERNS: u64 = 506;
    pub const INVENTORS_FAIR: u64 = 507;
    pub const MINAMO_SCHOOL: u64 = 508;
    pub const MISTY_RAINFOREST: u64 = 509;
    pub const MISTRISE_VILLAGE: u64 = 510;
    pub const OTAWARA_SOARING_CITY: u64 = 511;
    pub const SEAT_OF_THE_SYNOD: u64 = 512;
    pub const SHIFTING_WOODLAND: u64 = 513;
    pub const SNOW_COVERED_FOREST: u64 = 514;
    pub const SNOW_COVERED_ISLAND: u64 = 515;
    pub const TREASURE_VAULT: u64 = 516;
    pub const TREE_OF_TALES: u64 = 517;
    pub const TROPICAL_ISLAND: u64 = 518;
    pub const WATERLOGGED_GROVE: u64 = 519;
    pub const WINDSWEPT_HEATH: u64 = 520;
    pub const YAVIMAYA_COAST: u64 = 521;
    pub const ARCANE_SIGNET: u64 = 550;
    pub const BASALT_MONOLITH: u64 = 551;
    pub const CHROME_MOX: u64 = 552;
    pub const FELLWAR_STONE: u64 = 553;
    pub const GRIM_MONOLITH: u64 = 554;
    pub const LOTUS_PETAL: u64 = 555;
    pub const MANA_VAULT: u64 = 556;
    pub const MOX_AMBER: u64 = 557;
    pub const MOX_DIAMOND: u64 = 558;
    pub const MOX_OPAL: u64 = 559;
    pub const MOONSILVER_KEY: u64 = 560;
    pub const SIMIC_SIGNET: u64 = 561;
    pub const SPRINGLEAF_DRUM: u64 = 562;
    pub const TALISMAN_OF_CURIOSITY: u64 = 563;
    pub const AGATHAS_SOUL_CAULDRON: u64 = 580;
    pub const THE_ONE_RING: u64 = 581;
    pub const MIRAGE_MIRROR: u64 = 582;
    pub const WALKING_BALLISTA: u64 = 583;
    pub const KINNAN_BONDER_PRODIGY: u64 = 600;
    pub const BIRDS_OF_PARADISE: u64 = 601;
    pub const FYNDHORN_ELVES: u64 = 602;
    pub const DELIGHTED_HALFLING: u64 = 603;
    pub const BADGERMOLE_CUB: u64 = 604;
    pub const CLEVER_IMPERSONATOR: u64 = 605;
    pub const COLOSSAL_SKYTURTLE: u64 = 606;
    pub const CONSECRATED_SPHINX: u64 = 607;
    pub const DRIFT_OF_PHANTASMS: u64 = 608;
    pub const ELVISH_SPIRIT_GUIDE: u64 = 609;
    pub const ENDURANCE: u64 = 610;
    pub const ENDURING_VITALITY: u64 = 611;
    pub const FAERIE_MASTERMIND: u64 = 612;
    pub const FLESH_DUPLICATE: u64 = 613;
    pub const HIGH_FAE_TRICKSTER: u64 = 614;
    pub const HULLBREAKER_HORROR: u64 = 615;
    pub const MOCKINGBIRD: u64 = 616;
    pub const NEZAHAL_PRIMAL_TIDE: u64 = 617;
    pub const NYXBLOOM_ANCIENT: u64 = 618;
    pub const PHYREXIAN_METAMORPH: u64 = 619;
    pub const SEEDBORN_MUSE: u64 = 620;
    pub const THRASIOS_TRITON_HERO: u64 = 621;
    pub const TIDESPOUT_TYRANT: u64 = 622;
    pub const TROPHY_MAGE: u64 = 623;
    pub const WAN_SHI_TONG: u64 = 624;
    pub const WANDERING_ARCHAIC: u64 = 625;
    pub const AN_OFFER_YOU_CANT_REFUSE: u64 = 700;
    pub const CHORD_OF_CALLING: u64 = 701;
    pub const CROP_ROTATION: u64 = 702;
    pub const CYCLONIC_RIFT: u64 = 703;
    pub const FIERCE_GUARDIANSHIP: u64 = 704;
    pub const FLUSTERSTORM: u64 = 705;
    pub const FORCE_OF_NEGATION: u64 = 706;
    pub const FORCE_OF_WILL: u64 = 707;
    pub const INTO_THE_FLOOD_MAW: u64 = 708;
    pub const MENTAL_MISSTEP: u64 = 709;
    pub const MINDBREAK_TRAP: u64 = 710;
    pub const MYSTICAL_TUTOR: u64 = 711;
    pub const NOXIOUS_REVIVAL: u64 = 712;
    pub const PACT_OF_NEGATION: u64 = 713;
    pub const SWAN_SONG: u64 = 714;
    pub const VEIL_OF_SUMMER: u64 = 715;
    pub const WHIR_OF_INVENTION: u64 = 716;
    pub const WORLDLY_TUTOR: u64 = 717;
    pub const MUDDLE_THE_MIXTURE: u64 = 718;
    pub const FINALE_OF_DEVASTATION: u64 = 750;
    pub const GREEN_SUNS_ZENITH: u64 = 751;
    pub const NATURES_RHYTHM: u64 = 752;
    pub const MYSTIC_REMORA: u64 = 770;
    pub const RHYSTIC_STUDY: u64 = 771;
    pub const TEZZERET_THE_SEEKER: u64 = 790;
    pub const BRIDGEWORKS_BATTLE: u64 = 800;
    pub const DISCIPLE_OF_FREYALISE: u64 = 801;
    pub const HYDROELECTRIC_SPECIMEN: u64 = 802;
    pub const INVASION_OF_IKORIA: u64 = 803;
    pub const SINK_INTO_STUPOR: u64 = 804;

    // =====================================================================
    // Ashcoat of the Shadow Swarm Commander Deck
    // =====================================================================

    // --- Commander ---
    pub const ASHCOAT_OF_THE_SHADOW_SWARM: u64 = 900;

    // --- Creatures ---
    pub const ASSASSIN_INITIATE: u64 = 901;
    pub const AYARA_FIRST_OF_LOCTHWAIN: u64 = 902;
    pub const BLOOD_ARTIST: u64 = 903;
    pub const BLOODLINE_PRETENDER: u64 = 904;
    pub const BURGLAR_RAT: u64 = 905;
    pub const CHANGELING_OUTCAST: u64 = 906;
    pub const CHITTERING_RATS: u64 = 907;
    pub const CHITTERING_WITCH: u64 = 908;
    pub const CRYPT_GHAST: u64 = 909;
    pub const FALKENRATH_NOBLE: u64 = 910;
    pub const GNAT_MISER: u64 = 911;
    pub const INK_EYES_SERVANT_OF_ONI: u64 = 912;
    pub const KARUMONIX_THE_RAT_KING: u64 = 913;
    pub const LORD_SKITTER_SEWER_KING: u64 = 914;
    pub const MARROW_GNAWER: u64 = 915;
    pub const MIKAEUS_THE_UNHALLOWED: u64 = 916;
    pub const NASHI_MOON_SAGES_SCION: u64 = 917;
    pub const NEZUMI_BONE_READER: u64 = 918;
    pub const NEZUMI_CUTTHROAT: u64 = 919;
    pub const NEZUMI_GRAVEROBBER: u64 = 920;
    pub const NEZUMI_SHORTFANG: u64 = 921;
    pub const NIRKANA_REVENANT: u64 = 922;
    pub const OGRE_SLUMLORD: u64 = 923;
    pub const PACK_RAT: u64 = 924;
    pub const RATCATCHER: u64 = 925;
    pub const RAVENOUS_RATS: u64 = 926;
    pub const REFURBISHED_FAMILIAR: u64 = 927;
    pub const ROAMING_THRONE: u64 = 928;
    pub const SKULLSNATCHER: u64 = 929;
    pub const SPECIES_SPECIALIST: u64 = 930;
    pub const TYPHOID_RATS: u64 = 931;
    pub const VALLEY_ROTCALLER: u64 = 932;
    pub const ZULAPORT_CUTTHROAT: u64 = 933;

    // --- Artifacts ---
    pub const BONTUS_MONUMENT: u64 = 950;
    pub const CAGED_SUN: u64 = 951;
    pub const COAT_OF_ARMS: u64 = 952;
    pub const CRYPTOLITH_FRAGMENT: u64 = 953;
    pub const DARKSTEEL_INGOT: u64 = 954;
    pub const DOOR_OF_DESTINIES: u64 = 955;
    pub const HERALDS_HORN: u64 = 956;
    pub const JET_MEDALLION: u64 = 957;
    pub const NIM_DEATHMANTLE: u64 = 958;
    pub const SEMBLANCE_ANVIL: u64 = 959;
    pub const SKULLCLAMP: u64 = 960;
    pub const STRIONIC_RESONATOR: u64 = 961;
    pub const THE_IMMORTAL_SUN: u64 = 962;
    pub const THORNBITE_STAFF: u64 = 963;
    pub const THRAN_DYNAMO: u64 = 964;
    pub const THRONE_OF_THE_GOD_PHARAOH: u64 = 965;
    pub const URZAS_INCUBATOR: u64 = 966;
    pub const VANQUISHERS_BANNER: u64 = 967;

    // --- Enchantments ---
    pub const BLACK_MARKET: u64 = 980;
    pub const BLACK_MARKET_CONNECTIONS: u64 = 981;
    pub const DICTATE_OF_EREBOS: u64 = 982;
    pub const GRAVE_PACT: u64 = 983;
    pub const PHYREXIAN_ARENA: u64 = 984;
    pub const PHYREXIAN_RECLAMATION: u64 = 985;
    pub const BOLASS_CITADEL: u64 = 986;

    // --- Lands ---
    pub const CABAL_COFFERS: u64 = 1000;
    pub const CASTLE_LOCTHWAIN: u64 = 1001;
    pub const CRYPT_OF_AGADEEM: u64 = 1002;
    pub const MIKOKORO_CENTER_OF_THE_SEA: u64 = 1003;
    pub const NYKTHOS_SHRINE_TO_NYX: u64 = 1004;
    pub const PATH_OF_ANCESTRY: u64 = 1005;
    pub const SWARMYARD: u64 = 1006;

    // --- Instants / Sorceries ---
    pub const CHAIN_ASSASSINATION: u64 = 1010;
    pub const LIVING_DEATH: u64 = 1011;

    // --- Flubs deck cards ---
    pub const FLUBS_THE_FOOL: u64 = 1100;
    pub const ABUNDANCE: u64 = 1101;
    pub const AMPHIBIAN_DOWNPOUR: u64 = 1102;
    pub const BIRGI_GOD_OF_STORYTELLING: u64 = 1103;
    pub const BLACKBLADE_REFORGED: u64 = 1104;
    pub const BRIDGE_OF_KHAZAD_DUM: u64 = 1105;
    pub const BUCKLEBURY_FERRY: u64 = 1106;
    pub const CASE_OF_THE_LOCKED_HOTHOUSE: u64 = 1107;
    pub const CHOCOBO_RACETRACK: u64 = 1108;
    pub const CODEX_SHREDDER: u64 = 1109;
    pub const CONDUIT_OF_WORLDS: u64 = 1110;
    pub const CRUCIBLE_OF_WORLDS: u64 = 1111;
    pub const DRAGONBACK_ASSAULT: u64 = 1112;
    pub const DRUID_CLASS: u64 = 1113;
    pub const DRYAD_OF_THE_ILYSIAN_GROVE: u64 = 1114;
    pub const EVERFLOWING_CHALICE: u64 = 1115;
    pub const EXPLORATION: u64 = 1116;
    pub const EXPLORE_CARD: u64 = 1117;
    pub const FORTUNE_TELLERS_TALENT: u64 = 1118;
    pub const GLACIERWOOD_SIEGE: u64 = 1119;
    pub const GUSTHAS_SCEPTER: u64 = 1120;
    pub const LANTERN_OF_INSIGHT: u64 = 1121;
    pub const LIGHTNING_GREAVES: u64 = 1122;
    pub const LOTUS_COBRA: u64 = 1123;
    pub const MIKU_LOST_BUT_SINGING: u64 = 1124;
    pub const MONUMENT_TO_ENDURANCE: u64 = 1125;
    pub const MYSTIC_SANCTUARY: u64 = 1126;
    pub const NULL_BROOCH: u64 = 1127;
    pub const OTHERWORLDLY_GAZE: u64 = 1128;
    pub const PHIAL_OF_GALADRIEL: u64 = 1129;
    pub const PRISMATIC_OMEN: u64 = 1130;
    pub const RENFIELD_DELUSIONAL_MINION: u64 = 1131;
    pub const SABOTENDER: u64 = 1132;
    pub const SAW_IT_COMING: u64 = 1133;
    pub const SCUTE_SWARM: u64 = 1134;
    pub const SIX: u64 = 1135;
    pub const SWIFTFOOT_BOOTS: u64 = 1136;
    pub const THESPIANS_STAGE: u64 = 1137;
    pub const TIFA_LOCKHART: u64 = 1138;
    pub const TIRELESS_PROVISIONER: u64 = 1139;
    pub const VALAKUT_THE_MOLTEN_PINNACLE: u64 = 1140;
    pub const VESUVA: u64 = 1141;
    pub const WALK_IN_CLOSET: u64 = 1142;
    pub const WAYWARD_SWORDTOOTH: u64 = 1143;
    pub const WONDER: u64 = 1144;
    pub const YAVIMAYA_CRADLE_OF_GROWTH: u64 = 1145;
    pub const DRAGONBACK_ASSAULT_TOKEN: u64 = 1146;
    pub const SCUTE_SWARM_INSECT_TOKEN: u64 = 1147;
    pub const CHOCOBO_TOKEN: u64 = 1148;
    pub const SEETHING_SONG: u64 = 1149;
    pub const BATTLE_HYMN: u64 = 1150;
    pub const DRAGON_FODDER: u64 = 1151;
    pub const KRENKOS_COMMAND: u64 = 1152;
    pub const KRENKO_MOB_BOSS: u64 = 1153;
    pub const BRIGHTSTONE_RITUAL: u64 = 1154;
    pub const GOBLIN_CHIEFTAIN: u64 = 1155;
    pub const ZADA_HEDRON_GRINDER: u64 = 1156;
}

pub const ALL_CARDS: &[CardCatalogEntry] = &[
    CardCatalogEntry {
        id: ids::ZADA_HEDRON_GRINDER,
        key: "ZADA_HEDRON_GRINDER",
        name: "Zada, Hedron Grinder",
    },
    CardCatalogEntry {
        id: ids::GOBLIN_CHIEFTAIN,
        key: "GOBLIN_CHIEFTAIN",
        name: "Goblin Chieftain",
    },
    CardCatalogEntry {
        id: ids::BRIGHTSTONE_RITUAL,
        key: "BRIGHTSTONE_RITUAL",
        name: "Brightstone Ritual",
    },
    CardCatalogEntry {
        id: ids::KRENKO_MOB_BOSS,
        key: "KRENKO_MOB_BOSS",
        name: "Krenko, Mob Boss",
    },
    CardCatalogEntry { id: ids::SEETHING_SONG, key: "SEETHING_SONG", name: "Seething Song" },
    CardCatalogEntry { id: ids::BATTLE_HYMN, key: "BATTLE_HYMN", name: "Battle Hymn" },
    CardCatalogEntry { id: ids::DRAGON_FODDER, key: "DRAGON_FODDER", name: "Dragon Fodder" },
    CardCatalogEntry { id: ids::KRENKOS_COMMAND, key: "KRENKOS_COMMAND", name: "Krenko's Command" },

    CardCatalogEntry {
        id: ids::MOUNTAIN,
        key: "MOUNTAIN",
        name: "Mountain",
    },
    CardCatalogEntry {
        id: ids::FOREST,
        key: "FOREST",
        name: "Forest",
    },
    CardCatalogEntry {
        id: ids::PLAINS,
        key: "PLAINS",
        name: "Plains",
    },
    CardCatalogEntry {
        id: ids::ISLAND,
        key: "ISLAND",
        name: "Island",
    },
    CardCatalogEntry {
        id: ids::SWAMP,
        key: "SWAMP",
        name: "Swamp",
    },
    CardCatalogEntry {
        id: ids::LIGHTNING_BOLT,
        key: "LIGHTNING_BOLT",
        name: "Lightning Bolt",
    },
    CardCatalogEntry {
        id: ids::GIANT_GROWTH,
        key: "GIANT_GROWTH",
        name: "Giant Growth",
    },
    CardCatalogEntry {
        id: ids::GRIZZLY_BEARS,
        key: "GRIZZLY_BEARS",
        name: "Grizzly Bears",
    },
    CardCatalogEntry {
        id: ids::GREY_OGRE,
        key: "GREY_OGRE",
        name: "Grey Ogre",
    },
    CardCatalogEntry {
        id: ids::SERRA_ANGEL,
        key: "SERRA_ANGEL",
        name: "Serra Angel",
    },
    CardCatalogEntry {
        id: ids::SHIVAN_DRAGON,
        key: "SHIVAN_DRAGON",
        name: "Shivan Dragon",
    },
    CardCatalogEntry {
        id: ids::LLANOWAR_ELVES,
        key: "LLANOWAR_ELVES",
        name: "Llanowar Elves",
    },
    CardCatalogEntry {
        id: ids::SWORDS_TO_PLOWSHARES,
        key: "SWORDS_TO_PLOWSHARES",
        name: "Swords to Plowshares",
    },
    CardCatalogEntry {
        id: ids::COUNTERSPELL,
        key: "COUNTERSPELL",
        name: "Counterspell",
    },
    CardCatalogEntry {
        id: ids::DARK_RITUAL,
        key: "DARK_RITUAL",
        name: "Dark Ritual",
    },
    CardCatalogEntry {
        id: ids::SAVANNAH_LIONS,
        key: "SAVANNAH_LIONS",
        name: "Savannah Lions",
    },
    CardCatalogEntry {
        id: ids::SHOCK,
        key: "SHOCK",
        name: "Shock",
    },
    CardCatalogEntry {
        id: ids::GOBLIN_GUIDE,
        key: "GOBLIN_GUIDE",
        name: "Goblin Guide",
    },
    CardCatalogEntry {
        id: ids::ELVISH_MYSTIC,
        key: "ELVISH_MYSTIC",
        name: "Elvish Mystic",
    },
    CardCatalogEntry {
        id: ids::KALONIAN_TUSKER,
        key: "KALONIAN_TUSKER",
        name: "Kalonian Tusker",
    },
    CardCatalogEntry {
        id: ids::LEATHERBACK_BALOTH,
        key: "LEATHERBACK_BALOTH",
        name: "Leatherback Baloth",
    },
    CardCatalogEntry {
        id: ids::MONASTERY_SWIFTSPEAR,
        key: "MONASTERY_SWIFTSPEAR",
        name: "Monastery Swiftspear",
    },
    CardCatalogEntry {
        id: ids::LAVA_SPIKE,
        key: "LAVA_SPIKE",
        name: "Lava Spike",
    },
    CardCatalogEntry {
        id: ids::RIFT_BOLT,
        key: "RIFT_BOLT",
        name: "Rift Bolt",
    },
    CardCatalogEntry {
        id: ids::STOMPING_GROUND,
        key: "STOMPING_GROUND",
        name: "Stomping Ground",
    },
    CardCatalogEntry {
        id: ids::ELVISH_VISIONARY,
        key: "ELVISH_VISIONARY",
        name: "Elvish Visionary",
    },
    CardCatalogEntry {
        id: ids::BLADE_SPLICER,
        key: "BLADE_SPLICER",
        name: "Blade Splicer",
    },
    CardCatalogEntry {
        id: ids::SIEGE_GANG_COMMANDER,
        key: "SIEGE_GANG_COMMANDER",
        name: "Siege Gang Commander",
    },
    CardCatalogEntry {
        id: ids::FIERY_CONCLUSION_ELEMENTAL,
        key: "FIERY_CONCLUSION_ELEMENTAL",
        name: "Fiery Conclusion Elemental",
    },
    CardCatalogEntry {
        id: ids::PYROCLASM_ELEMENTAL,
        key: "PYROCLASM_ELEMENTAL",
        name: "Pyroclasm Elemental",
    },
    CardCatalogEntry {
        id: ids::MOTHER_OF_RUNES,
        key: "MOTHER_OF_RUNES",
        name: "Mother of Runes",
    },
    CardCatalogEntry {
        id: ids::ELITE_VANGUARD,
        key: "ELITE_VANGUARD",
        name: "Elite Vanguard",
    },
    CardCatalogEntry {
        id: ids::WHITE_KNIGHT,
        key: "WHITE_KNIGHT",
        name: "White Knight",
    },
    CardCatalogEntry {
        id: ids::LEONIN_SKYHUNTER,
        key: "LEONIN_SKYHUNTER",
        name: "Leonin Skyhunter",
    },
    CardCatalogEntry {
        id: ids::BANESLAYER_ANGEL,
        key: "BANESLAYER_ANGEL",
        name: "Baneslayer Angel",
    },
    CardCatalogEntry {
        id: ids::THALIA_GUARDIAN,
        key: "THALIA_GUARDIAN",
        name: "Thalia Guardian",
    },
    CardCatalogEntry {
        id: ids::BRIMAZ_KING,
        key: "BRIMAZ_KING",
        name: "Brimaz King",
    },
    CardCatalogEntry {
        id: ids::SOLDIER_OF_THE_PANTHEON,
        key: "SOLDIER_OF_THE_PANTHEON",
        name: "Soldier of the Pantheon",
    },
    CardCatalogEntry {
        id: ids::HERO_OF_BLADEHOLD,
        key: "HERO_OF_BLADEHOLD",
        name: "Hero of Bladehold",
    },
    CardCatalogEntry {
        id: ids::PRECINCT_CAPTAIN,
        key: "PRECINCT_CAPTAIN",
        name: "Precinct Captain",
    },
    CardCatalogEntry {
        id: ids::PATH_TO_EXILE,
        key: "PATH_TO_EXILE",
        name: "Path to Exile",
    },
    CardCatalogEntry {
        id: ids::WRATH_OF_GOD,
        key: "WRATH_OF_GOD",
        name: "Wrath of God",
    },
    CardCatalogEntry {
        id: ids::DAY_OF_JUDGMENT,
        key: "DAY_OF_JUDGMENT",
        name: "Day of Judgment",
    },
    CardCatalogEntry {
        id: ids::OBLIVION_RING,
        key: "OBLIVION_RING",
        name: "Oblivion Ring",
    },
    CardCatalogEntry {
        id: ids::DISENCHANT,
        key: "DISENCHANT",
        name: "Disenchant",
    },
    CardCatalogEntry {
        id: ids::GLORIOUS_ANTHEM,
        key: "GLORIOUS_ANTHEM",
        name: "Glorious Anthem",
    },
    CardCatalogEntry {
        id: ids::HONOR_OF_THE_PURE,
        key: "HONOR_OF_THE_PURE",
        name: "Honor of the Pure",
    },
    CardCatalogEntry {
        id: ids::CRUSADE,
        key: "CRUSADE",
        name: "Crusade",
    },
    CardCatalogEntry {
        id: ids::HUMILITY,
        key: "HUMILITY",
        name: "Humility",
    },
    CardCatalogEntry {
        id: ids::DELVER_OF_SECRETS,
        key: "DELVER_OF_SECRETS",
        name: "Delver of Secrets",
    },
    CardCatalogEntry {
        id: ids::SNAPCASTER_MAGE,
        key: "SNAPCASTER_MAGE",
        name: "Snapcaster Mage",
    },
    CardCatalogEntry {
        id: ids::VENDILION_CLIQUE,
        key: "VENDILION_CLIQUE",
        name: "Vendilion Clique",
    },
    CardCatalogEntry {
        id: ids::MAN_O_WAR,
        key: "MAN_O_WAR",
        name: "Man O War",
    },
    CardCatalogEntry {
        id: ids::SERENDIB_EFREET,
        key: "SERENDIB_EFREET",
        name: "Serendib Efreet",
    },
    CardCatalogEntry {
        id: ids::PHANTASMAL_BEAR,
        key: "PHANTASMAL_BEAR",
        name: "Phantasmal Bear",
    },
    CardCatalogEntry {
        id: ids::MANA_LEAK,
        key: "MANA_LEAK",
        name: "Mana Leak",
    },
    CardCatalogEntry {
        id: ids::REMAND,
        key: "REMAND",
        name: "Remand",
    },
    CardCatalogEntry {
        id: ids::BRAINSTORM,
        key: "BRAINSTORM",
        name: "Brainstorm",
    },
    CardCatalogEntry {
        id: ids::PONDER,
        key: "PONDER",
        name: "Ponder",
    },
    CardCatalogEntry {
        id: ids::UNSUMMON,
        key: "UNSUMMON",
        name: "Unsummon",
    },
    CardCatalogEntry {
        id: ids::DARK_CONFIDANT,
        key: "DARK_CONFIDANT",
        name: "Dark Confidant",
    },
    CardCatalogEntry {
        id: ids::HYPNOTIC_SPECTER,
        key: "HYPNOTIC_SPECTER",
        name: "Hypnotic Specter",
    },
    CardCatalogEntry {
        id: ids::NANTUKO_SHADE,
        key: "NANTUKO_SHADE",
        name: "Nantuko Shade",
    },
    CardCatalogEntry {
        id: ids::VAMPIRE_NIGHTHAWK,
        key: "VAMPIRE_NIGHTHAWK",
        name: "Vampire Nighthawk",
    },
    CardCatalogEntry {
        id: ids::GATEKEEPER_OF_MALAKIR,
        key: "GATEKEEPER_OF_MALAKIR",
        name: "Gatekeeper of Malakir",
    },
    CardCatalogEntry {
        id: ids::BLOODGHAST,
        key: "BLOODGHAST",
        name: "Bloodghast",
    },
    CardCatalogEntry {
        id: ids::GERALF_MESSENGER,
        key: "GERALF_MESSENGER",
        name: "Geralf Messenger",
    },
    CardCatalogEntry {
        id: ids::PHYREXIAN_OBLITERATOR,
        key: "PHYREXIAN_OBLITERATOR",
        name: "Phyrexian Obliterator",
    },
    CardCatalogEntry {
        id: ids::KNIGHT_OF_THE_EBON_LEGION,
        key: "KNIGHT_OF_THE_EBON_LEGION",
        name: "Knight of the Ebon Legion",
    },
    CardCatalogEntry {
        id: ids::DOOM_BLADE,
        key: "DOOM_BLADE",
        name: "Doom Blade",
    },
    CardCatalogEntry {
        id: ids::GO_FOR_THE_THROAT,
        key: "GO_FOR_THE_THROAT",
        name: "Go For the Throat",
    },
    CardCatalogEntry {
        id: ids::THOUGHTSEIZE,
        key: "THOUGHTSEIZE",
        name: "Thoughtseize",
    },
    CardCatalogEntry {
        id: ids::HYMN_TO_TOURACH,
        key: "HYMN_TO_TOURACH",
        name: "Hymn to Tourach",
    },
    CardCatalogEntry {
        id: ids::DIABOLIC_EDICT,
        key: "DIABOLIC_EDICT",
        name: "Diabolic Edict",
    },
    CardCatalogEntry {
        id: ids::TRAGIC_SLIP,
        key: "TRAGIC_SLIP",
        name: "Tragic Slip",
    },
    CardCatalogEntry {
        id: ids::ASH_ZEALOT,
        key: "ASH_ZEALOT",
        name: "Ash Zealot",
    },
    CardCatalogEntry {
        id: ids::EMBER_HAULER,
        key: "EMBER_HAULER",
        name: "Ember Hauler",
    },
    CardCatalogEntry {
        id: ids::HELLRIDER,
        key: "HELLRIDER",
        name: "Hellrider",
    },
    CardCatalogEntry {
        id: ids::JACKAL_PUP,
        key: "JACKAL_PUP",
        name: "Jackal Pup",
    },
    CardCatalogEntry {
        id: ids::KELDON_MARAUDERS,
        key: "KELDON_MARAUDERS",
        name: "Keldon Marauders",
    },
    CardCatalogEntry {
        id: ids::VEXING_DEVIL,
        key: "VEXING_DEVIL",
        name: "Vexing Devil",
    },
    CardCatalogEntry {
        id: ids::EIDOLON_OF_GREAT_REVEL,
        key: "EIDOLON_OF_GREAT_REVEL",
        name: "Eidolon of Great Revel",
    },
    CardCatalogEntry {
        id: ids::YOUNG_PYROMANCER,
        key: "YOUNG_PYROMANCER",
        name: "Young Pyromancer",
    },
    CardCatalogEntry {
        id: ids::GOBLIN_CHAINWHIRLER,
        key: "GOBLIN_CHAINWHIRLER",
        name: "Goblin Chainwhirler",
    },
    CardCatalogEntry {
        id: ids::CHAIN_LIGHTNING,
        key: "CHAIN_LIGHTNING",
        name: "Chain Lightning",
    },
    CardCatalogEntry {
        id: ids::SEARING_BLAZE,
        key: "SEARING_BLAZE",
        name: "Searing Blaze",
    },
    CardCatalogEntry {
        id: ids::SKULLCRACK,
        key: "SKULLCRACK",
        name: "Skullcrack",
    },
    CardCatalogEntry {
        id: ids::FLAMES_OF_THE_BLOOD_HAND,
        key: "FLAMES_OF_THE_BLOOD_HAND",
        name: "Flames of the Blood Hand",
    },
    CardCatalogEntry {
        id: ids::SEARING_BLOOD,
        key: "SEARING_BLOOD",
        name: "Searing Blood",
    },
    CardCatalogEntry {
        id: ids::TARMOGOYF,
        key: "TARMOGOYF",
        name: "Tarmogoyf",
    },
    CardCatalogEntry {
        id: ids::SCAVENGING_OOZE,
        key: "SCAVENGING_OOZE",
        name: "Scavenging Ooze",
    },
    CardCatalogEntry {
        id: ids::STRANGLEROOT_GEIST,
        key: "STRANGLEROOT_GEIST",
        name: "Strangleroot Geist",
    },
    CardCatalogEntry {
        id: ids::WILD_NACATL,
        key: "WILD_NACATL",
        name: "Wild Nacatl",
    },
    CardCatalogEntry {
        id: ids::EXPERIMENT_ONE,
        key: "EXPERIMENT_ONE",
        name: "Experiment One",
    },
    CardCatalogEntry {
        id: ids::DRYAD_MILITANT,
        key: "DRYAD_MILITANT",
        name: "Dryad Militant",
    },
    CardCatalogEntry {
        id: ids::THRUN_LAST_TROLL,
        key: "THRUN_LAST_TROLL",
        name: "Thrun Last Troll",
    },
    CardCatalogEntry {
        id: ids::RANCOR_BEAST,
        key: "RANCOR_BEAST",
        name: "Rancor Beast",
    },
    CardCatalogEntry {
        id: ids::RANCOR,
        key: "RANCOR",
        name: "Rancor",
    },
    CardCatalogEntry {
        id: ids::VINES_OF_VASTWOOD,
        key: "VINES_OF_VASTWOOD",
        name: "Vines of Vastwood",
    },
    CardCatalogEntry {
        id: ids::COLLECTED_COMPANY,
        key: "COLLECTED_COMPANY",
        name: "Collected Company",
    },
    CardCatalogEntry {
        id: ids::GAEA_ANTHEM,
        key: "GAEA_ANTHEM",
        name: "Gaea Anthem",
    },
    CardCatalogEntry {
        id: ids::SOL_RING,
        key: "SOL_RING",
        name: "Sol Ring",
    },
    CardCatalogEntry {
        id: ids::SIGNAL_PEST,
        key: "SIGNAL_PEST",
        name: "Signal Pest",
    },
    CardCatalogEntry {
        id: ids::VAULT_SKIRGE,
        key: "VAULT_SKIRGE",
        name: "Vault Skirge",
    },
    CardCatalogEntry {
        id: ids::CRANIAL_PLATING,
        key: "CRANIAL_PLATING",
        name: "Cranial Plating",
    },
    CardCatalogEntry {
        id: ids::STEEL_OVERSEER,
        key: "STEEL_OVERSEER",
        name: "Steel Overseer",
    },
    CardCatalogEntry {
        id: ids::LIGHTNING_HELIX,
        key: "LIGHTNING_HELIX",
        name: "Lightning Helix",
    },
    CardCatalogEntry {
        id: ids::TERMINATE,
        key: "TERMINATE",
        name: "Terminate",
    },
    CardCatalogEntry {
        id: ids::GEIST_OF_SAINT_TRAFT,
        key: "GEIST_OF_SAINT_TRAFT",
        name: "Geist of Saint Traft",
    },
    CardCatalogEntry {
        id: ids::FLEECEMANE_LION,
        key: "FLEECEMANE_LION",
        name: "Fleecemane Lion",
    },
    CardCatalogEntry {
        id: ids::TIDEHOLLOW_SCULLER,
        key: "TIDEHOLLOW_SCULLER",
        name: "Tidehollow Sculler",
    },
    CardCatalogEntry {
        id: ids::ANCIENT_TOMB,
        key: "ANCIENT_TOMB",
        name: "Ancient Tomb",
    },
    CardCatalogEntry {
        id: ids::BOSEIJU_WHO_ENDURES,
        key: "BOSEIJU_WHO_ENDURES",
        name: "Boseiju Who Endures",
    },
    CardCatalogEntry {
        id: ids::BREEDING_POOL,
        key: "BREEDING_POOL",
        name: "Breeding Pool",
    },
    CardCatalogEntry {
        id: ids::COMMAND_TOWER,
        key: "COMMAND_TOWER",
        name: "Command Tower",
    },
    CardCatalogEntry {
        id: ids::FLOODED_STRAND,
        key: "FLOODED_STRAND",
        name: "Flooded Strand",
    },
    CardCatalogEntry {
        id: ids::GAEAS_CRADLE,
        key: "GAEAS_CRADLE",
        name: "Gaeas Cradle",
    },
    CardCatalogEntry {
        id: ids::GEMSTONE_CAVERNS,
        key: "GEMSTONE_CAVERNS",
        name: "Gemstone Caverns",
    },
    CardCatalogEntry {
        id: ids::INVENTORS_FAIR,
        key: "INVENTORS_FAIR",
        name: "Inventors Fair",
    },
    CardCatalogEntry {
        id: ids::MINAMO_SCHOOL,
        key: "MINAMO_SCHOOL",
        name: "Minamo School",
    },
    CardCatalogEntry {
        id: ids::MISTY_RAINFOREST,
        key: "MISTY_RAINFOREST",
        name: "Misty Rainforest",
    },
    CardCatalogEntry {
        id: ids::MISTRISE_VILLAGE,
        key: "MISTRISE_VILLAGE",
        name: "Mistrise Village",
    },
    CardCatalogEntry {
        id: ids::OTAWARA_SOARING_CITY,
        key: "OTAWARA_SOARING_CITY",
        name: "Otawara Soaring City",
    },
    CardCatalogEntry {
        id: ids::SEAT_OF_THE_SYNOD,
        key: "SEAT_OF_THE_SYNOD",
        name: "Seat of the Synod",
    },
    CardCatalogEntry {
        id: ids::SHIFTING_WOODLAND,
        key: "SHIFTING_WOODLAND",
        name: "Shifting Woodland",
    },
    CardCatalogEntry {
        id: ids::SNOW_COVERED_FOREST,
        key: "SNOW_COVERED_FOREST",
        name: "Snow Covered Forest",
    },
    CardCatalogEntry {
        id: ids::SNOW_COVERED_ISLAND,
        key: "SNOW_COVERED_ISLAND",
        name: "Snow Covered Island",
    },
    CardCatalogEntry {
        id: ids::TREASURE_VAULT,
        key: "TREASURE_VAULT",
        name: "Treasure Vault",
    },
    CardCatalogEntry {
        id: ids::TREE_OF_TALES,
        key: "TREE_OF_TALES",
        name: "Tree of Tales",
    },
    CardCatalogEntry {
        id: ids::TROPICAL_ISLAND,
        key: "TROPICAL_ISLAND",
        name: "Tropical Island",
    },
    CardCatalogEntry {
        id: ids::WATERLOGGED_GROVE,
        key: "WATERLOGGED_GROVE",
        name: "Waterlogged Grove",
    },
    CardCatalogEntry {
        id: ids::WINDSWEPT_HEATH,
        key: "WINDSWEPT_HEATH",
        name: "Windswept Heath",
    },
    CardCatalogEntry {
        id: ids::YAVIMAYA_COAST,
        key: "YAVIMAYA_COAST",
        name: "Yavimaya Coast",
    },
    CardCatalogEntry {
        id: ids::ARCANE_SIGNET,
        key: "ARCANE_SIGNET",
        name: "Arcane Signet",
    },
    CardCatalogEntry {
        id: ids::BASALT_MONOLITH,
        key: "BASALT_MONOLITH",
        name: "Basalt Monolith",
    },
    CardCatalogEntry {
        id: ids::CHROME_MOX,
        key: "CHROME_MOX",
        name: "Chrome Mox",
    },
    CardCatalogEntry {
        id: ids::FELLWAR_STONE,
        key: "FELLWAR_STONE",
        name: "Fellwar Stone",
    },
    CardCatalogEntry {
        id: ids::GRIM_MONOLITH,
        key: "GRIM_MONOLITH",
        name: "Grim Monolith",
    },
    CardCatalogEntry {
        id: ids::LOTUS_PETAL,
        key: "LOTUS_PETAL",
        name: "Lotus Petal",
    },
    CardCatalogEntry {
        id: ids::MANA_VAULT,
        key: "MANA_VAULT",
        name: "Mana Vault",
    },
    CardCatalogEntry {
        id: ids::MOX_AMBER,
        key: "MOX_AMBER",
        name: "Mox Amber",
    },
    CardCatalogEntry {
        id: ids::MOX_DIAMOND,
        key: "MOX_DIAMOND",
        name: "Mox Diamond",
    },
    CardCatalogEntry {
        id: ids::MOX_OPAL,
        key: "MOX_OPAL",
        name: "Mox Opal",
    },
    CardCatalogEntry {
        id: ids::MOONSILVER_KEY,
        key: "MOONSILVER_KEY",
        name: "Moonsilver Key",
    },
    CardCatalogEntry {
        id: ids::SIMIC_SIGNET,
        key: "SIMIC_SIGNET",
        name: "Simic Signet",
    },
    CardCatalogEntry {
        id: ids::SPRINGLEAF_DRUM,
        key: "SPRINGLEAF_DRUM",
        name: "Springleaf Drum",
    },
    CardCatalogEntry {
        id: ids::TALISMAN_OF_CURIOSITY,
        key: "TALISMAN_OF_CURIOSITY",
        name: "Talisman of Curiosity",
    },
    CardCatalogEntry {
        id: ids::AGATHAS_SOUL_CAULDRON,
        key: "AGATHAS_SOUL_CAULDRON",
        name: "Agathas Soul Cauldron",
    },
    CardCatalogEntry {
        id: ids::THE_ONE_RING,
        key: "THE_ONE_RING",
        name: "the One Ring",
    },
    CardCatalogEntry {
        id: ids::MIRAGE_MIRROR,
        key: "MIRAGE_MIRROR",
        name: "Mirage Mirror",
    },
    CardCatalogEntry {
        id: ids::KINNAN_BONDER_PRODIGY,
        key: "KINNAN_BONDER_PRODIGY",
        name: "Kinnan Bonder Prodigy",
    },
    CardCatalogEntry {
        id: ids::BIRDS_OF_PARADISE,
        key: "BIRDS_OF_PARADISE",
        name: "Birds of Paradise",
    },
    CardCatalogEntry {
        id: ids::FYNDHORN_ELVES,
        key: "FYNDHORN_ELVES",
        name: "Fyndhorn Elves",
    },
    CardCatalogEntry {
        id: ids::DELIGHTED_HALFLING,
        key: "DELIGHTED_HALFLING",
        name: "Delighted Halfling",
    },
    CardCatalogEntry {
        id: ids::BADGERMOLE_CUB,
        key: "BADGERMOLE_CUB",
        name: "Badgermole Cub",
    },
    CardCatalogEntry {
        id: ids::CLEVER_IMPERSONATOR,
        key: "CLEVER_IMPERSONATOR",
        name: "Clever Impersonator",
    },
    CardCatalogEntry {
        id: ids::COLOSSAL_SKYTURTLE,
        key: "COLOSSAL_SKYTURTLE",
        name: "Colossal Skyturtle",
    },
    CardCatalogEntry {
        id: ids::CONSECRATED_SPHINX,
        key: "CONSECRATED_SPHINX",
        name: "Consecrated Sphinx",
    },
    CardCatalogEntry {
        id: ids::DRIFT_OF_PHANTASMS,
        key: "DRIFT_OF_PHANTASMS",
        name: "Drift of Phantasms",
    },
    CardCatalogEntry {
        id: ids::ELVISH_SPIRIT_GUIDE,
        key: "ELVISH_SPIRIT_GUIDE",
        name: "Elvish Spirit Guide",
    },
    CardCatalogEntry {
        id: ids::ENDURANCE,
        key: "ENDURANCE",
        name: "Endurance",
    },
    CardCatalogEntry {
        id: ids::ENDURING_VITALITY,
        key: "ENDURING_VITALITY",
        name: "Enduring Vitality",
    },
    CardCatalogEntry {
        id: ids::FAERIE_MASTERMIND,
        key: "FAERIE_MASTERMIND",
        name: "Faerie Mastermind",
    },
    CardCatalogEntry {
        id: ids::FLESH_DUPLICATE,
        key: "FLESH_DUPLICATE",
        name: "Flesh Duplicate",
    },
    CardCatalogEntry {
        id: ids::HIGH_FAE_TRICKSTER,
        key: "HIGH_FAE_TRICKSTER",
        name: "High Fae Trickster",
    },
    CardCatalogEntry {
        id: ids::HULLBREAKER_HORROR,
        key: "HULLBREAKER_HORROR",
        name: "Hullbreaker Horror",
    },
    CardCatalogEntry {
        id: ids::MOCKINGBIRD,
        key: "MOCKINGBIRD",
        name: "Mockingbird",
    },
    CardCatalogEntry {
        id: ids::NEZAHAL_PRIMAL_TIDE,
        key: "NEZAHAL_PRIMAL_TIDE",
        name: "Nezahal Primal Tide",
    },
    CardCatalogEntry {
        id: ids::NYXBLOOM_ANCIENT,
        key: "NYXBLOOM_ANCIENT",
        name: "Nyxbloom Ancient",
    },
    CardCatalogEntry {
        id: ids::PHYREXIAN_METAMORPH,
        key: "PHYREXIAN_METAMORPH",
        name: "Phyrexian Metamorph",
    },
    CardCatalogEntry {
        id: ids::SEEDBORN_MUSE,
        key: "SEEDBORN_MUSE",
        name: "Seedborn Muse",
    },
    CardCatalogEntry {
        id: ids::THRASIOS_TRITON_HERO,
        key: "THRASIOS_TRITON_HERO",
        name: "Thrasios Triton Hero",
    },
    CardCatalogEntry {
        id: ids::TIDESPOUT_TYRANT,
        key: "TIDESPOUT_TYRANT",
        name: "Tidespout Tyrant",
    },
    CardCatalogEntry {
        id: ids::TROPHY_MAGE,
        key: "TROPHY_MAGE",
        name: "Trophy Mage",
    },
    CardCatalogEntry {
        id: ids::WAN_SHI_TONG,
        key: "WAN_SHI_TONG",
        name: "Wan Shi Tong",
    },
    CardCatalogEntry {
        id: ids::WANDERING_ARCHAIC,
        key: "WANDERING_ARCHAIC",
        name: "Wandering Archaic",
    },
    CardCatalogEntry {
        id: ids::AN_OFFER_YOU_CANT_REFUSE,
        key: "AN_OFFER_YOU_CANT_REFUSE",
        name: "an Offer You Cant Refuse",
    },
    CardCatalogEntry {
        id: ids::CHORD_OF_CALLING,
        key: "CHORD_OF_CALLING",
        name: "Chord of Calling",
    },
    CardCatalogEntry {
        id: ids::CROP_ROTATION,
        key: "CROP_ROTATION",
        name: "Crop Rotation",
    },
    CardCatalogEntry {
        id: ids::CYCLONIC_RIFT,
        key: "CYCLONIC_RIFT",
        name: "Cyclonic Rift",
    },
    CardCatalogEntry {
        id: ids::FIERCE_GUARDIANSHIP,
        key: "FIERCE_GUARDIANSHIP",
        name: "Fierce Guardianship",
    },
    CardCatalogEntry {
        id: ids::FLUSTERSTORM,
        key: "FLUSTERSTORM",
        name: "Flusterstorm",
    },
    CardCatalogEntry {
        id: ids::FORCE_OF_NEGATION,
        key: "FORCE_OF_NEGATION",
        name: "Force of Negation",
    },
    CardCatalogEntry {
        id: ids::FORCE_OF_WILL,
        key: "FORCE_OF_WILL",
        name: "Force of Will",
    },
    CardCatalogEntry {
        id: ids::INTO_THE_FLOOD_MAW,
        key: "INTO_THE_FLOOD_MAW",
        name: "Into the Flood Maw",
    },
    CardCatalogEntry {
        id: ids::MENTAL_MISSTEP,
        key: "MENTAL_MISSTEP",
        name: "Mental Misstep",
    },
    CardCatalogEntry {
        id: ids::MINDBREAK_TRAP,
        key: "MINDBREAK_TRAP",
        name: "Mindbreak Trap",
    },
    CardCatalogEntry {
        id: ids::MYSTICAL_TUTOR,
        key: "MYSTICAL_TUTOR",
        name: "Mystical Tutor",
    },
    CardCatalogEntry {
        id: ids::NOXIOUS_REVIVAL,
        key: "NOXIOUS_REVIVAL",
        name: "Noxious Revival",
    },
    CardCatalogEntry {
        id: ids::PACT_OF_NEGATION,
        key: "PACT_OF_NEGATION",
        name: "Pact of Negation",
    },
    CardCatalogEntry {
        id: ids::SWAN_SONG,
        key: "SWAN_SONG",
        name: "Swan Song",
    },
    CardCatalogEntry {
        id: ids::VEIL_OF_SUMMER,
        key: "VEIL_OF_SUMMER",
        name: "Veil of Summer",
    },
    CardCatalogEntry {
        id: ids::WHIR_OF_INVENTION,
        key: "WHIR_OF_INVENTION",
        name: "Whir of Invention",
    },
    CardCatalogEntry {
        id: ids::WORLDLY_TUTOR,
        key: "WORLDLY_TUTOR",
        name: "Worldly Tutor",
    },
    CardCatalogEntry {
        id: ids::MUDDLE_THE_MIXTURE,
        key: "MUDDLE_THE_MIXTURE",
        name: "Muddle the Mixture",
    },
    CardCatalogEntry {
        id: ids::FINALE_OF_DEVASTATION,
        key: "FINALE_OF_DEVASTATION",
        name: "Finale of Devastation",
    },
    CardCatalogEntry {
        id: ids::GREEN_SUNS_ZENITH,
        key: "GREEN_SUNS_ZENITH",
        name: "Green Suns Zenith",
    },
    CardCatalogEntry {
        id: ids::NATURES_RHYTHM,
        key: "NATURES_RHYTHM",
        name: "Natures Rhythm",
    },
    CardCatalogEntry {
        id: ids::MYSTIC_REMORA,
        key: "MYSTIC_REMORA",
        name: "Mystic Remora",
    },
    CardCatalogEntry {
        id: ids::RHYSTIC_STUDY,
        key: "RHYSTIC_STUDY",
        name: "Rhystic Study",
    },
    CardCatalogEntry {
        id: ids::TEZZERET_THE_SEEKER,
        key: "TEZZERET_THE_SEEKER",
        name: "Tezzeret the Seeker",
    },
    CardCatalogEntry {
        id: ids::BRIDGEWORKS_BATTLE,
        key: "BRIDGEWORKS_BATTLE",
        name: "Bridgeworks Battle",
    },
    CardCatalogEntry {
        id: ids::DISCIPLE_OF_FREYALISE,
        key: "DISCIPLE_OF_FREYALISE",
        name: "Disciple of Freyalise",
    },
    CardCatalogEntry {
        id: ids::HYDROELECTRIC_SPECIMEN,
        key: "HYDROELECTRIC_SPECIMEN",
        name: "Hydroelectric Specimen",
    },
    CardCatalogEntry {
        id: ids::INVASION_OF_IKORIA,
        key: "INVASION_OF_IKORIA",
        name: "Invasion of Ikoria",
    },
    CardCatalogEntry {
        id: ids::SINK_INTO_STUPOR,
        key: "SINK_INTO_STUPOR",
        name: "Sink Into Stupor",
    },
    // --- Ashcoat Commander Deck ---
    CardCatalogEntry { id: ids::ASHCOAT_OF_THE_SHADOW_SWARM, key: "ASHCOAT_OF_THE_SHADOW_SWARM", name: "Ashcoat of the Shadow Swarm" },
    CardCatalogEntry { id: ids::ASSASSIN_INITIATE, key: "ASSASSIN_INITIATE", name: "Assassin Initiate" },
    CardCatalogEntry { id: ids::AYARA_FIRST_OF_LOCTHWAIN, key: "AYARA_FIRST_OF_LOCTHWAIN", name: "Ayara, First of Locthwain" },
    CardCatalogEntry { id: ids::BLOOD_ARTIST, key: "BLOOD_ARTIST", name: "Blood Artist" },
    CardCatalogEntry { id: ids::BLOODLINE_PRETENDER, key: "BLOODLINE_PRETENDER", name: "Bloodline Pretender" },
    CardCatalogEntry { id: ids::BURGLAR_RAT, key: "BURGLAR_RAT", name: "Burglar Rat" },
    CardCatalogEntry { id: ids::CHANGELING_OUTCAST, key: "CHANGELING_OUTCAST", name: "Changeling Outcast" },
    CardCatalogEntry { id: ids::CHITTERING_RATS, key: "CHITTERING_RATS", name: "Chittering Rats" },
    CardCatalogEntry { id: ids::CHITTERING_WITCH, key: "CHITTERING_WITCH", name: "Chittering Witch" },
    CardCatalogEntry { id: ids::CRYPT_GHAST, key: "CRYPT_GHAST", name: "Crypt Ghast" },
    CardCatalogEntry { id: ids::FALKENRATH_NOBLE, key: "FALKENRATH_NOBLE", name: "Falkenrath Noble" },
    CardCatalogEntry { id: ids::GNAT_MISER, key: "GNAT_MISER", name: "Gnat Miser" },
    CardCatalogEntry { id: ids::INK_EYES_SERVANT_OF_ONI, key: "INK_EYES_SERVANT_OF_ONI", name: "Ink-Eyes, Servant of Oni" },
    CardCatalogEntry { id: ids::KARUMONIX_THE_RAT_KING, key: "KARUMONIX_THE_RAT_KING", name: "Karumonix, the Rat King" },
    CardCatalogEntry { id: ids::LORD_SKITTER_SEWER_KING, key: "LORD_SKITTER_SEWER_KING", name: "Lord Skitter, Sewer King" },
    CardCatalogEntry { id: ids::MARROW_GNAWER, key: "MARROW_GNAWER", name: "Marrow-Gnawer" },
    CardCatalogEntry { id: ids::MIKAEUS_THE_UNHALLOWED, key: "MIKAEUS_THE_UNHALLOWED", name: "Mikaeus, the Unhallowed" },
    CardCatalogEntry { id: ids::NASHI_MOON_SAGES_SCION, key: "NASHI_MOON_SAGES_SCION", name: "Nashi, Moon Sage's Scion" },
    CardCatalogEntry { id: ids::NEZUMI_BONE_READER, key: "NEZUMI_BONE_READER", name: "Nezumi Bone-Reader" },
    CardCatalogEntry { id: ids::NEZUMI_CUTTHROAT, key: "NEZUMI_CUTTHROAT", name: "Nezumi Cutthroat" },
    CardCatalogEntry { id: ids::NEZUMI_GRAVEROBBER, key: "NEZUMI_GRAVEROBBER", name: "Nezumi Graverobber" },
    CardCatalogEntry { id: ids::NEZUMI_SHORTFANG, key: "NEZUMI_SHORTFANG", name: "Nezumi Shortfang" },
    CardCatalogEntry { id: ids::NIRKANA_REVENANT, key: "NIRKANA_REVENANT", name: "Nirkana Revenant" },
    CardCatalogEntry { id: ids::OGRE_SLUMLORD, key: "OGRE_SLUMLORD", name: "Ogre Slumlord" },
    CardCatalogEntry { id: ids::PACK_RAT, key: "PACK_RAT", name: "Pack Rat" },
    CardCatalogEntry { id: ids::RATCATCHER, key: "RATCATCHER", name: "Ratcatcher" },
    CardCatalogEntry { id: ids::RAVENOUS_RATS, key: "RAVENOUS_RATS", name: "Ravenous Rats" },
    CardCatalogEntry { id: ids::REFURBISHED_FAMILIAR, key: "REFURBISHED_FAMILIAR", name: "Refurbished Familiar" },
    CardCatalogEntry { id: ids::ROAMING_THRONE, key: "ROAMING_THRONE", name: "Roaming Throne" },
    CardCatalogEntry { id: ids::SKULLSNATCHER, key: "SKULLSNATCHER", name: "Skullsnatcher" },
    CardCatalogEntry { id: ids::SPECIES_SPECIALIST, key: "SPECIES_SPECIALIST", name: "Species Specialist" },
    CardCatalogEntry { id: ids::TYPHOID_RATS, key: "TYPHOID_RATS", name: "Typhoid Rats" },
    CardCatalogEntry { id: ids::VALLEY_ROTCALLER, key: "VALLEY_ROTCALLER", name: "Valley Rotcaller" },
    CardCatalogEntry { id: ids::ZULAPORT_CUTTHROAT, key: "ZULAPORT_CUTTHROAT", name: "Zulaport Cutthroat" },
    CardCatalogEntry { id: ids::BONTUS_MONUMENT, key: "BONTUS_MONUMENT", name: "Bontu's Monument" },
    CardCatalogEntry { id: ids::CAGED_SUN, key: "CAGED_SUN", name: "Caged Sun" },
    CardCatalogEntry { id: ids::COAT_OF_ARMS, key: "COAT_OF_ARMS", name: "Coat of Arms" },
    CardCatalogEntry { id: ids::CRYPTOLITH_FRAGMENT, key: "CRYPTOLITH_FRAGMENT", name: "Cryptolith Fragment" },
    CardCatalogEntry { id: ids::DARKSTEEL_INGOT, key: "DARKSTEEL_INGOT", name: "Darksteel Ingot" },
    CardCatalogEntry { id: ids::DOOR_OF_DESTINIES, key: "DOOR_OF_DESTINIES", name: "Door of Destinies" },
    CardCatalogEntry { id: ids::HERALDS_HORN, key: "HERALDS_HORN", name: "Herald's Horn" },
    CardCatalogEntry { id: ids::JET_MEDALLION, key: "JET_MEDALLION", name: "Jet Medallion" },
    CardCatalogEntry { id: ids::NIM_DEATHMANTLE, key: "NIM_DEATHMANTLE", name: "Nim Deathmantle" },
    CardCatalogEntry { id: ids::SEMBLANCE_ANVIL, key: "SEMBLANCE_ANVIL", name: "Semblance Anvil" },
    CardCatalogEntry { id: ids::SKULLCLAMP, key: "SKULLCLAMP", name: "Skullclamp" },
    CardCatalogEntry { id: ids::STRIONIC_RESONATOR, key: "STRIONIC_RESONATOR", name: "Strionic Resonator" },
    CardCatalogEntry { id: ids::THE_IMMORTAL_SUN, key: "THE_IMMORTAL_SUN", name: "The Immortal Sun" },
    CardCatalogEntry { id: ids::THORNBITE_STAFF, key: "THORNBITE_STAFF", name: "Thornbite Staff" },
    CardCatalogEntry { id: ids::THRAN_DYNAMO, key: "THRAN_DYNAMO", name: "Thran Dynamo" },
    CardCatalogEntry { id: ids::THRONE_OF_THE_GOD_PHARAOH, key: "THRONE_OF_THE_GOD_PHARAOH", name: "Throne of the God-Pharaoh" },
    CardCatalogEntry { id: ids::URZAS_INCUBATOR, key: "URZAS_INCUBATOR", name: "Urza's Incubator" },
    CardCatalogEntry { id: ids::VANQUISHERS_BANNER, key: "VANQUISHERS_BANNER", name: "Vanquisher's Banner" },
    CardCatalogEntry { id: ids::BLACK_MARKET, key: "BLACK_MARKET", name: "Black Market" },
    CardCatalogEntry { id: ids::BLACK_MARKET_CONNECTIONS, key: "BLACK_MARKET_CONNECTIONS", name: "Black Market Connections" },
    CardCatalogEntry { id: ids::DICTATE_OF_EREBOS, key: "DICTATE_OF_EREBOS", name: "Dictate of Erebos" },
    CardCatalogEntry { id: ids::GRAVE_PACT, key: "GRAVE_PACT", name: "Grave Pact" },
    CardCatalogEntry { id: ids::PHYREXIAN_ARENA, key: "PHYREXIAN_ARENA", name: "Phyrexian Arena" },
    CardCatalogEntry { id: ids::PHYREXIAN_RECLAMATION, key: "PHYREXIAN_RECLAMATION", name: "Phyrexian Reclamation" },
    CardCatalogEntry { id: ids::BOLASS_CITADEL, key: "BOLASS_CITADEL", name: "Bolas's Citadel" },
    CardCatalogEntry { id: ids::CABAL_COFFERS, key: "CABAL_COFFERS", name: "Cabal Coffers" },
    CardCatalogEntry { id: ids::CASTLE_LOCTHWAIN, key: "CASTLE_LOCTHWAIN", name: "Castle Locthwain" },
    CardCatalogEntry { id: ids::CRYPT_OF_AGADEEM, key: "CRYPT_OF_AGADEEM", name: "Crypt of Agadeem" },
    CardCatalogEntry { id: ids::MIKOKORO_CENTER_OF_THE_SEA, key: "MIKOKORO_CENTER_OF_THE_SEA", name: "Mikokoro, Center of the Sea" },
    CardCatalogEntry { id: ids::NYKTHOS_SHRINE_TO_NYX, key: "NYKTHOS_SHRINE_TO_NYX", name: "Nykthos, Shrine to Nyx" },
    CardCatalogEntry { id: ids::PATH_OF_ANCESTRY, key: "PATH_OF_ANCESTRY", name: "Path of Ancestry" },
    CardCatalogEntry { id: ids::SWARMYARD, key: "SWARMYARD", name: "Swarmyard" },
    CardCatalogEntry { id: ids::CHAIN_ASSASSINATION, key: "CHAIN_ASSASSINATION", name: "Chain Assassination" },
    CardCatalogEntry { id: ids::LIVING_DEATH, key: "LIVING_DEATH", name: "Living Death" },
    // --- Flubs deck ---
    CardCatalogEntry { id: ids::FLUBS_THE_FOOL, key: "FLUBS_THE_FOOL", name: "Flubs, the Fool" },
    CardCatalogEntry { id: ids::ABUNDANCE, key: "ABUNDANCE", name: "Abundance" },
    CardCatalogEntry { id: ids::AMPHIBIAN_DOWNPOUR, key: "AMPHIBIAN_DOWNPOUR", name: "Amphibian Downpour" },
    CardCatalogEntry { id: ids::BIRGI_GOD_OF_STORYTELLING, key: "BIRGI_GOD_OF_STORYTELLING", name: "Birgi, God of Storytelling" },
    CardCatalogEntry { id: ids::BLACKBLADE_REFORGED, key: "BLACKBLADE_REFORGED", name: "Blackblade Reforged" },
    CardCatalogEntry { id: ids::BRIDGE_OF_KHAZAD_DUM, key: "BRIDGE_OF_KHAZAD_DUM", name: "Bridge of Khazad-dum" },
    CardCatalogEntry { id: ids::BUCKLEBURY_FERRY, key: "BUCKLEBURY_FERRY", name: "Bucklebury Ferry" },
    CardCatalogEntry { id: ids::CASE_OF_THE_LOCKED_HOTHOUSE, key: "CASE_OF_THE_LOCKED_HOTHOUSE", name: "Case of the Locked Hothouse" },
    CardCatalogEntry { id: ids::CHOCOBO_RACETRACK, key: "CHOCOBO_RACETRACK", name: "Chocobo Racetrack" },
    CardCatalogEntry { id: ids::CODEX_SHREDDER, key: "CODEX_SHREDDER", name: "Codex Shredder" },
    CardCatalogEntry { id: ids::CONDUIT_OF_WORLDS, key: "CONDUIT_OF_WORLDS", name: "Conduit of Worlds" },
    CardCatalogEntry { id: ids::CRUCIBLE_OF_WORLDS, key: "CRUCIBLE_OF_WORLDS", name: "Crucible of Worlds" },
    CardCatalogEntry { id: ids::DRAGONBACK_ASSAULT, key: "DRAGONBACK_ASSAULT", name: "Dragonback Assault" },
    CardCatalogEntry { id: ids::DRUID_CLASS, key: "DRUID_CLASS", name: "Druid Class" },
    CardCatalogEntry { id: ids::DRYAD_OF_THE_ILYSIAN_GROVE, key: "DRYAD_OF_THE_ILYSIAN_GROVE", name: "Dryad of the Ilysian Grove" },
    CardCatalogEntry { id: ids::EVERFLOWING_CHALICE, key: "EVERFLOWING_CHALICE", name: "Everflowing Chalice" },
    CardCatalogEntry { id: ids::EXPLORATION, key: "EXPLORATION", name: "Exploration" },
    CardCatalogEntry { id: ids::EXPLORE_CARD, key: "EXPLORE_CARD", name: "Explore" },
    CardCatalogEntry { id: ids::FORTUNE_TELLERS_TALENT, key: "FORTUNE_TELLERS_TALENT", name: "Fortune Teller's Talent" },
    CardCatalogEntry { id: ids::GLACIERWOOD_SIEGE, key: "GLACIERWOOD_SIEGE", name: "Glacierwood Siege" },
    CardCatalogEntry { id: ids::GUSTHAS_SCEPTER, key: "GUSTHAS_SCEPTER", name: "Gustha's Scepter" },
    CardCatalogEntry { id: ids::LANTERN_OF_INSIGHT, key: "LANTERN_OF_INSIGHT", name: "Lantern of Insight" },
    CardCatalogEntry { id: ids::LIGHTNING_GREAVES, key: "LIGHTNING_GREAVES", name: "Lightning Greaves" },
    CardCatalogEntry { id: ids::LOTUS_COBRA, key: "LOTUS_COBRA", name: "Lotus Cobra" },
    CardCatalogEntry { id: ids::MIKU_LOST_BUT_SINGING, key: "MIKU_LOST_BUT_SINGING", name: "Miku, Lost but Singing" },
    CardCatalogEntry { id: ids::MONUMENT_TO_ENDURANCE, key: "MONUMENT_TO_ENDURANCE", name: "Monument to Endurance" },
    CardCatalogEntry { id: ids::MYSTIC_SANCTUARY, key: "MYSTIC_SANCTUARY", name: "Mystic Sanctuary" },
    CardCatalogEntry { id: ids::NULL_BROOCH, key: "NULL_BROOCH", name: "Null Brooch" },
    CardCatalogEntry { id: ids::OTHERWORLDLY_GAZE, key: "OTHERWORLDLY_GAZE", name: "Otherworldly Gaze" },
    CardCatalogEntry { id: ids::PHIAL_OF_GALADRIEL, key: "PHIAL_OF_GALADRIEL", name: "Phial of Galadriel" },
    CardCatalogEntry { id: ids::PRISMATIC_OMEN, key: "PRISMATIC_OMEN", name: "Prismatic Omen" },
    CardCatalogEntry { id: ids::RENFIELD_DELUSIONAL_MINION, key: "RENFIELD_DELUSIONAL_MINION", name: "Renfield, Delusional Minion" },
    CardCatalogEntry { id: ids::SABOTENDER, key: "SABOTENDER", name: "Sabotender" },
    CardCatalogEntry { id: ids::SAW_IT_COMING, key: "SAW_IT_COMING", name: "Saw It Coming" },
    CardCatalogEntry { id: ids::SCUTE_SWARM, key: "SCUTE_SWARM", name: "Scute Swarm" },
    CardCatalogEntry { id: ids::SIX, key: "SIX", name: "Six" },
    CardCatalogEntry { id: ids::SWIFTFOOT_BOOTS, key: "SWIFTFOOT_BOOTS", name: "Swiftfoot Boots" },
    CardCatalogEntry { id: ids::THESPIANS_STAGE, key: "THESPIANS_STAGE", name: "Thespian's Stage" },
    CardCatalogEntry { id: ids::TIFA_LOCKHART, key: "TIFA_LOCKHART", name: "Tifa Lockhart" },
    CardCatalogEntry { id: ids::TIRELESS_PROVISIONER, key: "TIRELESS_PROVISIONER", name: "Tireless Provisioner" },
    CardCatalogEntry { id: ids::VALAKUT_THE_MOLTEN_PINNACLE, key: "VALAKUT_THE_MOLTEN_PINNACLE", name: "Valakut, the Molten Pinnacle" },
    CardCatalogEntry { id: ids::VESUVA, key: "VESUVA", name: "Vesuva" },
    CardCatalogEntry { id: ids::WALK_IN_CLOSET, key: "WALK_IN_CLOSET", name: "Walk-In Closet" },
    CardCatalogEntry { id: ids::WAYWARD_SWORDTOOTH, key: "WAYWARD_SWORDTOOTH", name: "Wayward Swordtooth" },
    CardCatalogEntry { id: ids::WONDER, key: "WONDER", name: "Wonder" },
    CardCatalogEntry { id: ids::YAVIMAYA_CRADLE_OF_GROWTH, key: "YAVIMAYA_CRADLE_OF_GROWTH", name: "Yavimaya, Cradle of Growth" },
];

/// Returns catalog entries annotated with whether the card currently has
/// any modeled spell/ability effect in the rules engine.
pub fn card_implementation_status(db: &CardDatabase) -> Vec<CardImplementationStatus> {
    ALL_CARDS
        .iter()
        .map(|entry| {
            let effects_implemented = db
                .get(entry.id)
                .map(|def| {
                    def.spell_effect.is_some()
                        || !def.activated_abilities.is_empty()
                        || !def.triggered_abilities.is_empty()
                        || !def.static_abilities.is_empty()
                })
                .unwrap_or(false);

            CardImplementationStatus {
                id: entry.id,
                key: entry.key,
                name: entry.name,
                effects_implemented,
            }
        })
        .collect()
}

/// Card coverage level for a single card in a deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageLevel {
    /// Card is hand-authored in sample database with full effects.
    FullyImplemented,
    /// Card is in sample database but effects are stub/unimplemented.
    Stubbed,
    /// Card would be auto-parsed from Scryfall oracle text (not hand-authored).
    AutoParsed,
    /// Card is unknown (not in sample DB or Scryfall).
    Unknown,
}

/// Coverage analysis for a single card.
#[derive(Debug, Clone)]
pub struct CardCoverageEntry {
    pub name: String,
    pub level: CoverageLevel,
    pub has_unimplemented: bool,
}

/// Deck coverage summary.
#[derive(Debug, Clone)]
pub struct DeckCoverage {
    pub cards: Vec<CardCoverageEntry>,
    pub fully_implemented: usize,
    pub stubbed: usize,
    pub auto_parsed: usize,
    pub unknown: usize,
}

impl DeckCoverage {
    /// Coverage percentage (fully implemented + auto-parsed / total).
    pub fn coverage_pct(&self) -> f64 {
        let total = self.cards.len();
        if total == 0 {
            return 100.0;
        }
        let covered = self.fully_implemented + self.auto_parsed;
        (covered as f64 / total as f64) * 100.0
    }
}

/// Analyze card coverage for a list of card names.
/// Checks each card against the sample database to determine implementation level.
pub fn analyze_deck_coverage(card_names: &[&str], db: &CardDatabase) -> DeckCoverage {
    let mut entries = Vec::new();
    let mut fully = 0;
    let mut stubbed = 0;
    let mut auto_parsed = 0;
    let mut unknown = 0;

    for &name in card_names {
        let card_id = db.find_by_name(name);
        let (level, has_unimplemented) = if let Some(id) = card_id {
            if let Some(def) = db.get(id) {
                let has_effects = def.spell_effect.is_some()
                    || !def.activated_abilities.is_empty()
                    || !def.triggered_abilities.is_empty()
                    || !def.static_abilities.is_empty()
                    || !def.mana_abilities.is_empty()
                    || def.is_land()
                    || def.is_creature();

                let has_unimpl = has_unimplemented_effects(def);

                if has_effects && !has_unimpl {
                    (CoverageLevel::FullyImplemented, false)
                } else {
                    (CoverageLevel::Stubbed, has_unimpl)
                }
            } else {
                (CoverageLevel::Unknown, false)
            }
        } else {
            (CoverageLevel::AutoParsed, false) // Would need Scryfall
        };

        match level {
            CoverageLevel::FullyImplemented => fully += 1,
            CoverageLevel::Stubbed => stubbed += 1,
            CoverageLevel::AutoParsed => auto_parsed += 1,
            CoverageLevel::Unknown => unknown += 1,
        }

        entries.push(CardCoverageEntry {
            name: name.to_string(),
            level,
            has_unimplemented,
        });
    }

    DeckCoverage {
        cards: entries,
        fully_implemented: fully,
        stubbed: stubbed,
        auto_parsed: auto_parsed,
        unknown: unknown,
    }
}

/// Check if a CardDef has any Unimplemented effects.
fn has_unimplemented_effects(def: &super::CardDef) -> bool {
    use super::Effect;

    if let Some(ref eff) = def.spell_effect {
        if matches!(eff, Effect::Unimplemented(_)) {
            return true;
        }
    }
    for ability in &def.activated_abilities {
        if matches!(&ability.effect, Effect::Unimplemented(_)) {
            return true;
        }
    }
    for ability in &def.triggered_abilities {
        if matches!(&ability.effect, Effect::Unimplemented(_)) {
            return true;
        }
    }
    false
}
