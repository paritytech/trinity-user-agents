import Foundation
import IssueMonitoring
import SDKLogger
import SubstrateSdk

/// Free notification slots of a period: full person before lite, ascending seq within each. A slot reserved
/// for the asking target counts as free for it; one reserved for anyone else never does.
final class NotificationSeqPicker: @unchecked Sendable {
    private let sources: NotificationSlotSources
    private let reservations: NotificationSeqReservations
    private let unsupportedRuntime: IssueDiagnostic
    private let logger: SDKLoggerProtocol

    init(
        sources: NotificationSlotSources,
        reservations: NotificationSeqReservations,
        unsupportedRuntime: IssueDiagnostic,
        logger: SDKLoggerProtocol
    ) {
        self.sources = sources
        self.reservations = reservations
        self.unsupportedRuntime = unsupportedRuntime
        self.logger = logger
    }

    func freeSlots(period: UInt32, forTarget target: AccountId?) async throws -> [NotificationSlot] {
        guard try await sources.repository.isSupported() else {
            logger.warning("Notification slots are not supported by the runtime")
            unsupportedRuntime.recordFailure(for: "runtime", error: nil, counters: [:])
            return []
        }

        let origins = try await sources.originPersonProvider.pickPersonOrigins()
        let networkSuffix = try await sources.networkSuffixProvider.networkSuffix()
        let reserved = reservations.reserved(in: period, exceptFor: target)

        var free: [NotificationSlot] = []
        for origin in origins {
            let unregistered = try await unregisteredSlots(in: origin, period: period, networkSuffix: networkSuffix)
            free += unregistered.filter { !reserved.contains($0.key) }
        }

        logger.debug("Notification slots of period \(period): \(free.count) free, \(reserved.count) reserved by others")

        return free
    }
}

private extension NotificationSeqPicker {
    func unregisteredSlots(
        in origin: PersonOrigin,
        period: UInt32,
        networkSuffix: Data
    ) async throws -> [NotificationSlot] {
        let highestSeq = try await sources.repository.highestSeq(for: origin)

        let slots = (0 ... highestSeq).map { NotificationSlot(personOrigin: origin, period: period, seq: $0) }
        let aliases = try slots.map { slot in
            let context = try ProductContextSuffix
                .notificationSlot(period: period, seq: slot.seq)
                .context(networkSuffix: networkSuffix)

            return try origin.keyManager.deriveAlias(for: context)
        }

        let registered = try await sources.repository.registeredAliases(aliases)

        return zip(slots, aliases).compactMap { slot, alias in
            registered.contains(alias) ? nil : slot
        }
    }
}
