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
    func immutableBytesHaveNoFreshnessDeadlineAndProcessedImagesStayInMemory() {
        let options = KingfisherParsedOptionsInfo(
            PublicProfileAvatarImageOptionsFactory().options(for: .originalImage, animated: false)
        )
        #expect((options.cacheMemoryOnly, options.cacheOriginalImage) == (true, false))
    }

    @Test
    func reopeningTheCacheAndChangingImageSizeReuseTheOriginalBytes() async throws {
        let name = UUID().uuidString
        let avatar = ProfileAvatar(cid: name, format: .png)
        let bytes = testPicturePNG(size: CGSize(width: 30, height: 20))
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let cache = PublicProfileAvatarImageCache(name: name)
        defer { try? cache.clear() }
        let first = makeProvider(avatar, cache: cache, resolver: resolver)
        let options = PublicProfileAvatarImageOptionsFactory().options(for: .originalImage, animated: false)
        _ = try await KingfisherManager.shared.retrieveImage(with: .provider(first), options: options)
        let file = ImageCache(name: name).diskStorage.cacheFileURL(forKey: avatar.cid)
        let expiration = try FileManager.default.attributesOfItem(atPath: file.path)[.modificationDate] as? Date
        #expect(try #require(expiration) > Date().addingTimeInterval(100 * 365 * 24 * 3_600))
        KingfisherManager.shared.cache.clearMemoryCache()
        let reopened = PublicProfileAvatarImageCache(name: name)
        let second = makeProvider(avatar, cache: reopened, resolver: resolver)
        let resized = try await KingfisherManager.shared.retrieveImage(
            with: .provider(second),
            options: PublicProfileAvatarImageOptionsFactory().options(
                for: ImageViewModelSettings(targetSize: CGSize(width: 10, height: 10)), animated: false
            )
        )
        #expect(await (resized.image.size, resolver.imageRequests) == (CGSize(width: 10, height: 10), [avatar]))
        #expect(try await second.data() == bytes)
    }

    @Test
    func aChangedDeclarationRejectsCachedBytesWithoutDownloadingAgain() async throws {
        let name = UUID().uuidString
        let avatar = ProfileAvatar(cid: name, format: .png)
        let resolver = MockPublicProfileAvatarResolver(result: .success(testPicturePNG(size: CGSize(
            width: 3,
            height: 2
        ))))
        let cache = PublicProfileAvatarImageCache(name: name)
        defer { try? cache.clear() }
        let first = makeProvider(avatar, cache: cache, resolver: resolver)
        _ = try await first.data()
        let declaredJPEG = ProfileAvatar(cid: name, format: .jpeg)
        let second = makeProvider(declaredJPEG, cache: PublicProfileAvatarImageCache(name: name), resolver: resolver)

        await #expect(throws: ImageError.Rejected(reason: "declared image format does not match cached bytes")) {
            try await second.data()
        }
        #expect(first.cacheKey != second.cacheKey)
        #expect(await resolver.imageRequests == [avatar])
    }

    @Test
    func evictedBytesAreLoadedAgain() async throws {
        let avatar = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let resolver = MockPublicProfileAvatarResolver(result: .success(testPicturePNG(size: CGSize(
            width: 3,
            height: 2
        ))))
        let cache = PublicProfileAvatarImageCache(name: avatar.cid)
        defer { try? cache.clear() }
        let provider = makeProvider(avatar, cache: cache, resolver: resolver)
        _ = try await provider.data()
        try cache.clear()
        _ = try await provider.data()
        #expect(await resolver.imageRequests == [avatar, avatar])
    }

    @Test
    func aCacheWriteFailureStillDisplaysTheVerifiedImage() async throws {
        let name = UUID().uuidString
        let avatar = ProfileAvatar(cid: name, format: .png)
        let bytes = testPicturePNG(size: CGSize(width: 3, height: 2))
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let cache = PublicProfileAvatarImageCache(name: name)
        let directory = ImageCache(name: name).diskStorage.cacheFileURL(forKey: name).deletingLastPathComponent()
        try FileManager.default.removeItem(at: directory)
        try Data().write(to: directory)
        defer { try? cache.clear() }
        let provider = makeProvider(avatar, cache: cache, resolver: resolver)

        #expect(try await provider.data() == bytes)
        #expect(await resolver.imageRequests == [avatar])
    }

    private func makeProvider(
        _ avatar: ProfileAvatar,
        cache: PublicProfileAvatarImageCache,
        resolver: MockPublicProfileAvatarResolver
    ) -> PublicProfileAvatarImageDataProvider {
        PublicProfileAvatarImageDataProvider(avatar: avatar, cache: cache) { avatar in
            try await resolver.loadAvatar(avatar)
        }
    }
}
