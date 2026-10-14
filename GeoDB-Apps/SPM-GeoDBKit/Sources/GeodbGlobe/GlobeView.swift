// The 3D globe of geodb-globe (wgpu on Metal) as a SwiftUI view: `GlobeView` draws it on a CAMetalLayer,
// `GlobeController` drives it (fly to a city, relief, the labels of the top-ranked cities around the centre).
// The C interface is crates/geodb-ffi/src/globe.rs (include/geodb_globe.h). Not on watchOS.

#if !os(watchOS)
import Combine
import QuartzCore
import SwiftUI
#if canImport(GeodbFfi)
import GeodbFfi
#endif
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// A city label of the overlay, where the camera sees it now (points from the view's top-left).
public struct GlobeLabel: Identifiable, Equatable {
    public let id: Int
    public let name: String
    public let detail: String
    public let latitude: Double
    public let longitude: Double
    public let point: CGPoint
    public let selected: Bool
}

/// Where the camera is and what the last query around the centre found.
public struct GlobeStatus: Equatable {
    public var latitude = 0.0
    public var longitude = 0.0
    public var altitudeKm = 0.0
    public var radiusKm = 0.0
    public var cities = 0
    public var moving = false
    /// Degrees of longitude a second the globe turns by itself (0: it does not).
    public var spinDegreesPerSecond = 0.0
}

@MainActor
public final class GlobeController: ObservableObject {
    @Published public private(set) var labels: [GlobeLabel] = []
    @Published public private(set) var status = GlobeStatus()
    /// Why the globe could not start (no Metal device, ...).
    @Published public private(set) var error: String?
    @Published public var relief = true { didSet { applyRelief() } }
    @Published public var isohypses = false { didSet { applyRelief() } }
    /// The globe turns by itself (as the web demo's Spin); a flight to a city stops it.
    @Published public var spinning = false { didSet { if spinning != oldValue { applySpin() } } }
    /// Degrees of longitude a second while spinning.
    public var spinSpeed = 12.0 { didSet { applySpin() } }

    /// The Rust globe (`GeodbGlobe *`), made when the view has a size.
    private var globe: OpaquePointer?
    /// A flight asked for before the globe existed.
    private var pendingFlight: (Double, Double, Double)?
    private var idleFrames = 0
    private var lastIdleDraw: CFTimeInterval = 0
    private var buffer = [GeodbGlobeLabel](repeating: GeodbGlobeLabel(), count: 32)

    public init() {}

    deinit {
        if let globe { geodb_globe_free(globe) }
    }

    public var selected: GlobeLabel? { labels.first(where: \.selected) }

    /// Flies to (latitude, longitude) at `altitudeKm` above the ground.
    public func fly(to latitude: Double, longitude: Double, altitudeKm: Double = 400) {
        spinning = false
        guard let globe else {
            pendingFlight = (latitude, longitude, altitudeKm)
            return
        }
        geodb_globe_fly_to(globe, latitude, longitude, altitudeKm)
        wake()
    }

    /// Multiplies the altitude by `factor` (below 1 zooms in).
    public func zoom(by factor: Double) {
        guard let globe else { return }
        geodb_globe_zoom(globe, factor)
        wake()
    }

    // MARK: driven by the view

    func attach(layer: CAMetalLayer, pixels: CGSize, scale: CGFloat) {
        let (w, h) = (UInt32(max(pixels.width, 1)), UInt32(max(pixels.height, 1)))
        if let globe {
            geodb_globe_resize(globe, w, h, Float(scale))
        } else {
            guard error == nil else { return }
            let made = geodb_globe_new(Unmanaged.passUnretained(layer).toOpaque(), w, h, Float(scale))
            guard let made else {
                error = String(cString: geodb_globe_last_error())
                return
            }
            globe = made
            applyRelief()
            applySpin()
            if let (lat, lon, alt) = pendingFlight {
                geodb_globe_fly_to(made, lat, lon, alt)
                pendingFlight = nil
            }
        }
        wake()
    }

