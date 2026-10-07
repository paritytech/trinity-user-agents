import Foundation
import FoundationExt
import Keystore_iOS
import UserNotifications
import Individuality

protocol GameNotificationServicing {
    var localNotificationService: UserNotificationServicing { get }

    func scheduleGameStartNotifications(for gameInfo: GameInfo?)
    func scheduleRegistrationStartNotifications(for schedule: GameSchedule?, currentGameInfo: GameInfo?)
    func scheduleRegistrationOpenNotifications(for schedule: GameSchedule?)
}

class GameNotificationService {
    let localNotificationService: UserNotificationServicing
    private let gameStartReminder: any GameStartReminderServicing
    private let settingsManager: SettingsManagerProtocol
    private let logger: LoggerProtocol
    private let workQueue = DispatchQueue(label: "GameNotificationService.workQueue")

    init(
        localNotificationService: UserNotificationServicing,
        gameStartReminder: any GameStartReminderServicing,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.localNotificationService = localNotificationService
        self.gameStartReminder = gameStartReminder
        self.settingsManager = settingsManager
        self.logger = logger
    }
}

extension GameNotificationService: GameNotificationServicing {
    func scheduleGameStartNotifications(for gameInfo: GameInfo?) {
        switch gameInfo?.state {
        case .processing,
             .cancelling:
            cleanupWaitingRoomNotification()
            cleanupAboutToStartNotification()
        default:
            localNotificationService.notificationAccessStatus { [weak self] status in
                guard status.accessGranted else {
                    return
                }
                self?.workQueue.async {
                    self?.performScheduleWaitingRoomNotifications(for: gameInfo)
                    self?.performScheduleAboutToStartNotifications(for: gameInfo)
                }
            }
        }

        // Scheduling game
        guard
            let gameDate = gameInfo?.gameDate,
            let gameIndex = gameInfo?.index,
            gameInfo?.isRegistered == true
        else {
            gameStartReminder.cancelReminder()
            return
        }
        gameStartReminder.scheduleReminder(
            gameDate: gameDate,
            target: .game(gameIndex),
            timingSeconds: settingsManager.gameAlarmTimingSeconds
        )
    }

    func scheduleRegistrationStartNotifications(for schedule: GameSchedule?, currentGameInfo: GameInfo?) {
        localNotificationService.notificationAccessStatus { [weak self] status in
            guard status.accessGranted else {
                return
            }

            self?.workQueue.async {
                self?.performScheduleRegistrationStartNotifications(
                    for: schedule,
                    currentGameInfo: currentGameInfo
                )
            }
        }
    }

    func scheduleRegistrationOpenNotifications(for schedule: GameSchedule?) {
        localNotificationService.notificationAccessStatus { [weak self] status in
            guard status.accessGranted else {
                return
            }
            self?.workQueue.async {
                self?.performScheduleRegistrationOpenNotifications(for: schedule)
            }
        }
    }

    func cleanupWaitingRoomNotification() {
        workQueue.async { [weak self] in
            self?.cleanupDatedNotifications(
                identifierPrefix: Identifiers.waitingRoom,
                storedSetting: .waitingRoomNotificationDate
            )
        }
    }

    func cleanupAboutToStartNotification() {
        workQueue.async { [weak self] in
            self?.cleanupDatedNotifications(
                identifierPrefix: Identifiers.aboutToStart,
                storedSetting: .gameAboutToStartNotificationDate
            )
        }
    }
}

private extension GameNotificationService {
    enum Identifiers {
        static let waitingRoom = "game_waiting_room"
        static let registrationStart = "registration_start"
        static let aboutToStart = "game_about_to_start"
        static let registrationOpen = "registration_open"
    }

    enum TimeOffsets {
        static let waitingRoomMinutes: UInt64 = 1
        static let aboutToStartMinutes: UInt64 = 5
        static let registrationClosingMinutes: UInt64 = 60
    }

    // MARK: - Per-notification schedulers

    func performScheduleWaitingRoomNotifications(for gameInfo: GameInfo?) {
        scheduleNotifications(
            identifierPrefix: Identifiers.waitingRoom,
            storedSetting: .waitingRoomNotificationDate,
            fireDates: gameStartReminderDates(for: gameInfo, minutesBeforeStart: TimeOffsets.waitingRoomMinutes)
        ) { [self] in
            makeContent(
                title: String(localized: .Notification.gameNotificationWaitingRoomTitle),
                body: String(localized: .Notification.gameNotificationWaitingRoomBody),
                sound: .default,
                userInfo: gameStartUserInfo(gameIndex: gameInfo?.index)
            )
        }
    }

    func performScheduleAboutToStartNotifications(for gameInfo: GameInfo?) {
        let monthText = gameInfo?.gameDate.map {
            DateFormatter.fullMonth.value(for: .current).string(from: $0)
        } ?? ""
        scheduleNotifications(
            identifierPrefix: Identifiers.aboutToStart,
            storedSetting: .gameAboutToStartNotificationDate,
            fireDates: gameStartReminderDates(for: gameInfo, minutesBeforeStart: TimeOffsets.aboutToStartMinutes)
        ) { [self] in
            makeContent(
                title: String(localized: .Notification.gameNotificationRegistrationStartTitle(monthText)),
                body: String(
                    localized: .Notification.gameNotificationRegistrationStartBody(Int(TimeOffsets.aboutToStartMinutes))
                ),
                sound: .default,
                userInfo: gameStartUserInfo(gameIndex: gameInfo?.index)
            )
        }
    }

