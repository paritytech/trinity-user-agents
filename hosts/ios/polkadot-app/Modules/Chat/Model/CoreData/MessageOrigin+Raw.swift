import Foundation
import SubstrateSdk
import SubstrateSdkExt

extension Chat.LocalMessage.Origin {
    var rawType: Int16 {
        switch self {
        case .user:
            0
        case .contact:
            1
        case .chatExtension:
            2
        }
    }

    var rawKey: String? {
        switch self {
        case .user:
            nil
        case let .contact(accountId):
            accountId.toHex()
        case let .chatExtension(extId):
            extId
        }
    }

    init?(rawType: Int16, rawKey: String?) {
        switch rawType {
        case 0:
            self = .user
        case 1:
            guard let accountId = try? rawKey?.fromHex() else {
                return nil
            }

            self = .contact(accountId)
        case 2:
            guard let extId = rawKey else {
                return nil
            }

            self = .chatExtension(extId)
        default:
            return nil
        }
    }
}
