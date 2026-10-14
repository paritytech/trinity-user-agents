import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk

struct PolkadotHostRemoteMessage {
    let messageId: String
    let versionedContent: VersionedContent

    func latestContent() -> LatestContent? {
        switch versionedContent {
        case let .v1(contentV1):
            contentV1
        }
    }
}

typealias OpaquePolkadotHostRemoteMessage = OpaqueMessageWrapper<PolkadotHostRemoteMessage>

extension PolkadotHostRemoteMessage {
    typealias LatestContent = ContentV1

    enum VersionedContent {
        // swiftlint:disable:next identifier_name
        case v1(ContentV1)
    }

    enum ContentV1 {
        case disconnected
        case signingRequest(SigningRequest)
        case signingResponse(requestMessageId: String, result: SigningResult)
        case aliasRequest(AliasRequest)
        case aliasResponse(requestMessageId: String, result: AliasResult)
        case resourceAllocationRequest(ResourceAllocationRequest)
        case resourceAllocationResponse(requestMessageId: String, result: ResourceAllocationResult)
        case createTransactionRequest(CreateTransactionRequest)
        case createTransactionResponse(requestMessageId: String, result: CreateTransactionResultAP)
        case createTransactionLegacyRequest(CreateTransactionLegacyRequest)
        case signRawLegacyRequest(SignRawLegacyRequest)
        case signRawLegacyResponse(requestMessageId: String, result: SignRawLegacyResult)
        case createProofRequest(CreateProofRequest)
        case createProofResponse(requestMessageId: String, result: CreateProofResult)
        case signVrfRequest(SignVrfRequest)
        case signVrfResponse(requestMessageId: String, result: SignVrfHostResult)
        case productSubtreeRequest(ProductSubtreeRequest)
        case productSubtreeResponse(requestMessageId: String, result: ProductSubtreeResult)
    }

    enum HostResult<Success, Failure> {
        case success(Success)
        case failure(Failure)
    }

    typealias SigningResult = HostResult<Signature, String>
    typealias AliasResult = HostResult<ContextualAlias, RingVrfError>
    typealias SignRawLegacyResult = HostResult<Data, String>
}

extension PolkadotHostRemoteMessage: HostMessageIdentifiable {}

extension PolkadotHostRemoteMessage: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
        versionedContent = try VersionedContent(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try versionedContent.encode(scaleEncoder: scaleEncoder)
    }
}

