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

    public func updateUIView(_ imageView: UIImageView, context _: Context) {
        viewModel.loadImage(on: imageView, settings: settings, animated: animated, completion: nil)
    }

    public static func dismantleUIView(_ imageView: UIImageView, coordinator _: ()) {
        imageView.image = nil
    }
}
