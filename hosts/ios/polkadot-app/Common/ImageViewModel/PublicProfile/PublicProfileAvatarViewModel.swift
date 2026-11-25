import AsyncAlgorithms
import PolkadotUI
import TrUAPIHost
import UIKit

@MainActor
final class PublicProfileAvatarViewModel: IdentifiableImageViewModelProtocol {
    private nonisolated let username: String
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let imageLoader: CIDImageLoader
    private var options: PublicProfileResolutionOptions
    private var task: Task<Void, Never>?
    private weak var imageView: UIImageView?

    nonisolated var identifier: String { "profile:\(username)" }

    nonisolated init(
        username: String,
        options: PublicProfileResolutionOptions,
        runtimeProvider: TrUAPIHostRuntimeProviding,
        imageLoader: CIDImageLoader
    ) {
        self.username = username
        self.options = options
        self.runtimeProvider = runtimeProvider
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
        imageView.image = nil
        let requestOptions = options
        task = Task { [weak self, runtimeProvider, username, requestOptions, imageLoader, weak imageView] in
            do {
                try Task.checkCancellation()
                let runtime = try await Self.sharedRuntime(from: runtimeProvider)
                try Task.checkCancellation()
                let avatars = runtime.resolvePublicProfile(username: username, options: requestOptions)
                    .map(\.avatar)
                    .removeDuplicates()
                for try await avatar in avatars {
                    try Task.checkCancellation()
                    guard let imageView else { return }
                    imageLoader.cancel(on: imageView)
                    if let avatar {
                        imageLoader.load(
                            .bulletin(cid: avatar.cid, declaredFormat: avatar.format),
                            on: imageView,
                            settings: settings,
                            animated: animated,
                            completion: completion
                        )
                    } else {
                        imageView.image = nil
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

    // Keep first-time runtime initialization off the main actor.
    private nonisolated static func sharedRuntime(
        from provider: TrUAPIHostRuntimeProviding
    ) async throws -> TrUAPIHostRuntime {
        try provider.sharedRuntime()
    }
}
