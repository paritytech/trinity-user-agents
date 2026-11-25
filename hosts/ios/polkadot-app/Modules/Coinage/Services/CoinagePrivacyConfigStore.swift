import AsyncExtensions
import Coinage
import Foundation
import os

/// `@unchecked Sendable`: the only mutable state is the observation task behind its lock, and the
/// non-Sendable config observer is called only inside that lock.
final class CoinagePrivacyConfigStore: CoinagePrivacyConfigProviding, @unchecked Sendable {
    static let shared = CoinagePrivacyConfigStore()

    private let subject: AsyncCurrentValueSubject<CoinagePrivacyConfig>
    private let configObserver: RemoteConfigObserving
    private let logger: LoggerProtocol
    private let observation = OSAllocatedUnfairLock<Task<Void, Never>?>(initialState: nil)

    init(
        configObserver: RemoteConfigObserving = FirebaseFacade.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.configObserver = configObserver
        self.logger = logger
        subject = AsyncCurrentValueSubject(.bundled)
    }

    deinit {
        observation.withLock { $0?.cancel() }
    }

    var privacyConfig: CoinagePrivacyConfig {
        subject.value
    }

    func privacyConfigStream() -> AnyAsyncSequence<CoinagePrivacyConfig> {
        subject.eraseToAnyAsyncSequence()
    }

    func start() {
        observation.withLock { task in
            guard task == nil else { return }

            let stream = configObserver.remoteConfigStream()

            task = Task { [subject, logger] in
                do {
                    for try await config in stream {
                        let parsed = CoinagePrivacyConfigParser.parse(config.coinagePrivacyConfig)
                        guard parsed != subject.value else { continue }
                        subject.send(parsed)
                    }
                } catch {
                    logger.error("Coinage privacy config stopped following remote config: \(error)")
                }
            }
        }
    }
}
