// Synthetic launch Apple Events only; does not start an app or alter Login Items.
#import "../native/launch.h"
#include <assert.h>
static NSAppleEventDescriptor *event(AEEventID identifier) {
    return [NSAppleEventDescriptor appleEventWithEventClass:kCoreEventClass eventID:identifier targetDescriptor:nil returnID:kAutoGenerateReturnID transactionID:kAnyTransactionID];
}
int main(void) { @autoreleasepool {
    assert(!me_is_login_event(nil));
    NSAppleEventDescriptor *normal=event(kAEOpenApplication);
    assert(!me_is_login_event(normal));
    NSAppleEventDescriptor *login=event(kAEOpenApplication);
    [login setParamDescriptor:[NSAppleEventDescriptor descriptorWithEnumCode:keyAELaunchedAsLogInItem] forKeyword:keyAEPropData];
    assert(me_is_login_event(login));
    NSAppleEventDescriptor *marked=event(kAEOpenApplication);
    [marked setParamDescriptor:[NSAppleEventDescriptor descriptorWithBoolean:YES] forKeyword:keyAELaunchedAsLogInItem];
    assert(me_is_login_event(marked));
    NSAppleEventDescriptor *other=event(kAEReopenApplication);
    [other setParamDescriptor:[NSAppleEventDescriptor descriptorWithEnumCode:keyAELaunchedAsLogInItem] forKeyword:keyAEPropData];
    assert(!me_is_login_event(other));
    NSAppleEventDescriptor *unrelated=event(kAEOpenApplication);
    [unrelated setParamDescriptor:[NSAppleEventDescriptor descriptorWithEnumCode:'test'] forKeyword:keyAEPropData];
    assert(!me_is_login_event(unrelated));
    puts("Native launch tests passed (normal, login enum, login marker, reopen, unrelated event)");
    return 0;
} }
