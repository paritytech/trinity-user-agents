import AVFoundation
import Foundation

/// What the read-aloud service does now.
enum ChatReadAloudState: Equatable {
    case idle
    /// The service speaks a text of a message of a room.
    case speaking(chatId: Chat.Id, messageId: Chat.MessageId)
}

@MainActor
protocol ChatReadAloudServicing: AnyObject {
    var state: ChatReadAloudState { get }

    /// Called after each change of the state.
    var onChange: ((ChatReadAloudState) -> Void)? { get set }

    /// Called when the synthesizer starts to speak a text.
    var onSpeechStart: (() -> Void)? { get set }

    /// Stops what the service speaks, and then speaks the text of a message in the language of the host.
    func speak(_ text: String, messageId: Chat.MessageId, chatId: Chat.Id)

    /// Stops the speech when it is about a message of this room.
    func stop(in chatId: Chat.Id)
}

/// Speaks the text of a message with the speech synthesizer of the system (07-LATER §9.5, D33). A room offers it
/// only when its product declares `read_aloud`. The room screen stops it when the user leaves the room.
@MainActor
final class ChatReadAloudService: NSObject, ChatReadAloudServicing {
    static let shared = ChatReadAloudService()

    private let synthesizer: AVSpeechSynthesizer
    private let language: () -> String

    private(set) var state: ChatReadAloudState = .idle {
        didSet {
            if state != oldValue { onChange?(state) }
        }
    }

    var onChange: ((ChatReadAloudState) -> Void)?
    var onSpeechStart: (() -> Void)?

    init(
        synthesizer: AVSpeechSynthesizer = AVSpeechSynthesizer(),
        language: @escaping () -> String = { AVSpeechSynthesisVoice.currentLanguageCode() }
    ) {
        self.synthesizer = synthesizer
        self.language = language
        super.init()
        synthesizer.delegate = self
    }

    func speak(_ text: String, messageId: Chat.MessageId, chatId: Chat.Id) {
        let spoken = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !spoken.isEmpty else { return }

        synthesizer.stopSpeaking(at: .immediate)

        let utterance = AVSpeechUtterance(string: spoken)
        utterance.voice = AVSpeechSynthesisVoice(language: language())
        state = .speaking(chatId: chatId, messageId: messageId)
        synthesizer.speak(utterance)
    }

    func stop(in chatId: Chat.Id) {
        guard case let .speaking(speakingChatId, _) = state, speakingChatId == chatId else { return }

        state = .idle
        synthesizer.stopSpeaking(at: .immediate)
    }
}

extension ChatReadAloudService: AVSpeechSynthesizerDelegate {
    nonisolated func speechSynthesizer(_: AVSpeechSynthesizer, didStart _: AVSpeechUtterance) {
        Task { @MainActor [weak self] in
            self?.onSpeechStart?()
        }
    }

    nonisolated func speechSynthesizer(_: AVSpeechSynthesizer, didFinish _: AVSpeechUtterance) {
        Task { @MainActor [weak self] in
            guard let self, !synthesizer.isSpeaking else { return }
            state = .idle
        }
    }
}

extension ChatReadAloudService {
    /// The text that the service speaks for a message: the plain text of Markdown, without the marks.
    nonisolated static func plainText(fromMarkdown markdown: String) -> String {
        let options = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace,
            failurePolicy: .returnPartiallyParsedIfPossible
        )

        guard let attributed = try? AttributedString(markdown: markdown, options: options) else {
            return markdown
        }

        return String(attributed.characters)
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map { Self.withoutBlockMarks(String($0)) }
            .joined(separator: "\n")
    }

    /// Removes the marks at the start of a Markdown line: headings, quotes and list items.
    private nonisolated static func withoutBlockMarks(_ line: String) -> String {
        let pattern = #"^\s{0,3}(#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+)"#
        guard let range = line.range(of: pattern, options: .regularExpression) else {
            return line
        }
        return String(line[range.upperBound...])
    }
}
