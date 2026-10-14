import Kingfisher
import UIKit

final class KFTintImageProcessor: ImageProcessor {
    let identifier: String
    let tintColor: UIColor

    func process(item: ImageProcessItem, options: KingfisherParsedOptionsInfo) -> KFCrossPlatformImage? {
        switch item {
        case let .image(kFCrossPlatformImage):
            kFCrossPlatformImage.tinted(with: tintColor)
        case .data:
            (DefaultImageProcessor.default |> self).process(item: item, options: options)
        }
    }

    init(tintColor: UIColor) {
        identifier = "io.papp.kf.tint(\(tintColor.hexRGBA))"
        self.tintColor = tintColor
    }
}
