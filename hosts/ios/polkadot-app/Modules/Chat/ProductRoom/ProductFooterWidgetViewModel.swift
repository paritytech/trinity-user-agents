import Foundation
import PolkadotUI
import Products

/// The tree of the custom footer of a product room, as the product draws it (P6.6, 02-DESIGN §6.4).
///
/// The view model opens the render stream once and keeps the newest tree. A stream that ends before it drew is
/// opened again, at most three times, as for a custom message.
@Observable
final class ProductFooterWidgetViewModel: WidgetNodeProviding {
    @MainActor private(set) var node: CustomMessageWidgetNode?

    @ObservationIgnored private var renderTask: Task<Void, Never>?

    private static let renderAttempts = 3

    init(
        roomId: String,
        renderer: ProductRoomFooterRendering,
        tokenResolver: any WidgetDesignTokenResolving = WidgetDesignTokenResolver(),
        logger: LoggerProtocol = Logger.shared,
        retryDelay: Duration = .seconds(1)
    ) {
        renderTask = Task { [weak self] in
            for attempt in 1 ... Self.renderAttempts {
                var drew = false
                do {
                    for try await output in await renderer.renderFooter(roomId: roomId) {
                        guard !Task.isCancelled else { return }
                        let resolved: CustomMessageWidgetNode? =
                            switch output {
                            case let .scaleEncoded(hexString):
                                try ScaleWidget.decode(from: hexString).toWidgetNode(resolver: tokenResolver)
                            case let .native(node):
                                node.toWidgetNode(resolver: tokenResolver)
                            }
                        await MainActor.run { self?.node = resolved }
                        drew = true
                    }
                } catch {
                    guard !Task.isCancelled, !(error is CancellationError) else { return }
                    logger.error("Footer render of room \(roomId) failed: \(error)")
                }

                guard !drew, attempt < Self.renderAttempts, self != nil else { return }
                try? await Task.sleep(for: retryDelay)
            }
        }
    }

    deinit {
        renderTask?.cancel()
    }
}
