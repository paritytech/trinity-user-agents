import SwiftUI
import Products

// TODO: Temp interface for adding products: will be removed in future
struct DebugProductsListView: View {
    @State var viewModel: DebugProductsViewModel

    var body: some View {
        List {
            ForEach(viewModel.products) { product in
                VStack(alignment: .leading, spacing: 4) {
                    Text(product.name)
                        .font(.headline)
                    Text(product.identifier)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .contentShape(Rectangle())
                .onTapGesture {
                    viewModel.presentedSheet = .edit(product)
                }
            }
            .onDelete { viewModel.delete(at: $0) }
        }
        .navigationTitle("Products")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Add", systemImage: "plus") {
                    viewModel.presentedSheet = .add()
                }
            }
        }
        .sheet(item: $viewModel.presentedSheet) { sheet in
            DebugProductEditView(
                name: sheet.name,
                scriptURL: sheet.scriptURL,
                title: sheet.title
            ) { name, url in
                viewModel.saveProduct(name: name, scriptURL: url)
            }
        }
        .alert(
            "Error",
            isPresented: Binding(
                get: { viewModel.downloadError != nil },
                set: { if !$0 { viewModel.downloadError = nil } }
            )
        ) {
            Button("OK") { viewModel.downloadError = nil }
        } message: {
            Text(viewModel.downloadError ?? "")
        }
        .task {
            viewModel.loadProducts()
        }
    }
}
