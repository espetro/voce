/*
 * VoceAudio.c — CoreAudio HAL plugin for "Voce Microphone"
 *
 * Creates a virtual input-only device (mono, 16000 Hz) that reads from a
 * POSIX shared-memory ring buffer written by the Voce.app process.
 *
 * Build & install (run from repo root):
 *   make -C audio-driver install
 *
 * The compiled .driver bundle lands in ~/Library/Audio/Plug-Ins/HAL/;
 * CoreAudio picks it up after `launchctl kickstart -k system/com.apple.audio.coreaudiod`.
 */

#include <CoreAudio/AudioServerPlugIn.h>
#include <pthread.h>
#include <stdatomic.h>
#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>
#include <mach/mach_time.h>
#include <string.h>
#include <stdlib.h>
#include <stdio.h>

// ─── Shared-memory ring buffer ────────────────────────────────────────────────

#define VOCE_SHM_NAME    "/voce_audio_ring"
#define RING_CAP         (16000u * 4u)          // 4 seconds of f32 @ 16000 Hz

typedef struct {
    _Atomic(uint32_t) write_pos;
    _Atomic(uint32_t) read_pos;
    float             samples[RING_CAP];
} VoceRingBuffer;

// ─── Device constants ─────────────────────────────────────────────────────────

#define kSampleRate      16000.0
#define kNumChannels     1u
#define kBufferFrames    512u

#define kObjectID_PlugIn     1u
#define kObjectID_Device     2u
#define kObjectID_Stream     3u

#define kDeviceUID       CFSTR("com.voce.microphone")
#define kDeviceName      CFSTR("Voce Microphone")
#define kStreamName      CFSTR("Voce Mic In")
#define kMfgName         CFSTR("Voce")
#define kPlugInBundleID  CFSTR("com.voce.audio-driver")

// ─── Driver state ─────────────────────────────────────────────────────────────

typedef struct {
    AudioServerPlugInDriverInterface *mInterface;
    AudioServerPlugInDriverInterface  mInterfaceImpl;

    pthread_mutex_t  mMutex;
    volatile bool    mDeviceRunning;

    VoceRingBuffer  *mRing;
    int              mShmFd;

    uint64_t         mAnchorSampleTime;
    uint64_t         mAnchorHostTime;
    mach_timebase_info_data_t mTimebase;
} VoceDriver;

static VoceDriver gDriver;

#define LOCK()   pthread_mutex_lock(&gDriver.mMutex)
#define UNLOCK() pthread_mutex_unlock(&gDriver.mMutex)

// ─── Shared-memory helpers ────────────────────────────────────────────────────

static void ring_open(void) {
    gDriver.mShmFd = shm_open(VOCE_SHM_NAME, O_CREAT | O_RDWR, 0600);
    if (gDriver.mShmFd < 0) { perror("voce: shm_open"); return; }

    if (ftruncate(gDriver.mShmFd, sizeof(VoceRingBuffer)) < 0)
        perror("voce: ftruncate");

    gDriver.mRing = mmap(NULL, sizeof(VoceRingBuffer),
                         PROT_READ | PROT_WRITE, MAP_SHARED,
                         gDriver.mShmFd, 0);
    if (gDriver.mRing == MAP_FAILED) {
        perror("voce: mmap");
        gDriver.mRing = NULL;
    }
}

static void ring_close(void) {
    if (gDriver.mRing && gDriver.mRing != MAP_FAILED)
        munmap(gDriver.mRing, sizeof(VoceRingBuffer));
    if (gDriver.mShmFd >= 0)
        close(gDriver.mShmFd);
    gDriver.mRing  = NULL;
    gDriver.mShmFd = -1;
}

