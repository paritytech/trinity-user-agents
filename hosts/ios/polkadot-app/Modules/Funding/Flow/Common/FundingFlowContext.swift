import Foundation
import TrUAPIHost

/// One funding flow's shared draft: what the user picked on its screens,
/// the providers' brands read so far, and the selection the core was last
/// told of. Each screen's presenter reads and changes it, and hears when
/// another screen did.
@MainActor
final class FundingFlowContext {
    let entryId: String
    let direction: FundingDirection
    let cash: FundingCash
    let branding: FundingProviderBranding

    private(set) var draft: FundingDraft {
        didSet { if draft != oldValue { notify() } }
    }

    private(set) var brands: [ProductId: FundingProviderBrand] = [:]
    /// The session as the core last pushed it to any of the flow's screens.
    private(set) var latestEntry: FundingEntry

    private var reportedSelection: FundingSelection?
    private var brandReads: [ProductId: Task<Void, Never>] = [:]
    private var listeners: [ObjectIdentifier: () -> Void] = [:]

    init(entry: FundingEntry, cash: FundingCash, branding: FundingProviderBranding) {
        entryId = entry.id
        direction = entry.direction
        latestEntry = entry
        self.cash = cash
        self.branding = branding

        let detected = (entry.options?.detectedCountry ?? FundingCountries.localeRegion())
            .flatMap { FundingCountries.country(code: $0) }
        draft = FundingDraft(
            rail: entry.session?.choice?.rail ?? entry.options?.rails.first ?? .card,
            amountText: entry.amount.flatMap { cash.decimal($0) }.map(Self.text(for:)) ?? "",
            country: detected,
            detectedCountry: detected,
            network: entry.session?.choice?.network,
            asset: entry.session?.choice?.asset,
            chosenProviderId: entry.session?.providerId
        )
        reportedSelection = entry.hasStarted ? draft.selection : nil
    }

    /// The state a screen draws for `entry`, with this flow's draft.
    func state(for entry: FundingEntry) -> FundingFlowState {
        FundingFlowState(entry: entry, draft: draft, cash: cash, branding: branding, brands: brands)
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
    }

    /// Takes what the core offers into the draft: its detected country leads
    /// until the user picks one, and the candidates' brands are read.
    func absorb(_ entry: FundingEntry) {
        latestEntry = entry
        loadBrands(entry.options?.candidates.map(\.providerId) ?? [])
        loadBrands([entry.session?.providerId].compactMap(\.self))

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

private extension FundingFlowContext {
    static func text(for value: Decimal) -> String {
        NSDecimalNumber(decimal: value.fundingRounded(scale: 2, mode: .down)).stringValue
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

    func notify() {
        for listener in listeners.values {
            listener()
        }
    }
}
