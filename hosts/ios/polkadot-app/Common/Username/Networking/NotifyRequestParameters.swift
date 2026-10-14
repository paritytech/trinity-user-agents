import Foundation

struct NotifyRequestParameters: Encodable {
    let deviceToken: String
    let pushId: String
    let bundlerId: String
    let platform: String
    let message: String
    let voip: Bool
}
