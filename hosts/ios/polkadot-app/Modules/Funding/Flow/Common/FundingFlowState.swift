import Foundation
import SubstrateSdk
import TrUAPIHost
import UIKit

/// One funding flow's state, shared by its screens: what the user picked
/// on them, the providers' brands read so far, the selection the core was
/// last told of, and what the app does when the flow closes or leaves for
/// another sheet. Each screen's presenter reads and changes it, and hears
/// when another screen did.
@MainActor
final class FundingFlowState {
    let entryId: String
    let direction: FundingDirection
    let cashInfo: AssetBalanceDisplayInfo
    let branding: FundingProviderBranding
    private let iconLoader: FundingIconLoader?

    /// The flow's sheet went away.
    var onClose: (() -> Void)?
    /// Shows the history of every session over the flow.
    var onHistory: (() -> Void)?
    /// Leaves the flow for a new session the same way.
    var onStartOver: ((FundingDirection) -> Void)?

    private(set) var draft: FundingDraft {
        didSet { if draft != oldValue { notify() } }
    }

    private(set) var brands: [ProductId: FundingProviderBrand] = [:]
    private(set) var icons: [FundingIconKey: UIImage] = [:]
    /// The session as the core last pushed it to any of the flow's screens.
    private(set) var latestEntry: FundingEntry

    private var reportedSelection: FundingSelection?
    private var brandReads: [ProductId: Task<Void, Never>] = [:]
    private var iconReads: Set<FundingIconKey> = []
    private var listeners: [ObjectIdentifier: () -> Void] = [:]

    init(
        entry: FundingEntry,
        cashInfo: AssetBalanceDisplayInfo,
        branding: FundingProviderBranding,
        iconLoader: FundingIconLoader?
    ) {
        entryId = entry.id
        direction = entry.direction
        latestEntry = entry
        self.cashInfo = cashInfo
        self.branding = branding
        self.iconLoader = iconLoader

        let detected = (entry.options?.detectedCountry ?? FundingCountries.localeRegion())
            .flatMap { FundingCountries.country(code: $0) }
        draft = FundingDraft(
            rail: entry.session?.choice?.rail ?? entry.options?.rails.first ?? .card,
            amountText: entry.amount.map { Self.text(for: $0.decimal(assetInfo: cashInfo)) } ?? "",
            country: detected,
            detectedCountry: detected,
            network: entry.session?.choice?.network,
            asset: entry.session?.choice?.asset,
            chosenProviderId: entry.session?.providerId
        )
        reportedSelection = entry.hasStarted ? draft.selection : nil
    }

    /// What a screen draws for `entry`, with this flow's draft.
    func model(for entry: FundingEntry) -> FundingEntryModel {
        FundingEntryModel(
            entry: entry,
            draft: draft,
            cashInfo: cashInfo,
            branding: branding,
            brands: brands,
            icons: icons
        )
    }

    /// Calls `onChange` whenever the draft or a brand changes, until `owner`
    /// is gone.
    func observe(_ owner: AnyObject, onChange: @escaping () -> Void) {
        let id = ObjectIdentifier(owner)
        listeners[id] = { [weak owner] in
            if owner != nil { onChange() }
        }
    }

    func update(_ change: (inout FundingDraft) -> Void) {
        change(&draft)
        loadIcons()
    }

    /// Takes what the core offers into the draft: its detected country leads
    /// until the user picks one, and the candidates' brands are read.
    func absorb(_ entry: FundingEntry) {
        latestEntry = entry
        loadBrands(entry.options?.candidates.map(\.providerId) ?? [])
        loadBrands([entry.session?.providerId].compactMap(\.self))
        loadIcons()

        guard let code = entry.options?.detectedCountry, code != draft.detectedCountry?.code,
              let detected = FundingCountries.country(code: code)
        else { return }

        update { draft in
            draft.detectedCountry = detected
            if !draft.hasPickedCountry { draft.country = detected }
        }
    }

    /// The selection to tell the core of, when the draft changed it since it
    /// was last told and the session has not started.
    func selectionToReport(hasStarted: Bool) -> FundingSelection? {
        let selection = draft.selection
        guard !hasStarted, selection != reportedSelection else { return nil }

        reportedSelection = selection
        return selection
    }

    /// Returns once every brand read has finished.
    func brandsSettled() async {
        for read in brandReads.values {
            await read.value
        }
    }
}

private extension FundingFlowState {
    /// `value` as the keypad types it: at most two decimals, rounded down.
    static func text(for value: Decimal) -> String {
        var source = value
        var rounded = Decimal()
        NSDecimalRound(&rounded, &source, 2, .down)
        return NSDecimalNumber(decimal: rounded).stringValue
    }

    func loadBrands(_ providerIds: [ProductId]) {
        for providerId in providerIds where brands[providerId] == nil && brandReads[providerId] == nil {
            brandReads[providerId] = Task { [branding] in
                let brand = await branding.brand(for: providerId)
                brands[providerId] = brand
                brandReads[providerId] = nil
                notify()
            }
        }
    }

    /// Reads the icons the marks draw now, each from the provider it is
    /// drawn for; a miss keeps the monogram.
    func loadIcons() {
        guard let iconLoader else { return }

        for key in model(for: latestEntry).iconKeys where icons[key] == nil && !iconReads.contains(key) {
            iconReads.insert(key)
            Task {
                guard let icon = await iconLoader.image(key.kind, id: key.id, provider: key.provider) else { return }
                icons[key] = icon
                notify()
            }
        }
    }

    func notify() {
        for listener in listeners.values {
            listener()
        }
    }
}
