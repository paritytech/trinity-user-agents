import Foundation
import Kingfisher
import PolkadotUI
import UIKit

extension WidgetImageResolver {
    /// Fetches the picture at the address `locate` gives a source, through
    /// Kingfisher's cache, and logs a source that cannot be located or loaded.
    static func cached(
        logger: any LoggerProtocol & Sendable = Logger.shared,
        locate: @escaping @Sendable (CustomMessageWidgetNode.ImageSource) async -> URL?
    ) -> WidgetImageResolver {
        WidgetImageResolver { source in
            guard let url = await locate(source) else {
                logger.error("Widget image \(source) has no address")
                return nil
            }
            do {
                return try await KingfisherManager.shared.retrieveImage(with: url).image
            } catch {
                guard !Task.isCancelled else { return nil }
                logger.error("Widget image \(source) failed to load from \(url): \(error)")
                return nil
            }
        }
    }
}
