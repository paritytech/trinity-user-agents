@testable import Products

extension PocketCardScreening {
    /// Screening belongs to the core, which tests the rules themselves. A test
    /// in this package only needs to see which field reached which rule.
    static var passingThrough: PocketCardScreening {
        PocketCardScreening(id: { $0 }, title: { $0 })
    }
}
