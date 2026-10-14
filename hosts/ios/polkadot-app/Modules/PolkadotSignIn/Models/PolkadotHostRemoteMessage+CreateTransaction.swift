import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    enum VersionedCreateTransactionPayload {
        // swiftlint:disable:next identifier_name
        case v1(CreateTransactionPayload<ProductAccountId>)
    }

    struct CreateTransactionRequest {
        let payload: VersionedCreateTransactionPayload
    }

    typealias CreateTransactionResultAP = HostResult<Data, String>
}

// MARK: - VersionedCreateTransactionPayload

extension PolkadotHostRemoteMessage.VersionedCreateTransactionPayload: MessageExchange.CodableMessage {
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

// MARK: - CreateTransactionRequest

extension PolkadotHostRemoteMessage.CreateTransactionRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        payload = try PolkadotHostRemoteMessage.VersionedCreateTransactionPayload(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try payload.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - Domain Conversion

extension PolkadotHostRemoteMessage.CreateTransactionRequest {
    func toDomainPayload() -> CreateTransactionPayload<ProductAccountId> {
        switch payload {
        case let .v1(txPayload):
            txPayload
        }
    }
}

extension CreateTransactionPayload where Signer == ProductAccountId {
    func toScaleRequest() -> PolkadotHostRemoteMessage.CreateTransactionRequest {
        PolkadotHostRemoteMessage.CreateTransactionRequest(payload: .v1(self))
    }
}