extension PolkadotHostRemoteMessage.VersionedContent: MessageExchange.CodableMessage {
    private var scaleIndex: UInt8 {
        switch self {
        case .v1: 0
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            self = try .v1(PolkadotHostRemoteMessage.ContentV1(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .v1(contentV1):
            try contentV1.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension PolkadotHostRemoteMessage.ContentV1: MessageExchange.CodableMessage {
    private var scaleIndex: UInt8 {
        switch self {
        case .disconnected: 0
        case .signingRequest: 1
        case .signingResponse: 2
        case .aliasRequest: 3
        case .aliasResponse: 4
        case .resourceAllocationRequest: 5
        case .resourceAllocationResponse: 6
        case .createTransactionRequest: 7
        case .createTransactionResponse: 8
        case .createTransactionLegacyRequest: 9
        case .signRawLegacyRequest: 10
        case .signRawLegacyResponse: 11
        case .createProofRequest: 12
        case .createProofResponse: 13
        case .signVrfRequest: 14
        case .signVrfResponse: 15
        case .productSubtreeRequest: 16
        case .productSubtreeResponse: 17
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            self = .disconnected
        case 1:
            let value = try PolkadotHostRemoteMessage.SigningRequest(scaleDecoder: scaleDecoder)
            self = .signingRequest(value)
        case 2:
            let requestMessageId = try String(scaleDecoder: scaleDecoder)
            let result = try PolkadotHostRemoteMessage.SigningResult(scaleDecoder: scaleDecoder)
            self = .signingResponse(requestMessageId: requestMessageId, result: result)
        case 3:
            let value = try PolkadotHostRemoteMessage.AliasRequest(scaleDecoder: scaleDecoder)
            self = .aliasRequest(value)
        case 4:
            let requestMessageId = try String(scaleDecoder: scaleDecoder)
            let result = try PolkadotHostRemoteMessage.AliasResult(scaleDecoder: scaleDecoder)
            self = .aliasResponse(requestMessageId: requestMessageId, result: result)
        case 5:
            let value = try PolkadotHostRemoteMessage.ResourceAllocationRequest(scaleDecoder: scaleDecoder)
            self = .resourceAllocationRequest(value)
        case 6:
            let requestMessageId = try String(scaleDecoder: scaleDecoder)
            let result = try PolkadotHostRemoteMessage.ResourceAllocationResult(scaleDecoder: scaleDecoder)
            self = .resourceAllocationResponse(requestMessageId: requestMessageId, result: result)
        case 7:
            let value = try PolkadotHostRemoteMessage.CreateTransactionRequest(scaleDecoder: scaleDecoder)
            self = .createTransactionRequest(value)
        case 8:
            let requestMessageId = try String(scaleDecoder: scaleDecoder)
            let result = try PolkadotHostRemoteMessage.CreateTransactionResultAP(scaleDecoder: scaleDecoder)
            self = .createTransactionResponse(requestMessageId: requestMessageId, result: result)
        case 9:
            let value = try PolkadotHostRemoteMessage.CreateTransactionLegacyRequest(scaleDecoder: scaleDecoder)
            self = .createTransactionLegacyRequest(value)
        case 10:
            let value = try PolkadotHostRemoteMessage.SignRawLegacyRequest(scaleDecoder: scaleDecoder)
            self = .signRawLegacyRequest(value)
        case 11:
            let requestMessageId = try String(scaleDecoder: scaleDecoder)
            let result = try PolkadotHostRemoteMessage.SignRawLegacyResult(scaleDecoder: scaleDecoder)
            self = .signRawLegacyResponse(requestMessageId: requestMessageId, result: result)
        case 12:
            let value = try PolkadotHostRemoteMessage.CreateProofRequest(scaleDecoder: scaleDecoder)
            self = .createProofRequest(value)
        case 13:
            let requestMessageId = try String(scaleDecoder: scaleDecoder)
            let result = try PolkadotHostRemoteMessage.CreateProofResult(scaleDecoder: scaleDecoder)
            self = .createProofResponse(requestMessageId: requestMessageId, result: result)
        case 14:
            let value = try PolkadotHostRemoteMessage.SignVrfRequest(scaleDecoder: scaleDecoder)
            self = .signVrfRequest(value)
        case 15:
            let requestMessageId = try String(scaleDecoder: scaleDecoder)
            let result = try PolkadotHostRemoteMessage.SignVrfHostResult(scaleDecoder: scaleDecoder)
            self = .signVrfResponse(requestMessageId: requestMessageId, result: result)
        case 16:
            let value = try PolkadotHostRemoteMessage.ProductSubtreeRequest(scaleDecoder: scaleDecoder)
            self = .productSubtreeRequest(value)
        case 17:
            let requestMessageId = try String(scaleDecoder: scaleDecoder)
            let result = try PolkadotHostRemoteMessage.ProductSubtreeResult(scaleDecoder: scaleDecoder)
            self = .productSubtreeResponse(requestMessageId: requestMessageId, result: result)
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case .disconnected:
            break
        case let .signingRequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .signingResponse(requestMessageId, result):
            try requestMessageId.encode(scaleEncoder: scaleEncoder)
            try result.encode(scaleEncoder: scaleEncoder)
        case let .aliasRequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .aliasResponse(requestMessageId, result):
            try requestMessageId.encode(scaleEncoder: scaleEncoder)
            try result.encode(scaleEncoder: scaleEncoder)
        case let .resourceAllocationRequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .resourceAllocationResponse(requestMessageId, result):
            try requestMessageId.encode(scaleEncoder: scaleEncoder)
            try result.encode(scaleEncoder: scaleEncoder)
        case let .createTransactionRequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .createTransactionResponse(requestMessageId, result):
            try requestMessageId.encode(scaleEncoder: scaleEncoder)
            try result.encode(scaleEncoder: scaleEncoder)
        case let .createTransactionLegacyRequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .signRawLegacyRequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .signRawLegacyResponse(requestMessageId, result):
            try requestMessageId.encode(scaleEncoder: scaleEncoder)
            try result.encode(scaleEncoder: scaleEncoder)
        case let .createProofRequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .createProofResponse(requestMessageId, result):
            try requestMessageId.encode(scaleEncoder: scaleEncoder)
            try result.encode(scaleEncoder: scaleEncoder)
        case let .signVrfRequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .signVrfResponse(requestMessageId, result):
            try requestMessageId.encode(scaleEncoder: scaleEncoder)
            try result.encode(scaleEncoder: scaleEncoder)
        case let .productSubtreeRequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .productSubtreeResponse(requestMessageId, result):
            try requestMessageId.encode(scaleEncoder: scaleEncoder)
            try result.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension PolkadotHostRemoteMessage.HostResult: MessageExchange.CodableMessage
    where Success: MessageExchange.CodableMessage, Failure: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            self = try .success(Success(scaleDecoder: scaleDecoder))
        case 1:
            self = try .failure(Failure(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .success(value):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try value.encode(scaleEncoder: scaleEncoder)
        case let .failure(reason):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try reason.encode(scaleEncoder: scaleEncoder)
        }
    }
}
