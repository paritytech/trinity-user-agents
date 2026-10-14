import Foundation
import Keystore_iOS
import SubstrateSdk

enum FiatOnrampTrackedTransactionStatus: Codable, Equatable, Hashable {
    case funding(Funding)
    case swapping(Swapping)

    enum Funding: Codable, Equatable, Hashable {
        case inProgress
        case completed
        case failed
    }

    struct Swapping: Codable, Equatable, Hashable {
        enum Status: Codable, Equatable, Hashable {
            case inProgress(remainingTime: TimeInterval)
            case completed
            case failed
        }

        var status: Status
        /// The swapLabel is used as a way to associate the transaction to an unique deposit execution,
        /// with the assumption two possible simultaneous depost executions have different deposit amount.
        /// It would be safer, if there would really an unique identifier for a DepositExecution.
        let swapLabel: DepositExecLabel
        let amountIn: Balance
        let amountOut: Balance
    }
}

struct FiatOnrampTrackedTransaction: Codable, Equatable, Hashable {
    let id: FiatOnRampTransactionId
    var status: FiatOnrampTrackedTransactionStatus
    var lastUpdate: TimeInterval

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}

struct FiatOnrampPendingSession: Codable, Hashable {
    let id: FiatOnRampSessionId
    var createdAt: TimeInterval

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }

    static func == (lhs: FiatOnrampPendingSession, rhs: FiatOnrampPendingSession) -> Bool {
        lhs.id == rhs.id
    }
}

protocol FiatOnrampStoring: Actor {
    func addSessionId(_ id: FiatOnRampSessionId, createdAt: TimeInterval)
    func removeSessionIds(_ ids: Set<FiatOnRampSessionId>)
    func removeExpiredSessionIds(olderThan cutoff: TimeInterval) -> Set<FiatOnRampSessionId>
    func getSessionIds() -> Set<FiatOnRampSessionId>
    func addTrackedTransactions(_ transactions: Set<FiatOnrampTrackedTransaction>)
    func removeTrackedTransactions(_ ids: Set<FiatOnRampTransactionId>)
    func getTrackedTransactions() -> Set<FiatOnrampTrackedTransaction>
}

actor FiatOnrampStorage: FiatOnrampStoring {
    private let settings: SettingsManagerProtocol

    init(settings: SettingsManagerProtocol = SettingsManager.shared) {
        self.settings = settings
    }

    func addSessionId(_ id: FiatOnRampSessionId, createdAt: TimeInterval) {
        var pendingSessions = settings.fiatOnrampPendingSessions
        if let existing = pendingSessions.first(where: { $0.id == id }) {
            pendingSessions.remove(existing)
            pendingSessions.insert(.init(id: id, createdAt: min(existing.createdAt, createdAt)))
        } else {
            pendingSessions.insert(.init(id: id, createdAt: createdAt))
        }
        settings.fiatOnrampPendingSessions = pendingSessions
    }

    func removeSessionIds(_ ids: Set<FiatOnRampSessionId>) {
        guard !ids.isEmpty else {
            return
        }

        let pendingSessions = settings.fiatOnrampPendingSessions
        let remainingSessions = pendingSessions.filter { !ids.contains($0.id) }
        settings.fiatOnrampPendingSessions = Set(remainingSessions)
    }

    func removeExpiredSessionIds(olderThan cutoff: TimeInterval) -> Set<FiatOnRampSessionId> {
        let pendingSessions = settings.fiatOnrampPendingSessions

        let expiredIds = Set(
            pendingSessions.compactMap { pendingSession in
                pendingSession.createdAt <= cutoff ? pendingSession.id : nil
            }
        )
        guard !expiredIds.isEmpty else {
            return []
        }

        let remainingSessions = pendingSessions.filter { !expiredIds.contains($0.id) }
        settings.fiatOnrampPendingSessions = Set(remainingSessions)

        return expiredIds
    }

    func getSessionIds() -> Set<FiatOnRampSessionId> {
        Set(settings.fiatOnrampPendingSessions.map(\.id))
    }

    func addTrackedTransactions(_ transactions: Set<FiatOnrampTrackedTransaction>) {
        var storedTransactions = Array(settings.fiatOnrampTrackedTransactions)

        // Remove existing transactions with matching IDs
        for transaction in transactions {
            storedTransactions.removeAll(where: { $0.id == transaction.id })
        }

        // Add the new/updated transactions
        storedTransactions.append(contentsOf: transactions)
        settings.fiatOnrampTrackedTransactions = Set(storedTransactions)
    }

    func removeTrackedTransactions(_ ids: Set<FiatOnRampTransactionId>) {
        guard !ids.isEmpty else {
            return
        }

        let remainingTransactions = settings.fiatOnrampTrackedTransactions.filter { transaction in
            !ids.contains(transaction.id)
        }

        settings.fiatOnrampTrackedTransactions = Set(remainingTransactions)
    }

    func getTrackedTransactions() -> Set<FiatOnrampTrackedTransaction> {
        settings.fiatOnrampTrackedTransactions
    }
}

// MARK: - SettingsManagerProtocol Extension

extension SettingsManagerProtocol {
    var fiatOnrampPendingSessions: Set<FiatOnrampPendingSession> {
        get {
            guard let data = anyValue(for: SettingsKey.fiatOnrampSessionIds.rawValue) as? Data else {
                return []
            }
            let decoder = JSONDecoder()
            guard let sessions = try? decoder.decode([FiatOnrampPendingSession].self, from: data) else {
                return []
            }
            return Set(sessions)
        }
        set {
            let encoder = JSONEncoder()
            let sessions = Array(newValue)
            guard let data = try? encoder.encode(sessions) else {
                return
            }
            set(anyValue: data, for: SettingsKey.fiatOnrampSessionIds.rawValue)
        }
    }

    var fiatOnrampTrackedTransactions: Set<FiatOnrampTrackedTransaction> {
        get {
            guard let data = anyValue(for: SettingsKey.fiatOnrampTrackedTransactionIds.rawValue) as? Data else {
                return []
            }

            let decoder = JSONDecoder()
            guard let transactions = try? decoder.decode([FiatOnrampTrackedTransaction].self, from: data) else {
                return []
            }

            return Set(transactions)
        }
        set {
            let encoder = JSONEncoder()
            let transactions = Array(newValue)

            guard let data = try? encoder.encode(transactions) else {
                return
            }

            set(anyValue: data, for: SettingsKey.fiatOnrampTrackedTransactionIds.rawValue)
        }
    }
}
