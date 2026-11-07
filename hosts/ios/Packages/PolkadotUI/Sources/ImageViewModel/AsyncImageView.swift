import SwiftUI
import UIKit

public struct AsyncImageView {
    let viewModel: any ImageViewModelProtocol
    let settings: ImageViewModelSettings
    let animated: Bool

    public init(
        viewModel: any ImageViewModelProtocol,
        settings: ImageViewModelSettings = .originalImage,
        animated: Bool = true
    ) {
        self.viewModel = viewModel
        self.settings = settings
        self.animated = animated
    }
}

extension AsyncImageView: UIViewRepresentable {
    public func makeUIView(context _: Context) -> UIImageView {
        let imageView = UIImageView()
        imageView.contentMode = .scaleAspectFit
        imageView.clipsToBounds = true
        return imageView
    }

    public func makeCoordinator() -> Coordinator { Coordinator() }

    public func updateUIView(_ imageView: UIImageView, context: Context) {
        context.coordinator.load(viewModel, on: imageView, settings: settings, animated: animated)
    }

    public static func dismantleUIView(_ imageView: UIImageView, coordinator: Coordinator) {
        coordinator.cancel(on: imageView)
    }

    /// Owns the image request for the lifetime of its UIKit view.
    @MainActor
    public final class Coordinator {
        private var viewModel: (any ImageViewModelProtocol)?

        func load(
            _ next: any ImageViewModelProtocol,
            on imageView: UIImageView,
            settings: ImageViewModelSettings,
            animated: Bool
        ) {
            if let viewModel, viewModel as AnyObject !== next as AnyObject {
                viewModel.cancel(on: imageView)
            }
            viewModel = next
            next.loadImage(on: imageView, settings: settings, animated: animated, completion: nil)
        }

        func cancel(on imageView: UIImageView) {
            viewModel?.cancel(on: imageView)
            viewModel = nil
            imageView.image = nil
        }
    }
}
