import Foundation

extension Chat.Id {
    var colorSeed: String {
        switch self {
        case let .person(accountId):
            accountId.toHex()
        case let .chatExtension(extId, _):
            extId
        }
    }
}
