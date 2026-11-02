import Foundation
@testable import polkadot_app

@available(iOS 26.1, *)
final class MockGameAlarmManager: GameAlarmManaging, @unchecked Sendable {
    var isAuthorized = true
    var scheduleError: Error?
    private(set) var scheduled: [(id: UUID, date: Date)] = []
    private(set) var cancelled: [UUID] = []

    func alarmIds() throws -> [UUID] {
        scheduled.map(\.id).filter { !cancelled.contains($0) }
    }

    func scheduleGameAlarm(id: UUID, at date: Date, playIntent _: GameAlarmPlayIntent) async throws {
        if let scheduleError {
            throw scheduleError
        }
        scheduled.append((id, date))
    }

    func cancel(id: UUID) throws {
        cancelled.append(id)
    }
}
