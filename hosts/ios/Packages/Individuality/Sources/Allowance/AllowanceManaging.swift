import Foundation
import SubstrateSdk

public enum OnExistingAllowancePolicy: Equatable, ScaleCodable {
    /// Return existing keys without extra slot assignment (first-time or re-syncing Host).
    case ignore
    /// Assign one additional slot to the same allowance account (scale-up path).
    case increase

    public init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0: self = .ignore
        case 1: self = .increase
        default: throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case .ignore: try UInt8(0).encode(scaleEncoder: scaleEncoder)
        case .increase: try UInt8(1).encode(scaleEncoder: scaleEncoder)
        }
    }
}

public protocol AllowanceManaging<Holder> {
    associatedtype Holder

    func allocate(
        holder: Holder,
        policy: OnExistingAllowancePolicy,
        priority: AllowanceRecord.Priority,
        domain: AllowanceRecord.Domain
    ) async throws
    /// Defaults to `allocate(policy: .ignore)`; Bulletin gates on `can_account_promote` instead of the stored extent.
    func ensureCanSubmit(
        holder: Holder,
        priority: AllowanceRecord.Priority,
        domain: AllowanceRecord.Domain
    ) async throws
    func renew() async throws
    func release(with strategy: AllowanceReleaseStrategy) async throws
}

public extension AllowanceManaging {
    func ensureCanSubmit(
        holder: Holder,
        priority: AllowanceRecord.Priority,
        domain: AllowanceRecord.Domain
    ) async throws {
        try await allocate(holder: holder, policy: .ignore, priority: priority, domain: domain)
    }

    func renew() async throws {}
    func release(with _: AllowanceReleaseStrategy) async throws {}
}

public extension AllowanceManaging where Holder == AccountId {
    func allocate(
        accountId: AccountId,
        policy: OnExistingAllowancePolicy,
        priority: AllowanceRecord.Priority,
        domain: AllowanceRecord.Domain
    ) async throws {
        try await allocate(holder: accountId, policy: policy, priority: priority, domain: domain)
    }

    func ensureCanSubmit(
        accountId: AccountId,
        priority: AllowanceRecord.Priority,
        domain: AllowanceRecord.Domain
    ) async throws {
        try await ensureCanSubmit(holder: accountId, priority: priority, domain: domain)
    }
}
