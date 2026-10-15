// The board's globe for the Apple Watch (no Metal for apps): geodb-fw-core's software renderer drawing into
// a pixel buffer, crates/geodb-ffi/src/watch_globe.rs. Main thread; lengths in pixels of the buffer.
// Only on watchOS (iOS, tvOS and macOS have the Metal globe, geodb_globe.h).

#ifndef GEODB_WATCH_GLOBE_H
#define GEODB_WATCH_GLOBE_H

#include <stdbool.h>
#include <stdint.h>
#include <TargetConditionals.h>

#if TARGET_OS_WATCH

#ifdef __cplusplus
extern "C" {
#endif

typedef struct GeodbWatchGlobe GeodbWatchGlobe;

/// Makes the globe for a buffer of width x height pixels (builds the earth from the board's image).
/// NULL on failure: geodb_watch_globe_last_error() says why.
GeodbWatchGlobe *geodb_watch_globe_new(uint32_t width, uint32_t height);
const char *geodb_watch_globe_last_error(void);
void geodb_watch_globe_free(GeodbWatchGlobe *globe);
void geodb_watch_globe_resize(GeodbWatchGlobe *globe, uint32_t width, uint32_t height);

/// Draws a frame dt_s seconds after the last: width x height opaque RGBA pixels, valid until the next call.
const uint8_t *geodb_watch_globe_frame(GeodbWatchGlobe *globe, double dt_s);

void geodb_watch_globe_drag(GeodbWatchGlobe *globe, float dx, float dy);
/// Divides the zoom by factor (below 1 zooms in).
void geodb_watch_globe_zoom(GeodbWatchGlobe *globe, double factor);
/// Turns the globe by itself, deg_per_s degrees of longitude a second (0 stops it).
void geodb_watch_globe_set_spin(GeodbWatchGlobe *globe, double deg_per_s);
/// A finger is down on the globe: the spin waits.
void geodb_watch_globe_set_pressed(GeodbWatchGlobe *globe, bool pressed);
/// Looks at (lat, lon) from closer; stops the spin.
void geodb_watch_globe_fly_to(GeodbWatchGlobe *globe, double lat, double lon);
/// The city nearest the centre, "Name, CC · 12 km" (valid until the next frame).
const char *geodb_watch_globe_nearest(GeodbWatchGlobe *globe);

#ifdef __cplusplus
}
#endif

#endif  // TARGET_OS_WATCH
#endif
