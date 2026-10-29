import DesignSystem
import UIKit
internal import SnapKit

public final class ScanPanelViewLayout: UIView {
    /// Margin DSSearchRowView leaves above and below its capsule. Mirrors that view, so the gaps below are
    /// measured to the capsule rather than to the row.
    private static let searchRowFieldMargin: CGFloat = 8
    private static let gapBelowContent = DSTabBarStyle.scanFieldTopGap - searchRowFieldMargin
    /// The 48pt capsule of the search row plus its margin on each edge.
    private static let searchRowHeight = DSTabBarStyle.scanFieldHeight + 2 * searchRowFieldMargin
    private static let searchRowBottomInset = DSTabBarStyle.scanFieldBottomInset - searchRowFieldMargin

    public let searchRow = DSSearchRowView()
    public let resultsView = SearchContactResultsView()

    /// Stands in for the camera while `AVCaptureSession` configures and starts, which takes
    /// roughly a second. It sits behind the preview, which fades in over it.
    private let placeholderView: UIView = {
        let view = UIView()
        view.backgroundColor = .bgSurfaceContainer
        view.layer.cornerRadius = DSRadii.large
        view.layer.masksToBounds = true
        return view
    }()

    private let scanButton: UIButton = {
        let button = UIButton()
        let material = DSTabBarFieldMaterialView(cornerRadius: 24)
        let icon = UIImageView(image: UIImage(resource: .scan18).withRenderingMode(.alwaysTemplate))
        icon.tintColor = .fgPrimary
        icon.contentMode = .scaleAspectFit
        icon.isUserInteractionEnabled = false
        button.addSubview(material)
        button.addSubview(icon)
        button.snp.makeConstraints { $0.size.equalTo(48) }
        material.snp.makeConstraints { $0.edges.equalToSuperview() }
        icon.snp.makeConstraints {
            $0.size.equalTo(24)
            $0.center.equalToSuperview()
        }
        button.accessibilityLabel = String(localized: .searchContactScanButton)
        return button
    }()

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
        stack.spacing = 10
        stack.alignment = .center
        return stack
    }()

    private var expandedCameraWidthConstraint: Constraint?
    private var collapsedCameraWidthConstraint: Constraint?
    private var resultsCollapsedConstraint: Constraint?

    public var onCameraTapped: (() -> Void)?

    override public init(frame: CGRect) {
        super.init(frame: frame)

        resultsView.clipsToBounds = true
        contentStack.addArrangedSubview(resultsView)
        addSubview(contentStack)

        searchRowStack.addArrangedSubview(scanButton)
        searchRowStack.addArrangedSubview(searchRow)
        addSubview(searchRowStack)

        searchRowStack.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(DSSpacings.mediumIncreased)
            make.bottom.equalToSuperview().inset(Self.searchRowBottomInset)
        }

        searchRow.snp.makeConstraints { make in
            make.height.equalTo(Self.searchRowHeight)
        }

        scanButton.addAction(UIAction { [weak self] _ in self?.onCameraTapped?() }, for: .touchUpInside)

        contentStack.snp.makeConstraints { make in
            make.top.equalToSuperview()
            make.leading.trailing.equalToSuperview()
            make.bottom.equalTo(searchRowStack.snp.top).offset(-Self.gapBelowContent)
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

    /// Height the panel declares with search unfocused. The tab bar reads it before the panel lays out.
    public static var declaredHeight: CGFloat { DSTabBarStyle.scanStateHeight }

    public func setupScannerView(_ scannerView: UIView) {
        contentStack.addArrangedSubview(scannerView)
        insertSubview(placeholderView, at: 0)

        placeholderView.snp.makeConstraints { make in
            make.edges.equalTo(scannerView)
        }

        scannerView.snp.makeConstraints { make in
            expandedCameraWidthConstraint = make.width.equalTo(DSTabBarStyle.scanCameraSide).constraint
            collapsedCameraWidthConstraint = make.width.equalTo(0).constraint
        }

        setSearchFocused(false)
    }

    /// When search is focused, the camera collapses away entirely and a scan button appears
    /// to the left of the search field. Unfocused, the camera expands to its fixed square and the
    /// button hides. The collapse is a constraint, not the stack's animated hide, so the layout
    /// reaches its final height at once and `declaredHeight` matches it.
    public func setSearchFocused(_ focused: Bool) {
        if focused {
            resultsCollapsedConstraint?.deactivate()
        } else {
            resultsCollapsedConstraint?.activate()
        }
        searchRow.setSearchFocused(focused)
        resultsView.alpha = 0
        if focused {
            UIView.animate(
                withDuration: DSTabBarStyle.contentRevealDuration,
                delay: DSTabBarStyle.contentRevealDelay,
                options: .curveEaseOut
            ) {
                self.resultsView.alpha = 1
            }
        }
        contentStack.directionalLayoutMargins.top = focused
            ? DSSpacings.tiny
            : DSTabBarStyle.scanCameraTopInset

        if focused {
            expandedCameraWidthConstraint?.deactivate()
            collapsedCameraWidthConstraint?.activate()
        } else {
            collapsedCameraWidthConstraint?.deactivate()
            expandedCameraWidthConstraint?.activate()
        }

        scanButton.isHidden = !focused
        scanButton.alpha = focused ? 1 : 0
    }
}
