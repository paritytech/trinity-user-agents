import Foundation
import SubstrateSdk

// MARK: - Model

extension Chat {
    enum DeviceSyncEntity: Equatable {
        case devices([DeviceSyncWireDevice])
        case chatsAdded([DeviceSyncChatId])
        case chatsRemoved([DeviceSyncChatId])
        case messages([DeviceSyncWireMessage])

        var logLabel: String {
            switch self {
            case .devices: "devices"
            case .chatsAdded: "chatsAdded"
            case .chatsRemoved: "chatsRemoved"
            case .messages: "messages"
            }
        }
    }

    struct DeviceSyncUpdate: Equatable {
        let id: UInt32
        let entities: [DeviceSyncEntity]
        let timePoint: UInt64
    }

    struct DeviceSyncUpdateAck: Equatable {
        let id: UInt32
    }

    enum DeviceSyncMessage: Equatable {
        case update(DeviceSyncUpdate)
        case ack(DeviceSyncUpdateAck)
    }
}

// MARK: - ScaleCodable: DeviceSyncEntity

extension Chat.DeviceSyncEntity: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = try .devices([Chat.DeviceSyncWireDevice](scaleDecoder: scaleDecoder))
        case 1:
            self = try .chatsAdded([Chat.DeviceSyncChatId](scaleDecoder: scaleDecoder))
        case 2:
            self = try .chatsRemoved([Chat.DeviceSyncChatId](scaleDecoder: scaleDecoder))
        case 3:
            self = try .messages([Chat.DeviceSyncWireMessage](scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .devices(devices):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try devices.encode(scaleEncoder: scaleEncoder)
        case let .chatsAdded(chatIds):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try chatIds.encode(scaleEncoder: scaleEncoder)
        case let .chatsRemoved(chatIds):
            try UInt8(2).encode(scaleEncoder: scaleEncoder)
            try chatIds.encode(scaleEncoder: scaleEncoder)
        case let .messages(messages):
            try UInt8(3).encode(scaleEncoder: scaleEncoder)
            try messages.encode(scaleEncoder: scaleEncoder)
        }
    }
}

// MARK: - ScaleCodable: DeviceSyncUpdate & DeviceSyncUpdateAck

extension Chat.DeviceSyncUpdate: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        id = try UInt32(scaleDecoder: scaleDecoder)
        entities = try [Chat.DeviceSyncEntity](scaleDecoder: scaleDecoder)
        timePoint = try UInt64(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try id.encode(scaleEncoder: scaleEncoder)
        try entities.encode(scaleEncoder: scaleEncoder)
        try timePoint.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.DeviceSyncUpdateAck: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        id = try UInt32(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try id.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - ScaleCodable: DeviceSyncMessage

extension Chat.DeviceSyncMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            self = try .update(Chat.DeviceSyncUpdate(scaleDecoder: scaleDecoder))
        case 1:
            self = try .ack(Chat.DeviceSyncUpdateAck(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .update(update):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try update.encode(scaleEncoder: scaleEncoder)
        case let .ack(ack):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try ack.encode(scaleEncoder: scaleEncoder)
        }
    }
}
