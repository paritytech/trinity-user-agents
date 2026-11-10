import Foundation_iOS
import Metal
import os

protocol CoinageRendererLoading: AnyObject, Sendable {
    /// What ``CoinageMetalRenderer/sampleCount(for:)`` will answer, for a view that has to be
    /// configured before there is a renderer to ask.
    func sampleCount(for device: MTLDevice?) -> Int

    /// Hands over the renderer, building it first if nobody has yet.
    ///
    /// Always answers on the main queue and never inline, so a caller has the same shape of life
    /// whether it is the first to ask or the hundredth.
    func load(_ deliver: @escaping (CoinageMetalRenderer?) -> Void)
}

/// Builds the one renderer, once, away from the main thread.
///
/// Everything it owns — the meshes, the studio, the relief atlas, the compiled pipeline — is the
/// same for every coin on every screen, and none of it changes while the app runs. It used to be
/// built in the view's coordinator, so it happened again on every appearance and, worse, with the
/// main thread held: about three hundred milliseconds of assets and, the first time, another four
/// hundred or so for Metal to compile the pipeline. That is the whole of the pause between tapping
/// the card and anything happening.
///
/// Callers get the renderer when it is ready and draw nothing until then, which is a frame or two
/// of an empty strip rather than a frozen tap.
///
/// One instance per process is the point, so views take ``shared`` unless a test hands them
/// something else. An instance built per module assembly would be the original stall again.
final class CoinageRendererLoader: CoinageRendererLoading, @unchecked Sendable {
    static let shared = CoinageRendererLoader()

    private enum State {
        case idle
        case loading
        case ready(CoinageMetalRenderer?)
    }

    private let logger: LoggerProtocol
    private let state = OSAllocatedUnfairLock(initialState: State.idle)
    private let waiting = OSAllocatedUnfairLock(initialState: [(CoinageMetalRenderer?) -> Void]())

    init(logger: LoggerProtocol = Logger.shared) {
        self.logger = logger
    }

    func sampleCount(for device: MTLDevice?) -> Int {
        CoinageMetalRenderer.sampleCount(for: device)
    }

    func load(_ deliver: @escaping (CoinageMetalRenderer?) -> Void) {
        let work: (() -> Void)? = state.withLockUnchecked { current in
            switch current {
            case let .ready(renderer):
                return { DispatchQueue.main.async { deliver(renderer) } }
            case .loading:
                waiting.withLockUnchecked { $0.append(deliver) }
                return nil
            case .idle:
                current = .loading
                waiting.withLockUnchecked { $0.append(deliver) }
                return build
            }
        }

        work?()
    }
}

private extension CoinageRendererLoader {
    func build() {
        DispatchQueue.global(qos: .userInitiated).async { [self] in
            let renderer = makeRenderer()

            state.withLockUnchecked { $0 = .ready(renderer) }

            let deliveries = waiting.withLockUnchecked { waiting -> [(CoinageMetalRenderer?) -> Void] in
                defer { waiting = [] }

                return waiting
            }

            DispatchQueue.main.async {
                for deliver in deliveries {
                    deliver(renderer)
                }
            }
        }
    }

    /// Nothing can be drawn without a renderer, and nothing on this screen can recover from that,
    /// so the failure is reported rather than raised: the card keeps its figures and shows no
    /// coins. Worth a line in the log, because the causes (a device without Metal, an asset that
    /// did not ship, a shader that did not compile) are all things we would want to know about.
    func makeRenderer() -> CoinageMetalRenderer? {
        do {
            return try CoinageMetalRenderer()
        } catch {
            logger.error("Coin renderer unavailable, drawing no coins: \(error)")

            return nil
        }
    }
}
