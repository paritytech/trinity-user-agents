import UIKit_iOS
import Kingfisher
import Operation_iOS
import UIKit

final class AssetLogoCreationOperation: BaseOperation<UIImage>, @unchecked Sendable {
    let image: UIImage
    let brandColor: UIColor

    init(
        image: UIImage,
        brandColor: UIColor
    ) {
        self.image = image
        self.brandColor = brandColor
    }

    override func performAsync(_ callback: @escaping (Result<UIImage, Error>) -> Void) throws {
        callback(.success(createLogo(from: image)))
    }

    private func createLogo(from logo: UIImage) -> UIImage {
        // Make background a bit larger than the logo
        let size = CGRect(origin: .zero, size: logo.size).insetBy(dx: -8, dy: -8).size
        let scaledSize = size.scaled(by: UIScreen.main.scale)
        let backgroundFrame = CGRect(origin: .zero, size: scaledSize)

        let scaledLogoSize = logo.size.scaled(by: UIScreen.main.scale)
        let logoFrame = CGRect(origin: .zero, size: scaledLogoSize)
            .offsetBy(
                dx: (scaledSize.width - scaledLogoSize.width) / 2,
                dy: (scaledSize.height - scaledLogoSize.height) / 2
            )

        let renderer = UIGraphicsImageRenderer(size: scaledSize)
        return renderer.image { ctx in
            // Draw background
            let bezierPath = UIBezierPath(
                roundedRect: backgroundFrame,
                cornerRadius: backgroundFrame.size.width / 2
            )
            ctx.cgContext.setFillColor(brandColor.cgColor)
            ctx.cgContext.addPath(bezierPath.cgPath)
            ctx.cgContext.fillPath()
            // Draw logo
            logo.draw(in: logoFrame)
        }
    }
}

private extension CGSize {
    func scaled(by scale: CGFloat) -> CGSize {
        CGSize(
            width: width * scale,
            height: height * scale
        )
    }
}
