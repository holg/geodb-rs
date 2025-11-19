/// Simple aggregate statistics for the database.
///
/// Returned by [`GeoDb::stats`], these counts reflect the materialized
/// in-memory database after any filtering that might have been applied at
/// load time.
/// use
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DbStats {
    pub countries: usize,
    pub states: usize,
    pub cities: usize,
}