static void ring_read(float *dst, uint32_t n_frames) {
    VoceRingBuffer *r = gDriver.mRing;
    if (!r) { memset(dst, 0, n_frames * sizeof(float)); return; }

    uint32_t wp = atomic_load_explicit(&r->write_pos, memory_order_acquire);
    uint32_t rp = atomic_load_explicit(&r->read_pos,  memory_order_relaxed);
    uint32_t avail = (wp - rp + RING_CAP) % RING_CAP;
    uint32_t to_read = (avail < n_frames) ? avail : n_frames;

    for (uint32_t i = 0; i < to_read; i++)
        dst[i] = r->samples[(rp + i) % RING_CAP];
    for (uint32_t i = to_read; i < n_frames; i++)
        dst[i] = 0.0f;

    atomic_store_explicit(&r->read_pos, (rp + to_read) % RING_CAP,
                          memory_order_release);
}

// ─── Host-time helpers ────────────────────────────────────────────────────────

static uint64_t host_ticks_per_frame(void) {
    return (uint64_t)(
        (double)gDriver.mTimebase.numer * 1e9 /
        ((double)gDriver.mTimebase.denom * kSampleRate)
    );
}

// ─── IUnknown ─────────────────────────────────────────────────────────────────

static HRESULT Voce_QueryInterface(void *inDriver, REFIID inUUID,
                                   LPVOID *outInterface) {
    if (!outInterface) return E_POINTER;

    CFUUIDRef uuid = CFUUIDCreateFromUUIDBytes(NULL, inUUID);
    if (CFEqual(uuid, IUnknownUUID) ||
        CFEqual(uuid, kAudioServerPlugInDriverInterfaceUUID)) {
        CFRelease(uuid);
        *outInterface = inDriver;
        return S_OK;
    }
    CFRelease(uuid);
    *outInterface = NULL;
    return E_NOINTERFACE;
}

static ULONG Voce_AddRef(void *inDriver)  { (void)inDriver; return 1; }
static ULONG Voce_Release(void *inDriver) { (void)inDriver; return 1; }

// ─── AudioServerPlugIn: Init / Device lifecycle ───────────────────────────────

static OSStatus Voce_Initialize(
    AudioServerPlugInDriverRef   inDriver,
    AudioServerPlugInHostRef     inHost)
{
    (void)inDriver; (void)inHost;
    pthread_mutex_init(&gDriver.mMutex, NULL);
    mach_timebase_info(&gDriver.mTimebase);
    gDriver.mAnchorHostTime   = mach_absolute_time();
    gDriver.mAnchorSampleTime = 0;
    ring_open();
    return kAudioHardwareNoError;
}

static OSStatus Voce_CreateDevice(
    AudioServerPlugInDriverRef  inDriver,
    CFDictionaryRef             inDescription,
    const AudioServerPlugInClientInfo *inClientInfo,
    AudioObjectID               *outDeviceObjectID)
{
    (void)inDriver; (void)inDescription; (void)inClientInfo;
    if (outDeviceObjectID) *outDeviceObjectID = kObjectID_Device;
    return kAudioHardwareNoError;
}

static OSStatus Voce_DestroyDevice(
    AudioServerPlugInDriverRef inDriver,
    AudioObjectID              inDeviceObjectID)
{
    (void)inDriver; (void)inDeviceObjectID;
    ring_close();
    pthread_mutex_destroy(&gDriver.mMutex);
    return kAudioHardwareNoError;
}

// ─── Property helpers ─────────────────────────────────────────────────────────

static AudioStreamBasicDescription voce_format(void) {
    AudioStreamBasicDescription f = {0};
    f.mSampleRate       = kSampleRate;
    f.mFormatID         = kAudioFormatLinearPCM;
    f.mFormatFlags      = kAudioFormatFlagsNativeFloatPacked |
                          kAudioFormatFlagIsNonInterleaved;
    f.mBytesPerPacket   = sizeof(float);
    f.mFramesPerPacket  = 1;
    f.mBytesPerFrame    = sizeof(float);
    f.mChannelsPerFrame = kNumChannels;
    f.mBitsPerChannel   = 32;
    return f;
}

// ─── HasProperty ─────────────────────────────────────────────────────────────

