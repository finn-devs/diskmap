import SwiftUI

/// Donut pie chart view that renders the scan results.
struct PieChartView: View {
    @Bindable var viewModel: ScanViewModel

    var body: some View {
        GeometryReader { geo in
            let cx = geo.size.width / 2
            let cy = geo.size.height / 2
            let radius = min(cx, cy) - 80
            let innerRadius = radius * 0.35

            ZStack {
                // Dark background
                Color(red: 0.12, green: 0.12, blue: 0.14)

                if viewModel.scanner.isScanning {
                    // Spinner overlay
                    VStack(spacing: 12) {
                        ProgressView()
                            .scaleEffect(1.5)
                        Text("Scanning...")
                            .foregroundStyle(.secondary)
                    }
                } else if viewModel.slices.isEmpty && viewModel.scanner.isComplete {
                    Text("Empty directory")
                        .foregroundStyle(.secondary)
                } else {
                    Canvas { context, size in
                        let cx = size.width / 2
                        let cy = size.height / 2
                        let radius = min(cx, cy) - 80
                        let innerRadius = radius * 0.35

                        for slice in viewModel.slices {
                            let isHovered = viewModel.hoveredIndex == slice.id
                            let isSelected = viewModel.selectedIndex == slice.id
                            let midAngle = (slice.startAngle + slice.endAngle) / 2
                            let explode: CGFloat = isHovered ? 8 : 0
                            let ex = explode * cos(midAngle)
                            let ey = explode * sin(midAngle)

                            // Donut slice path
                            var path = Path()
                            path.addArc(center: CGPoint(x: cx + ex, y: cy + ey),
                                       radius: radius,
                                       startAngle: .radians(slice.startAngle),
                                       endAngle: .radians(slice.endAngle),
                                       clockwise: false)
                            path.addArc(center: CGPoint(x: cx + ex, y: cy + ey),
                                       radius: innerRadius,
                                       startAngle: .radians(slice.endAngle),
                                       endAngle: .radians(slice.startAngle),
                                       clockwise: true)
                            path.closeSubpath()

                            // Fill
                            let baseColor = Color(red: slice.color.r, green: slice.color.g, blue: slice.color.b)
                            let fillColor = isHovered
                                ? baseColor.opacity(1.0)
                                : baseColor.opacity(0.85)
                            context.fill(path, with: .color(fillColor))

                            // Border
                            context.stroke(path, with: .color(Color(red: 0.12, green: 0.12, blue: 0.14)), lineWidth: 2)

                            // Selection ring
                            if isSelected {
                                var ringPath = Path()
                                ringPath.addArc(center: CGPoint(x: cx + ex, y: cy + ey),
                                               radius: radius + 3,
                                               startAngle: .radians(slice.startAngle),
                                               endAngle: .radians(slice.endAngle),
                                               clockwise: false)
                                context.stroke(ringPath, with: .color(.blue.opacity(0.9)), lineWidth: 3)
                            }

                            // Labels for slices > 2%
                            if slice.fraction > 0.02 {
                                let labelR = radius + 20
                                let lx = cx + labelR * cos(midAngle) + ex
                                let ly = cy + labelR * sin(midAngle) + ey

                                let icon = slice.isDir ? "\u{1F4C1} " : ""
                                let label = "\(icon)\(slice.name) (\(slice.realPercent)%)"
                                let anchor: UnitPoint = cos(midAngle) >= 0 ? .leading : .trailing

                                context.draw(
                                    Text(label)
                                        .font(.system(size: 11))
                                        .foregroundStyle(.white.opacity(0.9)),
                                    at: CGPoint(x: lx, y: ly),
                                    anchor: anchor
                                )

                                context.draw(
                                    Text(formatSize(slice.size))
                                        .font(.system(size: 9))
                                        .foregroundStyle(.white.opacity(0.5)),
                                    at: CGPoint(x: lx, y: ly + 14),
                                    anchor: anchor
                                )
                            }
                        }

                        // Center text
                        context.draw(
                            Text(formatSize(viewModel.totalSize))
                                .font(.system(size: 18, weight: .bold))
                                .foregroundStyle(.white.opacity(0.9)),
                            at: CGPoint(x: cx, y: cy),
                            anchor: .center
                        )
                        context.draw(
                            Text("\(viewModel.slices.count) items")
                                .font(.system(size: 11))
                                .foregroundStyle(.white.opacity(0.4)),
                            at: CGPoint(x: cx, y: cy + 20),
                            anchor: .center
                        )
                    }
                    .onContinuousHover { phase in
                        switch phase {
                        case .active(let location):
                            viewModel.hoveredIndex = hitTest(
                                at: location,
                                center: CGPoint(x: cx, y: cy),
                                radius: radius,
                                innerRadius: innerRadius,
                                slices: viewModel.slices
                            )
                        case .ended:
                            viewModel.hoveredIndex = nil
                        }
                    }
                    .onTapGesture(count: 2) { location in
                        if let idx = hitTest(at: location, center: CGPoint(x: cx, y: cy),
                                            radius: radius, innerRadius: innerRadius,
                                            slices: viewModel.slices) {
                            if let slice = viewModel.slices.first(where: { $0.id == idx }) {
                                withAnimation(.easeInOut(duration: 0.2)) {
                                    viewModel.drillDown(into: slice)
                                }
                            }
                        }
                    }
                    .onTapGesture { location in
                        if let idx = hitTest(at: location, center: CGPoint(x: cx, y: cy),
                                            radius: radius, innerRadius: innerRadius,
                                            slices: viewModel.slices) {
                            viewModel.selectedIndex = idx
                        } else {
                            viewModel.selectedIndex = nil
                        }
                    }
                    .contextMenu {
                        if let idx = viewModel.selectedIndex {
                            let isDir = viewModel.scanner.nodeIsDir(index: idx)
                            if isDir {
                                Button("Open in Finder") {
                                    viewModel.openInFinder(index: idx)
                                }
                            }
                            Button("Copy Path") {
                                viewModel.copyPath(index: idx)
                            }
                            Divider()
                            Button("Move to Trash", role: .destructive) {
                                viewModel.trashItem(index: idx)
                            }
                        }
                    }
                }
            }
        }
    }

    // MARK: - Hit testing

    private func hitTest(at point: CGPoint, center: CGPoint, radius: CGFloat,
                         innerRadius: CGFloat, slices: [ScanViewModel.PieSlice]) -> UInt32? {
        let dx = point.x - center.x
        let dy = point.y - center.y
        let dist = sqrt(dx * dx + dy * dy)

        guard dist >= innerRadius, dist <= radius + 10 else { return nil }

        let angle = atan2(dy, dx)

        for slice in slices {
            var test = angle
            while test < slice.startAngle { test += 2 * .pi }
            while test > slice.startAngle + 2 * .pi { test -= 2 * .pi }
            if test >= slice.startAngle && test < slice.endAngle {
                return slice.id
            }
        }
        return nil
    }
}
