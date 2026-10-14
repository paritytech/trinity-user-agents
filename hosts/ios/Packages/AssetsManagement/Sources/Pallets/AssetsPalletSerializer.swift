import Foundation
import SubstrateSdk
import BigInt
import SubstrateSdkExt

public enum AssetsPalletSerializerError: Error {
    case assetIdTypeNotFound(palletName: String?)
}

public enum AssetsPalletSerializer {}

public extension AssetsPalletSerializer {
    static func decode(
        assetId: String,
        palletName: String?,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> JSON {
        // assetId is either integer or complicated data structure
        guard assetId.isHex() else {
            return JSON.stringValue(assetId)
        }

        guard let assetIdType = extractAssetIdType(from: codingFactory, palletName: palletName) else {
            throw AssetsPalletSerializerError.assetIdTypeNotFound(palletName: palletName)
        }

        let data = try Data(hexString: assetId)

        let decoder = try codingFactory.createDecoder(from: data)

        return try decoder.read(type: assetIdType)
    }

    static func encode(
        assetId: JSON,
        palletName: String?,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> String {
        // assetId is either integer or complicated data structure
        if case let .stringValue(assetIdString) = assetId, BigUInt(assetIdString) != nil {
            return assetIdString
        }

        guard let assetIdType = extractAssetIdType(from: codingFactory, palletName: palletName) else {
            throw AssetsPalletSerializerError.assetIdTypeNotFound(palletName: palletName)
        }

        let encoder = codingFactory.createEncoder()

        try encoder.append(json: assetId, type: assetIdType)

        let data = try encoder.encode()

        return data.toHex(includePrefix: true)
    }

    static func subscriptionKeyEncoder(for assetId: String) -> ((String) throws -> Data)? {
        if assetId.isHex() {
            { try Data(hexString: $0) }
        } else {
            nil
        }
    }
}

private extension AssetsPalletSerializer {
    static func extractAssetIdType(
        from codingFactory: RuntimeCoderFactoryProtocol,
        palletName: String?
    ) -> String? {
        let callPath = AssetsPallet.Transfer.codingPath(for: palletName ?? AssetsPallet.name)

        guard let call = codingFactory.getCall(for: callPath), !call.arguments.isEmpty else {
            return nil
        }

        return call.arguments[0].type
    }
}
