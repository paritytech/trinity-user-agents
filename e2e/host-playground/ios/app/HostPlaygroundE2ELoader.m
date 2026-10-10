// Starts the host-playground driver in the e2e build of the app.
//
// Swift has no load-time hook, so this class's +load stands in for one. It
// waits for the app to finish launching, which is before its scene connects,
// and then calls the Swift entry by its Objective-C name. The name is resolved
// at runtime rather than through the app's generated Swift header, which would
// pull every @objc declaration in the app into this file.

#import <TargetConditionals.h>

#if TARGET_OS_SIMULATOR

#import <UIKit/UIKit.h>

@protocol HostPlaygroundE2EEntry <NSObject>
+ (void)install;
@end

@interface HostPlaygroundE2ELoader : NSObject
@end

@implementation HostPlaygroundE2ELoader

+ (void)load {
    __block id observer = [NSNotificationCenter.defaultCenter
        addObserverForName:UIApplicationDidFinishLaunchingNotification
                    object:nil
                     queue:NSOperationQueue.mainQueue
                usingBlock:^(NSNotification *notification) {
                    [NSNotificationCenter.defaultCenter removeObserver:observer];
                    Class<HostPlaygroundE2EEntry> entry = (Class<HostPlaygroundE2EEntry>)NSClassFromString(@"HostPlaygroundE2EEntry");
                    [entry install];
                }];
}

@end

#endif