    func performScheduleRegistrationOpenNotifications(for schedule: GameSchedule?) {
        scheduleNotifications(
            identifierPrefix: Identifiers.registrationOpen,
            storedSetting: .registrationOpenNotificationDates,
            fireDates: (schedule?.items ?? []).map(\.registrationStartDate)
        ) { [self] in
            makeContent(
                title: String(localized: .Notification.gameNotificationRegistrationOpenTitle),
                body: String(localized: .Notification.gameNotificationRegistrationOpenBody),
                sound: .default,
                userInfo: registrationUserInfo
            )
        }
    }

    func performScheduleRegistrationStartNotifications(
        for schedule: GameSchedule?,
        currentGameInfo: GameInfo?
    ) {
        scheduleNotifications(
            identifierPrefix: Identifiers.registrationStart,
            storedSetting: .registrationStartNotificationDates,
            fireDates: registrationClosingDates(schedule: schedule, currentGameInfo: currentGameInfo)
        ) { [self] in
            makeContent(
                title: String(localized: .Notification.gameNotificationRegistrationClosingTitle),
                body: String(localized: .Notification.gameNotificationRegistrationClosingBody),
                sound: .default,
                userInfo: registrationUserInfo
            )
        }
    }

    // MARK: - Fire-date producers

    func gameStartReminderDates(for gameInfo: GameInfo?, minutesBeforeStart minutes: UInt64) -> [Date] {
        guard gameInfo?.isRegistered == true, let gameDate = gameInfo?.gameDate else {
            return []
        }
        return [gameDate.addingTimeInterval(-minutes.minutesToSeconds())]
    }

    func registrationClosingDates(schedule: GameSchedule?, currentGameInfo: GameInfo?) -> [Date] {
        let offset = TimeOffsets.registrationClosingMinutes.minutesToSeconds()
        let upcoming = (schedule?.items ?? []).map(\.registrationEndDate)
        let current = currentGameRegistrationCloseDate(currentGameInfo).map { [$0] } ?? []
        return (upcoming + current).map { $0.addingTimeInterval(-offset) }
    }

    func currentGameRegistrationCloseDate(_ gameInfo: GameInfo?) -> Date? {
        guard
            let gameInfo,
            case .registration = gameInfo.state,
            !gameInfo.isRegistered,
            let registrationEnds = gameInfo.registrationEnds
        else {
            return nil
        }
        return registrationEnds
    }

    // MARK: - User info

    var registrationUserInfo: [String: Any] {
        [
            PushNotificationKeys.chatExtensionId: DIM2ChatExtension.identifier,
            PushNotificationKeys.pushSource: PushNotificationSource.chat.rawValue,
            PushNotificationKeys.gameState: PushGameNotificationType.register.rawValue
        ]
    }

    func gameStartUserInfo(gameIndex: GamePallet.GameIndex?) -> [String: Any] {
        var userInfo: [String: Any] = [
            PushNotificationKeys.chatExtensionId: DIM2ChatExtension.identifier,
            PushNotificationKeys.pushSource: PushNotificationSource.chat.rawValue,
            PushNotificationKeys.gameState: PushGameNotificationType.waitingRoom.rawValue
        ]
        if let gameIndex {
            userInfo[PushNotificationKeys.gameIndex] = Int(gameIndex)
        }
        return userInfo
    }

    // MARK: - Universal scheduling

    /// Schedules one local notification per fire date under `<prefix>_<date>`, replacing the
    /// previously scheduled set whenever the dates change. Past dates are skipped; an empty
    /// `fireDates` just cancels the existing set.
    func scheduleNotifications(
        identifierPrefix: String,
        storedSetting: SettingsKey,
        fireDates: [Date],
        contentFactory: () -> UNNotificationContent
    ) {
        let dates = Array(Set(fireDates.map { Int($0.timeIntervalSinceReferenceDate) })).sorted()

        guard dates != (settingsManager.integerArray(for: storedSetting) ?? []) else {
            return
        }

        cleanupDatedNotifications(identifierPrefix: identifierPrefix, storedSetting: storedSetting)

        guard !dates.isEmpty else {
            return
        }

        settingsManager.set(intArray: dates, for: storedSetting)

        let now = Date()
        for storedDate in dates {
            let date = Date(timeIntervalSinceReferenceDate: TimeInterval(storedDate))
            guard date > now else {
                continue
            }
            let identifier = notificationIdentifier(prefix: identifierPrefix, date: storedDate)
            localNotificationService.scheduleNotification(
                withIdentifier: identifier,
                content: contentFactory(),
                after: date.timeIntervalSince(now)
            ) { [weak self] error in
                self?.logger.debug("\(identifier) scheduled with error: \(String(describing: error))")
            }
        }
    }

    func cleanupDatedNotifications(identifierPrefix: String, storedSetting: SettingsKey) {
        let identifiers = (settingsManager.integerArray(for: storedSetting) ?? [])
            .map { notificationIdentifier(prefix: identifierPrefix, date: $0) }

        if !identifiers.isEmpty {
            localNotificationService.cancelScheduledNotifications(withIdentifiers: identifiers)
            localNotificationService.removeDeliveredNotifications(withIdentifiers: identifiers)
        }
        settingsManager.removeValue(for: storedSetting)
    }

    func notificationIdentifier(prefix: String, date: Int) -> String {
        "\(prefix)_\(date)"
    }

    func makeContent(
        title: String,
        body: String,
        sound: UNNotificationSound,
        userInfo: [AnyHashable: Any]
    ) -> UNNotificationContent {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = sound
        content.userInfo = userInfo

        return content
    }
}
