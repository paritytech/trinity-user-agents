import Foundation
import MessageExchangeKit
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    struct Signature {
        let rawSignature: Data
        let signedTransaction: Data?
    }
}

extension PolkadotHostRemoteMessage.Signature: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        rawSignature = try Data(scaleDecoder: scaleDecoder)
        signedTransaction = try ScaleOption(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawSignature.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: signedTransaction).encode(scaleEncoder: scaleEncoder)
    }
}
