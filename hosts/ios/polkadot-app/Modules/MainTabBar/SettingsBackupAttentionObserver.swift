import Foundation
import EventCenter

#if !FEATURE_PRODUCTS
    protocol SettingsBackupAttentionObserving: AnyObject {
        func start(onChange: @escaping @MainActor (Bool) -> Void)
    }

    /// Reports whether the Settings tab needs the no-backup attention dot.
    final class SettingsBackupAttentionObserver: SettingsBackupAttentionObserving {
        private let mnemonicBackupHelper: MnemonicBackupHelperProtocol
        private let notificationCenter: NotificationCenter
        private let eventCenter: EventCenterProtocol
        private let logger: LoggerProtocol
        private var availabilityObserver: NSObjectProtocol?
        private var onChange: (@MainActor (Bool) -> Void)?

        init(
            mnemonicBackupHelper: MnemonicBackupHelperProtocol = MnemonicBackupHelper(),
            notificationCenter: NotificationCenter = .default,
            eventCenter: EventCenterProtocol = EventCenter.shared,
            logger: LoggerProtocol = Logger.shared
        ) {
            self.mnemonicBackupHelper = mnemonicBackupHelper
            self.notificationCenter = notificationCenter
            self.eventCenter = eventCenter
            self.logger = logger
        }

        deinit {
            if let availabilityObserver {
                notificationCenter.removeObserver(availabilityObserver)
            }
        }

        func start(onChange: @escaping @MainActor (Bool) -> Void) {
            self.onChange = onChange

            guard availabilityObserver == nil else {
                return
            }

            availabilityObserver = notificationCenter.addObserver(
                forName: mnemonicBackupHelper.didChangeAvailabilityNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                self?.evaluate()
            }
            eventCenter.add(observer: self, dispatchIn: .main)
            evaluate()
        }
    }

    extension SettingsBackupAttentionObserver: AppEventVisiting {
        func processBackupStatusChanged(event _: BackupStatusChanged) {
            evaluate()
        }
    }

    private extension SettingsBackupAttentionObserver {
        func evaluate() {
            guard let onChange else {
                return
            }
            Task { [self] in
                await onChange(needsAttention())
            }
        }

        func needsAttention() -> Bool {
            guard mnemonicBackupHelper.isAvailable else {
                return true
            }

            do {
                return try !mnemonicBackupHelper.checkForBackup()
            } catch {
                logger.warning("Failed to check backup status: \(error.localizedDescription)")
                return true
            }
        }
    }
#endif
