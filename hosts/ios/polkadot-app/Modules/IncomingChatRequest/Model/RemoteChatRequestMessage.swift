import Foundation
import SubstrateSdk

extension Chat {
    struct RequestMessage: Equatable {
        let messageId: String
        let timestamp: UInt64
        let content: VersionedRequestContent
    }

    enum VersionedRequestContent: Equatable {
        // swiftlint:disable:next identifier_name
        case v1(RequestContentV1)
        // swiftlint:disable:next identifier_name
        case v2(RequestContentV2)

        func ensureV1() -> RequestContentV1 {
            switch self {
            case let .v1(content):
                content
            case let .v2(content):
                RequestContentV1(
                    pushToken: content.pushToken,
                    welcomeMessage: content.welcomeMessage
                )
            }
        }

        /// Extracts the peer's identity account ID from a V2 chat request.
        func extractIdentityAccountId() -> Data? {
            switch self {
            case .v1:
                nil
            case let .v2(content):
                content.identityProof.identityAccountId
            }
        }

        /// Extracts the sender's device as a PeerDevice from a V2 chat request.
        func extractSenderDevice(statementAccountId: Data) -> Chat.PeerDevice? {
            switch self {
            case .v1:
                nil
            case let .v2(content):
                Chat.PeerDevice(
                    statementAccountId: statementAccountId,
                    encryptionPublicKey: content.deviceEncPubKey
                )
            }
        }
    }

    struct RequestContentV1: Equatable {
        let pushToken: Chat.RemoteTokenContent?
        let welcomeMessage: ChatRemoteMessageContent.RichText?
    }

    struct IdentityProof: Equatable {
        let identityAccountId: Data
        let proof: Data
    }

    struct RequestContentV2: Equatable {
        let identityProof: IdentityProof
        let deviceEncPubKey: Data
        let pushToken: Chat.RemoteTokenContent?
        let welcomeMessage: ChatRemoteMessageContent.RichText?
    }
}

// MARK: - Protocol Conformance

extension Chat.RequestMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
        timestamp = try UInt64(scaleDecoder: scaleDecoder)
        content = try Chat.VersionedRequestContent(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try timestamp.encode(scaleEncoder: scaleEncoder)
        try content.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.VersionedRequestContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = try .v1(Chat.RequestContentV1(scaleDecoder: scaleDecoder))
        case 1:
            self = try .v2(Chat.RequestContentV2(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .v1(content):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .v2(content):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.RequestContentV1: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let pushTokenOption = try ScaleOption<Chat.RemoteTokenContent>(scaleDecoder: scaleDecoder)
        pushToken = pushTokenOption.value

        let welcomeMessageOption = try ScaleOption<ChatRemoteMessageContent.RichText>(scaleDecoder: scaleDecoder)
        welcomeMessage = welcomeMessageOption.value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: pushToken).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: welcomeMessage).encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.IdentityProof: ScaleCodable {
    private enum Constants {
        static let identityAccountIdLength = 32
        static let proofLength = 32
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        identityAccountId = try scaleDecoder.readAndConfirm(count: Constants.identityAccountIdLength)
        proof = try scaleDecoder.readAndConfirm(count: Constants.proofLength)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: identityAccountId)
        scaleEncoder.appendRaw(data: proof)
    }
}

extension Chat.RequestContentV2: ScaleCodable {
    private enum Constants {
        static let deviceEncPubKeyLength = 32
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        identityProof = try Chat.IdentityProof(scaleDecoder: scaleDecoder)
        deviceEncPubKey = try scaleDecoder.readAndConfirm(count: Constants.deviceEncPubKeyLength)

        let pushTokenOption = try ScaleOption<Chat.RemoteTokenContent>(scaleDecoder: scaleDecoder)
        pushToken = pushTokenOption.value

        let welcomeMessageOption = try ScaleOption<ChatRemoteMessageContent.RichText>(scaleDecoder: scaleDecoder)
        welcomeMessage = welcomeMessageOption.value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try identityProof.encode(scaleEncoder: scaleEncoder)
        scaleEncoder.appendRaw(data: deviceEncPubKey)
        try ScaleOption(value: pushToken).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: welcomeMessage).encode(scaleEncoder: scaleEncoder)
    }
}
