import SwiftUI
import UIKit

public struct AsyncImageView {
    let viewModel: AnyImageViewModel
    let settings: ImageViewModelSettings
    let animated: Bool

    public init(
        viewModel: AnyImageViewModel,
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
        private var viewModel: AnyImageViewModel?
        private var settings: ImageViewModelSettings?

        func load(
            _ next: AnyImageViewModel,
            on imageView: UIImageView,
            settings: ImageViewModelSettings,
            animated: Bool
        ) {
            if viewModel == next, self.settings == settings { return }
            viewModel?.cancel(on: imageView)
            viewModel = next
            self.settings = settings
            next.loadImage(on: imageView, settings: settings, animated: animated, completion: nil)
        }

        func cancel(on imageView: UIImageView) {
            viewModel?.cancel(on: imageView)
            viewModel = nil
            settings = nil
            imageView.image = nil
        }
    }
}
