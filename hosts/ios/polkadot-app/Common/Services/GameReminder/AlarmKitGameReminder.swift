import AlarmKit
import AppIntents
import Foundation
import Keystore_iOS
import Products
import SwiftUI
import Individuality

/// The AlarmKit calls a game reminder makes, so the reminder can be exercised without the system alarm store.
@available(iOS 26.1, *)
protocol GameAlarmManaging {
    var isAuthorized: Bool { get }
    func alarmIds() throws -> [UUID]
    func scheduleGameAlarm(id: UUID, at date: Date, playIntent: GameAlarmPlayIntent) async throws
    func cancel(id: UUID) throws
}

@available(iOS 26.1, *)
final class AlarmKitGameReminder: GameStartReminderServicing {
    private let alarmManger: GameAlarmManaging
    private let settingsManager: SettingsManagerProtocol
    private let keys: GameReminderStorageKeys
    private let logger: LoggerProtocol
    private let diagnostics: GameReminderDiagnostics
    private let taskLock = NSLock()
    private var lastTask: Task<Void, Never>?

    init(
        alarmManger: GameAlarmManaging,
        settingsManager: SettingsManagerProtocol,
        keys: GameReminderStorageKeys,
        logger: LoggerProtocol = Logger.shared,
        diagnostics: GameReminderDiagnostics = GameReminderDiagnostics()
    ) {
        self.alarmManger = alarmManger
        self.settingsManager = settingsManager
        self.keys = keys
        self.logger = logger
        self.diagnostics = diagnostics
    }

    func scheduleReminder(gameDate: Date, productId: ProductId) {
        enqueue { [weak self] in
            await self?.performScheduleReminder(gameDate: gameDate, productId: productId)
        }
    }

    func cancelReminder() {
        enqueue { [weak self] in
            self?.performCancelReminder(alarmId: nil)
        }
    }

    // AlarmKit schedule/cancel calls must run one after another. Otherwise
    // parallel schedule requests can create several alarms while only the last
    // id is saved in settings, leaving earlier alarms impossible to cancel.
    private func enqueue(_ work: @escaping () async -> Void) {
        taskLock.lock()
        let previousTask = lastTask

        let task = Task { [previousTask] in
            await previousTask?.value
            await work()
        }

        lastTask = task
        taskLock.unlock()
    }

    private func performScheduleReminder(gameDate fireDate: Date, productId: ProductId) async {
        guard alarmManger.isAuthorized else {
            logger.debug("Not authorized, skipping schedule")
            recordNotScheduled(.notAuthorized, gameDate: fireDate, productId: productId)
            return
        }

        guard fireDate > Date() else {
            logger.debug("Fire date is in the past, skipping")
            recordNotScheduled(.startInPast, gameDate: fireDate, productId: productId)
            performCancelReminder(alarmId: nil)
            return
        }

        let storedFireDate = settingsManager.integer(for: keys.alarmFireDate)
        let storedAlarmId = settingsManager.string(for: keys.alarmId).flatMap(UUID.init(uuidString:))

        if storedFireDate == Int(fireDate.timeIntervalSinceReferenceDate),
           let storedAlarmId,
           isAlarmScheduled(id: storedAlarmId) {
            logger.debug("Already scheduled, skipping")
            return
        }

        if let storedAlarmId {
            performCancelReminder(alarmId: storedAlarmId)
        }

        let newAlarmId = UUID()

        do {
            try await schedule(id: newAlarmId, at: fireDate, productId: productId)
            settingsManager.set(string: newAlarmId.uuidString, for: keys.alarmId)
            settingsManager.set(value: Int(fireDate.timeIntervalSinceReferenceDate), for: keys.alarmFireDate)
            logger.debug("Alarm scheduled \(newAlarmId) for \(fireDate)")
            diagnostics.record(
                GameReminderEvent(step: .scheduled, productId: productId, startsAt: fireDate, delivery: .alarm)
            )
        } catch {
            logger.error("Failed to schedule AlarmKit alarm: \(error)")
            recordNotScheduled(.failed, gameDate: fireDate, productId: productId)
        }
    }

    private func recordNotScheduled(_ reason: GameReminderEvent.SkipReason, gameDate: Date, productId: ProductId) {
        diagnostics.record(
            GameReminderEvent(step: .notScheduled, productId: productId, startsAt: gameDate, skipReason: reason)
        )
    }

    private func performCancelReminder(alarmId: UUID?) {
        guard let alarmId = alarmId ?? settingsManager
            .string(for: keys.alarmId)
            .flatMap(UUID.init(uuidString:))
        else {
            return
        }

        settingsManager.removeValue(for: keys.alarmId)
        settingsManager.removeValue(for: keys.alarmFireDate)

        do {
            try alarmManger.cancel(id: alarmId)
            logger.debug("Alarm cancelled \(alarmId)")
        } catch {
            logger.error("Failed to cancel alarm: \(error)")
        }
    }

    private func isAlarmScheduled(id: UUID) -> Bool {
        do {
            return try alarmManger.alarmIds().contains(id)
        } catch {
            logger.error("Failed to fetch scheduled alarms: \(error)")
            return false
        }
    }

    private func schedule(
        id newAlarmId: UUID,
        at date: Date,
        productId: ProductId
    ) async throws {
        let playIntent = GameAlarmPlayIntent()
        playIntent.alarmID = newAlarmId.uuidString
        playIntent.productId = productId

        try await alarmManger.scheduleGameAlarm(id: newAlarmId, at: date, playIntent: playIntent)
    }
}

@available(iOS 26.1, *)
extension AlarmManager: GameAlarmManaging {
    var isAuthorized: Bool {
        authorizationState == .authorized
    }

    func alarmIds() throws -> [UUID] {
        try alarms.map(\.id)
    }

    func scheduleGameAlarm(id: UUID, at date: Date, playIntent: GameAlarmPlayIntent) async throws {
        let attributes = AlarmAttributes(
            presentation: AlarmPresentation(
                alert: .init(
                    title: .Notification.gameNotificationGameStartTitle,
                    secondaryButton: .init(
                        text: .Notification.gameAlarmPlayGame,
                        textColor: .textAndIconsPrimaryDark,
                        systemImageName: "figure.walk.arrival"
                    ),
                    secondaryButtonBehavior: .custom
                )
            ),
            metadata: GameAlarmMetadata(),
            tintColor: Color.accentColor
        )

        _ = try await schedule(
            id: id,
            configuration: AlarmManager.AlarmConfiguration(
                countdownDuration: nil,
                schedule: .fixed(date),
                attributes: attributes,
                stopIntent: nil,
                secondaryIntent: playIntent,
                sound: .named("game_alarm.caf")
            )
        )
    }
}

@available(iOS 26.1, *)
struct GameAlarmMetadata: AlarmMetadata {}
