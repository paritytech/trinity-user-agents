import PolkadotUI
import UIKit
import UIKitExt

/// The participants of a product room, one page after the other (P6.3, scenario S30).
final class ChatParticipantsViewController: UIViewController {
    let presenter: ChatParticipantsPresenterProtocol

    private let tableView = UITableView(frame: .zero, style: .plain)
    private let spinner = UIActivityIndicatorView(style: .medium)
    private let failureLabel = UILabel()
    private var viewModel = ChatParticipantsViewModel(rows: [], isLoading: false, failed: false)

    init(presenter: ChatParticipantsPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        title = String(localized: .ChatExtension.participantsTitle)
        view.backgroundColor = .bgSurfaceMain
        setupTable()
        presenter.setup()
    }
}

extension ChatParticipantsViewController: ChatParticipantsViewProtocol {
    func didReceive(_ viewModel: ChatParticipantsViewModel) {
        self.viewModel = viewModel
        tableView.reloadData()

        if viewModel.isLoading {
            spinner.startAnimating()
        } else {
            spinner.stopAnimating()
        }
        failureLabel.isHidden = !viewModel.failed
    }
}

extension ChatParticipantsViewController: UITableViewDataSource, UITableViewDelegate {
    func tableView(_: UITableView, numberOfRowsInSection _: Int) -> Int {
        viewModel.rows.count
    }

    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = tableView.dequeueReusableCell(withIdentifier: ChatParticipantRow.cellIdentifier, for: indexPath)
        (cell as? ChatParticipantRow)?.bind(viewModel.rows[indexPath.row])
        return cell
    }

    func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath, animated: true)
        presenter.selectParticipant(at: indexPath.row)
    }

    func tableView(_: UITableView, willDisplay _: UITableViewCell, forRowAt indexPath: IndexPath) {
        let isNearEnd = indexPath.row >= viewModel.rows.count - 10
        if isNearEnd, !viewModel.isLoading, !viewModel.failed {
            presenter.loadMore()
        }
    }
}

private extension ChatParticipantsViewController {
    func setupTable() {
        tableView.register(ChatParticipantRow.self, forCellReuseIdentifier: ChatParticipantRow.cellIdentifier)
        tableView.dataSource = self
        tableView.delegate = self
        tableView.backgroundColor = .clear
        tableView.tableFooterView = spinner
        spinner.frame = CGRect(x: 0, y: 0, width: 44, height: 44)

        failureLabel.text = String(localized: .ChatExtension.participantsFailed)
        failureLabel.textColor = .fgSecondary
        failureLabel.textAlignment = .center
        failureLabel.numberOfLines = 0
        failureLabel.isHidden = true

        view.addSubview(tableView)
        view.addSubview(failureLabel)
        tableView.translatesAutoresizingMaskIntoConstraints = false
        failureLabel.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            tableView.topAnchor.constraint(equalTo: view.topAnchor),
            tableView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            tableView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            tableView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            failureLabel.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            failureLabel.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 32),
            failureLabel.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -32)
        ])
    }
}
