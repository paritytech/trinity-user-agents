import Foundation

@testable import polkadot_app

final class MockProductContentPrewarmer: ProductContentPrewarming {
    @MainActor
    func prewarm() {}
}
