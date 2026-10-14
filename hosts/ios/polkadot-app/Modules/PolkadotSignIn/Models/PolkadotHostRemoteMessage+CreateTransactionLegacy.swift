import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    enum VersionedCreateTransactionLegacyPayload {
        // swiftlint:disable:next identifier_name
        case v1(CreateTransactionPayload<LegacyAccountId>)
    }

    struct CreateTransactionLegacyRequest {
        let payload: VersionedCreateTransactionLegacyPayload
    }
}

extension PolkadotHostRemoteMessage.VersionedCreateTransactionLegacyPayload: MessageExchange.CodableMessage {
    private var scaleIndex: UInt8 {
        switch self {
        case .v1: 0
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            self = try .v1(.init(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .v1(payload):
            try payload.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension PolkadotHostRemoteMessage.CreateTransactionLegacyRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        payload = try PolkadotHostRemoteMessage.VersionedCreateTransactionLegacyPayload(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try payload.encode(scaleEncoder: scaleEncoder)
    }
}

extension PolkadotHostRemoteMessage.CreateTransactionLegacyRequest {
    func toDomainPayload() -> CreateTransactionPayload<LegacyAccountId> {
        switch payload {
        case let .v1(txPayload):
            txPayload
        }
    }
}

extension CreateTransactionPayload where Signer == LegacyAccountId {
    func toScaleLegacyRequest() -> PolkadotHostRemoteMessage.CreateTransactionLegacyRequest {
        PolkadotHostRemoteMessage.CreateTransactionLegacyRequest(payload: .v1(self))
    }
}
