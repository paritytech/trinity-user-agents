import Foundation
import PolkadotUI

@MainActor
final class ChatSearchPresenter {
    weak var view: ChatSearchViewProtocol?
    let wireframe: ChatSearchWireframeProtocol
    let interactor: ChatSearchInteractorInputProtocol
    /// The room, for the markers of the E2E harness.
    let roomId: String
    /// The name of the other side of the room, for a hit of the product.
    let peerName: String
    let timestampFormatter: TimestampFormatting
    let logger: LoggerProtocol

    private(set) var state: ChatSearchViewState = .idle
    /// The hits on the screen, newest first.
    private(set) var hits: [ChatSearchHit] = []
    private var query = ""
    var e2eObserver: NSObjectProtocol?

    init(
        interactor: ChatSearchInteractorInputProtocol,
        wireframe: ChatSearchWireframeProtocol,
        roomId: String,
        peerName: String,
        timestampFormatter: TimestampFormatting = ContactTimestampFormatter(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.roomId = roomId
        self.peerName = peerName
        self.timestampFormatter = timestampFormatter
        self.logger = logger
    }
}

extension ChatSearchPresenter: ChatSearchPresenterProtocol {
    func setup() {
        provide(.idle)
        #if targetEnvironment(simulator)
            e2eObserver = observeE2EActions()
        #endif
    }

    func search(text: String) {
        query = text.trimmingCharacters(in: .whitespacesAndNewlines)
        interactor.search(text: text)
    }

    func loadMore() {
        interactor.loadMore()
    }

    func select(messageId: Chat.MessageId) {
        wireframe.close(from: view, showing: messageId)
    }
}

extension ChatSearchPresenter: ChatSearchInteractorOutputProtocol {
    func didStartSearch() {
        if hits.isEmpty {
            provide(.searching)
        }
    }

    func didReceive(hits: [ChatSearchHit], hasMore: Bool) {
        self.hits = hits
        provide(hits.isEmpty ? .empty : .results(hits.map(row(for:)), hasMore: hasMore))
    }

    func didClear() {
        hits = []
        provide(.idle)
    }

    func didFail(_: ChatRoomSearchError) {
        hits = []
        provide(.unavailable)
    }
}

private extension ChatSearchPresenter {
    func provide(_ state: ChatSearchViewState) {
        self.state = state
        view?.didReceive(state: state)
        markState()
    }

    func row(for hit: ChatSearchHit) -> ChatSearchRowViewModel {
        let excerpt = ChatSearchExcerpt(text: hit.text, query: query)
        return ChatSearchRowViewModel(
            messageId: hit.messageId,
            author: hit.isOutgoing ? String(localized: .chatReplyYou) : hit.authorParticipantId ?? peerName,
            time: timestampFormatter.string(for: Date.fromChatTimestamp(hit.timestamp)),
            line: excerpt.line,
            match: excerpt.match
        )
    }
}

/// The line of a message that holds the match, cut so that the match shows, and the range of the match in it.
struct ChatSearchExcerpt: Equatable {
    /// The characters before the match that the excerpt keeps.
    static let leadingContext = 24

    let line: String
    let match: NSRange?

    init(text: String, query: String) {
        let options: String.CompareOptions = [.caseInsensitive, .diacriticInsensitive, .widthInsensitive]
        guard !query.isEmpty, let found = text.range(of: query, options: options) else {
            line = text.components(separatedBy: .newlines).first ?? text
            match = nil
            return
        }

        var lineText = String(text[text.lineRange(for: found)]).trimmingCharacters(in: .newlines)
        if let inLine = lineText.range(of: query, options: options),
           lineText.distance(from: lineText.startIndex, to: inLine.lowerBound) > Self.leadingContext * 2 {
            let start = lineText.index(inLine.lowerBound, offsetBy: -Self.leadingContext)
            lineText = "…" + lineText[start...]
        }

        line = lineText
        match = lineText.range(of: query, options: options).map { NSRange($0, in: lineText) }
    }
}
