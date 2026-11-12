import Testing
import TrUAPIHost
@testable import polkadot_app

@MainActor
struct ExecutionPurposeTests {
    @Test
    func opensACardsPageAsTheWidgetAndAnAppAsTheApp() {
        let purposes: [ExecutionPurpose] = [.expandedCard(face: StubCardFace()), .app]

        #expect(purposes.map(\.executionKind) == [.widget, .app])
    }
}
