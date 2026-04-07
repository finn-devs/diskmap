import SwiftUI

/// Breadcrumb navigation bar showing the current scan path.
struct BreadcrumbBar: View {
    let path: String
    let onNavigate: (String) -> Void

    var body: some View {
        HStack(spacing: 4) {
            ForEach(Array(pathComponents.enumerated()), id: \.offset) { index, component in
                if index > 0 {
                    Image(systemName: "chevron.right")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                Button(component.name) {
                    onNavigate(component.fullPath)
                }
                .buttonStyle(.plain)
                .font(.callout)
                .foregroundStyle(index == pathComponents.count - 1 ? .primary : .secondary)
            }
            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(.bar)
    }

    private var pathComponents: [(name: String, fullPath: String)] {
        guard !path.isEmpty else { return [] }

        let parts = path.split(separator: "/", omittingEmptySubsequences: true)
        var result: [(name: String, fullPath: String)] = [("/", "/")]

        var accumulated = ""
        for part in parts {
            accumulated += "/\(part)"
            result.append((String(part), accumulated))
        }
        return result
    }
}