static Boolean Voce_HasProperty(
    AudioServerPlugInDriverRef inDriver, AudioObjectID inObjectID,
    pid_t inClientPID, const AudioObjectPropertyAddress *inAddress)
{
    (void)inDriver; (void)inClientPID;
    AudioObjectPropertySelector sel = inAddress->mSelector;
    switch (inObjectID) {
        case kObjectID_PlugIn:
            return sel == kAudioObjectPropertyBaseClass     ||
                   sel == kAudioObjectPropertyClass         ||
                   sel == kAudioObjectPropertyOwner         ||
                   sel == kAudioObjectPropertyName          ||
                   sel == kAudioObjectPropertyManufacturer  ||
                   sel == kAudioPlugInPropertyBundleID      ||
                   sel == kAudioPlugInPropertyDeviceList    ||
                   sel == kAudioPlugInPropertyTranslateUIDToDevice;
        case kObjectID_Device:
            return sel == kAudioObjectPropertyBaseClass     ||
                   sel == kAudioObjectPropertyClass         ||
                   sel == kAudioObjectPropertyOwner         ||
                   sel == kAudioObjectPropertyName          ||
                   sel == kAudioObjectPropertyManufacturer  ||
                   sel == kAudioDevicePropertyDeviceUID     ||
                   sel == kAudioDevicePropertyModelUID      ||
                   sel == kAudioDevicePropertyTransportType ||
                   sel == kAudioDevicePropertyRelatedDevices||
                   sel == kAudioDevicePropertyClockDomain   ||
                   sel == kAudioDevicePropertyDeviceIsAlive ||
                   sel == kAudioDevicePropertyDeviceIsRunning||
                   sel == kAudioDevicePropertyDeviceCanBeDefaultDevice ||
                   sel == kAudioDevicePropertyDeviceCanBeDefaultSystemDevice ||
                   sel == kAudioDevicePropertyLatency       ||
                   sel == kAudioDevicePropertyStreams        ||
                   sel == kAudioObjectPropertyControlList   ||
                   sel == kAudioDevicePropertySafetyOffset  ||
                   sel == kAudioDevicePropertyNominalSampleRate ||
                   sel == kAudioDevicePropertyAvailableNominalSampleRates ||
                   sel == kAudioDevicePropertyIsHidden       ||
                   sel == kAudioDevicePropertyZeroTimeStampPeriod;
        case kObjectID_Stream:
            return sel == kAudioObjectPropertyBaseClass     ||
                   sel == kAudioObjectPropertyClass         ||
                   sel == kAudioObjectPropertyOwner         ||
                   sel == kAudioObjectPropertyName          ||
                   sel == kAudioStreamPropertyIsActive      ||
                   sel == kAudioStreamPropertyDirection     ||
                   sel == kAudioStreamPropertyTerminalType  ||
                   sel == kAudioStreamPropertyStartingChannel||
                   sel == kAudioStreamPropertyLatency       ||
                   sel == kAudioStreamPropertyVirtualFormat ||
                   sel == kAudioStreamPropertyPhysicalFormat||
                   sel == kAudioStreamPropertyAvailableVirtualFormats ||
                   sel == kAudioStreamPropertyAvailablePhysicalFormats;
        default:
            return false;
    }
}

// ─── IsPropertySettable ──────────────────────────────────────────────────────

static OSStatus Voce_IsPropertySettable(
    AudioServerPlugInDriverRef inDriver, AudioObjectID inObjectID,
    pid_t inClientPID, const AudioObjectPropertyAddress *inAddress,
    Boolean *outIsSettable)
{
    (void)inDriver; (void)inClientPID;
    if (outIsSettable) *outIsSettable = false;
    if (inObjectID == kObjectID_Stream &&
        (inAddress->mSelector == kAudioStreamPropertyVirtualFormat ||
         inAddress->mSelector == kAudioStreamPropertyPhysicalFormat))
        if (outIsSettable) *outIsSettable = false;
    return kAudioHardwareNoError;
}

// ─── GetPropertyDataSize ─────────────────────────────────────────────────────

