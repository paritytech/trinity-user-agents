import Foundation
import Kingfisher
import PolkadotUI
import Testing
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
        let resolver = MockPublicProfileAvatarResolver(result: .success(nil))
        let provider = makeProvider(username: "alice.01", resolver: resolver)

        await #expect(throws: PublicProfileAvatarImageError.unavailable) {
            try await provider.data()
        }

        let bytes = Data([1, 2, 3])
        await resolver.setResult(.success(bytes))

        #expect(try await provider.data() == bytes)
        #expect(await resolver.requestedUsernames == ["alice.01", "alice.01"])
    }

    @Test
    func lookupFailureLeavesThePlaceholderAndCanBeRetried() async throws {
        let resolver = MockPublicProfileAvatarResolver(result: .failure(.offline))
        let provider = makeProvider(username: "alice.01", resolver: resolver)

        await #expect(throws: MockPublicProfileAvatarResolver.Failure.offline) {
            try await provider.data()
        }

        let bytes = Data([4, 5, 6])
        await resolver.setResult(.success(bytes))

        #expect(try await provider.data() == bytes)
        #expect(await resolver.requestedUsernames == ["alice.01", "alice.01"])
    }

    @Test
    func cacheKeysShareOneUsernameWithoutMixingDifferentPeople() {
        let first = PublicProfileAvatarImageDataProvider(username: "alice.01", resolveAvatar: { _, _ in nil })
        let repeated = PublicProfileAvatarImageDataProvider(username: "alice.01", resolveAvatar: { _, _ in nil })
        let other = PublicProfileAvatarImageDataProvider(username: "bob.02", resolveAvatar: { _, _ in nil })

        #expect(first.cacheKey == repeated.cacheKey)
        #expect(first.cacheKey != other.cacheKey)
    }

    /// A forced load stores under the cache key of an ordinary one, so its image replaces the cached avatar.
    @Test
    func aForcedLoadRereadsTheProfileUnderTheSameCacheKey() async throws {
        let resolver = MockPublicProfileAvatarResolver(result: .success(Data([1, 2, 3])))
        let forced = PublicProfileAvatarImageDataProvider(username: "alice.01", force: true) { username, force in
            try await resolver.resolveAvatar(username: username, force: force)
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
            (options.cacheMemoryOnly, options.memoryCacheExpiration, options.memoryCacheAccessExtendingExpiration) ==
                (true, Optional.some(StorageExpiration.seconds(3_600)), ExpirationExtending.none)
        )
    }

    @MainActor
    @Test
    func kingfisherReusesTheAvatarInMemoryAndDoesNotKeepItOnDisk() async throws {
        let username = "avatar-cache-\(UUID().uuidString)"
        let bytes = testPicturePNG(size: CGSize(width: 3, height: 2))
        let resolver = MockPublicProfileAvatarResolver(result: .success(bytes))
        let provider = makeProvider(username: username, resolver: resolver)
        let cache = ImageCache(name: username)
        defer {
            cache.clearMemoryCache()
            cache.clearDiskCache()
        }
        let options = PublicProfileAvatarImageOptionsFactory().options(for: .originalImage, animated: false) +
            [.targetCache(cache), .waitForCache]

        let loaded = try await KingfisherManager.shared.retrieveImage(with: .provider(provider), options: options)
        let cached = try await KingfisherManager.shared.retrieveImage(with: .provider(provider), options: options)
        cache.clearMemoryCache()

        #expect([loaded.cacheType, cached.cacheType] == [.none, .memory])
        #expect(await resolver.requestedUsernames == [username])
        #expect([
            cache.imageCachedType(
                forKey: provider.cacheKey,
                processorIdentifier: KingfisherParsedOptionsInfo(options).processor.identifier
            ),
            cache.imageCachedType(forKey: provider.cacheKey)
        ] == [.none, .none])
    }
}

private extension PublicProfileAvatarImageDataProviderTests {
    func makeProvider(
        username: String,
        resolver: MockPublicProfileAvatarResolver
    ) -> PublicProfileAvatarImageDataProvider {
        PublicProfileAvatarImageDataProvider(username: username) { username, force in
            try await resolver.resolveAvatar(username: username, force: force)
        }
    }
}
