import Foundation
@testable import PolkadotUI
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

@MainActor
struct PublicProfileAvatarViewModelFactoryTests {
    @Test(arguments: [false, true])
    func cachedAvatarIsDisplayedWhileForcedMetadataIsPending(animated: Bool) async {
        let username = UUID().uuidString
        let avatar = ProfileAvatar(cid: username, format: .png)
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2))), avatar: avatar
        )
        _ = await load(makeFactory(resolver).createViewModel(for: username, refresh: false), on: UIImageView())
        let gate = ProfileResolutionGate()
        let (started, didStart) = AsyncStream<Void>.makeStream()
        let (completed, didComplete) = AsyncStream<Bool>.makeStream()
        let factory = PublicProfileAvatarViewModelFactory(resolveProfile: { _, force in
            if force { await gate.wait { didStart.yield(()) } }
            return await resolver.profile
        }, loadAvatar: { avatar in
            try await resolver.loadAvatar(avatar)
        })
        let imageView = UIImageView()
        let viewModel = factory.createViewModel(for: username, refresh: true)
        var completions: [Bool] = []
        viewModel.loadImage(on: imageView, settings: .originalImage, animated: animated) {
            completions.append($0)
            didComplete.yield($0)
            didComplete.finish()
        }
        for await _ in started {
            break
        }
        #expect(imageView.image?.cgImage?.width == 3)
        #expect(completions.isEmpty)
        await gate.release()
        var results = completed.makeAsyncIterator()
        #expect(await results.next() == true)
        #expect(completions == [true])
        #expect(await resolver.imageRequests == [avatar])
    }

    @Test
    func failedRefreshPreservesTheCachedImage() async {
        let avatar = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2))), avatar: avatar
        )
        let factory = PublicProfileAvatarViewModelFactory(resolveProfile: { _, force in
            if force { throw ImageError.Unavailable(reason: "offline") }
            return await resolver.profile
        }, loadAvatar: { avatar in
            try await resolver.loadAvatar(avatar)
        })
        let imageView = UIImageView()

        let succeeded = await load(factory.createViewModel(for: "alice.01", refresh: true), on: imageView)

        #expect(await (succeeded, imageView.image?.cgImage?.width, resolver.imageRequests) == (true, 3, [avatar]))
    }

    @Test
    func changedAvatarIsDisplayedAfterTheCachedAvatar() async {
        let initial = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let refreshed = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2))), avatar: initial
        )
        let gate = ProfileResolutionGate()
        let (started, didStart) = AsyncStream<Void>.makeStream()
        let (completed, didComplete) = AsyncStream<Bool>.makeStream()
        let refreshedBytes = testPicturePNG(size: CGSize(width: 5, height: 4))
        let factory = PublicProfileAvatarViewModelFactory(resolveProfile: { _, force in
            if force {
                await gate.wait { didStart.yield(()) }
                await resolver.setAvatar(refreshed, bytes: refreshedBytes)
            }
            return await resolver.profile
        }, loadAvatar: { avatar in
            try await resolver.loadAvatar(avatar)
        })
        let imageView = UIImageView()
        let viewModel = factory.createViewModel(for: "alice.01", refresh: true)
        viewModel.loadImage(on: imageView, settings: .originalImage, animated: false) {
            didComplete.yield($0)
            didComplete.finish()
        }
        for await _ in started {
            break
        }
        #expect(imageView.image?.cgImage?.width == 3)
        await gate.release()
        var results = completed.makeAsyncIterator()
        #expect(await results.next() == true)
        #expect(await (imageView.image?.cgImage?.width, resolver.imageRequests) == (5, [initial, refreshed]))
    }

    @Test
    func removingTheAvatarDuringRefreshClearsTheCachedImage() async {
        let avatar = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2))), avatar: avatar
        )
        let factory = PublicProfileAvatarViewModelFactory(resolveProfile: { _, force in
            if force { return PublicProfile(avatar: nil) }
            return await resolver.profile
        }, loadAvatar: { avatar in
            try await resolver.loadAvatar(avatar)
        })
        let imageView = UIImageView()

        let succeeded = await load(factory.createViewModel(for: "alice.01", refresh: true), on: imageView)

        #expect(await (succeeded, imageView.image, resolver.imageRequests) == (false, nil, [avatar]))
    }

    @Test(arguments: [false, true])
    func switchingToAStaticImageOrDismantlingCancelsMetadata(dismantle: Bool) async throws {
        let initial = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let bytes = testPicturePNG(size: CGSize(width: 3, height: 2))
        let gate = ProfileResolutionGate()
        let (started, didStart) = AsyncStream<Void>.makeStream()
        let (finished, didFinish) = AsyncStream<Bool>.makeStream()
        let factory = PublicProfileAvatarViewModelFactory(resolveProfile: { _, force in
            if !force { return PublicProfile(avatar: initial) }
            await gate.wait { didStart.yield(()) }
            defer { didFinish.yield(Task.isCancelled); didFinish.finish() }
            return PublicProfile(avatar: nil)
        }, loadAvatar: { avatar in
            if avatar == initial { return bytes }
            Issue.record("A replaced view must not fetch bytes")
            return Data()
        })
        let viewModel = factory.createViewModel(for: "alice.01", refresh: true)
        let represented = AsyncImageView(viewModel: viewModel)
        let coordinator = represented.makeCoordinator()
        let imageView = UIImageView()
        coordinator.load(viewModel, on: imageView, settings: .originalImage, animated: false)
        for await _ in started {
            break
        }
        let replacement = try #require(UIImage(data: testPicturePNG(size: CGSize(width: 7, height: 6))))
        if dismantle {
            AsyncImageView.dismantleUIView(imageView, coordinator: coordinator)
        } else {
            let staticImage = StaticImageViewModel(image: replacement)
            coordinator.load(staticImage, on: imageView, settings: .originalImage, animated: false)
            _ = await load(staticImage, on: imageView)
        }
        await gate.release()
        var completed = finished.makeAsyncIterator()
        #expect(await completed.next() == true)
        #expect(imageView.image?.cgImage?.width == (dismantle ? nil : 7))
    }

    private func makeFactory(_ resolver: MockPublicProfileAvatarResolver) -> PublicProfileAvatarViewModelFactory {
        PublicProfileAvatarViewModelFactory(resolveProfile: { _, _ in
            await resolver.profile
        }, loadAvatar: { avatar in
            try await resolver.loadAvatar(avatar)
        })
    }

    private func load(_ viewModel: any ImageViewModelProtocol, on imageView: UIImageView) async -> Bool {
        await withCheckedContinuation { continuation in
            viewModel.loadImage(on: imageView, settings: .originalImage, animated: false) {
                continuation.resume(returning: $0)
            }
        }
    }
}

private actor ProfileResolutionGate {
    private var continuation: CheckedContinuation<Void, Never>?

    func wait(onStart: @Sendable () -> Void) async {
        await withCheckedContinuation {
            continuation = $0
            onStart()
        }
    }

    func release() {
        continuation?.resume()
        continuation = nil
    }
}
