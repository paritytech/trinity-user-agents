import CoreMotion
import MetalKit
import SwiftUI

/// A `Materials` node: its children until its layers are ready and compiled, then the last layer's
/// output in their place. For good after a failure, the children alone, exactly as a box draws them.
struct MaterialsNodeView<Children: View>: View {
    let index: UInt32
    @ViewBuilder let children: () -> Children

    @Environment(\.widgetMaterials) private var materials
    @Environment(\.displayScale) private var displayScale
    @Environment(\.widgetImageResolver) private var resolveImage
    @Environment(\.widgetActionHandler) private var actionHandler
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.widgetImages) private var enclosingImages
    @State private var ownImages = WidgetImageStore()
    @State private var surface: MaterialSurface?
    @State private var size: CGSize = .zero

    var body: some View {
        children()
            .environment(\.widgetImages, images)
            .opacity(surface == nil ? 1 : 0)
            .overlay(alignment: .topLeading) {
                if let surface {
                    MaterialSurfaceView(surface: surface)
                        .allowsHitTesting(false)
                }
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
    func prepare() async {
        guard let materials, case let .ready(layers) = materials.state(of: index) else {
            if materials?.state(of: index) != .loading { surface = nil }
            return
        }
        guard let first = layers.first, size.width > 0, size.height > 0 else { return }
        guard let gpu = MaterialGPU.shared, let noise = materials.noise, let snapshot = snapshot() else {
            surface = nil
            materials.fail(node: index, material: first.material, reason: "no Metal device, noise image or snapshot")
            return
        }
        do {
            let made = try await MaterialSurface.make(layers: layers, gpu: gpu, base: snapshot, noise: noise)
            guard !Task.isCancelled else { return }
            surface = made
        } catch let failure as MaterialSurface.Failure {
            guard !Task.isCancelled else { return }
            surface = nil
            materials.fail(node: index, material: failure.material, reason: "\(failure.reason)")
        } catch {
            guard !Task.isCancelled else { return }
            surface = nil
            materials.fail(node: index, material: first.material, reason: "\(error)")
        }
    }

    /// The children as the card draws them, at the laid-out size. Nested `Materials` nodes draw plain here.
    func snapshot() -> CGImage? {
        let renderer = ImageRenderer(
            content: children()
                .environment(\.widgetImageResolver, resolveImage)
                .environment(\.widgetImages, images)
                .environment(\.widgetActionHandler, actionHandler)
                .environment(\.colorScheme, colorScheme)
                .frame(width: size.width, height: size.height, alignment: .topLeading)
        )
        renderer.proposedSize = ProposedViewSize(size)
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
    let pixelSize: CGSize

    private init(renderer: MaterialLayerRenderer, layers: [WidgetMaterialLayer], pixelSize: CGSize) {
        self.renderer = renderer
        isAnimated = layers.contains(where: \.isAnimated)
        readsMotion = layers.contains(where: \.readsMotion)
        self.pixelSize = pixelSize
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
        noise: WidgetMaterialNoise
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
            pixelSize: CGSize(width: base.width, height: base.height)
        )
    }
}

private struct MaterialSurfaceView: UIViewRepresentable {
    let surface: MaterialSurface

    func makeUIView(context _: Context) -> MaterialMetalView {
        MaterialMetalView(surface: surface)
    }

    func updateUIView(_ view: MaterialMetalView, context _: Context) {
        view.surface = surface
    }
}

/// Draws only while on screen and its scene is active. A still material draws once. Reduced motion
/// draws once with time and tilt zero. Time counts from zero each time the view comes on screen.
final class MaterialMetalView: MTKView, MTKViewDelegate {
    var surface: MaterialSurface {
        didSet { restart() }
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
        guard let pass = view.currentRenderPassDescriptor,
              let drawable = view.currentDrawable,
              let commands = surface.renderer.draw(into: pass, inputs: frameInputs())
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
        drawableSize = surface.pixelSize
        startedAt = CACurrentMediaTime()
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

    /// Until the first reading arrives, and when none does, the motion stays still.
    func frameInputs() -> MaterialFrameInputs {
        guard runsContinuously else { return MaterialFrameInputs(resolution: .zero) }
        return MaterialFrameInputs(time: Float(CACurrentMediaTime() - startedAt), motion: motion, resolution: .zero)
    }
}

private extension CMRotationMatrix {
    /// Core Motion's matrix turns the reference frame into the device's, so its columns are the
    /// rows of the device-to-world rotation the RFC angles are read from.
    var deviceToWorldRows: [Float] {
        [m11, m21, m31, m12, m22, m32, m13, m23, m33].map(Float.init)
    }
}
