#import <Foundation/Foundation.h>
#import <CoreAudio/CoreAudio.h>
#import <CoreAudio/AudioHardwareTapping.h>
#import <CoreAudio/CATapDescription.h>
#include <unistd.h>
#include <stddef.h>
#include <string.h>
#include <stdatomic.h>

// All control functions run on the app thread. Only io() runs on the HAL thread.
typedef struct { uint32_t id; char uid[512]; char name[512]; double rate; } MEDevice;
typedef void (*MEProcess)(void *, const float *, const float *, uint32_t, float *, float *, uint32_t, uint32_t);
typedef struct { AudioObjectID tap, aggregate; AudioDeviceIOProcID proc; MEProcess process; void *context; bool planarIn, planarOut; double rate; AudioObjectID device, stream; atomic_bool invalid; UInt32 listeners; } MEEngine;
static OSStatus readProp(AudioObjectID id, AudioObjectPropertySelector sel, AudioObjectPropertyScope scope, UInt32 *size, void *data) {
    AudioObjectPropertyAddress a={sel,scope,kAudioObjectPropertyElementMain};
    return AudioObjectGetPropertyData(id,&a,0,NULL,size,data);
}
static NSArray<NSNumber *> *streams(AudioObjectID id, AudioObjectPropertyScope scope) {
    AudioObjectPropertyAddress a={kAudioDevicePropertyStreams,scope,kAudioObjectPropertyElementMain}; UInt32 size=0;
    if(AudioObjectGetPropertyDataSize(id,&a,0,NULL,&size)) return nil;
    NSMutableData *data=[NSMutableData dataWithLength:size];
    if(AudioObjectGetPropertyData(id,&a,0,NULL,&size,data.mutableBytes)) return nil;
    NSMutableArray *result=[NSMutableArray array]; AudioStreamID *s=data.mutableBytes;
    for(UInt32 i=0;i<size/sizeof(AudioStreamID);i++) [result addObject:@(s[i])];
    return result;
}
static NSString *stringProp(AudioObjectID id,AudioObjectPropertySelector sel) {
    CFStringRef str=NULL; UInt32 size=sizeof(str); if(readProp(id,sel,kAudioObjectPropertyScopeGlobal,&size,&str)||!str) return nil;
    return CFBridgingRelease(str);
}
int me_devices(MEDevice *out,int capacity) { @autoreleasepool {
    AudioObjectPropertyAddress a={kAudioHardwarePropertyDevices,kAudioObjectPropertyScopeGlobal,kAudioObjectPropertyElementMain}; UInt32 size=0;
    if(AudioObjectGetPropertyDataSize(kAudioObjectSystemObject,&a,0,NULL,&size)) return -1;
    NSMutableData *data=[NSMutableData dataWithLength:size]; if(AudioObjectGetPropertyData(kAudioObjectSystemObject,&a,0,NULL,&size,data.mutableBytes)) return -1;
    AudioObjectID *ids=data.mutableBytes; int count=0;
    for(UInt32 i=0;i<size/sizeof(AudioObjectID)&&count<capacity;i++) {
        NSArray *s=streams(ids[i],kAudioObjectPropertyScopeOutput); if(!s.count) continue;
        NSString *uid=stringProp(ids[i],kAudioDevicePropertyDeviceUID); NSString *name=stringProp(ids[i],kAudioObjectPropertyName);
        if(!uid||!name||[uid hasPrefix:@"MacEQ-"])continue;
        if([uid lengthOfBytesUsingEncoding:NSUTF8StringEncoding]>=512 || [name lengthOfBytesUsingEncoding:NSUTF8StringEncoding]>=512)continue;
        MEDevice *d=&out[count++]; memset(d,0,sizeof(*d)); d->id=ids[i];
        [uid getCString:d->uid maxLength:512 encoding:NSUTF8StringEncoding]; [name getCString:d->name maxLength:512 encoding:NSUTF8StringEncoding];
        UInt32 n=sizeof(d->rate); readProp(ids[i],kAudioDevicePropertyNominalSampleRate,kAudioObjectPropertyScopeGlobal,&n,&d->rate);
    } return count;
} }
static bool stereoFloat(AudioObjectID id, AudioObjectPropertyScope scope, AudioStreamBasicDescription *format) {
    NSArray<NSNumber *> *s=streams(id,scope); if(s.count!=1) return false; UInt32 n=sizeof(*format);
    if(readProp(s[0].unsignedIntValue,kAudioStreamPropertyVirtualFormat,kAudioObjectPropertyScopeGlobal,&n,format))return false;
    bool planar=(format->mFormatFlags&kAudioFormatFlagIsNonInterleaved)!=0;
    return format->mFormatID==kAudioFormatLinearPCM && (format->mFormatFlags&kAudioFormatFlagIsFloat) && !(format->mFormatFlags&kAudioFormatFlagIsBigEndian) && format->mBitsPerChannel==32 && format->mChannelsPerFrame==2 && format->mBytesPerFrame==(planar?4:8) && format->mSampleRate>=8000 && format->mSampleRate<=384000;
}
int me_supported(uint32_t device) { AudioStreamBasicDescription f; return stereoFloat(device,kAudioObjectPropertyScopeOutput,&f); }
static OSStatus changed(AudioObjectID id, UInt32 count, const AudioObjectPropertyAddress *addresses, void *context) {
    (void)id;(void)count;(void)addresses;
    atomic_store_explicit(&((MEEngine *)context)->invalid,true,memory_order_relaxed);
    return noErr;
}
static AudioObjectPropertyAddress rateAddress={kAudioDevicePropertyNominalSampleRate,kAudioObjectPropertyScopeGlobal,kAudioObjectPropertyElementMain};
static AudioObjectPropertyAddress aliveAddress={kAudioDevicePropertyDeviceIsAlive,kAudioObjectPropertyScopeGlobal,kAudioObjectPropertyElementMain};
static AudioObjectPropertyAddress formatAddress={kAudioStreamPropertyVirtualFormat,kAudioObjectPropertyScopeGlobal,kAudioObjectPropertyElementMain};
int me_valid(MEEngine *e) { return e&&!atomic_load_explicit(&e->invalid,memory_order_relaxed); }
static OSStatus io(AudioObjectID device,const AudioTimeStamp *now,const AudioBufferList *input,const AudioTimeStamp *it,AudioBufferList *output,const AudioTimeStamp *ot,void *opaque) {
    (void)device;(void)now;(void)it;(void)ot;
    MEEngine *e=opaque;
    for(UInt32 b=0;b<output->mNumberBuffers;b++)if(output->mBuffers[b].mData)memset(output->mBuffers[b].mData,0,output->mBuffers[b].mDataByteSize);
    if(!me_valid(e))return noErr;
    UInt32 ni=e->planarIn?2:1, no=e->planarOut?2:1;
    if(input->mNumberBuffers<ni||output->mNumberBuffers!=no)return noErr;
    // The aggregate contains physical inputs first and the tap last. Disabled
    // physical inputs may be represented by null buffers; never select them.
    const AudioBuffer *il=&input->mBuffers[input->mNumberBuffers-ni], *ir=e->planarIn?il+1:il;
    AudioBuffer *ol=&output->mBuffers[0], *orr=e->planarOut?ol+1:ol;
    UInt32 is=e->planarIn?1:2, os=e->planarOut?1:2;
    if(il->mNumberChannels!=is||ir->mNumberChannels!=is||ol->mNumberChannels!=os||orr->mNumberChannels!=os||!il->mData||!ir->mData||!ol->mData||!orr->mData)return noErr;
    UInt32 n=il->mDataByteSize/(4*is); n=MIN(n,ir->mDataByteSize/(4*is)); n=MIN(n,ol->mDataByteSize/(4*os)); n=MIN(n,orr->mDataByteSize/(4*os));
    e->process(e->context,il->mData,(float *)ir->mData+(e->planarIn?0:1),is,ol->mData,(float *)orr->mData+(e->planarOut?0:1),os,n);
    return noErr;
}
// A removed device/stream no longer owns listeners. Clear each registration
// independently so partial cleanup can safely be retried after hot-unplug.
static bool removeListeners(MEEngine *e) {
    AudioObjectID objects[3]={e->device,e->device,e->stream};
    AudioObjectPropertyAddress *addresses[3]={&rateAddress,&aliveAddress,&formatAddress};
    for(UInt32 i=0;i<3;i++) {
        UInt32 bit=1u<<i;
        if(!(e->listeners&bit))continue;
        OSStatus status=AudioObjectRemovePropertyListener(objects[i],addresses[i],changed,e);
        if(status==noErr||status==kAudioHardwareBadObjectError)e->listeners&=~bit;
    }
    return e->listeners==0;
}
// Return false rather than freeing callback storage if HAL fails to quiesce it.
int me_stop(MEEngine *e) {
    if(!e)return 1;
    if(e->proc) {
        OSStatus stopped=AudioDeviceStop(e->aggregate,e->proc);
        if(stopped!=noErr&&stopped!=kAudioHardwareNotRunningError)return 0;
        if(AudioDeviceDestroyIOProcID(e->aggregate,e->proc))return 0;
        e->proc=NULL;
    }
    if(!removeListeners(e))return 0;
    if(e->aggregate){ if(AudioHardwareDestroyAggregateDevice(e->aggregate))return 0; e->aggregate=0; }
    if(e->tap){ if(AudioHardwareDestroyProcessTap(e->tap))return 0; e->tap=0; }
    free(e);return 1;
}
MEEngine *me_start(uint32_t device,MEProcess process,void *context,double *rate,char *error,size_t errorLen) { @autoreleasepool {
    MEEngine *e=calloc(1,sizeof(*e)); OSStatus status=0; NSString *why=@"Audio setup failed"; NSString *uid=nil;
    e->process=process;e->context=context; e->device=device;atomic_init(&e->invalid,false);
    AudioStreamBasicDescription format={0};
    if(!stereoFloat(device,kAudioObjectPropertyScopeOutput,&format)){why=@"Choose an output with one stereo 32-bit float PCM stream. Multichannel and encoded outputs are not supported yet.";goto fail;}
    UInt32 pair[2]={1,2}, pairSize=sizeof(pair);
    if(!readProp(device,kAudioDevicePropertyPreferredChannelsForStereo,kAudioObjectPropertyScopeOutput,&pairSize,pair) && (pair[0]!=1||pair[1]!=2)){why=@"This output's preferred speaker mapping is unsupported.";goto fail;}
    if(format.mSampleRate!=*rate){why=@"Output sample rate changed; retry Start.";goto fail;}
    e->planarOut=(format.mFormatFlags&kAudioFormatFlagIsNonInterleaved)!=0; e->rate=format.mSampleRate;
    uid=stringProp(device,kAudioDevicePropertyDeviceUID); if(!uid){why=@"Selected output disappeared.";goto fail;}
    pid_t pid=getpid(); AudioObjectID own=0; UInt32 n=sizeof(own); AudioObjectPropertyAddress address={kAudioHardwarePropertyTranslatePIDToProcessObject,kAudioObjectPropertyScopeGlobal,kAudioObjectPropertyElementMain};
    status=AudioObjectGetPropertyData(kAudioObjectSystemObject,&address,sizeof(pid),&pid,&n,&own);
    if(status||own==kAudioObjectUnknown){why=@"Could not exclude MacEQ from capture.";goto fail;}
    {
        CATapDescription *tap=[[CATapDescription alloc] initExcludingProcesses:@[@(own)] andDeviceUID:uid withStream:0];
        tap.privateTap=YES;tap.muteBehavior=CATapMutedWhenTapped;tap.name=@"MacEQ selected output";
        status=AudioHardwareCreateProcessTap(tap,&e->tap);
        if(status){why=@"Could not capture the selected output. Check System Audio Recording permission.";goto fail;}
        AudioStreamBasicDescription tf={0};n=sizeof(tf);status=readProp(e->tap,kAudioTapPropertyFormat,kAudioObjectPropertyScopeGlobal,&n,&tf);
        bool planar=(tf.mFormatFlags&kAudioFormatFlagIsNonInterleaved)!=0;
        if(status||tf.mFormatID!=kAudioFormatLinearPCM||!(tf.mFormatFlags&kAudioFormatFlagIsFloat)||(tf.mFormatFlags&kAudioFormatFlagIsBigEndian)||tf.mBitsPerChannel!=32||tf.mChannelsPerFrame!=2||tf.mBytesPerFrame!=(planar?4:8)||tf.mSampleRate!=e->rate){why=@"Unsupported tap format or sample-rate mismatch.";goto fail;}
        e->planarIn=planar;
        NSDictionary *description=@{@kAudioAggregateDeviceNameKey:@"MacEQ Private Output",@kAudioAggregateDeviceUIDKey:[@"MacEQ-" stringByAppendingString:NSUUID.UUID.UUIDString],@kAudioAggregateDeviceIsPrivateKey:@YES,@kAudioAggregateDeviceMainSubDeviceKey:uid,@kAudioAggregateDeviceSubDeviceListKey:@[@{@kAudioSubDeviceUIDKey:uid}],@kAudioAggregateDeviceTapListKey:@[@{@kAudioSubTapUIDKey:tap.UUID.UUIDString,@kAudioSubTapDriftCompensationKey:@YES}],@kAudioAggregateDeviceTapAutoStartKey:@YES};
        status=AudioHardwareCreateAggregateDevice((__bridge CFDictionaryRef)description,&e->aggregate);
        if(status){why=@"Could not create the private output aggregate.";goto fail;}
    }
    {
        AudioStreamBasicDescription af={0};
        if(!stereoFloat(e->aggregate,kAudioObjectPropertyScopeOutput,&af)||af.mSampleRate!=e->rate||((af.mFormatFlags&kAudioFormatFlagIsNonInterleaved)!=0)!=e->planarOut){why=@"Aggregate output format does not match selected device.";goto fail;}
    }
    status=AudioDeviceCreateIOProcID(e->aggregate,io,e,&e->proc);if(status){why=@"Could not create the audio callback.";goto fail;}
    {
        NSArray *ins=streams(e->aggregate,kAudioObjectPropertyScopeInput); if(!ins.count){why=@"Aggregate has no tap input.";goto fail;}
        size_t bytes=offsetof(AudioHardwareIOProcStreamUsage,mStreamIsOn)+ins.count*sizeof(UInt32);
        AudioHardwareIOProcStreamUsage *usage=calloc(1,bytes); usage->mIOProc=(void *)e->proc;usage->mNumberStreams=(UInt32)ins.count;usage->mStreamIsOn[ins.count-1]=1;
        AudioObjectPropertyAddress a={kAudioDevicePropertyIOProcStreamUsage,kAudioObjectPropertyScopeInput,kAudioObjectPropertyElementMain};status=AudioObjectSetPropertyData(e->aggregate,&a,0,NULL,(UInt32)bytes,usage);free(usage);
        if(status){why=@"Could not isolate the tap from physical input streams.";goto fail;}
    }
    {
        NSArray *current=streams(device,kAudioObjectPropertyScopeOutput);
        if(current.count!=1){why=@"Selected output disconnected during setup.";goto fail;}
        e->stream=[current[0] unsignedIntValue];
    }
    // Register independently, then explicitly remove any partial registration on failure.
    status=AudioObjectAddPropertyListener(device,&rateAddress,changed,e);
    if(status){why=@"Could not monitor output sample rate.";goto fail;}
    e->listeners|=1;
    status=AudioObjectAddPropertyListener(device,&aliveAddress,changed,e);
    if(status){why=@"Could not monitor output connection.";goto fail;}
    e->listeners|=2;
    status=AudioObjectAddPropertyListener(e->stream,&formatAddress,changed,e);
    if(status){why=@"Could not monitor output format.";goto fail;}
    e->listeners|=4;
    *rate=e->rate;
    status=AudioDeviceStart(e->aggregate,e->proc);if(status){why=@"Could not start audio. Allow MacEQ in Privacy & Security → Screen & System Audio Recording.";goto fail;}
    return e;
fail:
    snprintf(error,errorLen,"%s (Core Audio %d)",why.UTF8String,(int)status);
    // Failed setup has not published the callback context to a running IOProc.
    bool listenersRemoved=removeListeners(e);
    if(e->proc)AudioDeviceDestroyIOProcID(e->aggregate,e->proc);
    if(e->aggregate)AudioHardwareDestroyAggregateDevice(e->aggregate);
    if(e->tap)AudioHardwareDestroyProcessTap(e->tap);
    // A failed listener removal must not leave a callback pointing at freed storage.
    if(listenersRemoved)free(e);return NULL;
} }
