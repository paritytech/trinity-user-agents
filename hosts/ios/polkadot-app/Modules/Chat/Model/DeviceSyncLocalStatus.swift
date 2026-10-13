import Foundation
import SubstrateSdk

// MARK: - Model

extension Chat {
    enum DeviceSyncLocalStatus: Equatable {
        case outgoing(DeviceSyncOutgoingStatus)
        case incoming(DeviceSyncIncomingStatus)
    }

    enum DeviceSyncOutgoingStatus: UInt8, Equatable {
        case new = 0
        case sent = 1
        case delivered = 2
    }

    enum DeviceSyncIncomingStatus: UInt8, Equatable {
        case new = 0
        case seen = 1
    }
}

// MARK: - Local Mapping

extension Chat.DeviceSyncLocalStatus {
    init(from local: Chat.LocalMessage.Status) {
        switch local {
        case .outgoing(.new): self = .outgoing(.new)
        case .outgoing(.sent): self = .outgoing(.sent)
        case .outgoing(.delivered): self = .outgoing(.delivered)
        case .outgoing(.failed): self = .outgoing(.new)
        case .incoming(.new): self = .incoming(.new)
        case .incoming(.seen): self = .incoming(.seen)
        }
    }

    func toLocal() -> Chat.LocalMessage.Status {
        switch self {
        case .outgoing(.new): .outgoing(.new)
        case .outgoing(.sent): .outgoing(.sent)
        case .outgoing(.delivered): .outgoing(.delivered)
        case .incoming(.new): .incoming(.new)
        case .incoming(.seen): .incoming(.seen)
        }
    }
}

// MARK: - ScaleCodable

extension Chat.DeviceSyncLocalStatus: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            let sub = try Chat.DeviceSyncOutgoingStatus(scaleDecoder: scaleDecoder)
            self = .outgoing(sub)
        case 1:
            let sub = try Chat.DeviceSyncIncomingStatus(scaleDecoder: scaleDecoder)
            self = .incoming(sub)
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .outgoing(sub):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try sub.encode(scaleEncoder: scaleEncoder)
        case let .incoming(sub):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try sub.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.DeviceSyncOutgoingStatus: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = Self(rawValue: index) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self = value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.DeviceSyncIncomingStatus: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = Self(rawValue: index) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self = value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}
