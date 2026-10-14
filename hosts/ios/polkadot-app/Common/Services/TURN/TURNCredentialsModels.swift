import Foundation

struct TURNIssueRequest: Hashable, Encodable {
    /// Optional region hint (reserved for future use)
    let regionHint: String?
}

struct TURNCredentials: Decodable {
    let servers: [String]
    let username: String
    let password: String
    let ttl: Int

    var stunUrls: [String] {
        servers.filter { $0.hasPrefix("stun:") }
    }

    var turnUrls: [String] {
        servers.filter { $0.hasPrefix("turn:") || $0.hasPrefix("turns:") }
    }
}