static OSStatus Voce_GetPropertyDataSize(
    AudioServerPlugInDriverRef inDriver, AudioObjectID inObjectID,
    pid_t inClientPID, const AudioObjectPropertyAddress *inAddress,
    UInt32 inQualifierDataSize, const void *inQualifierData,
    UInt32 *outDataSize)
{
    (void)inDriver; (void)inClientPID;
    (void)inQualifierDataSize; (void)inQualifierData;
    if (!outDataSize) return kAudioHardwareBadObjectError;

    AudioObjectPropertySelector sel = inAddress->mSelector;
#define SZ(T)   *outDataSize = sizeof(T); return kAudioHardwareNoError
#define SZ_CF   *outDataSize = sizeof(CFStringRef); return kAudioHardwareNoError

    switch (inObjectID) {
        case kObjectID_PlugIn:
            if (sel == kAudioObjectPropertyBaseClass)   { SZ(AudioClassID); }
            if (sel == kAudioObjectPropertyClass)       { SZ(AudioClassID); }
            if (sel == kAudioObjectPropertyOwner)       { SZ(AudioObjectID); }
            if (sel == kAudioObjectPropertyName)        { SZ_CF; }
            if (sel == kAudioObjectPropertyManufacturer){ SZ_CF; }
            if (sel == kAudioPlugInPropertyBundleID)    { SZ_CF; }
            if (sel == kAudioPlugInPropertyDeviceList)  { SZ(AudioObjectID); }
            if (sel == kAudioPlugInPropertyTranslateUIDToDevice) { SZ(AudioObjectID); }
            break;
        case kObjectID_Device:
            if (sel == kAudioObjectPropertyBaseClass)   { SZ(AudioClassID); }
            if (sel == kAudioObjectPropertyClass)       { SZ(AudioClassID); }
            if (sel == kAudioObjectPropertyOwner)       { SZ(AudioObjectID); }
            if (sel == kAudioObjectPropertyName)        { SZ_CF; }
            if (sel == kAudioObjectPropertyManufacturer){ SZ_CF; }
            if (sel == kAudioDevicePropertyDeviceUID)   { SZ_CF; }
            if (sel == kAudioDevicePropertyModelUID)    { SZ_CF; }
            if (sel == kAudioDevicePropertyTransportType)    { SZ(UInt32); }
            if (sel == kAudioDevicePropertyRelatedDevices)   { SZ(AudioObjectID); }
            if (sel == kAudioDevicePropertyClockDomain)      { SZ(UInt32); }
            if (sel == kAudioDevicePropertyDeviceIsAlive)    { SZ(UInt32); }
            if (sel == kAudioDevicePropertyDeviceIsRunning)  { SZ(UInt32); }
            if (sel == kAudioDevicePropertyDeviceCanBeDefaultDevice) { SZ(UInt32); }
            if (sel == kAudioDevicePropertyDeviceCanBeDefaultSystemDevice) { SZ(UInt32); }
            if (sel == kAudioDevicePropertyLatency)          { SZ(UInt32); }
            if (sel == kAudioDevicePropertySafetyOffset)     { SZ(UInt32); }
            if (sel == kAudioDevicePropertyNominalSampleRate){ SZ(Float64); }
            if (sel == kAudioDevicePropertyIsHidden)         { SZ(UInt32); }
            if (sel == kAudioDevicePropertyZeroTimeStampPeriod) { SZ(UInt32); }
            if (sel == kAudioDevicePropertyStreams) {
                AudioObjectPropertyScope scope = inAddress->mScope;
                if (scope == kAudioObjectPropertyScopeInput ||
                    scope == kAudioObjectPropertyScopeGlobal)
                    { SZ(AudioObjectID); }
                *outDataSize = 0; return kAudioHardwareNoError;
            }
            if (sel == kAudioObjectPropertyControlList) { *outDataSize = 0; return kAudioHardwareNoError; }
            if (sel == kAudioDevicePropertyAvailableNominalSampleRates) { SZ(AudioValueRange); }
            if (sel == kAudioDevicePropertyPreferredChannelsForStereo) { *outDataSize = 2 * sizeof(UInt32); return kAudioHardwareNoError; }
            break;
        case kObjectID_Stream:
            if (sel == kAudioObjectPropertyBaseClass)   { SZ(AudioClassID); }
            if (sel == kAudioObjectPropertyClass)       { SZ(AudioClassID); }
            if (sel == kAudioObjectPropertyOwner)       { SZ(AudioObjectID); }
            if (sel == kAudioObjectPropertyName)        { SZ_CF; }
            if (sel == kAudioStreamPropertyIsActive)    { SZ(UInt32); }
            if (sel == kAudioStreamPropertyDirection)   { SZ(UInt32); }
            if (sel == kAudioStreamPropertyTerminalType){ SZ(UInt32); }
            if (sel == kAudioStreamPropertyStartingChannel) { SZ(UInt32); }
            if (sel == kAudioStreamPropertyLatency)     { SZ(UInt32); }
            if (sel == kAudioStreamPropertyVirtualFormat)  { SZ(AudioStreamBasicDescription); }
            if (sel == kAudioStreamPropertyPhysicalFormat) { SZ(AudioStreamBasicDescription); }
            if (sel == kAudioStreamPropertyAvailableVirtualFormats)  { SZ(AudioStreamRangedDescription); }
            if (sel == kAudioStreamPropertyAvailablePhysicalFormats) { SZ(AudioStreamRangedDescription); }
            break;
    }
#undef SZ
#undef SZ_CF
    return kAudioHardwareUnknownPropertyError;
}

