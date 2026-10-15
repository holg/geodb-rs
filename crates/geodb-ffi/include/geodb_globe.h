// The 3D globe of geodb-globe (wgpu on Metal) for the Apple apps: crates/geodb-ffi/src/globe.rs.
//
// Everything runs on the main thread: geodb_globe_frame once per display refresh, the gestures in
// between. Lengths are in points; the CAMetalLayer's pixels are points times `scale`.
// Not available on watchOS.

#ifndef GEODB_GLOBE_H
#define GEODB_GLOBE_H

#include <stdbool.h>
#include <stdint.h>
#include <TargetConditionals.h>

#if !TARGET_OS_WATCH

#ifdef __cplusplus
extern "C" {
#endif

typedef struct GeodbGlobe GeodbGlobe;

/// One of the top-ranked cities around the centre of view, for the app's label overlay.
typedef struct {
    const char *name;    // owned by the globe, valid until the next query
    const char *detail;  // "State, Country"
    double lat;
    double lon;
    float x;             // points from the view's top-left corner
    float y;
    bool visible;        // false behind the horizon
    bool selected;
} GeodbGlobeLabel;

typedef struct {
    double lat;
    double lon;
    double altitude_km;
    double radius_km;    // of the last query
    uint32_t cities;     // cities within it
    bool moving;
    double spin_deg_s;   // the globe turns by itself (0: it does not)
} GeodbGlobeState;

/// Creates the globe on a CAMetalLayer of width x height pixels; bakes the earth (a few hundred ms).
/// NULL on failure: geodb_globe_last_error() says why.
GeodbGlobe *geodb_globe_new(void *metal_layer, uint32_t width, uint32_t height, float scale);
const char *geodb_globe_last_error(void);
void geodb_globe_free(GeodbGlobe *globe);

void geodb_globe_resize(GeodbGlobe *globe, uint32_t width, uint32_t height, float scale);
/// Draws a frame, dt_s seconds after the last. True while the camera moves.
bool geodb_globe_frame(GeodbGlobe *globe, double dt_s);

void geodb_globe_drag(GeodbGlobe *globe, float dx, float dy);
/// Multiplies the altitude by factor (below 1 zooms in).
void geodb_globe_zoom(GeodbGlobe *globe, double factor);
/// The index of the label the tap picked, -1 for a point on the globe, -2 for the sky.
int32_t geodb_globe_tap(GeodbGlobe *globe, float x, float y);
void geodb_globe_fly_to(GeodbGlobe *globe, double lat, double lon, double altitude_km);
void geodb_globe_set_relief(GeodbGlobe *globe, bool shading, bool isohypses);
/// Turns the globe by itself, deg_per_s degrees of longitude a second (0 stops it). A flight stops it.
void geodb_globe_set_spin(GeodbGlobe *globe, double deg_per_s);
/// A finger or the mouse button is down on the globe: the spin waits.
void geodb_globe_set_pressed(GeodbGlobe *globe, bool pressed);

bool geodb_globe_state(GeodbGlobe *globe, GeodbGlobeState *out);
/// Fills up to max labels where the camera sees them now; returns how many.
uint32_t geodb_globe_labels(GeodbGlobe *globe, GeodbGlobeLabel *out, uint32_t max);

#ifdef __cplusplus
}
#endif

#endif  // !TARGET_OS_WATCH
#endif
