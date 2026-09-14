// audiotap.swift - global system-audio tap -> WAV. macOS 14.2+.
// Usage: audiotap <out.wav>  (records until SIGTERM/SIGINT)
import Foundation
import CoreAudio
import AudioToolbox

func check(_ status: OSStatus, _ what: String) {
    guard status == noErr else {
        FileHandle.standardError.write("audiotap: \(what) failed: \(status)\n".data(using: .utf8)!)
        exit(1)
    }
}

guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write("usage: audiotap <out.wav>\n".data(using: .utf8)!)
    exit(2)
}
let outURL = URL(fileURLWithPath: CommandLine.arguments[1])

// 1. Global tap: everything the system plays, no excluded processes.
let tapDesc = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
tapDesc.name = "miris-audiotap"
tapDesc.isPrivate = true
tapDesc.muteBehavior = .unmuted
var tapID = AudioObjectID(kAudioObjectUnknown)
check(AudioHardwareCreateProcessTap(tapDesc, &tapID), "create tap")

// 2. Tap stream format.
var fmtAddr = AudioObjectPropertyAddress(
    mSelector: kAudioTapPropertyFormat,
    mScope: kAudioObjectPropertyScopeGlobal,
    mElement: kAudioObjectPropertyElementMain)
var asbd = AudioStreamBasicDescription()
var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
check(AudioObjectGetPropertyData(tapID, &fmtAddr, 0, nil, &size, &asbd), "get tap format")

// 3. Private aggregate device containing only the tap.
let aggDict: [String: Any] = [
    kAudioAggregateDeviceNameKey: "miris-agg",
    kAudioAggregateDeviceUIDKey: UUID().uuidString,
    kAudioAggregateDeviceIsPrivateKey: true,
    kAudioAggregateDeviceTapListKey: [
        [kAudioSubTapUIDKey: tapDesc.uuid.uuidString]
    ],
]
var aggID = AudioObjectID(kAudioObjectUnknown)
check(AudioHardwareCreateAggregateDevice(aggDict as CFDictionary, &aggID), "create aggregate")

// 4. WAV writer: ExtAudioFile converts tap float32 -> 16-bit PCM on disk.
var outFmt = AudioStreamBasicDescription(
    mSampleRate: min(asbd.mSampleRate, 48_000),
    mFormatID: kAudioFormatLinearPCM,
    mFormatFlags: kLinearPCMFormatFlagIsSignedInteger | kLinearPCMFormatFlagIsPacked,
    mBytesPerPacket: 2 * asbd.mChannelsPerFrame,
    mFramesPerPacket: 1,
    mBytesPerFrame: 2 * asbd.mChannelsPerFrame,
    mChannelsPerFrame: asbd.mChannelsPerFrame,
    mBitsPerChannel: 16,
    mReserved: 0)
var extFile: ExtAudioFileRef?
check(ExtAudioFileCreateWithURL(outURL as CFURL, kAudioFileWAVEType, &outFmt, nil,
                                AudioFileFlags.eraseFile.rawValue, &extFile), "create wav")
check(ExtAudioFileSetProperty(extFile!, kExtAudioFileProperty_ClientDataFormat,
                              UInt32(MemoryLayout<AudioStreamBasicDescription>.size), &asbd),
      "set client format")

// 5. IO proc: each callback delivers tap buffers; write them straight out.
var procID: AudioDeviceIOProcID?
check(AudioDeviceCreateIOProcIDWithBlock(&procID, aggID, nil) { _, inData, _, _, _ in
    let frames = inData.pointee.mBuffers.mDataByteSize / asbd.mBytesPerFrame
    _ = ExtAudioFileWrite(extFile!, frames, inData)
}, "create io proc")
check(AudioDeviceStart(aggID, procID!), "start device")

// 6. Record until SIGTERM/SIGINT, then tear down in order.
let stop = DispatchSemaphore(value: 0)
let termSrc = DispatchSource.makeSignalSource(signal: SIGTERM)
let intSrc = DispatchSource.makeSignalSource(signal: SIGINT)
for src in [termSrc, intSrc] {
    src.setEventHandler { stop.signal() }
    src.resume()
}
signal(SIGTERM, SIG_IGN)
signal(SIGINT, SIG_IGN)

// 7. Exit alongside the parent recorder: a recorder killed by SIGKILL cannot
// stop us, and an orphaned helper must not keep capturing system audio.
// Parent death runs the same teardown as SIGTERM, finalizing the WAV header.
let initialParent = getppid()
let parentWatch = DispatchSource.makeTimerSource()
parentWatch.schedule(deadline: .now() + 2, repeating: 2)
parentWatch.setEventHandler {
    if getppid() != initialParent { stop.signal() }
}
parentWatch.resume()

stop.wait()

AudioDeviceStop(aggID, procID!)
AudioDeviceDestroyIOProcID(aggID, procID!)
AudioHardwareDestroyAggregateDevice(aggID)
AudioHardwareDestroyProcessTap(tapID)
ExtAudioFileDispose(extFile!)
exit(0)