// ─── GetPropertyData ─────────────────────────────────────────────────────────

static OSStatus Voce_GetPropertyData(
    AudioServerPlugInDriverRef inDriver, AudioObjectID inObjectID,
    pid_t inClientPID, const AudioObjectPropertyAddress *inAddress,
    UInt32 inQualifierDataSize, const void *inQualifierData,
    UInt32 inDataSize, UInt32 *outDataSize, void *outData)
{
    (void)inDriver; (void)inClientPID;
    (void)inQualifierDataSize; (void)inQualifierData;
    if (!outData || !outDataSize) return kAudioHardwareBadObjectError;

    AudioObjectPropertySelector sel = inAddress->mSelector;

#define SET(T, val) do { *(T*)outData = (val); *outDataSize = sizeof(T); return kAudioHardwareNoError; } while(0)
#define SET_CF(str) do { *(CFStringRef*)outData = CFRetain(str); *outDataSize = sizeof(CFStringRef); return kAudioHardwareNoError; } while(0)

    switch (inObjectID) {
        case kObjectID_PlugIn:
            if (sel == kAudioObjectPropertyBaseClass)    SET(AudioClassID, kAudioPlugInClassID);
            if (sel == kAudioObjectPropertyClass)        SET(AudioClassID, kAudioPlugInClassID);
            if (sel == kAudioObjectPropertyOwner)        SET(AudioObjectID, kAudioObjectUnknown);
            if (sel == kAudioObjectPropertyName)         SET_CF(CFSTR("Voce Audio"));
            if (sel == kAudioObjectPropertyManufacturer) SET_CF(kMfgName);
            if (sel == kAudioPlugInPropertyBundleID)     SET_CF(kPlugInBundleID);
            if (sel == kAudioPlugInPropertyDeviceList)   SET(AudioObjectID, kObjectID_Device);
            if (sel == kAudioPlugInPropertyTranslateUIDToDevice) {
                CFStringRef uid = *(CFStringRef*)inQualifierData;
                AudioObjectID result = CFEqual(uid, kDeviceUID) ? kObjectID_Device : kAudioObjectUnknown;
                SET(AudioObjectID, result);
            }
            break;

        case kObjectID_Device:
            if (sel == kAudioObjectPropertyBaseClass)    SET(AudioClassID, kAudioDeviceClassID);
            if (sel == kAudioObjectPropertyClass)        SET(AudioClassID, kAudioDeviceClassID);
            if (sel == kAudioObjectPropertyOwner)        SET(AudioObjectID, kObjectID_PlugIn);
            if (sel == kAudioObjectPropertyName)         SET_CF(kDeviceName);
            if (sel == kAudioObjectPropertyManufacturer) SET_CF(kMfgName);
            if (sel == kAudioDevicePropertyDeviceUID)    SET_CF(kDeviceUID);
            if (sel == kAudioDevicePropertyModelUID)     SET_CF(CFSTR("com.voce.microphone.model"));
            if (sel == kAudioDevicePropertyTransportType) SET(UInt32, kAudioDeviceTransportTypeVirtual);
            if (sel == kAudioDevicePropertyRelatedDevices) SET(AudioObjectID, kObjectID_Device);
            if (sel == kAudioDevicePropertyClockDomain)   SET(UInt32, 0);
            if (sel == kAudioDevicePropertyDeviceIsAlive) SET(UInt32, 1);
            if (sel == kAudioDevicePropertyDeviceIsRunning) {
                LOCK();
                UInt32 running = gDriver.mDeviceRunning ? 1 : 0;
                UNLOCK();
                SET(UInt32, running);
            }
            if (sel == kAudioDevicePropertyDeviceCanBeDefaultDevice) SET(UInt32, 1);
            if (sel == kAudioDevicePropertyDeviceCanBeDefaultSystemDevice) SET(UInt32, 1);
            if (sel == kAudioDevicePropertyLatency)        SET(UInt32, 0);
            if (sel == kAudioDevicePropertySafetyOffset)   SET(UInt32, 0);
            if (sel == kAudioDevicePropertyNominalSampleRate) SET(Float64, kSampleRate);
            if (sel == kAudioDevicePropertyIsHidden)       SET(UInt32, 0);
            if (sel == kAudioDevicePropertyZeroTimeStampPeriod) SET(UInt32, kBufferFrames);
            if (sel == kAudioDevicePropertyStreams) {
                AudioObjectPropertyScope scope = inAddress->mScope;
                if (scope == kAudioObjectPropertyScopeInput ||
                    scope == kAudioObjectPropertyScopeGlobal)
                    SET(AudioObjectID, kObjectID_Stream);
                *outDataSize = 0;
                return kAudioHardwareNoError;
            }
            if (sel == kAudioObjectPropertyControlList) {
                *outDataSize = 0; return kAudioHardwareNoError;
            }
            if (sel == kAudioDevicePropertyAvailableNominalSampleRates) {
                if (inDataSize >= sizeof(AudioValueRange)) {
                    AudioValueRange *r = (AudioValueRange*)outData;
                    r->mMinimum = kSampleRate;
                    r->mMaximum = kSampleRate;
                    *outDataSize = sizeof(AudioValueRange);
                }
                return kAudioHardwareNoError;
            }
            if (sel == kAudioDevicePropertyPreferredChannelsForStereo) {
                if (inDataSize >= 2 * sizeof(UInt32)) {
                    UInt32 *ch = (UInt32*)outData;
                    ch[0] = 1; ch[1] = 1;
                    *outDataSize = 2 * sizeof(UInt32);
                }
                return kAudioHardwareNoError;
            }
            break;

        case kObjectID_Stream: {
            AudioStreamBasicDescription fmt = voce_format();
            if (sel == kAudioObjectPropertyBaseClass)    SET(AudioClassID, kAudioStreamClassID);
            if (sel == kAudioObjectPropertyClass)        SET(AudioClassID, kAudioStreamClassID);
            if (sel == kAudioObjectPropertyOwner)        SET(AudioObjectID, kObjectID_Device);
            if (sel == kAudioObjectPropertyName)         SET_CF(kStreamName);
            if (sel == kAudioStreamPropertyIsActive)     SET(UInt32, 1);
            if (sel == kAudioStreamPropertyDirection)    SET(UInt32, 1);
            if (sel == kAudioStreamPropertyTerminalType) SET(UInt32, kAudioStreamTerminalTypeMicrophone);
            if (sel == kAudioStreamPropertyStartingChannel) SET(UInt32, 1);
            if (sel == kAudioStreamPropertyLatency)      SET(UInt32, 0);
            if (sel == kAudioStreamPropertyVirtualFormat)  { *(AudioStreamBasicDescription*)outData = fmt; *outDataSize = sizeof(fmt); return kAudioHardwareNoError; }
            if (sel == kAudioStreamPropertyPhysicalFormat) { *(AudioStreamBasicDescription*)outData = fmt; *outDataSize = sizeof(fmt); return kAudioHardwareNoError; }
            if (sel == kAudioStreamPropertyAvailableVirtualFormats ||
                sel == kAudioStreamPropertyAvailablePhysicalFormats) {
                if (inDataSize >= sizeof(AudioStreamRangedDescription)) {
                    AudioStreamRangedDescription *rd = (AudioStreamRangedDescription*)outData;
                    rd->mFormat = fmt;
                    rd->mSampleRateRange.mMinimum = kSampleRate;
                    rd->mSampleRateRange.mMaximum = kSampleRate;
                    *outDataSize = sizeof(AudioStreamRangedDescription);
                }
                return kAudioHardwareNoError;
            }
            break;
        }
    }

#undef SET
#undef SET_CF
    return kAudioHardwareUnknownPropertyError;
}

