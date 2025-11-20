#![allow(clippy::useless_conversion)]

// 1. Import the Prelude (Crucial for Trait methods like .find_country_by_iso2)
use geodb_core::prelude::*;

// 2. Import Views for Serialization
use geodb_core::api::{CityView, CountryView, StateView};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde::Serialize;
use std::path::{Path, PathBuf};

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

fn to_py<'py, T: Serialize + ?Sized>(py: Python<'py>, value: &T) -> PyResult<Bound<'py, PyAny>> {
    // Serialize to JSON string, then parse in Python via json.loads to get native dict/list
    let s = serde_json::to_string(value)
        .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("serde error: {e}")))?;
    let json_mod = PyModule::import_bound(py, "json")?;
    let loads = json_mod.getattr("loads")?;
    let obj = loads.call1((s,))?;
    Ok(obj)
}

/// Find the bundled data file in the Python package
fn find_bundled_data() -> PyResult<PathBuf> {
    Python::with_gil(|py| {
        let geodb_module = py.import_bound("geodb_rs")?;
        let module_path = geodb_module.getattr("__file__")?.extract::<String>()?;

        let module_dir = Path::new(&module_path).parent().ok_or_else(|| {
            PyErr::new::<PyRuntimeError, _>("Could not determine module directory")
        })?;

        // Try multiple possible locations for the data file
        let possible_paths = [
            // Location when installed via pip (maturin's data directory)
            module_dir.join("../geodb_rs_data/countries+states+cities.json.gz"),
            // Location if data is in the package itself
            module_dir.join("data/countries+states+cities.json.gz"),
            // Fallback to sibling directory
            module_dir.join("geodb_rs_data/countries+states+cities.json.gz"),
        ];

        for path in &possible_paths {
            if path.exists() {
                return path.canonicalize().map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Failed to canonicalize path: {e}"))
                });
            }
        }

        Err(PyErr::new::<pyo3::exceptions::PyFileNotFoundError, _>(
            format!(
                "Data file not found. Searched in:\n{}",
                possible_paths
                    .iter()
                    .map(|p| format!("  - {}", p.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        ))
    })
}

#[pymethods]
impl PyGeoDb {
    #[staticmethod]
    pub fn load(path: &str) -> PyResult<Self> {
        let db = GeoDb::<DefaultBackend>::load_from_path(path, None).into_py()?;
        Ok(Self { inner: db })
    }

    #[staticmethod]
    pub fn load_default() -> PyResult<Self> {
        // Try to find bundled data first, fall back to geodb-core's default
        match find_bundled_data() {
            Ok(path) => {
                let path_str = path
                    .to_str()
                    .ok_or_else(|| PyRuntimeError::new_err("Invalid path encoding"))?;
                let db = GeoDb::<DefaultBackend>::load_from_path(path_str, None).into_py()?;
                Ok(Self { inner: db })
            }
            Err(_) => {
                // Fall back to geodb-core's default location
                let db = GeoDb::<DefaultBackend>::load().into_py()?;
                Ok(Self { inner: db })
            }
        }
    }

    #[staticmethod]
    pub fn load_filtered(iso2_list: Vec<String>) -> PyResult<Self> {
        let tmp: Vec<String> = iso2_list
            .into_iter()
            .map(|s| s.trim().to_string())
            .collect();
        let refs: Vec<&str> = tmp.iter().map(String::as_str).collect();
        let db = GeoDb::<DefaultBackend>::load_filtered_by_iso2(&refs).into_py()?;
        Ok(Self { inner: db })
    }

    pub fn stats(&self) -> PyResult<(usize, usize, usize)> {
        let s = self.inner.stats();
        Ok((s.countries, s.states, s.cities))
    }

    /// Return a list of all countries as dicts
    pub fn countries<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let items: Vec<_> = self.inner.countries().iter().map(CountryView).collect();
        to_py(py, &items)
    }

    /// Find a country by ISO2/ISO3/code and return as dict (or None)
    pub fn find_country<'py>(
        &self,
        py: Python<'py>,
        code: &str,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
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
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
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
    ) -> PyResult<Bound<'py, PyAny>> {
        let code = phone.trim().trim_start_matches('+');
        let items: Vec<_> = self
            .inner
            .find_countries_by_phone_code(code)
            .iter()
            .map(|c| CountryView(*c))
            .collect();
        to_py(py, &items)
    }

    /// Find states containing a substring (ASCII, case-insensitive). Returns list of dicts
    pub fn find_states_by_substring<'py>(
        &self,
        py: Python<'py>,
        substr: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
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
    ) -> PyResult<Bound<'py, PyAny>> {
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
    pub fn smart_search<'py>(&self, py: Python<'py>, query: &str) -> PyResult<Bound<'py, PyAny>> {
        let hits = self.inner.smart_search(query);
        // Map to a homogeneous list by emitting the view of the matched entity
        let mut out: Vec<serde_json::Value> = Vec::with_capacity(hits.len());
        for hit in hits {
            let v = match hit.item {
                SmartItem::Country(c) => serde_json::to_value(CountryView(c))
                    .map_err(|e| PyRuntimeError::new_err(format!("serde error: {e}")))?,
                SmartItem::State { country, state } => {
                    serde_json::to_value(&StateView { country, state })
                        .map_err(|e| PyRuntimeError::new_err(format!("serde error: {e}")))?
                }
                SmartItem::City {
                    country,
                    state,
                    city,
                } => serde_json::to_value(&CityView {
                    country,
                    state,
                    city,
                })
                .map_err(|e| PyRuntimeError::new_err(format!("serde error: {e}")))?,
            };
            out.push(v);
        }
        to_py(py, &out)
    }
}

/// Python module entry point
/// IMPORTANT: The function name must match the module-name in pyproject.toml
#[pymodule]
fn geodb_rs(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyGeoDb>()?;
    Ok(())
}
