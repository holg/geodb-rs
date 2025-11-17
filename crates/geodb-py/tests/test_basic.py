import sys
import pytest


def test_import_module():
    # Basic import should work even if data isn't available on the system
    import geodb_rs  # noqa: F401


def test_load_default_smoke():
    import geodb_rs

    try:
        db = geodb_rs.PyGeoDb.load_default()
    except Exception as e:  # Data may be missing in dev mode; skip if so
        msg = str(e).lower()
        if "data file not found" in msg or "no such file" in msg:
            pytest.skip("geodb data not available in this environment; skipping runtime test")
        raise

    # If it loads, run a couple of cheap queries
    countries, states, cities = db.stats()
    assert countries > 0
    # Look up a commonly present ISO2 code
    maybe_us = db.find_country("US")
    if maybe_us is not None:
        assert maybe_us["iso2"] == "US"
