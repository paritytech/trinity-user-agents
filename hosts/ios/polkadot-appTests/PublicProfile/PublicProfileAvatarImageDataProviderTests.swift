import Foundation
import Kingfisher
import PolkadotUI
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

@MainActor
struct PublicProfileAvatarImageDataProviderTests {
    @Test
    func kingfisherReopensOriginalAndProcessedImagesWithoutCallingTheProvider() async throws {
        let name = UUID().uuidString
        let avatar = ProfileAvatar(cid: name, format: .png)
        let bytes = testPicturePNG(size: CGSize(width: 40, height: 20))
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let cache = ImageCache(name: name)
        defer { cache.clearCache(completion: nil) }
        let provider = makeProvider(avatar, resolver: resolver)
        _ = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider), options: options(for: avatar, cache: cache)
        )
        cache.clearMemoryCache()
        let reopened = ImageCache(name: name)
        let original = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider), options: options(for: avatar, cache: reopened) + [.onlyFromCache]
        )
        let resized = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider),
            options: options(
                for: avatar, cache: reopened,
                settings: ImageViewModelSettings(targetSize: CGSize(width: 10, height: 10))
            )
        )
        let originalFile = reopened.diskStorage.cacheFileURL(forKey: avatar.cid)
        let expiration = try FileManager.default.attributesOfItem(atPath: originalFile.path)[.modificationDate] as? Date
        #expect(try #require(expiration) > Date().addingTimeInterval(100 * 365 * 24 * 3_600))
        #expect(try Data(contentsOf: originalFile) == bytes)
        #expect(await (original.image.cgImage?.width, resized.image.size, resolver.imageRequests) == (
            40, CGSize(width: 10, height: 10), [avatar]
        ))
    }

    @Test
    func roundedJPEGCornersStayTransparentAfterReopeningTheCache() async throws {
        let name = UUID().uuidString
        let avatar = ProfileAvatar(cid: name, format: .jpeg)
        let image = try #require(UIImage(data: testPicturePNG(size: CGSize(width: 40, height: 40))))
        let bytes = try #require(image.jpegData(compressionQuality: 1))
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let cache = ImageCache(name: name)
        defer { cache.clearCache(completion: nil) }
        let provider = makeProvider(avatar, resolver: resolver)
        let settings = ImageViewModelSettings(targetSize: CGSize(width: 20, height: 20), cornerRadius: 10)
        let first = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider), options: options(for: avatar, cache: cache, settings: settings)
        ).image
        cache.clearMemoryCache()
        let reopened = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider),
            options: options(for: avatar, cache: ImageCache(name: name), settings: settings) + [.onlyFromCache]
        ).image
        #expect(try await (cornerAlpha(first), cornerAlpha(reopened), resolver.imageRequests) == (0, 0, [avatar]))
        #expect(try Data(contentsOf: cache.diskStorage.cacheFileURL(forKey: avatar.cid)) == bytes)
    }

    @Test
    func aChangedDeclarationRejectsCachedBytesWithoutDownloadingAgain() async throws {
        let name = UUID().uuidString
        let avatar = ProfileAvatar(cid: name, format: .png)
        let resolver = MockPublicProfileAvatarResolver(result: .success(testPicturePNG(size: CGSize(
            width: 3,
            height: 2
        ))))
        let cache = ImageCache(name: name)
        defer { cache.clearCache(completion: nil) }
        _ = try await KingfisherManager.shared.retrieveImage(
            with: .provider(makeProvider(avatar, resolver: resolver)), options: options(for: avatar, cache: cache)
        )
        cache.clearMemoryCache()
        let declaredJPEG = ProfileAvatar(cid: name, format: .jpeg)
        let reopened = ImageCache(name: name)

        await #expect {
            try await KingfisherManager.shared.retrieveImage(
                with: .provider(makeProvider(declaredJPEG, resolver: resolver)),
                options: options(for: declaredJPEG, cache: reopened)
            )
        } throws: { error in
            guard let error = error as? KingfisherError else { return false }
            if case .processorError = error { return true }
            return false
        }
        #expect(await resolver.imageRequests == [avatar])
    }

    private func cornerAlpha(_ image: UIImage) throws -> UInt8 {
        let cgImage = try #require(image.cgImage)
        var pixel = [UInt8](repeating: 0, count: 4)
        try pixel.withUnsafeMutableBytes { buffer in
            let context = try #require(CGContext(
                data: buffer.baseAddress, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue
            ))
            context.draw(
                cgImage,
                in: CGRect(x: 0, y: 0, width: CGFloat(cgImage.width), height: CGFloat(cgImage.height))
            )
        }
        return pixel[3]
    }

    private func makeProvider(
        _ avatar: ProfileAvatar,
        resolver: MockPublicProfileAvatarResolver
    ) -> PublicProfileAvatarImageDataProvider {
        PublicProfileAvatarImageDataProvider(avatar: avatar) { avatar in
            try await resolver.loadAvatar(avatar)
        }
    }

    private func options(
        for avatar: ProfileAvatar,
        cache: ImageCache,
        settings: ImageViewModelSettings = .originalImage
    ) -> KingfisherOptionsInfo {
        PublicProfileAvatarImageOptionsFactory(format: avatar.format, cache: cache)
            .options(for: settings, animated: false) + [.waitForCache]
    }
}
