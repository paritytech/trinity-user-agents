import Foundation
import BigInt
import Products
import SubstrateSdk
import KeyDerivation

struct PolkadotParsedSigningRequestResult {
    enum ResultError: Error {
        case missingCodingFactory
    }

    let wallet: WalletManaging
    let parsedRequest: PolkadotParsedSigningRequest
    let requester: PolkadotSigningRequester
    let detailsText: String

    var isTransaction: Bool {
        switch parsedRequest {
        case .legacyTransaction,
             .createTransaction:
            true
        case .rawBytes:
            false
        }
    }
}

enum PolkadotParsedSigningRequest {
    case legacyTransaction(PolkadotLegacyTransaction)
    case rawBytes(Data)
    case createTransaction(PolkadotParsedCreateTransaction)

    var descriptionText: String {
        switch self {
        case let .legacyTransaction(polkadotParsedTransaction):
            polkadotParsedTransaction.call.descriptionText
        case .rawBytes:
            "Raw bytes"
        case let .createTransaction(parsedCreateTx):
            parsedCreateTx.call.descriptionText
        }
    }
}

struct PolkadotParsedCreateTransaction {
    let callData: Data
    let call: PolkadotParsedTransactionCall
    let resolvedExtensions: CreateTransactionPayloadExtensions
    let genesisHash: String
    let txExtVersion: UInt8
}

struct PolkadotLegacyTransaction: Encodable {
    let address: String
    let blockHash: String
    let blockNumber: BigUInt
    let era: Era
    let genesisHash: String
    let call: PolkadotParsedTransactionCall
    let nonce: UInt32
    let specVersion: UInt32
    let tip: BigUInt
    let transactionVersion: UInt32
    let metadataHash: Data?
    let assetId: JSON?
    let withSignedTransaction: Bool
    let signedExtensions: [String]
    let version: Version
}

enum PolkadotParsedTransactionCall {
    case raw(bytes: Data)
    case callable(value: RuntimeCall<JSON>)

    var descriptionText: String {
        switch self {
        case .raw:
            "Raw call"
        case let .callable(value):
            "\(value.moduleName).\(value.callName)"
        }
    }
}

extension PolkadotParsedTransactionCall: Encodable {
    func encode(to encoder: Encoder) throws {
        switch self {
        case let .raw(bytes):
            try bytes.toHex(includePrefix: true).encode(to: encoder)
        case let .callable(value):
            try value.encode(to: encoder)
        }
    }
}

extension PolkadotLegacyTransaction {
    enum Version: UInt32, Encodable {
        case version4 = 4
        case version5 = 5
    }
}
