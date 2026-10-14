import Foundation

final class AppConfigProvider {
    static let shared = AppConfigProvider()
    private let lock = NSLock()
    private var remote: RemoteAppConfig?
    private init() {}

    func apply(_ config: RemoteAppConfig) {
        lock.lock()

        defer {
            lock.unlock()
        }

        remote = config
    }

    func getRemoteConfig() -> RemoteAppConfig? {
        lock.lock()

        defer {
            lock.unlock()
        }

        return remote
    }
}
