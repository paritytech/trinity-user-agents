import Foundation
import PolkadotUI
import Products

/// The tree of one product body, from the render stream of the product.
///
/// The stream runs only while a view shows the body (02-DESIGN §5.2). The core holds the worker of the product for
/// each open render stream, and the decoder keeps this view model for the life of the bot. A stream that opened
/// when the body was decoded, and stayed open, kept the worker for the rest of the session. The last tree stays,
/// so a body that comes back on screen shows it while a new stream opens.
@Observable
final class ProductWidgetViewModel: WidgetNodeProviding {
    @MainActor private(set) var node: CustomMessageWidgetNode?

    private let roomId: String?
    private let messageId: String
    private let messageType: String
    private let messageData: Data
    private let runtime: ChatRuntimeProtocol
    private let tokenResolver: any WidgetDesignTokenResolving
    private let logger: LoggerProtocol
    /// Not private: tests await this instead of a wall clock. Under a loaded
    /// runner a fixed sleep expires before the loop reopens the stream, which
    /// reads as "never retried" even though the retry is on its way.
    private(set) var renderTask: Task<Void, Never>?
    private let retryDelay: Duration
    /// The views that show the body now.
    @MainActor private var visibleBodies = 0

    /// Streams opened for one body. The runtime retries the throws that precede
    /// a stream; this covers a stream that fails, or that ends before drawing.
    /// Without it the cell keeps whatever it last showed — a spinner, or a body
    /// the product can no longer update — for as long as the decoder caches this
    /// view model, which is the bot's lifetime.
    private static let renderAttempts = 3

    init(
        roomId: String?,
        messageId: String,
        messageType: String,
        messageData: Data,
        runtime: ChatRuntimeProtocol,
        tokenResolver: any WidgetDesignTokenResolving,
        logger: LoggerProtocol,
        retryDelay: Duration = .seconds(1)
    ) {
        self.roomId = roomId
        self.messageId = messageId
        self.messageType = messageType
        self.messageData = messageData
        self.runtime = runtime
        self.tokenResolver = tokenResolver
        self.logger = logger
        self.retryDelay = retryDelay
    }

    deinit {
        renderTask?.cancel()
    }

    @MainActor
    func bodyAppeared() {
        visibleBodies += 1
        guard renderTask == nil else { return }

        renderTask = makeRenderTask()
    }

    @MainActor
    func bodyDisappeared() {
        visibleBodies = max(0, visibleBodies - 1)
        guard visibleBodies == 0 else { return }

        renderTask?.cancel()
        renderTask = nil
    }
}

private extension ProductWidgetViewModel {
    func makeRenderTask() -> Task<Void, Never> {
        Task { [weak self, roomId, messageId, messageType, messageData, retryDelay, logger] in
            for attempt in 1 ... Self.renderAttempts {
                guard let self else { return }

                // Republishing `node` from this same instance is what reaches the
                // cell: it is observed, so nothing has to reconfigure it.
                let outcome = await draw(
                    roomId: roomId,
                    messageId: messageId,
                    messageType: messageType,
                    messageData: messageData
                )
                switch outcome {
                case .cancelled,
                     .ended(drew: true):
                    return
                case let .failed(error) where error is CancellationError:
                    // The runtime is disposed: every reopen would fail the same way.
                    return
                case .ended(drew: false),
                     .failed:
                    break
                }

                if attempt < Self.renderAttempts {
                    try? await Task.sleep(for: retryDelay)
                }
            }

            logger.error("Widget render gave up after \(Self.renderAttempts) streams: \(messageId)")
        }
    }

    enum RenderOutcome {
        /// The stream closed on its own. `drew` says whether it produced a tree
        /// first — a tree the host cannot map counts, since reopening maps it the
        /// same way. A body that never arrived is worth another stream.
        case ended(drew: Bool)
        /// The stream threw. Whatever it drew stays on screen while the reopen
        /// runs, so a mid-stream drop does not blank the cell.
        case failed(Error)
        /// This view model was released; nothing more to do.
        case cancelled
    }

    /// One render, from a fresh stream.
    func draw(
        roomId: String?,
        messageId: String,
        messageType: String,
        messageData: Data
    ) async -> RenderOutcome {
        let stream = await runtime.renderMessage(
            roomId: roomId,
            messageId: messageId,
            messageType: messageType,
            messageData: messageData
        )

        var drew = false
        do {
            for try await output in stream {
                guard !Task.isCancelled else { return .cancelled }

                let resolved: CustomMessageWidgetNode? =
                    switch output {
                    case let .scaleEncoded(hexString):
                        try ScaleWidget.decode(from: hexString)
                            .toWidgetNode(resolver: tokenResolver)
                    case let .native(node):
                        node.toWidgetNode(resolver: tokenResolver)
                    }
                await MainActor.run { self.node = resolved }
                drew = true
                #if targetEnvironment(simulator)
                    TrUAPIE2EMarkers.write("custom-renderer-update", logger: logger)
                #endif
            }
        } catch {
            guard !Task.isCancelled else { return .cancelled }
            logger.error("Widget render stream failed for \(messageId): \(error)")
            return .failed(error)
        }
        return .ended(drew: drew)
    }
}
