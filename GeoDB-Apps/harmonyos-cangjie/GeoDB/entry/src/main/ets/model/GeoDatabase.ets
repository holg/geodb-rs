// GeoDatabase.ets - ArkTS wrapper for native GeoDB

import geoDbNapi from 'libgeodb_napi.so'  // Module name matches nm_modname in C++

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

export class GeoDatabase {
  private initialized: boolean = false
  private statsCache: DbStats | null = null

  constructor() {
    this.initialize()
  }

  private initialize() {
    try {
      console.info('[GeoDB] Attempting to call geoDbNapi.init()')
      geoDbNapi.init()
      console.info('[GeoDB] Init successful, getting stats...')
      this.statsCache = geoDbNapi.getStats()
      console.info('[GeoDB] Stats retrieved:', JSON.stringify(this.statsCache))
      this.initialized = true
      console.info('[GeoDB] GeoDB initialized successfully')
    } catch (error) {
      console.error('[GeoDB] Failed to initialize GeoDB:', error)
      console.error('[GeoDB] Error details:', JSON.stringify(error))
      this.initialized = false
    }
  }

  isInitialized(): boolean {
    return this.initialized
  }

  getStats(): DbStats | null {
    if (!this.initialized) {
      return null
    }
    return this.statsCache
  }

  smartSearch(query: string): CityResult[] {
    if (!this.initialized || !query) {
      return []
    }

    try {
      return geoDbNapi.smartSearch(query)
    } catch (error) {
      console.error('Smart search failed:', error)
      return []
    }
  }

  findNearest(lat: number, lng: number, count: number): CityResult[] {
    if (!this.initialized) {
      return []
    }

    try {
      return geoDbNapi.findNearest(lat, lng, count)
    } catch (error) {
      console.error('Find nearest failed:', error)
      return []
    }
  }

  findInRadius(lat: number, lng: number, radiusKm: number): CityResult[] {
    if (!this.initialized) {
      return []
    }

    try {
      // Note: NAPI bridge doesn't have findInRadius yet, use findNearest as fallback
      console.warn('[GeoDB] findInRadius not implemented in NAPI, using findNearest')
      return geoDbNapi.findNearest(lat, lng, 50) // Return 50 nearest as approximation
    } catch (error) {
      console.error('Find in radius failed:', error)
      return []
    }
  }

  // Helper: Convert ISO2 code to emoji flag
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
