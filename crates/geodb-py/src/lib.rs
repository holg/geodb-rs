#![allow(clippy::useless_conversion)]

use geodb_core::{
    CityView, CountryView, DefaultGeoDb, GeoDb, PhoneCodeSearch, SmartItem, StandardBackend,
    StateView,
};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde::Serialize;
use serde_json::to_value as to_json_value;

// Provide a single helper to convert geodb-core Results into PyResult
// while avoiding per-call mapping. Due to Rust orphan rules we cannot
// implement `From<GeoError> for PyErr` in this crate, so we add an
// extension trait and use `.into_py()?` at call sites.
trait IntoPyResult<T> {
    fn into_py(self) -> PyResult<T>;
}

impl<T> IntoPyResult<T> for geodb_core::Result<T> {
    fn into_py(self) -> PyResult<T> {
        self.map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}

#[pyclass]
pub struct PyGeoDb {
    inner: DefaultGeoDb,
}

fn to_py<'py, T: Serialize + ?Sized>(
    py: Python<'py>,
    value: &T,
) -> PyResult<Bound<'py, pyo3::PyAny>> {
    // Serialize to JSON string, then parse in Python via json.loads to get native dict/list
    let s = serde_json::to_string(value)
        .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("serde error: {e}")))?;
    let json_mod = PyModule::import_bound(py, "json")?;
    let loads = json_mod.getattr("loads")?;
    let obj = loads.call1((s,))?;
    Ok(obj)
}

// PyO3's #[pymethods] codegen can trigger a false-positive clippy::useless_conversion
// on PyResult return types. Allow it for this impl block to keep the wrapper thin.
#[allow(clippy::useless_conversion)]
#[pymethods]
impl PyGeoDb {
    #[staticmethod]
    #[allow(clippy::useless_conversion)]
    pub fn load(path: &str) -> PyResult<Self> {
        // Avoid triggering clippy::useless_conversion by mapping the error directly here
        match GeoDb::<StandardBackend>::load_from_path(path, None) {
            Ok(db) => Ok(Self { inner: db }),
            Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
        }
    }

    #[staticmethod]
    #[allow(clippy::useless_conversion)]
    pub fn load_default() -> PyResult<Self> {
        let db = GeoDb::<StandardBackend>::load().into_py()?;
        Ok(Self { inner: db })
    }

    #[staticmethod]
    pub fn load_filtered(iso2_list: Vec<String>) -> PyResult<Self> {
        let tmp: Vec<String> = iso2_list
            .into_iter()
            .map(|s| s.trim().to_string())
            .collect();
        let refs: Vec<&str> = tmp.iter().map(|s| s.as_str()).collect();
        let db = GeoDb::<StandardBackend>::load_filtered_by_iso2(&refs).into_py()?;
        Ok(Self { inner: db })
    }

    pub fn stats(&self) -> PyResult<(usize, usize, usize)> {
        let s = self.inner.stats();
        Ok((s.countries, s.states, s.cities))
    }

    /// Return a list of all countries as dicts
    pub fn countries<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::PyAny>> {
        let items: Vec<_> = self.inner.countries().iter().map(CountryView).collect();
        to_py(py, &items)
    }

    /// Find a country by ISO2/ISO3/code and return as dict (or None)
    pub fn find_country<'py>(
        &self,
        py: Python<'py>,
        code: &str,
    ) -> PyResult<Option<Bound<'py, pyo3::PyAny>>> {
        if let Some(c) = self.inner.find_country_by_code(code) {
            let v = to_py(py, &CountryView(c))?;
            Ok(Some(v))
        } else {
            Ok(None)
        }
    }

    /// List all states for a given country ISO2 as dicts
    pub fn states_in_country<'py>(
        &self,
        py: Python<'py>,
        iso2: &str,
    ) -> PyResult<Option<Bound<'py, pyo3::PyAny>>> {
        if let Some(country) = self.inner.find_country_by_iso2(iso2) {
            let items: Vec<_> = country
                .states()
                .iter()
                .map(|s| StateView { country, state: s })
                .collect();
            let obj = to_py(py, &items)?;
            Ok(Some(obj))
        } else {
            Ok(None)
        }
    }

    /// Find countries by phone code (e.g. "+49", "1") and return list of dicts
    pub fn search_countries_by_phone<'py>(
        &self,
        py: Python<'py>,
        phone: &str,
    ) -> PyResult<Bound<'py, pyo3::PyAny>> {
        let code = phone.trim().trim_start_matches('+');
        let items: Vec<_> = self
            .inner
            .find_countries_by_phone_code(code)
            .iter()
            .map(|c| CountryView(c))
            .collect();
        to_py(py, &items)
    }

    /// Find states containing a substring (ASCII, case-insensitive). Returns list of dicts
    pub fn find_states_by_substring<'py>(
        &self,
        py: Python<'py>,
        substr: &str,
    ) -> PyResult<Bound<'py, pyo3::PyAny>> {
        let items: Vec<_> = self
            .inner
            .find_states_by_substring(substr)
            .into_iter()
            .map(|(state, country)| StateView { country, state })
            .collect();
        to_py(py, &items)
    }

    /// Find cities containing a substring (ASCII, case-insensitive). Returns list of dicts
    pub fn find_cities_by_substring<'py>(
        &self,
        py: Python<'py>,
        substr: &str,
    ) -> PyResult<Bound<'py, pyo3::PyAny>> {
        let items: Vec<_> = self
            .inner
            .find_cities_by_substring(substr)
            .into_iter()
            .map(|(city, state, country)| CityView {
                country,
                state,
                city,
            })
            .collect();
        to_py(py, &items)
    }

    /// Smart search across countries, states, cities, and phone codes. Returns list of dicts
    pub fn smart_search<'py>(
        &self,
        py: Python<'py>,
        query: &str,
    ) -> PyResult<Bound<'py, pyo3::PyAny>> {
        let hits = self.inner.smart_search(query);
        // Map to a homogeneous list by emitting the view of the matched entity
        let mut out: Vec<serde_json::Value> = Vec::with_capacity(hits.len());
        for hit in hits {
            let v = match hit.item {
                SmartItem::Country(c) => to_json_value(CountryView(c)).unwrap(),
                SmartItem::State { country, state } => {
                    to_json_value(&StateView { country, state }).unwrap()
                }
                SmartItem::City {
                    country,
                    state,
                    city,
                } => to_json_value(&CityView {
                    country,
                    state,
                    city,
                })
                .unwrap(),
            };
            out.push(v);
        }
        to_py(py, &out)
    }
}

/// Python module entry point
#[pymodule]
fn geodb(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyGeoDb>()?;
    Ok(())
}
