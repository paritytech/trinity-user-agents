import CoreMotion
import MetalKit
import SwiftUI

/// A `Materials` node: its children until its layers are ready and compiled, then the last layer's
/// output in their place. For good after a failure, the children alone, exactly as a box draws them.
/// A node that bleeds hands its output to the card root, which draws it over the whole tree.
struct MaterialsNodeView<Children: View>: View {
    let index: UInt32
    let bleed: CGFloat
    @ViewBuilder let children: () -> Children

    @Environment(\.widgetMaterials) private var materials
    @Environment(\.displayScale) private var displayScale
    @Environment(\.widgetImageResolver) private var resolveImage
    @Environment(\.widgetActionHandler) private var actionHandler
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.widgetImages) private var enclosingImages
    @Environment(\.widgetTouchArea) private var touchArea
    @State private var ownImages = WidgetImageStore()
    @State private var size: CGSize = .zero

    var body: some View {
        let surface = materials?.surface(of: index)
        children()
            .environment(\.widgetImages, images)
            .opacity(surface == nil ? 1 : 0)
            .overlay(alignment: .topLeading) {
                if let surface, bleed == 0 {
                    MaterialSurfaceView(surface: surface, touchArea: touchArea)
                        .allowsHitTesting(false)
                }
            }
            .anchorPreference(key: BleedingMaterialsKey.self, value: .bounds) { bounds in
                guard let surface, bleed > 0 else { return [] }
                return [BleedingMaterial(
                    index: index,
                    bounds: bounds,
                    bleed: bleed,
                    surface: surface,
                    touchArea: touchArea
                )]
            }
            .onGeometryChange(for: CGSize.self) { $0.size } action: { size = $0 }
            .task(id: preparation) {
                await prepare()
            }
    }
}

private struct Preparation: Equatable {
    let state: WidgetMaterialsState?
    let generation: Int?
    let size: CGSize
    let colorScheme: ColorScheme
    let displayScale: CGFloat
    let storedImages: Int
}

private extension MaterialsNodeView {
    /// A nested node shares the store of the outermost one, so its pictures reach that node's base too.
    var images: WidgetImageStore {
        enclosingImages ?? ownImages
    }

    var preparation: Preparation {
        Preparation(
            state: materials?.state(of: index),
            generation: materials?.generation,
            size: size,
            colorScheme: colorScheme,
            displayScale: displayScale,
            storedImages: images.stored
        )
    }
}

private extension MaterialsNodeView {
    /// A node still loading keeps the surface it draws, which may be one a node of the tree before left.
    /// The new surface carries the clock of the tree it was made for, so `Elapsed` restarts with the
    /// arguments that go with it.
    func prepare() async {
        guard let materials else { return }
        let state = materials.state(of: index)
        guard case let .ready(layers) = state else {
            if state != .loading { materials.dropSurface(of: index) }
            return
        }
        guard let first = layers.first, size.width > 0, size.height > 0 else { return }
        let treeStartedAt = materials.treeStartedAt
        guard let gpu = MaterialGPU.shared, let noise = materials.noise, let snapshot = snapshot() else {
            materials.fail(node: index, material: first.material, reason: "no Metal device, noise image or snapshot")
            return
        }
        do {
            let made = try await MaterialSurface.make(
                layers: layers,
                gpu: gpu,
                base: snapshot,
                noise: noise,
                treeStartedAt: treeStartedAt
            )
            guard !Task.isCancelled else { return }
            materials.keep(made, for: index)
        } catch let failure as MaterialSurface.Failure {
            guard !Task.isCancelled else { return }
            materials.fail(node: index, material: failure.material, reason: "\(failure.reason)")
        } catch {
            guard !Task.isCancelled else { return }
            materials.fail(node: index, material: first.material, reason: "\(error)")
        }
    }

    /// The children as the card draws them, at the laid-out size, inside a clear margin as wide as the
    /// bleed. Nested `Materials` nodes draw plain here.
    func snapshot() -> CGImage? {
        let renderer = ImageRenderer(
            content: children()
                .environment(\.widgetImageResolver, resolveImage)
                .environment(\.widgetImages, images)
                .environment(\.widgetActionHandler, actionHandler)
                .environment(\.colorScheme, colorScheme)
                .frame(width: size.width, height: size.height, alignment: .topLeading)
                .padding(bleed)
        )
        renderer.proposedSize = ProposedViewSize(width: size.width + bleed * 2, height: size.height + bleed * 2)
        renderer.scale = displayScale
        renderer.isOpaque = false
        return renderer.cgImage
    }
}

