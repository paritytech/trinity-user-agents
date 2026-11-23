import DesignSystem
import SwiftUI

/// Where the user pays from, which the quote is asked for. The core says
/// which countries a provider serves, with the smallest amount it takes
/// there, and which none does: those cannot be picked.
struct FundingCountryView: View {
    let viewModel: FundingCountryViewModel
    let presenter: FundingCountryPresenterProtocol

    @State private var query = ""
    private let countries = FundingCountries.all()

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(
                title: String(localized: .Funding.countryTitle),
                leadingTitle: true,
                onBack: presenter.back
            )
            searchField

            ScrollView {
                LazyVStack(alignment: .leading, spacing: DSSpacings.extraSmall) {
                    if query.isEmpty, let detected = viewModel.detected {
                        sectionTitle(String(localized: .Funding.countryDetected))
                        row(detected)
                        sectionTitle(String(localized: .Funding.countryOther))
                    }
                    ForEach(open) { row($0) }

                    if !withMinimum.isEmpty {
                        sectionTitle(String(localized: .Funding.countryMinimumSection))
                        ForEach(withMinimum) { row($0) }
                    }

                    if !unsupported.isEmpty {
                        sectionTitle(String(localized: .Funding.countryUnsupported))
                        ForEach(unsupported) { row($0) }
                    }
                }
            }
            .scrollDismissesKeyboard(.immediately)
        }
        .fundingScreen()
    }
}

private extension FundingCountryView {
    var matching: [PaymentCountry] {
        let listed = countries.filter { query.isEmpty ? $0 != viewModel.detected : true }
        guard !query.isEmpty else { return listed }
        return listed.filter { country in
            country.name.localizedCaseInsensitiveContains(query)
                || (country.currencyName?.localizedCaseInsensitiveContains(query) ?? false)
                || (country.currencyCode?.localizedCaseInsensitiveContains(query) ?? false)
        }
    }

    var open: [PaymentCountry] {
        matching
            .filter { !viewModel.unsupported.contains($0.code) && minimum(of: $0) == nil }
    }

    var withMinimum: [PaymentCountry] {
        matching
            .filter { !viewModel.unsupported.contains($0.code) && minimum(of: $0) != nil }
    }

    var unsupported: [PaymentCountry] {
        matching.filter { viewModel.unsupported.contains($0.code) }
    }

    func minimum(of country: PaymentCountry) -> String? {
        viewModel.minimums[country.code.uppercased()]
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

    func row(_ country: PaymentCountry) -> some View {
        let isRefused = viewModel.unsupported.contains(country.code)
        let isSelected = country == viewModel.selected

        return Button {
            presenter.chooseCountry(country)
        } label: {
            HStack(spacing: DSSpacings.smallIncreased) {
                Text(verbatim: country.flag)
                    .font(.system(size: 30))
                VStack(alignment: .leading, spacing: DSSpacings.extraTiny) {
                    Text(verbatim: country.name)
                        .typography(.bodyLargeEmphasized)
                        .foregroundStyle(isRefused ? Color.fgTertiary : Color.fgPrimary)
                    subtitle(country, isRefused: isRefused)
                }
                Spacer()
            }
            .padding(DSSpacings.small)
            .background(
                isSelected ? Color.bgSurfaceNested : Color.clear,
                in: RoundedRectangle(cornerRadius: DSRadii.mediumIncreased)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(isRefused)
    }

    @ViewBuilder
    func subtitle(_ country: PaymentCountry, isRefused: Bool) -> some View {
        if isRefused {
            Text(.Funding.countryRefused)
                .typography(.bodySmall)
                .foregroundStyle(.fgTertiary)
        } else if let minimum = minimum(of: country) {
            Text(verbatim: minimum)
                .typography(.bodySmall)
                .foregroundStyle(.fgSecondary)
        } else if let currency = country.currencyName {
            Text(verbatim: currency)
                .typography(.bodySmall)
                .foregroundStyle(.fgSecondary)
        }
    }
}
