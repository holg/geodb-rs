// The board's globe on the Apple Watch, which gives apps no Metal: geodb-fw-core's software renderer (the code
// the STM32F769I-DISCO and the STM32MP135F-DK kiosk run) draws into a pixel buffer that is shown as an image.
// The C interface is crates/geodb-ffi/src/watch_globe.rs (include/geodb_watch_globe.h). Only on watchOS.

#if os(watchOS)
import Combine
import CoreGraphics
import SwiftUI
import WatchKit
#if canImport(GeodbFfi)
import GeodbFfi
#endif

@MainActor
public final class WatchGlobeController: ObservableObject {
    /// The globe turns by itself (as on the board); a flight to a city stops it.
    @Published public var spinning = true { didSet { applySpin() } }
    /// Why the globe could not start.
    @Published public private(set) var error: String?
    /// Degrees of longitude a second while spinning.
    public var spinSpeed = 12.0 { didSet { applySpin() } }
    /// The city nearest the centre, "Name, CC · 12 km", as of the last frame.
    public private(set) var nearest = ""

    /// The Rust globe (`GeodbWatchGlobe *`), made at the first frame (it needs the size).
    private var globe: OpaquePointer?
    private var size = (0, 0)
    private var pendingFlight: (Double, Double)?
    private var lastFrame: Date?

    public init() {}

    deinit {
        if let globe { geodb_watch_globe_free(globe) }
    }

    /// Looks at (latitude, longitude) from closer and stops the spin.
    public func fly(to latitude: Double, longitude: Double) {
        spinning = false
        guard let globe else {
            pendingFlight = (latitude, longitude)
            return
        }
        geodb_watch_globe_fly_to(globe, latitude, longitude)
    }

    func zoom(by factor: Double) {
        guard let globe else { return }
        geodb_watch_globe_zoom(globe, factor)
    }

    func drag(dx: CGFloat, dy: CGFloat) {
        guard let globe else { return }
        geodb_watch_globe_drag(globe, Float(dx), Float(dy))
    }

    func setPressed(_ down: Bool) {
        guard let globe else { return }
        geodb_watch_globe_set_pressed(globe, down)
    }

    /// Draws a frame of width x height pixels (making the globe at the first one).
    func frame(width: Int, height: Int, at now: Date) -> CGImage? {
        guard width > 0, height > 0 else { return nil }
        if globe == nil {
            guard error == nil else { return nil }
            guard let made = geodb_watch_globe_new(UInt32(width), UInt32(height)) else {
                let why = String(cString: geodb_watch_globe_last_error())
                DispatchQueue.main.async { self.error = why }
                return nil
            }
            globe = made
            size = (width, height)
            applySpin()
            if let (lat, lon) = pendingFlight {
                geodb_watch_globe_fly_to(made, lat, lon)
                pendingFlight = nil
            }
        } else if size != (width, height), let globe {
            geodb_watch_globe_resize(globe, UInt32(width), UInt32(height))
            size = (width, height)
        }
        guard let globe else { return nil }
        let dt = lastFrame.map { min(now.timeIntervalSince($0), 0.25) } ?? 0
        lastFrame = now
        guard let pixels = geodb_watch_globe_frame(globe, dt) else { return nil }
        if let name = geodb_watch_globe_nearest(globe) { nearest = String(cString: name) }
        let data = Data(bytes: pixels, count: width * height * 4) as CFData
        guard let provider = CGDataProvider(data: data) else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }

    private func applySpin() {
        guard let globe else { return }
        geodb_watch_globe_set_spin(globe, spinning ? spinSpeed : 0)
        lastFrame = nil
    }
}

/// The globe and the city nearest its centre. Drag to turn, the Digital Crown zooms, a tap starts or stops the
/// spin. Redrawn about 30 times a second while it spins (not in Always On), else when touched.
public struct WatchGlobeView: View {
    @ObservedObject private var controller: WatchGlobeController
    @Environment(\.isLuminanceReduced) private var dimmed
    @State private var crown = 0.0
    @State private var lastCrown = 0.0
    @State private var lastDrag = CGSize.zero
    /// Bumped by the gestures: a paused timeline draws again.
    @State private var touched = 0

    public init(controller: WatchGlobeController) {
        self.controller = controller
    }

    public var body: some View {
        GeometryReader { geo in
            TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: dimmed || !controller.spinning)) { context in
                let scale = WKInterfaceDevice.current().screenScale
                let image = controller.frame(
                    width: Int(geo.size.width * scale), height: Int(geo.size.height * scale),
                    at: context.date)
                ZStack(alignment: .bottom) {
                    if let image {
                        Image(decorative: image, scale: scale)
                            .frame(width: geo.size.width, height: geo.size.height)
                    } else if let error = controller.error {
                        Text(error).font(.footnote).foregroundStyle(.secondary)
                    } else {
                        ProgressView()
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                    if !controller.nearest.isEmpty {
                        Text(controller.nearest)
                            .font(.caption2)
                            .lineLimit(1)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 2)
                            .background(.black.opacity(0.55), in: Capsule())
                    }
                }
                .id(touched)
            }
        }
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 4)
                .onChanged { value in
                    if lastDrag == .zero { controller.setPressed(true) }
                    let scale = WKInterfaceDevice.current().screenScale
                    controller.drag(
                        dx: (value.translation.width - lastDrag.width) * scale,
                        dy: (value.translation.height - lastDrag.height) * scale)
                    lastDrag = value.translation
                    touched &+= 1
                }
                .onEnded { _ in
                    controller.setPressed(false)
                    lastDrag = .zero
                    touched &+= 1
                }
        )
        .onTapGesture {
            controller.spinning.toggle()
            touched &+= 1
        }
        .focusable()
        .digitalCrownRotation(
            $crown, from: -1000, through: 1000, by: 1, sensitivity: .medium,
            isContinuous: true, isHapticFeedbackEnabled: false)
        .onChange(of: crown) { value in
            controller.zoom(by: exp(-(value - lastCrown) * 0.04))
            lastCrown = value
            touched &+= 1
        }
    }
}
#endif
