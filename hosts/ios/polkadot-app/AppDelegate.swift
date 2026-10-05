import UIKit
import TipKit
import IssueMonitoring
import Keystore_iOS

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    let logger: LoggerProtocol = Logger.shared
    let issueMonitoringService: IssueMonitoringServiceProtocol = IssueMonitoringFactory.createService(
        dsn: GeneratedSecrets.sentryDSN
    )

    var apnsTokenProvider: APNSTokenProviding {
        APNSTokenProviderFacade.sharedManager
    }

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions _: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        guard
            !isUnitTesting,
            !isPreviewBuild
        else {
            return true
        }

        configureTips()

        issueMonitoringService.setup()

        #if FEATURE_DIMS
            DIM1BackgroundTaskRegistrator.shared.registerBackgroundTask()
            PersonRegistrationBackgroundTaskRegistrator.shared.registerBackgroundTask()
            PersonSelfIncludeBackgroundTaskRegistrator.shared.registerBackgroundTask()
        #endif

        UserNotificationService.shared.startGatheringNotifications()

        PushKitService.shared.register(for: [.voIP])
        application.registerForRemoteNotifications()

        return true
    }

    func application(
        _: UIApplication,
        configurationForConnecting connectingSceneSession: UISceneSession,
        options _: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        UISceneConfiguration(name: "Default Configuration", sessionRole: connectingSceneSession.role)
    }

    func application(
        _: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        apnsTokenProvider.setDeviceToken(deviceToken)
    }

    func application(
        _: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: any Error
    ) {
        logger.error("DidFailToRegisterForRemoteNotificationsWithError \(error)")
    }
}

private extension AppDelegate {
    func configureTips() {
        #if TESTNET_FEATURE
            resetTipsDatastoreIfRequested()
        #endif

        do {
            try Tips.configure()
        } catch {
            logger.error("Failed to configure TipKit: \(error)")
        }
    }

    #if TESTNET_FEATURE
        /// `resetDatastore` only works before `configure`, so the Debug Settings action flags the
        /// reset and restarts the app; the flag is consumed here on the next launch.
        func resetTipsDatastoreIfRequested() {
            let settings = SettingsManager.shared

            guard settings.value(for: .tipsResetPending) else {
                return
            }

            settings.removeValue(for: .tipsResetPending)

            do {
                try Tips.resetDatastore()
            } catch {
                logger.error("Failed to reset the tips datastore: \(error)")
            }
        }
    #endif
}

var isUnitTesting: Bool {
    #if DEBUG
        ProcessInfo.processInfo.environment.keys.contains("XCTestConfigurationFilePath") ||
            ProcessInfo.processInfo.environment.keys.contains("XCTestBundlePath")
    #else
        false
    #endif
}

var isPreviewBuild: Bool {
    #if DEBUG
        ProcessInfo.processInfo.environment["XCODE_RUNNING_FOR_PREVIEWS"] != nil
    #else
        false
    #endif
}
