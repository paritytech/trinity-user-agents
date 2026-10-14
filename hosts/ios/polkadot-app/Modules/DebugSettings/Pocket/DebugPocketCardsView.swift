import SwiftUI

/// Cards typed in by hand, for driving a product's Pocket before it
/// publishes a manifest.
///
/// The product must be one with **no published worker**: a published one
/// always wins, so a card named here would never be reached.
struct DebugPocketCardsView: View {
    @State var viewModel = DebugPocketCardsViewModel()

    var body: some View {
        List {
            Section {
                ForEach(viewModel.cards) { card in
                    VStack(alignment: .leading, spacing: 4) {
                        Text("\(card.title): \(card.cardId)")
                            .font(.headline)
                        Text(card.productId)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(card.faceUrl)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                .onDelete { viewModel.delete(at: $0) }
            }

            Section("Add a card") {
                TextField("Product (dotNS name)", text: $viewModel.productId)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Card id", text: $viewModel.cardId)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Title", text: $viewModel.title)
                TextField("Face URL", text: $viewModel.faceUrl)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()

                Button("Save") { viewModel.save() }
                    .disabled(!viewModel.canSave)

                if let refusal = viewModel.refusal {
                    Text(refusal).foregroundStyle(.red)
                }
            }

            Section("Open") {
                ForEach(viewModel.cards) { card in
                    Text("polkadot://\(card.productId)/-/pocket/add?card=\(card.cardId)")
                        .font(.caption)
                        .textSelection(.enabled)
                }
            }
        }
        .navigationTitle("Pocket cards")
        .task { viewModel.load() }
    }
}
