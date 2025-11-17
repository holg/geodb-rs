
import geodb_rs

print("-- Load default bundled dataset --")
db = geodb_rs.PyGeoDb.load_default()

countries, states, cities = db.stats()
print(f"Stats -> Countries: {countries}, States: {states}, Cities: {cities}")

print("\n-- List a few countries --")
all_c = db.countries()
print(all_c[:5])

print("\n-- Find country by code --")
print(db.find_country("US"))
print(db.find_country("deu"))

print("\n-- States in country (ISO2) --")
print(db.states_in_country("US")[:5])

print("\n-- Search by phone code --")
print(db.search_countries_by_phone("+1")[:5])

print("\n-- State substring search --")
print(db.find_states_by_substring("bavar")[:5])

print("\n-- City substring search --")
print(db.find_cities_by_substring("berlin")[:5])

print("\n-- Smart search --")
print(db.smart_search("berlin")[:5])

print("\n-- Filtered load (DE, FR) --")
fdb = geodb_rs.PyGeoDb.load_filtered(["DE", "FR"])
print(fdb.stats())

def __main__():
    pass