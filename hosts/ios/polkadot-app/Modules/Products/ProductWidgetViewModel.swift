import Foundation
import PolkadotUI
import Products

@Observable
final class ProductWidgetViewModel: WidgetNodeProviding {
    @MainActor private(set) var node: CustomMessageWidgetNode?

    private let messageId: String
    private let runtime: ChatRuntimeProtocol
    private let tokenResolver: any WidgetDesignTokenResolving
    private let logger: LoggerProtocol
    /// Not private: tests await this instead of a wall clock. Under a loaded
    /// runner a fixed sleep expires before the loop reopens the stream, which
    /// reads as "never retried" even though the retry is on its way.
    private(set) var renderTask: Task<Void, Never>?
    private let retryDelay: Duration

    /// Streams in a row that end or fail without drawing, after which the body
    /// is given up. The runtime retries the throws that precede a stream; this
    /// covers a stream that fails, or that ends before drawing. Without it the
    /// cell keeps whatever it last showed — a spinner, or a body the product
    /// can no longer update — for as long as the decoder caches this view
    /// model, which is the bot's lifetime. A stream that drew starts the count
    /// again: every resume that closes the product connection fails each open
    /// render, and each has to be opened again however many the cell has seen.
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
        self.messageId = messageId
        self.runtime = runtime
        self.tokenResolver = tokenResolver
        self.logger = logger
        self.retryDelay = retryDelay

        renderTask = Task { [weak self] in
            guard let self else { return }

            var emptyStreamsInARow = 0
            while true {
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
                case let .failed(error, _) where error is CancellationError:
                    // The runtime is disposed: every reopen would fail the same way.
                    return
                case let .failed(_, drew):
                    emptyStreamsInARow = drew ? 0 : emptyStreamsInARow + 1
                case .ended(drew: false):
                    emptyStreamsInARow += 1
                }

                guard emptyStreamsInARow < Self.renderAttempts else { break }
                try? await Task.sleep(for: retryDelay)
            }

            logger.error("Widget render gave up after \(Self.renderAttempts) streams drew nothing: \(messageId)")
        }
    }

    deinit {
        renderTask?.cancel()
    }
}

private extension ProductWidgetViewModel {
    enum RenderOutcome {
        /// The stream closed on its own. `drew` says whether it produced a tree
        /// first — a tree the host cannot map counts, since reopening maps it the
        /// same way. A body that never arrived is worth another stream.
        case ended(drew: Bool)
        /// The stream threw. Whatever it drew stays on screen while the reopen
        /// runs, so a mid-stream drop does not blank the cell. `drew` says
        /// whether it produced a tree before throwing.
        case failed(Error, drew: Bool)
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
            return .failed(error, drew: drew)
        }
        return .ended(drew: drew)
    }
}
