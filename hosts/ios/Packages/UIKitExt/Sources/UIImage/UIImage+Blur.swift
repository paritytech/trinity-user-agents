import CoreImage
import UIKit

public extension UIImage {
    /// Returns a copy blurred by `radius` points, at the image's own scale.
    ///
    /// Returns `nil` when the image has no Core Image backing or the blur cannot be rendered.
    func blurred(radius: CGFloat) -> UIImage? {
        guard let input = CIImage(image: self) else { return nil }

        let output = input.clampedToExtent()
            .applyingGaussianBlur(sigma: radius * scale)
            .cropped(to: input.extent)
        guard let cgImage = CIContext().createCGImage(output, from: input.extent) else { return nil }

        return UIImage(cgImage: cgImage, scale: scale, orientation: .up)
    }
}
