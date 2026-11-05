import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct ProductPermissionAuthorizationRequestTests {
    @Test(arguments: [
        (ProductPermission.deviceCapability(.camera), PermissionAuthorizationRequest.device(.camera)),
        (.deviceCapability(.motion), .device(.motion)),
        (.networkAccess(domain: "api.example.com"), .remote(.init(permission: .remote(domains: ["api.example.com"])))),
        (.accountAccess(targetProductId: "stash.paseo"), .accountAccess(targetProductId: "stash.paseo")),
        (.balanceAccess, .remote(.init(permission: .balanceAccess))),
        (.webRtcAccess, .remote(.init(permission: .webRtc))),
        (.chainSubmitAccess, .remote(.init(permission: .chainSubmit))),
        (.preimageSubmitAccess, .remote(.init(permission: .preimageSubmit))),
        (.statementSubmitAccess, .remote(.init(permission: .statementSubmit))),
        (.userIdentityAccess, .identityDisclosure),
        (
            .nftPurseRead(ownerProductId: "stash.paseo"),
            .remote(.init(permission: .nftPurseRead(owner: .product("stash.paseo"))))
        ),
        (.nftPurseRead(ownerProductId: nil), .remote(.init(permission: .nftPurseRead(owner: .root))))
    ])
    func namesTheCoreDecisionBehindAStoredPermission(
        permission: ProductPermission,
        request: PermissionAuthorizationRequest
    ) {
        #expect(PermissionAuthorizationRequest(permission) == request)
        #expect(request.productPermissions == [permission])
    }

    @Test
    func aDomainBundleStandsForEachOfItsDomains() {
        let request = PermissionAuthorizationRequest.remote(.init(permission: .remote(domains: [
            "a.example",
            "b.example"
        ])))

        #expect(request.productPermissions == [
            .networkAccess(domain: "a.example"),
            .networkAccess(domain: "b.example")
        ])
    }

    @Test
    func aSignatureApprovalIsNotASettingsPermission() {
        let request = PermissionAuthorizationRequest.crossProductSignature(
            targetProductId: "stash.paseo",
            kind: .payload
        )

        #expect(request.productPermissions.isEmpty)
    }
}