/// What a ready node draws with: its compiled layers and their images.
@MainActor
final class MaterialSurface {
    let renderer: MaterialLayerRenderer
    let isAnimated: Bool
    let readsMotion: Bool
    let readsTrail: Bool
    let pixelSize: CGSize
    /// The material of each layer, which a node of a later tree matches to draw this surface while it loads.
    let materials: [Data]
    /// When the card was given the tree this surface draws, the zero of `elapsed`.
    let treeStartedAt: CFTimeInterval

    private init(
        renderer: MaterialLayerRenderer,
        layers: [WidgetMaterialLayer],
        pixelSize: CGSize,
        treeStartedAt: CFTimeInterval
    ) {
        self.renderer = renderer
        isAnimated = layers.contains(where: \.isAnimated)
        readsMotion = layers.contains(where: \.readsMotion)
        readsTrail = layers.contains(where: \.readsTrail)
        self.pixelSize = pixelSize
        materials = layers.map(\.material)
        self.treeStartedAt = treeStartedAt
    }

    /// Names the layer that could not be set up.
    struct Failure: Error {
        let material: Data
        let reason: any Error
    }

    enum SetupError: Error {
        case unsupportedTexture
        case noNoiseTexture
        case undrawableSnapshot
    }

    static func make(
        layers: [WidgetMaterialLayer],
        gpu: MaterialGPU,
        base: CGImage,
        noise: WidgetMaterialNoise,
        treeStartedAt: CFTimeInterval
    ) async throws -> MaterialSurface {
        if let unsupported = layers.first(where: { !$0.usesOnlyHostTextures }) {
            throw Failure(material: unsupported.material, reason: SetupError.unsupportedTexture)
        }
        var compiled: [MaterialLayerRenderer.CompiledLayer] = []
        for (position, layer) in layers.enumerated() {
            let format: MTLPixelFormat = position == layers.count - 1 ? .bgra8Unorm : .rgba32Float
            do {
                try await compiled.append(.init(layer: layer, pipeline: gpu.pipeline(for: layer, pixelFormat: format)))
            } catch {
                throw Failure(material: layer.material, reason: error)
            }
        }
        guard let baseTexture = gpu.texture(of: base) else { throw SetupError.undrawableSnapshot }
        guard let noiseTexture = gpu.noise(noise) else { throw SetupError.noNoiseTexture }
        return MaterialSurface(
            renderer: MaterialLayerRenderer(gpu: gpu, layers: compiled, base: baseTexture, noise: noiseTexture),
            layers: layers,
            pixelSize: CGSize(width: base.width, height: base.height),
            treeStartedAt: treeStartedAt
        )
    }
}

/// The output of a node that bleeds, which the card root draws over its bounds and the margin around them.
struct BleedingMaterial {
    let index: UInt32
    let bounds: Anchor<CGRect>
    let bleed: CGFloat
    let surface: MaterialSurface
    let touchArea: WidgetTouchArea?
}

struct BleedingMaterialsKey: PreferenceKey {
    static var defaultValue: [BleedingMaterial] { [] }

    static func reduce(value: inout [BleedingMaterial], nextValue: () -> [BleedingMaterial]) {
        value.append(contentsOf: nextValue())
    }
}

/// Draws over the whole card, past the clip of every node, so a bleeding node's output can leave the card.
struct BleedingMaterialsView: View {
    let materials: [BleedingMaterial]

    var body: some View {
        GeometryReader { proxy in
            ForEach(materials, id: \.index) { material in
                let stage = proxy[material.bounds].insetBy(dx: -material.bleed, dy: -material.bleed)
                MaterialSurfaceView(surface: material.surface, touchArea: material.touchArea)
                    .frame(width: stage.width, height: stage.height)
                    .position(x: stage.midX, y: stage.midY)
            }
        }
        .allowsHitTesting(false)
    }
}

private struct MaterialSurfaceView: UIViewRepresentable {
    let surface: MaterialSurface
    let touchArea: WidgetTouchArea?

    func makeUIView(context _: Context) -> MaterialMetalView {
        let view = MaterialMetalView(surface: surface)
        configure(view)
        return view
    }

