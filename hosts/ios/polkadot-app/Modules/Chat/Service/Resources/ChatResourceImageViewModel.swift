import PolkadotUI
import UIKit

/// Draws a chat resource into an image view once the resolver has read it.
final class ChatResourceImageViewModel {
    private let resource: ChatResource
    private let resolver: ChatResourceResolving
    private var task: Task<Void, Never>?

    init(resource: ChatResource, resolver: ChatResourceResolving) {
        self.resource = resource
        self.resolver = resolver
    }
}

extension ChatResourceImageViewModel: ImageViewModelProtocol {
    @MainActor
    func loadImage(
        on imageView: UIImageView,
        settings _: ImageViewModelSettings,
        animated _: Bool,
        completion: ((Bool) -> Void)?
    ) {
        task?.cancel()
        task = Task { @MainActor [resource, resolver] in
            let image = await resolver.image(for: resource)
            guard !Task.isCancelled else { return }
            imageView.image = image
            completion?(image != nil)
        }
    }

    @MainActor
    func cancel(on imageView: UIImageView) {
        task?.cancel()
        imageView.image = nil
    }
}
