import Foundation
import PolkadotUI
import Testing
import UIKit
@testable import polkadot_app

@MainActor
struct PublicProfileAvatarViewModelFactoryTests {
    @Test
    func loadsTheRequestedPersonsAvatarThroughTheExistingImageViewModel() async {
        let username = "factory-\(UUID().uuidString)"
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2)))
        )
        let factory = PublicProfileAvatarViewModelFactory { username, force in
            try await resolver.resolveAvatar(username: username, force: force)
        }
        let imageView = UIImageView()
        let viewModel = factory.createViewModel(for: username, refresh: false)

        let succeeded = await load(viewModel, on: imageView)

        #expect(succeeded)
        #expect(imageView.image != nil)
        #expect(await resolver.requestedUsernames == [username])
    }

    @Test
    func missingAvatarClearsThePreviousPersonsImage() async {
        let imageView = UIImageView(image: UIImage(systemName: "person.fill"))
        let factory = PublicProfileAvatarViewModelFactory(resolveAvatar: { _, _ in nil })
        let viewModel = factory.createViewModel(for: "missing-\(UUID().uuidString)", refresh: false)

        let succeeded = await load(viewModel, on: imageView)

        #expect(!succeeded)
        #expect(imageView.image == nil)
    }

    /// Opening a chat rereads the profile, and later loads find the new avatar in the cache.
    @Test
    func aRefreshReplacesTheCachedAvatar() async {
        let username = "refresh-\(UUID().uuidString)"
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2)))
        )
        let factory = makeFactory(resolver: resolver)
        _ = await load(factory.createViewModel(for: username, refresh: false), on: UIImageView())
        await resolver.setResult(.success(testPicturePNG(size: CGSize(width: 5, height: 4))))
        let refreshed = UIImageView()
        let later = UIImageView()

        _ = await load(factory.createViewModel(for: username, refresh: true), on: refreshed)
        _ = await load(factory.createViewModel(for: username, refresh: false), on: later)
        let requests = await resolver.requests

        #expect(
            ([pixelSize(of: refreshed), pixelSize(of: later)], requests) == (
                [CGSize(width: 5, height: 4), CGSize(width: 5, height: 4)],
                [.init(username: username, force: false), .init(username: username, force: true)]
            )
        )
    }

    /// The chat header loads its avatar again on every chat update, which must not reread the profile each time.
    @Test
    func loadsDuringARefreshReadTheProfileOnce() async {
        let username = "refresh-once-\(UUID().uuidString)"
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2)))
        )
        let viewModel = makeFactory(resolver: resolver).createViewModel(for: username, refresh: true)
        let imageView = UIImageView()

        let refreshed = await withCheckedContinuation { continuation in
            viewModel.loadImage(on: imageView, settings: .originalImage, animated: false) { succeeded in
                continuation.resume(returning: succeeded)
            }
            viewModel.loadImage(on: imageView, settings: .originalImage, animated: false, completion: nil)
        }
        let reloaded = await load(viewModel, on: imageView)
        let requests = await resolver.requests

        #expect((refreshed, reloaded, requests) == (true, true, [.init(username: username, force: true)]))
    }

    /// A refresh draws the cached avatar while it loads, and a failed one, for example offline, keeps it.
    @Test
    func aRefreshKeepsTheCachedAvatarUntilItLoads() async {
        let username = "refresh-offline-\(UUID().uuidString)"
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2)))
        )
        let factory = makeFactory(resolver: resolver)
        _ = await load(factory.createViewModel(for: username, refresh: false), on: UIImageView())
        await resolver.setResult(.failure(.offline))
        let viewModel = factory.createViewModel(for: username, refresh: true)
        let imageView = UIImageView()
        var whileLoading: CGSize?

        let refreshed = await withCheckedContinuation { continuation in
            viewModel.loadImage(on: imageView, settings: .originalImage, animated: false) { succeeded in
                continuation.resume(returning: succeeded)
            }
            whileLoading = pixelSize(of: imageView)
        }

        #expect(
            (refreshed, whileLoading, pixelSize(of: imageView)) ==
                (false, CGSize(width: 3, height: 2), CGSize(width: 3, height: 2))
        )
    }
}

private extension PublicProfileAvatarViewModelFactoryTests {
    func makeFactory(resolver: MockPublicProfileAvatarResolver) -> PublicProfileAvatarViewModelFactory {
        PublicProfileAvatarViewModelFactory { username, force in
            try await resolver.resolveAvatar(username: username, force: force)
        }
    }

    func pixelSize(of imageView: UIImageView) -> CGSize? {
        imageView.image.map { CGSize(width: $0.size.width * $0.scale, height: $0.size.height * $0.scale) }
    }

    func load(_ viewModel: any ImageViewModelProtocol, on imageView: UIImageView) async -> Bool {
        await withCheckedContinuation { continuation in
            viewModel.loadImage(on: imageView, settings: .originalImage, animated: false) { succeeded in
                continuation.resume(returning: succeeded)
            }
        }
    }
}
