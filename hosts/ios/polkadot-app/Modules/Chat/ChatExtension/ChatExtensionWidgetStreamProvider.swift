import Foundation
import AsyncExtensions
import PolkadotUI

struct ChatExtensionWidgetUpdate {
    let extensionId: ChatExtension.Id
    let configuration: (any HashableContentConfiguration)?
}

protocol ChatExtensionWidgetStreaming: AnyObject {
    func setup()
    func throttle()
    func widgetConfigurationStream() -> AnyAsyncSequence<ChatExtensionWidgetUpdate>
}

// @unchecked Sendable: all mutable state confined to serial workQueue
final class ChatExtensionWidgetStreamProvider: @unchecked Sendable {
    private let registry: ChatExtensionsRegistering
    private let logger: LoggerProtocol
    private let workQueue = DispatchQueue(label: "ChatExtensionWidgetStreamProvider.state")
    private let widgetSubject = AsyncPassthroughSubject<ChatExtensionWidgetUpdate>()

    private var registryTask: Task<Void, Never>?
    private var widgetTasks: [ChatExtension.Id: Task<Void, Never>] = [:]

    init(
        registry: ChatExtensionsRegistering,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.registry = registry
        self.logger = logger
    }

    deinit {
        registryTask?.cancel()
        widgetTasks.values.forEach { $0.cancel() }
    }
}

extension ChatExtensionWidgetStreamProvider: ChatExtensionWidgetStreaming {
    func setup() {
        workQueue.async { [weak self] in
            self?.setupIfNeeded()
        }
    }

    func throttle() {
        workQueue.async { [weak self] in
            self?.cancelStreams()
        }
    }

    func widgetConfigurationStream() -> AnyAsyncSequence<ChatExtensionWidgetUpdate> {
        widgetSubject.eraseToAnyAsyncSequence()
    }
}

private extension ChatExtensionWidgetStreamProvider {
    func setupIfNeeded() {
        guard registryTask == nil else {
            return
        }

        registry.getWidgetProviders().forEach(startWidgetProvider)

        registryTask = Task { [weak self, registry] in
            do {
                for try await change in registry.onChangeStream {
                    self?.workQueue.async { [weak self] in
                        self?.handleRegistryChange(change)
                    }
                }
            } catch {
                self?.logger.error("Extension widget registry stream failed: \(error)")
            }
        }
    }

    func cancelStreams() {
        registryTask?.cancel()
        registryTask = nil

        widgetTasks.values.forEach { $0.cancel() }
        widgetTasks.removeAll()
    }

    func handleRegistryChange(_ change: ChatExtensionRegistryChange) {
        switch change {
        case let .enabled(extensionIds):
            registry
                .getWidgetProviders()
                .filter { extensionIds.contains($0.extensionId) }
                .forEach(startWidgetProvider)

        case let .disabled(extensionIds):
            extensionIds.forEach(stopWidgetProvider)
        }
    }

    func startWidgetProvider(_ widgetProvider: ChatExtensionWidgetProvider) {
        let extensionId = widgetProvider.extensionId
        guard widgetTasks[extensionId] == nil else {
            return
        }

        widgetTasks[extensionId] = Task { [weak self, provider = widgetProvider.provider, extensionId] in
            do {
                let widgetConfigurationStream = try await provider.widgetConfigurationStream()
                for try await configuration in widgetConfigurationStream {
                    guard !Task.isCancelled else {
                        return
                    }

                    self?.sendWidgetUpdate(.init(
                        extensionId: extensionId,
                        configuration: configuration
                    ))
                }
            } catch {
                guard !Task.isCancelled else {
                    return
                }

                self?.logger.error("Extension widget stream failed: \(error)")
            }

            guard !Task.isCancelled else {
                return
            }

            self?.didFinishWidgetProvider(extensionId: extensionId)
        }
    }

    func stopWidgetProvider(extensionId: ChatExtension.Id) {
        cancelWidgetProviderTask(extensionId: extensionId)
        sendWidgetUpdate(.init(extensionId: extensionId, configuration: nil))
    }

    func cancelWidgetProviderTask(extensionId: ChatExtension.Id) {
        widgetTasks[extensionId]?.cancel()
        widgetTasks[extensionId] = nil
    }

    func didFinishWidgetProvider(extensionId: ChatExtension.Id) {
        workQueue.async { [weak self] in
            self?.widgetTasks[extensionId] = nil
        }
    }

    func sendWidgetUpdate(_ update: ChatExtensionWidgetUpdate) {
        workQueue.async { [weak self] in
            guard let self else {
                return
            }

            guard update.configuration == nil || widgetTasks[update.extensionId] != nil else {
                return
            }

            widgetSubject.send(update)
        }
    }
}
