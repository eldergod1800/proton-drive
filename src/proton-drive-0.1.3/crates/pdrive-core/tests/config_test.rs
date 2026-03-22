use pdrive_core::config::{Config, SyncPair, SyncDirection};

#[test]
fn test_config_round_trip() {
    let config = Config {
        sync_pairs: vec![
            SyncPair {
                local: "/home/user/Documents".into(),
                remote: "/My Files/Documents".into(),
                direction: SyncDirection::Bidirectional,
            }
        ],
    };
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.sync_pairs[0].local, "/home/user/Documents");
    assert_eq!(parsed.sync_pairs[0].remote, "/My Files/Documents");
    assert_eq!(parsed.sync_pairs[0].direction, SyncDirection::Bidirectional);
}

#[test]
fn test_config_path_contains_pdrive() {
    let path = pdrive_core::config::config_path();
    assert!(path.to_str().unwrap().contains("pdrive"));
}

#[test]
fn test_empty_config_has_no_sync_pairs() {
    let config = Config::default();
    assert!(config.sync_pairs.is_empty());
}
