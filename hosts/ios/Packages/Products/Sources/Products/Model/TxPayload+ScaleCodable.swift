import Foundation
import SubstrateSdk
import SubstrateSdkExt

extension EncodedTransactionExtensionValue: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        let id = try String(scaleDecoder: scaleDecoder)
        let explicitData = try Data(scaleDecoder: scaleDecoder)
        let implicitData = try Data(scaleDecoder: scaleDecoder)
        self.init(id: id, explicit: explicitData, implicit: implicitData)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try id.encode(scaleEncoder: scaleEncoder)
        try explicit.encode(scaleEncoder: scaleEncoder)
        try implicit.encode(scaleEncoder: scaleEncoder)
    }
}

extension CreateTransactionPayload: ScaleCodable where Signer: ScaleCodable {
    static var genesisHashSize: Int { 32 }

    public init(scaleDecoder: any ScaleDecoding) throws {
        let signer = try Signer(scaleDecoder: scaleDecoder)
        let genesisData = try scaleDecoder.readAndConfirm(count: Self.genesisHashSize)
        let callDataBytes = try Data(scaleDecoder: scaleDecoder)
        let extensions = try [EncodedTransactionExtensionValue](scaleDecoder: scaleDecoder)
        let txExtVersion = try UInt8(scaleDecoder: scaleDecoder)

        self.init(
            signer: signer,
            genesisHash: genesisData,
            callData: callDataBytes,
            extensions: extensions,
            txExtVersion: txExtVersion
        )
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try signer.encode(scaleEncoder: scaleEncoder)
        scaleEncoder.appendRaw(data: genesisHash)
        try callData.encode(scaleEncoder: scaleEncoder)
        try extensions.encode(scaleEncoder: scaleEncoder)
        try txExtVersion.encode(scaleEncoder: scaleEncoder)
    }
}
