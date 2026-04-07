import SwiftUI

/// Main application view — sidebar + pie chart.
struct ContentView: View {
    @State private var viewModel = ScanViewModel()

    var body: some View {
        NavigationSplitView {
            DirectorySidebar(viewModel: viewModel) { path in
                viewModel.scan(path: path)
            }
        } detail: {
            VStack(spacing: 0) {
                if viewModel.scanner.isComplete || viewModel.scanner.isScanning {
                    BreadcrumbBar(path: viewModel.scanPath) { path in
                        viewModel.scan(path: path)
                    }
                }

                PieChartView(viewModel: viewModel)

                // Status bar
                statusBar
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    viewModel.openFolderAndScan()
                } label: {
                    Label("Scan Folder", systemImage: "folder.badge.plus")
                }
            }

            ToolbarItem(placement: .navigation) {
                Button {
                    viewModel.goBack()
                } label: {
                    Label("Back", systemImage: "chevron.left")
                }
                .disabled(!viewModel.canGoBack)
            }
        }
        .onChange(of: viewModel.scanner.isComplete) { _, isComplete in
            if isComplete {
                viewModel.onScanComplete()
            }
        }
        .alert("Restricted Directories", isPresented: $viewModel.showDeniedAlert) {
            Button("Open System Settings") {
                if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles") {
                    NSWorkspace.shared.open(url)
                }
            }
            Button("Skip", role: .cancel) {}
        } message: {
            Text("DiskMap found \(viewModel.scanner.deniedPaths.count) directories it cannot read. Grant Full Disk Access in System Settings to scan all files.")
        }
    }

    @ViewBuilder
    private var statusBar: some View {
        HStack {
            if viewModel.scanner.isComplete {
                Text("\(viewModel.scanner.filesScanned) files  \u{00b7}  \(viewModel.scanner.dirsScanned) dirs  \u{00b7}  \(formatSize(viewModel.totalSize))")
            } else if viewModel.scanner.isScanning {
                Text("Scanning \(viewModel.scanPath)...")
            } else {
                Text("Select a location to scan")
            }
            Spacer()
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .background(.bar)
    }
}
