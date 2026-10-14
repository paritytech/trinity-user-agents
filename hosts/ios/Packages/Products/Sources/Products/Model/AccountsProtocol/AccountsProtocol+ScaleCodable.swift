import Foundation
import SubstrateSdk

// MARK: - ProductProofContext + ScaleCodable

extension ProductProofContext: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        try self.init(
            productId: String(scaleDecoder: scaleDecoder),
            suffix: ProductAccountSelector(scaleDecoder: scaleDecoder)
        )
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try productId.encode(scaleEncoder: scaleEncoder)
        try suffix.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - RingLocationJunction + ScaleCodable

extension RingLocationJunction: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            self = try .palletInstance(UInt8(scaleDecoder: scaleDecoder))
        case 1:
            self = try .collectionId(Data(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .palletInstance(index):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try index.encode(scaleEncoder: scaleEncoder)
        case let .collectionId(bytes):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try bytes.encode(scaleEncoder: scaleEncoder)
        }
    }
}

// MARK: - RingLocation + ScaleCodable

extension RingLocation: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        try self.init(
            // chainId is `GenesisHash = [u8; 32]` — 32 raw bytes, no length prefix.
            chainId: scaleDecoder.readAndConfirm(count: 32),
            junctions: [RingLocationJunction](scaleDecoder: scaleDecoder)
        )
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: chainId)
        try junctions.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - ContextualAlias + ScaleCodable

extension ContextualAlias: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        // context is `[u8; 32]` on the wire — 32 raw bytes, no length prefix.
        context = try scaleDecoder.readAndConfirm(count: 32)
        alias = try Data(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: context)
        try alias.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - RingVrfProof + ScaleCodable

extension RingVrfProof: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        proof = try Data(scaleDecoder: scaleDecoder)
        contextualAlias = try ContextualAlias(scaleDecoder: scaleDecoder)
        ringIndex = try UInt32(scaleDecoder: scaleDecoder)
        ringRevision = try UInt32(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try proof.encode(scaleEncoder: scaleEncoder)
        try contextualAlias.encode(scaleEncoder: scaleEncoder)
        try ringIndex.encode(scaleEncoder: scaleEncoder)
        try ringRevision.encode(scaleEncoder: scaleEncoder)
    }
}
