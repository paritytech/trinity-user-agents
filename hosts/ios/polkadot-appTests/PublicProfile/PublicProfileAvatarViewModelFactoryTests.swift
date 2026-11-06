import Foundation
import Kingfisher
@testable import PolkadotUI
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

@MainActor
struct PublicProfileAvatarViewModelFactoryTests {
    @Test
    func metadataRefreshWithTheSameCIDDoesNotDownloadAgain() async {
        let username = UUID().uuidString
        let avatar = ProfileAvatar(cid: username, format: .png)
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2))), avatar: avatar
        )
        let factory = makeFactory(resolver)
        let first = await load(factory.createViewModel(for: username, refresh: false), on: UIImageView())
        let second = await load(factory.createViewModel(for: username, refresh: true), on: UIImageView())
        #expect(await (first, second, resolver.requests, resolver.imageRequests) == (
            true, true,
            [.init(username: username, force: false), .init(username: username, force: true)], [avatar]
        ))
    }

    @Test
    func changingTheCIDLoadsTheNewImageAndRemovingItClearsTheView() async {
        let username = UUID().uuidString
        let first = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let second = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2))), avatar: first
        )
        let factory = makeFactory(resolver)
        let imageView = UIImageView()
        _ = await load(factory.createViewModel(for: username, refresh: false), on: imageView)
        await resolver.setAvatar(second, bytes: testPicturePNG(size: CGSize(width: 5, height: 4)))
        _ = await load(factory.createViewModel(for: username, refresh: true), on: imageView)
        let replaced = imageView.image?.cgImage?.width
        await resolver.setAvatar(nil)
        let removed = await load(factory.createViewModel(for: username, refresh: true), on: imageView)
        #expect(await (replaced, removed, imageView.image, resolver.imageRequests) == (5, false, nil, [first, second]))
    }

    @Test
    func repeatedChatUpdatesOnlyForceMetadataOnTheFirstLoad() async {
        let username = UUID().uuidString
        let avatar = ProfileAvatar(cid: username, format: .png)
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 3, height: 2))), avatar: avatar
        )
        let viewModel = makeFactory(resolver).createViewModel(for: username, refresh: true)
        let imageView = UIImageView()
        let first = await withCheckedContinuation { continuation in
            viewModel
                .loadImage(on: imageView, settings: .originalImage, animated: false) {
                    continuation.resume(returning: $0)
                }
            viewModel.loadImage(on: imageView, settings: .originalImage, animated: false, completion: nil)
        }
        let second = await load(viewModel, on: imageView)
        #expect(await (first, second, resolver.requests, resolver.imageRequests) == (
            true, true,
            [.init(username: username, force: true), .init(username: username, force: false)], [avatar]
        ))
    }

    @Test
    func aFailedMetadataRefreshShowsThePlaceholder() async {
        let factory = PublicProfileAvatarViewModelFactory(resolveProfile: { _, _ in
            throw ImageError.Unavailable(reason: "offline")
        }, loadAvatar: { _ in
            Issue.record("Failed metadata must not load image bytes")
            return Data()
        })
        let current = UIImage(systemName: "person.fill")
        let imageView = UIImageView(image: current)
        let succeeded = await load(factory.createViewModel(for: "alice.01", refresh: true), on: imageView)
        #expect(!succeeded)
        #expect(imageView.image == nil)
    }

    @Test(arguments: [false, true])
    func olderMetadataCannotReplaceTheNextContactsImage(fails: Bool) async {
        let (started, didStart) = AsyncStream<Void>.makeStream()
        let gate = ProfileResolutionGate()
        let (finished, didFinish) = AsyncStream<Bool>.makeStream()
        let factory = PublicProfileAvatarViewModelFactory(resolveProfile: { _, _ in
            await gate.wait { didStart.yield(()) }
            defer { didFinish.yield(Task.isCancelled); didFinish.finish() }
            if fails { throw ImageError.Unavailable(reason: "offline") }
            return PublicProfile(avatar: ProfileAvatar(cid: UUID().uuidString, format: .png))
        }, loadAvatar: { _ in
            Issue.record("Obsolete metadata must not load image bytes")
            return Data()
        })
        let imageView = UIImageView()
        let viewModel = factory.createViewModel(for: "alice.01", refresh: false)
        viewModel.loadImage(on: imageView, settings: .originalImage, animated: false) { _ in
            Issue.record("Obsolete metadata must not call the replaced completion")
        }
        for await _ in started {
            break
        }
        let avatar = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 5, height: 4))), avatar: avatar
        )
        let replacement = makeFactory(resolver).createViewModel(for: "bob.02", refresh: false)
        _ = await load(replacement, on: imageView)
        await gate.release()
        var completed = finished.makeAsyncIterator()
        #expect(await completed.next() == true)
        #expect(imageView.image?.cgImage?.width == 5)
    }

    @Test(arguments: [false, true])
    func switchingToAStaticImageOrDismantlingCancelsMetadata(dismantle: Bool) async throws {
        let gate = ProfileResolutionGate()
        let (started, didStart) = AsyncStream<Void>.makeStream()
        let (finished, didFinish) = AsyncStream<Bool>.makeStream()
        let factory = PublicProfileAvatarViewModelFactory(resolveProfile: { _, _ in
            await gate.wait { didStart.yield(()) }
            defer { didFinish.yield(Task.isCancelled); didFinish.finish() }
            return PublicProfile(avatar: nil)
        }, loadAvatar: { _ in
            Issue.record("A replaced view must not fetch bytes")
            return Data()
        })
        let viewModel = factory.createViewModel(for: "alice.01", refresh: false)
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

    @Test
    func aQueuedKingfisherImageCannotReplaceTheNewAvatar() async throws {
        let bytes = testPicturePNG(size: CGSize(width: 3, height: 2))
        let provider = RawImageDataProvider(data: bytes, cacheKey: UUID().uuidString)
        let cache = ImageCache(name: UUID().uuidString)
        try await cache.store(#require(UIImage(data: bytes)), forKey: provider.cacheKey, toDisk: false)
        let callbackQueue = DispatchQueue(label: "old-avatar-callback")
        let release = DispatchSemaphore(value: 0)
        callbackQueue.async { release.wait() }
        defer { release.signal() }
        let (completed, didComplete) = AsyncStream<Void>.makeStream()
        let imageView = UIImageView()
        imageView.kf.setImage(
            with: provider,
            options: [.targetCache(cache), .callbackQueue(.dispatch(callbackQueue))]
        ) { _ in didComplete.yield(()); didComplete.finish() }
        let avatar = ProfileAvatar(cid: UUID().uuidString, format: .png)
        let resolver = MockPublicProfileAvatarResolver(
            result: .success(testPicturePNG(size: CGSize(width: 5, height: 4))), avatar: avatar
        )
        _ = await load(makeFactory(resolver).createViewModel(for: "alice.01", refresh: false), on: imageView)
        release.signal()
        for await _ in completed {
            break
        }
        #expect(imageView.image?.cgImage?.width == 5)
    }

    private func makeFactory(_ resolver: MockPublicProfileAvatarResolver) -> PublicProfileAvatarViewModelFactory {
        PublicProfileAvatarViewModelFactory(resolveProfile: { username, force in
            await resolver.resolve(username: username, force: force)
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