    /// A finger (or the mouse button) is down on the globe: the spin waits for it.
    func setPressed(_ down: Bool) {
        guard let globe else { return }
        geodb_globe_set_pressed(globe, down)
        wake()
    }

    func drag(dx: CGFloat, dy: CGFloat) {
        guard let globe else { return }
        geodb_globe_drag(globe, Float(dx), Float(dy))
        wake()
    }

    func tap(at point: CGPoint) {
        guard let globe else { return }
        _ = geodb_globe_tap(globe, Float(point.x), Float(point.y))
        wake()
    }

    func wake() {
        idleFrames = 0
    }

    /// One display refresh: draws while the camera moves and a few frames after (the query runs when it
    /// comes to rest), then once a second (the terminator moves with the sun).
    func tick(dt: CFTimeInterval, now: CFTimeInterval) {
        guard let globe else { return }
        if idleFrames > 3 && now - lastIdleDraw < 1 { return }
        if geodb_globe_frame(globe, dt) {
            idleFrames = 0
        } else {
            idleFrames += 1
            if idleFrames > 3 { lastIdleDraw = now }
        }
        refreshOverlay(globe)
    }

    private func refreshOverlay(_ globe: OpaquePointer) {
        let n = Int(geodb_globe_labels(globe, &buffer, UInt32(buffer.count)))
        let fresh: [GlobeLabel] = buffer.prefix(n).enumerated().compactMap { i, l in
            guard l.visible else { return nil }
            return GlobeLabel(
                id: i,
                name: String(cString: l.name),
                detail: String(cString: l.detail),
                latitude: l.lat,
                longitude: l.lon,
                point: CGPoint(x: CGFloat(l.x), y: CGFloat(l.y)),
                selected: l.selected
            )
        }
        if fresh != labels { labels = fresh }
        var s = GeodbGlobeState()
        if geodb_globe_state(globe, &s) {
            let now = GlobeStatus(
                latitude: s.lat, longitude: s.lon, altitudeKm: s.altitude_km,
                radiusKm: s.radius_km, cities: Int(s.cities), moving: s.moving,
                spinDegreesPerSecond: s.spin_deg_s)
            if now != status { status = now }
            // (a tap that flies somewhere stops the spin in the globe itself)
            if spinning && s.spin_deg_s == 0 { spinning = false }
        }
    }

    private func applySpin() {
        guard let globe else { return }
        geodb_globe_set_spin(globe, spinning ? spinSpeed : 0)
        wake()
    }

    private func applyRelief() {
        guard let globe else { return }
        geodb_globe_set_relief(globe, relief, isohypses)
        wake()
    }
}

/// The globe with its city labels. Drag to turn, pinch (or scroll) to zoom, tap a label to fly to that city,
/// tap the globe to fly closer. On tvOS: swipe on the remote to turn, click to zoom in, hold the click to
/// zoom out, play/pause to start or stop the spin.
public struct GlobeView: View {
    @ObservedObject private var controller: GlobeController
    private let showsLabels: Bool

    public init(controller: GlobeController, showsLabels: Bool = true) {
        self.controller = controller
        self.showsLabels = showsLabels
    }

    public var body: some View {
        ZStack(alignment: .topLeading) {
            GlobeSurface(controller: controller)
            if showsLabels {
                ForEach(Self.unclutter(controller.labels)) { label in
                    GlobeLabelView(label: label)
                        .position(x: label.point.x, y: label.point.y - 16)
                }
                .allowsHitTesting(false)
            }
            if let error = controller.error {
                Text("The globe needs Metal: \(error)")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .padding()
            }
        }
        .background(Color.black)
    }
}

extension GlobeView {
    /// The labels that fit: in rank order (the selected one first), each one only where it covers none
    /// placed before it (the width is estimated from the name).
    static func unclutter(_ labels: [GlobeLabel]) -> [GlobeLabel] {
        #if os(tvOS)
        let (perChar, height): (CGFloat, CGFloat) = (14, 34)
        #else
        let (perChar, height): (CGFloat, CGFloat) = (7, 20)
        #endif
        var placed: [CGRect] = []
        var shown: [GlobeLabel] = []
        for label in labels.filter(\.selected) + labels.filter({ !$0.selected }) {
            let width = CGFloat(label.name.count) * perChar + 14
            let rect = CGRect(
                x: label.point.x - width / 2, y: label.point.y - 16 - height / 2,
                width: width, height: height)
            if placed.contains(where: { $0.intersects(rect) }) { continue }
            placed.append(rect)
            shown.append(label)
        }
        return shown
    }
}

