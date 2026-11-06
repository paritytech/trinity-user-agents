import Foundation
import Kingfisher
import PolkadotUI
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

struct PublicProfileAvatarImageDataProviderTests {
    @Test
    func returnsVerifiedBytesForTheRequestedUsername() async throws {
        let bytes = Data([1, 2, 3])
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let provider = makeProvider(username: "alice.01", resolver: resolver)

        let loaded = try await provider.data()

        #expect(loaded == bytes)
        #expect(await resolver.requestedUsernames == ["alice.01"])
    }

    @Test
    func noAvatarLeavesThePlaceholderAndCanBeRetried() async throws {
        let resolver = MockPublicProfileAvatarResolver(result: .failure(.Unavailable(reason: "no profile avatar")))
        let provider = makeProvider(username: "alice.01", resolver: resolver)

        await #expect(throws: ImageError.Unavailable(reason: "no profile avatar")) {
            try await provider.data()
        }

        let bytes = Data([1, 2, 3])
        await resolver.setResult(.success(bytes))

        #expect(try await provider.data() == bytes)
        #expect(await resolver.requestedUsernames == ["alice.01", "alice.01"])
    }

    @Test
    func lookupFailureLeavesThePlaceholderAndCanBeRetried() async throws {
        let resolver = MockPublicProfileAvatarResolver(result: .failure(.Unavailable(reason: "offline")))
        let provider = makeProvider(username: "alice.01", resolver: resolver)

        await #expect(throws: ImageError.Unavailable(reason: "offline")) {
            try await provider.data()
        }

        let bytes = Data([4, 5, 6])
        await resolver.setResult(.success(bytes))

        #expect(try await provider.data() == bytes)
        #expect(await resolver.requestedUsernames == ["alice.01", "alice.01"])
    }

    @Test
    func cacheKeysShareOneUsernameWithoutMixingDifferentPeople() {
        let first = PublicProfileAvatarImageDataProvider(username: "alice.01") { _, _ in
            throw ImageError.Unavailable(reason: "no profile avatar")
        }
        let repeated = PublicProfileAvatarImageDataProvider(username: "alice.01") { _, _ in
            throw ImageError.Unavailable(reason: "no profile avatar")
        }
        let other = PublicProfileAvatarImageDataProvider(username: "bob.02") { _, _ in
            throw ImageError.Unavailable(reason: "no profile avatar")
        }

        #expect(first.cacheKey == repeated.cacheKey)
        #expect(first.cacheKey != other.cacheKey)
    }

    /// A forced load stores under the cache key of an ordinary one, so its image replaces the cached avatar.
    @Test
    func aForcedLoadRereadsTheProfileUnderTheSameCacheKey() async throws {
        let resolver = MockPublicProfileAvatarResolver(result: .success(Data([1, 2, 3])))
        let forced = PublicProfileAvatarImageDataProvider(username: "alice.01", force: true) { username, force in
            try await resolver.loadAvatar(username: username, force: force)
        }

        _ = try await forced.data()
        let requests = await resolver.requests
        let expected = [MockPublicProfileAvatarResolver.Request(username: "alice.01", force: true)]

        #expect((requests, forced.cacheKey) == (expected, "public-profile:alice.01"))
    }

    @MainActor
    @Test
    func cachedAvatarsExpireWithoutExtendingTheirLifetimeOnAccess() {
        let options = KingfisherParsedOptionsInfo(
            PublicProfileAvatarImageOptionsFactory().options(for: .originalImage, animated: false)
        )

        #expect(
            (options.cacheMemoryOnly, options.memoryCacheExpiration, options.diskCacheExpiration) ==
                (
                    false,
                    Optional.some(StorageExpiration.seconds(3_600)),
                    Optional.some(StorageExpiration.seconds(3_600))
                )
        )
        #expect(
            (options.memoryCacheAccessExtendingExpiration, options.diskCacheAccessExtendingExpiration) ==
                (ExpirationExtending.none, ExpirationExtending.none)
        )
    }

    @MainActor
    @Test
    func kingfisherReusesTheAvatarAfterTheCacheIsReopened() async throws {
        let username = "avatar-cache-\(UUID().uuidString)"
        let bytes = testPicturePNG(size: CGSize(width: 3, height: 2))
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let provider = makeProvider(username: username, resolver: resolver)
        let cache = PublicProfileAvatarImageCache(name: username)
        defer {
            cache.clearMemoryCache()
            cache.clearDiskCache()
        }
        let options = cacheOptions(cache)

        let loaded = try await KingfisherManager.shared.retrieveImage(with: .provider(provider), options: options)
        let cached = try await KingfisherManager.shared.retrieveImage(with: .provider(provider), options: options)
        cache.clearMemoryCache()
        let cacheFile = URL(fileURLWithPath: cache.cachePath(
            forKey: provider.cacheKey,
            processorIdentifier: KingfisherParsedOptionsInfo(options).processor.identifier
        ))
        let expiration = try FileManager.default.attributesOfItem(atPath: cacheFile.path)[.modificationDate] as? Date

        let reopened = PublicProfileAvatarImageCache(name: username)
        let restored = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider),
            options: options + [.targetCache(reopened)]
        )

        let reused = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider),
            options: cacheOptions(reopened)
        )
        let expirationAfterReads = try FileManager.default
            .attributesOfItem(atPath: cacheFile.path)[.modificationDate] as? Date

        let cacheTypes = [loaded, cached, restored, reused].map(\.cacheType)
        #expect(cacheTypes == [.none, .memory, .disk, .memory])
        #expect(await resolver.requestedUsernames == [username])
        #expect(expirationAfterReads == expiration)
    }

    @MainActor
    @Test
    func anExpiredAvatarIsResolvedAgainAfterRestart() async throws {
        let username = "avatar-expiry-\(UUID().uuidString)"
        let bytes = testPicturePNG(size: CGSize(width: 3, height: 2))
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let provider = makeProvider(username: username, resolver: resolver)
        let cache = PublicProfileAvatarImageCache(name: username)
        defer { cache.clearDiskCache() }
        let options = cacheOptions(cache)
        _ = try await KingfisherManager.shared.retrieveImage(with: .provider(provider), options: options)
        try expireDiskImage(provider, cache: cache, options: options)
        await resolver.setResult(.success(testPicturePNG(size: CGSize(width: 5, height: 4))))

        let reopened = PublicProfileAvatarImageCache(name: username)
        let reloaded = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider),
            options: cacheOptions(reopened)
        )

        #expect((reloaded.cacheType, reloaded.image.cgImage?.width) == (.none, 5))
        #expect(await resolver.requestedUsernames == [username, username])
    }

    @MainActor
    @Test
    func aDelayedDiskReadKeepsItsDeadlineWhenARefreshReplacesTheFile() async throws {
        let username = "avatar-promotion-\(UUID().uuidString)"
        let bytes = testPicturePNG(size: CGSize(width: 3, height: 2))
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let provider = makeProvider(username: username, resolver: resolver)
        let cache = PublicProfileAvatarImageCache(name: username)
        defer { cache.clearDiskCache() }
        let options = cacheOptions(cache)
        _ = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider),
            options: options + [.diskCacheExpiration(.seconds(60))]
        )
        cache.clearMemoryCache()
        let file = URL(fileURLWithPath: cache.cachePath(
            forKey: provider.cacheKey,
            processorIdentifier: KingfisherParsedOptionsInfo(options).processor.identifier
        ))
        let deadline = try #require(FileManager.default.attributesOfItem(atPath: file.path)[.modificationDate] as? Date)
        let callbackQueue = DispatchQueue(label: "avatar-promotion-test")
        let releaseCallback = DispatchSemaphore(value: 0)
        callbackQueue.async { releaseCallback.wait() }
        defer { releaseCallback.signal() }
        let (decoded, decodedContinuation) = AsyncStream<StorageExpiration?>.makeStream()
        var readOptions = KingfisherParsedOptionsInfo(options)
        readOptions.cacheSerializer = ExpirationObservingSerializer(base: readOptions.cacheSerializer) { expiration in
            decodedContinuation.yield(expiration)
            decodedContinuation.finish()
        }
        let capturedOptions = readOptions
        let diskRead = Task {
            try await withCheckedThrowingContinuation { continuation in
                cache.retrieveImage(
                    forKey: provider.cacheKey,
                    options: capturedOptions,
                    callbackQueue: .dispatch(callbackQueue)
                ) {
                    continuation.resume(with: $0)
                }
            }
        }
        var observations = decoded.makeAsyncIterator()
        let capturedExpiration = await observations.next() ?? nil
        await resolver.setResult(.success(testPicturePNG(size: CGSize(width: 5, height: 4))))
        _ = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider),
            options: options + [.forceRefresh]
        )
        let replacementDeadline = try #require(file.resourceValues(forKeys: [.contentModificationDateKey])
            .contentModificationDate)
        releaseCallback.signal()
        let delayed = try await diskRead.value

        #expect(capturedExpiration == .date(deadline))
        #expect(replacementDeadline > deadline)
        #expect((delayed.cacheType, delayed.image?.cgImage?.width) == (.disk, 3))
    }

    @MainActor
    @Test
    func aForcedRefreshReplacesThePersistedAvatar() async throws {
        let username = "avatar-forced-disk-\(UUID().uuidString)"
        let bytes = testPicturePNG(size: CGSize(width: 3, height: 2))
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let provider = makeProvider(username: username, resolver: resolver)
        let forced = PublicProfileAvatarImageDataProvider(username: username, force: true) { username, force in
            try await resolver.loadAvatar(username: username, force: force)
        }
        let cache = PublicProfileAvatarImageCache(name: username)
        defer { cache.clearDiskCache() }
        let options = cacheOptions(cache)
        let loaded = try await KingfisherManager.shared.retrieveImage(with: .provider(provider), options: options)
        await resolver.setResult(.success(testPicturePNG(size: CGSize(width: 5, height: 4))))

        let refreshed = try await KingfisherManager.shared.retrieveImage(
            with: .provider(forced),
            options: options + [.forceRefresh]
        )
        let reopened = PublicProfileAvatarImageCache(name: username)
        let restored = try await KingfisherManager.shared.retrieveImage(
            with: .provider(provider),
            options: cacheOptions(reopened)
        )
        let results = [loaded, refreshed, restored]
        let requests = await resolver.requests

        #expect(results.map(\.cacheType) == [.none, .none, .disk])
        #expect(
            (results.map { $0.image.cgImage?.width }, requests) == (
                [3, 5, 5],
                [.init(username: username, force: false), .init(username: username, force: true)]
            )
        )
    }
}

