import PolkadotUI
import UIKit
import UIKitExt

/// The in-room search: a field above the results, newest first, each with its author, its time and the line of
/// the match (07-LATER §8.2). A tap on a result shows the message in the room.
final class ChatSearchViewController: UIViewController {
    let presenter: ChatSearchPresenterProtocol

    private let searchController = UISearchController(searchResultsController: nil)
    private let tableView = UITableView(frame: .zero, style: .plain)
    private let statusLabel = UILabel()
    private let spinner = UIActivityIndicatorView(style: .medium)
    private var rows: [ChatSearchRowViewModel] = []
    private var hasMore = false

    private static let cellIdentifier = "ChatSearchResult"
    /// How close to the end of the results the screen asks for the next page.
    private static let pagingThreshold = 5

    init(presenter: ChatSearchPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .bgSurfaceMain
        setupSearchField()
        setupTable()
        presenter.setup()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        DispatchQueue.main.async { [searchController] in
            searchController.searchBar.becomeFirstResponder()
        }
    }

    /// Puts a text in the field, as the user types it.
    func setSearchText(_ text: String) {
        searchController.searchBar.text = text
    }
}

extension ChatSearchViewController: ChatSearchViewProtocol {
    func didReceive(state: ChatSearchViewState) {
        switch state {
        case .idle:
            show(rows: [], hasMore: false, status: nil, isSearching: false)
        case .searching:
            show(rows: rows, hasMore: false, status: nil, isSearching: true)
        case let .results(newRows, hasMore):
            show(rows: newRows, hasMore: hasMore, status: nil, isSearching: false)
        case .empty:
            show(
                rows: [],
                hasMore: false,
                status: String(localized: .ChatExtension.roomSearchEmpty),
                isSearching: false
            )
        case .unavailable:
            show(
                rows: [],
                hasMore: false,
                status: String(localized: .ChatExtension.roomSearchUnavailable),
                isSearching: false
            )
        }
    }
}

extension ChatSearchViewController: UISearchResultsUpdating {
    func updateSearchResults(for searchController: UISearchController) {
        presenter.search(text: searchController.searchBar.text ?? "")
    }
}

extension ChatSearchViewController: UITableViewDataSource, UITableViewDelegate {
    func tableView(_: UITableView, numberOfRowsInSection _: Int) -> Int {
        rows.count
    }

    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = tableView.dequeueReusableCell(withIdentifier: Self.cellIdentifier, for: indexPath)
        cell.contentConfiguration = configuration(for: rows[indexPath.row])
        cell.backgroundColor = .clear
        return cell
    }

    func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath, animated: true)
        presenter.select(messageId: rows[indexPath.row].messageId)
    }

    func tableView(_: UITableView, willDisplay _: UITableViewCell, forRowAt indexPath: IndexPath) {
        if hasMore, indexPath.row >= rows.count - Self.pagingThreshold {
            presenter.loadMore()
        }
    }
}

private extension ChatSearchViewController {
    func setupSearchField() {
        searchController.searchResultsUpdater = self
        searchController.obscuresBackgroundDuringPresentation = false
        searchController.hidesNavigationBarDuringPresentation = false
        searchController.searchBar.placeholder = String(localized: .ChatExtension.roomSearchPlaceholder)
        navigationItem.searchController = searchController
        // Under the title, not in the bottom bar: the tab bar of the app covers the bottom.
        navigationItem.preferredSearchBarPlacement = .stacked
        navigationItem.hidesSearchBarWhenScrolling = false
        definesPresentationContext = true
    }

    func setupTable() {
        tableView.backgroundColor = .clear
        tableView.dataSource = self
        tableView.delegate = self
        tableView.keyboardDismissMode = .onDrag
        tableView.register(UITableViewCell.self, forCellReuseIdentifier: Self.cellIdentifier)
        view.addSubview(tableView)
        tableView.translatesAutoresizingMaskIntoConstraints = false

        statusLabel.textColor = .fgSecondary
        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 0
        statusLabel.isHidden = true

        for subview in [statusLabel, spinner] {
            view.addSubview(subview)
            subview.translatesAutoresizingMaskIntoConstraints = false
        }

        NSLayoutConstraint.activate([
            tableView.topAnchor.constraint(equalTo: view.topAnchor),
            tableView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            tableView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            tableView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            statusLabel.centerYAnchor.constraint(equalTo: view.safeAreaLayoutGuide.centerYAnchor),
            statusLabel.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 24),
            statusLabel.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -24),
            spinner.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            spinner.centerYAnchor.constraint(equalTo: view.safeAreaLayoutGuide.centerYAnchor)
        ])
    }

    func show(rows newRows: [ChatSearchRowViewModel], hasMore: Bool, status: String?, isSearching: Bool) {
        let changed = newRows != rows
        rows = newRows
        self.hasMore = hasMore
        if changed {
            tableView.reloadData()
        }

        statusLabel.text = status
        statusLabel.isHidden = status == nil
        if isSearching {
            spinner.startAnimating()
        } else {
            spinner.stopAnimating()
        }
    }

    func configuration(for row: ChatSearchRowViewModel) -> UIListContentConfiguration {
        var configuration = UIListContentConfiguration.subtitleCell()
        configuration.text = "\(row.author) · \(row.time)"
        configuration.textProperties.color = .fgSecondary
        configuration.textProperties.font = .preferredFont(forTextStyle: .footnote)

        let line = NSMutableAttributedString(
            string: row.line,
            attributes: [.foregroundColor: UIColor.fgPrimary, .font: UIFont.preferredFont(forTextStyle: .body)]
        )
        if let match = row.match {
            line.addAttributes(
                [.backgroundColor: UIColor.systemYellow.withAlphaComponent(0.35), .foregroundColor: UIColor.fgPrimary],
                range: match
            )
        }
        configuration.secondaryAttributedText = line
        configuration.secondaryTextProperties.numberOfLines = 2
        return configuration
    }
}
