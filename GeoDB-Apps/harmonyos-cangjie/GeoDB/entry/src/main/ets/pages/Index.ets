// Enhanced GeoDB Search Interface with multiple search modes
// Based on tvOS version
import { GeoDatabase, CityResult } from '../model/GeoDatabase'

enum SearchMode {
  Smart,
  Cities,
  States,
  Countries,
  Nearest,
  Radius
}

@Entry
@Component
struct Index {
  @State database: GeoDatabase = new GeoDatabase()
  @State dbInitialized: boolean = false
  @State searchText: string = ''
  @State results: CityResult[] = []
  @State isSearching: boolean = false
  @State searchMode: SearchMode = SearchMode.Smart
  @State hasSearched: boolean = false

  // Spatial search parameters
  @State spatialLat: string = '52.52'    // Berlin default
  @State spatialLng: string = '13.405'
  @State nearestCount: string = '10'
  @State radiusKm: string = '50'

  aboutToAppear() {
    this.dbInitialized = this.database.isInitialized()
    console.info(`DB initialized: ${this.dbInitialized}`)
  }

  build() {
    Navigation() {
      Column() {
        if (!this.dbInitialized) {
          // Loading state
          Column({ space: 12 }) {
            LoadingProgress()
              .width(48)
              .height(48)
            Text('Loading database...')
              .fontSize(14)
              .fontColor($r('sys.color.ohos_id_color_text_secondary'))
          }
          .justifyContent(FlexAlign.Center)
          .width('100%')
          .height('100%')
        } else {
          Column({ space: 0 }) {
            // Search mode tabs
            this.searchModeTabs()

            // Search input area
            if (this.searchMode === SearchMode.Nearest || this.searchMode === SearchMode.Radius) {
              this.spatialParametersView()
            } else {
              this.searchFieldView()
            }

            // Results or startup view
            if (!this.hasSearched && this.results.length === 0) {
              this.startupView()
            } else if (this.isSearching) {
              Row() {
                LoadingProgress()
                  .width(24)
                  .height(24)
                Text('Searching...')
                  .fontSize(14)
                  .margin({ left: 8 })
              }
              .padding(16)
            } else if (this.results.length === 0 && this.hasSearched) {
              Text('No results found')
                .fontSize(14)
                .fontColor($r('sys.color.ohos_id_color_text_secondary'))
                .padding(16)
            } else {
              this.resultsListView()
            }

            // Stats footer
            if (this.database.getStats() !== null && !this.hasSearched) {
              this.statsFooter()
            }
          }
          .width('100%')
          .height('100%')
        }
      }
      .height('100%')
      .width('100%')
      .backgroundColor($r('sys.color.ohos_id_color_sub_background'))
    }
    .title('GeoDB')
    .titleMode(NavigationTitleMode.Mini)
  }

  // Search mode tabs
  @Builder searchModeTabs() {
    Row({ space: 8 }) {
      this.searchModeButton('Smart', SearchMode.Smart, 'sparkle')
      this.searchModeButton('Cities', SearchMode.Cities, 'building')
      this.searchModeButton('States', SearchMode.States, 'map')
      this.searchModeButton('Countries', SearchMode.Countries, 'flag')
      this.searchModeButton('Nearest', SearchMode.Nearest, 'location')
      this.searchModeButton('Radius', SearchMode.Radius, 'circle')
    }
    .width('100%')
    .padding({ left: 12, right: 12, top: 12, bottom: 8 })
  }

  @Builder searchModeButton(label: string, mode: SearchMode, icon: string) {
    Button() {
      Column({ space: 2 }) {
        Text(label)
          .fontSize(11)
          .fontColor(this.searchMode === mode ? Color.White : $r('sys.color.ohos_id_color_text_primary'))
      }
      .padding({ left: 8, right: 8, top: 6, bottom: 6 })
    }
    .backgroundColor(this.searchMode === mode ? $r('sys.color.ohos_id_color_emphasize') : $r('sys.color.ohos_id_color_button_normal'))
    .layoutWeight(1)
    .onClick(() => {
      this.searchMode = mode
      this.results = []
      this.hasSearched = false
      this.searchText = ''
    })
  }

  // Search field for text-based searches
  @Builder searchFieldView() {
    Search({ placeholder: `Search ${this.getSearchModePlaceholder()}...`, value: this.searchText })
      .searchButton('Search')
      .width('90%')
      .height(48)
      .margin({ top: 8, bottom: 8 })
      .onChange((value: string) => {
        this.searchText = value
        if (value.length > 0) {
          this.performTextSearch()
        } else {
          this.results = []
          this.hasSearched = false
        }
      })
      .onSubmit((value: string) => {
        this.performTextSearch()
      })
  }

