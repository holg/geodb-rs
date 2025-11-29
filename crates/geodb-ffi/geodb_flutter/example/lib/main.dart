import 'package:flutter/material.dart';
import 'package:geodb_flutter/geodb_flutter.dart';

void main() {
  runApp(const MyApp());
}

class MyApp extends StatefulWidget {
  const MyApp({super.key});

  @override
  State<MyApp> createState() => _MyAppState();
}

class _MyAppState extends State<MyApp> {
  final _geodbFlutterPlugin = GeodbFlutter();
  bool _isInitialized = false;
  String _statusMessage = 'Not initialized';
  List<CityResult> _searchResults = [];
  DbStats? _stats;
  bool _isLoading = false;

  final _searchController = TextEditingController();

  @override
  void initState() {
    super.initState();
    _initializeGeoDB();
  }

  Future<void> _initializeGeoDB() async {
    setState(() {
      _isLoading = true;
      _statusMessage = 'Initializing GeoDB...';
    });

    try {
      await _geodbFlutterPlugin.initialize();
      final stats = await _geodbFlutterPlugin.getStats();

      setState(() {
        _isInitialized = true;
        _stats = stats;
        _statusMessage = 'GeoDB initialized successfully!\n'
            'Countries: ${stats.countries}, States: ${stats.states}, Cities: ${stats.cities}';
        _isLoading = false;
      });
    } catch (e) {
      setState(() {
        _statusMessage = 'Failed to initialize: $e';
        _isLoading = false;
      });
    }
  }

  Future<void> _performSmartSearch() async {
    if (!_isInitialized || _searchController.text.isEmpty) return;

    setState(() {
      _isLoading = true;
      _searchResults = [];
    });

    try {
      final results = await _geodbFlutterPlugin.smartSearch(_searchController.text);
      setState(() {
        _searchResults = results;
        _isLoading = false;
      });
    } catch (e) {
      setState(() {
        _statusMessage = 'Search failed: $e';
        _isLoading = false;
      });
    }
  }

  Future<void> _findNearestCities() async {
    if (!_isInitialized) return;

    setState(() {
      _isLoading = true;
      _searchResults = [];
    });

    try {
      // Berlin coordinates
      final results = await _geodbFlutterPlugin.findNearest(
        lat: 52.52,
        lng: 13.405,
        count: 10,
      );
      setState(() {
        _searchResults = results;
        _statusMessage = 'Found ${results.length} nearest cities to Berlin';
        _isLoading = false;
      });
    } catch (e) {
      setState(() {
        _statusMessage = 'Failed to find nearest: $e';
        _isLoading = false;
      });
    }
  }

  Future<void> _findInRadius() async {
    if (!_isInitialized) return;

    setState(() {
      _isLoading = true;
      _searchResults = [];
    });

    try {
      // Berlin coordinates, 50km radius
      final results = await _geodbFlutterPlugin.findInRadius(
        lat: 52.52,
        lng: 13.405,
        radiusKm: 50.0,
      );
      setState(() {
        _searchResults = results;
        _statusMessage = 'Found ${results.length} cities within 50km of Berlin';
        _isLoading = false;
      });
    } catch (e) {
      setState(() {
        _statusMessage = 'Failed to find in radius: $e';
        _isLoading = false;
      });
    }
  }

