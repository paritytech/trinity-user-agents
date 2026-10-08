import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The core lets only a `Widget` execution move a card's face, so a card's
/// page opened as anything else is answered `Denied`.
struct ExecutableKindExecutionTests {
    @Test(arguments: [
        (ExecutableKind.app, ProductExecutionKind.app),
        (.widget, .widget),
        (.worker, .worker)
    ])
    func opensTheExecutionTheExecutableNames(executable: ExecutableKind, expected: ProductExecutionKind) {
        #expect(executable.executionKind == expected)
    }
}
