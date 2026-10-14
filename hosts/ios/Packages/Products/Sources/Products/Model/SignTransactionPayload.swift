import Foundation
import SubstrateSdk
import SubstrateSdkExt

public struct SignTransactionPayload<Signer: Equatable & Decodable>: Equatable, Decodable {
    public let account: Signer
    @HexCodable public var blockHash: Data
    @HexCodable public var blockNumber: Data
    @HexCodable public var era: Data
    @HexCodable public var genesisHash: Data
    @HexCodable public var method: Data
    @HexCodable public var nonce: Data
    @HexCodable public var specVersion: Data
    @HexCodable public var tip: Data
    @HexCodable public var transactionVersion: Data
    public let signedExtensions: [String]
    public let version: UInt32
    @OptionHexCodable public var assetId: Data?
    @OptionHexCodable public var metadataHash: Data?
    public let mode: UInt32?
    public let withSignedTransaction: Bool?

    public init(
        account: Signer,
        blockHash: Data,
        blockNumber: Data,
        era: Data,
        genesisHash: Data,
        method: Data,
        nonce: Data,
        specVersion: Data,
        tip: Data,
        transactionVersion: Data,
        signedExtensions: [String],
        version: UInt32,
        assetId: Data?,
        metadataHash: Data?,
        mode: UInt32?,
        withSignedTransaction: Bool?
    ) {
        self.account = account
        _blockHash = HexCodable(wrappedValue: blockHash)
        _blockNumber = HexCodable(wrappedValue: blockNumber)
        _era = HexCodable(wrappedValue: era)
        _genesisHash = HexCodable(wrappedValue: genesisHash)
        _method = HexCodable(wrappedValue: method)
        _nonce = HexCodable(wrappedValue: nonce)
        _specVersion = HexCodable(wrappedValue: specVersion)
        _tip = HexCodable(wrappedValue: tip)
        _transactionVersion = HexCodable(wrappedValue: transactionVersion)
        self.signedExtensions = signedExtensions
        self.version = version
        _assetId = OptionHexCodable(wrappedValue: assetId)
        _metadataHash = OptionHexCodable(wrappedValue: metadataHash)
        self.mode = mode
        self.withSignedTransaction = withSignedTransaction
    }
}