// ─── SetPropertyData ─────────────────────────────────────────────────────────

static OSStatus Voce_SetPropertyData(
    AudioServerPlugInDriverRef inDriver, AudioObjectID inObjectID,
    pid_t inClientPID, const AudioObjectPropertyAddress *inAddress,
    UInt32 inQualifierDataSize, const void *inQualifierData,
    UInt32 inDataSize, const void *inData)
{
    (void)inDriver; (void)inObjectID; (void)inClientPID; (void)inAddress;
    (void)inQualifierDataSize; (void)inQualifierData; (void)inDataSize; (void)inData;
    return kAudioHardwareNoError;
}

// ─── Device start / stop ─────────────────────────────────────────────────────

static OSStatus Voce_StartIO(
    AudioServerPlugInDriverRef inDriver,
    AudioObjectID inDeviceObjectID,
    UInt32 inClientID)
{
    (void)inDriver; (void)inDeviceObjectID; (void)inClientID;
    LOCK();
    gDriver.mDeviceRunning    = true;
    gDriver.mAnchorSampleTime = 0;
    gDriver.mAnchorHostTime   = mach_absolute_time();
    UNLOCK();
    return kAudioHardwareNoError;
}

static OSStatus Voce_StopIO(
    AudioServerPlugInDriverRef inDriver,
    AudioObjectID inDeviceObjectID,
    UInt32 inClientID)
{
    (void)inDriver; (void)inDeviceObjectID; (void)inClientID;
    LOCK();
    gDriver.mDeviceRunning = false;
    UNLOCK();
    return kAudioHardwareNoError;
}

