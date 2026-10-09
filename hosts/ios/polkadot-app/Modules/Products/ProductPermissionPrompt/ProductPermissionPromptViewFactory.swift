import Foundation_iOS
import Products
import UIKit
import PolkadotUI
import UIKitExt

@MainActor
enum ProductPermissionPromptViewFactory {
    static func createView(context: ProductPermissionContext) -> ControllerBackedProtocol {
        let viewModel = makeViewModel(for: context)
        let styler = ProductPromptStyler()

        let view = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: styler,
            allowsSwipeDown: false
        )

        BottomSheetViewFacade.setupBottomSheet(from: view.controller)

        return view
    }
}

// MARK: - Prompt Content

private struct PromptContent {
    let title: String
    let body: String
    let icon: UIImage?
}

// MARK: - ViewModel Building

private extension ProductPermissionPromptViewFactory {
    static func makeViewModel(
        for context: ProductPermissionContext
    ) -> TitleDetailsSheetViewModel {
        let content: PromptContent =
            if context.permissions.count == 1, let permission = context.permissions.first {
                makeSingleContent(
                    productId: context.productId,
                    permission: permission
                )
            } else {
                makeBatchedContent(
                    productId: context.productId,
                    permissions: context.permissions
                )
            }

        let manageHint = String(localized: .Products.permissionBodyManageInSettingsHint)
        let bodyWithHint = "\(content.body) \(manageHint)"

        return TitleDetailsSheetViewModel(
            graphics: content.icon,
            title: LocalizableResource { _ in content.title },
            message: LocalizableResource { _ in .normal(bodyWithHint) },
            mainAction: makeAction(
                title: String(localized: .Products.permissionActionAllowAlways)
            ) { context.deliver(.allowAlways) },
            secondaryAction: makeAction(
                title: String(localized: .Products.permissionActionAllowOnce)
            ) { context.deliver(.allowOnce) },
            tertiaryAction: makeAction(
                title: String(localized: .Products.permissionActionDeny)
            ) { context.deliver(.deny) }
        )
    }

