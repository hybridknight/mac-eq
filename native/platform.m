#import <AppKit/AppKit.h>
#import <ServiceManagement/ServiceManagement.h>
#include <stdint.h>
#include <stdio.h>
#include <malloc/malloc.h>
#include "maceq-menu-icon.h"
#import "launch.h"
#ifdef ME_MEMORY_PROFILE
#include <mach/mach.h>
#endif
// Installed before winit registers its application delegate. Capture the launch
// event during didFinishLaunching, while currentAppleEvent still refers to it.
static BOOL MEForceBackground=NO;
static int MELaunchMode=0; // 0 pending, 1 normal, 2 login/background
@interface MELaunchObserver : NSObject
- (void)didFinish:(NSNotification *)notification;
@end
@implementation MELaunchObserver
- (void)didFinish:(NSNotification *)notification {
    (void)notification;
    MELaunchMode=(MEForceBackground||me_is_login_event(NSAppleEventManager.sharedAppleEventManager.currentAppleEvent))?2:1;
    [NSNotificationCenter.defaultCenter removeObserver:self];
}
@end
static MELaunchObserver *MELaunchCapture;
void me_prepare_launch(int background) {
    MEForceBackground=background!=0;MELaunchMode=MEForceBackground?2:0;
    MELaunchCapture=[MELaunchObserver new];
    [NSNotificationCenter.defaultCenter addObserver:MELaunchCapture selector:@selector(didFinish:) name:NSApplicationDidFinishLaunchingNotification object:nil];
}
int me_start_in_background(void) {
    if(MELaunchMode==0&&me_is_login_event(NSAppleEventManager.sharedAppleEventManager.currentAppleEvent))MELaunchMode=2;
    return MELaunchMode==2;
}
typedef void (*MEWake)(void *);
@interface MEStatus : NSObject
@property NSStatusItem *item;
@property NSMenuItem *summary;
@property NSMenuItem *devices;
@property NSMenuItem *login;
@property NSTimer *timer;
@property uint32_t events;
@property BOOL editorHidden;
@property (weak) NSWindow *compactWindow;
@property NSRect editorFrame;
@property NSSize editorMinSize;
@property NSSize editorContentMinSize;
@property unsigned compactTicks;
@property NSString *uid;
@property MEWake wake;
@property void *context;
- (void)hideEditor;
- (void)restoreEditor;
@end
@implementation MEStatus
- (void)signal:(uint32_t)event {self.events|=event;if(self.wake)self.wake(self.context);}
- (void)show:(id)sender {(void)sender;[self signal:1];}
- (void)quit:(id)sender {(void)sender;[self signal:2];}
- (void)loginAction:(id)sender {(void)sender;[self signal:8];}
- (void)deviceAction:(NSMenuItem *)sender {self.uid=sender.representedObject;[self signal:16];}
- (void)hideEditor {
    for(NSWindow *window in NSApp.windows)if([window.title isEqualToString:@"MacEQ"]) {
        // eframe forces the first frame visible. Alpha also prevents a flash
        // during background startup, before the next native timer pulse.
        window.alphaValue=0.;if(window.visible)[window orderOut:nil];
        if(self.compactWindow!=window) {
            self.editorFrame=window.frame;self.editorMinSize=window.minSize;
            self.editorContentMinSize=window.contentMinSize;self.compactWindow=window;
            // Order-out alone retains full Retina GL/WindowServer surfaces.
            // Resize once so the renderer can release those large drawables.
            window.minSize=NSZeroSize;window.contentMinSize=NSZeroSize;
            [window setContentSize:NSMakeSize(16,16)];
            self.compactTicks=4;
        }
    }
}
- (void)restoreEditor {
    NSWindow *window=self.compactWindow;
    if(window) {
        window.minSize=self.editorMinSize;window.contentMinSize=self.editorContentMinSize;
        [window setFrame:self.editorFrame display:NO];self.compactWindow=nil;
    }
    self.compactTicks=0;
    for(NSWindow *editor in NSApp.windows)if([editor.title isEqualToString:@"MacEQ"])editor.alphaValue=1.;
}
- (void)pulse:(NSTimer *)timer {
    (void)timer;
    if(self.editorHidden) {
        [self hideEditor];
        // Give the resize/render pass time to free temporary editor buffers,
        // then return unused allocator pages to macOS once per hide.
        if(self.compactTicks&&--self.compactTicks==0)malloc_zone_pressure_relief(NULL,0);
    }
    if(self.wake)self.wake(self.context);
}
@end
static NSMenuItem *action(NSString *title,SEL selector,NSString *key,MEStatus *owner) {
    NSMenuItem *item=[[NSMenuItem alloc]initWithTitle:title action:selector keyEquivalent:key];item.target=owner;return item;
}
void *me_platform_create(MEWake wake,void *context) { @autoreleasepool {
    MEStatus *owner=[MEStatus new];owner.wake=wake;owner.context=context;
    owner.item=[NSStatusBar.systemStatusBar statusItemWithLength:NSSquareStatusItemLength];
    // Embed the authored SVG so the same custom mark works in a bundle or cargo run.
    NSImage *image=[[NSImage alloc]initWithData:[NSData dataWithBytes:MacEQMenuBarSVG length:sizeof(MacEQMenuBarSVG)]];
    if(!image){NSString *path=[NSBundle.mainBundle pathForResource:@"MenuBarTemplate" ofType:@"png"];if(path)image=[[NSImage alloc]initWithContentsOfFile:path];}
    image.size=NSMakeSize(18,18);image.template=YES;
    owner.item.button.image=image;owner.item.button.toolTip=@"MacEQ — device equalizer";
    [owner.item.button setAccessibilityLabel:@"MacEQ"];
    NSString *appIconName=[NSBundle.mainBundle objectForInfoDictionaryKey:@"CFBundleIconFile"];
    NSString *appIcon=appIconName?[NSBundle.mainBundle.resourcePath stringByAppendingPathComponent:appIconName]:nil;
    if(appIcon){NSImage *appImage=[[NSImage alloc]initWithContentsOfFile:appIcon];if(appImage)NSApp.applicationIconImage=appImage;}
    NSMenu *menu=[NSMenu new];menu.autoenablesItems=NO;
    owner.summary=[[NSMenuItem alloc]initWithTitle:@"MacEQ" action:nil keyEquivalent:@""];owner.summary.enabled=NO;[menu addItem:owner.summary];
    [menu addItem:action(@"Open MacEQ",@selector(show:),@"o",owner)];
    owner.devices=[[NSMenuItem alloc]initWithTitle:@"EQ by device" action:nil keyEquivalent:@""];owner.devices.submenu=[NSMenu new];[menu addItem:owner.devices];
    owner.login=action(@"Open at login",@selector(loginAction:),@"",owner);[menu addItem:owner.login];
    [menu addItem:NSMenuItem.separatorItem];[menu addItem:action(@"Quit MacEQ",@selector(quit:),@"q",owner)];
    owner.item.menu=menu;
    // Route Command-Q through Rust cleanup; closing the window only hides it.
    NSMenu *main=NSApp.mainMenu;if(!main){main=[NSMenu new];NSApp.mainMenu=main;}
    if(main.numberOfItems==0){NSMenuItem *root=[NSMenuItem new];root.submenu=[[NSMenu alloc]initWithTitle:@"MacEQ"];[main addItem:root];}
    NSMenu *appMenu=[main itemAtIndex:0].submenu;
    for(NSMenuItem *existing in appMenu.itemArray)if([existing.keyEquivalent isEqualToString:@"q"])[appMenu removeItem:existing];
    [appMenu addItem:action(@"Open MacEQ",@selector(show:),@"o",owner)];
    [appMenu addItem:action(@"Quit MacEQ",@selector(quit:),@"q",owner)];
    owner.timer=[NSTimer timerWithTimeInterval:0.25 target:owner selector:@selector(pulse:) userInfo:nil repeats:YES];
    [NSRunLoop.mainRunLoop addTimer:owner.timer forMode:NSRunLoopCommonModes];
    return (__bridge_retained void *)owner;
} }
uint32_t me_platform_poll(void *handle,char *uid,size_t length) {
    MEStatus *owner=(__bridge MEStatus *)handle;uint32_t events=owner.events;owner.events=0;
    snprintf(uid,length,"%s",owner.uid.UTF8String?:"");owner.uid=nil;return events;
}
void me_platform_sync(void *handle,const char *json,int active,int enabled,int login) { @autoreleasepool {
    MEStatus *owner=(__bridge MEStatus *)handle;
    owner.summary.title=[NSString stringWithFormat:@"MacEQ · %d active / %d enabled",active,enabled];
    owner.item.button.toolTip=owner.summary.title;owner.login.state=(login==1||login==2)?NSControlStateValueOn:NSControlStateValueOff;
    if(!json)return;
    NSData *data=[[NSString stringWithUTF8String:json]dataUsingEncoding:NSUTF8StringEncoding];
    NSArray *rows=[NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
    NSMenu *menu=[NSMenu new];menu.autoenablesItems=NO;
    for(NSDictionary *row in rows) {
        NSString *name=row[@"name"];if(![row[@"connected"]boolValue])name=[name stringByAppendingString:@" (disconnected)"];
        NSMenuItem *item=action(name,@selector(deviceAction:),@"",owner);item.representedObject=row[@"uid"];
        item.state=[row[@"enabled"]boolValue]?NSControlStateValueOn:NSControlStateValueOff;[menu addItem:item];
    }
    if(rows.count==0){NSMenuItem *item=[[NSMenuItem alloc]initWithTitle:@"No output devices" action:nil keyEquivalent:@""];item.enabled=NO;[menu addItem:item];}
    owner.devices.submenu=menu;
} }
void me_platform_destroy(void *handle) {
    MEStatus *owner=CFBridgingRelease(handle);[owner.timer invalidate];owner.timer=nil;
    owner.wake=NULL;owner.context=NULL;[NSStatusBar.systemStatusBar removeStatusItem:owner.item];
}
// Keep the NSStatusItem and timer alive while making the hidden editor an
// accessory app. Restore ordinary Dock/window behavior before showing it again.
int me_set_editor_visible(void *handle,int visible) {
    MEStatus *owner=(__bridge MEStatus *)handle;
    owner.editorHidden=!visible;
    if(!visible)[owner hideEditor];
    NSApplicationActivationPolicy policy=visible?NSApplicationActivationPolicyRegular:NSApplicationActivationPolicyAccessory;
    if(![NSApp setActivationPolicy:policy])return 0;
    if(visible) {
        [owner restoreEditor];
        [NSApp activateIgnoringOtherApps:YES];
    }
    return 1;
}
int me_login_status(void) {return (int)SMAppService.mainAppService.status;}
int me_set_login(int enabled,char *error,size_t length) { @autoreleasepool {
    SMAppService *service=SMAppService.mainAppService;NSError *failure=nil;
    if(enabled&&(service.status==SMAppServiceStatusEnabled||service.status==SMAppServiceStatusRequiresApproval))return 1;
    if(!enabled&&service.status==SMAppServiceStatusNotRegistered)return 1;
    BOOL ok=enabled?[service registerAndReturnError:&failure]:[service unregisterAndReturnError:&failure];
    if(!ok)snprintf(error,length,"%s",failure.localizedDescription.UTF8String?:"Could not change login setting");
    return ok?1:0;
} }
void me_open_login_settings(void) {[SMAppService openSystemSettingsLoginItems];}
#ifdef ME_MEMORY_PROFILE
// Compiled only into the isolated benchmark, never the shipped app.
int me_memory_snapshot(uint32_t phase) {
    task_vm_info_data_t info={0};mach_msg_type_number_t count=TASK_VM_INFO_COUNT;
    if(task_info(mach_task_self(),TASK_VM_INFO,(task_info_t)&info,&count)!=KERN_SUCCESS)return 0;
    for(NSWindow *window in NSApp.windows)if([window.title isEqualToString:@"MacEQ"]) {
        NSSize size=window.contentView.bounds.size;
        printf("memory phase=%u footprint=%.1f MiB content=%.0fx%.0f\n",phase,(double)info.phys_footprint/(1024.*1024.),size.width,size.height);fflush(stdout);
        if(phase%2==0)return size.width>=1000&&size.height>=740&&window.visible&&window.alphaValue==1.&&NSApp.activationPolicy==NSApplicationActivationPolicyRegular;
        return size.width<=32&&size.height<=32&&!window.visible&&window.alphaValue==0.&&NSApp.activationPolicy==NSApplicationActivationPolicyAccessory;
    }
    return 0;
}
#endif
