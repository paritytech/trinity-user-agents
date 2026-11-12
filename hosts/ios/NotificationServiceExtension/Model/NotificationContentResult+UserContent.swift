import UserNotifications

// MARK: - Notification Content Extension

extension NotificationContentResult {
    func toFallbackUserNotificationContent(with originalContent: UNNotificationContent? = nil)
        -> UNNotificationContent {
        let content = createBaseUserNotificationContent(with: originalContent)
        content.badge = originalContent?.badge
        return content
    }

    func toUserNotificationContent(with originalContent: UNNotificationContent? = nil) -> UNNotificationContent {
        let content = createBaseUserNotificationContent(with: originalContent)

        if let badgeCount {
            content.badge = NSNumber(value: badgeCount)
        } else {
            content.badge = originalContent?.badge
        }

        return content
    }

    private func createBaseUserNotificationContent(with originalContent: UNNotificationContent? = nil)
        -> UNMutableNotificationContent {
        let content = UNMutableNotificationContent()
        content.title = title
        content.subtitle = subtitle ?? ""
        content.sound = originalContent?.sound
        content.body = body

        var userInfo = originalContent?.userInfo ?? [:]
        if let accountId {
            userInfo[PushNotificationKeys.accountId] = accountId
        }
        if isFromBlockedContact {
            userInfo[PushNotificationKeys.blockedContact] = true
        }
        content.userInfo = userInfo

        return content
    }
}
