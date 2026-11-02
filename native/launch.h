#pragma once
#import <Foundation/Foundation.h>
#import <CoreServices/CoreServices.h>
// Apple marks login-item launches in the open-application Apple Event. The
// property-data enum is used by SMAppService; also accept the documented marker.
static inline BOOL me_is_login_event(NSAppleEventDescriptor *event) {
    if(!event||event.eventClass!=kCoreEventClass||event.eventID!=kAEOpenApplication)return NO;
    if([event paramDescriptorForKeyword:keyAELaunchedAsLogInItem])return YES;
    return [event paramDescriptorForKeyword:keyAEPropData].enumCodeValue==keyAELaunchedAsLogInItem;
}
