import 'dart:async';
import 'package:flutter/material.dart';
import 'package:geodb_flutter/geodb_flutter.dart';
import 'package:geolocator/geolocator.dart';

void main() {
  runApp(const GeoDBCityAutocompleteApp());
}

class GeoDBCityAutocompleteApp extends StatelessWidget {
  const GeoDBCityAutocompleteApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'City Autocomplete',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.blue),
        useMaterial3: true,
      ),
      home: const CityAutocompleteScreen(),
    );
  }
}

class CityAutocompleteScreen extends StatefulWidget {
  const CityAutocompleteScreen({super.key});

  @override
  State<CityAutocompleteScreen> createState() => _CityAutocompleteScreenState();
}

class _CityAutocompleteScreenState extends State<CityAutocompleteScreen> {
  final _geoDb = GeodbFlutter();
  final _searchController = TextEditingController();
  final _focusNode = FocusNode();

  bool _isInitialized = false;
  bool _isLoading = false;
  bool _isLocating = false;
  String? _error;
  List<CityResult> _suggestions = [];
  CityResult? _selectedCity;
  DbStats? _stats;
  Position? _currentPosition;

  Timer? _debounce;

  @override
  void initState() {
    super.initState();
    _initializeDatabase();
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _searchController.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  Future<void> _initializeDatabase() async {
    setState(() {
      _isLoading = true;
      _error = null;
    });

    try {
      await _geoDb.initialize();
      final stats = await _geoDb.getStats();
      setState(() {
        _isInitialized = true;
        _stats = stats;
        _isLoading = false;
      });
    } catch (e) {
      setState(() {
        _error = 'Failed to initialize database: $e';
        _isLoading = false;
      });
    }
  }

  void _onSearchChanged(String query) {
    _debounce?.cancel();

    if (query.isEmpty) {
      setState(() {
        _suggestions = [];
      });
      return;
    }

    // Debounce search by 150ms
    _debounce = Timer(const Duration(milliseconds: 150), () {
      _searchCities(query);
    });
  }

  Future<void> _searchCities(String query) async {
    if (!_isInitialized || query.length < 2) return;

    try {
      final results = await _geoDb.findCitiesBySubstring(query);
      if (mounted) {
        setState(() {
          _suggestions = results.take(15).toList();
        });
      }
    } catch (e) {
      debugPrint('Search error: $e');
    }
  }

  void _selectCity(CityResult city) {
    setState(() {
      _selectedCity = city;
      _suggestions = [];
      _searchController.text = city.name;
    });
    _focusNode.unfocus();
  }

  void _clearSelection() {
    setState(() {
      _selectedCity = null;
      _suggestions = [];
      _currentPosition = null;
      _searchController.clear();
    });
  }

  Future<void> _useCurrentLocation() async {
    if (!_isInitialized) return;

    setState(() {
      _isLocating = true;
      _error = null;
    });

    try {
      // Check if location services are enabled
      bool serviceEnabled = await Geolocator.isLocationServiceEnabled();
      if (!serviceEnabled) {
        setState(() {
          _error = 'Location services are disabled. Please enable them in Settings.';
          _isLocating = false;
        });
        return;
      }

      // Check and request permission
      LocationPermission permission = await Geolocator.checkPermission();
      if (permission == LocationPermission.denied) {
        permission = await Geolocator.requestPermission();
        if (permission == LocationPermission.denied) {
          setState(() {
            _error = 'Location permission denied.';
            _isLocating = false;
          });
          return;
        }
      }

      if (permission == LocationPermission.deniedForever) {
        setState(() {
          _error = 'Location permission permanently denied. Please enable in Settings.';
          _isLocating = false;
        });
        return;
      }

      // Get current position (try current first, then fall back to last known)
      Position? position;
      try {
        position = await Geolocator.getCurrentPosition(
          locationSettings: const LocationSettings(
            accuracy: LocationAccuracy.low,
            timeLimit: Duration(seconds: 15),
          ),
        );
      } catch (e) {
        // Try to get last known position as fallback
        position = await Geolocator.getLastKnownPosition();
        if (position == null) {
          setState(() {
            _error = 'Could not get location. On emulator, set location in Extended Controls (⋮ → Location).';
            _isLocating = false;
          });
          return;
        }
      }

      // Find nearest city
      final nearest = await _geoDb.findNearest(
        lat: position.latitude,
        lng: position.longitude,
        count: 1,
      );

      if (mounted) {
        setState(() {
          _currentPosition = position;
          _isLocating = false;
          if (nearest.isNotEmpty) {
            _selectedCity = nearest.first;
            _suggestions = [];
            _searchController.text = nearest.first.name;
          } else {
            _error = 'No city found near your location.';
          }
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = 'Failed to get location: $e';
          _isLocating = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('City Autocomplete'),
        backgroundColor: Theme.of(context).colorScheme.inversePrimary,
      ),
      body: SafeArea(
        child: Column(
          children: [
            // Database stats header
            if (_stats != null)
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(12),
                color: Colors.blue.shade50,
                child: Text(
                  '${_stats!.cities} cities | ${_stats!.states} states | ${_stats!.countries} countries',
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: Colors.blue.shade700,
                  ),
                  textAlign: TextAlign.center,
                ),
              ),

            // Search field and location button
            Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: TextField(
                          controller: _searchController,
                          focusNode: _focusNode,
                          enabled: _isInitialized,
                          decoration: InputDecoration(
                            hintText: _isInitialized
                                ? 'Search for a city...'
                                : 'Loading database...',
                            prefixIcon: const Icon(Icons.search),
                            suffixIcon: _searchController.text.isNotEmpty
                                ? IconButton(
                                    icon: const Icon(Icons.clear),
                                    onPressed: _clearSelection,
                                  )
                                : null,
                            border: OutlineInputBorder(
                              borderRadius: BorderRadius.circular(12),
                            ),
                            filled: true,
                            fillColor: Colors.grey.shade100,
                          ),
                          onChanged: _onSearchChanged,
                        ),
                      ),
                      const SizedBox(width: 12),
                      // Location button
                      SizedBox(
                        height: 56,
                        child: ElevatedButton(
                          onPressed: _isInitialized && !_isLocating
                              ? _useCurrentLocation
                              : null,
                          style: ElevatedButton.styleFrom(
                            backgroundColor: Colors.blue,
                            foregroundColor: Colors.white,
                            shape: RoundedRectangleBorder(
                              borderRadius: BorderRadius.circular(12),
                            ),
                            padding: const EdgeInsets.symmetric(horizontal: 16),
                          ),
                          child: _isLocating
                              ? const SizedBox(
                                  width: 24,
                                  height: 24,
                                  child: CircularProgressIndicator(
                                    strokeWidth: 2,
                                    color: Colors.white,
                                  ),
                                )
                              : const Icon(Icons.my_location),
                        ),
                      ),
                    ],
                  ),

                  // Loading indicator
                  if (_isLoading)
                    const Padding(
                      padding: EdgeInsets.only(top: 16),
                      child: CircularProgressIndicator(),
                    ),

                  // Error message
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 16),
                      child: Text(
                        _error!,
                        style: TextStyle(color: Colors.red.shade700),
                        textAlign: TextAlign.center,
                      ),
                    ),
                ],
              ),
            ),

            // Suggestions list
            if (_suggestions.isNotEmpty)
              Expanded(
                child: ListView.builder(
                  itemCount: _suggestions.length,
                  itemBuilder: (context, index) {
                    final city = _suggestions[index];
                    return _CityListTile(
                      city: city,
                      onTap: () => _selectCity(city),
                    );
                  },
                ),
              ),

            // Selected city details
            if (_selectedCity != null && _suggestions.isEmpty)
              Expanded(
                child: _SelectedCityCard(
                  city: _selectedCity!,
                  currentPosition: _currentPosition,
                ),
              ),

            // Empty state
            if (_isInitialized &&
                _suggestions.isEmpty &&
                _selectedCity == null &&
                _searchController.text.isEmpty)
              Expanded(
                child: Center(
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      const Icon(
                        Icons.location_city,
                        size: 64,
                        color: Colors.grey,
                      ),
                      const SizedBox(height: 16),
                      const Text(
                        'Search for a city or',
                        style: TextStyle(
                          fontSize: 16,
                          color: Colors.grey,
                        ),
                      ),
                      const SizedBox(height: 8),
                      TextButton.icon(
                        onPressed: _useCurrentLocation,
                        icon: const Icon(Icons.my_location),
                        label: const Text('Use current location'),
                      ),
                    ],
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _CityListTile extends StatelessWidget {
  final CityResult city;
  final VoidCallback onTap;

  const _CityListTile({
    required this.city,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return ListTile(
      leading: CircleAvatar(
        backgroundColor: Colors.blue.shade100,
        child: Text(
          city.iso2,
          style: TextStyle(
            fontSize: 12,
            fontWeight: FontWeight.bold,
            color: Colors.blue.shade700,
          ),
        ),
      ),
      title: Text(
        city.name,
        style: const TextStyle(fontWeight: FontWeight.w500),
      ),
      subtitle: Text(
        [city.state, city.country]
            .where((s) => s.isNotEmpty)
            .join(', '),
        style: TextStyle(color: Colors.grey.shade600),
      ),
      trailing: const Icon(Icons.chevron_right),
      onTap: onTap,
    );
  }
}

class _SelectedCityCard extends StatelessWidget {
  final CityResult city;
  final Position? currentPosition;

  const _SelectedCityCard({
    required this.city,
    this.currentPosition,
  });

  @override
  Widget build(BuildContext context) {
    // Calculate distance if we have current position
    double? distanceKm;
    if (currentPosition != null) {
      distanceKm = Geolocator.distanceBetween(
        currentPosition!.latitude,
        currentPosition!.longitude,
        city.lat,
        city.lng,
      ) / 1000; // Convert to km
    }

    return SingleChildScrollView(
      padding: const EdgeInsets.all(16),
      child: Card(
        elevation: 4,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(16),
        ),
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // City name header
              Row(
                children: [
                  CircleAvatar(
                    radius: 24,
                    backgroundColor: Colors.blue.shade100,
                    child: Text(
                      city.iso2,
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.bold,
                        color: Colors.blue.shade700,
                      ),
                    ),
                  ),
                  const SizedBox(width: 16),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          city.name,
                          style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                        Text(
                          [city.state, city.country]
                              .where((s) => s.isNotEmpty)
                              .join(', '),
                          style: TextStyle(
                            color: Colors.grey.shade600,
                            fontSize: 14,
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),

              const Divider(height: 32),

              // Distance from current location
              if (distanceKm != null)
                Container(
                  padding: const EdgeInsets.all(12),
                  margin: const EdgeInsets.only(bottom: 16),
                  decoration: BoxDecoration(
                    color: Colors.green.shade50,
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: Colors.green.shade200),
                  ),
                  child: Row(
                    children: [
                      Icon(Icons.near_me, color: Colors.green.shade700),
                      const SizedBox(width: 12),
                      Text(
                        '${distanceKm.toStringAsFixed(1)} km from your location',
                        style: TextStyle(
                          color: Colors.green.shade700,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ],
                  ),
                ),

              // Location details
              _DetailRow(
                icon: Icons.location_on,
                label: 'Coordinates',
                value: '${city.lat.toStringAsFixed(4)}, ${city.lng.toStringAsFixed(4)}',
              ),

              if (city.state.isNotEmpty)
                _DetailRow(
                  icon: Icons.map,
                  label: 'State',
                  value: city.state,
                ),

              _DetailRow(
                icon: Icons.flag,
                label: 'Country',
                value: '${city.country} (${city.iso2})',
              ),

              if (city.population > 0)
                _DetailRow(
                  icon: Icons.people,
                  label: 'Population',
                  value: _formatPopulation(city.population),
                ),
            ],
          ),
        ),
      ),
    );
  }

  String _formatPopulation(int population) {
    if (population >= 1000000) {
      return '${(population / 1000000).toStringAsFixed(1)}M';
    } else if (population >= 1000) {
      return '${(population / 1000).toStringAsFixed(1)}K';
    }
    return population.toString();
  }
}

class _DetailRow extends StatelessWidget {
  final IconData icon;
  final String label;
  final String value;

  const _DetailRow({
    required this.icon,
    required this.label,
    required this.value,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        children: [
          Icon(icon, size: 20, color: Colors.grey.shade600),
          const SizedBox(width: 12),
          Text(
            '$label: ',
            style: TextStyle(
              color: Colors.grey.shade600,
              fontWeight: FontWeight.w500,
            ),
          ),
          Expanded(
            child: Text(
              value,
              style: const TextStyle(fontWeight: FontWeight.w500),
            ),
          ),
        ],
      ),
    );
  }
}
