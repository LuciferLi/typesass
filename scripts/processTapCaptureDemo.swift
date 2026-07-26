import AppKit
import AudioToolbox
import AVFoundation
import Foundation

extension String: @retroactive LocalizedError {
  public var errorDescription: String? { self }
}

extension AudioObjectID {
  static let system = AudioObjectID(kAudioObjectSystemObject)
  static let unknown = AudioObjectID(kAudioObjectUnknown)

  var isValid: Bool {
    self != .unknown
  }

  func read<T>(
    _ selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal,
    element: AudioObjectPropertyElement = kAudioObjectPropertyElementMain,
    defaultValue: T
  ) throws -> T {
    try read(
      AudioObjectPropertyAddress(
        mSelector: selector,
        mScope: scope,
        mElement: element
      ),
      defaultValue: defaultValue,
      qualifierSize: 0,
      qualifierData: nil
    )
  }

  func read<T, Q>(
    _ selector: AudioObjectPropertySelector,
    defaultValue: T,
    qualifier: Q
  ) throws -> T {
    var mutableQualifier = qualifier
    return try withUnsafeMutablePointer(to: &mutableQualifier) { pointer in
      try read(
        AudioObjectPropertyAddress(
          mSelector: selector,
          mScope: kAudioObjectPropertyScopeGlobal,
          mElement: kAudioObjectPropertyElementMain
        ),
        defaultValue: defaultValue,
        qualifierSize: UInt32(MemoryLayout<Q>.size),
        qualifierData: pointer
      )
    }
  }

  private func read<T>(
    _ address: AudioObjectPropertyAddress,
    defaultValue: T,
    qualifierSize: UInt32,
    qualifierData: UnsafeRawPointer?
  ) throws -> T {
    var mutableAddress = address
    var dataSize: UInt32 = 0
    var status = AudioObjectGetPropertyDataSize(
      self,
      &mutableAddress,
      qualifierSize,
      qualifierData,
      &dataSize
    )
    guard status == noErr else {
      throw "读取 Core Audio 属性大小失败：\(status)"
    }

    var value = defaultValue
    status = withUnsafeMutablePointer(to: &value) { pointer in
      AudioObjectGetPropertyData(
        self,
        &mutableAddress,
        qualifierSize,
        qualifierData,
        &dataSize,
        pointer
      )
    }
    guard status == noErr else {
      throw "读取 Core Audio 属性失败：\(status)"
    }
    return value
  }

  func readString(_ selector: AudioObjectPropertySelector) throws -> String {
    try read(selector, defaultValue: "" as CFString) as String
  }

  func readBool(_ selector: AudioObjectPropertySelector) -> Bool {
    (try? read(selector, defaultValue: UInt32(0))) == 1
  }
}

struct AudioProcessInfo {
  let pid: pid_t
  let objectID: AudioObjectID
  let name: String
  let bundleID: String
  let audioActive: Bool
}

final class ProcessTapRecorder {
  private let targets: [AudioProcessInfo]
  private let outputURL: URL
  private let durationSeconds: Double
  private let queue = DispatchQueue(label: "typesass.process-tap.demo")
  private var tapID = AudioObjectID.unknown
  private var aggregateDeviceID = AudioObjectID.unknown
  private var deviceProcID: AudioDeviceIOProcID?
  private var audioFile: AVAudioFile?
  private var bufferCount = 0
  private var frameCount: AVAudioFramePosition = 0

  init(targets: [AudioProcessInfo], outputURL: URL, durationSeconds: Double) {
    self.targets = targets
    self.outputURL = outputURL
    self.durationSeconds = durationSeconds
  }

