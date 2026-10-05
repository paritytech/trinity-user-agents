import Foundation
import TrUAPIHost
import UserNotifications

struct TrUAPINotifications: Sendable {
    let walletId: String?
    private var center: UNUserNotificationCenter { .current() }

    func schedule(productId: String, id: UInt32, request: HostPushNotificationRequest) async throws {
        let content = UNMutableNotificationContent()
        content.title = productId
        content.body = request.text
        content.sound = .default
        content.userInfo = [PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue]
        if let deeplink = request.deeplink {
            content.userInfo[PushNotificationKeys.deeplink] = deeplink
        }
        let seconds = request.scheduledAt.map { Double($0) / 1000 - Date().timeIntervalSince1970 } ?? 1
        try await center.add(UNNotificationRequest(
            identifier: try identifier(productId: productId, id: id),
            content: content,
            trigger: UNTimeIntervalNotificationTrigger(timeInterval: max(1, seconds), repeats: false)
        ))
    }

    func isPending(productId: String, id: UInt32) async throws -> Bool {
        let requests = await center.pendingNotificationRequests()
        let target = try identifier(productId: productId, id: id)
        return requests.contains { $0.identifier == target }
    }

    func cancel(productId: String, id: UInt32) async throws {
        let target = try identifier(productId: productId, id: id)
        center.removePendingNotificationRequests(withIdentifiers: [target])
        guard !(await center.pendingNotificationRequests()).contains(where: { $0.identifier == target }) else {
            throw HostRejection.Rejected(reason: "Notification cancellation has not completed")
        }
    }

    private func identifier(productId: String, id: UInt32) throws -> String {
        guard let walletId else { throw HostRejection.Rejected(reason: "Wallet is unavailable") }
        return "truapi:\(walletId):\(productId):\(id)"
    }
}
