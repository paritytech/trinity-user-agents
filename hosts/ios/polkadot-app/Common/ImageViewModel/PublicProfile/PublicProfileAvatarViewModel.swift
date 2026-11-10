import PolkadotUI
import TrUAPIHost
import UIKit

@MainActor
final class PublicProfileAvatarViewModel: IdentifiableImageViewModelProtocol {
    private nonisolated let username: String
    private let profiles: @Sendable (String, PublicProfileResolutionOptions) async throws -> AsyncThrowingStream<
        PublicProfile,
        Error
    >
    private let imageLoader: any CIDImageLoading
    private var options: PublicProfileResolutionOptions
    private var task: Task<Void, Never>?
    private weak var imageView: UIImageView?

    nonisolated var identifier: String { "profile:\(username)" }

    nonisolated init(
        username: String,
        options: PublicProfileResolutionOptions,
        profiles: @escaping @Sendable (String, PublicProfileResolutionOptions) async throws -> AsyncThrowingStream<
            PublicProfile,
            Error
        >,
        imageLoader: any CIDImageLoading
    ) {
        self.username = username
        self.options = options
        self.profiles = profiles
        self.imageLoader = imageLoader
    }

    func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
        if self.imageView === imageView, task != nil { return }
        if let current = self.imageView { cancel(on: current) }
        self.imageView = imageView
        let requestOptions = options
        task = Task { [weak self, profiles, username, requestOptions, imageLoader, weak imageView] in
            do {
                try Task.checkCancellation()
                let updates = try await profiles(username, requestOptions)
                try Task.checkCancellation()
                var keepsCurrentImage = false
                for try await profile in updates {
                    try Task.checkCancellation()
                    guard let imageView else { return }
                    if let avatar = profile.avatar {
                        if keepsCurrentImage { imageLoader.cancel(on: imageView) }
                        imageLoader.load(
                            .bulletin(cid: avatar.cid, declaredFormat: avatar.format),
                            on: imageView,
                            settings: settings,
                            animated: animated,
                            keepsCurrentImageWhileLoading: keepsCurrentImage,
                            completion: completion
                        )
                        keepsCurrentImage = true
                    } else {
                        imageLoader.cancel(on: imageView)
                        imageView.image = nil
                        keepsCurrentImage = false
                        completion?(false)
                    }
                }
                if !Task.isCancelled { self?.options.forceFresh = false }
            } catch {
                guard !Task.isCancelled else { return }
                self?.options.forceFresh = false
                completion?(imageView?.image != nil)
            }
        }
    }

    func cancel(on imageView: UIImageView) {
        guard self.imageView === imageView else { return }
        task?.cancel()
        task = nil
        imageLoader.cancel(on: imageView)
        imageView.image = nil
        self.imageView = nil
    }

    isolated deinit {
        task?.cancel()
        if let imageView { imageLoader.cancel(on: imageView) }
    }
}
