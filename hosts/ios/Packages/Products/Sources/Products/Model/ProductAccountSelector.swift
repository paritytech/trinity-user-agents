import Foundation
import KeyDerivation
import SubstrateSdk

public enum ProductAccountSelectorError: Error, Equatable {
    case invalidRawLength(Int)
}

/// `Either<u32, [u8; 32]>` account selector within a product subtree.
/// The plain index is the primary, enumerable form; raw 32 bytes are the escape hatch
/// for byte-valued selectors. Wire forms: JSON number | `0x`-prefixed 32-byte hex string;
/// SCALE `0x00 ++ u32-LE` | `0x01 ++ 32 raw bytes`.
public enum ProductAccountSelector: Hashable, Sendable {
    case index(UInt32)
    case raw(Data)

    /// The 32-byte derivation index this selector expands to.
    public func index32() throws -> DerivationIndex32 {
        switch self {
        case let .index(value):
            DerivationIndex32(index: value)
        case let .raw(bytes):
            try DerivationIndex32(raw: bytes)
        }
    }
}

// MARK: - Codable

extension ProductAccountSelector: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()

        if let index = try? container.decode(UInt32.self) {
            self = .index(index)
            return
        }

        let hex = try container.decode(String.self)
        let bytes = try Data(hexString: hex)

        guard bytes.count == DerivationIndex32.length else {
            throw ProductAccountSelectorError.invalidRawLength(bytes.count)
        }

        self = .raw(bytes)
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()

        switch self {
        case let .index(value):
            try container.encode(value)
        case let .raw(bytes):
            try container.encode(bytes.toHex(includePrefix: true))
        }
    }
}

// MARK: - ScaleCodable

extension ProductAccountSelector: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        let variant = try UInt8(scaleDecoder: scaleDecoder)

        switch variant {
        case 0:
            self = try .index(UInt32(scaleDecoder: scaleDecoder))
        case 1:
            self = try .raw(scaleDecoder.readAndConfirm(count: DerivationIndex32.length))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .index(value):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try value.encode(scaleEncoder: scaleEncoder)
        case let .raw(bytes):
            guard bytes.count == DerivationIndex32.length else {
                throw ProductAccountSelectorError.invalidRawLength(bytes.count)
            }

            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            scaleEncoder.appendRaw(data: bytes)
        }
    }
}
