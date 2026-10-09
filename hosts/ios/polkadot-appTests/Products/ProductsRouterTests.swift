import Testing
import UIKit
import UIKitExt
import Products
import TrUAPIHost
import SubstrateSdk
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
            TrUAPIActionConfirmationRequest.productSubtree(productId: "test.product")
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

    @Test
    func uploadConsentPreservesLifetimeAndIgnoresLateDecisions() async throws {
        let review = PreimageSubmitReview(
            size: 1_024,
            productId: "upload.product",
            rootPublicKey: Data(repeating: 0x12, count: 32),
            genesisHash: Data(repeating: 0x34, count: 32),
            automaticMaxBytes: 262_144,
            automaticMaxUploads: 4,
            automaticWindowSeconds: 3_600
        )
        for expected in [TrUAPIPermissionDecision.allowOnce, .allowAlways, .deny] {
            let context = TrUAPIPreimageConfirmationContext(review: review)
            let model = TrUAPIActionPromptViewFactory.makePreimageViewModel(for: context)
            let once = try #require(model.mainAction)
            let automatic = try #require(model.secondaryAction)
            let deny = try #require(model.tertiaryAction)
            let body = switch model.message.value(for: .current) {
            case let .normal(text): text
            case let .attributed(text): text.string
            }
            #expect(body.contains(review.productId))
            #expect(body.contains(review.size.formatted()))
            #expect(body.contains(review.rootPublicKey.toHex(includePrefix: true)))
            #expect(body.contains(review.genesisHash.toHex(includePrefix: true)))
            #expect(body.contains(review.automaticMaxBytes.formatted()))
            #expect(body.contains(review.automaticWindowSeconds.formatted()))
            let decision = await withCheckedContinuation { continuation in
                context.setContinuation(continuation)
                switch expected {
                case .allowOnce: once.handler()
                case .allowAlways: automatic.handler()
                case .deny: deny.handler()
                }
                // Dismissal or a second tap may never upgrade the first decision.
                automatic.handler()
                deny.handler()
            }
            #expect(decision == expected)
        }
    }

    @Test
    func dismissedUploadConsentDenies() async {
        let decision = await withCheckedContinuation { continuation in
            let context = TrUAPIPreimageConfirmationContext(review: PreimageSubmitReview(
                size: 1,
                productId: "upload.product",
                rootPublicKey: Data(repeating: 1, count: 32),
                genesisHash: Data(repeating: 2, count: 32),
                automaticMaxBytes: 262_144,
                automaticMaxUploads: 4,
                automaticWindowSeconds: 3_600
            ))
            context.setContinuation(continuation)
        }
        #expect(decision == .deny)
    }
}
