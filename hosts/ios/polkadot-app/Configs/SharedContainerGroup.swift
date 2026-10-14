import Foundation

enum SharedContainerGroup {
    static var name: String { AppConfig.Brand.appGroup }

    static var containerURL: URL {
        guard let url = FileManager.default
            .containerURL(forSecurityApplicationGroupIdentifier: name) else {
            fatalError(
                "App Group container '\(name)' missing — entitlement not applied"
            )
        }
        return url
    }

    static var userDefaults: UserDefaults {
        // Resolve the container first. UserDefaults(suiteName:) returns a working but EMPTY
        // suite for any well-formed identifier, granted or not, and this suite indexes the
        // Keychain (root entropy id, device encryption id) — so an ungranted group must trap
        // here rather than silently onboard the user into an empty store.
        _ = containerURL

        guard let defaults = UserDefaults(suiteName: name) else {
            fatalError("Failed to create UserDefaults for suite: \(name)")
        }
        return defaults
    }
}