private extension PublicProfileAvatarImageDataProviderTests {
    @MainActor
    func cacheOptions(_ cache: ImageCache) -> KingfisherOptionsInfo {
        PublicProfileAvatarImageOptionsFactory().options(for: .originalImage, animated: false) +
            [.targetCache(cache), .waitForCache]
    }

    @MainActor
    func expireDiskImage(
        _ provider: PublicProfileAvatarImageDataProvider,
        cache: ImageCache,
        options: KingfisherOptionsInfo
    ) throws {
        let path = cache.cachePath(
            forKey: provider.cacheKey,
            processorIdentifier: KingfisherParsedOptionsInfo(options).processor.identifier
        )
        try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSince1970: 1)], ofItemAtPath: path)
    }

    func makeProvider(
        username: String,
        resolver: MockPublicProfileAvatarResolver
    ) -> PublicProfileAvatarImageDataProvider {
        PublicProfileAvatarImageDataProvider(username: username) { username, force in
            try await resolver.loadAvatar(username: username, force: force)
        }
    }
}

private struct ExpirationObservingSerializer: CacheSerializer {
    let base: any CacheSerializer
    let didDecode: @Sendable (StorageExpiration?) -> Void

    func data(with image: UIImage, original: Data?) -> Data? {
        base.data(with: image, original: original)
    }

    func image(with data: Data, options: KingfisherParsedOptionsInfo) -> UIImage? {
        let image = base.image(with: data, options: options)
        didDecode(options.memoryCacheExpiration)
        return image
    }
}
