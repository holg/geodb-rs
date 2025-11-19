// // crates/geodb-core/src/search.rs
//
// use crate::model::{City, Country, DbStats, GeoDb, SmartHit, SmartItem, State};
// use crate::text::fold_key;
// use crate::traits::GeoBackend;
// use crate::phone::PhoneCodeSearch;
//
// /// The Logic Trait.
// /// Defines the search operations available on the Database.
// pub trait GeoSearch<B: GeoBackend> {
//     fn stats(&self) -> DbStats;
//     fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>>;
//     fn find_country_by_code(&self, code: &str) -> Option<&Country<B>>;
//     fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)>;
//     fn find_cities_by_substring(&self, substr: &str) -> Vec<(&City<B>, &State<B>, &Country<B>)>;
//     fn smart_search(&self, query: &str) -> Vec<SmartHit<'_, B>>;
// }
//
// impl<B: GeoBackend> GeoSearch<B> for GeoDb<B> {
//
//     fn stats(&self) -> DbStats {
//         DbStats {
//             countries: self.countries.len(),
//             states: self.states.len(),
//             cities: self.cities.len(),
//         }
//     }
//
//     fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>> {
//         // Linear scan of countries is fast (N < 300)
//         self.countries
//             .iter()
//             .find(|c| c.iso2.as_ref().eq_ignore_ascii_case(iso2))
//     }
//
//     fn find_country_by_code(&self, code: &str) -> Option<&Country<B>> {
//         let code = code.trim();
//         // Try ISO2 first
//         self.find_country_by_iso2(code).or_else(|| {
//             // Try ISO3
//             self.countries.iter().find(|c| {
//                 c.iso3
//                     .as_ref()
//                     .is_some_and(|s| s.as_ref().eq_ignore_ascii_case(code))
//             })
//         })
//     }
//
//     fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)> {
//         let q = fold_key(substr);
//         if q.is_empty() {
//             return Vec::new();
//         }
//
//         let mut out = Vec::new();
//
//         // 🚀 Optimization: Linear scan over flat states vector.
//         // CPU pre-fetcher loves this contiguous memory.
//         for state in &self.states {
//             if fold_key(state.name.as_ref()).contains(&q) {
//                 // O(1) Lookup: Jump directly to parent Country using ID
//                 // Note: We cast u16 -> usize for indexing
//                 let country = &self.countries[state.country_id as usize];
//                 out.push((state, country));
//             }
//         }
//         out
//     }
//
//     fn find_cities_by_substring(&self, substr: &str) -> Vec<(&City<B>, &State<B>, &Country<B>)> {
//         let q = fold_key(substr);
//         if q.is_empty() {
//             return Vec::new();
//         }
//
//         let mut out = Vec::new();
//
//         // 🚀 Optimization: Linear scan over flat cities vector.
//         for city in &self.cities {
//             // Check canonical name
//             let mut matched = fold_key(city.name.as_ref()).contains(&q);
//
//             // Check aliases (Baked in at build time!)
//             if !matched {
//                 if let Some(aliases) = &city.aliases {
//                     for alias in aliases {
//                         if fold_key(alias).contains(&q) {
//                             matched = true;
//                             break;
//                         }
//                     }
//                 }
//             }
//
//             if matched {
//                 // O(1) Lookup: Jump directly to parents
//                 let state = &self.states[city.state_id as usize];
//                 let country = &self.countries[city.country_id as usize];
//                 out.push((city, state, country));
//             }
//         }
//         out
//     }
//
//     /// Unified Search: Countries + States + Cities + Phone
//     fn smart_search(&self, query: &str) -> Vec<SmartHit<'_, B>> {
//         let q_raw = query.trim();
//         if q_raw.is_empty() {
//             return Vec::new();
//         }
//         let q = fold_key(q_raw);
//         let phone = q_raw.trim_start_matches('+');
//
//         let mut out: Vec<SmartHit<'_, B>> = Vec::new();
//
//         // 1. Scan Countries
//         for c in &self.countries {
//             // Exact ISO match (Highest Priority)
//             if c.iso2.as_ref().eq_ignore_ascii_case(q_raw) {
//                 out.push(SmartHit::country(100, c));
//                 continue;
//             }
//
//             // Fuzzy Name match
//             let cname = fold_key(c.name.as_ref());
//             if cname == q {
//                 out.push(SmartHit::country(90, c));
//             } else if cname.starts_with(&q) {
//                 out.push(SmartHit::country(80, c));
//             } else if cname.contains(&q) {
//                 out.push(SmartHit::country(70, c));
//             }
//         }
//
//         // 2. Scan States
//         for s in &self.states {
//             let sname = fold_key(s.name.as_ref());
//             if sname.starts_with(&q) {
//                 let c = &self.countries[s.country_id as usize];
//                 out.push(SmartHit::state(60, c, s));
//             } else if sname.contains(&q) {
//                 let c = &self.countries[s.country_id as usize];
//                 out.push(SmartHit::state(50, c, s));
//             }
//         }
//
//         // 3. Scan Cities (The heavy part)
//         for city in &self.cities {
//             let cname = fold_key(city.name.as_ref());
//
//             let mut score = 0;
//
//             // Exact match
//             if cname == q {
//                 score = 45;
//             }
//             // Prefix match
//             else if cname.starts_with(&q) {
//                 score = 40;
//             }
//
//             // Check Aliases (if logic dictates)
//             if score < 45 {
//                 if let Some(aliases) = &city.aliases {
//                     for a in aliases {
//                         let fa = fold_key(a);
//                         if fa == q { score = 45; break; }
//                         if fa.starts_with(&q) { score = std::cmp::max(score, 40); }
//                     }
//                 }
//             }
//
//             if score > 0 {
//                 let s = &self.states[city.state_id as usize];
//                 let c = &self.countries[city.country_id as usize];
//                 out.push(SmartHit::city(score, c, s, city));
//             }
//         }
//
//         // 4. Phone Codes (Using the trait implementation below)
//         for c in self.find_countries_by_phone_code(phone) {
//             out.push(SmartHit::country(20, c));
//         }
//
//         // Sort by relevance
//         out.sort_by(|a, b| b.score.cmp(&a.score));
//         out
//     }
// }
//
// // Implementation of the Phone Trait for the Flat DB
// impl<B: GeoBackend> PhoneCodeSearch<B> for GeoDb<B> {
//     fn find_countries_by_phone_code<'a>(&'a self, prefix: &str) -> Vec<&'a Country<B>> {
//         // Linear scan is perfectly fine for ~250 countries
//         self.countries
//             .iter()
//             .filter(|c| {
//                 c.phone_code
//                     .as_ref()
//                     .map(|p| p.as_ref().starts_with(prefix))
//                     .unwrap_or(false)
//             })
//             .collect()
//     }
// }