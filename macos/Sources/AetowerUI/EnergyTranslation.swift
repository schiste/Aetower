import Foundation

/// Pure translation of the kernel's per-process power signal (`energyNjPerS`,
/// a rate in nanowatts) into human-legible units: watts, hourly cost, hourly
/// carbon, and battery-time impact.
///
/// All functions are pure and total — invalid/zero inputs return `nil` rather
/// than producing `NaN`/`Inf`, so callers render a graceful fallback. Outputs
/// are estimates (kernel *billed* energy, same source as Activity Monitor's
/// Energy Impact; nominal battery voltage), always presented with a "~".
enum EnergyTranslation {
    /// Typical MacBook battery pack nominal voltage. Used only to turn the
    /// IOKit mAh capacity into watt-hours; device variance makes this an
    /// estimate (e.g. 8000 mAh × 11.4 V ≈ 91 Wh, matching Apple's ratings).
    static let nominalPackVoltage: Double = 11.4

    // MARK: - Core conversions

    /// Convert the nanowatt rate to watts. Returns `nil` for non-positive input
    /// (no kernel energy reported).
    static func watts(fromNjPerS njPerS: Double) -> Double? {
        guard njPerS > 0, njPerS.isFinite else { return nil }
        return njPerS / 1_000_000_000.0
    }

    /// Hourly running cost: kW × price-per-kWh.
    static func costPerHour(watts: Double, pricePerKwh: Double) -> Double {
        (watts / 1000.0) * pricePerKwh
    }

    /// Hourly carbon in grams: kW × grid intensity (gCO₂/kWh).
    static func carbonGramsPerHour(watts: Double, gridGramsPerKwh: Double) -> Double {
        (watts / 1000.0) * gridGramsPerKwh
    }

    /// Remaining battery energy in watt-hours from IOKit capacity + charge.
    /// `nil` when capacity/charge are unavailable.
    static func remainingWattHours(maxCapacityMah: UInt32?, chargePercent: UInt8?) -> Double? {
        guard let mah = maxCapacityMah, mah > 0, let pct = chargePercent else { return nil }
        let fraction = Double(min(pct, 100)) / 100.0
        return Double(mah) * fraction * nominalPackVoltage / 1000.0
    }

    /// Projected minutes of battery left at a given total draw. `nil` when the
    /// draw is zero (would be infinite).
    static func batteryMinutesRemaining(wattHours: Double, totalWatts: Double) -> Double? {
        guard totalWatts > 0, wattHours > 0 else { return nil }
        return (wattHours / totalWatts) * 60.0
    }

    /// An entity's share of the remaining battery time — what "~22 min" means:
    /// the slice of the projected battery life attributable to this process's
    /// draw. `nil` when total draw is zero.
    static func entityBatteryShareMinutes(
        entityWatts: Double,
        totalWatts: Double,
        minutesRemaining: Double
    ) -> Double? {
        guard totalWatts > 0 else { return nil }
        return (entityWatts / totalWatts) * minutesRemaining
    }

    // MARK: - Formatters (all prefixed "~" by the caller where appropriate)

    static func formatPower(_ watts: Double) -> String {
        let mw = watts * 1000.0
        if mw < 1 {
            return String(format: "%.2f mW", mw)
        } else if mw < 1000 {
            return String(format: "%.0f mW", mw)
        }
        return String(format: "%.1f W", watts)
    }

    static func formatCostPerHour(_ costPerHour: Double, currency: String) -> String {
        let decimals: Int
        if costPerHour >= 1 {
            decimals = 2
        } else if costPerHour >= 0.01 {
            decimals = 3
        } else {
            decimals = 4
        }
        return "\(currency)\(String(format: "%.\(decimals)f", costPerHour))/hr"
    }

    /// Daily cost (24 h continuous) — hourly is often microscopic, so the day
    /// figure is the legible one.
    static func formatCostPerDay(_ costPerHour: Double, currency: String) -> String {
        let perDay = costPerHour * 24.0
        let decimals = perDay >= 1 ? 2 : 3
        return "\(currency)\(String(format: "%.\(decimals)f", perDay))/day"
    }

    static func formatCarbonPerHour(_ gramsPerHour: Double) -> String {
        if gramsPerHour < 1 {
            return String(format: "%.2f gCO₂/hr", gramsPerHour)
        } else if gramsPerHour < 1000 {
            return String(format: "%.1f gCO₂/hr", gramsPerHour)
        }
        return String(format: "%.2f kgCO₂/hr", gramsPerHour / 1000.0)
    }

    static func formatMinutes(_ minutes: Double) -> String {
        if minutes >= 90 {
            return String(format: "%.1f h", minutes / 60.0)
        }
        return String(format: "%.0f min", minutes.rounded())
    }
}
