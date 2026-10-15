import Foundation

/// Asks the search of the room after a pause in the typing, and asks for older pages on demand. A new text cancels
/// the search of the text before it.
@MainActor
final class ChatSearchInteractor {
    weak var presenter: ChatSearchInteractorOutputProtocol?

    private let source: ChatRoomSearching
    private var text = ""
    private var hits: [ChatSearchHit] = []
    private var next: ChatRoomSearchCursor?
    private var searchTask: Task<Void, Never>?
    private var pageTask: Task<Void, Never>?

    init(source: ChatRoomSearching) {
        self.source = source
    }

    deinit {
        searchTask?.cancel()
        pageTask?.cancel()
    }
}

extension ChatSearchInteractor: ChatSearchInteractorInputProtocol {
    func search(text newText: String) {
        let trimmed = newText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed != text else { return }

        text = trimmed
        hits = []
        next = nil
        searchTask?.cancel()
        pageTask?.cancel()
        pageTask = nil

        // One grapheme at least (07-LATER §3.5).
        guard !trimmed.isEmpty else {
            presenter?.didClear()
            return
        }

        presenter?.didStartSearch()
        searchTask = Task { [weak self, source] in
            try? await Task.sleep(for: source.typingPause)
            guard !Task.isCancelled else { return }
            await self?.loadPage(of: trimmed, after: nil)
        }
    }

    func loadMore() {
        guard let next, pageTask == nil, searchTask == nil else { return }

        let text = text
        pageTask = Task { [weak self] in
            await self?.loadPage(of: text, after: next)
            self?.pageTask = nil
        }
    }
}

private extension ChatSearchInteractor {
    func loadPage(of text: String, after cursor: ChatRoomSearchCursor?) async {
        do {
            let page = try await source.page(matching: text, after: cursor)
            guard !Task.isCancelled, text == self.text else { return }

            hits += page.hits.filter { hit in !hits.contains { $0.messageId == hit.messageId } }
            next = page.next
            if cursor == nil { searchTask = nil }
            presenter?.didReceive(hits: hits, hasMore: next != nil)
        } catch {
            guard !Task.isCancelled, text == self.text else { return }

            if cursor == nil { searchTask = nil }
            presenter?.didFail(error as? ChatRoomSearchError ?? .unavailable)
        }
    }
}
