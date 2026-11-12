import Foundation
import SubstrateSdk

enum CallMediaStateSignalError: Error {
    case unknownIndex(UInt8)
}

enum CallMediaStateSignal: Equatable {
    static let cameraEnabledIndex: UInt8 = 0
    static let microphoneEnabledIndex: UInt8 = 1

    case cameraEnabled(Bool)
    case microphoneEnabled(Bool)
}

extension CallMediaStateSignal: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case Self.cameraEnabledIndex:
            let isEnabled = try Bool(scaleDecoder: scaleDecoder)
            self = .cameraEnabled(isEnabled)
        case Self.microphoneEnabledIndex:
            let isEnabled = try Bool(scaleDecoder: scaleDecoder)
            self = .microphoneEnabled(isEnabled)
        default:
            throw CallMediaStateSignalError.unknownIndex(index)
        }
    }

    func encode(scaleEncoder: ScaleEncoding) throws {
        switch self {
        case let .cameraEnabled(isEnabled):
            try Self.cameraEnabledIndex.encode(scaleEncoder: scaleEncoder)
            try isEnabled.encode(scaleEncoder: scaleEncoder)
        case let .microphoneEnabled(isEnabled):
            try Self.microphoneEnabledIndex.encode(scaleEncoder: scaleEncoder)
            try isEnabled.encode(scaleEncoder: scaleEncoder)
        }
    }
}
