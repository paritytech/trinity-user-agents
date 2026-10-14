import Foundation
import Kingfisher
import PolkadotUI
import UIKit

extension ChatImageLoader {
    /// Loads a product media source: an inline `data:` image, or an `https` URL through Kingfisher's cache.
    static func productMedia(logger: any LoggerProtocol & Sendable = Logger.shared) -> ChatImageLoader {
        ChatImageLoader { source in
            if let inline = InlineDataURL(source) {
                return UIImage(data: inline.data)
            }

            guard let url = URL(string: source), url.scheme?.lowercased() == "https" else {
                logger.warning("Product media has no supported address")
                return nil
            }

            do {
                return try await KingfisherManager.shared.retrieveImage(with: url).image
            } catch {
                guard !Task.isCancelled else { return nil }
                logger.error("Product media failed to load: \(error)")
                return nil
            }
        }
    }
}
