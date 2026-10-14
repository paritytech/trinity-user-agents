import Foundation
import SubstrateSdk
import Operation_iOS
import SubstrateSdkExt

extension Chat.Id {
    // DON'T change indexes to be in sync with db
    enum ChatType: UInt8 {
        case person = 0
        case chatExtension = 1
    }

    var chatType: UInt8 {
        switch self {
        case .person:
            ChatType.person.rawValue
        case .chatExtension:
            ChatType.chatExtension.rawValue
        }
    }

    var chatTypeContext: String {
        switch self {
        case let .person(accountId):
            accountId.toHex()
        case let .chatExtension(extensionId, _):
            extensionId
        }
    }
}

extension Chat.Id {
    var rawRepresentation: String {
        var components = [String(chatType)]

        switch self {
        case let .person(accountId):
            components.append(accountId.toHex())
        case let .chatExtension(extId, roomId):
            components.append(extId)

            if let roomId {
                components.append(roomId)
            }
        }

        return components.joined(with: .colon)
    }

    static func fromRawRepresentation(_ identifier: String) -> Chat.Id? {
        let components = identifier.split(by: .colon, maxSplits: 1)

        guard
            components.count == 2,
            let rawChatType = UInt8(components[0]),
            let chatType = ChatType(rawValue: rawChatType) else {
            return nil
        }

        switch chatType {
        case .person:
            guard let accountId = try? components[1].fromHex() else {
                return nil
            }

            return .person(accountId)
        case .chatExtension:
            let extensionComponents = components[1].split(by: .colon, maxSplits: 1)

            guard !extensionComponents.isEmpty else {
                return nil
            }

            let extensionId = extensionComponents[0]
            let roomId: String? = extensionComponents.count == 2 ? extensionComponents[1] : nil
            return .chatExtension(extensionId, roomId: roomId)
        }
    }
}
