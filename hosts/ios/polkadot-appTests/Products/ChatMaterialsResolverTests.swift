import Foundation
import os
import QuartzCore
@testable import PolkadotUI
import Testing
import TrUAPIHost
@testable import polkadot_app

@MainActor
@Suite(.timeLimit(.minutes(1)))
struct ChatMaterialsResolverTests {
    private let tree = RendererNode.materials(props: MaterialsProps(layers: [], bleed: 0), children: [])

    @Test func aReadyNodeReachesTheCardWithItsLayers() async throws {
        // Given
        let resolution = StubMaterialsResolution()
        let resolver = makeResolver { _ in resolution }
        resolver.follow(tree: tree)

        // When
        await change(of: resolver.materials) {
            resolution.send(ResolvedMaterials(nodes: [.ready(layers: [Self.layer])], noiseSeed: 3))
        }

        // Then
        #expect(resolver.materials.state(of: 0) == .ready([Self.layer.widgetLayer]))
        #expect(resolver.materials.noise?.bytes == Data([7]))
    }

    /// The old resolution holds the materials both trees share, so it goes only once the new one is open.
    @Test func aNewTreeIsResolvedBeforeTheOldResolutionIsCancelled() {
        // Given
        var events: [String] = []
        let first = StubMaterialsResolution { events.append("cancel first") }
        let second = StubMaterialsResolution { events.append("cancel second") }
        var resolutions = [first, second]
        let resolver = makeResolver { _ in
            events.append("resolve")
            return resolutions.removeFirst()
        }

        // When
        resolver.follow(tree: tree)
        resolver.follow(tree: tree)

        // Then
        #expect(events == ["resolve", "resolve", "cancel first"])
    }

    /// The core keeps a reported node plain only for its own resolution, so the new one must hear it too.
    @Test func aNodeThatFailsOnTheDeviceIsReportedOncePerResolution() {
        // Given
        let first = StubMaterialsResolution()
        let second = StubMaterialsResolution()
        var resolutions = [first, second]
        let resolver = makeResolver { _ in resolutions.removeFirst() }
        let report = StubMaterialsResolution.Report(node: 0, material: Self.layer.material, reason: .unsupported)

        // When
        resolver.follow(tree: tree)
        resolver.materials.fail(node: 0, material: Self.layer.material, reason: "compile")
        resolver.materials.fail(node: 0, material: Self.layer.material, reason: "compile")
        resolver.follow(tree: tree)
        resolver.materials.fail(node: 0, material: Self.layer.material, reason: "compile")

        // Then
        #expect([first.reports, second.reports] == [[report], [report]])
    }

    @Test func aTreeTheHandlerCannotResolveDrawsPlain() async {
        // Given
        let resolution = StubMaterialsResolution()
        var resolutions: [StubMaterialsResolution?] = [resolution, nil]
        let resolver = makeResolver { _ in resolutions.removeFirst() }
        resolver.follow(tree: tree)
        await change(of: resolver.materials) {
            resolution.send(ResolvedMaterials(nodes: [.ready(layers: [Self.layer])], noiseSeed: 3))
        }

        // When
        resolver.follow(tree: tree)

        // Then
        #expect(resolver.materials.state(of: 0) == .plain)
    }

    /// The old layers must not draw with the new tree's clock, and a node that loads draws the output it
    /// had, so every node of a new tree loads until its first update, whatever the number of nodes.
    @Test func aNewTreeLoadsEveryNodeUntilItsFirstUpdate() async {
        // Given
        let resolution = StubMaterialsResolution()
        let resolver = makeResolver { _ in resolution }
        resolver.follow(tree: tree)
        await change(of: resolver.materials) {
            resolution.send(ResolvedMaterials(nodes: [.ready(layers: [Self.layer])], noiseSeed: 3))
        }
        let twoNodes = RendererNode.materials(props: MaterialsProps(layers: [], bleed: 0), children: [tree])

        // When
        resolver.follow(tree: twoNodes)

        // Then
        #expect([resolver.materials.state(of: 0), resolver.materials.state(of: 1)] == [.loading, .loading])
    }

