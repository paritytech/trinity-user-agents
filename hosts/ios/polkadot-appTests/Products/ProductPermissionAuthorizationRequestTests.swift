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
        (.userIdentityAccess, .identityDisclosure)
    ])
    func namesTheCoreDecisionBehindAStoredPermission(
        permission: ProductPermission,
        request: PermissionAuthorizationRequest
    ) {
        #expect(PermissionAuthorizationRequest(permission) == request)
    }
}
