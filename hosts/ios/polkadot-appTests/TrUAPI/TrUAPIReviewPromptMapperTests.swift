import Foundation
import Testing
import KeyDerivation
import Products
import SubstrateSdk
import TrUAPIHost
@testable import polkadot_app

struct TrUAPIReviewPromptMapperTests {
    private let callerId = try! ProductId("caller.dot")
    private let otherId = try! ProductId("other.dot")
    let mapper = TrUAPIReviewPromptMapper()

    @Test
    func mapsIdentityDisclosureToUserIdentityPermission() {
        let request = mapper.makePermissionRequest(
            from: IdentityDisclosureReview(productId: callerId)
        )

        #expect(request == TrUAPIPermissionRequest(
            productId: callerId,
            permissions: [.userIdentityAccess]
        ))
    }

    @Test
    func mapsAccountAccessToTargetedPermission() {
        let request = mapper.makePermissionRequest(from: AccountAccessReview(
            requestingProductId: callerId,
            targetProductId: try! ProductId("target.dot")
        ))

        #expect(request == TrUAPIPermissionRequest(
            productId: callerId,
            permissions: [.accountAccess(targetProductId: try! ProductId("target.dot"))]
        ))
    }

    @Test
    func mapsProductSubtreeToAccountRetrievalAction() {
        let request = mapper.makeActionRequest(
            from: ProductSubtreeReview(productId: callerId)
        )

        #expect(request == .productSubtree(productId: callerId))
    }

    @Test
    func mapsNftPurseDrainOfARootPurseToAnExternalKey() {
        let request = mapper.makeActionRequest(from: NftPurseDrainReview(
            callingProductId: callerId,
            fromPurse: 4,
            owner: .root,
            itemCount: 1,
            destination: .externalKey(key: Data(repeating: 0xAB, count: 32))
        ))

        #expect(request == .nftPurseDrain(
            productId: callerId,
            fromPurse: 4,
            ownerProductId: nil,
            itemCount: 1,
            destination: .key(hex: String(repeating: "ab", count: 32))
        ))
    }

    @Test
    func mapsNftPurseDrainIntoACheque() {
        let request = mapper.makeActionRequest(from: NftPurseDrainReview(
            callingProductId: callerId,
            fromPurse: 4,
            owner: .product(otherId),
            itemCount: 2,
            destination: .cheque
        ))

        #expect(request == .nftPurseDrain(
            productId: callerId,
            fromPurse: 4,
            ownerProductId: otherId,
            itemCount: 2,
            destination: .cheque
        ))
    }

    @Test
    func mapsNftPurseDrainBackToAChequeSender() {
        let request = mapper.makeActionRequest(from: NftPurseDrainReview(
            callingProductId: callerId,
            fromPurse: 4,
            owner: .product(otherId),
            itemCount: 2,
            destination: .refund
        ))

        #expect(request == .nftPurseDrain(
            productId: callerId,
            fromPurse: 4,
            ownerProductId: otherId,
            itemCount: 2,
            destination: .refund
        ))
    }

    @Test
    func mapsAccountAliasToContextProductAccountAccess() {
        let request = mapper.makePermissionRequest(from: AccountAliasReview(
            callingProductId: callerId,
            context: ProductProofContext(productId: try! ProductId("ring-owner.dot"), suffix: .index(3)),
            ringLocation: Self.makeRingLocation()
        ))

        #expect(request == TrUAPIPermissionRequest(
            productId: callerId,
            permissions: [.accountAccess(targetProductId: try! ProductId("ring-owner.dot"))]
        ))
    }

    @Test
    func mapsCreateProofRequest() throws {
        let message = try Data.randomOrError(of: 24)

        let request = try mapper.makeCreateProofRequest(from: CreateProofReview(
            callingProductId: callerId,
            context: ProductProofContext(productId: try! ProductId("ring-owner.dot"), suffix: .index(9)),
            ringLocation: Self.makeRingLocation(),
            message: message
        ))

        #expect(request.callingProductId?.rawValue == "caller.dot")
        #expect(request.onBehalfOfProductId.rawValue == "ring-owner.dot")
        #expect(request.suffix == DerivationIndex32(index: 9).bytes)
        #expect(request.message == message)
    }

    @Test
    func mapsCreateProofRawSuffix() throws {
        let rawSuffix = try Data.randomOrError(of: 32)

        let request = try mapper.makeCreateProofRequest(from: CreateProofReview(
            callingProductId: callerId,
            context: ProductProofContext(productId: try! ProductId("ring-owner.dot"), suffix: .raw(rawSuffix)),
            ringLocation: Self.makeRingLocation(),
            message: Data()
        ))

        #expect(request.suffix == rawSuffix)
    }

    @Test
    func mapsAllowanceResources() throws {
        let request = try mapper.makeAllowanceRequest(from: ResourceAllocationReview(
            callingProductId: callerId,
            resources: [
                .statementStoreAllowance,
                .bulletinAllowance,
                .smartContractAllowance(.index(4)),
                .autoSigning
            ]
        ))

        #expect(request == TrUAPIAllowanceRequest(
            productId: callerId,
            resources: [
                .statementStoreAllowance,
                .bulletInAllowance,
                .smartContractAllowance(dest: .index(4)),
                .autoSigning
            ]
        ))
    }

    @Test
    func mapsSignVrfRequest() throws {
        let label = try Data.randomOrError(of: 8)
        let itemLabel = try Data.randomOrError(of: 4)
        let itemValue = try Data.randomOrError(of: 4)

        let request = try mapper.makeSignVrfRequest(from: SignVrfReview(
            callingProductId: callerId,
            request: TrUAPIHostSignVrfRequest(
                account: TrUAPIHostProductAccountId(
                    dotNsIdentifier: try! ProductId("signer.dot"),
                    derivationIndex: .index(2)
                ),
                transcriptLabel: label,
                items: [TrUAPIHostVrfTranscriptItem(label: itemLabel, value: itemValue)]
            )
        ))

        #expect(request.callingProductId?.rawValue == "caller.dot")
        #expect(request.payload == SignVrfPayload(
            account: Products.ProductAccountId(productId: try! ProductId("signer.dot"), derivationIndex: .index(2)),
            transcriptLabel: label,
            items: [KeyDerivation.VrfTranscriptItem(label: itemLabel, value: itemValue)]
        ))
    }

    @Test
    func mapsStatementSignRequestFromSigningAccount() throws {
        let payload = try Data.randomOrError(of: 48)

        let request = mapper.makeStatementSignRequest(from: StatementStoreProductSignReview(
            callingProductId: nil,
            account: TrUAPIHostProductAccountId(
                dotNsIdentifier: try! ProductId("signer.dot"),
                derivationIndex: .index(0)
            ),
            payload: payload
        ))

        #expect(request == StatementSignConfirmationRequest(
            productId: try! ProductId("signer.dot"),
            payload: payload
        ))
    }
}

private extension TrUAPIReviewPromptMapperTests {
    static func makeRingLocation() -> TrUAPIHostRingLocation {
        TrUAPIHostRingLocation(chainId: Data(repeating: 0, count: 32), junctions: [])
    }
}
