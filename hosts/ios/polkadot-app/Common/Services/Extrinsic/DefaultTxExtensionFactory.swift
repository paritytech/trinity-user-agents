import Foundation
import SubstrateSdk
import BigInt
import XcmDefinition
import Individuality

///     Signed extension setup consists of the 2 parts:
///      - provide signed extension class that contains parameters for the extrinsic's signed extra
///      - provide coders to encode/decode signed extension's parameters part of signed extra
protocol ExtrinsicTransactionExtensionMaking {
    func createExtensions() -> [TransactionExtending]

    func createCoders(for metadata: RuntimeMetadataProtocol) -> [TransactionExtensionCoding]
}

final class DefaultTxExtensionFactory {}

extension DefaultTxExtensionFactory: ExtrinsicTransactionExtensionMaking {
    func createExtensions() -> [TransactionExtending] {
        [
            TransactionExtension.ChargeAssetTxPayment<JSON>(),
            OriginRestrictionPallet.TransactionExtension(enabled: false)
        ]
    }

    func createCoders(for metadata: RuntimeMetadataProtocol) -> [TransactionExtensionCoding] {
        DefaultSignedExtensionCoders.createDefaultCoders(for: metadata)
    }
}

enum DefaultSignedExtensionCoders {
    static func createDefaultCoders(for metadata: RuntimeMetadataProtocol) -> [TransactionExtensionCoding] {
        [
            DefaultVersionedTransactionExtensionCoder(
                txExtensionId: Extrinsic.TransactionExtensionId.assetTxPayment,
                metadata: metadata
            )
        ]
    }
}
