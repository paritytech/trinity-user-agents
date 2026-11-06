import Kingfisher
import PolkadotUI
import TrUAPIHost
import UIKit

@MainActor private var avatarRequestKey: Void?

final class PublicProfileAvatarViewModel {
    private let username: String
    private let resolveProfile: @Sendable (String, Bool) async throws -> PublicProfile
    private let loadAvatar: @Sendable (ProfileAvatar) async throws -> Data
    @MainActor private var refresh: Bool

    private final class Request {
        let id = UUID()
        weak var owner: PublicProfileAvatarViewModel?
        var task: Task<Void, Never>?

        init(owner: PublicProfileAvatarViewModel) {
            self.owner = owner
        }

        deinit { task?.cancel() }
    }

    init(
        username: String,
        refresh: Bool,
        resolveProfile: @escaping @Sendable (String, Bool) async throws -> PublicProfile,
        loadAvatar: @escaping @Sendable (ProfileAvatar) async throws -> Data
    ) {
        self.username = username
        self.refresh = refresh
        self.resolveProfile = resolveProfile
        self.loadAvatar = loadAvatar
    }

    @MainActor
    private func request(on imageView: UIImageView) -> Request? {
        objc_getAssociatedObject(imageView, &avatarRequestKey) as? Request
    }
}

extension PublicProfileAvatarViewModel: ImageViewModelProtocol {
    @MainActor
    func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
        if let current = request(on: imageView), current.owner === self, current.task != nil { return }
        let force = refresh
        refresh = false
        if request(on: imageView)?.owner !== self {
            imageView.kf.cancelDownloadTask()
            imageView.kf.setImage(with: nil as Source?)
        }
        let current = Request(owner: self)
        let identifier = current.id
        objc_setAssociatedObject(imageView, &avatarRequestKey, current, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        current.task = Task { [weak imageView] in
            defer {
                if let imageView, request(on: imageView)?.id == identifier { request(on: imageView)?.task = nil }
            }
            do {
                let profile = try await resolveProfile(username, force)
                try Task.checkCancellation()
                guard let imageView, request(on: imageView)?.id == identifier else { return }
                guard let avatar = profile.avatar else {
                    imageView.image = nil
                    completion?(false)
                    return
                }
                let provider = PublicProfileAvatarImageDataProvider(avatar: avatar, loadAvatar: loadAvatar)
                let options = PublicProfileAvatarImageOptionsFactory().options(for: settings, animated: animated)
                let image = try await KingfisherManager.shared
                    .retrieveImage(with: .provider(provider), options: options).image
                try Task.checkCancellation()
                guard request(on: imageView)?.id == identifier else { return }
                if animated {
                    UIView.transition(with: imageView, duration: 0.25, options: .transitionCrossDissolve) {
                        imageView.image = image
                    }
                } else {
                    imageView.image = image
                }
                completion?(true)
            } catch {
                guard let imageView, request(on: imageView)?.id == identifier else { return }
                imageView.image = nil
                completion?(false)
            }
        }
    }

    @MainActor
    func cancel(on imageView: UIImageView) {
        guard request(on: imageView)?.owner === self else { return }
        objc_setAssociatedObject(imageView, &avatarRequestKey, nil, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        imageView.image = nil
    }
}