private struct GlobeLabelView: View {
    let label: GlobeLabel

    var body: some View {
        Text(label.name)
            #if os(tvOS)
            .font(.caption)
            #else
            .font(.caption2.weight(label.selected ? .bold : .medium))
            #endif
            .foregroundStyle(label.selected ? Color.black : Color.white)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(
                Capsule().fill(label.selected ? Color.pink : Color.black.opacity(0.55))
            )
            .fixedSize()
    }
}

#if canImport(UIKit)
private struct GlobeSurface: UIViewRepresentable {
    let controller: GlobeController

    func makeUIView(context: Context) -> GlobeMetalView {
        GlobeMetalView(controller: controller)
    }

    func updateUIView(_ view: GlobeMetalView, context: Context) {}
}

/// A UIView on a CAMetalLayer, redrawn from a display link; gestures turn and zoom the globe.
final class GlobeMetalView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    private weak var controller: GlobeController?
    private var link: CADisplayLink?
    private var lastTimestamp: CFTimeInterval = 0

    init(controller: GlobeController) {
        self.controller = controller
        super.init(frame: .zero)
        isOpaque = true
        backgroundColor = .black
        let pan = UIPanGestureRecognizer(target: self, action: #selector(panned(_:)))
        addGestureRecognizer(pan)
        #if os(tvOS)
        // the remote: a click zooms in, holding it zooms out, play/pause starts or stops the spin
        let select = NSNumber(value: UIPress.PressType.select.rawValue)
        let click = UITapGestureRecognizer(target: self, action: #selector(clicked))
        click.allowedPressTypes = [select]
        addGestureRecognizer(click)
        let hold = UILongPressGestureRecognizer(target: self, action: #selector(held(_:)))
        hold.allowedPressTypes = [select]
        hold.minimumPressDuration = 0.5
        addGestureRecognizer(hold)
        click.require(toFail: hold)
        let playPause = UITapGestureRecognizer(target: self, action: #selector(toggleSpin))
        playPause.allowedPressTypes = [NSNumber(value: UIPress.PressType.playPause.rawValue)]
        addGestureRecognizer(playPause)
        #else
        addGestureRecognizer(UIPinchGestureRecognizer(target: self, action: #selector(pinched(_:))))
        addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped(_:))))
        #endif
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    #if os(tvOS)
    override var canBecomeFocused: Bool { true }
    #endif

    override func didMoveToWindow() {
        super.didMoveToWindow()
        link?.invalidate()
        link = nil
        guard window != nil else { return }
        let link = CADisplayLink(target: self, selector: #selector(step(_:)))
        link.add(to: .main, forMode: .common)
        self.link = link
        lastTimestamp = 0
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let scale = traitCollection.displayScale > 0 ? traitCollection.displayScale : 2
        contentScaleFactor = scale
        guard let metal = layer as? CAMetalLayer, bounds.width >= 1, bounds.height >= 1 else { return }
        let pixels = CGSize(width: bounds.width * scale, height: bounds.height * scale)
        controller?.attach(layer: metal, pixels: pixels, scale: scale)
    }

    @objc private func step(_ link: CADisplayLink) {
        let dt = lastTimestamp == 0 ? 1.0 / 60.0 : link.timestamp - lastTimestamp
        lastTimestamp = link.timestamp
        controller?.tick(dt: dt, now: link.timestamp)
    }

    @objc private func panned(_ pan: UIPanGestureRecognizer) {
        switch pan.state {
        case .began:
            controller?.setPressed(true)
        case .ended, .cancelled, .failed:
            controller?.setPressed(false)
            return
        default:
            break
        }
        let t = pan.translation(in: self)
        pan.setTranslation(.zero, in: self)
        controller?.drag(dx: t.x, dy: t.y)
    }

    #if os(tvOS)
    @objc private func clicked() {
        controller?.zoom(by: 0.5)
    }

    @objc private func held(_ hold: UILongPressGestureRecognizer) {
        if hold.state == .began { controller?.zoom(by: 2) }
    }

    @objc private func toggleSpin() {
        controller?.spinning.toggle()
    }
    #else
    @objc private func pinched(_ pinch: UIPinchGestureRecognizer) {
        let factor = 1 / max(pinch.scale, 0.01)
        pinch.scale = 1
        controller?.zoom(by: Double(factor))
    }

    @objc private func tapped(_ tap: UITapGestureRecognizer) {
        let p = tap.location(in: self)
        controller?.tap(at: p)
    }
    #endif
}

#elseif canImport(AppKit)
private struct GlobeSurface: NSViewRepresentable {
    let controller: GlobeController

    func makeNSView(context: Context) -> GlobeMetalView {
        GlobeMetalView(controller: controller)
    }

    func updateNSView(_ view: GlobeMetalView, context: Context) {}
}

/// An NSView on a CAMetalLayer, redrawn from a display link; drag, scroll and pinch turn and zoom the globe.
final class GlobeMetalView: NSView {
    private weak var controller: GlobeController?
    /// Stops the display link (macOS 14) or the timer that stands in for it.
    private var stopTicks: (() -> Void)?
    private var lastTimestamp: CFTimeInterval = 0
    private var dragged = false

    init(controller: GlobeController) {
        self.controller = controller
        super.init(frame: .zero)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    override var isFlipped: Bool { true }

    override func makeBackingLayer() -> CALayer { CAMetalLayer() }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        stopTicks?()
        stopTicks = nil
        guard window != nil else { return }
        lastTimestamp = 0
        if #available(macOS 14.0, *) {
            let link = displayLink(target: self, selector: #selector(step(_:)))
            link.add(to: .main, forMode: .common)
            stopTicks = { link.invalidate() }
        } else {
            let timer = Timer(
                timeInterval: 1.0 / 60.0, target: self, selector: #selector(timerStep),
                userInfo: nil, repeats: true)
            RunLoop.main.add(timer, forMode: .common)
            stopTicks = { timer.invalidate() }
        }
        attach()
    }

    override func layout() {
        super.layout()
        attach()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        attach()
    }

    private func attach() {
        let scale = window?.backingScaleFactor ?? 2
        guard let metal = layer as? CAMetalLayer, bounds.width >= 1, bounds.height >= 1 else { return }
        metal.contentsScale = scale
        let pixels = CGSize(width: bounds.width * scale, height: bounds.height * scale)
        controller?.attach(layer: metal, pixels: pixels, scale: scale)
    }

    @available(macOS 14.0, *)
    @objc private func step(_ link: CADisplayLink) {
        tick(at: link.timestamp)
    }

    @objc private func timerStep() {
        tick(at: CACurrentMediaTime())
    }

    private func tick(at now: CFTimeInterval) {
        let dt = lastTimestamp == 0 ? 1.0 / 60.0 : now - lastTimestamp
        lastTimestamp = now
        controller?.tick(dt: dt, now: now)
    }

    override func mouseDown(with event: NSEvent) {
        dragged = false
        controller?.setPressed(true)
    }

    override func mouseDragged(with event: NSEvent) {
        dragged = true
        controller?.drag(dx: event.deltaX, dy: event.deltaY)
    }

    override func mouseUp(with event: NSEvent) {
        controller?.setPressed(false)
        guard !dragged else { return }
        let p = convert(event.locationInWindow, from: nil)
        controller?.tap(at: p)
    }

    override func scrollWheel(with event: NSEvent) {
        let factor = exp(-Double(event.scrollingDeltaY) * (event.hasPreciseScrollingDeltas ? 0.004 : 0.04))
        controller?.zoom(by: factor)
    }

    override func magnify(with event: NSEvent) {
        let factor = 1 / max(1 + Double(event.magnification), 0.01)
        controller?.zoom(by: factor)
    }
}
#endif
#endif
