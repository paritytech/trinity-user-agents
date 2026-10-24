import Foundation

/// Records the steps of a product's game reminder. A no-op in Release, where no reporting SDK is linked.
public protocol GameReminderTelemetry: Sendable {
    func record(_ event: GameReminderEvent)
}

public struct NoopGameReminderTelemetry: GameReminderTelemetry {
    public init() {}

    public func record(_: GameReminderEvent) {}
}
