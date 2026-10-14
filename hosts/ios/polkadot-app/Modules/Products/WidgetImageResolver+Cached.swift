import Foundation
import Kingfisher
import PolkadotUI
import UIKit

extension WidgetImageResolver {
    /// Fetches the picture at the address `locate` gives a source, through the
    /// chat resource cache, and logs a source that cannot be located or loaded.
    /// A source that `options` gives Kingfisher options for loads through
    /// Kingfisher with those options instead.
    static func cached(
        resources: ChatResourceResolving = ChatResourceResolver.shared,
        logger: any LoggerProtocol & Sendable = Logger.shared,
        options: @escaping @Sendable (CustomMessageWidgetNode.ImageSource) -> KingfisherOptionsInfo = { _ in [] },
        locate: @escaping @Sendable (CustomMessageWidgetNode.ImageSource) async -> URL?
    ) -> WidgetImageResolver {
        WidgetImageResolver { source in
            guard let url = await locate(source) else {
                logger.error("Widget image \(source) has no address")
                return nil
            }

            let kingfisherOptions = options(source)
            guard kingfisherOptions.isEmpty else {
                do {
                    return try await InFlightImages.shared.image(at: url) {
                        try await KingfisherManager.shared.retrieveImage(with: url, options: kingfisherOptions).image
                    }
                } catch {
                    guard !Task.isCancelled else { return nil }
                    logger.error("Widget image \(source) failed to load from \(url): \(error)")
                    return nil
                }
            }

            guard let image = await resources.image(for: .address(url)) else {
                guard !Task.isCancelled else { return nil }
                logger.error("Widget image \(source) failed to load from \(url)")
                return nil
            }

            return image
        }
    }
}

/// Kingfisher's async retrieve never returns for the second of two overlapping
/// loads of one address, so overlapping loads share the first one. Kingfisher
/// 8.6.1 fixes this (onevcat/Kingfisher#2444); the app pins 8.2.0.
actor InFlightImages {
    static let shared = InFlightImages()

    private var loads: [URL: Task<UIImage, Error>] = [:]

    func image(at url: URL, load: @escaping @Sendable () async throws -> UIImage) async throws -> UIImage {
        if let running = loads[url] { return try await running.value }

        let task = Task { try await load() }
        loads[url] = task
        defer { loads[url] = nil }
        return try await task.value
    }
}
