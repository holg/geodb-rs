// GeoDB Search Interface
// TOGGLE: Use GeoDatabase_Mock for Previewer, GeoDatabase for real device/emulator
// import { GeoDatabase, CityResult } from '../model/GeoDatabase_Mock'  // Mock for Previewer
import { GeoDatabase, CityResult } from '../model/GeoDatabase'  // Real for device

@Entry
@Component
struct Index {
  @State database: GeoDatabase = new GeoDatabase()
  @State dbInitialized: boolean = false
  @State searchText: string = ''
  @State results: CityResult[] = []
  @State isSearching: boolean = false

  aboutToAppear() {
    // Force check initialization
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
          // Main search interface
          Column({ space: 0 }) {
            // Search bar
            Search({ placeholder: 'Search cities...', value: this.searchText })
              .searchButton('Search')
              .width('90%')
              .height(56)
              .backgroundColor($r('sys.color.ohos_id_color_background'))
              .placeholderColor(Color.Grey)
              .placeholderFont({ size: 16, weight: FontWeight.Normal })
              .textFont({ size: 16, weight: FontWeight.Normal })
              .margin({ top: 16, bottom: 8 })
              .onChange((value: string) => {
                this.searchText = value
              })
              .onSubmit((value: string) => {
                this.performSearch()
              })

            // Results list
            if (this.isSearching) {
              Row() {
                LoadingProgress()
                  .width(24)
                  .height(24)
                Text('Searching...')
                  .fontSize(14)
                  .margin({ left: 8 })
              }
              .padding(16)
            } else if (this.results.length === 0 && this.searchText.length > 0) {
              Text('No results found')
                .fontSize(14)
                .fontColor($r('sys.color.ohos_id_color_text_secondary'))
                .padding(16)
            } else if (this.results.length > 0) {
              List({ space: 0 }) {
                ForEach(this.results.slice(0, 20), (city: CityResult, index: number) => {
                  ListItem() {
                    Row({ space: 12 }) {
                      // Country flag
                      Text(GeoDatabase.countryFlag(city.iso2))
                        .fontSize(28)

                      // City info
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
                          .textOverflow({ overflow: TextOverflow.Ellipsis })

                        if (city.population > 0) {
                          Text(`Pop: ${this.formatPopulation(city.population)}`)
                            .fontSize(11)
                            .fontColor($r('sys.color.ohos_id_color_text_tertiary'))
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

            // Stats footer
            if (this.database.getStats() !== null) {
              Text(`Database: ${this.database.getStats()!.cities} cities`)
                .fontSize(12)
                .fontColor($r('sys.color.ohos_id_color_text_tertiary'))
                .padding(12)
                .width('100%')
                .textAlign(TextAlign.Center)
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

  private performSearch() {
    if (this.searchText.length === 0) {
      this.results = []
      return
    }

    this.isSearching = true

    // Simulate async search (in real app, use async/await)
    setTimeout(() => {
      this.results = this.database.smartSearch(this.searchText)
      this.isSearching = false
    }, 100)
  }

  private showCityDetails(city: CityResult) {
    // Show city details in alert dialog
    AlertDialog.show({
      title: `${GeoDatabase.countryFlag(city.iso2)} ${city.name}`,
      message: `Country: ${city.country}\n` +
               `State: ${city.state || 'N/A'}\n` +
               `ISO2: ${city.iso2}\n` +
               `Coordinates: ${city.lat.toFixed(4)}, ${city.lng.toFixed(4)}\n` +
               `Population: ${city.population > 0 ? this.formatPopulation(city.population) : 'Unknown'}` +
               (city.distanceKm ? `\nDistance: ${city.distanceKm.toFixed(1)} km` : ''),
      primaryButton: {
        value: 'OK',
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
