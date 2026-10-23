import Foundation
import Coinage
import ChainRegistry
import Products

final class ChatFlowState {
    let extensionsRegistry: ChatExtensionsRegistering
    let audioSessionManager: AudioSessionManaging
    weak var foregroundVisibilityReporter: PushForegroundVisibilityReporting?

    let notificationsCleaner: any PushNotificationsCleaning
    let coinageService: CoinageServicing
    let networkStatusService: NetworkStatusProviding
    let flowState: SPAFlowState

    init(
        extensionsRegistry: ChatExtensionsRegistering,
        foregroundVisibilityReporter: PushForegroundVisibilityReporting?,
        audioSessionManager: AudioSessionManaging,
        notificationsCleaner: any PushNotificationsCleaning,
        coinageService: CoinageServicing,
        networkStatusService: NetworkStatusProviding,
        flowState: SPAFlowState
    ) {
        self.extensionsRegistry = extensionsRegistry
        self.foregroundVisibilityReporter = foregroundVisibilityReporter
        self.audioSessionManager = audioSessionManager
        self.notificationsCleaner = notificationsCleaner
        self.coinageService = coinageService
        self.networkStatusService = networkStatusService
        self.flowState = flowState
    }
}
