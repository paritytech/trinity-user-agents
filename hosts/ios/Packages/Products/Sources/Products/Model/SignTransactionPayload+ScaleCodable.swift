import Foundation
import SubstrateSdk

extension SignTransactionPayload: ScaleCodable where Signer: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        let account = try Signer(scaleDecoder: scaleDecoder)
        let blockHash = try Data(scaleDecoder: scaleDecoder)
        let blockNumber = try Data(scaleDecoder: scaleDecoder)
        let era = try Data(scaleDecoder: scaleDecoder)
        let genesisHash = try Data(scaleDecoder: scaleDecoder)
        let method = try Data(scaleDecoder: scaleDecoder)
        let nonce = try Data(scaleDecoder: scaleDecoder)
        let specVersion = try Data(scaleDecoder: scaleDecoder)
        let tip = try Data(scaleDecoder: scaleDecoder)
        let transactionVersion = try Data(scaleDecoder: scaleDecoder)
        let signedExtensions = try [String](scaleDecoder: scaleDecoder)
        let version = try UInt32(scaleDecoder: scaleDecoder)
        let assetId = try ScaleOption<Data>(scaleDecoder: scaleDecoder).value
        let metadataHash = try ScaleOption<Data>(scaleDecoder: scaleDecoder).value
        let mode = try ScaleOption<UInt32>(scaleDecoder: scaleDecoder).value
        let withSignedTransaction = try ScaleBoolOption(scaleDecoder: scaleDecoder).value

        self.init(
            account: account,
            blockHash: blockHash,
            blockNumber: blockNumber,
            era: era,
            genesisHash: genesisHash,
            method: method,
            nonce: nonce,
            specVersion: specVersion,
            tip: tip,
            transactionVersion: transactionVersion,
            signedExtensions: signedExtensions,
            version: version,
            assetId: assetId,
            metadataHash: metadataHash,
            mode: mode,
            withSignedTransaction: withSignedTransaction
        )
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try account.encode(scaleEncoder: scaleEncoder)
        try blockHash.encode(scaleEncoder: scaleEncoder)
        try blockNumber.encode(scaleEncoder: scaleEncoder)
        try era.encode(scaleEncoder: scaleEncoder)
        try genesisHash.encode(scaleEncoder: scaleEncoder)
        try method.encode(scaleEncoder: scaleEncoder)
        try nonce.encode(scaleEncoder: scaleEncoder)
        try specVersion.encode(scaleEncoder: scaleEncoder)
        try tip.encode(scaleEncoder: scaleEncoder)
        try transactionVersion.encode(scaleEncoder: scaleEncoder)
        try signedExtensions.encode(scaleEncoder: scaleEncoder)
        try version.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: assetId).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: metadataHash).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: mode).encode(scaleEncoder: scaleEncoder)
        try ScaleBoolOption(value: withSignedTransaction).encode(scaleEncoder: scaleEncoder)
    }
}
