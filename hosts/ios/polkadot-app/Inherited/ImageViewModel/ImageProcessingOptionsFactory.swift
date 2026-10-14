import Kingfisher
import UIKit
import PolkadotUI

protocol ImageProcessingOptionsProducing: AnyObject, Sendable {
    func options(for settings: ImageViewModelSettings, animated: Bool) -> KingfisherOptionsInfo
}

final class ImageProcessingOptionsFactory: ImageProcessingOptionsProducing, Sendable {
    func options(
        for settings: ImageViewModelSettings,
        animated: Bool
    ) -> KingfisherOptionsInfo {
        var processor: ImageProcessor = SVGImageProcessor()
        if let targetSize = settings.targetSize, targetSize.width > 0, targetSize.height > 0 {
            processor = processor |> ResizingImageProcessor(referenceSize: targetSize, mode: .aspectFill) |>
                CroppingImageProcessor(size: targetSize)
        }

        if let tintColor = settings.tintColor {
            processor = processor |> KFTintImageProcessor(tintColor: tintColor)
        }

        if let cornerRadius = settings.cornerRadius, cornerRadius > 0 {
            processor = processor |> RoundCornerImageProcessor(cornerRadius: cornerRadius)
        }

        var options: KingfisherOptionsInfo = [
            .processor(processor),
            .scaleFactor(UIScreen.main.scale),
            .cacheSerializer(RemoteImageSerializer.shared),
            .cacheOriginalImage,
            .diskCacheExpiration(.days(1))
        ]

        if let renderingMode = settings.renderingMode {
            let imageModifier = RenderingModeImageModifier(renderingMode: renderingMode)
            options.append(.imageModifier(imageModifier))
        }

        if animated {
            options.append(.transition(.fade(0.25)))
        }

        return options
    }
}
