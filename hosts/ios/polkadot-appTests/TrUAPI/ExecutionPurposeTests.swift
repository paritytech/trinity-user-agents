import Testing
import TrUAPIHost
@testable import polkadot_app

/// The core lets only a `Widget` execution move a card's face, so a card's
/// page opened as anything else is answered `Denied`.
@MainActor
struct ExecutionPurposeTests {
    @Test
    func opensAPageUnderACardFaceAsTheWidget() {
        #expect(ExecutionPurpose.page(cardFace: StubCardFace()).executionKind == .widget)
    }

    @Test
    func opensAnyOtherPageAsTheApp() {
        #expect(ExecutionPurpose.page(cardFace: nil).executionKind == .app)
    }
}
