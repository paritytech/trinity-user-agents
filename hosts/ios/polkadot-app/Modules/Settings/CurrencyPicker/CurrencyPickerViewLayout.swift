import SwiftUI
import PolkadotUI
import DesignSystem

struct CurrencyPickerViewLayout: View {
    @Bindable var viewModel: CurrencyPickerViewModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgDisabled)

                TextField("Search", text: $viewModel.searchText)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgPrimary)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .tint(.fgPrimary)

                if !viewModel.searchText.isEmpty {
                    Button {
                        viewModel.searchText = ""
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .typography(.bodyMedium)
                            .foregroundStyle(.fgDisabled)
                    }
                }
            }
            .padding(.horizontal, 12)
            .frame(height: 36)
            .background(Color(.fill18))
            .clipShape(RoundedRectangle(cornerRadius: 18))
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            List {
                ForEach(viewModel.filteredCurrencies, id: \.code) { currency in
                    currencyRow(for: currency)
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .disabled(viewModel.loadingCode != nil)
        }
        .alert(
            String(localized: .currencyPickerRateLimitTitle),
            isPresented: $viewModel.showRateLimitError
        ) {
            Button(String(localized: .currencyPickerRateLimitOk), role: .cancel) {}
        } message: {
            Text(String(localized: .currencyPickerRateLimitMessage))
        }
    }
}

// MARK: - Private functions

extension CurrencyPickerViewLayout {
    private func currencyRow(for currency: Currency) -> some View {
        Button {
            Task {
                let success = await viewModel.select(currency)
                if success {
                    dismiss()
                }
            }
        } label: {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(currency.code)
                        .typography(.titleLarge)
                        .foregroundStyle(.fgPrimary)

                    Text(currency.name)
                        .typography(.bodyMedium)
                        .foregroundStyle(.fgSecondary)
                }

                Spacer()

                if viewModel.loadingCode == currency.code {
                    ProgressView()
                } else if currency.code == viewModel.selectedCode {
                    Image(systemName: "checkmark")
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(.fgPrimary)
                }
            }
        }
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
    }
}
