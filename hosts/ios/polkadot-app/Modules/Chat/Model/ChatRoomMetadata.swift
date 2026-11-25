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
    }
}

/// Stored in the single `footer` string of a room: the two plain footers by
/// name, a docked card as JSON.
extension Chat.RoomFooter {
    private struct StoredCard: Codable {
        let messageType: String
        let payload: Data
    }

    init?(storedValue: String) {
        switch storedValue {
        case "textInput":
            self = .textInput
        case "empty":
            self = .empty
        default:
            guard let card = try? JSONDecoder().decode(StoredCard.self, from: Data(storedValue.utf8)) else {
                return nil
            }
            self = .card(messageType: card.messageType, payload: card.payload)
        }
    }

    var storedValue: String {
        get throws {
            switch self {
            case .textInput:
                return "textInput"
            case .empty:
                return "empty"
            case let .card(messageType, payload):
                let card = StoredCard(messageType: messageType, payload: payload)
                guard let json = try String(bytes: JSONEncoder().encode(card), encoding: .utf8) else {
                    throw EncodingError.invalidValue(card, .init(codingPath: [], debugDescription: "not UTF-8"))
                }
                return json
            }
        }
    }
}
