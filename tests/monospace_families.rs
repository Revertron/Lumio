//! The list an app offers when it lets the user choose a terminal font.

use lumio::assets::monospace_families;

#[test]
fn the_list_holds_the_fixed_pitch_families_and_nothing_else() {
    let families = monospace_families();
    assert!(
        !families.is_empty(),
        "no monospaced family found at all — has this machine any fonts?"
    );

    let has = |name: &str| families.iter().any(|family| family == name);
    #[cfg(target_os = "windows")]
    {
        // Both ship with every Windows since Vista, so they say something about
        // the filter rather than about this machine.
        assert!(has("Courier New"), "a fixed-pitch family was left out: {families:?}");
        assert!(!has("Arial"), "Arial is proportional");
        assert!(!has("Segoe UI"), "Segoe UI is proportional");
    }
    #[cfg(not(target_os = "windows"))]
    let _ = has;

    // Sorted and deduplicated: this goes straight into a list to pick from.
    let mut expected = families.clone();
    expected.sort_by_key(|family| family.to_lowercase());
    expected.dedup();
    assert_eq!(families, expected, "the list is not in order, or repeats itself");
}
