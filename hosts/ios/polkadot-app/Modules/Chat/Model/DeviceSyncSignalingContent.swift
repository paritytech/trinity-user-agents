import Foundation
import SubstrateSdk

// MARK: - Model

extension Chat {
    enum DeviceSyncSignalingContent: Equatable {
        case reconnected
        case offer(Data)
        case answer(Data)
        case candidates(Data)

        var isOffer: Bool {
            if case .offer = self { return true }
            return false
        }
    }

    struct DeviceSyncSignalingEnvelope: Equatable {
        let offerId: String
        let message: DeviceSyncSignalingContent
    }
}

// MARK: - ScaleCodable

extension Chat.DeviceSyncSignalingContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = .reconnected
        case 1:
            self = try .offer(Data(scaleDecoder: scaleDecoder))
        case 2:
            self = try .answer(Data(scaleDecoder: scaleDecoder))
        case 3:
            self = try .candidates(Data(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case .reconnected:
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
        case let .offer(sdpData):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try sdpData.encode(scaleEncoder: scaleEncoder)
        case let .answer(sdpData):
            try UInt8(2).encode(scaleEncoder: scaleEncoder)
            try sdpData.encode(scaleEncoder: scaleEncoder)
        case let .candidates(candidatesData):
            try UInt8(3).encode(scaleEncoder: scaleEncoder)
            try candidatesData.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.DeviceSyncSignalingEnvelope: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        offerId = try String(scaleDecoder: scaleDecoder)
        message = try Chat.DeviceSyncSignalingContent(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try offerId.encode(scaleEncoder: scaleEncoder)
        try message.encode(scaleEncoder: scaleEncoder)
    }
}
