import AetowerBridge
import SwiftUI

/// Surfaces the hardware sensor data Aetower already collects but never showed
/// (iStat-parity): fans, temperatures, power rails, storage SMART health,
/// battery health, and Bluetooth device batteries.
public struct SensorDashboardView: View {
    let state: AppState
    let settings: SettingsStore

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    private var host: HostSnapshot { state.snapshot.host }

    private var hasAnySensor: Bool {
        !host.perCoreCpu.isEmpty
            || !host.fans.isEmpty
            || !host.cpuTemperatures.isEmpty
            || !host.powerReadings.isEmpty
            || !host.disks.isEmpty
            || host.batteryHealth != nil
            || !host.bluetoothDevices.isEmpty
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Sensors")
                        .font(.system(size: 28, weight: .semibold, design: .rounded))
                    Text("Live hardware telemetry — fans, temperatures, power, storage health, and batteries.")
                        .foregroundStyle(.secondary)
                }

                if !hasAnySensor {
                    ContentUnavailableView(
                        "No sensor data",
                        systemImage: "sensor",
                        description: Text("This Mac is not reporting sensor readings, or collection has not sampled them yet.")
                    )
                } else {
                    if totalAttributedWatts > 0 { energyCostSection }
                    if !host.perCoreCpu.isEmpty { coresSection }
                    if !host.fans.isEmpty { fansSection }
                    if !host.cpuTemperatures.isEmpty || host.gpuTemperatureCelsius != nil { temperatureSection }
                    if !host.powerReadings.isEmpty { powerSection }
                    if !host.disks.isEmpty { storageSection }
                    if let battery = host.batteryHealth { batterySection(battery) }
                    if !host.bluetoothDevices.isEmpty { bluetoothSection }
                }
            }
            .frame(maxWidth: 920, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
    }

    private var coresSection: some View {
        let performanceCount = host.perCoreCpu.filter { $0.kind == .performance }.count
        let efficiencyCount = host.perCoreCpu.filter { $0.kind == .efficiency }.count
        let detail = performanceCount + efficiencyCount > 0
            ? "\(performanceCount)P · \(efficiencyCount)E"
            : "\(host.perCoreCpu.count) cores"
        return GroupBox(label: Text("CPU cores · \(detail)")) {
            VStack(alignment: .leading, spacing: 5) {
                ForEach(host.perCoreCpu, id: \.index) { core in
                    HStack(spacing: 8) {
                        Text(coreKindLabel(core.kind))
                            .font(.system(size: 10, weight: .bold, design: .monospaced))
                            .foregroundStyle(coreKindTone(core.kind))
                            .frame(width: 16, alignment: .leading)
                        Text("Core \(core.index)")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .frame(width: 52, alignment: .leading)
                        ProgressView(value: Double(min(max(core.percent, 0), 100)), total: 100)
                            .tint(coreKindTone(core.kind))
                        Text(String(format: "%.0f%%", core.percent))
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .frame(width: 40, alignment: .trailing)
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private func coreKindLabel(_ kind: CoreKind) -> String {
        switch kind {
        case .performance: return "P"
        case .efficiency: return "E"
        case .unknown: return "—"
        }
    }

    private func coreKindTone(_ kind: CoreKind) -> Color {
        switch kind {
        case .performance: return AetowerDesign.Tone.cpu
        case .efficiency: return AetowerDesign.Tone.memory
        case .unknown: return AetowerDesign.Status.neutral
        }
    }

    private var fansSection: some View {
        GroupBox("Fans") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(host.fans, id: \.id) { fan in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(fan.name).font(.caption.weight(.medium))
                            Spacer()
                            Text("\(Int(fan.currentRpm)) rpm")
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(.secondary)
                        }
                        ProgressView(
                            value: Double(max(0, fan.currentRpm - fan.minRpm)),
                            total: Double(max(1, fan.maxRpm - fan.minRpm))
                        )
                        .tint(AetowerDesign.Tone.network)
                        Text("min \(Int(fan.minRpm)) · max \(Int(fan.maxRpm)) rpm")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private var temperatureSection: some View {
        GroupBox("Temperatures") {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.sm)], alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(host.cpuTemperatures, id: \.label) { reading in
                    sensorTile(reading.label, value: String(format: "%.1f °C", reading.celsius), tone: temperatureTone(reading.celsius))
                }
                if let gpu = host.gpuTemperatureCelsius {
                    sensorTile("GPU", value: String(format: "%.1f °C", gpu), tone: temperatureTone(gpu))
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private var powerSection: some View {
        GroupBox("Power") {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.sm)], alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(host.powerReadings, id: \.label) { reading in
                    sensorTile(reading.label, value: String(format: "%.2f %@", reading.value, sensorEnumLabel(reading.unit)), tone: AetowerDesign.Tone.energy)
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    /// Total per-process power Aetower has attributed to running entities.
    private var totalAttributedWatts: Double {
        state.snapshot.entities
            .compactMap { EnergyTranslation.watts(fromNjPerS: $0.metrics.energyNjPerS) }
            .reduce(0, +)
    }

    private var energyCostSection: some View {
        let watts = totalAttributedWatts
        let cost = EnergyTranslation.costPerHour(
            watts: watts,
            pricePerKwh: settings.electricityPricePerKwh
        )
        let carbon = EnergyTranslation.carbonGramsPerHour(
            watts: watts,
            gridGramsPerKwh: settings.gridCarbonIntensityGramsPerKwh
        )
        return GroupBox("Energy & cost") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                LazyVGrid(
                    columns: [
                        GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.sm)
                    ],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.sm
                ) {
                    sensorTile(
                        "Attributed draw",
                        value: "~\(EnergyTranslation.formatPower(watts))",
                        tone: AetowerDesign.Tone.energy
                    )
                    sensorTile(
                        "Cost",
                        value:
                            "~\(EnergyTranslation.formatCostPerHour(cost, currency: settings.energyCurrencySymbol))",
                        tone: AetowerDesign.Tone.energy
                    )
                    sensorTile(
                        "Carbon",
                        value: "~\(EnergyTranslation.formatCarbonPerHour(carbon))",
                        tone: AetowerDesign.Tone.energy
                    )
                    sensorTile(
                        "Battery left",
                        value: batteryLeftLabel(totalWatts: watts),
                        tone: AetowerDesign.Tone.energy
                    )
                }
                Text(
                    "Sum of per-process kernel-billed energy across observed apps — an estimate that understates total system draw (display, idle, other users aren't attributed)."
                )
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private func batteryLeftLabel(totalWatts: Double) -> String {
        guard host.onBattery else { return "on AC power" }
        guard
            let wattHours = EnergyTranslation.remainingWattHours(
                maxCapacityMah: host.batteryHealth?.maxCapacityMah,
                chargePercent: host.batteryChargePercent
            ),
            let minutes = EnergyTranslation.batteryMinutesRemaining(
                wattHours: wattHours,
                totalWatts: totalWatts
            )
        else {
            return "—"
        }
        return "~\(EnergyTranslation.formatMinutes(minutes))"
    }

    private var storageSection: some View {
        GroupBox("Storage health (SMART)") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(host.disks, id: \.deviceIdentifier) { disk in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(disk.model.isEmpty ? disk.deviceIdentifier : disk.model)
                                .font(.caption.weight(.medium))
                                .lineLimit(1)
                            Spacer()
                            Text(sensorEnumLabel(disk.status))
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(diskStatusTone(disk.status))
                        }
                        HStack(spacing: AetowerDesign.Spacing.md) {
                            if let used = disk.percentageUsed { metric("Used", "\(used)%") }
                            if let spare = disk.availableSparePercent { metric("Spare", "\(spare)%") }
                            if let temp = disk.temperatureCelsius { metric("Temp", String(format: "%.0f°C", temp)) }
                            if let hours = disk.powerOnHours { metric("On", "\(hours)h") }
                            if let errors = disk.mediaErrors { metric("Errors", "\(errors)") }
                        }
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private func batterySection(_ battery: BatteryHealthSnapshot) -> some View {
        GroupBox("Battery health") {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.sm)], alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                if let health = battery.healthPercent {
                    sensorTile("Health", value: String(format: "%.0f%%", health), tone: health < 80 ? AetowerDesign.Status.warning : AetowerDesign.Status.success)
                }
                if let cycles = battery.cycleCount { sensorTile("Cycles", value: "\(cycles)", tone: AetowerDesign.Tone.cpu) }
                sensorTile("Condition", value: sensorEnumLabel(battery.condition), tone: AetowerDesign.Status.neutral)
                if let maxCap = battery.maxCapacityMah { sensorTile("Capacity", value: "\(maxCap) mAh", tone: AetowerDesign.Tone.memory) }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private var bluetoothSection: some View {
        GroupBox("Bluetooth devices") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                ForEach(host.bluetoothDevices, id: \BluetoothDeviceBattery.address) { btDevice in
                    HStack {
                        Text(verbatim: btDevice.name).font(.caption)
                        Spacer()
                        if let battery = btDevice.batteryPercent {
                            Text(verbatim: "\(battery)%")
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(battery < 20 ? AetowerDesign.Status.warning : Color.secondary)
                        } else {
                            Text(verbatim: "—").foregroundStyle(.tertiary)
                        }
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private func sensorTile(_ title: String, value: String, tone: Color) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            Text(value)
                .font(.system(size: 14, weight: .semibold, design: .rounded))
                .foregroundStyle(tone)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(AetowerDesign.Spacing.sm)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
    }

    private func metric(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(title).font(.caption2).foregroundStyle(.tertiary)
            Text(value).font(.system(size: 11, design: .monospaced)).foregroundStyle(.secondary)
        }
    }

    private func temperatureTone(_ celsius: Float) -> Color {
        if celsius >= 90 { return AetowerDesign.Status.error }
        if celsius >= 75 { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.success
    }

    private func diskStatusTone<Status>(_ status: Status) -> Color {
        let label = sensorEnumLabel(status).lowercased()
        if label.contains("fail") || label.contains("error") { return AetowerDesign.Status.error }
        if label.contains("warn") || label.contains("degrad") { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.success
    }

    /// Render an FFI enum case as a readable label (e.g. `.notSupported` → "Not supported").
    private func sensorEnumLabel<Value>(_ value: Value) -> String {
        let raw = String(describing: value)
        var result = ""
        for (index, character) in raw.enumerated() {
            if index == 0 {
                result.append(Character(character.uppercased()))
            } else if character.isUppercase {
                result.append(" ")
                result.append(Character(character.lowercased()))
            } else {
                result.append(character)
            }
        }
        return result
    }
}
