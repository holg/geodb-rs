#![allow(clippy::useless_conversion)]

// 1. Import the Prelude
use geodb_core::prelude::*;

// 2. Import Views for Serialization
use geodb_core::api::{CityView, CountryView, StateView};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde::Serialize;
use std::path::{Path, PathBuf};

// Helper to convert geodb-core Results into PyResult
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

// Helper to convert Rust Structs -> Python Dicts via JSON
// This is the safest way to ensure types match what Python expects.
fn to_py<'py, T: Serialize + ?Sized>(py: Python<'py>, value: &T) -> PyResult<Bound<'py, PyAny>> {
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

        let possible_paths = [
            module_dir.join("../geodb_rs_data/countries+states+cities.json.gz"),
            module_dir.join("data/countries+states+cities.json.gz"),
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
            format!("Data file not found. Searched in: {possible_paths:?}"),
        ))
    })
}

#[pymethods]
impl PyGeoDb {
    #[staticmethod]
    pub fn load() -> PyResult<Self> {
        let db = GeoDb::<DefaultBackend>::load().into_py()?;
        Ok(Self { inner: db })
    }

    #[staticmethod]
    #[pyo3(signature = (path, filter=None))]
    pub fn load_from_path(path: &str, filter: Option<Vec<String>>) -> PyResult<Self> {
        let filter_refs: Option<Vec<&str>> = filter
            .as_ref()
            .map(|v| v.iter().map(|s| s.as_str()).collect());
        let filter_slice: Option<&[&str]> = filter_refs.as_deref();
        let db = GeoDb::<DefaultBackend>::load_from_path(path, filter_slice).into_py()?;
        Ok(Self { inner: db })
    }

    #[staticmethod]
    pub fn load_default() -> PyResult<Self> {
        // Try bundled data first
        match find_bundled_data() {
            Ok(path) => {
                let path_str = path
                    .to_str()
                    .ok_or_else(|| PyRuntimeError::new_err("Invalid path"))?;
                // Note: load_raw_json builds cache automatically if 'builder' enabled
                // load_from_path handles both .json and .bin
                let db = GeoDb::<DefaultBackend>::load_from_path(path_str, None).into_py()?;
                Ok(Self { inner: db })
            }
            Err(_) => {
                // Fallback to core's embedded/default logic
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
            // FIX: Use Trait method
            let items: Vec<_> = self
                .inner
                .states_for_country(country)
                .iter()
                .map(|s| StateView { country, state: s })
                .collect();
            let obj = to_py(py, &items)?;
            Ok(Some(obj))
        } else {
            Ok(None)
        }
    }

    /// List all cities for a given state code (in a country)
    pub fn cities_in_state<'py>(
        &self,
        py: Python<'py>,
        iso2: &str,
        state_code: &str,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        if let Some(country) = self.inner.find_country_by_iso2(iso2) {
            let states = self.inner.states_for_country(country);
            if let Some(state) = states.iter().find(|s| s.state_code() == state_code) {
                // FIX: Use Trait method
                let items: Vec<_> = self
                    .inner
                    .cities_for_state(state)
                    .iter()
                    .map(|city| CityView {
                        country,
                        state,
                        city,
                    })
                    .collect();
                let obj = to_py(py, &items)?;
                return Ok(Some(obj));
            }
        }
        Ok(None)
    }

    /// Find countries by phone code (e.g. "+49", "1")
    pub fn search_countries_by_phone<'py>(
        &self,
        py: Python<'py>,
        phone: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Trait method already handles trimming
        let items: Vec<_> = self
            .inner
            .find_countries_by_phone_code(phone)
            .iter()
            .map(|c| CountryView(*c))
            .collect();
        to_py(py, &items)
    }

    /// Find countries containing a substring
    pub fn search_country_substring<'py>(
        &self,
        py: Python<'py>,
        substr: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Assuming find_countries_by_substring is exposed in GeoSearch trait now
        // (If not, use smart_search filtering)
        // Let's assume we exposed it in the trait as discussed.
        let items: Vec<_> = self
            .inner
            .find_countries_by_substring(substr)
            .iter()
            .map(|c| CountryView(*c))
            .collect();
        to_py(py, &items)
    }

    /// Find states containing a substring
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

    /// Find cities containing a substring
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

    // --- SPATIAL SEARCH ---

    /// Find nearest cities to a location (Lat, Lng)
    pub fn find_nearest<'py>(
        &self,
        py: Python<'py>,
        lat: f64,
        lng: f64,
        count: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let items: Vec<_> = self
            .inner
            .find_nearest(lat, lng, count)
            .into_iter()
            .map(|(city, state, country)| CityView {
                country,
                state,
                city,
            })
            .collect();
        to_py(py, &items)
    }

    /// Find cities within radius (km)
    pub fn find_in_radius<'py>(
        &self,
        py: Python<'py>,
        lat: f64,
        lng: f64,
        radius_km: f64,
    ) -> PyResult<Bound<'py, PyAny>> {
        // We need to generate the ID first because the trait expects GeoID
        let geoid = geodb_core::spatial::generate_geoid(lat, lng);

        let items: Vec<_> = self
            .inner
            .find_cities_in_radius_by_geoid(geoid, radius_km)
            .into_iter()
            .map(|(city, state, country)| CityView {
                country,
                state,
                city,
            })
            .collect();
        to_py(py, &items)
    }

    /// Smart search across countries, states, cities, and phone codes.
    pub fn smart_search<'py>(&self, py: Python<'py>, query: &str) -> PyResult<Bound<'py, PyAny>> {
        let hits = self.inner.smart_search(query);
        let mut out: Vec<serde_json::Value> = Vec::with_capacity(hits.len());

        for hit in hits {
            // Serialize the View based on the Enum variant
            let v = match hit.item {
                SmartItem::Country(c) => serde_json::to_value(CountryView(c)),
                SmartItem::State { country, state } => {
                    serde_json::to_value(&StateView { country, state })
                }
                SmartItem::City {
                    country,
                    state,
                    city,
                } => serde_json::to_value(&CityView {
                    country,
                    state,
                    city,
                }),
            }
            .map_err(|e| PyRuntimeError::new_err(format!("serde error: {e}")))?;

            out.push(v);
        }
        to_py(py, &out)
    }
}

/// Python module entry point
#[pymodule]
fn geodb_rs(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyGeoDb>()?;
    Ok(())
}
