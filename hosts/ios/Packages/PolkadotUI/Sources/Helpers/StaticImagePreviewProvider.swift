import Foundation
import UIKit

class StaticImagePreviewProvider: ChatMessageMediaPreviewProviding {
    let image: UIImage
    var identifier: String

    init(image: UIImage) {
        self.image = image
        identifier = image.debugDescription
    }

    func providePreview(for imageView: UIImageView, size _: CGSize?) {
        imageView.image = image
    }
}

extension StaticImagePreviewProvider: ImageViewModelProtocol {
    func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated _: Bool,
        completion _: ((Bool) -> Void)?
    ) {
        providePreview(for: imageView, size: settings.targetSize)
    }

    func cancel(on _: UIImageView) {}
}
