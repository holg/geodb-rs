use crate::model::domain::GeoDb;
use crate::traits::GeoBackend;
use bincode::Options;

impl<B: GeoBackend> GeoDb<B> {
    /// Reconstructs the database from a serialized binary format, optionally filtering
    /// countries by ISO2 code.
    ///
    /// This is a "Zero-Copy-ish" operation:
    /// 1. It deserializes the Master DB from the provided bytes.
    /// 2. If a filter is active, it performs a fast memory-copy of the relevant
    ///    state/city slices into a new structure, avoiding tree traversals.
    pub fn from_bytes(data: &[u8], filter_iso2: Option<&[&str]>) -> Result<Self, bincode::Error> {
        // Use standard bincode options
        // We use the 256MB limit you suggested to prevent malicious data bombs
        let master: GeoDb<B> = bincode::DefaultOptions::new()
            .with_limit(256 * 1024 * 1024)
            .allow_trailing_bytes()
            .deserialize(data)?;

        // If no filter is provided, return the master DB directly (Fast path)
        let filter = match filter_iso2 {
            Some(f) if !f.is_empty() => f,
            _ => return Ok(master),
        };

        // Fast Filter Path (Slice Copy)
        let mut new_db = GeoDb {
            countries: Vec::with_capacity(filter.len()),
            states: Vec::new(),
            cities: Vec::new(),
        };

        for country in master.countries {
            // Case-sensitive match here for speed; input should be normalized by caller
            if filter.contains(&country.iso2.as_ref()) {
                // 1. Calculate new offsets
                let s_start = new_db.states.len() as u32;
                let c_start = new_db.cities.len() as u32;

                // 2. Bulk Copy States (Memcpy)
                let s_slice = &master.states[country.states_range.clone()];
                new_db.states.extend_from_slice(s_slice);

                // 3. Bulk Copy Cities (Memcpy)
                let c_slice = &master.cities[country.cities_range.clone()];
                new_db.cities.extend_from_slice(c_slice);

                // 4. Add Country with UPDATED ranges
                let mut c = country.clone();
                c.states_range = s_start..(new_db.states.len() as u32);
                c.cities_range = c_start..(new_db.cities.len() as u32);

                new_db.countries.push(c);
            }
        }

        Ok(new_db)
    }
}
