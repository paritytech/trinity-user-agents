import Foundation
import Testing

@testable import polkadot_app

@Suite("PolkadotSigningRequestResultFactory Tests")
struct PolkadotSigningRequestResultFactoryTests {
    // MARK: - Transaction Tests

    @Test("Transaction happy path returns result with .transaction")
    func transactionHappyPath() async throws {
        let wallet = SsoTestData.makeWallet()
        let genesisHashData = Data.random(of: 32)!
        let genesisHash = genesisHashData.toHex()
        let chain = SsoTestData.makeChain(genesisHash: genesisHash)
        let account = SsoTestData.makeAccount()

        let chainRegistry = SsoTestData.makeChainRegistry(chain: chain, genesisHash: genesisHash)
        let factory = SsoTestData.makeFactory(chainRegistry: chainRegistry)

        let transaction = SsoTestData.makeTransaction(account: account, genesisHash: genesisHashData)
        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.transaction(transaction)
        let context = SsoTestData.makeSigningContext(wallet: wallet, signingRequest: signingRequest)

        let result = try await factory.makeParsedResult(signingContext: context)

        #expect(result.isTransaction)
    }

    @Test("Transaction missing chain throws missingChain")
    func transactionMissingChain() async throws {
        let wallet = SsoTestData.makeWallet()
        let genesisHashData = Data.random(of: 32)!
        let account = SsoTestData.makeAccount()

        let factory = SsoTestData.makeFactory()

        let transaction = SsoTestData.makeTransaction(account: account, genesisHash: genesisHashData)
        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.transaction(transaction)
        let context = SsoTestData.makeSigningContext(wallet: wallet, signingRequest: signingRequest)

        await #expect(throws: PolkadotSigningError.missingChain) {
            try await factory.makeParsedResult(signingContext: context)
        }
    }

    @Test("Transaction missing runtime provider throws missingRuntimeProvider")
    func transactionMissingRuntimeProvider() async throws {
        let wallet = SsoTestData.makeWallet()
        let genesisHashData = Data.random(of: 32)!
        let genesisHash = genesisHashData.toHex()
        let chain = SsoTestData.makeChain(genesisHash: genesisHash)
        let account = SsoTestData.makeAccount()

        let chainRegistry = SsoTestData.makeChainRegistry(chain: chain, genesisHash: genesisHash, coderFactory: nil)
        let factory = SsoTestData.makeFactory(chainRegistry: chainRegistry)

        let transaction = SsoTestData.makeTransaction(account: account, genesisHash: genesisHashData)
        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.transaction(transaction)
        let context = SsoTestData.makeSigningContext(wallet: wallet, signingRequest: signingRequest)

        await #expect(throws: PolkadotSigningError.missingRuntimeProvider) {
            try await factory.makeParsedResult(signingContext: context)
        }
    }

    @Test("Transaction account mismatch throws accountMismatch")
    func transactionAccountMismatch() async throws {
        let genesisHashData = Data.random(of: 32)!
        let genesisHash = genesisHashData.toHex()
        let chain = SsoTestData.makeChain(genesisHash: genesisHash)
        let account = SsoTestData.makeAccount()

        let chainRegistry = SsoTestData.makeChainRegistry(chain: chain, genesisHash: genesisHash)
        let factory = SsoTestData.makeFactory(chainRegistry: chainRegistry)

        let transaction = SsoTestData.makeTransaction(account: account, genesisHash: genesisHashData)
        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.transaction(transaction)
        let context = SsoTestData.makeFailingSigningContext(
            signingRequest: signingRequest,
            error: PolkadotSigningError.accountMismatch
        )

        await #expect(throws: PolkadotSigningError.accountMismatch) {
            try await factory.makeParsedResult(signingContext: context)
        }
    }

    // MARK: - Raw Bytes Tests

    @Test("Raw bytes (.bytes) happy path returns result with .rawBytes and nil codingFactory")
    func rawBytesHappyPath() async throws {
        let wallet = SsoTestData.makeWallet()
        let account = SsoTestData.makeAccount()

        let rawData = Data([0xDE, 0xAD, 0xBE, 0xEF])

        let suffixData = "</Bytes>".data(using: .ascii)!
        let prefixData = "<Bytes>".data(using: .ascii)!

        let targetData = prefixData + rawData + suffixData

        let rawPayload = PolkadotHostRemoteMessage.SigningRawPayload(
            account: account,
            type: .bytes(rawData)
        )
        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.rawPayload(rawPayload)

        let factory = SsoTestData.makeFactory()
        let context = SsoTestData.makeSigningContext(wallet: wallet, signingRequest: signingRequest)
        let result = try await factory.makeParsedResult(signingContext: context)

        #expect(!result.isTransaction)

        if case let .rawBytes(data) = result.parsedRequest {
            #expect(data == targetData)
        } else {
            Issue.record("Expected .rawBytes")
        }
    }

    @Test("Raw bytes (.payload string) wraps in <Bytes> tags")
    func rawBytesPayloadStringWrapped() async throws {
        let wallet = SsoTestData.makeWallet()
        let account = SsoTestData.makeAccount()

        let rawPayload = PolkadotHostRemoteMessage.SigningRawPayload(
            account: account,
            type: .payload("hello")
        )
        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.rawPayload(rawPayload)

        let factory = SsoTestData.makeFactory()
        let context = SsoTestData.makeSigningContext(wallet: wallet, signingRequest: signingRequest)
        let result = try await factory.makeParsedResult(signingContext: context)

        if case let .rawBytes(data) = result.parsedRequest {
            let string = String(data: data, encoding: .utf8)
            #expect(string == "<Bytes>hello</Bytes>")
        } else {
            Issue.record("Expected .rawBytes")
        }
    }

    @Test("Raw bytes account mismatch throws accountMismatch")
    func rawBytesAccountMismatch() async throws {
        let account = SsoTestData.makeAccount()

        let rawPayload = PolkadotHostRemoteMessage.SigningRawPayload(
            account: account,
            type: .bytes(Data([0x01]))
        )
        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.rawPayload(rawPayload)

        let factory = SsoTestData.makeFactory()
        let context = SsoTestData.makeFailingSigningContext(
            signingRequest: signingRequest,
            error: PolkadotSigningError.accountMismatch
        )

        await #expect(throws: PolkadotSigningError.accountMismatch) {
            try await factory.makeParsedResult(signingContext: context)
        }
    }

    @Test("Raw bytes already wrapped payload is not double-wrapped")
    func rawBytesAlreadyWrapped() async throws {
        let wallet = SsoTestData.makeWallet()
        let account = SsoTestData.makeAccount()

        let rawPayload = PolkadotHostRemoteMessage.SigningRawPayload(
            account: account,
            type: .payload("<Bytes>already wrapped</Bytes>")
        )
        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.rawPayload(rawPayload)

        let factory = SsoTestData.makeFactory()
        let context = SsoTestData.makeSigningContext(wallet: wallet, signingRequest: signingRequest)
        let result = try await factory.makeParsedResult(signingContext: context)

        if case let .rawBytes(data) = result.parsedRequest {
            let string = String(data: data, encoding: .utf8)
            #expect(string == "<Bytes>already wrapped</Bytes>")
        } else {
            Issue.record("Expected .rawBytes")
        }
    }
}
