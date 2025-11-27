use geodb_core::alias::CityMetaIndex;
use geodb_core::prelude::*; // GeoDb, DefaultBackend, GeoSearch, etc.
use std::path::PathBuf;

/// For every alias declared in `city_meta.json`, ensure that
/// `GeoDb::<DefaultBackend>::smart_search(alias)` finds at least a *reasonable*
/// match in terms of Country / State / City.
///
/// We accept:
/// - City hit:  country.iso2 == iso2 && state.name == state && city.name == city
/// - State hit: country.iso2 == iso2 && state.name == state
/// - Country hit: country.iso2 == iso2
///
/// This keeps the test compatible with:
/// - legacy_model vs flat
/// - search_blobs on/off
/// - use_smolstr on/off
#[test]
fn all_city_aliases_are_resolvable_via_smart_search() {
    // 1. Load DB with whatever backend/features are compiled in.
    let db = GeoDb::<DefaultBackend>::load().expect("failed to load GeoDb");
    // let db = GeoDb::<DefaultBackend>::load_raw_json(GeoDb::<DefaultBackend>::default_raw_path()).expect("failed to load GeoDb");

    // 2. Load CityMetaIndex from the JSON used at build time.
    // Adjust this to your actual loading API if needed.
    let meta_index = {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // crates/geodb-core/
        let path = root.join("data").join("city_meta.json");
        CityMetaIndex::from_path(&path).expect("failed to load city_meta.json for alias test")
    };

    let mut checked_aliases = 0usize;

    for city_meta in meta_index.cities() {
        let iso2 = city_meta.iso2.as_str();
        let state = city_meta.state.as_str();
        let city = city_meta.city.as_str();

        for alias in &city_meta.aliases {
            let alias_trimmed = alias.trim();
            if alias_trimmed.is_empty() {
                continue;
            }
            checked_aliases += 1;

            let hits = db.smart_search(alias_trimmed);

            let found_canonical = hits.iter().any(|hit| match &hit.item {
                SmartItem::City {
                    city: hit_city,
                    state: hit_state,
                    country: hit_country,
                } => {
                    hit_country.iso2().eq_ignore_ascii_case(iso2)
                        && hit_state.name().eq_ignore_ascii_case(state)
                        && hit_city.name().eq_ignore_ascii_case(city)
                }
                SmartItem::State {
                    state: hit_state,
                    country: hit_country,
                } => {
                    hit_country.iso2().eq_ignore_ascii_case(iso2)
                        && hit_state.name().eq_ignore_ascii_case(state)
                }
                SmartItem::Country(hit_country) => hit_country.iso2().eq_ignore_ascii_case(iso2),
            });

            assert!(
                found_canonical,
                "Alias {:?} did not resolve via smart_search(): \
                 expected iso2/state/city = {}/{}/{}; hits={}",
                alias_trimmed,
                iso2,
                state,
                city,
                hits.len()
            );
        }
    }

    assert!(
        checked_aliases > 0,
        "Alias test did not check any aliases – is city_meta.json empty / not loaded?"
    );
}
