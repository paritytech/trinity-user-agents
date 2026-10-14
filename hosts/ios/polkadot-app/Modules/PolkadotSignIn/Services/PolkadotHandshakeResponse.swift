import Foundation
import SubstrateSdk

// MARK: - App Handshake Response (what we send back)

enum HandshakeResponse: ScaleEncodable {
    // swiftlint:disable:next identifier_name
    case v1(DataV1)
    // swiftlint:disable:next identifier_name
    case v2(DataV2)

    private var scaleIndex: UInt8 {
        switch self {
        case .v1: 0
        case .v2: 1
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)
        switch self {
        case let .v1(data):
            try data.encode(scaleEncoder: scaleEncoder)
        case let .v2(data):
            try data.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension HandshakeResponse {
    struct DataV1: ScaleEncodable {
        let encryptedMessage: Data
        let publicKey: Data

        func encode(scaleEncoder: any ScaleEncoding) throws {
            try encryptedMessage.encode(scaleEncoder: scaleEncoder)
            scaleEncoder.appendRaw(data: publicKey)
        }
    }

    struct DataV2: ScaleEncodable {
        let encryptedMessage: Data
        let publicKey: Data

        func encode(scaleEncoder: any ScaleEncoding) throws {
            try encryptedMessage.encode(scaleEncoder: scaleEncoder)
            scaleEncoder.appendRaw(data: publicKey)
        }
    }
}

// MARK: - V1 Encrypted Payload

struct HandshakeSuccessV1: ScaleEncodable {
    let sharedSecretDerivationKey: Data
    let rootUserAccountId: Data
    let identityAccountId: Data

    func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: sharedSecretDerivationKey)
        scaleEncoder.appendRaw(data: rootUserAccountId)
        scaleEncoder.appendRaw(data: identityAccountId)
    }
}

// MARK: - V2 Encrypted Payload

struct HandshakeSuccessV2: ScaleEncodable {
    let identityAccountId: Data
    let rootAccountId: Data
    let identityChatPrivateKey: Data
    let ssoEncrPubKey: Data
    let deviceEncPubKey: Data
    /// Per TrUAPI RFC-7; shared so the host can derive per-product entropies locally.
    let rootEntropySource: Data

    func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: identityAccountId)
        scaleEncoder.appendRaw(data: rootAccountId)
        scaleEncoder.appendRaw(data: identityChatPrivateKey)
        scaleEncoder.appendRaw(data: ssoEncrPubKey)
        scaleEncoder.appendRaw(data: deviceEncPubKey)
        scaleEncoder.appendRaw(data: rootEntropySource)
    }
}

enum EncryptedHandshakeResponseV2: ScaleEncodable {
    case pending(HandshakeStatusV2)
    case success(HandshakeSuccessV2)
    case failed(String)

    private var scaleIndex: UInt8 {
        switch self {
        case .pending: 0
        case .success: 1
        case .failed: 2
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)
        switch self {
        case let .pending(status):
            try status.encode(scaleEncoder: scaleEncoder)
        case let .success(data):
            try data.encode(scaleEncoder: scaleEncoder)
        case let .failed(message):
            try message.encode(scaleEncoder: scaleEncoder)
        }
    }
}

enum HandshakeStatusV2: ScaleEncodable {
    case allowanceAllocation

    private var scaleIndex: UInt8 {
        switch self {
        case .allowanceAllocation: 0
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)
    }
}
