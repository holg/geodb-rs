// crates/geodb-core/src/model/search.rs
use super::model::flat::{City, Country, GeoDb, State};
use crate::common::{DbStats, SmartHitGeneric};
use crate::text::fold_key;
use crate::traits::{GeoBackend, GeoSearch};

type MySmartHit<'a, B> = SmartHitGeneric<'a, Country<B>, State<B>, City<B>>;

impl<B: GeoBackend> GeoSearch<B> for GeoDb<B> {
    fn stats(&self) -> DbStats {
        DbStats {
            countries: self.countries.len(),
            states: self.states.len(),
            cities: self.cities.len(),
        }
    }
    fn cities() -> Vec<City<B>> {
        self.cities.iter().cloned().collect()
    }

    fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>> {
        self.countries
            .iter()
            .find(|c| c.iso2.as_ref().eq_ignore_ascii_case(iso2))
    }

    fn find_country_by_code(&self, code: &str) -> Option<&Country<B>> {
        let code = code.trim();
        self.find_country_by_iso2(code).or_else(|| {
            self.countries.iter().find(|c| {
                c.iso3
                    .as_ref()
                    .is_some_and(|s| s.as_ref().eq_ignore_ascii_case(code))
            })
        })
    }

    fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)> {
        let q = fold_key(substr);
        let mut out = Vec::new();
        if q.is_empty() {
            return out;
        }

        // FLAT LOOP!
        for s in &self.states {
            if fold_key(s.name.as_ref()).contains(&q) {
                // ID Lookup!
                let c = &self.countries[s.country_id as usize];
                out.push((s, c));
            }
        }
        out
    }

    fn find_cities_by_substring(&self, substr: &str) -> Vec<(&City<B>, &State<B>, &Country<B>)> {
        let q = fold_key(substr);
        let mut out = Vec::new();
        if q.is_empty() {
            return out;
        }

        for c in &self.countries {
            for s in &c.states {
                for city in &s.cities {
                    // Check Name
                    let mut matched = fold_key(city.name.as_ref()).contains(&q);
                    // Check Aliases
                    if !matched {
                        for alias in &city.aliases {
                            if fold_key(alias).contains(&q) {
                                matched = true;
                                break;
                            }
                        }
                    }
                    if matched {
                        out.push((city, s, c));
                    }
                }
            }
        }
        out
    }

    fn smart_search(&self, query: &str) -> Vec<MySmartHit<'_, B>> {
        // ... smart search logic using flat loops ...
        vec![]
    }
}
