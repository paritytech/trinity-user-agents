import Foundation
@testable import polkadot_app

@MainActor
final class StubSPAProductReloader: SPAProductReloading {
    private(set) var serveCount = 0
    var servedContentHash: String?

    func serveAppliedBuild() {
        serveCount += 1
    }
}
