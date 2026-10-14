import Foundation

struct UsernameAvailableResponse: Decodable {
    let usernames: [String: UsernameAvailableType]

    private enum CodingKeys: String, CodingKey {
        case value
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let entries = try container.decode([String: Entry].self, forKey: .value)

        var result: [String: UsernameAvailableType] = [:]
        for (name, entry) in entries {
            result[name] = entry.toAvailableType
        }
        usernames = result
    }

    private struct Entry: Decodable {
        let status: String
        let availableDigits: [Int]?

        var toAvailableType: UsernameAvailableType {
            switch status {
            case "AVAILABLE":
                .available(digits: availableDigits ?? [])
            case "EXHAUSTED",
                 "TAKEN":
                .taken
            default:
                .invalid
            }
        }
    }
}
