import Foundation
import Kingfisher

final class PublicProfileAvatarImageCache: ImageCache, @unchecked Sendable {
    static let shared = PublicProfileAvatarImageCache(
        memoryStorage: ImageCache.default.memoryStorage,
        diskStorage: ImageCache.default.diskStorage
    )

    override func retrieveImage(
        forKey key: String,
        options: KingfisherParsedOptionsInfo,
        callbackQueue: CallbackQueue = .mainCurrentOrAsync,
        completionHandler: (@Sendable (Result<ImageCacheResult, KingfisherError>) -> Void)?
    ) {
        guard let completionHandler else { return }
        if let image = retrieveImageInMemoryCache(forKey: key, options: options) {
            callbackQueue.execute { completionHandler(.success(.memory(image))) }
            return
        }
        DispatchQueue.global(qos: .utility).async {
            // Kingfisher persists disk expiry as the file's modification date.
            let file = URL(fileURLWithPath: self.cachePath(
                forKey: key,
                processorIdentifier: options.processor.identifier,
                forcedExtension: options.forcedExtension
            ))
            let expiration = try? file.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate
            var options = options
            options.memoryCacheExpiration = .date(expiration ?? .distantPast)
            super.retrieveImage(
                forKey: key,
                options: options,
                callbackQueue: callbackQueue,
                completionHandler: completionHandler
            )
        }
    }
}
