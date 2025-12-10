// Mock GeoDatabase for Previewer Testing
// Use this when you don't have emulator/device access

export interface CityResult {
  name: string
  state: string
  country: string
  iso2: string
  lat: number
  lng: number
  population: number
  distanceKm?: number
}

export interface DbStats {
  countries: number
  states: number
  cities: number
}

@Observed
export class GeoDatabase {
  @Track initialized: boolean = false
  @Track statsCache: DbStats | null = null

  constructor() {
    this.initialize()
  }

  private initialize() {
    // Mock initialization - instant!
    // Initialize immediately for testing
    this.statsCache = {
      countries: 250,
      states: 5000,
      cities: 148249
    }
    this.initialized = true
    console.info('[MOCK] GeoDB initialized successfully')
  }

  isInitialized(): boolean {
    return this.initialized
  }

  getStats(): DbStats | null {
    return this.statsCache
  }

  smartSearch(query: string): CityResult[] {
    // Return mock data
    console.info(`[MOCK] Searching for: ${query}`)

    const q = query.toLowerCase()

    // Check for specific cities
    if (q.includes('berlin')) {
      return [
        {
          name: 'Berlin',
          state: 'Berlin',
          country: 'Germany',
          iso2: 'DE',
          lat: 52.5200,
          lng: 13.4050,
          population: 3644826
        },
        {
          name: 'Berliner Tor',
          state: 'Hamburg',
          country: 'Germany',
          iso2: 'DE',
          lat: 53.5533,
          lng: 10.0247,
          population: 12500
        },
        {
          name: 'Berlin Heights',
          state: 'Ohio',
          country: 'United States',
          iso2: 'US',
          lat: 41.3317,
          lng: -82.4879,
          population: 685
        }
      ]
    } else if (q.includes('london')) {
      return [
        {
          name: 'London',
          state: 'England',
          country: 'United Kingdom',
          iso2: 'GB',
          lat: 51.5074,
          lng: -0.1278,
          population: 8982000
        },
        {
          name: 'London',
          state: 'Ontario',
          country: 'Canada',
          iso2: 'CA',
          lat: 42.9849,
          lng: -81.2453,
          population: 383822
        },
        {
          name: 'Londonderry',
          state: 'New Hampshire',
          country: 'United States',
          iso2: 'US',
          lat: 42.8653,
          lng: -71.3739,
          population: 24129
        }
      ]
    } else if (q.includes('tokyo')) {
      return [
        {
          name: 'Tokyo',
          state: 'Tokyo',
          country: 'Japan',
          iso2: 'JP',
          lat: 35.6762,
          lng: 139.6503,
          population: 13960000
        }
      ]
    } else if (q.includes('paris')) {
      return [
        {
          name: 'Paris',
          state: 'Île-de-France',
          country: 'France',
          iso2: 'FR',
          lat: 48.8566,
          lng: 2.3522,
          population: 2165423
        },
        {
          name: 'Paris',
          state: 'Texas',
          country: 'United States',
          iso2: 'US',
          lat: 33.6609,
          lng: -95.5555,
          population: 25171
        }
      ]
    } else if (q.includes('new york') || q.includes('newyork')) {
      return [
        {
          name: 'New York',
          state: 'New York',
          country: 'United States',
          iso2: 'US',
          lat: 40.7128,
          lng: -74.0060,
          population: 8336817
        }
      ]
    } else if (q.includes('shanghai')) {
      return [
        {
          name: 'Shanghai',
          state: 'Shanghai',
          country: 'China',
          iso2: 'CN',
          lat: 31.2304,
          lng: 121.4737,
          population: 24870895
        }
      ]
    } else if (q.includes('munich') || q.includes('münchen')) {
      return [
        {
          name: 'Munich',
          state: 'Bavaria',
          country: 'Germany',
          iso2: 'DE',
          lat: 48.1351,
          lng: 11.5820,
          population: 1471508
        }
      ]
    } else if (q.includes('amsterdam')) {
      return [
        {
          name: 'Amsterdam',
          state: 'North Holland',
          country: 'Netherlands',
          iso2: 'NL',
          lat: 52.3676,
          lng: 4.9041,
          population: 872680
        }
      ]
    } else if (q.includes('sydney')) {
      return [
        {
          name: 'Sydney',
          state: 'New South Wales',
          country: 'Australia',
          iso2: 'AU',
          lat: -33.8688,
          lng: 151.2093,
          population: 5312163
        }
      ]
    } else if (q.includes('dubai')) {
      return [
        {
          name: 'Dubai',
          state: 'Dubai',
          country: 'United Arab Emirates',
          iso2: 'AE',
          lat: 25.2048,
          lng: 55.2708,
          population: 3411200
        }
      ]
    } else if (q.includes('singapore')) {
      return [
        {
          name: 'Singapore',
          state: 'Singapore',
          country: 'Singapore',
          iso2: 'SG',
          lat: 1.3521,
          lng: 103.8198,
          population: 5685807
        }
      ]
    } else if (q) {
      // Generic search - return some sample cities
      return [
        {
          name: 'Sample City A',
          state: 'Region A',
          country: 'Country A',
          iso2: 'AA',
          lat: 10.0,
          lng: 20.0,
          population: 500000
        },
        {
          name: 'Sample City B',
          state: 'Region B',
          country: 'Country B',
          iso2: 'BB',
          lat: -10.0,
          lng: -20.0,
          population: 250000
        },
        {
          name: 'Sample City C',
          state: 'Region C',
          country: 'Country C',
          iso2: 'CC',
          lat: 50.0,
          lng: 100.0,
          population: 150000
        }
      ]
    }

    return []
  }

  findNearest(lat: number, lng: number, count: number): CityResult[] {
    console.info(`[MOCK] Finding ${count} nearest to ${lat}, ${lng}`)

    // Return mock nearest cities
    return [
      {
        name: 'Nearby City 1',
        state: 'State 1',
        country: 'Country 1',
        iso2: 'C1',
        lat: lat + 0.1,
        lng: lng + 0.1,
        population: 50000,
        distanceKm: 5.2
      },
      {
        name: 'Nearby City 2',
        state: 'State 2',
        country: 'Country 2',
        iso2: 'C2',
        lat: lat + 0.2,
        lng: lng - 0.1,
        population: 30000,
        distanceKm: 12.8
      }
    ]
  }

  static countryFlag(iso2: string): string {
    const base = 127397
    let flag = ''
    for (let i = 0; i < iso2.length; i++) {
      const code = iso2.toUpperCase().charCodeAt(i)
      flag += String.fromCodePoint(base + code)
    }
    return flag
  }
}
