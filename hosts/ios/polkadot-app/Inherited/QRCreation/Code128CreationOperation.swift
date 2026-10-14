import Foundation
import CoreImage.CIFilterBuiltins
import Operation_iOS
import UIKit.UIImage

final class Code128CreationOperation: BaseOperation<UIImage>, @unchecked Sendable {
    let payloadClosure: () throws -> Data
    let height: Float
    let quietSpace: Float

    init(payload: Data, height: Float, quietSpace: Float = 5) {
        payloadClosure = { payload }
        self.height = height
        self.quietSpace = quietSpace

        super.init()
    }

    init(height: Float, quietSpace: Float = 5, payloadClosure: @escaping () throws -> Data) {
        self.height = height
        self.quietSpace = quietSpace
        self.payloadClosure = payloadClosure
    }

    override func performAsync(_ callback: @escaping (Result<UIImage, Error>) -> Void) throws {
        let filter = CIFilter.code128BarcodeGenerator()

        let payload = try payloadClosure()

        filter.message = payload
        filter.barcodeHeight = height
        filter.quietSpace = quietSpace

        guard let codeImage = filter.outputImage else {
            throw BarcodeCreationError.generatedImageInvalid
        }

        let transformedImage: CIImage

        if codeImage.extent.size.width * codeImage.extent.height > 0.0 {
            let transform = CGAffineTransform(
                scaleX: 2,
                y: 2
            )
            transformedImage = codeImage.transformed(by: transform)
        } else {
            transformedImage = codeImage
        }

        let context = CIContext()

        guard let cgImage = context.createCGImage(transformedImage, from: transformedImage.extent) else {
            throw BarcodeCreationError.bitmapImageCreationFailed
        }

        callback(.success(UIImage(cgImage: cgImage)))
    }
}
