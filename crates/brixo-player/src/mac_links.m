// macOS hands brixo:// links to the app as an Apple Event ("get URL"), not
// as a command-line argument, and winit doesn't listen for those. This
// catches them and passes each link to Rust (see mac_links.rs).
#import <Foundation/Foundation.h>

static void (*brixo_on_link)(const char *) = NULL;

@interface BrixoLinkCatcher : NSObject
@end

@implementation BrixoLinkCatcher
- (void)handleGetURL:(NSAppleEventDescriptor *)event withReplyEvent:(NSAppleEventDescriptor *)reply {
    // '----' is the event's main parameter: the link itself.
    NSString *url = [[event paramDescriptorForKeyword:0x2D2D2D2D] stringValue];
    if (url != nil && brixo_on_link != NULL) {
        brixo_on_link([url UTF8String]);
    }
}
@end

void brixo_listen_for_links(void (*on_link)(const char *)) {
    static BrixoLinkCatcher *catcher = nil;
    brixo_on_link = on_link;
    catcher = [BrixoLinkCatcher new];
    // 'GURL' / 'GURL': the internet event class and its "get URL" event.
    [[NSAppleEventManager sharedAppleEventManager] setEventHandler:catcher
                                                       andSelector:@selector(handleGetURL:withReplyEvent:)
                                                     forEventClass:0x4755524C
                                                        andEventID:0x4755524C];
}
