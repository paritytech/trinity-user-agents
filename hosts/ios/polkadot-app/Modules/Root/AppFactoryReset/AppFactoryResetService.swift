#if TESTNET_FEATURE
    import Foundation
    import Kingfisher
    import Security
    import UserNotifications
    import Keystore_iOS
    import Operation_iOS
    import AlarmKit
    import Products

    /// The scene restart that follows keeps the process, so in-memory state derived from the wiped data
    /// (the cached TLD, the applied remote config) is torn down here alongside the storage.
    final class AppFactoryResetService {
        private let tldProvider: DotNsTldProviding
        private let remoteConfig: AppliedConfigDiscarding
        private let mnemonicBackupHelper: MnemonicBackupHelperProtocol
        private let notificationCenter: UNUserNotificationCenter
        private let logger: LoggerProtocol

        init(
            tldProvider: DotNsTldProviding,
            remoteConfig: AppliedConfigDiscarding,
            mnemonicBackupHelper: MnemonicBackupHelperProtocol,
            notificationCenter: UNUserNotificationCenter = .current(),
            logger: LoggerProtocol
        ) {
            self.tldProvider = tldProvider
            self.remoteConfig = remoteConfig
            self.mnemonicBackupHelper = mnemonicBackupHelper
            self.notificationCenter = notificationCenter
            self.logger = logger
        }

        func resetAllData() async {
            if #available(iOS 26.0, *) {
                clearAlarmKitAlarms()
            }
            deleteAllKeychainItems()
            eraseAllUserDefaults()
            tldProvider.reset()
            await remoteConfig.discardAppliedConfig()
            clearAllNotifications()
            deleteCloudBackup()
            deleteCoreDataDatabases()
            clearSDKCaches()

            logger.debug("App reset performed")
        }
    }

    private extension AppFactoryResetService {
        func deleteAllKeychainItems() {
            let classes: [CFString] = [
                kSecClassKey,
                kSecClassGenericPassword,
                kSecClassInternetPassword,
                kSecClassCertificate,
                kSecClassIdentity
            ]

            for secClass in classes {
                let query: [CFString: Any] = [kSecClass: secClass]
                let status = SecItemDelete(query as CFDictionary)
                if status != errSecSuccess, status != errSecItemNotFound {
                    logger.error("Keychain wipe failed for class \(secClass), status=\(status)")
                }
            }
        }

        @available(iOS 26.0, *)
        func clearAlarmKitAlarms() {
            do {
                for alarm in try AlarmManager.shared.alarms {
                    try AlarmManager.shared.cancel(id: alarm.id)
                }
            } catch {
                logger.error("Failure to cancel alarm: \(error)")
            }
        }

        func clearAllNotifications() {
            notificationCenter.removeAllPendingNotificationRequests()
            notificationCenter.removeAllDeliveredNotifications()
        }

        func deleteCloudBackup() {
            do {
                try mnemonicBackupHelper.deleteMnemonic()
            } catch {
                logger.error("Failed to delete cloud backup: \(error)")
            }
        }

        func eraseAllUserDefaults() {
            SettingsManager.shared.removeAll()

            for suiteName in [SharedContainerGroup.name, ContentHashCache.suiteName] {
                let defaults = UserDefaults(suiteName: suiteName)
                defaults?.removePersistentDomain(forName: suiteName)
                defaults?.synchronize()
            }
        }

        /// Both stores share one directory and drop() deletes the whole directory, so every store is closed
        /// before any is dropped; otherwise the first drop deletes the second store's file while it is open.
        func deleteCoreDataDatabases() {
            let stores: [(String, CoreDataServiceProtocol)] = [
                ("UserData", UserDataStorageFacade.shared.databaseService),
                ("SubstrateData", SubstrateDataStorageFacade.shared.databaseService)
            ]

            for (name, service) in stores {
                do {
                    try service.close()
                } catch {
                    logger.error("Failed to close \(name): \(error)")
                }
            }

            for (name, service) in stores {
                do {
                    try dropClosed(service)
                } catch {
                    logger.error("Failed to wipe \(name): \(error)")
                }
            }
        }

        /// Any access reopens a closed store, so one that was touched after the close is closed again.
        func dropClosed(_ service: CoreDataServiceProtocol) throws {
            do {
                try service.drop()
            } catch CoreDataServiceError.unexpectedDropWhenOpen {
                try service.close()
                try service.drop()
            }
        }

        func clearSDKCaches() {
            KingfisherManager.shared.cache.clearMemoryCache()
            KingfisherManager.shared.cache.clearDiskCache()
        }
    }
#endif
