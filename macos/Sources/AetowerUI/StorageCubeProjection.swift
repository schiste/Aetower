import Foundation

struct StorageCubeProjectionInput: Equatable, Sendable {
    let id: String
    let sizeBytes: UInt64
}

struct StorageCubeProjectionBin: Equatable, Identifiable, Sendable {
    let id: String
    let sizeBytes: UInt64
    let cubeCount: Int
    let startIndex: Int
    let endIndex: Int
}

struct StorageCubeProjection: Equatable, Sendable {
    let unitBytes: UInt64
    let totalBytes: UInt64
    let totalCubes: Int
    let bins: [StorageCubeProjectionBin]
}

enum StorageCubeProjectionBuilder {
    static let defaultMaxCubes = 1_800

    static func build(
        inputs: [StorageCubeProjectionInput],
        maxCubes requestedMaxCubes: Int = defaultMaxCubes,
        preferredUnitBytes: UInt64? = nil
    ) -> StorageCubeProjection {
        let visibleInputs = inputs.filter { $0.sizeBytes > 0 }
        let totalBytes = visibleInputs.reduce(UInt64(0)) { total, input in
            let sum = total.addingReportingOverflow(input.sizeBytes)
            return sum.overflow ? UInt64.max : sum.partialValue
        }
        guard !visibleInputs.isEmpty, totalBytes > 0 else {
            return StorageCubeProjection(unitBytes: 1_024, totalBytes: 0, totalCubes: 0, bins: [])
        }

        // Every visible item gets at least one cube, so the hard cap cannot be
        // lower than the number of bins. This preserves the "tiny files matter"
        // semantics while keeping the projection bounded.
        let maxCubes = max(visibleInputs.count, requestedMaxCubes)
        let minimumUnitBytes = roundedCubeUnitBytes(
            for: ceilDivide(totalBytes, by: UInt64(max(1, maxCubes)))
        )
        var unitBytes = max(
            minimumUnitBytes,
            preferredUnitBytes ?? minimumUnitBytes
        )

        var counts = cubeCounts(for: visibleInputs, unitBytes: unitBytes)
        while counts.reduce(0, +) > maxCubes {
            unitBytes = nextRoundedCubeUnit(after: unitBytes)
            counts = cubeCounts(for: visibleInputs, unitBytes: unitBytes)
        }

        var cursor = 0
        let bins = zip(visibleInputs, counts).map { input, cubeCount in
            let bin = StorageCubeProjectionBin(
                id: input.id,
                sizeBytes: input.sizeBytes,
                cubeCount: cubeCount,
                startIndex: cursor,
                endIndex: cursor + cubeCount
            )
            cursor += cubeCount
            return bin
        }
        return StorageCubeProjection(
            unitBytes: unitBytes,
            totalBytes: totalBytes,
            totalCubes: cursor,
            bins: bins
        )
    }

    private static func cubeCounts(
        for inputs: [StorageCubeProjectionInput],
        unitBytes: UInt64
    ) -> [Int] {
        inputs.map { input in
            max(1, Int(min(UInt64(Int.max), ceilDivide(input.sizeBytes, by: unitBytes))))
        }
    }

    private static func ceilDivide(_ value: UInt64, by divisor: UInt64) -> UInt64 {
        guard divisor > 0 else { return value }
        let quotient = value / divisor
        return value % divisor == 0 ? quotient : quotient + 1
    }

    private static func roundedCubeUnitBytes(for rawBytes: UInt64) -> UInt64 {
        let raw = max(UInt64(1_024), rawBytes)
        var magnitude = UInt64(1_024)
        while magnitude.saturatingMultiply(1_024) < raw {
            magnitude = magnitude.saturatingMultiply(1_024)
        }
        for multiplier in [UInt64(1), 2, 4, 8, 16, 32, 64, 128, 256, 512] {
            let candidate = magnitude.saturatingMultiply(multiplier)
            if candidate >= raw {
                return candidate
            }
        }
        return magnitude.saturatingMultiply(1_024)
    }

    private static func nextRoundedCubeUnit(after unitBytes: UInt64) -> UInt64 {
        roundedCubeUnitBytes(for: unitBytes.saturatingAdding(1))
    }
}

private extension UInt64 {
    func saturatingMultiply(_ rhs: UInt64) -> UInt64 {
        let product = multipliedReportingOverflow(by: rhs)
        return product.overflow ? UInt64.max : product.partialValue
    }

    func saturatingAdding(_ rhs: UInt64) -> UInt64 {
        let sum = addingReportingOverflow(rhs)
        return sum.overflow ? UInt64.max : sum.partialValue
    }
}
