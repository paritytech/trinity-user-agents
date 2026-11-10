import Foundation
@testable import PolkadotUI
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

@MainActor
struct PublicProfileAvatarViewModelTests {
    @Test(.timeLimit(.minutes(1)))
    func reloadingAfterAFailedChatRefreshPreservesStalePolicy() async throws {
        let requested = AsyncStream<PublicProfileResolutionOptions>.makeStream()
        let completed = AsyncStream<Void>.makeStream()
        let viewModel = PublicProfileAvatarViewModel(
            username: "alice.01",
            options: .init(forceFresh: true, skipStale: true),
            profiles: { _, options in
                requested.continuation.yield(options)
                return AsyncThrowingStream { $0.finish(throwing: ImageError.Unavailable(reason: "offline")) }
            },
            imageLoader: RecordingCIDImageLoader()
        )
        let imageView = UIImageView()
        defer { viewModel.cancel(on: imageView) }
        var requests = requested.stream.makeAsyncIterator()
        var completions = completed.stream.makeAsyncIterator()

        viewModel.loadImage(on: imageView, settings: .originalImage, animated: false) { _ in
            completed.continuation.yield(())
        }
        let initial = try #require(await requests.next())
        _ = await completions.next()
        viewModel.cancel(on: imageView)
        viewModel.loadImage(
            on: imageView,
            settings: .init(targetSize: CGSize(width: 80, height: 80)),
            animated: false,
            completion: nil
        )
        let resized = try #require(await requests.next())

        #expect([initial, resized] == [
            .init(forceFresh: true, skipStale: true),
            .init(forceFresh: false, skipStale: true)
        ])
    }

    @Test(.timeLimit(.minutes(1)))
    func resizingBeforeTheFirstTaskStartsPreservesTheChatRefresh() async {
        let requested = AsyncStream<PublicProfileResolutionOptions>.makeStream()
        let viewModel = PublicProfileAvatarViewModel(
            username: "alice.01",
            options: .init(forceFresh: true, skipStale: false),
            profiles: { _, options in
                try Task.checkCancellation()
                requested.continuation.yield(options)
                return AsyncThrowingStream { $0.finish() }
            },
            imageLoader: RecordingCIDImageLoader()
        )
        let imageView = UIImageView()
        let coordinator = AsyncImageView(viewModel: viewModel).makeCoordinator()
        defer { coordinator.cancel(on: imageView) }

        coordinator.load(viewModel, on: imageView, settings: .originalImage, animated: false)
        coordinator.load(
            viewModel,
            on: imageView,
            settings: .init(targetSize: CGSize(width: 80, height: 80)),
            animated: false
        )
        var requests = requested.stream.makeAsyncIterator()

        #expect(await requests.next() == PublicProfileResolutionOptions(forceFresh: true, skipStale: false))
    }

    @Test(.timeLimit(.minutes(1)))
    func resizingWhileTheChatRefreshIsPendingKeepsItForced() async throws {
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
        let initial = try #require(await requests.next())
        initial.1.yield(PublicProfile(avatar: avatar("stored")))
        _ = await images.nextRequest()
        images.complete(with: UIImage())

        coordinator.load(
            viewModel,
            on: imageView,
            settings: .init(targetSize: CGSize(width: 80, height: 80)),
            animated: false
        )
        let resized = try #require(await requests.next())

        #expect([initial.0, resized.0] == [
            .init(forceFresh: true, skipStale: false),
            .init(forceFresh: true, skipStale: false)
        ])
        resized.1.finish()
    }

    @Test(.timeLimit(.minutes(1)))
    func aRefreshedAvatarKeepsTheStoredImageUntilItsReplacementArrives() async throws {
        let (profiles, updates) = AsyncThrowingStream<PublicProfile, Error>.makeStream()
        let images = RecordingCIDImageLoader()
        let viewModel = makeViewModel(profiles: profiles, images: images)
        let imageView = UIImageView()
        defer { viewModel.cancel(on: imageView) }
        viewModel.loadImage(on: imageView, settings: .originalImage, animated: false, completion: nil)

        updates.yield(PublicProfile(avatar: avatar("stored")))
        let first = try #require(await images.nextRequest())
        let storedImage = try #require(UIImage(systemName: "person.fill"))
        images.complete(with: storedImage)
        updates.yield(PublicProfile(avatar: avatar("resolved")))
        let second = try #require(await images.nextRequest())

        #expect([first, second] == [
            .init(reference: reference("stored"), keepsCurrentImage: false),
            .init(reference: reference("resolved"), keepsCurrentImage: true)
        ])
        #expect(imageView.image === storedImage)
        let resolvedImage = try #require(UIImage(systemName: "person.crop.circle.fill"))
        images.complete(with: resolvedImage)
        #expect(imageView.image === resolvedImage)
        updates.finish()
    }

    @Test(.timeLimit(.minutes(1)))
    func aFailedMetadataRefreshPreservesTheStoredImage() async {
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

        updates.finish(throwing: ImageError.Unavailable(reason: "offline"))
        _ = await results.next()

        #expect(imageView.image === storedImage)
        #expect(images.requests == [.init(reference: reference("stored"), keepsCurrentImage: false)])
    }

    @Test(.timeLimit(.minutes(1)))
    func aProfileWithoutAnAvatarRestoresThePlaceholder() async {
        let (profiles, updates) = AsyncThrowingStream<PublicProfile, Error>.makeStream()
        let images = RecordingCIDImageLoader()
        let viewModel = makeViewModel(profiles: profiles, images: images)
        let imageView = UIImageView()
        defer { viewModel.cancel(on: imageView) }
        viewModel.loadImage(on: imageView, settings: .originalImage, animated: false, completion: nil)
        updates.yield(PublicProfile(avatar: avatar("stored")))
        _ = await images.nextRequest()
        images.complete(with: UIImage())

        updates.yield(PublicProfile(avatar: nil))
        await images.nextCancellation()

        #expect(imageView.image == nil)
        updates.finish()
    }

    @Test(.timeLimit(.minutes(1)))
    func replacingTheViewModelStopsItsObservationAndImageRequest() async {
        let (profiles, updates) = AsyncThrowingStream<PublicProfile, Error>.makeStream()
        let (terminated, didTerminate) = AsyncStream<Void>.makeStream()
        updates.onTermination = { _ in didTerminate.yield(()) }
        let images = RecordingCIDImageLoader()
        let viewModel = makeViewModel(profiles: profiles, images: images)
        let imageView = UIImageView()
        let coordinator = AsyncImageView(viewModel: viewModel).makeCoordinator()
        defer { coordinator.cancel(on: imageView) }
        coordinator.load(viewModel, on: imageView, settings: .originalImage, animated: false)
        updates.yield(PublicProfile(avatar: avatar("stored")))
        _ = await images.nextRequest()
        let replacementImage = UIImage()
        let replacement = ImmediateImageViewModel(image: replacementImage)

        coordinator.load(replacement, on: imageView, settings: .originalImage, animated: false)
        var terminations = terminated.makeAsyncIterator()
        _ = await terminations.next()

        #expect(images.cancelledViews.last === imageView)
        #expect(imageView.image === replacementImage)
        if case .terminated = updates.yield(PublicProfile(avatar: avatar("late"))) {} else {
            Issue.record("The replaced view must stop observing the profile")
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func releasingTheViewModelCancelsItsObservationAndImageRequest() async {
        let (profiles, updates) = AsyncThrowingStream<PublicProfile, Error>.makeStream()
        let (terminated, didTerminate) = AsyncStream<Void>.makeStream()
        updates.onTermination = { _ in didTerminate.yield(()) }
        let images = RecordingCIDImageLoader()
        var viewModel: PublicProfileAvatarViewModel? = makeViewModel(profiles: profiles, images: images)
        let imageView = UIImageView()
        viewModel?.loadImage(on: imageView, settings: .originalImage, animated: false, completion: nil)
        updates.yield(PublicProfile(avatar: avatar("stored")))
        _ = await images.nextRequest()

        viewModel = nil
        var terminations = terminated.makeAsyncIterator()
        _ = await terminations.next()

        #expect(images.cancelledViews.last === imageView)
        if case .terminated = updates.yield(PublicProfile(avatar: avatar("late"))) {} else {
            Issue.record("Releasing the view model must stop observing its profile")
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func redrawingTheSameProfileKeepsItsExistingObservation() async throws {
        let (profiles, updates) = AsyncThrowingStream<PublicProfile, Error>.makeStream()
        let images = RecordingCIDImageLoader()
        let viewModel = makeViewModel(profiles: profiles, images: images)
        let imageView = UIImageView()
        let coordinator = AsyncImageView(viewModel: viewModel).makeCoordinator()
        defer { coordinator.cancel(on: imageView) }
        coordinator.load(viewModel, on: imageView, settings: .originalImage, animated: false)
        updates.yield(PublicProfile(avatar: avatar("stored")))
        _ = await images.nextRequest()
        let image = UIImage()
        images.complete(with: image)

        let redrawn = makeViewModel(profiles: profiles, images: images)
        coordinator.load(redrawn, on: imageView, settings: .originalImage, animated: false)
        updates.yield(PublicProfile(avatar: avatar("resolved")))
        let request = try #require(await images.nextRequest())

        #expect(request == .init(reference: reference("resolved"), keepsCurrentImage: true))
        #expect(imageView.image === image)
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
    struct Request: Equatable {
        let reference: ImageReference
        let keepsCurrentImage: Bool
    }

    private(set) var requests: [Request] = []
    private(set) var cancelledViews: [UIImageView] = []
    private let requested = AsyncStream<Request>.makeStream()
    private let cancelled = AsyncStream<Void>.makeStream()
    private var pending: (view: UIImageView, completion: ((Bool) -> Void)?)?

    func load(
        _ reference: ImageReference,
        on imageView: UIImageView,
        settings _: ImageViewModelSettings,
        animated _: Bool,
        keepsCurrentImageWhileLoading: Bool,
        completion: ((Bool) -> Void)?
    ) {
        let request = Request(reference: reference, keepsCurrentImage: keepsCurrentImageWhileLoading)
        requests.append(request)
        if !keepsCurrentImageWhileLoading { imageView.image = nil }
        pending = (imageView, completion)
        requested.continuation.yield(request)
    }

    func cancel(on imageView: UIImageView) {
        cancelledViews.append(imageView)
        pending = nil
        cancelled.continuation.yield(())
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

    func nextRequest() async -> Request? {
        var iterator = requested.stream.makeAsyncIterator()
        return await iterator.next()
    }

    func nextCancellation() async {
        var iterator = cancelled.stream.makeAsyncIterator()
        _ = await iterator.next()
    }
}

private struct ImmediateImageViewModel: ImageViewModelProtocol {
    let image: UIImage

    @MainActor
    func loadImage(
        on imageView: UIImageView,
        settings _: ImageViewModelSettings,
        animated _: Bool,
        completion: ((Bool) -> Void)?
    ) {
        imageView.image = image
        completion?(true)
    }

    @MainActor
    func cancel(on imageView: UIImageView) {
        imageView.image = nil
    }
}
