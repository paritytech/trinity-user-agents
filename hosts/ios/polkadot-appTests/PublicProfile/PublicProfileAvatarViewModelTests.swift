import Foundation
@testable import PolkadotUI
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

@MainActor
struct PublicProfileAvatarViewModelTests {
    @Test(.timeLimit(.minutes(1)), arguments: [false, true])
    func resizingPreservesThePendingChatRefresh(waitForInitialRequest: Bool) async throws {
        let requested = AsyncStream<(
            PublicProfileResolutionOptions,
            AsyncThrowingStream<PublicProfile, Error>.Continuation
        )>.makeStream()
        let images = RecordingCIDImageLoader()
        let viewModel = PublicProfileAvatarViewModel(
            username: "alice.01",
            options: .init(forceFresh: true, skipStale: false),
            profiles: { _, options in
                try Task.checkCancellation()
                let profiles = AsyncThrowingStream<PublicProfile, Error>.makeStream()
                requested.continuation.yield((options, profiles.continuation))
                return profiles.stream
            },
            imageLoader: images
        )
        let imageView = UIImageView()
        let coordinator = AsyncImageView(viewModel: viewModel).makeCoordinator()
        defer { coordinator.cancel(on: imageView) }
        var requests = requested.stream.makeAsyncIterator()
        coordinator.load(viewModel, on: imageView, settings: .originalImage, animated: false)
        if waitForInitialRequest {
            let initial = try #require(await requests.next())
            initial.1.yield(PublicProfile(avatar: avatar("stored")))
            _ = await images.nextRequest()
        }

        coordinator.load(
            viewModel,
            on: imageView,
            settings: .init(targetSize: CGSize(width: 80, height: 80)),
            animated: false
        )

        let resized = try #require(await requests.next())
        #expect(resized.0 == PublicProfileResolutionOptions(forceFresh: true, skipStale: false))
    }

    @Test(.timeLimit(.minutes(1)))
    func cachedAvatarSurvivesRedrawAndRefreshUntilTheViewLeaves() async throws {
        let (profiles, updates) = AsyncThrowingStream<PublicProfile, Error>.makeStream()
        let (terminated, termination) = AsyncStream<Void>.makeStream()
        updates.onTermination = { _ in termination.yield(()) }
        let images = RecordingCIDImageLoader()
        let viewModel = makeViewModel(profiles: profiles, images: images)
        let previousImage = try #require(UIImage(systemName: "person"))
        let imageView = UIImageView(image: previousImage)
        let coordinator = AsyncImageView(viewModel: viewModel).makeCoordinator()
        defer { coordinator.cancel(on: imageView) }
        coordinator.load(viewModel, on: imageView, settings: .originalImage, animated: false)
        #expect(imageView.image == nil)
        updates.yield(PublicProfile(avatar: avatar("stored")))
        let first = try #require(await images.nextRequest())
        let storedImage = try #require(UIImage(systemName: "person.fill"))
        images.complete(with: storedImage)

        let redrawn = makeViewModel(profiles: profiles, images: images)
        coordinator.load(redrawn, on: imageView, settings: .originalImage, animated: false)
        #expect(imageView.image === storedImage)
        updates.yield(PublicProfile(avatar: avatar("resolved")))
        let second = try #require(await images.nextRequest())

        #expect([first, second] == [reference("stored"), reference("resolved")])
        #expect(imageView.image === storedImage)
        let cancellations = images.cancelledViews.count
        coordinator.cancel(on: imageView)
        var terminations = terminated.makeAsyncIterator()
        _ = await terminations.next()

        #expect(images.cancelledViews.count == cancellations + 1)
        #expect(imageView.image == nil)
    }

    @Test(.timeLimit(.minutes(1)), arguments: [false, true])
    func refreshFailureKeepsTheAvatarButExplicitRemovalClearsIt(refreshFails: Bool) async {
        let (profiles, updates) = AsyncThrowingStream<PublicProfile, Error>.makeStream()
        let images = RecordingCIDImageLoader()
        let viewModel = makeViewModel(profiles: profiles, images: images)
        let imageView = UIImageView()
        defer { viewModel.cancel(on: imageView) }
        let (completed, completion) = AsyncStream<Bool>.makeStream()
        viewModel.loadImage(on: imageView, settings: .originalImage, animated: false) {
            completion.yield($0)
        }
        updates.yield(PublicProfile(avatar: avatar("stored")))
        _ = await images.nextRequest()
        let storedImage = UIImage()
        images.complete(with: storedImage)
        var results = completed.makeAsyncIterator()
        _ = await results.next()

        if refreshFails {
            updates.finish(throwing: ImageError.Unavailable(reason: "offline"))
        } else {
            updates.yield(PublicProfile(avatar: nil))
        }
        _ = await results.next()

        let expectedImage: UIImage? = refreshFails ? storedImage : nil
        #expect(imageView.image === expectedImage)
        #expect(images.requests == [reference("stored")])
        updates.finish()
    }

    private func makeViewModel(
        profiles: AsyncThrowingStream<PublicProfile, Error>,
        images: RecordingCIDImageLoader
    ) -> PublicProfileAvatarViewModel {
        PublicProfileAvatarViewModel(
            username: "alice.01",
            options: .init(forceFresh: false, skipStale: false),
            profiles: { _, _ in profiles },
            imageLoader: images
        )
    }

    private func avatar(_ cid: String) -> ProfileAvatar {
        ProfileAvatar(cid: cid, format: .png)
    }

    private func reference(_ cid: String) -> ImageReference {
        .bulletin(cid: cid, declaredFormat: .png)
    }
}

@MainActor
private final class RecordingCIDImageLoader: CIDImageLoading {
    private(set) var requests: [ImageReference] = []
    private(set) var cancelledViews: [UIImageView] = []
    private let requested = AsyncStream<ImageReference>.makeStream()
    private var pending: (view: UIImageView, completion: ((Bool) -> Void)?)?

    func load(
        _ reference: ImageReference,
        on imageView: UIImageView,
        settings _: ImageViewModelSettings,
        animated _: Bool,
        completion: ((Bool) -> Void)?
    ) {
        requests.append(reference)
        pending = (imageView, completion)
        requested.continuation.yield(reference)
    }

    func cancel(on imageView: UIImageView) {
        cancelledViews.append(imageView)
        pending = nil
    }

    func complete(with image: UIImage) {
        guard let pending else {
            Issue.record("An image request must be pending before it completes")
            return
        }
        self.pending = nil
        pending.view.image = image
        pending.completion?(true)
    }

    func nextRequest() async -> ImageReference? {
        var iterator = requested.stream.makeAsyncIterator()
        return await iterator.next()
    }
}
