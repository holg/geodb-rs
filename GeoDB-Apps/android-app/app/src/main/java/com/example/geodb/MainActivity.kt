package com.example.geodb

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.example.geodb.ui.theme.GeoDBAppTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.geodb_ffi.CityResult
import uniffi.geodb_ffi.DbStatsDto
import uniffi.geodb_ffi.GeoDbEngine

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            GeoDBAppTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    GeoDBScreen()
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GeoDBScreen() {
    var searchQuery by remember { mutableStateOf("") }
    var searchResults by remember { mutableStateOf(listOf<CityResult>()) }
    var isLoading by remember { mutableStateOf(false) }
    var isInitialized by remember { mutableStateOf(false) }
    var dbStats: DbStatsDto? by remember { mutableStateOf(null) }
    var errorMessage: String? by remember { mutableStateOf(null) }
    var engine: GeoDbEngine? by remember { mutableStateOf(null) }

    // Location search fields
    var latInput by remember { mutableStateOf("") }
    var lngInput by remember { mutableStateOf("") }
    var radiusInput by remember { mutableStateOf("10") }
    var searchMode by remember { mutableStateOf(SearchMode.TEXT) }

    // Detail dialog
    var selectedCity: CityResult? by remember { mutableStateOf(null) }

    val coroutineScope = rememberCoroutineScope()

    // Initialize database on first composition
    LaunchedEffect(Unit) {
        withContext(Dispatchers.IO) {
            try {
                val db = GeoDbEngine()
                engine = db
                dbStats = db.stats()
                isInitialized = true
            } catch (e: Exception) {
                errorMessage = "Failed to initialize: ${e.message}"
            }
        }
    }

    // Detail dialog
    selectedCity?.let { city ->
        CityDetailDialog(
            city = city,
            onDismiss = { selectedCity = null },
            onUseLocation = { lat, lng ->
                latInput = String.format("%.6f", lat)
                lngInput = String.format("%.6f", lng)
                selectedCity = null
            }
        )
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp)
    ) {
        // Header
        Text(
            text = "GeoDB",
            style = MaterialTheme.typography.headlineLarge,
            fontWeight = FontWeight.Bold
        )

        dbStats?.let { stats ->
            Text(
                text = "${stats.countries} countries, ${stats.states} states, ${stats.cities} cities",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.secondary
            )
        }

        Spacer(modifier = Modifier.height(12.dp))

        // Search mode tabs
        TabRow(selectedTabIndex = searchMode.ordinal) {
            Tab(
                selected = searchMode == SearchMode.TEXT,
                onClick = { searchMode = SearchMode.TEXT },
                text = { Text("Text Search") }
            )
            Tab(
                selected = searchMode == SearchMode.NEAREST,
                onClick = { searchMode = SearchMode.NEAREST },
                text = { Text("Nearest") }
            )
            Tab(
                selected = searchMode == SearchMode.RADIUS,
                onClick = { searchMode = SearchMode.RADIUS },
                text = { Text("In Radius") }
            )
        }

        Spacer(modifier = Modifier.height(12.dp))

        when (searchMode) {
            SearchMode.TEXT -> {
                // Text search field
                OutlinedTextField(
                    value = searchQuery,
                    onValueChange = { newQuery ->
                        searchQuery = newQuery
                        if (newQuery.length >= 2 && isInitialized && engine != null) {
                            coroutineScope.launch {
                                isLoading = true
                                withContext(Dispatchers.IO) {
                                    try {
                                        searchResults = engine!!.smartSearch(newQuery).take(30)
                                        errorMessage = null
                                    } catch (e: Exception) {
                                        errorMessage = "Search error: ${e.message}"
                                        searchResults = emptyList()
                                    }
                                }
                                isLoading = false
                            }
                        } else if (newQuery.isEmpty()) {
                            searchResults = emptyList()
                        }
                    },
                    label = { Text("Search cities, states, countries...") },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = isInitialized,
                    singleLine = true
                )
            }

            SearchMode.NEAREST, SearchMode.RADIUS -> {
                // Location input fields
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    OutlinedTextField(
                        value = latInput,
                        onValueChange = { latInput = it },
                        label = { Text("Latitude") },
                        modifier = Modifier.weight(1f),
                        enabled = isInitialized,
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal)
                    )
                    OutlinedTextField(
                        value = lngInput,
                        onValueChange = { lngInput = it },
                        label = { Text("Longitude") },
                        modifier = Modifier.weight(1f),
                        enabled = isInitialized,
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal)
                    )
                }

                Spacer(modifier = Modifier.height(8.dp))

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    if (searchMode == SearchMode.RADIUS) {
                        OutlinedTextField(
                            value = radiusInput,
                            onValueChange = { radiusInput = it },
                            label = { Text("Radius (km)") },
                            modifier = Modifier.weight(1f),
                            enabled = isInitialized,
                            singleLine = true,
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number)
                        )
                    } else {
                        Spacer(modifier = Modifier.weight(1f))
                    }

                    Button(
                        onClick = {
                            val lat = latInput.toDoubleOrNull()
                            val lng = lngInput.toDoubleOrNull()
                            if (lat != null && lng != null && engine != null) {
                                coroutineScope.launch {
                                    isLoading = true
                                    withContext(Dispatchers.IO) {
                                        try {
                                            searchResults = if (searchMode == SearchMode.NEAREST) {
                                                engine!!.findNearest(lat, lng, 20u)
                                            } else {
                                                val radius = radiusInput.toDoubleOrNull() ?: 10.0
                                                engine!!.findInRadius(lat, lng, radius).take(50)
                                            }
                                            errorMessage = null
                                        } catch (e: Exception) {
                                            errorMessage = "Search error: ${e.message}"
                                            searchResults = emptyList()
                                        }
                                    }
                                    isLoading = false
                                }
                            } else {
                                errorMessage = "Please enter valid latitude and longitude"
                            }
                        },
                        enabled = isInitialized && latInput.isNotEmpty() && lngInput.isNotEmpty()
                    ) {
                        Text(if (searchMode == SearchMode.NEAREST) "Find Nearest" else "Search Radius")
                    }
                }
            }
        }

        Spacer(modifier = Modifier.height(8.dp))

        // Error message
        errorMessage?.let {
            Text(
                text = it,
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodySmall
            )
            Spacer(modifier = Modifier.height(8.dp))
        }

        // Loading or results
        if (!isInitialized && errorMessage == null) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    CircularProgressIndicator()
                    Spacer(modifier = Modifier.height(8.dp))
                    Text("Initializing database...")
                }
            }
        } else if (isLoading) {
            Box(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                contentAlignment = Alignment.Center
            ) {
                CircularProgressIndicator(modifier = Modifier.size(24.dp))
            }
        } else {
            // Results count
            if (searchResults.isNotEmpty()) {
                Text(
                    text = "${searchResults.size} results",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.secondary
                )
                Spacer(modifier = Modifier.height(4.dp))
            }

            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                items(searchResults) { city ->
                    CityCard(
                        city = city,
                        onClick = { selectedCity = city }
                    )
                }
            }
        }
    }
}

