import struct TrUAPIHost.ProductId
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

    @MainActor
    private final class Counter {
        var value = 0
    }

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

    /// A worker running headless has no screen of its own, so its prompts
    /// present from the fallback until a view is anchored.
    @Test
    func presentsFromTheFallbackUntilAViewIsAnchored() {
        let fallback = StubView()
        let anchor = StubView()
        let fallbackReads = Counter()
        let router = ProductsRouter(fallback: {
            fallbackReads.value += 1
            return fallback
        })

        #expect(router.isReady)
        #expect(router.present(view: StubView()))
        #expect(fallbackReads.value == 2)

        router.setPresentationView(anchor)

        #expect(router.isReady)
        #expect(router.present(view: StubView()))
        #expect(fallbackReads.value == 2)
    }

    @Test
    func notReadyWhenTheFallbackHasNoView() {
        let router = ProductsRouter(fallback: { nil })

        #expect(!router.isReady)
        #expect(!router.present(view: StubView()))
    }

    @Test
    func actionReviewsOfferOnlyConfirmationAndRejection() async throws {
        let request = TrUAPIActionConfirmationRequest.productSubtree(productId: try! ProductId("test.product.dot"))
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
            #expect(body.contains("test.product.dot"))
            #expect(!body.contains(String(localized: .Products.permissionBodyManageInSettingsHint)))

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
