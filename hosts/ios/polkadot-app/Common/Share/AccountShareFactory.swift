import UIKit

protocol AccountShareFactoryProtocol {
    func createSources(username: Username, qrImage: UIImage) -> [Any]
}

final class AccountShareFactory {
    private let remoteConfig: () -> RemoteAppConfig?

    init(remoteConfig: @escaping () -> RemoteAppConfig? = { AppConfigProvider.shared.getRemoteConfig() }) {
        self.remoteConfig = remoteConfig
    }
}

extension AccountShareFactory: AccountShareFactoryProtocol {
    func createSources(username: Username, qrImage: UIImage) -> [Any] {
        [qrImage, makeMessage(username: username)]
    }
}

private extension AccountShareFactory {
    func makeMessage(username: Username) -> String {
        if let link = remoteConfig()?.appSharingUrl {
            String(localized: .identityCardShareMessage(link.absoluteString, username.value))
        } else {
            String(localized: .identityCardShareMessageNoLink(username.value))
        }
    }
}
