import UIKit
import SubstrateSdk
import Operation_iOS
import KeyDerivation
import SubstrateSdkExt
import StructuredConcurrency
import Individuality

struct ClaimLiteUsernameDependency {
    let walletSetupManagerFactory: () -> WalletSetupManaging
    let registrationParamsFactory: (
        _ mainWallet: WalletManaging,
        _ liteVrfManager: BandersnatchKeyManager
    ) throws -> LitePersonParamsFactoryProtocol
    let chainTimeProvider: () -> ChainTimeProviding
    let usernameOperationFactory: () -> UsernameOperationFactoryProtocol
    let usernameStorage: () -> UsernameStoring
    let walletRepo: WalletManagerRepositoryProtocol
    let vrfRepo: BandersnatchManagerRepositoryProtocol
}

final class ClaimLiteUsernameInteractor {
    weak var presenter: ClaimLiteUsernameInteractorOutputProtocol?

    private var walletCreated: Bool
    let dependencies: ClaimLiteUsernameDependency

    lazy var usernameOperationFactory = dependencies.usernameOperationFactory()
    lazy var usernameStorage = dependencies.usernameStorage()

    let logger: LoggerProtocol

    init(
        walletCreated: Bool,
        dependencies: ClaimLiteUsernameDependency,
        logger: LoggerProtocol
    ) {
        self.walletCreated = walletCreated
        self.dependencies = dependencies
        self.logger = logger
    }
}

extension ClaimLiteUsernameInteractor: ClaimUsernameInteractorInputProtocol {
    var metadata: UsernameMetadata {
        .default
    }

    func claim(username: Username) async throws -> Username {
        await presenter?.didChangeAccountCreation(inProgress: true)

        do {
            if !walletCreated {
                try await createWallet()
            }

            return try await performClaim(
                username: username,
                registrationFactory: registrationFactory()
            )
        } catch {
            await presenter?.didChangeAccountCreation(inProgress: false)
            throw error
        }
    }

    func check(username: Username) async throws -> UsernameAvailableType {
        try await withMinDuration(.milliseconds(300)) {
            try await usernameOperationFactory.availableUsernameWrapper(for: username.value).asyncExecute()
        }
    }

    func save(username: Username) async {
        usernameStorage.username = username
        await presenter?.didSaveUsername()
    }
}

private extension ClaimLiteUsernameInteractor {
    enum FlowError: Error {
        case internalError
    }

    func registrationFactory() throws -> LitePersonParamsFactoryProtocol {
        let mainWallet = try dependencies.walletRepo.main()
        let liteVrfManager = try dependencies.vrfRepo.litePerson()
        return try dependencies.registrationParamsFactory(mainWallet, liteVrfManager)
    }

    func performClaim(
        username: Username,
        registrationFactory: LitePersonParamsFactoryProtocol
    ) async throws -> Username {
        let attester = try await usernameOperationFactory.attester().asyncExecute().attester
        let signedAt = try await dependencies.chainTimeProvider().nowSeconds()

        let requestParams = try registrationFactory.registrationParams(
            for: username,
            attester: attester,
            signedAt: signedAt
        )

        let response = try await usernameOperationFactory.claimUsername(using: requestParams).asyncExecute()

        return Username(value: response.username)
    }

    func createWallet() async throws {
        guard await authorizeUserAsync() else {
            throw FlowError.internalError
        }

        let walletSetupManager = dependencies.walletSetupManagerFactory()
        try walletSetupManager.createWallets(with: nil)

        walletCreated = true
    }

    func authorizeUserAsync() async -> Bool {
        guard let presenter else {
            return false
        }

        return await withCheckedContinuation { continuation in
            Task { @MainActor in
                presenter.authorizeUser { authorized in
                    continuation.resume(returning: authorized)
                }
            }
        }
    }
}

private extension LitePersonParamsFactoryProtocol {
    func registrationParams(
        for username: Username,
        attester: AccountId,
        signedAt: UInt64
    ) throws -> RegisterUsernameParameters {
        let params = try deriveRegistrationParams(
            for: username.partialUsername,
            attester: attester,
            signedAt: signedAt,
            reservedUsername: username.partialUsername
        )

        return try RegisterUsernameParameters(
            username: username.partialUsername,
            preferredDigits: username.digits,
            candidateAccountId: params.litePerson.accountId.toAddress(using: .genericFormat),
            candidateSignature: params.litePerson.accountIdProofSignature,
            ringVrfKey: params.litePerson.personMemberKey,
            proofOfOwnership: params.litePerson.membershipProofSignature,
            identifierKey: params.litePerson.encryptionIdentifier.scaleEncoded(),
            consumerRegistrationSignature: params.litePerson.resourcesSignature,
            dotns: .init(
                signature: params.reservation.signature,
                signedAt: Int(params.reservation.signedAt),
                reservedUsername: params.reservation.reservedUsername
            )
        )
    }
}
