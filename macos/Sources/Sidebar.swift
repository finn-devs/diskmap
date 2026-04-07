import SwiftUI

/// Left sidebar with filesystem quick-access locations.
struct DirectorySidebar: View {
    @Bindable var viewModel: ScanViewModel
    let onSelect: (String) -> Void

    private var quickDirs: [(name: String, icon: String, path: String)] {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return [
            ("Root", "internaldrive", "/"),
            ("Home", "house", home),
            ("Documents", "doc.on.doc", "\(home)/Documents"),
            ("Downloads", "arrow.down.circle", "\(home)/Downloads"),
            ("Desktop", "menubar.dock.rectangle", "\(home)/Desktop"),
            ("Pictures", "photo", "\(home)/Pictures"),
            ("Music", "music.note", "\(home)/Music"),
            ("Movies", "film", "\(home)/Movies"),
            ("Projects", "folder", "\(home)/Projects"),
            ("/tmp", "clock", "/tmp"),
            ("/var", "folder", "/var"),
            ("/usr", "folder", "/usr"),
        ].filter { FileManager.default.fileExists(atPath: $0.path) }
    }

    var body: some View {
        List {
            Section("Filesystem") {
                ForEach(quickDirs, id: \.path) { dir in
                    Button {
                        onSelect(dir.path)
                    } label: {
                        Label(dir.name, systemImage: dir.icon)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .listStyle(.sidebar)
        .frame(minWidth: 200, idealWidth: 220)
    }
}
