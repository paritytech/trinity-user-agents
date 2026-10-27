import Testing
import TrUAPIHost
@testable import polkadot_app

/// The core lets only a `Widget` execution move a card's face, so a card's
/// page opened as anything else is answered `Denied`.
@MainActor
struct ExecutionPurposeTests {
    @Test
    func opensOnlyAPageUnderACardFaceAsTheWidget() {
        let pages: [ExecutionPurpose] = [.page(cardFace: StubCardFace()), .page(cardFace: nil)]

        #expect(pages.map(\.executionKind) == [.widget, .app])
    }
}
