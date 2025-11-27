// crates/geodb-core/src/common/raw_normalize.rs
use crate::alias::CityMetaIndex;
use crate::common::raw::CountryRaw;

pub fn apply_all_metadata(raw_countries: &mut [CountryRaw], meta_index: Option<&CityMetaIndex>) {
    let Some(meta_index) = meta_index else {
        // No metadata -> no-op
        return;
    };

    for country in raw_countries.iter_mut() {
        for state in &mut country.states {
            let corrected = meta_index.corrected_state_name(&country.iso2, &state.name);

            // Only override if something actually changed
            if corrected != state.name {
                state.native = Some(corrected);
            }
        }
    }
}

// optional, for future alias/region enrichment:
#[allow(dead_code)]
fn apply_city_meta(_raw_countries: &mut [CountryRaw], _meta_index: &CityMetaIndex) {
    // …
}
