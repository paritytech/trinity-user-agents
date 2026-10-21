import Foundation
import Testing
import SubstrateSdk
import TrUAPIHost
@testable import polkadot_app

struct ProductsSignConfirmModelFactoryTests {
    private let requester = PolkadotSigningRequester(name: "test.product", iconUrl: nil)

    private func makeFactory() -> ProductsSignConfirmModelFactory {
        ProductsSignConfirmModelFactory(chainRegistry: MockChainRegistry())
    }

    // MARK: - Transaction cases fall back to raw when the chain is unavailable

    @Test func signPayloadProductFallsBackToRawCall() async throws {
        let method = try Data.randomOrError(of: 12)
        let account = TrUAPIHostProductAccountId(dotNsIdentifier: "p.dot", derivationIndex: .index(0))
        let input = ProductsSignConfirmInput.signPayload(
            .product(
                callingProductId: "test.product",
                request: HostSignPayloadRequest(account: account, payload: Self.makeHostSignPayloadData(method: method))
            )
        )

        let model = try await makeFactory().makeModel(from: input, requester: requester)

        #expect(model.isTransaction)
        #expect(model.descriptionText == "Raw call")
        #expect(!model.detailsText.isEmpty)
        #expect(model.requester.name == "test.product")
    }

    @Test func signPayloadLegacyFallsBackToRawCall() async throws {
        let method = try Data.randomOrError(of: 12)
        let input = ProductsSignConfirmInput.signPayload(
            .legacyAccount(HostSignPayloadWithLegacyAccountRequest(
                signer: "5Fff",
                payload: Self.makeHostSignPayloadData(method: method)
            ))
        )

        let model = try await makeFactory().makeModel(from: input, requester: requester)

        #expect(model.isTransaction)
        #expect(model.descriptionText == "Raw call")
    }

    @Test func createTransactionProductFallsBackToRawCall() async throws {
        let signer = TrUAPIHostProductAccountId(dotNsIdentifier: "p.dot", derivationIndex: .index(1))
        let input = try ProductsSignConfirmInput.createTransaction(.product(
            callingProductId: "test.product",
            payload: ProductAccountTxPayload(
                signer: signer,
                genesisHash: Data.randomOrError(of: 32),
                callData: Data.randomOrError(of: 20),
                extensions: [],
                txExtVersion: 0,
                contacts: []
            )
        ))

        let model = try await makeFactory().makeModel(from: input, requester: requester)

        #expect(model.isTransaction)
        #expect(model.descriptionText == "Raw call")
    }

    @Test func createTransactionLegacyFallsBackToRawCall() async throws {
        let input = try ProductsSignConfirmInput.createTransaction(.legacyAccount(
            LegacyAccountTxPayload(
                signer: Data.randomOrError(of: 32),
                genesisHash: Data.randomOrError(of: 32),
                callData: Data.randomOrError(of: 10),
                extensions: [],
                txExtVersion: 5
            )
        ))

        let model = try await makeFactory().makeModel(from: input, requester: requester)

        #expect(model.isTransaction)
        #expect(model.descriptionText == "Raw call")
    }

    // MARK: - Raw signing wraps the payload and is not a transaction

    @Test func signRawBytesWrapsForDisplay() async throws {
        let bytes = try Data.randomOrError(of: 16)
        let account = TrUAPIHostProductAccountId(dotNsIdentifier: "p.dot", derivationIndex: .index(0))
        let input = ProductsSignConfirmInput.signRaw(
            .product(
                callingProductId: "test.product",
                request: HostSignRawRequest(account: account, payload: .bytes(bytes: bytes)),
                watermarked: true
            )
        )

        let model = try await makeFactory().makeModel(from: input, requester: requester)

        #expect(!model.isTransaction)
        #expect(model.descriptionText == "Raw bytes")
        #expect(model.detailsText == Self.wrapped(bytes).toHex(includePrefix: true))
    }

    @Test func signRawStringPayloadWrapsForDisplay() async throws {
        let input = ProductsSignConfirmInput.signRaw(
            .legacyAccount(
                request: HostSignRawWithLegacyAccountRequest(signer: "5Fff", payload: .payload(payload: "hello")),
                watermarked: true
            )
        )

        let model = try await makeFactory().makeModel(from: input, requester: requester)

        #expect(!model.isTransaction)
        #expect(model.detailsText == Self.wrapped(Data("hello".utf8)).toHex(includePrefix: true))
    }

    @Test(arguments: [false, true]) func signRawInvalidHexPayloadThrows(watermarked: Bool) async {
        let input = ProductsSignConfirmInput.signRaw(
            .legacyAccount(
                request: HostSignRawWithLegacyAccountRequest(signer: "5Fff", payload: .payload(payload: "0xZZ")),
                watermarked: watermarked
            )
        )

        await #expect(throws: (any Error).self) {
            _ = try await makeFactory().makeModel(from: input, requester: requester)
        }
    }

    @Test(arguments: [false, true]) func unwatermarkedSigningWarnsAndDisplaysExactBytes(legacy: Bool) async throws {
        let bytes = Data(repeating: 0x11, count: 32)
        let review: SignRawReview = legacy
            ? .legacyAccount(
                request: HostSignRawWithLegacyAccountRequest(
                    signer: "5Fff",
                    payload: .payload(payload: bytes.toHex(includePrefix: true))
                ),
                watermarked: false
            )
            : .product(
                callingProductId: "test.product",
                request: HostSignRawRequest(
                    account: TrUAPIHostProductAccountId(dotNsIdentifier: "p.dot", derivationIndex: .index(0)),
                    payload: .bytes(bytes: bytes)
                ),
                watermarked: false
            )

        let model = try await makeFactory().makeModel(from: .signRaw(review), requester: requester)

        #expect(model.isTransaction)
        #expect(model.descriptionText == "Unprotected signature: may authorize transactions")
        #expect(model.detailsText == bytes.toHex(includePrefix: true))
    }
}

private extension ProductsSignConfirmModelFactoryTests {
    static func makeHostSignPayloadData(method: Data) -> HostSignPayloadData {
        HostSignPayloadData(
            blockHash: Data(repeating: 0x01, count: 32),
            blockNumber: Data([0x01, 0x00, 0x00, 0x00]),
            era: Data([0x00]),
            genesisHash: Data(repeating: 0x02, count: 32),
            method: method,
            nonce: Data([0x05, 0x00, 0x00, 0x00]),
            specVersion: Data([0x0A, 0x00, 0x00, 0x00]),
            tip: Data([0x00]),
            transactionVersion: Data([0x02, 0x00, 0x00, 0x00]),
            signedExtensions: ["CheckNonce"],
            version: 4,
            assetId: nil,
            metadataHash: nil,
            mode: nil,
            withSignedTransaction: nil
        )
    }

    static func wrapped(_ message: Data) -> Data {
        Data("<Bytes>".utf8) + message + Data("</Bytes>".utf8)
    }
}
