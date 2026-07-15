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

    private var payload: SensorDashboardPayload { state.sensorDashboardPayload }
    private var host: HostSnapshot { payload.host }

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
                if hasAnySensor {
                    atAGlanceStrip
                }

                if !hasAnySensor {
                    ContentUnavailableView(
                        "No sensor data",
                        systemImage: "sensor",
                        description: Text("This Mac is not reporting sensor readings, or collection has not sampled them yet.")
                    )
                } else {
                    if payload.thermalForecast != nil { thermalForecastSection }
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
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
    }

    private var atAGlanceStrip: some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 142), spacing: AetowerDesign.Spacing.sm)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.sm
        ) {
            glanceCard(
                "Thermal",
                value: thermalSummaryLabel,
                detail: thermalDetail,
                systemImage: "thermometer.medium",
                tone: thermalSummaryTone
            )
            glanceCard(
                "Temperature",
                value: peakTemperatureLabel,
                detail: "highest reported package or GPU sensor",
                systemImage: "thermometer.sun",
                tone: peakTemperatureTone
            )
            glanceCard(
                "Power",
                value: totalAttributedWatts > 0 ? "~\(EnergyTranslation.formatPower(totalAttributedWatts))" : "No draw",
                detail: totalAttributedWatts > 0 ? "attributed to observed processes" : "waiting for energy attribution",
                systemImage: "bolt.fill",
                tone: AetowerDesign.Tone.energy
            )
            glanceCard(
                "Fans",
                value: fanSummaryLabel,
                detail: host.fans.isEmpty ? "not reported on this Mac" : "current hardware cooling",
                systemImage: "fan",
                tone: fanSummaryTone
            )
            glanceCard(
                "Storage",
                value: storageSummaryLabel,
                detail: host.disks.isEmpty ? "no SMART channel yet" : "\(host.disks.count) SMART device(s)",
                systemImage: "internaldrive",
                tone: storageSummaryTone
            )
            glanceCard(
                "Battery",
                value: batterySummaryLabel,
                detail: host.onBattery ? "running from battery" : "connected to AC power",
                systemImage: host.onBattery ? "battery.75percent" : "powerplug",
                tone: batterySummaryTone
            )
        }
    }

    private func glanceCard(
        _ title: String,
        value: String,
        detail: String,
        systemImage: String,
        tone: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 6) {
                Image(systemName: systemImage)
                    .foregroundStyle(tone)
                Text(title.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Text(value)
                .font(.system(size: 19, weight: .semibold, design: .rounded))
                .foregroundStyle(tone)
                .lineLimit(1)
                .minimumScaleFactor(0.82)
            Text(detail)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(2)
        }
        .padding(AetowerDesign.Spacing.sm)
        .frame(maxWidth: .infinity, minHeight: 104, alignment: .topLeading)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(tone.opacity(0.45))
                .frame(height: 2)
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
                            Text(SensorFanFormatter.rpmLabel(fan.currentRpm))
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(.secondary)
                        }
                        let progress = SensorFanFormatter.progress(
                            currentRpm: fan.currentRpm,
                            minRpm: fan.minRpm,
                            maxRpm: fan.maxRpm
                        )
                        ProgressView(
                            value: progress.value,
                            total: progress.total
                        )
                        .tint(AetowerDesign.Tone.network)
                        Text(SensorFanFormatter.rangeLabel(minRpm: fan.minRpm, maxRpm: fan.maxRpm))
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

    @ViewBuilder
    private var thermalForecastSection: some View {
        if let forecast = payload.thermalForecast {
            GroupBox("Thermal forecast") {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    HStack(spacing: 8) {
                        Image(systemName: "thermometer.sun.fill")
                        Text(forecastHeadline(forecast))
                            .font(.system(size: 15, weight: .semibold, design: .rounded))
                    }
                    .foregroundStyle(forecastTone(forecast.state))
                    if let label = forecast.topContributorLabel {
                        Text(
                            "Top contributor: \(label) — open it to suspend or lower its priority."
                        )
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    }
                    let temps = payload.hostTrend.maxCpuTemperature.map { Double($0) }
                    if temps.count >= 2 {
                        HorizonGraph(
                            title: "CPU temperature",
                            subtitle: String(
                                format: "trend %+.1f °C/min", forecast.trendCelsiusPerMin),
                            samples: temps,
                            timestamps: [],
                            tone: AetowerDesign.Tone.energy,
                            format: { String(format: "%.0f °C", $0) }
                        )
                    }
                    Text(
                        "Heuristic estimate from the recent temperature slope — not a hardware throttle reading."
                    )
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                }
                .padding(.top, AetowerDesign.Spacing.xs)
            }
        }
    }

    private func forecastHeadline(_ forecast: ThermalForecast) -> String {
        if let minutes = forecast.minutesToThrottle {
            return String(format: "Throttle likely in ~%.0f min", minutes)
        }
        return "Throttling now"
    }

    private func forecastTone(_ state: ThermalState) -> Color {
        switch state {
        case .critical, .serious: return AetowerDesign.Status.error
        case .fair: return AetowerDesign.Status.warning
        case .nominal: return AetowerDesign.Status.warning
        @unknown default: return AetowerDesign.Status.warning
        }
    }

    /// Total per-process power Aetower has attributed to running entities.
    private var totalAttributedWatts: Double {
        payload.totalAttributedWatts
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

    private var thermalSummaryLabel: String {
        sensorEnumLabel(host.thermalState)
    }

    private var thermalDetail: String {
        if let forecast = payload.thermalForecast,
           let minutes = forecast.minutesToThrottle
        {
            return String(format: "throttle risk in ~%.0f min", minutes)
        }
        return host.lowPowerMode ? "low power mode enabled" : "current system pressure"
    }

    private var thermalSummaryTone: Color {
        switch host.thermalState {
        case .nominal: return AetowerDesign.Status.success
        case .fair: return AetowerDesign.Status.warning
        case .serious, .critical: return AetowerDesign.Status.error
        }
    }

    private var peakTemperatureLabel: String {
        guard let peak = peakTemperatureCelsius else {
            return "No sensor"
        }
        return String(format: "%.1f °C", peak)
    }

    private var peakTemperatureTone: Color {
        guard let peak = peakTemperatureCelsius else {
            return AetowerDesign.Status.neutral
        }
        return temperatureTone(peak)
    }

    private var peakTemperatureCelsius: Float? {
        let cpuPeaks = host.cpuTemperatures.map(\.celsius)
        return (cpuPeaks + [host.gpuTemperatureCelsius].compactMap { $0 }).max()
    }

    private var fanSummaryLabel: String {
        guard !host.fans.isEmpty else {
            return "Passive"
        }
        let validRpms = host.fans.compactMap { SensorFanFormatter.sanitizedRpm($0.currentRpm) }
        guard let maxRpm = validRpms.max() else {
            return "\(host.fans.count) · rpm n/a"
        }
        return "\(host.fans.count) · \(SensorFanFormatter.rpmLabel(maxRpm))"
    }

    private var fanSummaryTone: Color {
        let normalizedLoads = host.fans.compactMap {
            SensorFanFormatter.normalizedLoad(
                currentRpm: $0.currentRpm,
                minRpm: $0.minRpm,
                maxRpm: $0.maxRpm
            )
        }
        guard let normalized = normalizedLoads.max() else {
            return AetowerDesign.Status.neutral
        }
        if normalized >= 0.85 { return AetowerDesign.Status.error }
        if normalized >= 0.6 { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.success
    }

    private var storageSummaryLabel: String {
        guard !host.disks.isEmpty else {
            return "No data"
        }
        let unhealthy = host.disks.filter {
            let label = sensorEnumLabel($0.status).lowercased()
            return label.contains("fail") || label.contains("warn") || label.contains("degrad")
        }.count
        return unhealthy == 0 ? "Healthy" : "\(unhealthy) flagged"
    }

    private var storageSummaryTone: Color {
        guard !host.disks.isEmpty else {
            return AetowerDesign.Status.neutral
        }
        return storageSummaryLabel == "Healthy" ? AetowerDesign.Status.success : AetowerDesign.Status.warning
    }

    private var batterySummaryLabel: String {
        if host.onBattery, let charge = host.batteryChargePercent {
            return "\(charge)%"
        }
        if let health = host.batteryHealth?.healthPercent {
            return String(format: "%.0f%% health", health)
        }
        return host.onBattery ? "Battery" : "AC power"
    }

    private var batterySummaryTone: Color {
        if host.onBattery, let charge = host.batteryChargePercent, charge <= 20 {
            return AetowerDesign.Status.warning
        }
        if let health = host.batteryHealth?.healthPercent, health < 80 {
            return AetowerDesign.Status.warning
        }
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

enum SensorFanFormatter {
    private static let plausibleFanRpmCeiling: Float = 10_000

    static func sanitizedRpm(_ rpm: Float) -> Float? {
        guard rpm.isFinite, rpm >= 0, rpm <= plausibleFanRpmCeiling else {
            return nil
        }
        return rpm
    }

    static func rpmLabel(_ rpm: Float) -> String {
        guard let rpm = sanitizedRpm(rpm) else {
            return "rpm n/a"
        }
        return "\(Int(rpm.rounded())) rpm"
    }

    static func rangeLabel(minRpm: Float, maxRpm: Float) -> String {
        let minLabel = sanitizedRpm(minRpm).map { "\(Int($0.rounded()))" } ?? "n/a"
        let maxLabel = sanitizedRpm(maxRpm).map { "\(Int($0.rounded()))" } ?? "n/a"
        return "min \(minLabel) · max \(maxLabel) rpm"
    }

    static func progress(
        currentRpm: Float,
        minRpm: Float,
        maxRpm: Float
    ) -> (value: Double, total: Double) {
        let current = sanitizedRpm(currentRpm) ?? 0
        let minimum = sanitizedRpm(minRpm) ?? 0
        let reportedMaximum = sanitizedRpm(maxRpm)
        let maximum = max(reportedMaximum ?? max(minimum + 1, current), minimum + 1)
        let total = max(1, maximum - minimum)
        let value = min(max(0, current - minimum), total)
        return (Double(value), Double(total))
    }

    static func normalizedLoad(
        currentRpm: Float,
        minRpm: Float,
        maxRpm: Float
    ) -> Float? {
        guard let current = sanitizedRpm(currentRpm) else {
            return nil
        }
        let minimum = sanitizedRpm(minRpm) ?? 0
        guard let maximum = sanitizedRpm(maxRpm), maximum > minimum else {
            return nil
        }
        return min(max((current - minimum) / (maximum - minimum), 0), 1)
    }
}