    /// A card redrawn with its nodes moved keeps drawing each node's last output, and never another
    /// node's: an output goes to a node with the same materials, the one at its own index first.
    @Test func eachNodeOfANewTreeStartsFromAnOutputWithItsMaterials() {
        // Given
        let (glow, ring, coin) = (Data([1]), Data([2]), Data([3]))
        let kept: [UInt32: [Data]] = [0: [glow], 1: [ring], 2: [glow]]

        // When
        let carried = WidgetMaterials.carried(kept, materials: { $0 }, into: [[ring], [glow], [coin], [glow]])

        // Then
        #expect(carried == [0: [ring], 1: [glow], 3: [glow]])
    }

    /// Equal layers leave the node's state as it was, so only the generation tells it to draw its
    /// new children.
    @Test func aNewTreeOfTheSameCardStartsANewGeneration() {
        // Given
        let resolver = makeResolver { _ in StubMaterialsResolution() }
        resolver.follow(tree: tree)
        let before = resolver.materials.generation

        // When
        resolver.follow(tree: tree)

        // Then
        #expect(resolver.materials.generation != before)
    }

    /// A node that draws plain on one phone is found only through the host log.
    @Test func aNodeThatCannotDrawIsLoggedWithItsMaterial() {
        // Given
        let logger = RecordingLogger()
        let resolver = makeResolver(logger: logger) { _ in StubMaterialsResolution() }
        resolver.follow(tree: tree)

        // When
        resolver.materials.fail(node: 0, material: Self.layer.material, reason: "compile")

        // Then
        let material = "0x" + String(repeating: "09", count: 32)
        #expect(logger.lines == [.warning("Materials node 0 draws plain, material \(material): compile")])
    }

    /// Each noise image is 1 MiB, made for nothing while no node can draw.
    @Test func aCardWithNoReadyNodeMakesNoNoise() async {
        // Given
        let made = OSAllocatedUnfairLock(initialState: 0)
        let cache = MaterialNoiseCache { _ in
            made.withLock { $0 += 1 }
            return Data([7])
        }

        // When
        let noise = await cache.noise(for: ResolvedMaterials(nodes: [.loading], noiseSeed: 3))

        // Then
        #expect(noise == nil)
        #expect(made.withLock { $0 } == 0)
    }

    @Test func twoCardsWithOneSeedMakeTheNoiseOnce() async {
        // Given
        let made = OSAllocatedUnfairLock(initialState: 0)
        let cache = MaterialNoiseCache { _ in
            made.withLock { $0 += 1 }
            return Data([7])
        }
        let update = ResolvedMaterials(nodes: [.ready(layers: [Self.layer])], noiseSeed: 3)

        // When
        _ = await cache.noise(for: update)
        _ = await cache.noise(for: update)

        // Then
        #expect(made.withLock { $0 } == 1)
    }

