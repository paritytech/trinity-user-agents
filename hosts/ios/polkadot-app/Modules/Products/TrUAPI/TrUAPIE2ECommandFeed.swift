import Foundation

#if targetEnvironment(simulator)
    /// A queue of user messages that the truapi E2E harness writes as files. The
    /// launcher uses it to send a list of commands to one product in one session.
    ///
    /// The harness puts one JSON file `{"room": …, "text": …}` for each message in
    /// `tmp/truapi-e2e/commands`, and gives it the extension `json` last. The feed
    /// sends the files in name order. After each send it writes `<name>.done` with
    /// the send time in milliseconds, or `<name>.failed` with the error, and
    /// removes the file.
    ///
    /// A file with `host` in place of `text` is an action of the user on the screen of the room, as
    /// ``TrUAPIE2EHostAction`` describes it.
    enum TrUAPIE2ECommandFeed {
        private struct Command: Decodable {
            let room: String
            let text: String?
            let host: String?
            let argument: String?
            let target: String?
        }

        private static let pollInterval: Duration = .milliseconds(100)

        static let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("truapi-e2e/commands", isDirectory: true)

        /// True when the harness asks for the feed, and names this product.
        static func isEnabled(for productId: String) -> Bool {
            let environment = ProcessInfo.processInfo.environment
            return environment["TRUAPI_IOS_E2E_CHAT_COMMANDS"] == "1"
                && environment["TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST"] == productId
        }

        /// True when the harness drives this product, so the product must run while no room screen is open.
        /// `TRUAPI_IOS_E2E_HOLD_WORKER=0` turns the hold off: then the worker runs as for a user, for example as a
        /// foreground worker that its manifest declares (VERBOSITY-WASM-PRODUCT I5).
        static func holdsWorker(for productId: String) -> Bool {
            holdsWorker(for: productId, environment: ProcessInfo.processInfo.environment)
        }

        static func holdsWorker(for productId: String, environment: [String: String]) -> Bool {
            environment["TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST"] == productId
                && environment["TRUAPI_IOS_E2E_HOLD_WORKER"] != "0"
                && (environment["TRUAPI_IOS_E2E_CHAT_COMMANDS"] == "1"
                    || environment["TRUAPI_IOS_E2E_CHAT_MESSAGE"] != nil)
        }

        /// Sends the queued messages with `send` until the task stops.
        static func run(
            productId: String,
            logger: LoggerProtocol,
            send: (_ text: String, _ roomId: String) async throws -> Void
        ) async {
            let fileManager = FileManager.default
            try? fileManager.createDirectory(at: directory, withIntermediateDirectories: true)

            while !Task.isCancelled {
                let files = (try? fileManager.contentsOfDirectory(
                    at: directory,
                    includingPropertiesForKeys: nil
                )) ?? []
                let queued = files
                    .filter { $0.pathExtension == "json" }
                    .sorted { $0.lastPathComponent < $1.lastPathComponent }

                for file in queued {
                    let name = file.deletingPathExtension().lastPathComponent
                    do {
                        let command = try JSONDecoder().decode(Command.self, from: Data(contentsOf: file))
                        logger.debug("[e2e] sending \(name) to room \(command.room)")
                        if let host = command.host {
                            await TrUAPIE2EHostAction(
                                room: command.room,
                                action: host,
                                argument: command.argument,
                                target: command.target
                            ).run(productId: productId)
                        } else if let text = command.text {
                            try await send(text, command.room)
                        }
                        let sentAt = String(Int64(Date().timeIntervalSince1970 * 1_000))
                        try Data(sentAt.utf8).write(to: directory.appendingPathComponent("\(name).done"))
                    } catch {
                        logger.error("[e2e] failed to send \(name): \(error)")
                        try? Data("\(error)".utf8).write(to: directory.appendingPathComponent("\(name).failed"))
                    }
                    try? fileManager.removeItem(at: file)
                }

                try? await Task.sleep(for: pollInterval)
            }
        }
    }
#endif