  func run() throws {
    let tapDescription: CATapDescription
    let processObjectIDs = targets.map(\.objectID).filter(\.isValid)
    if !processObjectIDs.isEmpty {
      tapDescription = CATapDescription(stereoMixdownOfProcesses: processObjectIDs)
    } else {
      let ownProcessID = try? translatePIDToProcessObjectID(pid: ProcessInfo.processInfo.processIdentifier)
      tapDescription = CATapDescription(stereoGlobalTapButExcludeProcesses: ownProcessID.map { [$0] } ?? [])
    }
    tapDescription.uuid = UUID()
    tapDescription.muteBehavior = .unmuted

    var createdTapID = AudioObjectID.unknown
    var status = AudioHardwareCreateProcessTap(tapDescription, &createdTapID)
    guard status == noErr else {
      throw "创建进程音频 Tap 失败：\(status)"
    }
    tapID = createdTapID

    let defaultOutput = try AudioObjectID.system.read(
      kAudioHardwarePropertyDefaultSystemOutputDevice,
      defaultValue: AudioDeviceID.unknown
    )
    let outputUID = try defaultOutput.readString(kAudioDevicePropertyDeviceUID)
    let aggregateUID = UUID().uuidString
    let aggregateName = targets.map(\.name).joined(separator: "+")
    let aggregateDescription: [String: Any] = [
      kAudioAggregateDeviceNameKey: "typesass-process-tap-\(aggregateName)",
      kAudioAggregateDeviceUIDKey: aggregateUID,
      kAudioAggregateDeviceMainSubDeviceKey: outputUID,
      kAudioAggregateDeviceIsPrivateKey: true,
      kAudioAggregateDeviceIsStackedKey: false,
      kAudioAggregateDeviceTapAutoStartKey: true,
      kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: outputUID]],
      kAudioAggregateDeviceTapListKey: [[
        kAudioSubTapDriftCompensationKey: true,
        kAudioSubTapUIDKey: tapDescription.uuid.uuidString,
      ]],
    ]

    status = AudioHardwareCreateAggregateDevice(
      aggregateDescription as CFDictionary,
      &aggregateDeviceID
    )
    guard status == noErr else {
      throw "创建聚合音频设备失败：\(status)"
    }

    var streamDescription = try tapID.read(
      kAudioTapPropertyFormat,
      defaultValue: AudioStreamBasicDescription()
    )
    guard let format = AVAudioFormat(streamDescription: &streamDescription) else {
      throw "创建 Tap 音频格式失败"
    }
    audioFile = try AVAudioFile(
      forWriting: outputURL,
      settings: [
        AVFormatIDKey: streamDescription.mFormatID,
        AVSampleRateKey: format.sampleRate,
        AVNumberOfChannelsKey: format.channelCount,
      ],
      commonFormat: .pcmFormatFloat32,
      interleaved: format.isInterleaved
    )

    status = AudioDeviceCreateIOProcIDWithBlock(
      &deviceProcID,
      aggregateDeviceID,
      queue
    ) { [weak self] _, inputData, _, _, _ in
      guard let self, let audioFile = self.audioFile else {
        return
      }
      guard let buffer = AVAudioPCMBuffer(
        pcmFormat: format,
        bufferListNoCopy: inputData,
        deallocator: nil
      ) else {
        return
      }
      do {
        try audioFile.write(from: buffer)
        bufferCount += 1
        frameCount += AVAudioFramePosition(buffer.frameLength)
      } catch {
        fputs("写入 Tap 音频失败：\(error.localizedDescription)\n", stderr)
      }
    }
    guard status == noErr else {
      throw "创建 Tap 回调失败：\(status)"
    }

    status = AudioDeviceStart(aggregateDeviceID, deviceProcID)
    guard status == noErr else {
      throw "启动 Tap 聚合设备失败：\(status)"
    }

    Thread.sleep(forTimeInterval: durationSeconds)
    cleanup()
    let targetSummary = targets
      .map { "\($0.name)(pid=\($0.pid))" }
      .joined(separator: ",")
    print(
      "wrote=\(outputURL.path) targets=\(targetSummary) buffers=\(bufferCount) frames=\(frameCount) sampleRate=\(Int(format.sampleRate)) channels=\(format.channelCount)"
    )
  }

  func cleanup() {
    if aggregateDeviceID.isValid {
      _ = AudioDeviceStop(aggregateDeviceID, deviceProcID)
      if let deviceProcID {
        _ = AudioDeviceDestroyIOProcID(aggregateDeviceID, deviceProcID)
      }
      _ = AudioHardwareDestroyAggregateDevice(aggregateDeviceID)
      aggregateDeviceID = .unknown
    }
    if tapID.isValid {
      _ = AudioHardwareDestroyProcessTap(tapID)
      tapID = .unknown
    }
    audioFile = nil
  }

  deinit {
    cleanup()
  }
}

func readProcessList() throws -> [AudioObjectID] {
  var address = AudioObjectPropertyAddress(
    mSelector: kAudioHardwarePropertyProcessObjectList,
    mScope: kAudioObjectPropertyScopeGlobal,
    mElement: kAudioObjectPropertyElementMain
  )
  var dataSize: UInt32 = 0
  var status = AudioObjectGetPropertyDataSize(
    AudioObjectID.system,
    &address,
    0,
    nil,
    &dataSize
  )
  guard status == noErr else {
    throw "读取音频进程列表大小失败：\(status)"
  }
  var objectIDs = [AudioObjectID](
    repeating: .unknown,
    count: Int(dataSize) / MemoryLayout<AudioObjectID>.size
  )
  status = AudioObjectGetPropertyData(
    AudioObjectID.system,
    &address,
    0,
    nil,
    &dataSize,
    &objectIDs
  )
  guard status == noErr else {
    throw "读取音频进程列表失败：\(status)"
  }
  return objectIDs
}

func translatePIDToProcessObjectID(pid: pid_t) throws -> AudioObjectID {
  try AudioObjectID.system.read(
    kAudioHardwarePropertyTranslatePIDToProcessObject,
    defaultValue: AudioObjectID.unknown,
    qualifier: pid
  )
}

