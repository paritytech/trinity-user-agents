import Testing
import UIKit
import UIKitExt
import Products
@testable import struct PolkadotUI.MessageSheetAction
@testable import struct PolkadotUI.TitleDetailsSheetViewModel
@testable import polkadot_app

@MainActor
struct ProductsRouterTests {
    private final class StubView: UIViewController, ControllerBackedProtocol {}

    @Test
    func notReadyUntilPresentationViewAttached() {
        let anchor = StubView()
        let router = ProductsRouter()

        #expect(!router.isReady)

        router.setPresentationView(anchor)

        #expect(router.isReady)
    }

    @Test
    func presentReturnsFalseWhenNoPresentationView() {
        let router = ProductsRouter()

        #expect(!router.present(view: StubView()))
    }

    @Test
    func presentReturnsTrueWhenAnchored() {
        let anchor = StubView()
        let router = ProductsRouter()
        router.setPresentationView(anchor)

        #expect(router.present(view: StubView()))
    }

    @Test
    func actionReviewsOfferOnlyConfirmationAndRejection() async throws {
        for request in [
            TrUAPIActionConfirmationRequest.preimageSubmit(productId: "test.product", size: 1_024),
            .productSubtree(productId: "test.product")
        ] {
            for approved in [true, false] {
                let context = TrUAPIActionConfirmationContext(request: request)
                let model = TrUAPIActionPromptViewFactory.makeViewModel(for: context)
                let confirm = try #require(model.mainAction)
                let reject = try #require(model.secondaryAction)
                #expect([
                    confirm.title.value(for: .current),
                    reject.title.value(for: .current),
                    model.tertiaryAction?.title.value(for: .current)
                ] == [String(localized: .Common.confirm), String(localized: .Common.reject), nil])
                let body =
                    switch model.message.value(for: .current) {
                    case let .normal(text): text
                    case let .attributed(text): text.string
                    }
                #expect(body.contains("test.product"))
                #expect(!body.contains(String(localized: .Products.permissionBodyManageInSettingsHint)))
                if case let .preimageSubmit(_, size) = request {
                    #expect(body.contains(size.formatted()))
                }

                let decision = await withCheckedContinuation { continuation in
                    context.setContinuation(continuation)
                    if approved {
                        confirm.handler()
                        reject.handler()
                    } else {
                        reject.handler()
                        confirm.handler()
                    }
                }
                #expect(decision == approved)
            }
        }
    }
}
