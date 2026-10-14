import Foundation

/// Parses the rust-core signing review for display and resolves the user's
/// decision through the context. No signature is produced here — the rust
/// core signs after receiving the confirmation.
final class ProductsSignConfirmInteractor {
    weak var presenter: ProductsSignConfirmInteractorOutputProtocol?

    private let context: any ProductsSignConfirmContextProtocol
    private let modelFactory: ProductsSignConfirmModelMaking
    private let logger: LoggerProtocol

    init(
        context: any ProductsSignConfirmContextProtocol,
        modelFactory: ProductsSignConfirmModelMaking = ProductsSignConfirmModelFactory(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.context = context
        self.modelFactory = modelFactory
        self.logger = logger
    }
}

extension ProductsSignConfirmInteractor: ProductsSignConfirmInteractorInputProtocol {
    func setup() {
        parseSigningRequest()
    }
}

private extension ProductsSignConfirmInteractor {
    func parseSigningRequest() {
        Task {
            await self.presenter?.didStartParsingRequest()

            do {
                let model = try await modelFactory.makeModel(
                    from: context.input,
                    requester: context.requester
                )
                await self.presenter?.didFinishParsingRequest(with: model)
            } catch {
                logger.error("Failed to parse rust-core signing review: \(error)")
                await self.presenter?.didFailToParseRequest(with: error)
            }
        }
    }
}