    /// The card draws exactly what the core built: a stage or offset swapped feeds the shader garbage.
    @Test func aReadyLayerKeepsEveryFieldTheCoreGave() {
        // Given
        let ready = ReadyLayer(
            material: Data(repeating: 9, count: 32),
            shader: MaterialShader(
                vertex: "v",
                fragment: "f",
                uniforms: [
                    Uniform(name: "flip", kind: .float, stage: .vertex, offset: 0, role: .targetFlip),
                    Uniform(name: "mode", kind: .int, stage: .fragment, offset: 96, role: .samplingMode(1))
                ],
                vertexBlockSize: 16,
                fragmentBlockSize: 112,
                textures: [Texture(name: "r_base", role: .base), Texture(name: "p", role: .argument(0))],
                readsTime: true,
                readsMotion: false
            ),
            args: [.samplerFloat(.noise)]
        )

        // When
        let layer = ready.widgetLayer

        // Then
        #expect(layer == WidgetMaterialLayer(
            material: Data(repeating: 9, count: 32),
            shader: .init(
                vertex: "v",
                fragment: "f",
                blockSizes: .init(vertex: 16, fragment: 112),
                readsTime: true,
                readsMotion: false
            ),
            uniforms: [
                .init(stage: .vertex, offset: 0, kind: .float, role: .targetFlip),
                .init(stage: .fragment, offset: 96, kind: .int, role: .samplingMode(1))
            ],
            textures: [.base, .argument(0)],
            arguments: [.noise]
        ))
    }

    /// The host fills these arguments itself, so each must reach the card as what it is.
    @Test func everyTouchArgumentReachesTheCardAsItsOwnCase() {
        // Given
        let ready = ReadyLayer(
            material: Data(repeating: 9, count: 32),
            shader: Self.layer.shader,
            args: [.touch, .touchState, .elapsed, .replay, .samplerFloat(.trail), .samplerFloat(.noise)]
        )

        // When
        let layer = ready.widgetLayer

        // Then
        #expect(layer.arguments == [.touch, .touchState, .elapsed, .replay, .trail, .noise])
    }

    /// The core runs the replay clock, and the card reads it through the touch inputs every frame.
    @Test func theReplayClockOfTheCoreReachesTheCard() throws {
        // Given
        let story = TouchProps(drag: .any, trail: nil, triggers: [], replay: ReplayProps(length: 2_200))
        let resolver = makeResolver { _ in StubMaterialsResolution() }
        resolver.follow(tree: .touch(props: story, children: []))
        let area = try #require(resolver.materials.touchArea(index: 0, size: CGSize(width: 300, height: 200)))
        let drawnAt = CACurrentMediaTime()

        // When
        let replay = area.tracking.inputs(now: drawnAt + 1.5).replay

        // Then
        #expect(abs(replay - 1.5) < 0.1)
    }

    /// A product sends a new tree after each trigger. A `Touch` node whose props stay keeps its trail,
    /// so the finger's work is not lost, and one whose props changed starts empty.
    @Test func aNewTreeKeepsATouchAreaWhosePropsStayAndReplacesOneWhosePropsChanged() throws {
        // Given
        func touch(_ radius: UInt64) -> RendererNode {
            .touch(
                props: TouchProps(drag: .any, trail: TrailProps(radius: radius), triggers: [], replay: nil),
                children: []
            )
        }
        let resolver = makeResolver { _ in StubMaterialsResolution() }
        let size = CGSize(width: 300, height: 200)
        let twoAreas = RendererNode.box(modifiers: [], props: BoxProps(contentAlignment: nil), children: [
            touch(20), touch(20)
        ])
        resolver.follow(tree: twoAreas)
        let kept = try #require(resolver.materials.touchArea(index: 0, size: size))
        let replaced = try #require(resolver.materials.touchArea(index: 1, size: size))

        // When
        resolver.follow(tree: .box(modifiers: [], props: BoxProps(contentAlignment: nil), children: [
            touch(20), touch(30)
        ]))

        // Then
        #expect(resolver.materials.touchArea(index: 0, size: size) === kept)
        #expect(resolver.materials.touchArea(index: 1, size: size) !== replaced)
    }

    private static let layer = ReadyLayer(
        material: Data(repeating: 9, count: 32),
        shader: MaterialShader(
            vertex: "v",
            fragment: "f",
            uniforms: [],
            vertexBlockSize: 16,
            fragmentBlockSize: 16,
            textures: [],
            readsTime: true,
            readsMotion: false
        ),
        args: []
    )

    private func makeResolver(
        logger: LoggerProtocol = StubLogger(),
        _ resolve: @escaping (RendererNode) -> (any MaterialsResolving)?
    ) -> ChatMaterialsResolver {
        ChatMaterialsResolver(
            resolve: resolve,
            logger: logger,
            noiseCache: MaterialNoiseCache { _ in Data([7]) }
        )
    }

    /// Resumes once the card's nodes change after `action`, which runs only after the change is watched.
    private func change(of materials: WidgetMaterials, after action: () -> Void) async {
        await withCheckedContinuation { continuation in
            withObservationTracking { _ = materials.nodes } onChange: { continuation.resume() }
            action()
        }
    }
}
