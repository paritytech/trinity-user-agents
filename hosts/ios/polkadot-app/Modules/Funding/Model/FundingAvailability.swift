import Foundation

/// Whether this build can move value in or out of the balance. Funding
/// providers run as product workers, which only builds with products start.
enum FundingAvailability {
    static var isAvailable: Bool {
        #if FEATURE_PRODUCTS
            true
        #else
            false
        #endif
    }
}
