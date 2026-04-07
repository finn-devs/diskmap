import Foundation
import CRustCore

/// Swift wrapper around the Rust FFI scan handle.
@Observable
final class RustScanner {
    private var handle: OpaquePointer?
    private var pollTimer: Timer?

    var filesScanned: UInt64 = 0
    var dirsScanned: UInt64 = 0
    var bytesScanned: UInt64 = 0
    var isScanning = false
    var isComplete = false
    var hasError = false
    var deniedPaths: [String] = []

    func startScan(path: String) {
        cleanup()
        isScanning = true
        isComplete = false
        hasError = false
        filesScanned = 0
        dirsScanned = 0
        bytesScanned = 0
        deniedPaths = []

        handle = path.withCString { dm_scan_start($0) }

        pollTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 20.0, repeats: true) { [weak self] _ in
            self?.poll()
        }
    }

    func cancelScan() {
        guard let h = handle else { return }
        dm_scan_cancel(h)
    }

    // MARK: - Node queries

    func rootIndex() -> UInt32 {
        guard let h = handle else { return 0 }
        return dm_root_index(h)
    }

    func totalSize() -> UInt64 {
        guard let h = handle else { return 0 }
        return dm_total_size(h)
    }

    func nodeName(index: UInt32) -> String {
        guard let h = handle else { return "" }
        let cStr = dm_node_name(h, index)
        defer { dm_string_free(cStr) }
        guard let cStr else { return "" }
        return String(cString: cStr)
    }

    func nodeSize(index: UInt32) -> UInt64 {
        guard let h = handle else { return 0 }
        return dm_node_size(h, index)
    }

    func nodeIsDir(index: UInt32) -> Bool {
        guard let h = handle else { return false }
        return dm_node_is_dir(h, index)
    }

    func nodeIsDenied(index: UInt32) -> Bool {
        guard let h = handle else { return false }
        return dm_node_is_denied(h, index)
    }

    func nodeChildCount(index: UInt32) -> UInt32 {
        guard let h = handle else { return 0 }
        return dm_node_child_count(h, index)
    }

    func nodeChildAt(index: UInt32, offset: UInt32) -> UInt32 {
        guard let h = handle else { return 0 }
        return dm_node_child_at(h, index, offset)
    }

    /// Get all children of a node as (index, name, size, isDir) tuples.
    func children(of nodeIndex: UInt32) -> [(index: UInt32, name: String, size: UInt64, isDir: Bool)] {
        let count = nodeChildCount(index: nodeIndex)
        return (0..<count).map { offset in
            let idx = nodeChildAt(index: nodeIndex, offset: offset)
            return (idx, nodeName(index: idx), nodeSize(index: idx), nodeIsDir(index: idx))
        }
    }

    // MARK: - Polling

    private func poll() {
        guard let h = handle else { return }
        let progress = dm_scan_poll(h)
        filesScanned = progress.files_scanned
        dirsScanned = progress.dirs_scanned
        bytesScanned = progress.bytes_scanned

        if progress.is_complete {
            isScanning = false
            isComplete = true
            hasError = progress.has_error
            pollTimer?.invalidate()
            pollTimer = nil

            let arr = dm_scan_denied_paths(h)
            defer { dm_string_array_free(arr) }
            if arr.count > 0, let strings = arr.strings {
                deniedPaths = (0..<Int(arr.count)).compactMap { i in
                    guard let cStr = strings[i] else { return nil }
                    return String(cString: cStr)
                }
            }
        }
    }

    private func cleanup() {
        pollTimer?.invalidate()
        pollTimer = nil
        if let h = handle {
            dm_scan_free(h)
            handle = nil
        }
    }

    deinit { cleanup() }
}

// MARK: - Helpers

func formatSize(_ bytes: UInt64) -> String {
    let kib: UInt64 = 1024
    let mib = 1024 * kib
    let gib = 1024 * mib
    let tib = 1024 * gib

    if bytes >= tib {
        return String(format: "%.1f TiB", Double(bytes) / Double(tib))
    } else if bytes >= gib {
        return String(format: "%.1f GiB", Double(bytes) / Double(gib))
    } else if bytes >= mib {
        return String(format: "%.1f MiB", Double(bytes) / Double(mib))
    } else if bytes >= kib {
        return String(format: "%.1f KiB", Double(bytes) / Double(kib))
    } else {
        return "\(bytes) B"
    }
}
