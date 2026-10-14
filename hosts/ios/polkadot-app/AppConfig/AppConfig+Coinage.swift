import Foundation
import Coinage

extension AppConfig {
    enum Coinage {
        static var instanceId: CoinageInstanceId {
            AppConfigProvider.shared.getRemoteConfig()!.coinageInstanceId!
        }
    }
}
