import Foundation
import Keystore_iOS
import Operation_iOS
import SubstrateSdk

#if targetEnvironment(simulator)
    /// Drives the native person chat for the truapi E2E harness, so that a run builds native history before it
    /// switches the app to the chat product and checks the migration (plan E4). The harness cannot tap the simulator.
    ///
    /// The harness writes `{"action": …, "username": …, "text": …}` files with the extension `json` last to
    /// `tmp/truapi-e2e/native`. The feed runs them in name order and answers `<name>.done` or `<name>.failed` with the
    /// error. Actions: `request` finds `username` and sends it a chat request with `text`, `accept` accepts the
    /// request of `username`, `send` sends `text` to `username`, and `dump` writes every native contact with its
    /// messages to `tmp/truapi-e2e/native-state.json`.
    enum TrUAPIE2ENativeChat {
        struct Command: Decodable {
            let action: String
            let username: String?
            let text: String?
        }

        struct ContactState: Encodable {
            let username: String?
            let accountId: String
            let request: String?
            let accepted: Bool
            let messages: [MessageState]
        }

        struct MessageState: Encodable {
            let text: String?
            let status: String
            let timestamp: UInt64
        }

        enum Failure: Error {
            case missingField(String)
            case noUser(String)
            case noContact(String)
            case unknownAction(String)
        }

        private static let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("truapi-e2e", isDirectory: true)
        private static let directory = root.appendingPathComponent("native", isDirectory: true)

        private static var started = false

        /// `TRUAPI_IOS_E2E_PERSON_CHAT=native` or `product` sets which chat runs the user's chats, as the switch in
        /// Debug Settings does, before anything reads ``PersonChatMode``.
        static func applyPersonChatModeIfRequested() {
            switch ProcessInfo.processInfo.environment["TRUAPI_IOS_E2E_PERSON_CHAT"] {
            case "native": SettingsManager.shared.set(value: false, for: .productChatEnabled)
            case "product": SettingsManager.shared.set(value: true, for: .productChatEnabled)
            default: break
            }
        }

        /// Runs the feed once per launch, for the first flow state of the app.
        static func startIfRequested(flowState: ChatFlowState, logger: LoggerProtocol) {
            guard !started, ProcessInfo.processInfo.environment["TRUAPI_IOS_E2E_NATIVE_CHAT"] == "1" else { return }
            started = true

            let engines = ChatEngineFactory(flowState: flowState)
            Task.detached {
                await run(engines: engines, logger: logger)
            }
        }
    }

    private extension TrUAPIE2ENativeChat {
        static func run(engines: ChatEngineFactory, logger: LoggerProtocol) async {
            let fileManager = FileManager.default
            try? fileManager.createDirectory(at: directory, withIntermediateDirectories: true)

            while !Task.isCancelled {
                let queued = ((try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? [])
                    .filter { $0.pathExtension == "json" }
                    .sorted { $0.lastPathComponent < $1.lastPathComponent }

                for file in queued {
                    let name = file.deletingPathExtension().lastPathComponent
                    do {
                        let command = try JSONDecoder().decode(Command.self, from: Data(contentsOf: file))
                        logger.debug("[e2e:native] \(command.action) \(command.username ?? "")")
                        try await perform(command, engines: engines)
                        try Data().write(to: directory.appendingPathComponent("\(name).done"))
                    } catch {
                        logger.error("[e2e:native] \(name) failed: \(error)")
                        try? Data("\(error)".utf8).write(to: directory.appendingPathComponent("\(name).failed"))
                    }
                    try? fileManager.removeItem(at: file)
                }

                try? await Task.sleep(for: .milliseconds(200))
            }
        }

        static func perform(_ command: Command, engines: ChatEngineFactory) async throws {
            switch command.action {
            case "request":
                let username = try required(command.username, "username")
                let found = try await RemoteContactOperationFactory().search(by: username)
                guard let contact = found.first(where: { $0.username == username }) else {
                    throw Failure.noUser(username)
                }
                let engine = try engines.createChatEngine(
                    for: ChatOpenModel.NewRequest(remoteContact: contact, ownKeyId: try .main())
                )
                try await engine.sendUserMessage(with: .text(command.text ?? ""))
            case "accept":
                let account = try await contactAccount(try required(command.username, "username"))
                try await engines.createChatEngine(for: .person(account)).acceptChatRequest()
            case "send":
                let account = try await contactAccount(try required(command.username, "username"))
                try await engines.createChatEngine(for: .person(account))
                    .sendUserMessage(with: .text(try required(command.text, "text")))
            case "dump":
                try await dump()
            default:
                throw Failure.unknownAction(command.action)
            }
        }

        static func required(_ value: String?, _ field: String) throws -> String {
            guard let value else { throw Failure.missingField(field) }
            return value
        }

        static func contacts() async throws -> [Chat.Contact] {
            try await ChatContactRepositoryFactory(storageFacade: UserDataStorageFacade.shared)
                .createRepository(forFilter: nil)
                .fetchAllOperation(with: RepositoryFetchOptions())
                .asyncExecute()
        }

        static func contactAccount(_ username: String) async throws -> AccountId {
            guard let contact = try await contacts().first(where: { $0.username == username }) else {
                throw Failure.noContact(username)
            }
            return contact.accountId
        }

        static func dump() async throws {
            let messages = try await ChatMessageRepositoryFactory()
                .createRepository(forFilter: NSPredicate.messageByChatType(.person))
                .fetchAllOperation(with: RepositoryFetchOptions())
                .asyncExecute()
            let state = try await contacts().map { contact in
                ContactState(
                    username: contact.username,
                    accountId: contact.accountId.toHex(),
                    request: contact.chatRequest.map { "\($0.status)" },
                    accepted: contact.acceptedAt != nil,
                    messages: messages
                        .filter { $0.chatId == .person(contact.accountId) }
                        .sorted { $0.timestamp < $1.timestamp }
                        .map { message in
                            let text: String? = if case let .text(text) = message.content { text } else { nil }
                            return MessageState(text: text, status: "\(message.status)", timestamp: message.timestamp)
                        }
                )
            }
            try JSONEncoder().encode(state).write(to: root.appendingPathComponent("native-state.json"), options: .atomic)
        }
    }
#endif
