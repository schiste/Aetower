import AetowerBridge

func machineFrictionScore(for host: HostSnapshot) -> Double {
    let cpuScore = min(Double(host.cpuPercent), 100.0) * 0.5
    let memoryRatio = host.memoryTotalBytes == 0 ? 0.0 : Double(host.memoryUsedBytes) / Double(host.memoryTotalBytes)
    let memoryScore = min(memoryRatio, 1.0) * 35.0
    let swapScore = host.swapUsedBytes == 0 ? 0.0 : (min(Double(host.swapUsedBytes) / 1_073_741_824.0, 8.0) / 8.0) * 15.0
    let compressedScore = host.memoryTotalBytes == 0 ? 0.0 : min(Double(host.compressedMemoryBytes) / Double(host.memoryTotalBytes), 1.0) * 12.0
    let networkScore = min(Double(host.networkReceiveBps + host.networkSendBps) / 8_388_608.0, 1.0) * 10.0
    let wakeupsScore = min(Double(host.wakeupsPerSecond) / 500.0, 1.0) * 8.0
    return min(cpuScore + memoryScore + swapScore + compressedScore + networkScore + wakeupsScore, 100.0)
}