  // Spatial parameters for Nearest/Radius
  @Builder spatialParametersView() {
    Column({ space: 12 }) {
      Row({ space: 12 }) {
        Column({ space: 4 }) {
          Text('Latitude')
            .fontSize(12)
            .fontColor($r('sys.color.ohos_id_color_text_secondary'))
          TextInput({ placeholder: '52.52', text: this.spatialLat })
            .type(InputType.Normal)  // Use Normal to allow decimal point
            .fontSize(14)
            .height(40)
            .onChange((value: string) => {
              this.spatialLat = value
            })
        }
        .layoutWeight(1)

        Column({ space: 4 }) {
          Text('Longitude')
            .fontSize(12)
            .fontColor($r('sys.color.ohos_id_color_text_secondary'))
          TextInput({ placeholder: '13.405', text: this.spatialLng })
            .type(InputType.Normal)  // Use Normal to allow decimal point and negative
            .fontSize(14)
            .height(40)
            .onChange((value: string) => {
              this.spatialLng = value
            })
        }
        .layoutWeight(1)

        if (this.searchMode === SearchMode.Nearest) {
          Column({ space: 4 }) {
            Text('Count')
              .fontSize(12)
              .fontColor($r('sys.color.ohos_id_color_text_secondary'))
            TextInput({ placeholder: '10', text: this.nearestCount })
              .type(InputType.Number)
              .fontSize(14)
              .height(40)
              .width(60)
              .onChange((value: string) => {
                this.nearestCount = value
              })
          }
        }

        if (this.searchMode === SearchMode.Radius) {
          Column({ space: 4 }) {
            Text('Radius (km)')
              .fontSize(12)
              .fontColor($r('sys.color.ohos_id_color_text_secondary'))
            TextInput({ placeholder: '50', text: this.radiusKm })
              .type(InputType.Number)
              .fontSize(14)
              .height(40)
              .width(80)
              .onChange((value: string) => {
                this.radiusKm = value
              })
          }
        }
      }
      .width('90%')

      Button('Search')
        .width('90%')
        .onClick(() => {
          this.performSpatialSearch()
        })
    }
    .width('100%')
    .padding({ top: 8, bottom: 8 })
  }

  // Startup view with stats
  @Builder startupView() {
    Column({ space: 20 }) {
      // Logo and title
      Row({ space: 12 }) {
        Text('🌍')
          .fontSize(48)
        Column({ space: 4 }) {
          Text('GeoDB')
            .fontSize(32)
            .fontWeight(FontWeight.Bold)
          Text('Geographic Database')
            .fontSize(14)
            .fontColor($r('sys.color.ohos_id_color_text_secondary'))
        }
        .alignItems(HorizontalAlign.Start)
      }

      // Stats cards
      if (this.database.getStats() !== null) {
        Row({ space: 16 }) {
          this.statCard('Countries', this.database.getStats()!.countries.toString(), '🌐', Color.Blue)
          this.statCard('States', this.database.getStats()!.states.toString(), '🗺️', Color.Green)
          this.statCard('Cities', this.database.getStats()!.cities.toString(), '🏙️', Color.Orange)
        }
        .width('90%')
      }

      Text('Select a search mode and start searching')
        .fontSize(12)
        .fontColor($r('sys.color.ohos_id_color_text_secondary'))
    }
    .justifyContent(FlexAlign.Center)
    .layoutWeight(1)
  }

  @Builder statCard(title: string, value: string, icon: string, color: ResourceColor) {
    Column({ space: 8 }) {
      Text(icon)
        .fontSize(32)
      Text(value)
        .fontSize(20)
        .fontWeight(FontWeight.Bold)
        .fontColor(color)
      Text(title)
        .fontSize(11)
        .fontColor($r('sys.color.ohos_id_color_text_secondary'))
    }
    .padding(16)
    .backgroundColor($r('sys.color.ohos_id_color_background'))
    .borderRadius(12)
    .layoutWeight(1)
  }

