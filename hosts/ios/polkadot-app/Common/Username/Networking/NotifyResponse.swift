import Foundation

struct NotifyResponse: Decodable {
    let success: Bool
    let platform: String
    let sent: Int?
    let failed: Int?
    let messageId: String?
}