// ─── IO callbacks ────────────────────────────────────────────────────────────

static OSStatus Voce_GetZeroTimeStamp(
    AudioServerPlugInDriverRef inDriver,
    AudioObjectID inDeviceObjectID,
    UInt32 inClientID,
    Float64 *outSampleTime,
    UInt64  *outHostTime,
    UInt64  *outSeed)
{
    (void)inDriver; (void)inDeviceObjectID; (void)inClientID;
    LOCK();
    *outSampleTime = (Float64)gDriver.mAnchorSampleTime;
    *outHostTime   = gDriver.mAnchorHostTime;
    *outSeed       = 1;
    UNLOCK();
    return kAudioHardwareNoError;
}

static OSStatus Voce_WillDoIOOperation(
    AudioServerPlugInDriverRef inDriver,
    AudioObjectID inDeviceObjectID,
    UInt32 inClientID,
    UInt32 inOperationID,
    Boolean *outWillDo,
    Boolean *outWillDoInPlace)
{
    (void)inDriver; (void)inDeviceObjectID; (void)inClientID;
    *outWillDo        = (inOperationID == kAudioServerPlugInIOOperationReadInput);
    *outWillDoInPlace = true;
    return kAudioHardwareNoError;
}

static OSStatus Voce_BeginIOOperation(
    AudioServerPlugInDriverRef inDriver, AudioObjectID inDeviceObjectID,
    UInt32 inClientID, UInt32 inOperationID,
    UInt32 inIOBufferFrameSize, const AudioServerPlugInIOCycleInfo *inIOCycleInfo)
{
    (void)inDriver;(void)inDeviceObjectID;(void)inClientID;(void)inOperationID;
    (void)inIOBufferFrameSize;(void)inIOCycleInfo;
    return kAudioHardwareNoError;
}

