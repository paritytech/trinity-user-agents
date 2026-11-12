import Foundation

@MainActor
final class ExpandedPocketCardInteractor {
    weak var presenter: ExpandedPocketCardInteractorOutputProtocol?

    private let runtimeFactory: ExpandedPocketCardRuntimeMaking
    private let presenceRegistry: ProductPresenceRegistering?
    private let configuration: SPAConfiguration
    private let logger: LoggerProtocol

    init(
        runtimeFactory: ExpandedPocketCardRuntimeMaking,
        presenceRegistry: ProductPresenceRegistering?,
        configuration: SPAConfiguration,
        logger: LoggerProtocol
    ) {
        self.runtimeFactory = runtimeFactory
        self.presenceRegistry = presenceRegistry
        self.configuration = configuration
        self.logger = logger
    }
}

extension ExpandedPocketCardInteractor: ExpandedPocketCardInteractorInputProtocol {}
