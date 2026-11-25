import UIKit

/// An image loader whose identity describes its logical content.
public protocol IdentifiableImageViewModelProtocol: ImageViewModelProtocol {
    var identifier: String { get }
}

/// Compares image loaders by their stable content identity.
public final class AnyImageViewModel: IdentifiableImageViewModelProtocol, Hashable {
    private let viewModel: any IdentifiableImageViewModelProtocol

    public var identifier: String { viewModel.identifier }

    public init(_ viewModel: any IdentifiableImageViewModelProtocol) {
        self.viewModel = viewModel
    }

    public static func == (lhs: AnyImageViewModel, rhs: AnyImageViewModel) -> Bool {
        lhs.identifier == rhs.identifier
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(identifier)
    }

    @MainActor
    public func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
        viewModel.loadImage(on: imageView, settings: settings, animated: animated, completion: completion)
    }

    @MainActor
    public func cancel(on imageView: UIImageView) {
        viewModel.cancel(on: imageView)
    }
}