    static func makeSingleContent(
        productId: String,
        permission: ProductPermission
    ) -> PromptContent {
        let title: String
        let body: String
        switch permission {
        case let .deviceCapability(capability):
            title = String(
                localized: .Products.permissionTitleDeviceCapability(
                    productId: productId,
                    capability: capabilityDisplayName(capability)
                )
            )
        case let .networkAccess(domain):
            PromptContent(
                title: String(
                    localized: .Products.permissionTitleNetworkAccess(
                        productId: productId
                    )
                ),
                body: String(
                    localized: .Products.permissionBodyNetworkAccess(domain: domain)
                ),
                icon: makeIcon(systemName: "globe")
            )
        case let .networkAccessBundle(domains):
            makeSingleContent(productId: productId, permission: .networkAccess(domain: domains.joined(separator: ", ")))
        case let .accountAccess(targetProductId):
            PromptContent(
                title: String(
                    localized: .Products.permissionTitleAccountAccess(
                        productId: productId
                    )
                ),
                body: String(
                    localized: .Products.permissionBodyAccountAccess(
                        targetProductId: targetProductId
                    )
                ),
                icon: makeIcon(systemName: "person.crop.circle")
            )
        case .balanceAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyBalanceAccess),
                icon: makeIcon(systemName: "dollarsign.circle.fill")
            )
        case .webRtcAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyWebRtc),
                icon: makeIcon(systemName: "video.fill")
            )
        case .chainSubmitAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyChainSubmit),
                icon: makeIcon(systemName: "link")
            )
        case .preimageSubmitAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyPreimageSubmit),
                icon: makeIcon(systemName: "doc.text")
            )
        case .statementSubmitAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyStatementSubmit),
                icon: makeIcon(systemName: "text.bubble")
            )
        case let .jamPeersAccess(genesis):
            PromptContent(
                title: String(
                    localized: .Products.permissionTitleJamPeers(
                        productId: productId,
                        shortGenesis: ProductPermission.shortGenesis(genesis)
                    )
                ),
                body: String(localized: .Products.permissionBodyJamPeers) + "\n\n" + genesis,
                icon: makeIcon(systemName: "point.3.connected.trianglepath.dotted")
            )
        case .userIdentityAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyUserIdentityAccess),
                icon: makeIcon(systemName: "person.text.rectangle")
            )
            body = String(localized: .Products.permissionBodyJamPeers)
        case .chatAuthority:
            PromptContent(
                title: String(localized: .Products.permissionTitleChatAuthority(productId: productId)),
                body: String(localized: .Products.permissionBodyChatAuthority),
                icon: makeIcon(systemName: "message.badge.shield")
            )
        case let .statementStoreAllowance(derivationIndex):
            PromptContent(
                title: String(localized: .Products.permissionTitleStatementStoreAllowance(productId: productId)),
                body: ProductPermission.statementStoreAllowanceDescription(derivationIndex: derivationIndex),
                icon: makeIcon(systemName: "externaldrive.badge.plus")
            )
        }
        return PromptContent(title: title, body: body, icon: makeIcon(systemName: permission.permissionIconSystemName))
    }

    static func makeBatchedContent(
        productId: String,
        permissions: [ProductPermission]
    ) -> PromptContent {
        let descriptions = permissions.map { permissionDescription(for: $0) }
        let body = descriptions.joined(separator: "\n")
        return PromptContent(
            title: String(localized: .Products.permissionTitleRemote(productId: productId)),
            body: body,
            icon: makeIcon(systemName: "shield.lefthalf.filled")
        )
    }

    static func permissionDescription(for permission: ProductPermission) -> String {
        switch permission {
        case let .networkAccess(domain):
            "- " + String(
                localized: .Products.permissionBodyNetworkAccess(domain: domain)
            )
        case let .networkAccessBundle(domains):
            permissionDescription(for: .networkAccess(domain: domains.joined(separator: ", ")))
        case .balanceAccess:
            "- " + String(localized: .Products.permissionBodyBalanceAccess)
        case .webRtcAccess:
            "- " + String(localized: .Products.permissionBodyWebRtc)
        case .chainSubmitAccess:
            "- " + String(localized: .Products.permissionBodyChainSubmit)
        case .preimageSubmitAccess:
            "- " + String(localized: .Products.permissionBodyPreimageSubmit)
        case .statementSubmitAccess:
            "- " + String(localized: .Products.permissionBodyStatementSubmit)
        case let .jamPeersAccess(genesis):
            "- " + String(
                localized: .Products.permissionLabelJamPeers(
                    genesis: genesis
                )
            )
        case let .deviceCapability(capability):
            "- " + capabilityDescription(capability)
        case let .accountAccess(targetProductId):
            "- " + String(
                localized: .Products.permissionBodyAccountAccess(
                    targetProductId: targetProductId
                )
            )
        case .userIdentityAccess:
            "- " + String(localized: .Products.permissionBodyUserIdentityAccess)
        case .chatAuthority:
            "- " + String(localized: .Products.permissionBodyChatAuthority)
        case let .statementStoreAllowance(derivationIndex):
            "- " + ProductPermission.statementStoreAllowanceDescription(derivationIndex: derivationIndex)
        }
    }

    static func makeAction(
        title: String,
        handler: @escaping () -> Void
    ) -> MessageSheetAction {
        MessageSheetAction(
            title: LocalizableResource { _ in title },
            handler: handler
        )
    }

    static func capabilityDisplayName(_ capability: DeviceCapabilityType) -> String {
        switch capability {
        case .notifications: String(localized: .Products.permissionCapabilityNotifications)
        case .camera: String(localized: .Products.permissionCapabilityCamera)
        case .microphone: String(localized: .Products.permissionCapabilityMicrophone)
        case .bluetooth: String(localized: .Products.permissionCapabilityBluetooth)
        case .nfc: String(localized: .Products.permissionCapabilityNfc)
        case .location: String(localized: .Products.permissionCapabilityLocation)
        case .clipboard: String(localized: .Products.permissionCapabilityClipboard)
        case .openUrl: String(localized: .Products.permissionCapabilityOpenUrl)
        case .biometrics: String(localized: .Products.permissionCapabilityBiometrics)
        case .motion: String(localized: .Products.permissionCapabilityMotion)
        }
    }

    static func makeIcon(systemName: String) -> UIImage? {
        let config = UIImage.SymbolConfiguration(pointSize: 60, weight: .regular)
        return UIImage(systemName: systemName, withConfiguration: config)?
            .withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
    }
}
