import Foundation
import SubstrateSdk
import ChainRegistry

final class PolkadotSigningInteractor {
    weak var presenter: PolkadotSigningInteractorOutputProtocol?

    private let signingContext: PolkadotSigningContextProtocol
    private let requestResultFactory: PolkadotSigningRequestResultMaking
    private let signatureFactory: PolkadotSignatureMaking
    private let chainRegistry: ChainRegistryProtocol
    private let logger: LoggerProtocol

    init(
        signingContext: PolkadotSigningContextProtocol,
        requestResultFactory: PolkadotSigningRequestResultMaking = PolkadotSigningRequestResultFactory(),
        signatureFactory: PolkadotSignatureMaking = PolkadotSignatureFactory(),
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.signingContext = signingContext
        self.requestResultFactory = requestResultFactory
        self.signatureFactory = signatureFactory
        self.chainRegistry = chainRegistry
        self.logger = logger
    }
}

extension PolkadotSigningInteractor: PolkadotSigningInteractorInputProtocol {
    func setup() {
        parseSigningRequest()
    }

    func signParsedResult(_ result: PolkadotParsedSigningRequestResult) {
        Task {
            await self.presenter?.didStartSigning()

            do {
                logger.debug("Going to build signature for \(signingContext.requester.name)")
                let signingResult = try await signatureFactory.makeSignature(
                    result: result,
                    chainRegistry: chainRegistry
                )

                logger.debug("Signature built for \(signingContext.requester.name)")

                await self.presenter?.didFinishSigning(with: signingResult)
            } catch {
                logger.error("Error: \(error)")
                await self.presenter?.didFailToSign(with: error)
            }
        }
    }
}

private extension PolkadotSigningInteractor {
    func parseSigningRequest() {
        Task {
            await self.presenter?.didStartParsingRequest()

            do {
                let result = try await requestResultFactory.makeParsedResult(
                    signingContext: signingContext
                )
                logger.debug("Parsed result: \(result)")
                await self.presenter?.didFinishParsingRequest(with: result)
            } catch {
                logger.error("Error: \(error)")
                await self.presenter?.didFailToParseRequest(with: error)
            }
        }
    }
}

enum PolkadotSigningError: Error {
    case missingChain
    case missingRuntimeProvider
    case invalidAddress
    case accountMismatch
    case invalidNumberInHex
    case invalidVersion
    case rawDataCorrupted
    case missingExtensionValue
}
