import Foundation
import Testing
import KeyDerivation
import Products
import SubstrateSdk
import TrUAPIHost
@testable import polkadot_app

struct TrUAPIReviewPromptMapperTests {
    let mapper = TrUAPIReviewPromptMapper()

    @Test
    func mapsIdentityDisclosureToUserIdentityPermission() {
        let request = mapper.makePermissionRequest(
            from: IdentityDisclosureReview(productId: "caller.dot")
        )

        #expect(request == TrUAPIPermissionRequest(
            productId: "caller.dot",
            permissions: [.userIdentityAccess]
        ))
    }

    @Test
    func mapsPreimageSubmitToActionWithRequesterAndSize() {
        let request = mapper.makeActionRequest(
            from: PreimageSubmitReview(size: 1_024),
            requester: "caller.dot"
        )

        #expect(request == .preimageSubmit(productId: "caller.dot", size: 1_024))
    }

    @Test
    func mapsAccountAccessToTargetedPermission() {
        let request = mapper.makePermissionRequest(from: AccountAccessReview(
            requestingProductId: "caller.dot",
            targetProductId: "target.dot"
        ))

        #expect(request == TrUAPIPermissionRequest(
            productId: "caller.dot",
            permissions: [.accountAccess(targetProductId: "target.dot")]
        ))
    }

    @Test
    func mapsProductSubtreeToAccountRetrievalAction() {
        let request = mapper.makeActionRequest(
            from: ProductSubtreeReview(productId: "caller.dot")
        )

        #expect(request == .productSubtree(productId: "caller.dot"))
    }

    @Test
    func mapsAccountAliasToContextProductAccountAccess() {
        let request = mapper.makePermissionRequest(from: AccountAliasReview(
            callingProductId: "caller.dot",
            context: ProductProofContext(productId: "ring-owner.dot", suffix: .index(3)),
            ringLocation: Self.makeRingLocation()
        ))

        #expect(request == TrUAPIPermissionRequest(
            productId: "caller.dot",
            permissions: [.accountAccess(targetProductId: "ring-owner.dot")]
        ))
    }

    @Test
    func mapsCreateProofRequest() throws {
        let message = try Data.randomOrError(of: 24)

        let request = try mapper.makeCreateProofRequest(from: CreateProofReview(
            callingProductId: "caller.dot",
            context: ProductProofContext(productId: "ring-owner.dot", suffix: .index(9)),
            ringLocation: Self.makeRingLocation(),
            message: message
        ))

        #expect(request.callingProductId == "caller.dot")
        #expect(request.onBehalfOfProductId == "ring-owner.dot")
        #expect(request.suffix == DerivationIndex32(index: 9).bytes)
        #expect(request.message == message)
    }

    @Test
    func mapsCreateProofRawSuffix() throws {
        let rawSuffix = try Data.randomOrError(of: 32)

        let request = try mapper.makeCreateProofRequest(from: CreateProofReview(
            callingProductId: "caller.dot",
            context: ProductProofContext(productId: "ring-owner.dot", suffix: .raw(rawSuffix)),
            ringLocation: Self.makeRingLocation(),
            message: Data()
        ))

        #expect(request.suffix == rawSuffix)
    }

    @Test
    func mapsAllowanceResources() throws {
        let request = try mapper.makeAllowanceRequest(from: ResourceAllocationReview(
            callingProductId: "caller.dot",
            resources: [
                .statementStoreAllowance,
                .bulletinAllowance,
                .smartContractAllowance(.index(4)),
                .autoSigning
            ]
        ))

        #expect(request == TrUAPIAllowanceRequest(
            productId: "caller.dot",
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
            callingProductId: "caller.dot",
            request: TrUAPIHostSignVrfRequest(
                account: TrUAPIHostProductAccountId(
                    dotNsIdentifier: "signer.dot",
                    derivationIndex: .index(2)
                ),
                transcriptLabel: label,
                items: [TrUAPIHostVrfTranscriptItem(label: itemLabel, value: itemValue)]
            )
        ))

        #expect(request.callingProductId == "caller.dot")
        #expect(request.payload == SignVrfPayload(
            account: Products.ProductAccountId(productId: "signer.dot", derivationIndex: .index(2)),
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
                dotNsIdentifier: "signer.dot",
                derivationIndex: .index(0)
            ),
            payload: payload
        ))

        #expect(request == StatementSignConfirmationRequest(
            productId: "signer.dot",
            payload: payload
        ))
    }
}

private extension TrUAPIReviewPromptMapperTests {
    static func makeRingLocation() -> TrUAPIHostRingLocation {
        TrUAPIHostRingLocation(chainId: Data(repeating: 0, count: 32), junctions: [])
    }
}
