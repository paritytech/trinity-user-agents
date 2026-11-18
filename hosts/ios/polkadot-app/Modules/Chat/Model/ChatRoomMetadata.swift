import Foundation

extension Chat {
    struct RoomMetadata: Equatable {
        let chatRelativeId: String
        let name: String?
        let icon: String?
    }

    /// What a room shows below its messages.
    enum RoomFooter: Equatable {
        case textInput
        case empty
        case card(messageType: String, payload: Data)
        /// A page of the product's own web content, `path` relative to its web root.
        case web(path: String, height: UInt32)
    }
}

/// Stored in the single `footer` string of a room: the two plain footers by
/// name, a docked card or web page as JSON.
extension Chat.RoomFooter {
    private struct StoredCard: Codable {
        let messageType: String
        let payload: Data
    }

    private struct StoredWeb: Codable {
        struct Page: Codable {
            let path: String
            let height: UInt32
        }

        let web: Page
    }

    init?(storedValue: String) {
        switch storedValue {
        case "textInput":
            self = .textInput
        case "empty":
            self = .empty
        default:
            let json = Data(storedValue.utf8)
            if let card = try? JSONDecoder().decode(StoredCard.self, from: json) {
                self = .card(messageType: card.messageType, payload: card.payload)
            } else if let stored = try? JSONDecoder().decode(StoredWeb.self, from: json) {
                self = .web(path: stored.web.path, height: stored.web.height)
            } else {
                return nil
            }
        }
    }

    var storedValue: String {
        get throws {
            switch self {
            case .textInput:
                "textInput"
            case .empty:
                "empty"
            case let .card(messageType, payload):
                try Self.json(StoredCard(messageType: messageType, payload: payload))
            case let .web(path, height):
                try Self.json(StoredWeb(web: .init(path: path, height: height)))
            }
        }
    }

    private static func json(_ value: some Encodable) throws -> String {
        guard let json = try String(bytes: JSONEncoder().encode(value), encoding: .utf8) else {
            throw EncodingError.invalidValue(value, .init(codingPath: [], debugDescription: "not UTF-8"))
        }
        return json
    }
}
