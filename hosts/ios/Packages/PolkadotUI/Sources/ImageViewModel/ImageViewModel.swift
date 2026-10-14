import UIKit

public struct ImageViewModelSettings {
    public let targetSize: CGSize?
    public let cornerRadius: CGFloat?
    public let tintColor: UIColor?
    public let renderingMode: UIImage.RenderingMode?

    public init(
        targetSize: CGSize? = nil,
        cornerRadius: CGFloat? = nil,
        tintColor: UIColor? = nil,
        renderingMode: UIImage.RenderingMode? = nil
    ) {
        self.targetSize = targetSize
        self.cornerRadius = cornerRadius
        self.tintColor = tintColor
        self.renderingMode = renderingMode
    }
}

public extension ImageViewModelSettings {
    static var originalImage: ImageViewModelSettings {
        ImageViewModelSettings()
    }
}

public protocol ImageViewModelProtocol {
    @MainActor func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    )

    @MainActor func cancel(on imageView: UIImageView)
}

public extension ImageViewModelProtocol {
    @MainActor func loadImage(
        on imageView: UIImageView,
        targetSize: CGSize,
        animated: Bool,
        renderingMode: UIImage.RenderingMode? = nil,
        completion: ((Bool) -> Void)? = nil
    ) {
        let settings = ImageViewModelSettings(
            targetSize: targetSize,
            cornerRadius: nil,
            tintColor: nil,
            renderingMode: renderingMode
        )

        loadImage(on: imageView, settings: settings, animated: animated, completion: completion)
    }

    @MainActor func loadImage(
        on imageView: UIImageView,
        targetSize: CGSize,
        cornerRadius: CGFloat,
        animated: Bool,
        completion: ((Bool) -> Void)? = nil
    ) {
        let settings = ImageViewModelSettings(
            targetSize: targetSize,
            cornerRadius: cornerRadius,
            tintColor: nil,
            renderingMode: nil
        )

        loadImage(on: imageView, settings: settings, animated: animated, completion: completion)
    }
}
