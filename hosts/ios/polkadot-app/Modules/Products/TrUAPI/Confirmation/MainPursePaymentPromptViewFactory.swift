import Foundation
import Foundation_iOS
import FoundationExt
import PolkadotUI
import TrUAPIHost
import UIKit
import UIKitExt

/// A single exact payment, never a permission or an AutoSigning grant.
@MainActor
final class MainPursePaymentConfirmationContext {
    let review: MainPurseChatPaymentReview
    let requiresPrivacyConfirmation: Bool
    private var continuation: CheckedContinuation<Bool, Never>?

    init(review: MainPurseChatPaymentReview, requiresPrivacyConfirmation: Bool = false) {
        self.review = review
        self.requiresPrivacyConfirmation = requiresPrivacyConfirmation
    }

    deinit {
        continuation?.resume(returning: false)
    }

    func setContinuation(_ continuation: CheckedContinuation<Bool, Never>) {
        self.continuation = continuation
    }

    func deliver(_ approved: Bool) {
        continuation?.resume(returning: approved)
        continuation = nil
    }
}

@MainActor
enum MainPursePaymentPromptViewFactory {
    static func createView(context: MainPursePaymentConfirmationContext) -> ControllerBackedProtocol {
        let review = context.review
        let privacyWarning = context.requiresPrivacyConfirmation
            ? "\n\nPrivacy warning: this payment spends funds that are still gaining privacy. " +
            "Sending now reduces their privacy. Confirm only if you want to send anyway."
            : ""
        // All identity fields come from the Host review. `.normal` is literal
        // text, not HTML/Markdown; product content cannot supply the prompt.
        let details = """
        Product: \(review.callingProductId)

        Recipient name (Host-resolved): \(review.recipientUsername ?? "Not available")
        Recipient identity: 0x\(review.recipientIdentity.toHex())

        Recipient receives: \(amount(review.amountCents)) dotUSD
        Maximum main-purse debit: \(amount(review.maxDebitCents)) dotUSD

        Host-selected network (genesis): 0x\(review.genesisHash.toHex())
        Coinage asset: \(review.coinageInstanceId.map { "instance \($0)" } ?? "legacy single asset")
        Payment operation: 0x\(review.operationId.toHex())

        Approve only this payment from your main purse. This does not grant permission for future payments.\(privacyWarning)
        """
        let viewModel = TitleDetailsSheetViewModel(
            graphics: UIImage(systemName: "creditcard"),
            title: LocalizableResource { _ in "Confirm main-purse payment" },
            message: LocalizableResource { _ in .normal(details) },
            mainAction: MessageSheetAction(
                title: LocalizableResource { _ in "Confirm payment" },
                handler: { context.deliver(true) }
            ),
            secondaryAction: MessageSheetAction(
                title: LocalizableResource { _ in String(localized: .Common.reject) },
                handler: { context.deliver(false) }
            )
        )
        let view = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: ProductPromptStyler(),
            allowsSwipeDown: false
        )
        return MainPursePaymentScrollController(content: view.controller)
    }

    /// Integer-only formatting preserves every cent, including values beyond
    /// Double's exact integer range. There is no whole-coin rounding.
    private static func amount(_ cents: UInt64) -> String {
        let fraction = cents % 100
        return "\(cents / 100).\(fraction < 10 ? "0" : "")\(fraction)"
    }
}

/// The normal title/details sheet does not scroll. Payment identities and
/// genesis must remain fully readable on small screens and at large text sizes.
@MainActor
private final class MainPursePaymentScrollController: UIViewController, ControllerBackedProtocol {
    private let content: UIViewController

    init(content: UIViewController) {
        self.content = content
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .pageSheet
        isModalInPresentation = true
        sheetPresentationController?.detents = [.large()]
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        let scroll = UIScrollView()
        scroll.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scroll)
        addChild(content)
        content.view.translatesAutoresizingMaskIntoConstraints = false
        scroll.addSubview(content.view)
        NSLayoutConstraint.activate([
            scroll.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            content.view.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor),
            content.view.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor),
            content.view.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor),
            content.view.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor),
            content.view.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor)
        ])
        content.didMove(toParent: self)
    }
}
