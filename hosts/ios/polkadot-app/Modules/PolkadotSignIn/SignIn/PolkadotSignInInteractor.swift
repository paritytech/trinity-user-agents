import Foundation

final class PolkadotSignInInteractor {
    weak var presenter: PolkadotSignInInteractorOutputProtocol?

    private let serviceCoordinator: ServiceCoordinatorProtocol
    private let deviceMessageBroadcaster: DeviceMessageBroadcasting
    private let localNetworkPermissionService: LocalNetworkPermissionServicing
    private let url: URL
    private let logger: LoggerProtocol

    init(
        serviceCoordinator: ServiceCoordinatorProtocol,
        deviceMessageBroadcaster: DeviceMessageBroadcasting,
        localNetworkPermissionService: LocalNetworkPermissionServicing,
        url: URL,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.serviceCoordinator = serviceCoordinator
        self.deviceMessageBroadcaster = deviceMessageBroadcaster
        self.localNetworkPermissionService = localNetworkPermissionService
        self.url = url
        self.logger = logger
    }
}

extension PolkadotSignInInteractor: PolkadotSignInInteractorInputProtocol {
    func setup() {
        fetchInput()
    }

    func approve(with input: HandshakeInput) {
        Task {
            await reportSendHandshakeStart()

            do {
                logger.debug("Going to send handshake for \(input.metadata.name)")
                let device = try await serviceCoordinator
                    .polkadotHandshakeService
                    .sendHandshake(with: input)
                logger.debug("Handshake sent for \(input.metadata.name)")

                try await deviceMessageBroadcaster.broadcastDeviceAdded(
                    statementAccountId: device.statementAccountId,
                    encryptionPublicKey: device.encryptionPublicKey
                )

                await reportSendHandshakeFinish(device: device)
            } catch {
                logger.error("Error: \(error)")
                await reportSendHandshakeError(error)
            }
        }
    }
}

private extension PolkadotSignInInteractor {
    enum SignInError: Error {
        case missingInput
    }

    func fetchInput() {
        Task {
            await reportFetchInputStart()

            do {
                guard let input = try await serviceCoordinator
                    .polkadotHandshakeService
                    .prepareInput(for: url)
                else {
                    throw SignInError.missingInput
                }
                logger.debug("Received handshake from \(input.metadata.name)")
                await reportFetchInputFinish(input)
            } catch {
                logger.error("Error: \(error)")
                await reportFetchInputError(error)
            }
        }
    }
}

@MainActor
private extension PolkadotSignInInteractor {
    func reportFetchInputStart() {
        presenter?.didStartFetchingInput()
    }

    func reportFetchInputFinish(_ input: HandshakeInput) async {
        presenter?.didFinishFetchingInput(input)
        await localNetworkPermissionService.requestPermissionIfNeeded()
    }

    func reportFetchInputError(_ error: Error) {
        presenter?.didFailToFetchInput(with: error)
    }

    func reportSendHandshakeStart() {
        presenter?.didStartSendingHandshake()
    }

    func reportSendHandshakeFinish(device: Chat.LocalDevice) {
        presenter?.didFinishSendingHandshake(with: device)
    }

    func reportSendHandshakeError(_ error: Error) {
        presenter?.didFailToSendHandshake(with: error)
    }
}
