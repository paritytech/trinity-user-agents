// Swift has no load-time hook, so +load starts the driver once the app finishes launching, before
// its scene connects. The entry is looked up by name: the generated Swift header would pull in
// every @objc declaration in the app.

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
