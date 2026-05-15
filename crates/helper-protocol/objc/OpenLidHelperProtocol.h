#import <Foundation/Foundation.h>

// The XPC protocol that both the helper (server) and app (client) speak.
// Declared in Objective-C (not built dynamically at runtime) because NSXPC
// requires the Clang-emitted extended type metadata for block reply handlers.
//
// Three production methods mirror openlid_core::ipc::helper::*:
//   - setSleepPreventionEnabled:withReply:
//   - getSleepPreventionStatusWithReply:
//   - pingWithReply:
@protocol OpenLidHelperProtocol
- (void)setSleepPreventionEnabled:(BOOL)enabled
                        withReply:(void (^ _Nonnull)(BOOL ok, NSString * _Nullable error))reply;
- (void)getSleepPreventionStatusWithReply:(void (^ _Nonnull)(BOOL ok, BOOL active, NSString * _Nullable error))reply;
- (void)pingWithReply:(void (^ _Nonnull)(void))reply;
@end

extern Protocol * _Nonnull OpenLidHelperProtocol_get(void);
