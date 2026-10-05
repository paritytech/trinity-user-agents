import DesignSystem
import UIKit
internal import SnapKit

public final class ScanPanelViewLayout: UIView {
    public let grabber = DSPanelGrabberView()
    public let searchRow = DSSearchRowView()
    public let resultsView = SearchContactResultsView()

    /// Stands in for the camera while `AVCaptureSession` configures and starts, which takes
    /// roughly a second. It sits behind the preview, which fades in over it.
    private let placeholderView: UIView = {
        let view = UIView()
        view.backgroundColor = .bgSurfaceNested
        view.layer.cornerRadius = DSRadii.large
        view.layer.masksToBounds = true
        return view
    }()

    private let scanButton = DSIconButton(
        style: .secondary,
        shape: .pill,
        size: .mediumIncreased,
        icon: UIImage(resource: .scan18),
        glass: true
    )

    private let contentStack: UIStackView = {
        let stack = UIStackView()
        stack.axis = .vertical
        stack.alignment = .center
        stack.isLayoutMarginsRelativeArrangement = true
        return stack
    }()

    private let searchRowStack: UIStackView = {
        let stack = UIStackView()
        stack.axis = .horizontal
        stack.spacing = DSSpacings.small
        stack.alignment = .center
        return stack
    }()

    private var fullCameraWidthConstraint: Constraint?
    private var collapsedCameraWidthConstraint: Constraint?
    private var resultsCollapsedConstraint: Constraint?

    public var onCameraTapped: (() -> Void)?

    override public init(frame: CGRect) {
        super.init(frame: frame)

        resultsView.clipsToBounds = true
        contentStack.addArrangedSubview(resultsView)
        addSubview(contentStack)
        addSubview(grabber)

        searchRowStack.addArrangedSubview(scanButton)
        searchRowStack.addArrangedSubview(searchRow)
        addSubview(searchRowStack)

        searchRowStack.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(DSSpacings.mediumIncreased)
            make.bottom.equalToSuperview().inset(DSSpacings.small)
        }

        scanButton.onTap = { [weak self] in self?.onCameraTapped?() }

        contentStack.snp.makeConstraints { make in
            make.top.equalTo(grabber.snp.bottom)
            make.leading.trailing.equalToSuperview()
            make.bottom.equalTo(searchRowStack.snp.top).offset(-DSSpacings.small)
        }

        grabber.snp.makeConstraints { make in
            make.top.leading.trailing.equalToSuperview()
        }

        resultsView.snp.makeConstraints { make in
            make.width.equalTo(contentStack).offset(-DSSpacings.small * 2)
            resultsCollapsedConstraint = make.height.equalTo(0).constraint
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func setupScannerView(_ scannerView: UIView) {
        contentStack.addArrangedSubview(scannerView)
        insertSubview(placeholderView, at: 0)

        placeholderView.snp.makeConstraints { make in
            make.edges.equalTo(scannerView)
        }

        scannerView.snp.makeConstraints { make in
            fullCameraWidthConstraint = make.width.equalTo(contentStack)
                .offset(-DSSpacings.mediumIncreased * 2).constraint
            collapsedCameraWidthConstraint = make.width.equalTo(0).constraint
        }

        setSearchFocused(false)
    }

    /// When search is focused, the camera collapses away entirely and a scan button appears
    /// to the left of the search field. Unfocused, the camera expands to full width and the
    /// button hides. The collapse is a constraint, not the stack's animated hide, because the
    /// panel measures its height mid-animation and the hide still reports the rows' height then.
    public func setSearchFocused(_ focused: Bool) {
        if focused {
            resultsCollapsedConstraint?.deactivate()
        } else {
            resultsCollapsedConstraint?.activate()
        }
        resultsView.alpha = focused ? 1 : 0
        contentStack.directionalLayoutMargins.top = focused ? DSSpacings.tiny : DSSpacings.mediumIncreased

        if focused {
            fullCameraWidthConstraint?.deactivate()
            collapsedCameraWidthConstraint?.activate()
        } else {
            collapsedCameraWidthConstraint?.deactivate()
            fullCameraWidthConstraint?.activate()
        }

        scanButton.isHidden = !focused
        scanButton.alpha = focused ? 1 : 0
    }
}