enum class SearchMode {
    TEXT, NEAREST, RADIUS
}

@Composable
fun CityCard(city: CityResult, onClick: () -> Unit) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick),
        elevation = CardDefaults.cardElevation(defaultElevation = 2.dp)
    ) {
        Column(
            modifier = Modifier.padding(16.dp)
        ) {
            Text(
                text = city.name,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold
            )
            if (city.state.isNotEmpty()) {
                Text(
                    text = "${city.state}, ${city.country}",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.secondary
                )
            } else {
                Text(
                    text = city.country,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.secondary
                )
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Text(
                    text = "${String.format("%.4f", city.lat)}, ${String.format("%.4f", city.lng)}",
                    style = MaterialTheme.typography.bodySmall
                )
                city.distanceKm?.let { dist ->
                    Text(
                        text = String.format("%.1f km", dist),
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.primary
                    )
                }
            }
        }
    }
}

@Composable
fun CityDetailDialog(
    city: CityResult,
    onDismiss: () -> Unit,
    onUseLocation: (Double, Double) -> Unit
) {
    Dialog(onDismissRequest = onDismiss) {
        Card(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            elevation = CardDefaults.cardElevation(defaultElevation = 8.dp)
        ) {
            Column(
                modifier = Modifier.padding(20.dp)
            ) {
                Text(
                    text = city.name,
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold
                )

                Spacer(modifier = Modifier.height(16.dp))

                DetailRow("Type", when {
                    city.state.isEmpty() && city.name == city.country -> "Country"
                    city.state == city.name -> "State/Region"
                    else -> "City"
                })

                if (city.state.isNotEmpty() && city.state != city.name) {
                    DetailRow("State/Region", city.state)
                }

                DetailRow("Country", city.country)
                DetailRow("ISO Code", city.iso2)
                DetailRow("Latitude", String.format("%.6f", city.lat))
                DetailRow("Longitude", String.format("%.6f", city.lng))

                if (city.population > 0uL) {
                    DetailRow("Population", String.format("%,d", city.population.toLong()))
                }

                city.distanceKm?.let { dist ->
                    DetailRow("Distance", String.format("%.2f km", dist))
                }

                Spacer(modifier = Modifier.height(20.dp))

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    OutlinedButton(
                        onClick = onDismiss,
                        modifier = Modifier.weight(1f)
                    ) {
                        Text("Close")
                    }
                    Button(
                        onClick = { onUseLocation(city.lat, city.lng) },
                        modifier = Modifier.weight(1f)
                    ) {
                        Text("Use Location")
                    }
                }
            }
        }
    }
}

@Composable
fun DetailRow(label: String, value: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        horizontalArrangement = Arrangement.SpaceBetween
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.secondary
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium
        )
    }
}
