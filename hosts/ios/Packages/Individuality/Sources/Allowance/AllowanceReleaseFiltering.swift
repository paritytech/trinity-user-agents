import Foundation
import SubstrateSdk

public protocol AllowanceReleaseFiltering {
    /// The records whose allowances are no longer needed; the manager deletes them so they are never renewed.
    func recordsToRelease(from records: [AllowanceRecord]) async throws -> [AllowanceRecord]
}

public enum AllowanceReleaseStrategy {
    /// Deletes these accounts' records directly, without reading the others.
    case accountIds(Set<AccountId>)
    /// Reads every record and deletes the ones the filter selects.
    case filter(AllowanceReleaseFiltering)
}
