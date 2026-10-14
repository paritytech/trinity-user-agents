import UIKit

extension UIImage {
    static func downsampleImage(at url: URL, maxSideSize: CGFloat, scale: CGFloat) -> UIImage? {
        let sourceOptions: CFDictionary = [
            kCGImageSourceShouldCache: false
        ] as CFDictionary

        guard let source = CGImageSourceCreateWithURL(url as CFURL, sourceOptions) else {
            return nil
        }

        let maxPixelSize = maxSideSize * scale

        let options: CFDictionary = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixelSize
        ] as CFDictionary

        guard let cgImage = CGImageSourceCreateThumbnailAtIndex(source, 0, options) else {
            return nil
        }

        return UIImage(cgImage: cgImage)
    }
}
