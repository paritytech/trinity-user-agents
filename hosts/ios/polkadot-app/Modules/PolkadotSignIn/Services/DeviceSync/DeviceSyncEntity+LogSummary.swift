import Foundation

extension [Chat.DeviceSyncEntity] {
    var syncLogSummary: String {
        map { entity in
            switch entity {
            case let .devices(devices):
                "devices:\(devices.count)"
            case let .chatsAdded(chatIds):
                "chatsAdded:\(chatIds.count)"
            case let .chatsRemoved(chatIds):
                "chatsRemoved:\(chatIds.count)"
            case let .messages(messages):
                "messages:\(messages.count)"
            }
        }.joined(separator: ";")
    }
}