  // Results list
  @Builder resultsListView() {
    Column({ space: 0 }) {
      Text(`Results (${this.results.length})`)
        .fontSize(14)
        .fontWeight(FontWeight.Medium)
        .padding({ left: 16, right: 16, top: 8, bottom: 8 })
        .width('100%')

      List({ space: 0 }) {
        ForEach(this.results.slice(0, 50), (city: CityResult, index: number) => {
          ListItem() {
            Row({ space: 12 }) {
              Text(GeoDatabase.countryFlag(city.iso2))
                .fontSize(28)

              Column({ space: 4 }) {
                Text(city.name)
                  .fontSize(16)
                  .fontWeight(FontWeight.Medium)
                  .maxLines(1)
                  .textOverflow({ overflow: TextOverflow.Ellipsis })

                Text(`${city.state ? city.state + ', ' : ''}${city.country}`)
                  .fontSize(13)
                  .fontColor($r('sys.color.ohos_id_color_text_secondary'))
                  .maxLines(1)

                Row({ space: 12 }) {
                  if (city.population > 0) {
                    Text(`Pop: ${this.formatPopulation(city.population)}`)
                      .fontSize(11)
                      .fontColor($r('sys.color.ohos_id_color_text_tertiary'))
                  }
                  if (city.distanceKm) {
                    Text(`${city.distanceKm.toFixed(1)} km`)
                      .fontSize(11)
                      .fontColor(Color.Blue)
                  }
                }
              }
              .alignItems(HorizontalAlign.Start)
              .layoutWeight(1)
            }
            .width('100%')
            .padding({ left: 16, right: 16, top: 12, bottom: 12 })
            .backgroundColor($r('sys.color.ohos_id_color_background'))
          }
          .onClick(() => {
            this.showCityDetails(city)
          })
        }, (city: CityResult, index: number) => `${city.name}-${city.iso2}-${index}`)
      }
      .layoutWeight(1)
      .divider({
        strokeWidth: 1,
        color: $r('sys.color.ohos_id_color_list_separator'),
        startMargin: 60,
        endMargin: 16
      })
    }
    .layoutWeight(1)
  }

  @Builder statsFooter() {
    Text(`Database: ${this.database.getStats()!.cities} cities`)
      .fontSize(12)
      .fontColor($r('sys.color.ohos_id_color_text_tertiary'))
      .padding(12)
      .width('100%')
      .textAlign(TextAlign.Center)
  }

  // Helper methods
  private getSearchModePlaceholder(): string {
    switch (this.searchMode) {
      case SearchMode.Smart:
        return 'cities, states, countries'
      case SearchMode.Cities:
        return 'cities'
      case SearchMode.States:
        return 'states'
      case SearchMode.Countries:
        return 'countries'
      default:
        return 'search'
    }
  }

  private performTextSearch() {
    if (this.searchText.length === 0) {
      this.results = []
      this.hasSearched = false
      return
    }

    this.isSearching = true
    this.hasSearched = true

    setTimeout(() => {
      switch (this.searchMode) {
        case SearchMode.Smart:
          this.results = this.database.smartSearch(this.searchText)
          break
        case SearchMode.Cities:
          this.results = this.database.smartSearch(this.searchText) // In real version, use findCities
          break
        case SearchMode.States:
          this.results = this.database.smartSearch(this.searchText) // In real version, use findStates
          break
        case SearchMode.Countries:
          this.results = this.database.smartSearch(this.searchText) // In real version, use findCountries
          break
      }
      this.isSearching = false
    }, 100)
  }

  private performSpatialSearch() {
    this.isSearching = true
    this.hasSearched = true

    setTimeout(() => {
      const lat = parseFloat(this.spatialLat)
      const lng = parseFloat(this.spatialLng)

      if (isNaN(lat) || isNaN(lng)) {
        this.results = []
        this.isSearching = false
        return
      }

      if (this.searchMode === SearchMode.Nearest) {
        const count = parseInt(this.nearestCount) || 10
        this.results = this.database.findNearest(lat, lng, count)
      } else if (this.searchMode === SearchMode.Radius) {
        const radius = parseFloat(this.radiusKm) || 50
        this.results = this.database.findInRadius(lat, lng, radius)
      }

      this.isSearching = false
    }, 100)
  }

  private showCityDetails(city: CityResult) {
    AlertDialog.show({
      title: `${GeoDatabase.countryFlag(city.iso2)} ${city.name}`,
      message: `Country: ${city.country}\n` +
               `State: ${city.state || 'N/A'}\n` +
               `ISO2: ${city.iso2}\n` +
               `Coordinates: ${city.lat.toFixed(4)}, ${city.lng.toFixed(4)}\n` +
               `Population: ${city.population > 0 ? this.formatPopulation(city.population) : 'Unknown'}` +
               (city.distanceKm ? `\nDistance: ${city.distanceKm.toFixed(1)} km` : ''),
      primaryButton: {
        value: 'Use Coordinates',
        action: () => {
          // Set this city's coordinates for spatial search
          this.spatialLat = city.lat.toString()
          this.spatialLng = city.lng.toString()
          this.searchMode = SearchMode.Nearest
          this.hasSearched = false
          console.info(`Set coordinates to ${city.name}: ${city.lat}, ${city.lng}`)
        }
      },
      secondaryButton: {
        value: 'Close',
        action: () => {
          console.info('Dialog closed')
        }
      }
    })
  }

  private formatPopulation(pop: number): string {
    if (pop >= 1000000) {
      return `${(pop / 1000000).toFixed(1)}M`
    } else if (pop >= 1000) {
      return `${(pop / 1000).toFixed(1)}K`
    }
    return pop.toString()
  }
}
