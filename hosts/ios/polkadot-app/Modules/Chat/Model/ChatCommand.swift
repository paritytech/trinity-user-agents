import Foundation

extension Chat {
    /// A slash command that the user types into a product room: `/name arguments`.
    struct Command: Equatable {
        static let maxNameBytes = 256

        let name: String
        let arguments: String

        init(name: String, arguments: String) {
            self.name = name
            self.arguments = arguments
        }

        /// Reads a command from the text of a message, or gives nil when the text is an ordinary message.
        ///
        /// The name ends at the first white space. A name that is empty, that contains a slash, or that is
        /// longer than the core accepts for an identifier, makes the text an ordinary message.
        init?(text: String) {
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard trimmed.hasPrefix("/") else { return nil }

            let body = trimmed.dropFirst()
            let nameEnd = body.firstIndex(where: \.isWhitespace) ?? body.endIndex
            let name = String(body[..<nameEnd])

            guard
                !name.isEmpty,
                !name.contains("/"),
                name.utf8.count <= Self.maxNameBytes
            else {
                return nil
            }

            self.name = name
            arguments = body[nameEnd...].trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }
}

/// A chat extension that can take a slash command instead of a posted message.
protocol ChatCommandHandling {
    /// Gives true when the extension took the command, so the host must not post the text.
    func handle(command: Chat.Command, roomId: String?) async -> Bool
}
