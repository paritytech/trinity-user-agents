import Foundation
import ChainRegistry
import PolkadotUI

enum ChainConnectionTarget: CaseIterable {
    case chat
    case assethub
    case bulletin

    var chainId: ChainModel.Id {
        switch self {
        case .chat:
            AppConfig.Chains.chatChain
        case .bulletin:
            AppConfig.Chains.bulletInChain
        case .assethub:
            AppConfig.Chains.assethubChain
        }
    }

    /// Deliberately not the registry's chain name — those are long ("Polkadot People") and vary by
    /// build arm, which reads badly under a 40pt ring. Not localized, as chain names never were.
    var title: String {
        switch self {
        case .chat:
            "People Chain"
        case .bulletin:
            "Bulletin Chain"
        case .assethub:
            "Hub Chain"
        }
    }

    /// Used until the registry has the chain: chains sync asynchronously, and `defaultBlockTime`
    /// is optional in the remote config.
    var fallbackBlockTime: Duration {
        switch self {
        case .chat,
             .assethub:
            .seconds(2)
        case .bulletin:
            .seconds(6)
        }
    }

    func blockTime(from chain: ChainModel?) -> Duration {
        guard let millis = chain?.defaultBlockTimeMillis, millis > 0 else {
            return fallbackBlockTime
        }

        return .milliseconds(millis)
    }

    var statusIcon: ChainStatusIcon {
        switch self {
        case .chat:
            .people
        case .bulletin:
            .bulletin
        case .assethub:
            .assetHub
        }
    }
}

extension NetworkStatus {
    var connectionState: ChainConnectionState {
        switch self {
        case .connected:
            .connected
        case .connecting:
            .connecting
        case .waitingForNetwork:
            .offline
        }
    }
}

extension ChainConnectionState {
    var localizedTitle: String {
        switch self {
        case .connected:
            String(localized: .Common.chainConnectionStatusConnected)
        case .connecting:
            String(localized: .Common.chainConnectionStatusConnecting)
        case .offline:
            String(localized: .Common.chainConnectionStatusOffline)
        }
    }
}
