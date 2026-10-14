import Foundation
import SubstrateSdk

struct OpaqueMessageWrapper<M: ScaleCodable> {
    let message: M
}

extension OpaqueMessageWrapper: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let bytes = try Data(scaleDecoder: scaleDecoder)

        let bytesDecoder = try ScaleDecoder(data: bytes)

        message = try M(scaleDecoder: bytesDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        let bytesEncoder = ScaleEncoder()

        try message.encode(scaleEncoder: bytesEncoder)

        let bytes = bytesEncoder.encode()

        try bytes.encode(scaleEncoder: scaleEncoder)
    }
}

extension OpaqueMessageWrapper: Equatable where M: Equatable {}
