import Foundation

/// Pages through the participants of a product room with `participants_query { Page }` (P6.3, scenario S30).
@MainActor
final class ChatParticipantsInteractor {
    weak var presenter: ChatParticipantsInteractorOutputProtocol?

    private let directory: ChatParticipantDirectory
    private var cursor: String?
    private var reachedEnd = false
    private var loadTask: Task<Void, Never>?

    init(directory: ChatParticipantDirectory) {
        self.directory = directory
    }

    deinit {
        loadTask?.cancel()
    }
}

extension ChatParticipantsInteractor: ChatParticipantsInteractorInputProtocol {
    func loadNextPage() {
        guard loadTask == nil, !reachedEnd else { return }

        loadTask = Task { [weak self] in
            guard let self else { return }
            defer { loadTask = nil }

            do {
                let page = try await directory.page(after: cursor)
                cursor = page.next
                reachedEnd = page.next == nil
                presenter?.didReceive(page: page.participants, isLast: reachedEnd)
            } catch {
                presenter?.didFailToLoadPage()
            }
        }
    }
}
