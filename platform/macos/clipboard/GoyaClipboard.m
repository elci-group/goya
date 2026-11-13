#import "GoyaClipboard.h"
#import <AppKit/AppKit.h>

@implementation GoyaClipboard
+ (GoyaClipboardResult)exportText:(NSString *)text maxBytes:(NSUInteger)maxBytes admit:(GoyaClipboardAdmission)admit commit:(nullable GoyaClipboardCommit)commit {
    if (![NSThread isMainThread] || !admit || !text || maxBytes == 0) return GoyaClipboardDenied;
    NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding allowLossyConversion:NO];
    if (!data) return GoyaClipboardInvalidText;
    if (data.length > maxBytes) return GoyaClipboardTooLarge;
    if (!admit()) return GoyaClipboardDenied;
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
    if (!admit()) return GoyaClipboardDenied;
    NSPasteboard *board = [NSPasteboard generalPasteboard];
    NSData *data = [board dataForType:NSPasteboardTypeString];
    if (!data) return GoyaClipboardUnavailable;
    if (data.length > maxBytes) return GoyaClipboardTooLarge;
    NSString *text = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
    if (!text) return GoyaClipboardInvalidText;
    text = [text stringByReplacingOccurrencesOfString:@"\r\n" withString:@"\n"];
    text = [text stringByReplacingOccurrencesOfString:@"\r" withString:@"\n"];
    if (!admit()) return GoyaClipboardDenied;
    return commit(text) ? GoyaClipboardCommitted : GoyaClipboardDenied;
}
@end
