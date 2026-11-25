import Foundation
import Coinage
import ChainRegistry
import Products

final class ChatFlowState {
    let extensionsRegistry: ChatExtensionsRegistering
    let callCoordinator: CallCoordinating
    let outboxService: ChatOutboxServicing
    let seenReceiptService: SeenReceiptSending
    let attachmentUploadStateProvider: AttachmentLoadProgressProvidable
    let attachmentDownloadStateProvider: AttachmentLoadProgressProvidable
    let audioSessionManager: AudioSessionManaging
    weak var foregroundVisibilityReporter: PushForegroundVisibilityReporting?

    let notificationsCleaner: any PushNotificationsCleaning
    let coinageService: CoinageServicing
    let networkStatusService: NetworkStatusProviding
    let allowanceCleanupService: AllowanceCleanupServicing
    let chatRequestCancelService: ChatRequestCancelServicing
    let flowState: SPAFlowState

    init(
        extensionsRegistry: ChatExtensionsRegistering,
        callCoordinator: CallCoordinating,
        outboxService: ChatOutboxServicing,
        seenReceiptService: SeenReceiptSending,
        attachmentUploadStateProvider: AttachmentLoadProgressProvidable,
        attachmentDownloadStateProvider: AttachmentLoadProgressProvidable,
        foregroundVisibilityReporter: PushForegroundVisibilityReporting?,
        audioSessionManager: AudioSessionManaging,
        notificationsCleaner: any PushNotificationsCleaning,
        coinageService: CoinageServicing,
        networkStatusService: NetworkStatusProviding,
        allowanceCleanupService: AllowanceCleanupServicing,
        chatRequestCancelService: ChatRequestCancelServicing,
        flowState: SPAFlowState
    ) {
        self.extensionsRegistry = extensionsRegistry
        self.callCoordinator = callCoordinator
        self.outboxService = outboxService
        self.seenReceiptService = seenReceiptService
        self.attachmentUploadStateProvider = attachmentUploadStateProvider
        self.attachmentDownloadStateProvider = attachmentDownloadStateProvider
        self.foregroundVisibilityReporter = foregroundVisibilityReporter
        self.audioSessionManager = audioSessionManager
        self.notificationsCleaner = notificationsCleaner
        self.coinageService = coinageService
        self.networkStatusService = networkStatusService
        self.allowanceCleanupService = allowanceCleanupService
        self.chatRequestCancelService = chatRequestCancelService
        self.flowState = flowState
    }
}
