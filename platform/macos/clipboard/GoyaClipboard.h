#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN
typedef NS_ENUM(NSInteger, GoyaClipboardResult) {
    GoyaClipboardDenied, GoyaClipboardInvalidText, GoyaClipboardTooLarge,
    GoyaClipboardUnavailable, GoyaClipboardExportFailed,
    GoyaClipboardPartialExport, GoyaClipboardCommitted,
};
// The owner checks user initiation, policy, identity, focus, epoch, revision,
// selection and reservation on every invocation, including before mutation.
typedef BOOL (^GoyaClipboardAdmission)(void);
typedef BOOL (^GoyaClipboardCommit)(NSString *text);
@interface GoyaClipboard : NSObject
// Main-thread only; a zero limit denies access.
+ (GoyaClipboardResult)copyText:(NSString *)text maxBytes:(NSUInteger)maxBytes admit:(GoyaClipboardAdmission)admit;
+ (GoyaClipboardResult)cutText:(NSString *)text maxBytes:(NSUInteger)maxBytes admit:(GoyaClipboardAdmission)admit commit:(GoyaClipboardCommit)commit;
+ (GoyaClipboardResult)pasteWithMaxBytes:(NSUInteger)maxBytes admit:(GoyaClipboardAdmission)admit commit:(GoyaClipboardCommit)commit;
@end
NS_ASSUME_NONNULL_END
