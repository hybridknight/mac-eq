// Synthetic HAL buffers only: this executable never opens or captures a device.
#import "../native/audio.m"
#include <assert.h>
static void copyFrames(void *ctx,const float *l,const float *r,uint32_t is,float *ol,float *or,uint32_t os,uint32_t n) {
    (void)ctx;for(uint32_t i=0;i<n;i++){ol[i*os]=l[i*is];or[i*os]=r[i*is];}
}
int main(void) {
    MEEngine e={0};atomic_init(&e.invalid,false);e.process=copyFrames;
    float physical[]={9,9,9,9},tap[]={.1f,.2f,.3f,.4f};
    float out[]={-7,-7,-7,-7,-7,-7};
    struct {UInt32 count;AudioBuffer buffers[2];} input={2,{{2,sizeof(physical),physical},{2,sizeof(tap),tap}}};
    struct {UInt32 count;AudioBuffer buffers[1];} output={1,{{2,4*sizeof(float),out+1}}};
    io(0,NULL,(AudioBufferList *)&input,NULL,(AudioBufferList *)&output,NULL,&e);
    assert(out[0]==-7&&out[5]==-7);for(int i=0;i<4;i++)assert(out[i+1]==tap[i]);
    atomic_store(&e.invalid,true);
    io(0,NULL,(AudioBufferList *)&input,NULL,(AudioBufferList *)&output,NULL,&e);
    for(int i=1;i<5;i++)assert(out[i]==0);
    atomic_store(&e.invalid,false);e.planarIn=true;e.planarOut=true;
    float l[]={.1f,.3f},r[]={.2f,.4f},ol[2]={0},orr[2]={0};
    input.buffers[0]=(AudioBuffer){1,sizeof(l),l};input.buffers[1]=(AudioBuffer){1,sizeof(r),r};
    struct {UInt32 count;AudioBuffer buffers[2];} planar={2,{{1,sizeof(ol),ol},{1,sizeof(orr),orr}}};
    io(0,NULL,(AudioBufferList *)&input,NULL,(AudioBufferList *)&planar,NULL,&e);
    assert(ol[0]==l[0]&&ol[1]==l[1]&&orr[0]==r[0]&&orr[1]==r[1]);
    input.buffers[0].mData=NULL;
    io(0,NULL,(AudioBufferList *)&input,NULL,(AudioBufferList *)&planar,NULL,&e);
    assert(ol[0]==0&&orr[1]==0);
    puts("Native buffer tests passed (interleaved, planar, guards, invalidation, missing input)");
    return 0;
}
