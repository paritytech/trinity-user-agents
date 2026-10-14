import Foundation
import Operation_iOS
import QRCode
import UIKit.UIImage
import DesignSystem

final class AssetQrCreationOperation: BaseOperation<UIImage>, @unchecked Sendable {
    let payloadClosure: () throws -> Data
    let logoClosure: () throws -> UIImage?
    let qrSize: CGSize
    init(
        payload: Data,
        qrSize: CGSize,
        logo: UIImage? = nil
    ) {
        payloadClosure = { payload }
        logoClosure = { logo }
        self.qrSize = qrSize
        super.init()
    }

    init(
        qrSize: CGSize,
        logoClosure: @escaping () throws -> UIImage?,
        payloadClosure: @escaping () throws -> Data
    ) {
        self.qrSize = qrSize
        self.logoClosure = logoClosure
        self.payloadClosure = payloadClosure
    }

    override func performAsync(_ callback: @escaping (Result<UIImage, Error>) -> Void) throws {
        let data = try payloadClosure()
        let qrDoc = try QRCode.Document(data: data)
        qrDoc.design.backgroundColor(UIColor.clear.cgColor)
        qrDoc.design.shape.eye = QRCode.EyeShape.RoundedRect()
        qrDoc.design.shape.onPixels = QRCode.PixelShape.Circle(insetFraction: 0.2)
        qrDoc.design.style.onPixels = QRCode.FillStyle.Solid(UIColor.black.cgColor)
        qrDoc.design.shape.offPixels = nil
        qrDoc.design.style.offPixels = nil

        if let logo = try logoClosure()?.cgImage {
            qrDoc.logoTemplate = QRCode.LogoTemplate.CircleCenter(image: logo, inset: 15)
        }

        let scaledSize = CGSize(
            width: qrSize.width * UIScreen.main.scale,
            height: qrSize.height * UIScreen.main.scale
        )

        let cgImage = try qrDoc.cgImage(scaledSize)
        callback(.success(UIImage(cgImage: cgImage)))
    }
}
