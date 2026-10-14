import Foundation

extension AppConfig {
    enum Backend {
        static var baseUrl: URL {
            AppConfigProvider.shared.getRemoteConfig()!.identityBackendUrl!
        }
    }
}
