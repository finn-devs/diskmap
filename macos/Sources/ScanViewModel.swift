import SwiftUI
import AppKit

/// Represents one entry in the navigation history for back support.
struct HistoryEntry {
    let scanPath: String
    // We re-scan on back since the Rust handle only holds one tree at a time.
    // For instant back, we'd need multiple handles — but re-scan of a single
    // directory level is fast enough.
}

/// View model managing scan state, pie chart data, and navigation.
@Observable
final class ScanViewModel {
    let scanner = RustScanner()

    var scanPath: String = ""
    var history: [HistoryEntry] = []
    var selectedIndex: UInt32? = nil
    var hoveredIndex: UInt32? = nil
    var showDeniedAlert = false

    /// Pie slice data computed after scan.
    struct PieSlice: Identifiable {
        let id: UInt32 // node index
        let name: String
        let size: UInt64
        let isDir: Bool
        let fraction: Double      // sqrt-scaled visual fraction (for pie arc)
        let realPercent: Int       // actual linear percentage (for label)
        let startAngle: Double     // radians
        let endAngle: Double
        let color: (r: Double, g: Double, b: Double)
    }
    var slices: [PieSlice] = []
    var totalSize: UInt64 = 0

    // MARK: - Scan

    func scan(path: String) {
        scanPath = path
        slices = []
        selectedIndex = nil
        hoveredIndex = nil
        scanner.startScan(path: path)
    }

    func openFolderAndScan() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Select a directory to scan"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        scan(path: url.path)
    }

    func onScanComplete() {
        totalSize = scanner.totalSize()
        buildSlices()
        if !scanner.deniedPaths.isEmpty {
            showDeniedAlert = true
        }
    }

    // MARK: - Navigation

    func drillDown(into slice: PieSlice) {
        guard slice.isDir else { return }
        history.append(HistoryEntry(scanPath: scanPath))
        let newPath = scanPath == "/" ? "/\(slice.name)" : "\(scanPath)/\(slice.name)"
        scan(path: newPath)
    }

    func goBack() {
        guard let entry = history.popLast() else { return }
        scan(path: entry.scanPath)
    }

    var canGoBack: Bool { !history.isEmpty }

    // MARK: - Build pie slices

    private func buildSlices() {
        let root = scanner.rootIndex()
        let children = scanner.children(of: root)
        let nonEmpty = children.filter { $0.size > 0 }

        guard !nonEmpty.isEmpty else {
            slices = []
            return
        }

        let total = Double(totalSize)

        // Sqrt scale + minimum floor, then renormalize
        let sqrtSizes = nonEmpty.map { sqrt(Double($0.size)) }
        let sqrtTotal = sqrtSizes.reduce(0, +)
        let minVisual = 0.5 / Double(nonEmpty.count)

        var rawFractions = sqrtSizes.map { sqrtTotal > 0 ? max($0 / sqrtTotal, minVisual) : 0.0 }
        let adjTotal = rawFractions.reduce(0, +)
        rawFractions = rawFractions.map { adjTotal > 0 ? $0 / adjTotal : 0.0 }

        // Find max for heatmap
        let maxSize = nonEmpty.map(\.size).max() ?? 1

        // Sort descending by size
        let sorted = nonEmpty.enumerated()
            .sorted { $0.element.size > $1.element.size }

        var angle = -Double.pi / 2 // Start at 12 o'clock
        var result: [PieSlice] = []

        for (origIdx, child) in sorted {
            let fraction = rawFractions[origIdx]
            let sweep = fraction * 2 * Double.pi
            let endAngle = angle + sweep
            let realPct = total > 0 ? Int(Double(child.size) / total * 100) : 0

            // Heatmap color
            let ratio = log1p(Double(child.size)) / log1p(Double(maxSize))
            let t = min(max(ratio, 0), 1)
            let color = heatmapColor(t)

            result.append(PieSlice(
                id: child.index,
                name: child.name,
                size: child.size,
                isDir: child.isDir,
                fraction: fraction,
                realPercent: realPct,
                startAngle: angle,
                endAngle: endAngle,
                color: color
            ))
            angle = endAngle
        }

        slices = result
    }

    // MARK: - Actions

    func trashItem(index: UInt32) {
        let name = scanner.nodeName(index: index)
        let path = scanPath == "/" ? "/\(name)" : "\(scanPath)/\(name)"
        let url = URL(fileURLWithPath: path)
        do {
            try FileManager.default.trashItem(at: url, resultingItemURL: nil)
        } catch {
            print("Failed to trash \(path): \(error)")
        }
    }

    func openInFinder(index: UInt32) {
        let name = scanner.nodeName(index: index)
        let path = scanPath == "/" ? "/\(name)" : "\(scanPath)/\(name)"
        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: path)
    }

    func copyPath(index: UInt32) {
        let name = scanner.nodeName(index: index)
        let path = scanPath == "/" ? "/\(name)" : "\(scanPath)/\(name)"
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(path, forType: .string)
    }
}

// MARK: - Heatmap color

private func heatmapColor(_ t: Double) -> (r: Double, g: Double, b: Double) {
    if t < 0.2 {
        let s = t / 0.2
        return lerp3((0.30, 0.50, 0.80), (0.15, 0.65, 0.65), s)
    } else if t < 0.4 {
        let s = (t - 0.2) / 0.2
        return lerp3((0.15, 0.65, 0.65), (0.30, 0.75, 0.35), s)
    } else if t < 0.6 {
        let s = (t - 0.4) / 0.2
        return lerp3((0.30, 0.75, 0.35), (0.90, 0.80, 0.25), s)
    } else if t < 0.8 {
        let s = (t - 0.6) / 0.2
        return lerp3((0.90, 0.80, 0.25), (0.95, 0.55, 0.20), s)
    } else {
        let s = (t - 0.8) / 0.2
        return lerp3((0.95, 0.55, 0.20), (0.90, 0.25, 0.20), s)
    }
}

private func lerp3(_ a: (Double, Double, Double), _ b: (Double, Double, Double), _ t: Double) -> (r: Double, g: Double, b: Double) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}
