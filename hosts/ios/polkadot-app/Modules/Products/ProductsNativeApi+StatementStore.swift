import Foundation
import NovaCrypto
import Products
import StatementStore
import AsyncExtensions
import KeyDerivation

// MARK: - Statement Store

extension ProductsNativeApi {
    func subscribeStatements(filter: TopicFilter) throws -> AnyAsyncSequence<StatementsPageDto> {
        let connection = try makeStatementStoreConnection()

        return try connection.subscribeStatements(with: filter)
            .map { page in
                let statements = try page.statements.compactMap { data in
                    let remoteStatement = try Statement.fromScaleEncoded(data)
                    return try StatementDto(remoteStatement: remoteStatement)
                }
                return StatementsPageDto(statements: statements, isComplete: page.isComplete)
            }
            .eraseToAnyAsyncSequence()
    }

    func createStatementProof(_ request: CreateStatementProofDto) async throws -> StatementProofDto {
        // TODO: Remove when all migrate to authorized
        let signer = try makeStatementSigner(
            for: WalletDerivationPath.main(for: DotNsTldProviderFacade.shared.currentTldOrError())
        )

        let unsignedStatement = try request.toUnsignedRemoteStatement()

        let proofData = try unsignedStatement.deriveProofData()
        let proof = try signer.sign(proofData)

        return StatementProofDto(proof: proof)
    }

    func createStatementProofAuthorized(
        _ request: CreateStatementProofAuthorizedDto
    ) async throws -> StatementProofDto {
        let wallet = try await statementStoreSponsor.sponsor(productId: productId)
        let signer = try makeStatementSigner(from: wallet)

        let unsignedStatement = try request.toUnsignedRemoteStatement()

        let proofData = try unsignedStatement.deriveProofData()
        let proof = try signer.sign(proofData)

        return StatementProofDto(proof: proof)
    }

    func submitStatement(_ statement: StatementDto) async throws {
        guard try await permissionGuard.consumePermission(
            productId: productId,
            permission: .statementSubmitAccess
        ) else {
            throw ProductNativeApiError.permissionDenied
        }

        let remoteStatement = try statement.toRemoteStatement()

        let connection = try makeStatementStoreConnection()
        try await connection.submitStatement(with: remoteStatement)
    }
}

// MARK: - Private Helpers

private extension ProductsNativeApi {
    func makeStatementStoreConnection() throws -> StatementStoreConnecting {
        let connection = try chainRegistry.getConnectionOrError(for: AppConfig.Chains.chatChain)
        return StatementStoreConnection(
            connection: connection,
            retryMatcher: StatementSubmitErrorMatcher.retryWhenTimeoutOrNoAllowance(),
            logger: logger
        )
    }

    func makeStatementSigner(for derivationPath: String) throws -> StatementStoreSigning {
        let keypairFactory = WalletMnemonicKeypairFactory(
            derivationPath: derivationPath,
            entropyManager: entropyManager
        )

        let keypair = try keypairFactory.deriveKeypair()

        let publicKey = try SNPublicKey(rawData: keypair.publicKey().rawData())
        let privateKey = try SNPrivateKey(rawData: keypair.privateKey().rawData())

        return StatementStoreKeypairSigner(
            keypair: SNKeypair(privateKey: privateKey, publicKey: publicKey)
        )
    }

    func makeStatementSigner(from wallet: any WalletManaging) throws -> StatementStoreSigning {
        let rawPublicKey = try wallet.getRawPublicKey()
        let rawPrivateKey = try wallet.fetchRawSecretKey()

        let publicKey = try SNPublicKey(rawData: rawPublicKey)
        let privateKey = try SNPrivateKey(rawData: rawPrivateKey)

        return StatementStoreKeypairSigner(
            keypair: SNKeypair(privateKey: privateKey, publicKey: publicKey)
        )
    }
}
