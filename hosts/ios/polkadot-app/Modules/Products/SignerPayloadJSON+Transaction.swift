import Foundation
import Products

extension Products.SigningRawPayload {
    func toHostSigningRawPayload() -> PolkadotHostRemoteMessage.SigningRawPayload {
        let payloadType: PolkadotHostRemoteMessage.SigningRawPayload.PayloadType =
            switch content {
            case let .bytes(data):
                .bytes(data)
            case let .payload(string):
                .payload(string)
            }

        return PolkadotHostRemoteMessage.SigningRawPayload(
            account: account,
            type: payloadType
        )
    }
}
