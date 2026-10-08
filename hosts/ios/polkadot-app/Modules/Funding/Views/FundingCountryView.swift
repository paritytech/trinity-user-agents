import DesignSystem
import SwiftUI

/// Where the user pays from, which the quote is asked for. Countries every
/// provider on this rail refuses are listed apart and cannot be picked.
struct FundingCountryView: View {
    let model: FundingFlowModel

    @State private var query = ""
    private let countries = FundingCountries.all()
    private let detected = FundingCountries.detected()

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: String(localized: .Funding.countryTitle), leadingTitle: true, onBack: model.back)
            searchField

            ScrollView {
                LazyVStack(alignment: .leading, spacing: DSSpacings.extraSmall) {
                    if query.isEmpty, let detected {
                        sectionTitle(String(localized: .Funding.countryDetected))
                        row(detected)
                        sectionTitle(String(localized: .Funding.countryOther))
                    }
                    ForEach(supported) { row($0) }

                    if !unsupported.isEmpty {
                        sectionTitle(String(localized: .Funding.countryUnsupported))
                        ForEach(unsupported) { row($0) }
                    }
                }
            }
            .scrollDismissesKeyboard(.immediately)
        }
    }
}

private extension FundingCountryView {
    var matching: [FundingCountry] {
        guard !query.isEmpty else { return countries }
        return countries.filter { country in
            country.name.localizedCaseInsensitiveContains(query)
                || (country.currencyName?.localizedCaseInsensitiveContains(query) ?? false)
                || (country.currencyCode?.localizedCaseInsensitiveContains(query) ?? false)
        }
    }

    var supported: [FundingCountry] {
        matching.filter { !model.unsupportedCountries.contains($0.code) && (!query.isEmpty || $0 != detected) }
    }

    var unsupported: [FundingCountry] {
        matching.filter { model.unsupportedCountries.contains($0.code) }
    }

    var searchField: some View {
        HStack(spacing: DSSpacings.small) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.fgSecondary)
            TextField(String(localized: .Funding.countrySearch), text: $query)
                .typography(.bodyMedium)
                .foregroundStyle(.fgPrimary)
                .autocorrectionDisabled()
        }
        .padding(.horizontal, DSSpacings.medium)
        .frame(height: 40)
        .background(.bgSurfaceNested, in: Capsule())
    }

    func sectionTitle(_ title: String) -> some View {
        Text(title)
            .typography(.bodySmall)
            .foregroundStyle(.fgSecondary)
            .padding(.top, DSSpacings.medium)
            .padding(.bottom, DSSpacings.extraSmall)
    }

    func row(_ country: FundingCountry) -> some View {
        let isRefused = model.unsupportedCountries.contains(country.code)
        let isSelected = country == model.country

        return Button {
            model.chooseCountry(country)
        } label: {
            HStack(spacing: DSSpacings.smallIncreased) {
                Text(verbatim: country.flag)
                    .font(.system(size: 30))
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: country.name)
                        .typography(.bodyLargeEmphasized)
                        .foregroundStyle(isRefused ? Color.fgTertiary : Color.fgPrimary)
                    subtitle(country, isRefused: isRefused)
                }
                Spacer()
            }
            .padding(DSSpacings.small)
            .background(isSelected ? Color.bgSurfaceNested : Color.clear, in: RoundedRectangle(cornerRadius: 16))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(isRefused)
    }

    @ViewBuilder
    func subtitle(_ country: FundingCountry, isRefused: Bool) -> some View {
        if isRefused {
            Text(.Funding.countryRefused)
                .typography(.bodySmall)
                .foregroundStyle(.fgTertiary)
        } else if let currency = country.currencyName {
            Text(verbatim: currency)
                .typography(.bodySmall)
                .foregroundStyle(.fgSecondary)
        }
    }
}