func processName(pid: pid_t) -> String {
  let buffer = UnsafeMutablePointer<CChar>.allocate(capacity: Int(MAXPATHLEN))
  defer {
    buffer.deallocate()
  }
  let length = proc_name(pid, buffer, UInt32(MAXPATHLEN))
  guard length > 0 else {
    return "Unknown \(pid)"
  }
  return String(cString: buffer)
}

func readAudioProcesses() throws -> [AudioProcessInfo] {
  let applications = NSWorkspace.shared.runningApplications
  return try readProcessList().compactMap { objectID in
    guard let pid: pid_t = try? objectID.read(kAudioProcessPropertyPID, defaultValue: -1),
      pid > 0
    else {
      return nil
    }
    let application = applications.first { $0.processIdentifier == pid }
    let name = application?.localizedName ?? processName(pid: pid)
    let bundleID = (try? objectID.readString(kAudioProcessPropertyBundleID)) ?? application?.bundleIdentifier ?? ""
    return AudioProcessInfo(
      pid: pid,
      objectID: objectID,
      name: name,
      bundleID: bundleID,
      audioActive: objectID.readBool(kAudioProcessPropertyIsRunning)
    )
  }
}

/// 判断自动选择音频进程时是否应该跳过该进程。
func shouldSkipAutoTarget(_ process: AudioProcessInfo) -> Bool {
  let name = process.name.lowercased()
  let bundleID = process.bundleID.lowercased()
  if !process.audioActive {
    return true
  }
  if name.contains("typesass") || bundleID == "asia.aijob.aitool" {
    return true
  }
  if name.contains("graphics and media") || bundleID == "com.apple.webkit.gpu" {
    return true
  }
  if name.contains("process-tap") {
    return true
  }
  return false
}

/// 给自动选择的音频进程打分，优先选择真实用户媒体应用而不是后台 helper。
func autoTargetScore(_ process: AudioProcessInfo) -> Int {
  let name = process.name.lowercased()
  let bundleID = process.bundleID.lowercased()
  if bundleID == "com.apple.music" || name == "音乐" || name == "music" {
    return 100
  }
  if name.contains("helper") || name.contains("plugin") {
    return 20
  }
  return 60
}

/// 自动选择当前最适合作为字幕来源的音频进程，避免全局混音导致 ASR 输出漂移。
func findAutoTarget(processes: [AudioProcessInfo]) -> AudioProcessInfo {
  let candidates = processes
    .filter { !shouldSkipAutoTarget($0) }
    .sorted {
      let leftScore = autoTargetScore($0)
      let rightScore = autoTargetScore($1)
      if leftScore == rightScore {
        return $0.name < $1.name
      }
      return leftScore > rightScore
    }
  if let target = candidates.first {
    return target
  }
  return AudioProcessInfo(
    pid: 0,
    objectID: .unknown,
    name: "全局系统音频",
    bundleID: "global.system.audio",
    audioActive: true
  )
}

func findTarget(keyword: String, processes: [AudioProcessInfo]) throws -> AudioProcessInfo {
  let normalizedKeyword = keyword.lowercased()
  if normalizedKeyword == "active" || normalizedKeyword == "auto" {
    return findAutoTarget(processes: processes)
  }
  if let pid = pid_t(keyword),
    let process = processes.first(where: { $0.pid == pid })
  {
    return process
  }
  if let process = processes.first(where: {
    $0.name.lowercased().contains(normalizedKeyword)
      || $0.bundleID.lowercased().contains(normalizedKeyword)
  }) {
    return process
  }
  throw "未找到音频进程：\(keyword)"
}

func findTargets(keyword: String) throws -> [AudioProcessInfo] {
  let processes = try readAudioProcesses()
  if keyword == "--list" {
    for process in processes.sorted(by: { $0.name < $1.name }) {
      print(
        "pid=\(process.pid) active=\(process.audioActive) name=\(process.name) bundle=\(process.bundleID)"
      )
    }
    exit(0)
  }
  let keywords = keyword
    .split(separator: ",")
    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
    .filter { !$0.isEmpty }
  if keywords.isEmpty {
    return [findAutoTarget(processes: processes)]
  }
  var targets: [AudioProcessInfo] = []
  var seenPIDs = Set<pid_t>()
  for item in keywords {
    let target = try findTarget(keyword: item, processes: processes)
    if !seenPIDs.contains(target.pid) {
      targets.append(target)
      seenPIDs.insert(target.pid)
    }
  }
  return targets
}

@main
struct ProcessTapCaptureDemo {
  static func main() throws {
    let arguments = Array(CommandLine.arguments.dropFirst())
    let outputPath = arguments.first ?? "/tmp/typesass-process-tap-demo.caf"
    let duration = Double(arguments.dropFirst().first ?? "8") ?? 8
    let keyword = arguments.dropFirst().dropFirst().first ?? "Music"
    let targets = try findTargets(keyword: keyword)
    let recorder = ProcessTapRecorder(
      targets: targets,
      outputURL: URL(fileURLWithPath: outputPath),
      durationSeconds: duration
    )
    try recorder.run()
  }
}
