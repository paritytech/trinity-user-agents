import Foundation
import SubstrateSdk

extension Chat {
    /// Remote representation of a chat encryption key: on-chain identity records store it
    /// in a fixed 65-byte container — a keypair-type byte, the key bytes, and zero padding
    /// that readers ignore (chat-spec RFC-0004 §4). The local representation is `Chat.PublicKey`;
    /// raw container bytes appear only in `ConsumerInfo.identifierKey` and extrinsic call params.
    enum OnChainEncryptionIdentifier {
        case x25519(PublicKey)

        var localPublicKey: PublicKey {
            switch self {
            case let .x25519(publicKey):
                publicKey
            }
        }
    }
}

extension Chat.OnChainEncryptionIdentifier: ScaleCodable {
    static let containerSize = 65

    private static let typeSize = 1

    private enum KeypairType: UInt8 {
        case x25519 = 0x00

        var keySize: Int {
            switch self {
            case .x25519:
                Chat.PublicKey.keySize
            }
        }

        var paddingSize: Int {
            Chat.OnChainEncryptionIdentifier.containerSize
                - Chat.OnChainEncryptionIdentifier.typeSize
                - keySize
        }
    }

    private var keypairType: KeypairType {
        switch self {
        case .x25519:
            .x25519
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let rawType = try UInt8(scaleDecoder: scaleDecoder)
        guard let keypairType = KeypairType(rawValue: rawType) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        let rawKey = try scaleDecoder.readAndConfirm(count: keypairType.keySize)
        _ = try scaleDecoder.readAndConfirm(count: keypairType.paddingSize)

        switch keypairType {
        case .x25519:
            self = try .x25519(Chat.PublicKey(rawData: rawKey))
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try keypairType.rawValue.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .x25519(publicKey):
            scaleEncoder.appendRaw(data: publicKey.rawData)
        }

        scaleEncoder.appendRaw(data: Data(count: keypairType.paddingSize))
    }
}