  Future<void> _findCountries() async {
    if (!_isInitialized) return;

    setState(() {
      _isLoading = true;
      _searchResults = [];
    });

    try {
      final results = await _geodbFlutterPlugin.findCountriesBySubstring('United');
      setState(() {
        _searchResults = results;
        _statusMessage = 'Found ${results.length} countries with "United"';
        _isLoading = false;
      });
    } catch (e) {
      setState(() {
        _statusMessage = 'Failed to find countries: $e';
        _isLoading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(
          title: const Text('GeoDB Flutter Example'),
          backgroundColor: Colors.blue,
        ),
        body: Padding(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Status Card
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(16.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Icon(
                            _isInitialized ? Icons.check_circle : Icons.pending,
                            color: _isInitialized ? Colors.green : Colors.orange,
                          ),
                          const SizedBox(width: 8),
                          const Text(
                            'Status',
                            style: TextStyle(
                              fontSize: 18,
                              fontWeight: FontWeight.bold,
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 8),
                      Text(_statusMessage),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 16),

              // Search Field
              TextField(
                controller: _searchController,
                decoration: InputDecoration(
                  labelText: 'Search',
                  hintText: 'Enter city, state, or country name',
                  border: const OutlineInputBorder(),
                  suffixIcon: IconButton(
                    icon: const Icon(Icons.search),
                    onPressed: _isInitialized && !_isLoading ? _performSmartSearch : null,
                  ),
                ),
                enabled: _isInitialized && !_isLoading,
                onSubmitted: (_) => _performSmartSearch(),
              ),
              const SizedBox(height: 16),

              // Action Buttons
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  ElevatedButton.icon(
                    onPressed: _isInitialized && !_isLoading ? _performSmartSearch : null,
                    icon: const Icon(Icons.search),
                    label: const Text('Smart Search'),
                  ),
                  ElevatedButton.icon(
                    onPressed: _isInitialized && !_isLoading ? _findNearestCities : null,
                    icon: const Icon(Icons.near_me),
                    label: const Text('Nearest to Berlin'),
                  ),
                  ElevatedButton.icon(
                    onPressed: _isInitialized && !_isLoading ? _findInRadius : null,
                    icon: const Icon(Icons.location_on),
                    label: const Text('50km Radius'),
                  ),
                  ElevatedButton.icon(
                    onPressed: _isInitialized && !_isLoading ? _findCountries : null,
                    icon: const Icon(Icons.public),
                    label: const Text('Countries'),
                  ),
                ],
              ),
              const SizedBox(height: 16),

              // Results
              Expanded(
                child: _isLoading
                    ? const Center(child: CircularProgressIndicator())
                    : _searchResults.isEmpty
                        ? Center(
                            child: Text(
                              _isInitialized
                                  ? 'No results. Try searching!'
                                  : 'Initializing...',
                              style: TextStyle(color: Colors.grey[600]),
                            ),
                          )
                        : ListView.builder(
                            itemCount: _searchResults.length,
                            itemBuilder: (context, index) {
                              final result = _searchResults[index];
                              return Card(
                                margin: const EdgeInsets.only(bottom: 8),
                                child: ListTile(
                                  leading: CircleAvatar(
                                    backgroundColor: Colors.blue,
                                    child: Text(
                                      result.iso2,
                                      style: const TextStyle(
                                        color: Colors.white,
                                        fontSize: 12,
                                      ),
                                    ),
                                  ),
                                  title: Text(
                                    result.name,
                                    style: const TextStyle(fontWeight: FontWeight.bold),
                                  ),
                                  subtitle: Column(
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      if (result.state.isNotEmpty)
                                        Text('${result.state}, ${result.country}')
                                      else
                                        Text(result.country),
                                      Text(
                                        'Lat: ${result.lat.toStringAsFixed(2)}, '
                                        'Lng: ${result.lng.toStringAsFixed(2)}',
                                        style: TextStyle(
                                          fontSize: 11,
                                          color: Colors.grey[600],
                                        ),
                                      ),
                                      if (result.distanceKm != null)
                                        Text(
                                          'Distance: ${result.distanceKm!.toStringAsFixed(1)} km',
                                          style: const TextStyle(
                                            fontSize: 11,
                                            color: Colors.blue,
                                          ),
                                        ),
                                    ],
                                  ),
                                  trailing: result.population > 0
                                      ? Text(
                                          '${(result.population / 1000).toStringAsFixed(0)}k',
                                          style: TextStyle(color: Colors.grey[600]),
                                        )
                                      : null,
                                ),
                              );
                            },
                          ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }
}
