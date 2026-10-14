import Operation_iOS
import Foundation
import UIKit.UIImage

protocol QRCreationOperationFactoryProtocol {
    func createOperation(
        payload: Data,
        qrSize: CGSize
    ) -> CompoundOperationWrapper<UIImage>
}

final class QRCreationOperationFactory: QRCreationOperationFactoryProtocol {
    let chainStyle: ChainAssetStyle?

    init(chainStyle: ChainAssetStyle?) {
        self.chainStyle = chainStyle
    }

    func createOperation(
        payload: Data,
        qrSize: CGSize
    ) -> CompoundOperationWrapper<UIImage> {
        guard let chainStyle,
              let image = chainStyle.logo
        else {
            let operation = AssetQrCreationOperation(
                payload: payload,
                qrSize: qrSize
            )
            return CompoundOperationWrapper(targetOperation: operation)
        }
        let logoOperation = AssetLogoCreationOperation(image: image, brandColor: chainStyle.brandColor)
        let qrCreationOperation = AssetQrCreationOperation(
            qrSize: qrSize,
            logoClosure: { try logoOperation.extractNoCancellableResultData() },
            payloadClosure: { payload }
        )

        qrCreationOperation.addDependency(logoOperation)
        let wrapper = CompoundOperationWrapper(
            targetOperation: qrCreationOperation,
            dependencies: [logoOperation]
        )
        return wrapper
    }
}
