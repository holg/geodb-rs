// NAPI Bridge for GeoDB
// This bridges ArkTS ↔ Rust C API

#include <napi/native_api.h>
#include <string>
#include <vector>

// Include our C API declarations
extern "C" {
    // Forward declarations from geodb_ffi C API
    struct CGeoDbEngine;
    struct CCityResult {
        char* name;
        char* state;
        char* country;
        char* iso2;
        double lat;
        double lng;
        uint64_t population;
        double distance_km;
        char* translations_json;
    };

    struct CDbStatsDto {
        uint64_t countries;
        uint64_t states;
        uint64_t cities;
    };

    struct CCityResultList {
        CCityResult* data;
        size_t len;
        size_t capacity;
    };

    // Function declarations
    CGeoDbEngine* geodb_engine_new();
    void geodb_engine_free(CGeoDbEngine* engine);
    CDbStatsDto geodb_engine_stats(const CGeoDbEngine* engine);
    uint64_t geodb_engine_country_count(const CGeoDbEngine* engine);
    CCityResultList geodb_engine_smart_search(const CGeoDbEngine* engine, const char* query);
    CCityResultList geodb_engine_find_nearest(const CGeoDbEngine* engine, double lat, double lng, uint32_t count);
    void geodb_city_result_list_free(CCityResultList list);
}

// Global engine instance (singleton)
static CGeoDbEngine* g_engine = nullptr;

// Helper: Convert C string to napi_value
static napi_value CStringToNapi(napi_env env, const char* str) {
    napi_value result;
    napi_create_string_utf8(env, str ? str : "", NAPI_AUTO_LENGTH, &result);
    return result;
}

// Helper: Convert CCityResult to JavaScript object
static napi_value CityResultToNapi(napi_env env, const CCityResult& city) {
    napi_value obj;
    napi_create_object(env, &obj);

    napi_value name = CStringToNapi(env, city.name);
    napi_value state = CStringToNapi(env, city.state);
    napi_value country = CStringToNapi(env, city.country);
    napi_value iso2 = CStringToNapi(env, city.iso2);

    napi_value lat, lng, population;
    napi_create_double(env, city.lat, &lat);
    napi_create_double(env, city.lng, &lng);
    napi_create_int64(env, city.population, &population);

    napi_set_named_property(env, obj, "name", name);
    napi_set_named_property(env, obj, "state", state);
    napi_set_named_property(env, obj, "country", country);
    napi_set_named_property(env, obj, "iso2", iso2);
    napi_set_named_property(env, obj, "lat", lat);
    napi_set_named_property(env, obj, "lng", lng);
    napi_set_named_property(env, obj, "population", population);

    // Handle optional distance
    if (city.distance_km >= 0) {
        napi_value distance;
        napi_create_double(env, city.distance_km, &distance);
        napi_set_named_property(env, obj, "distanceKm", distance);
    }

    return obj;
}

// NAPI Function: Initialize database
static napi_value Init(napi_env env, napi_callback_info info) {
    if (g_engine == nullptr) {
        g_engine = geodb_engine_new();
        if (g_engine == nullptr) {
            napi_throw_error(env, nullptr, "Failed to initialize GeoDB engine");
            return nullptr;
        }
    }

    napi_value result;
    napi_get_boolean(env, true, &result);
    return result;
}

// NAPI Function: Get stats
static napi_value GetStats(napi_env env, napi_callback_info info) {
    if (g_engine == nullptr) {
        napi_throw_error(env, nullptr, "GeoDB not initialized");
        return nullptr;
    }

    CDbStatsDto stats = geodb_engine_stats(g_engine);

    napi_value obj;
    napi_create_object(env, &obj);

    napi_value countries, states, cities;
    napi_create_int64(env, stats.countries, &countries);
    napi_create_int64(env, stats.states, &states);
    napi_create_int64(env, stats.cities, &cities);

    napi_set_named_property(env, obj, "countries", countries);
    napi_set_named_property(env, obj, "states", states);
    napi_set_named_property(env, obj, "cities", cities);

    return obj;
}

// NAPI Function: Smart search
static napi_value SmartSearch(napi_env env, napi_callback_info info) {
    if (g_engine == nullptr) {
        napi_throw_error(env, nullptr, "GeoDB not initialized");
        return nullptr;
    }

    // Get query parameter
    size_t argc = 1;
    napi_value args[1];
    napi_get_cb_info(env, info, &argc, args, nullptr, nullptr);

    if (argc < 1) {
        napi_throw_error(env, nullptr, "Query parameter required");
        return nullptr;
    }

    // Convert query to C string
    size_t query_len;
    napi_get_value_string_utf8(env, args[0], nullptr, 0, &query_len);
    std::string query(query_len, '\0');
    napi_get_value_string_utf8(env, args[0], &query[0], query_len + 1, &query_len);

    // Call Rust FFI
    CCityResultList results = geodb_engine_smart_search(g_engine, query.c_str());

    // Convert results to JavaScript array
    napi_value array;
    napi_create_array_with_length(env, results.len, &array);

    for (size_t i = 0; i < results.len; i++) {
        napi_value city = CityResultToNapi(env, results.data[i]);
        napi_set_element(env, array, i, city);
    }

    // Free C results
    geodb_city_result_list_free(results);

    return array;
}

// NAPI Function: Find nearest
static napi_value FindNearest(napi_env env, napi_callback_info info) {
    if (g_engine == nullptr) {
        napi_throw_error(env, nullptr, "GeoDB not initialized");
        return nullptr;
    }

    // Get parameters: lat, lng, count
    size_t argc = 3;
    napi_value args[3];
    napi_get_cb_info(env, info, &argc, args, nullptr, nullptr);

    if (argc < 3) {
        napi_throw_error(env, nullptr, "Requires 3 parameters: lat, lng, count");
        return nullptr;
    }

    double lat, lng;
    int32_t count;
    napi_get_value_double(env, args[0], &lat);
    napi_get_value_double(env, args[1], &lng);
    napi_get_value_int32(env, args[2], &count);

    // Call Rust FFI
    CCityResultList results = geodb_engine_find_nearest(g_engine, lat, lng, (uint32_t)count);

    // Convert results to JavaScript array
    napi_value array;
    napi_create_array_with_length(env, results.len, &array);

    for (size_t i = 0; i < results.len; i++) {
        napi_value city = CityResultToNapi(env, results.data[i]);
        napi_set_element(env, array, i, city);
    }

    // Free C results
    geodb_city_result_list_free(results);

    return array;
}

// Module initialization
EXTERN_C_START
static napi_value Init_Module(napi_env env, napi_value exports) {
    napi_property_descriptor desc[] = {
        { "init", nullptr, Init, nullptr, nullptr, nullptr, napi_default, nullptr },
        { "getStats", nullptr, GetStats, nullptr, nullptr, nullptr, napi_default, nullptr },
        { "smartSearch", nullptr, SmartSearch, nullptr, nullptr, nullptr, napi_default, nullptr },
        { "findNearest", nullptr, FindNearest, nullptr, nullptr, nullptr, napi_default, nullptr },
    };

    napi_define_properties(env, exports, sizeof(desc) / sizeof(desc[0]), desc);
    return exports;
}
EXTERN_C_END

// Register module
static napi_module geoDbModule = {
    .nm_version = 1,
    .nm_flags = 0,
    .nm_filename = nullptr,
    .nm_register_func = Init_Module,
    .nm_modname = "geodb_napi",
    .nm_priv = ((void*)0),
    .reserved = { 0 },
};

extern "C" __attribute__((constructor)) void RegisterGeoDbModule(void) {
    napi_module_register(&geoDbModule);
}
