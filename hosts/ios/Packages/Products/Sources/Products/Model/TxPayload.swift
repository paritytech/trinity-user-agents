import Foundation
import SubstrateSdk
import SubstrateSdkExt

public struct EncodedTransactionExtensionValue: Equatable, Decodable {
    public let id: String
    @HexCodable public var explicit: Data
    @HexCodable public var implicit: Data

    public init(id: String, explicit: Data, implicit: Data) {
        self.id = id
        _explicit = HexCodable(wrappedValue: explicit)
        _implicit = HexCodable(wrappedValue: implicit)
    }
}

public struct CreateTransactionPayload<Signer: Equatable & Decodable>: Equatable, Decodable {
    public let signer: Signer
    @HexCodable public var genesisHash: Data
    @HexCodable public var callData: Data
    public let extensions: [EncodedTransactionExtensionValue]
    public let txExtVersion: UInt8

    public init(
        signer: Signer,
        genesisHash: Data,
        callData: Data,
        extensions: [EncodedTransactionExtensionValue],
        txExtVersion: UInt8
    ) {
        self.signer = signer
        _genesisHash = HexCodable(wrappedValue: genesisHash)
        _callData = HexCodable(wrappedValue: callData)
        self.extensions = extensions
        self.txExtVersion = txExtVersion
    }
}

public struct CreateTransactionResult {
    public let signedTransaction: String

    public init(signedTransaction: String) {
        self.signedTransaction = signedTransaction
    }
}
