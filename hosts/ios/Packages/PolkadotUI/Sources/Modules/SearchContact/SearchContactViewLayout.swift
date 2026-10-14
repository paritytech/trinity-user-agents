import DesignSystem
import UIKit
internal import SnapKit

/// Full-screen search: the field rides the keyboard's top edge and results fill the space above it.
/// The scan panel composes its own row instead, so this layout serves the modal screen only.
public final class SearchContactViewLayout: UIView {
    private let searchHeader = SearchContactHeaderView()
    private let resultsView = SearchContactResultsView()

    public var searchHandler: ((String?) -> Void)? {
        get { searchHeader.searchHandler }
        set { searchHeader.searchHandler = newValue }
    }

    public var cancelHandler: (() -> Void)? {
        get { searchHeader.cancelHandler }
        set { searchHeader.cancelHandler = newValue }
    }

    public var selectionHandler: ((String) -> Void)? {
        get { resultsView.selectionHandler }
        set { resultsView.selectionHandler = newValue }
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func focusSearchInput() {
        searchHeader.searchField.becomeFirstResponder()
    }

    public func bind(viewModel: SearchContactResultsView.ViewModel) {
        resultsView.bind(viewModel: viewModel)
    }

    public func bind(status: SearchContactResultsView.StatusViewModel) {
        resultsView.bind(status: status)
    }
}

private extension SearchContactViewLayout {
    func setupLayout() {
        backgroundColor = .bgSurfaceMain

        addSubview(resultsView)
        addSubview(searchHeader)

        searchHeader.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.bottom.equalTo(keyboardLayoutGuide.snp.top)
        }

        resultsView.snp.makeConstraints { make in
            make.top.equalTo(safeAreaLayoutGuide.snp.top)
            make.leading.trailing.equalToSuperview()
            make.bottom.equalTo(searchHeader.snp.top)
        }
    }
}
