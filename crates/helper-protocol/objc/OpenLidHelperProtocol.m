#import "OpenLidHelperProtocol.h"

// `@protocol(...)` is what causes Clang to emit the protocol metadata
// (including the extended block signature information that NSXPC requires).
// Without this, NSXPC throws:
//   "NSInvalidArgumentException: NSXPCInterface: Unable to get extended method
//    signature from Protocol data ... Use of clang is required for NSXPCInterface."
Protocol *OpenLidHelperProtocol_get(void) {
    return @protocol(OpenLidHelperProtocol);
}
