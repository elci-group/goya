#import "GoyaClipboard.h"
#import <AppKit/AppKit.h>

@implementation GoyaClipboard
+ (GoyaClipboardResult)exportText:(NSString *)text maxBytes:(NSUInteger)maxBytes admit:(GoyaClipboardAdmission)admit commit:(nullable GoyaClipboardCommit)commit {
    if (![NSThread isMainThread] || !admit || !text || maxBytes == 0) return GoyaClipboardDenied;
    if (!admit()) return GoyaClipboardDenied;
    if (text.length == 0) return GoyaClipboardNoChange;
    if (![text canBeConvertedToEncoding:NSUTF8StringEncoding]) return GoyaClipboardInvalidText;
    NSUInteger length = [text lengthOfBytesUsingEncoding:NSUTF8StringEncoding];
    if (length == 0) return GoyaClipboardInvalidText;
    if (length > MIN(maxBytes, (NSUInteger)(1024 * 1024))) return GoyaClipboardTooLarge;
    NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding allowLossyConversion:NO];
    if (!data) return GoyaClipboardInvalidText;
    if (data.length != length) return GoyaClipboardInvalidText;
    NSPasteboardItem *item = [[NSPasteboardItem alloc] init];
    if (![item setData:data forType:NSPasteboardTypeString]) return GoyaClipboardExportFailed;
    NSPasteboard *board = [NSPasteboard generalPasteboard];
    [board clearContents];
    if (![board writeObjects:@[item]]) return GoyaClipboardExportFailed;
    if (!commit) return GoyaClipboardCommitted;
    if (!admit() || !commit(text)) return GoyaClipboardPartialExport;
    return GoyaClipboardCommitted;
}
+ (GoyaClipboardResult)copyText:(NSString *)text maxBytes:(NSUInteger)maxBytes admit:(GoyaClipboardAdmission)admit {
    return [self exportText:text maxBytes:maxBytes admit:admit commit:nil];
}
+ (GoyaClipboardResult)cutText:(NSString *)text maxBytes:(NSUInteger)maxBytes admit:(GoyaClipboardAdmission)admit commit:(GoyaClipboardCommit)commit {
    if (!commit) return GoyaClipboardDenied;
    return [self exportText:text maxBytes:maxBytes admit:admit commit:commit];
}
+ (GoyaClipboardResult)pasteWithMaxBytes:(NSUInteger)maxBytes admit:(GoyaClipboardAdmission)admit commit:(GoyaClipboardCommit)commit {
    if (![NSThread isMainThread] || !admit || !commit || maxBytes == 0) return GoyaClipboardDenied;
    // AppKit materializes data before its size is known. Reject until a
    // bounded native transfer can be guaranteed.
    return GoyaClipboardUnavailable;
}
@end
