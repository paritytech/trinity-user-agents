import AlarmKit
import AppIntents
import Foundation
import Keystore_iOS
import SwiftUI
import Individuality

@available(iOS 26.1, *)
final class AlarmKitGameReminder: GameStartReminderServicing {
    private let alarmManger: AlarmManager
    private let settingsManager: SettingsManagerProtocol
    private let keys: GameReminderStorageKeys
    private let logger: LoggerProtocol
    private let taskLock = NSLock()
    private var lastTask: Task<Void, Never>?

    init(
        alarmManger: AlarmManager,
        settingsManager: SettingsManagerProtocol,
        keys: GameReminderStorageKeys = .game,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.alarmManger = alarmManger
        self.settingsManager = settingsManager
        self.keys = keys
        self.logger = logger
    }

    func scheduleReminder(gameDate: Date, target: GameReminderTarget, timingSeconds: Int) {
        enqueue { [weak self] in
            await self?.performScheduleReminder(
                gameDate: gameDate,
                target: target,
                timingSeconds: timingSeconds
            )
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

    private func performScheduleReminder(
        gameDate: Date,
        target: GameReminderTarget,
        timingSeconds: Int
    ) async {
        guard alarmManger.authorizationState == .authorized else {
            logger.debug("Not authorized, skipping schedule")
            return
        }

        let fireDate = gameDate.addingTimeInterval(-TimeInterval(timingSeconds))

        guard fireDate > Date() else {
            logger.debug("Fire date is in the past, skipping")
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
            try await schedule(id: newAlarmId, at: fireDate, timingSeconds: timingSeconds, target: target)
            settingsManager.set(string: newAlarmId.uuidString, for: keys.alarmId)
            settingsManager.set(value: Int(fireDate.timeIntervalSinceReferenceDate), for: keys.alarmFireDate)
            logger.debug("Alarm scheduled \(newAlarmId) for \(fireDate)")
        } catch {
            logger.error("Failed to schedule AlarmKit alarm: \(error)")
        }
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
        timingSeconds: Int,
        target: GameReminderTarget
    ) async throws {
        let attributes = AlarmAttributes(
            presentation: AlarmPresentation(
                alert: .init(
                    title: .Notification.gameNotificationGameStartTitle(String(timingSeconds)),
                    secondaryButton: .init(
                        text: .Notification.gameAlarmPlayGame,
                        textColor: .textAndIconsPrimaryDark,
                        systemImageName: "figure.walk.arrival"
                    ),
                    secondaryButtonBehavior: .custom
                )
            ),
            metadata: GameAlarmMetadata(timingSeconds: timingSeconds),
            tintColor: Color.accentColor
        )

        let playIntent = GameAlarmPlayIntent()
        playIntent.alarmID = newAlarmId.uuidString
        switch target {
        case let .game(gameIndex):
            playIntent.gameIndex = Int(gameIndex)
        case let .product(productId):
            playIntent.productId = productId
        }

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
struct GameAlarmMetadata: AlarmMetadata {
    let timingSeconds: Int
}
