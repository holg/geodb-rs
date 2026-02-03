package com.example.geodb_flutter

import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.MethodChannel.MethodCallHandler
import io.flutter.plugin.common.MethodChannel.Result
import uniffi.geodb_ffi.GeoDbEngine
import uniffi.geodb_ffi.CityResult
import uniffi.geodb_ffi.DbStatsDto

/** GeodbFlutterPlugin */
class GeodbFlutterPlugin : FlutterPlugin, MethodCallHandler {
    private lateinit var channel: MethodChannel
    private var geodb: GeoDbEngine? = null

    override fun onAttachedToEngine(flutterPluginBinding: FlutterPlugin.FlutterPluginBinding) {
        channel = MethodChannel(flutterPluginBinding.binaryMessenger, "geodb_flutter")
        channel.setMethodCallHandler(this)
    }

    override fun onMethodCall(call: MethodCall, result: Result) {
        when (call.method) {
            "initialize" -> handleInitialize(result)
            "getStats" -> handleGetStats(result)
            "getCountryCount" -> handleGetCountryCount(result)
            "findCountryByCode" -> {
                val code = call.argument<String>("code")
                if (code == null) {
                    result.error("INVALID_ARGS", "Missing 'code'", null)
                } else {
                    handleFindCountryByCode(code, result)
                }
            }
            "findCountriesBySubstring" -> {
                val substring = call.argument<String>("substring")
                if (substring == null) {
                    result.error("INVALID_ARGS", "Missing 'substring'", null)
                } else {
                    handleFindCountriesBySubstring(substring, result)
                }
            }
            "findStatesBySubstring" -> {
                val substring = call.argument<String>("substring")
                if (substring == null) {
                    result.error("INVALID_ARGS", "Missing 'substring'", null)
                } else {
                    handleFindStatesBySubstring(substring, result)
                }
            }
            "findCitiesBySubstring" -> {
                val substring = call.argument<String>("substring")
                if (substring == null) {
                    result.error("INVALID_ARGS", "Missing 'substring'", null)
                } else {
                    handleFindCitiesBySubstring(substring, result)
                }
            }
            "findNearest" -> {
                val lat = call.argument<Double>("lat")
                val lng = call.argument<Double>("lng")
                val count = call.argument<Int>("count")
                if (lat == null || lng == null || count == null) {
                    result.error("INVALID_ARGS", "Missing lat, lng, or count", null)
                } else {
                    handleFindNearest(lat, lng, count, result)
                }
            }
            "findInRadius" -> {
                val lat = call.argument<Double>("lat")
                val lng = call.argument<Double>("lng")
                val radiusKm = call.argument<Double>("radiusKm")
                if (lat == null || lng == null || radiusKm == null) {
                    result.error("INVALID_ARGS", "Missing lat, lng, or radiusKm", null)
                } else {
                    handleFindInRadius(lat, lng, radiusKm, result)
                }
            }
            "smartSearch" -> {
                val query = call.argument<String>("query")
                if (query == null) {
                    result.error("INVALID_ARGS", "Missing 'query'", null)
                } else {
                    handleSmartSearch(query, result)
                }
            }
            "getPlatformVersion" -> {
                result.success("Android ${android.os.Build.VERSION.RELEASE}")
            }
            else -> result.notImplemented()
        }
    }

    // Handler Methods

    private fun handleInitialize(result: Result) {
        try {
            geodb = GeoDbEngine()
            result.success(null)
        } catch (e: Exception) {
            result.error("INIT_ERROR", "Failed to initialize GeoDB", e.message)
        }
    }

    private fun handleGetStats(result: Result) {
        val db = geodb
        if (db == null) {
            result.error("NOT_INITIALIZED", "GeoDB not initialized", null)
            return
        }

        val stats = db.stats()
        result.success(mapOf(
            "countries" to stats.countries.toLong(),
            "states" to stats.states.toLong(),
            "cities" to stats.cities.toLong()
        ))
    }

    private fun handleGetCountryCount(result: Result) {
        val db = geodb
        if (db == null) {
            result.error("NOT_INITIALIZED", "GeoDB not initialized", null)
            return
        }

        result.success(db.countryCount().toInt())
    }

    private fun handleFindCountryByCode(code: String, result: Result) {
        val db = geodb
        if (db == null) {
            result.error("NOT_INITIALIZED", "GeoDB not initialized", null)
            return
        }

        val country = db.findCountryByCode(code)
        if (country != null) {
            result.success(cityResultToMap(country))
        } else {
            result.success(null)
        }
    }

    private fun handleFindCountriesBySubstring(substring: String, result: Result) {
        val db = geodb
        if (db == null) {
            result.error("NOT_INITIALIZED", "GeoDB not initialized", null)
            return
        }

        val countries = db.findCountriesBySubstring(substring)
        result.success(countries.map { cityResultToMap(it) })
    }

    private fun handleFindStatesBySubstring(substring: String, result: Result) {
        val db = geodb
        if (db == null) {
            result.error("NOT_INITIALIZED", "GeoDB not initialized", null)
            return
        }

        val states = db.findStatesBySubstring(substring)
        result.success(states.map { cityResultToMap(it) })
    }

    private fun handleFindCitiesBySubstring(substring: String, result: Result) {
        val db = geodb
        if (db == null) {
            result.error("NOT_INITIALIZED", "GeoDB not initialized", null)
            return
        }

        val cities = db.findCitiesBySubstring(substring)
        result.success(cities.map { cityResultToMap(it) })
    }

    private fun handleFindNearest(lat: Double, lng: Double, count: Int, result: Result) {
        val db = geodb
        if (db == null) {
            result.error("NOT_INITIALIZED", "GeoDB not initialized", null)
            return
        }

        val cities = db.findNearest(lat, lng, count.toUInt())
        result.success(cities.map { cityResultToMap(it) })
    }

    private fun handleFindInRadius(lat: Double, lng: Double, radiusKm: Double, result: Result) {
        val db = geodb
        if (db == null) {
            result.error("NOT_INITIALIZED", "GeoDB not initialized", null)
            return
        }

        val cities = db.findInRadius(lat, lng, radiusKm)
        result.success(cities.map { cityResultToMap(it) })
    }

    private fun handleSmartSearch(query: String, result: Result) {
        val db = geodb
        if (db == null) {
            result.error("NOT_INITIALIZED", "GeoDB not initialized", null)
            return
        }

        val results = db.smartSearch(query)
        result.success(results.map { cityResultToMap(it) })
    }

    // Helper Methods

    private fun cityResultToMap(city: CityResult): Map<String, Any?> {
        return mapOf(
            "name" to city.name,
            "state" to city.state,
            "country" to city.country,
            "iso2" to city.iso2,
            "lat" to city.lat,
            "lng" to city.lng,
            "population" to city.population.toLong(),
            "distanceKm" to city.distanceKm
        )
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        channel.setMethodCallHandler(null)
        geodb = null
    }
}