static OSStatus Voce_DoIOOperation(
    AudioServerPlugInDriverRef inDriver,
    AudioObjectID inDeviceObjectID,
    AudioObjectID inStreamObjectID,
    UInt32 inClientID,
    UInt32 inOperationID,
    UInt32 inIOBufferFrameSize,
    const AudioServerPlugInIOCycleInfo *inIOCycleInfo,
    void *ioMainBuffer,
    void *ioSecondaryBuffer)
{
    (void)inDriver;(void)inDeviceObjectID;(void)inStreamObjectID;(void)inClientID;
    (void)inIOCycleInfo;(void)ioSecondaryBuffer;

    if (inOperationID == kAudioServerPlugInIOOperationReadInput && ioMainBuffer)
        ring_read((float*)ioMainBuffer, inIOBufferFrameSize);

    return kAudioHardwareNoError;
}

static OSStatus Voce_EndIOOperation(
    AudioServerPlugInDriverRef inDriver, AudioObjectID inDeviceObjectID,
    UInt32 inClientID, UInt32 inOperationID,
    UInt32 inIOBufferFrameSize, const AudioServerPlugInIOCycleInfo *inIOCycleInfo)
{
    (void)inDriver;(void)inDeviceObjectID;(void)inClientID;(void)inOperationID;
    (void)inIOCycleInfo;

    LOCK();
    gDriver.mAnchorSampleTime += inIOBufferFrameSize;
    gDriver.mAnchorHostTime   += (uint64_t)inIOBufferFrameSize * host_ticks_per_frame();
    UNLOCK();

    return kAudioHardwareNoError;
}

// ─── Bundle entry point ───────────────────────────────────────────────────────

void *AudioServerPlugInBundleEntry(CFAllocatorRef inAllocator, CFUUIDRef inRequestedTypeUUID) {
    (void)inAllocator;

    if (!CFEqual(inRequestedTypeUUID, kAudioServerPlugInTypeUUID))
        return NULL;

    AudioServerPlugInDriverInterface *iface = &gDriver.mInterfaceImpl;
    memset(iface, 0, sizeof(*iface));
    iface->QueryInterface          = Voce_QueryInterface;
    iface->AddRef                  = Voce_AddRef;
    iface->Release                 = Voce_Release;
    iface->Initialize              = Voce_Initialize;
    iface->CreateDevice            = Voce_CreateDevice;
    iface->DestroyDevice           = Voce_DestroyDevice;
    iface->AddDeviceClient         = NULL;
    iface->RemoveDeviceClient      = NULL;
    iface->PerformDeviceConfigurationChange = NULL;
    iface->AbortDeviceConfigurationChange   = NULL;
    iface->HasProperty             = Voce_HasProperty;
    iface->IsPropertySettable      = Voce_IsPropertySettable;
    iface->GetPropertyDataSize     = Voce_GetPropertyDataSize;
    iface->GetPropertyData         = Voce_GetPropertyData;
    iface->SetPropertyData         = Voce_SetPropertyData;
    iface->StartIO                 = Voce_StartIO;
    iface->StopIO                  = Voce_StopIO;
    iface->GetZeroTimeStamp        = Voce_GetZeroTimeStamp;
    iface->WillDoIOOperation       = Voce_WillDoIOOperation;
    iface->BeginIOOperation        = Voce_BeginIOOperation;
    iface->DoIOOperation           = Voce_DoIOOperation;
    iface->EndIOOperation          = Voce_EndIOOperation;

    gDriver.mInterface = iface;
    gDriver.mShmFd     = -1;
    gDriver.mRing      = NULL;

    return &gDriver.mInterface;
}
