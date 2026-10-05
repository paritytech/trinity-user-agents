import Foundation
import Metal
import Testing

@testable import polkadot_app

/// The renderer holds the meshes, the studio and the compiled pipeline: the same for every coin on
/// every screen, and none of it changes while the app runs. Building it took the best part of a
/// second with the main thread held, which was the whole of the pause on opening the view.
@Suite("Coin renderer loader", .serialized)
struct CoinageRendererLoaderTests {
    /// The process-wide one, deliberately: these check that it hands everybody the same renderer,
    /// which a fresh instance per test could not show.
    private var loader: CoinageRendererLoader { .shared }

    @Test("Asking for the renderer does not make the caller wait for it")
    func loadingDoesNotBlockTheCaller() async {
        // How long `load` itself takes to come back, clocked inside the closure that calls it.
        // Two earlier versions of this test measured the wrong thing: one set a flag on the line
        // after `load` returned, which is true however long it blocked, and one timed the whole
        // `await`, which necessarily includes building the renderer — 23 seconds of it on CI.
        nonisolated(unsafe) var returned: TimeInterval?

        let start = CFAbsoluteTimeGetCurrent()
        let renderer: CoinageMetalRenderer? = await withCheckedContinuation { continuation in
            loader.load { continuation.resume(returning: $0) }
            returned = CFAbsoluteTimeGetCurrent() - start
        }

        #expect(renderer != nil)
        // Enqueuing is all `load` does, so its own return is immediate however slow the machine is
        // and however long the build that follows takes.
        #expect((returned ?? .infinity) < 1)
    }

    @Test("Everyone gets the same renderer rather than another copy of it")
    func theRendererIsSharedAcrossCallers() async {
        let first: CoinageMetalRenderer? = await withCheckedContinuation { continuation in
            loader.load { continuation.resume(returning: $0) }
        }
        let second: CoinageMetalRenderer? = await withCheckedContinuation { continuation in
            loader.load { continuation.resume(returning: $0) }
        }

        #expect(first != nil)
        #expect(first === second)
    }

    @Test("Callers waiting together are all answered")
    func everyWaiterIsAnswered() async {
        let delivered = await withTaskGroup(of: Bool.self) { group in
            for _ in 0 ..< 8 {
                group.addTask {
                    await withCheckedContinuation { continuation in
                        loader.load { continuation.resume(returning: $0 != nil) }
                    }
                }
            }

            return await group.reduce(into: 0) { $0 += $1 ? 1 : 0 }
        }

        #expect(delivered == 8)
    }

    @Test("The view is told the samples the pipeline will be built for, before there is one")
    func sampleCountAgreesWithTheRenderer() async throws {
        let device = try #require(MTLCreateSystemDefaultDevice())
        let renderer: CoinageMetalRenderer? = await withCheckedContinuation { continuation in
            loader.load { continuation.resume(returning: $0) }
        }

        // A view and a pipeline that disagree here fail validation at the draw call, with nothing
        // before it to say why.
        #expect(loader.sampleCount(for: device) == renderer?.sampleCount)
    }
}
