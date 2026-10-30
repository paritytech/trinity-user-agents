import SwiftUI
import UIKit

/// The overlay sheet. A swipe down is the user leaving, the same as Close.
final class FundingSheetController: UIHostingController<FundingSheetView>, UIAdaptivePresentationControllerDelegate {
    private let model: FundingFlowModel
    private var onAppear: (() -> Void)?

    init(model: FundingFlowModel, onAppear: (() -> Void)? = nil) {
        self.model = model
        self.onAppear = onAppear
        super.init(rootView: FundingSheetView(model: model))

        presentAsFundingSheet()
        presentationController?.delegate = self
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .clear
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        let onAppear = onAppear
        self.onAppear = nil
        onAppear?()
    }

    /// Dismisses from the presenter, so a provider's screen still over the
    /// sheet goes with it.
    func dismissIfPresented(completion: (() -> Void)? = nil) {
        guard let presenter = presentingViewController, !isBeingDismissed else {
            completion?()
            return
        }
        presenter.dismiss(animated: true, completion: completion)
    }

    func presentationControllerDidDismiss(_: UIPresentationController) {
        model.close()
    }
}

/// A list of sessions, on the same sheet as the overlay.
final class FundingListSheetController: UIHostingController<AnyView> {
    init() {
        super.init(rootView: AnyView(EmptyView()))
        presentAsFundingSheet()
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .clear
    }

    func show(_ content: some View) {
        rootView = AnyView(content.fundingScreen())
    }

    func dismissIfPresented(completion: @escaping () -> Void) {
        guard presentingViewController != nil, !isBeingDismissed else {
            completion()
            return
        }
        dismiss(animated: true, completion: completion)
    }
}

/// An ended session drawn from its history record, on the same sheet as the
/// overlay.
final class FundingEndedSheetController: UIHostingController<FundingEndedSheetView> {
    var onStartOver: (() -> Void)?

    init(ended: FundingEndedSession, cash: FundingCash, branding: FundingProviderBranding) {
        super.init(rootView: FundingEndedSheetView(ended: ended, cash: cash, branding: branding))

        rootView.onClose = { [weak self] in self?.dismiss(animated: true) }
        rootView.onStartOver = { [weak self] in self?.onStartOver?() }
        presentAsFundingSheet()
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .clear
    }

    func dismissIfPresented(completion: @escaping () -> Void) {
        guard presentingViewController != nil, !isBeingDismissed else {
            completion()
            return
        }
        dismiss(animated: true, completion: completion)
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

private extension UIViewController {
    func presentAsFundingSheet() {
        modalPresentationStyle = .pageSheet
        sheetPresentationController?.detents = [.custom { $0.maximumDetentValue * 0.92 }]
        sheetPresentationController?.prefersGrabberVisible = true
        sheetPresentationController?.preferredCornerRadius = 32
    }
}
