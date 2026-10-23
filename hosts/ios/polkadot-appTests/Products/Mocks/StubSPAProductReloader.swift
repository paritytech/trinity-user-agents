import Foundation
@testable import polkadot_app

@MainActor
final class StubSPAProductReloader: SPAProductReloading {
    private(set) var serveCount = 0

    func serveAppliedBuild() {
        serveCount += 1
    }
}
