import Foundation
import Operation_iOS

/// Which Core Data stack a benchmark runs against. Every timing test is parameterized over
/// `supported`, so old and new implementations are measured in the same process and run.
enum StackVariant: String, CaseIterable, Sendable {
    /// One private-queue context serves reads, writes and observation (2.7.0 behaviour).
    case serial
    case concurrent2
    case concurrent4

    static let supported: [StackVariant] = StackVariant.allCases

    var concurrencyMode: CoreDataConcurrencyMode {
        switch self {
        case .serial: .serial
        case .concurrent2: .concurrent(readerConcurrency: 2)
        case .concurrent4: .concurrent(readerConcurrency: 4)
        }
    }
}