    func updateUIView(_ view: MaterialMetalView, context _: Context) {
        view.surface = surface
        configure(view)
    }

    private func configure(_ view: MaterialMetalView) {
        view.touchArea = touchArea
    }
}

/// Draws only while on screen and its scene is active. A still material draws once, and again on each
/// finger sample of its `Touch` node. Reduced motion draws once with time and tilt zero, elapsed and
/// replay at an hour. Time counts from zero each time the view comes on screen, across new surfaces,
/// and elapsed from the tree of the surface it draws.
final class MaterialMetalView: MTKView, MTKViewDelegate {
    var surface: MaterialSurface {
        didSet {
            guard surface !== oldValue else { return }
            run()
        }
    }

    var touchArea: WidgetTouchArea? {
        didSet {
            guard touchArea !== oldValue else { return }
            touchArea?.redraw(on: self)
            setNeedsDisplay()
        }
    }

    private let motionSource: DeviceAttitudeObservable
    private var motionToken: DeviceMotionToken?
    private var motion = MaterialMotion.still
    private var startedAt = CACurrentMediaTime()
    private var observers: [NSObjectProtocol] = []

    init(surface: MaterialSurface, motionSource: DeviceAttitudeObservable = DeviceMotionSource.shared) {
        self.surface = surface
        self.motionSource = motionSource
        super.init(frame: .zero, device: surface.renderer.gpu.device)
        delegate = self
        isOpaque = false
        backgroundColor = .clear
        clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
        colorPixelFormat = .bgra8Unorm
        autoResizeDrawable = false
        drawableSize = surface.pixelSize
        observeActivity()
    }

    @available(*, unavailable)
    required init(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        restart()
    }

    func mtkView(_: MTKView, drawableSizeWillChange _: CGSize) {}

    func draw(in view: MTKView) {
        let trail = surface.readsTrail ? touchArea?.tracking.trail() : nil
        guard let pass = view.currentRenderPassDescriptor,
              let drawable = view.currentDrawable,
              let commands = surface.renderer.draw(into: pass, inputs: frameInputs(), trailImage: trail)
        else { return }
        commands.present(drawable)
        commands.commit()
    }
}

private extension MaterialMetalView {
    var runsContinuously: Bool {
        window?.windowScene?.activationState == .foregroundActive && surface.isAnimated
            && !UIAccessibility.isReduceMotionEnabled
    }

    func restart() {
        startedAt = CACurrentMediaTime()
        run()
    }

    func run() {
        drawableSize = surface.pixelSize
        let continuous = runsContinuously
        isPaused = !continuous
        enableSetNeedsDisplay = !continuous
        if continuous, surface.readsMotion {
            if motionToken == nil {
                motionToken = motionSource.observeMotion { [weak self] reading in
                    self?.motion = MaterialMotion(deviceToWorld: reading.attitude.rotationMatrix.deviceToWorldRows)
                }
            }
        } else {
            motionToken = nil
            motion = .still
        }
        if window != nil { setNeedsDisplay() }
    }

    func observeActivity() {
        let center = NotificationCenter.default
        for name in [UIScene.didActivateNotification, UIScene.willDeactivateNotification] {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                self?.restart()
            })
        }
    }

    /// Elapsed and replay read an hour under reduced motion, so an entry animation shows its end.
    static let reducedMotionElapsed: Float = 3_600

    /// Until the first reading arrives, and when none does, the motion stays still.
    func frameInputs() -> MaterialFrameInputs {
        let now = CACurrentMediaTime()
        var inputs = runsContinuously
            ? MaterialFrameInputs(time: Float(now - startedAt), motion: motion, resolution: .zero)
            : MaterialFrameInputs(resolution: .zero)
        inputs.touch = touchArea?.tracking.inputs(now: now) ?? .none
        inputs.elapsed = Float(now - surface.treeStartedAt)
        if UIAccessibility.isReduceMotionEnabled {
            inputs.elapsed = Self.reducedMotionElapsed
            inputs.touch.replay = Self.reducedMotionElapsed
        }
        return inputs
    }
}

private extension CMRotationMatrix {
    /// Core Motion's matrix turns the reference frame into the device's, so its columns are the
    /// rows of the device-to-world rotation the RFC angles are read from.
    var deviceToWorldRows: [Float] {
        [m11, m21, m31, m12, m22, m32, m13, m23, m33].map(Float.init)
    }
}
