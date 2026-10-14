import Foundation
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    struct SignRawLegacyRequest: Equatable {
        let account: AccountId
        let type: SigningRawPayload.PayloadType
    }
}

extension PolkadotHostRemoteMessage.SignRawLegacyRequest: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        account = try scaleDecoder.readAndConfirm(count: LegacyAccountId.length)
        type = try PolkadotHostRemoteMessage.SigningRawPayload.PayloadType(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: account)
        try type.encode(scaleEncoder: scaleEncoder)
    }
}
