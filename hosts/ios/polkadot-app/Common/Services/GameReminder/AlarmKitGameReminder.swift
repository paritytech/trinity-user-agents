import AlarmKit
import AppIntents
import Foundation
import Keystore_iOS
import Products
import SwiftUI
import Individuality
import IssueMonitoring

@available(iOS 26.1, *)
final class AlarmKitGameReminder: GameStartReminderServicing {
    private let alarmManger: AlarmManager
    private let settingsManager: SettingsManagerProtocol
    private let keys: GameReminderStorageKeys
    private let logger: LoggerProtocol
    private let telemetry: GameReminderTelemetry
    private let taskLock = NSLock()
    private var lastTask: Task<Void, Never>?

    init(
        alarmManger: AlarmManager,
        settingsManager: SettingsManagerProtocol,
        keys: GameReminderStorageKeys,
        logger: LoggerProtocol = Logger.shared,
        telemetry: GameReminderTelemetry = NoopGameReminderTelemetry()
    ) {
        self.alarmManger = alarmManger
        self.settingsManager = settingsManager
        self.keys = keys
        self.logger = logger
        self.telemetry = telemetry
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
        guard alarmManger.authorizationState == .authorized else {
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
            var event = GameReminderEvent(step: .scheduled, productId: productId, startsAt: fireDate)
            event.delivery = .alarm
            telemetry.record(event)
        } catch {
            logger.error("Failed to schedule AlarmKit alarm: \(error)")
            recordNotScheduled(.failed, gameDate: fireDate, productId: productId)
        }
    }

    private func recordNotScheduled(_ reason: GameReminderEvent.SkipReason, gameDate: Date, productId: ProductId) {
        var event = GameReminderEvent(step: .notScheduled, productId: productId, startsAt: gameDate)
        event.delivery = .alarm
        event.skipReason = reason
        telemetry.record(event)
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
            let alarms = try alarmManger.alarms
            return alarms.contains { $0.id == id }
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

        let playIntent = GameAlarmPlayIntent()
        playIntent.alarmID = newAlarmId.uuidString
        playIntent.productId = productId
        playIntent.startsAt = date

        _ = try await alarmManger.schedule(
            id: newAlarmId,
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
