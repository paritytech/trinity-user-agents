import Foundation
import SwiftCBOR

enum CarHeaderError: Error {
    case invalidHeader
    case unsupportedVersion(Int)
    case noRoots
    case invalidCborStructure
    case missingField(String)
}

struct CarHeader {
    let version: Int
    let roots: [Data]
}

enum CarHeaderDecoder {
    /// Decodes a CARv1 CBOR header from raw bytes.
    /// The CBOR header is a map: { "version": 1, "roots": [<CID bytes>] }
    static func decode(from data: Data) throws -> CarHeader {
        let cbor = try CBOR.decode(Array(data))

        guard let cbor else {
            throw CarHeaderError.invalidCborStructure
        }

        guard case let .map(map) = cbor else {
            throw CarHeaderError.invalidCborStructure
        }

        guard case let .unsignedInt(version)? = map[.utf8String("version")] else {
            throw CarHeaderError.missingField("version")
        }

        guard version == 1 else {
            throw CarHeaderError.unsupportedVersion(Int(version))
        }

        guard case let .array(rootsArray)? = map[.utf8String("roots")] else {
            throw CarHeaderError.missingField("roots")
        }

        guard !rootsArray.isEmpty else {
            throw CarHeaderError.noRoots
        }

        let roots: [Data] = try rootsArray.map { item in
            guard let cidBytes = item.cidValue else {
                throw CarHeaderError.invalidCborStructure
            }

            // DAG-CBOR CID tag uses 0x00 identity multibase prefix — strip it
            if cidBytes.first == 0x00 {
                return Data(cidBytes.dropFirst())
            } else {
                return cidBytes
            }
        }

        return CarHeader(version: Int(version), roots: roots)
    }
}

private extension CBOR {
    var cidValue: Data? {
        switch self {
        case let .byteString(array):
            Data(array)
        case let .tagged(_, cBOR):
            cBOR.cidValue
        default:
            nil
        }
    }
}
