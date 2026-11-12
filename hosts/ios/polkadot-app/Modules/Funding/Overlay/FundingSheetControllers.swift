import SwiftUI
import UIKit
import UIKit_iOS

/// A hosting controller shown as the funding sheet, presented like the app's
/// other bottom sheets: a rounded card between the safe area insets.
class FundingSheetHostingController<Content: View>: UIHostingController<Content>, ModalSheetPresenterDelegate {
    override init(rootView: Content) {
        super.init(rootView: rootView)

        BottomSheetViewFacade.setupFullHeightSheet(from: self)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceContainer
    }

    /// Dismisses the sheet, or completes at once when it is not on screen.
    func dismissIfPresented(completion: (() -> Void)? = nil) {
        guard presentingViewController != nil, !isBeingDismissed else {
            completion?()
            return
        }
        dismiss(animated: true, completion: completion)
    }

    /// The user dragged the sheet away or tapped outside it.
    func userDismissed() {}

    /// A drag moves the sheet only while its list is scrolled to the top, so
    /// scrolling a list does not pull the sheet down.
    func presenterCanDrag(_: ModalPresenterProtocol) -> Bool {
        view.scrollViewsAtTop
    }

    func presenterDidHide(_: ModalPresenterProtocol) {
        userDismissed()
    }
}

extension UIView {
    /// Whether every scroll view under this one shows its top.
    var scrollViewsAtTop: Bool {
        if let scrollView = self as? UIScrollView,
           scrollView.contentOffset.y > -scrollView.adjustedContentInset.top {
            return false
        }
        return subviews.allSatisfy(\.scrollViewsAtTop)
    }
}

/// An ended session drawn from its history record, on the same sheet as the
/// overlay.
final class FundingEndedSheetController: FundingSheetHostingController<FundingEndedSheetView> {
    var onStartOver: (() -> Void)?

    convenience init(ended: FundingEndedSession, cash: FundingCash, branding: FundingProviderBranding) {
        self.init(rootView: FundingEndedSheetView(ended: ended, cash: cash, branding: branding))

        rootView.onClose = { [weak self] in self?.dismiss(animated: true) }
        rootView.onStartOver = { [weak self] in self?.onStartOver?() }
    }
}

struct FundingEndedSheetView: View {
    let ended: FundingEndedSession
    let cash: FundingCash
    let branding: FundingProviderBranding
    var onClose: () -> Void = {}
    var onStartOver: () -> Void = {}

    var body: some View {
        FundingEndedView(
            ended: ended,
            cash: cash,
            branding: branding,
            onClose: onClose,
            onStartOver: onStartOver
        )
        .fundingScreen()
    }
}
