# geodb-py

Python bindings for the [`geodb-rs`](https://github.com/holg/geodb-rs) project.

For full documentation, examples, and details about the underlying Rust library,
please see the main project README:

- Project repository: https://github.com/holg/geodb-rs
- Main README: https://github.com/holg/geodb-rs/blob/main/README.md



```python
# Load the database
import geodb_py
db = geodb.PyGeoDb.load_default()

# Find a country by ISO2 code
country = db.find_country_by_iso2("US")
if country:
print(f"Country: {country['name']}")
print(f"Capital: {country['capital']}")
print(f"Phone Code: {country['phone_code']}")
print(f"Currency: {country['currency']}")

# Search countries by phone code
countries = db.find_countries_by_phone_code("+1")
for country in countries:
    print(f"{country['name']} uses phone code +1")
```
