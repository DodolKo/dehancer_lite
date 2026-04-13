use dehancer_lite::pipeline::{preset_library, FilmPresetV1, PRESET_SCHEMA, PRESET_VERSION};

#[test]
fn preset_json_roundtrip_stays_stable() {
    let preset = preset_library().remove(0);
    let json = preset.to_json_pretty().expect("must serialize");
    let restored = FilmPresetV1::from_json_str(&json).expect("must deserialize");

    assert_eq!(preset, restored);
    assert_eq!(restored.schema, PRESET_SCHEMA);
    assert_eq!(restored.version, PRESET_VERSION);
}

#[test]
fn preset_library_contains_expected_profiles() {
    let presets = preset_library();
    let names: Vec<_> = presets.iter().map(|p| p.name.as_str()).collect();

    assert!(names.contains(&"Vision3 50D Clean"));
    assert!(names.contains(&"Vision3 250D Balanced"));
    assert!(names.contains(&"Vision3 500T Night"));
    assert!(names.contains(&"No-Remjet Strong Halo"));
    assert!(names.contains(&"Kodak 2383 Print-ish"));
    assert!(names.contains(&"Portra 400 Photo"));
}
