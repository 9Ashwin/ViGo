#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#include <cstdio>
#include <mutex>
#include <thread>
#include <unistd.h>
#include <dlfcn.h>
#include <cstring>
#import <Carbon/Carbon.h>
#import <CoreImage/CoreImage.h>

static std::mutex outputLock;
static void output(NSDictionary *value) {
  std::lock_guard<std::mutex> guard(outputLock);
  NSData *data=[NSJSONSerialization dataWithJSONObject:value options:0 error:nil];
  if(data) fwrite(data.bytes,1,data.length,stdout);
  fputc('\n',stdout);
  fflush(stdout);
}
static id attribute(AXUIElementRef element, CFStringRef name) {
  CFTypeRef value=nullptr;
  if(AXUIElementCopyAttributeValue(element,name,&value)!=kAXErrorSuccess || !value)return nil;
  return CFBridgingRelease(value);
}
static CGRect boundsOf(AXUIElementRef window) {
  CGPoint p=CGPointZero;CGSize s=CGSizeZero;
  id position=attribute(window,kAXPositionAttribute),size=attribute(window,kAXSizeAttribute);
  if(position && CFGetTypeID((__bridge CFTypeRef)position)==AXValueGetTypeID())AXValueGetValue((__bridge AXValueRef)position,static_cast<AXValueType>(kAXValueCGPointType),&p);
  if(size && CFGetTypeID((__bridge CFTypeRef)size)==AXValueGetTypeID())AXValueGetValue((__bridge AXValueRef)size,static_cast<AXValueType>(kAXValueCGSizeType),&s);
  return CGRectMake(p.x,p.y,s.width,s.height);
}
static NSString *canonical(NSString *path) { return path.stringByResolvingSymlinksInPath; }
static NSNumber *windowNumber(pid_t pid, NSString *title, CGRect frame, NSArray *cgWindows) {
  NSNumber *frameMatch=nil,*titleMatch=nil;NSUInteger frames=0,titles=0;
  for(NSDictionary *info in cgWindows) {
    if([info[(__bridge NSString *)kCGWindowOwnerPID] intValue]!=pid || [info[(__bridge NSString *)kCGWindowLayer] intValue]!=0)continue;
    NSString *name=info[(__bridge NSString *)kCGWindowName];
    CGRect b=CGRectZero;CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)info[(__bridge NSString *)kCGWindowBounds],&b);
    if(title.length && [name isEqualToString:title]){titleMatch=info[(__bridge NSString *)kCGWindowNumber];titles++;}
    if(fabs(b.origin.x-frame.origin.x)<3 && fabs(b.origin.y-frame.origin.y)<3 && fabs(b.size.width-frame.size.width)<3 && fabs(b.size.height-frame.size.height)<3){frameMatch=info[(__bridge NSString *)kCGWindowNumber];frames++;}
  }
  return frames==1?frameMatch:titles==1?titleMatch:@0;
}
// Some frameworks expose no usable AX window nodes. Resolve the window-server bridge optionally.
static bool focusServerWindow(pid_t pid, CGWindowID wid) {
  using PSNForPid = OSStatus (*)(pid_t, ProcessSerialNumber *);
  using Focus = CGError (*)(ProcessSerialNumber *,CGWindowID,uint32_t);
  using Post = CGError (*)(ProcessSerialNumber *,const uint8_t *);
  static void *library=dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight",RTLD_LAZY);
  static auto getPSN=(PSNForPid)dlsym(RTLD_DEFAULT,"GetProcessForPID");
  static auto focus=library?(Focus)dlsym(library,"_SLPSSetFrontProcessWithOptions"):nullptr;
  static auto post=library?(Post)dlsym(library,"SLPSPostEventRecordTo"):nullptr;
  if(!getPSN || !focus || !post)return false;
  ProcessSerialNumber psn={};if(getPSN(pid,&psn)!=noErr || focus(&psn,wid,0x200)!=kCGErrorSuccess)return false;
  // A complete off-content key click selects the intended window without clicking any application control.
  for(uint8_t type:{1,2}) {
    uint8_t bytes[0x100]={};bytes[4]=0xf8;bytes[8]=type;bytes[0x3a]=0x10;
    CGPoint outside=CGPointMake(300000,300000);memcpy(bytes+0x20,&outside,sizeof(outside));memcpy(bytes+0x3c,&wid,sizeof(wid));
    if(post(&psn,bytes)!=kCGErrorSuccess)return false;
  }
  return true;
}
static CGWindowID exactWindowId(AXUIElementRef element) {
  using GetWindow=AXError (*)(AXUIElementRef,CGWindowID *);
  static auto get=(GetWindow)dlsym(RTLD_DEFAULT,"_AXUIElementGetWindow");
  CGWindowID wid=0;return get && get(element,&wid)==kAXErrorSuccess?wid:0;
}
static void listWindows(NSArray<NSString *> *paths) {
  BOOL trusted=AXIsProcessTrusted();
  NSMutableArray *windows=[NSMutableArray array],*running=[NSMutableArray array];
  NSArray *cgWindows=CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionAll,kCGNullWindowID));
  NSDate *deadline=[NSDate dateWithTimeIntervalSinceNow:2];
  for(NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
    if(!app.bundleURL)continue;
    NSString *pin=nil;
    for(NSString *path in paths) if([canonical(path) isEqualToString:canonical(app.bundleURL.path)]) {pin=path;break;}
    if(!pin)continue;
    [running addObject:pin];
    if(!trusted)continue;
    AXUIElementRef element=AXUIElementCreateApplication(app.processIdentifier);
    AXUIElementSetMessagingTimeout(element,0.15);
    NSArray *items=deadline.timeIntervalSinceNow>0?attribute(element,kAXWindowsAttribute):nil;
    if(![items isKindOfClass:NSArray.class])items=@[];
    NSUInteger index=0;
    for(id raw in items) {
      if(deadline.timeIntervalSinceNow<=0)break;
      AXUIElementRef w=(__bridge AXUIElementRef)raw;
      AXUIElementSetMessagingTimeout(w,0.15);
      NSString *role=attribute(w,kAXRoleAttribute);
      if(![role isEqualToString:(__bridge NSString *)kAXWindowRole]){index++;continue;}
      NSString *title=attribute(w,kAXTitleAttribute);
      if(![title isKindOfClass:NSString.class])title=@"";
      CGRect frame=boundsOf(w);
      CGWindowID exact=exactWindowId(w);
      NSNumber *wid=exact?@(exact):windowNumber(app.processIdentifier,title,frame,cgWindows);
      if(!wid.unsignedIntValue){index++;continue;}
      NSDictionary *descriptor=nil;for(NSDictionary *info in cgWindows)if([info[(__bridge NSString *)kCGWindowNumber] isEqual:wid] && [info[(__bridge NSString *)kCGWindowOwnerPID] intValue]==app.processIdentifier){descriptor=info;break;}
      if(!descriptor || [descriptor[(__bridge NSString *)kCGWindowLayer] intValue]!=0 || [descriptor[(__bridge NSString *)kCGWindowAlpha] doubleValue]<=0){index++;continue;}

      [windows addObject:@{ @"id":[NSString stringWithFormat:@"%d:%@",app.processIdentifier,wid],@"pid":@(app.processIdentifier),@"index":@(index),@"appPath":pin,@"title":title.length?title:(app.localizedName?:@"无标题窗口"),@"rawTitle":title,@"minimized":@([attribute(w,kAXMinimizedAttribute) boolValue]),@"windowId":wid,@"bounds":@[@(frame.origin.x),@(frame.origin.y),@(frame.size.width),@(frame.size.height)] }];
      index++;
      if(index>=12)break;
    }
    CFRelease(element);
    NSUInteger number=0;
    for(NSDictionary *info in cgWindows) {
      if([info[(__bridge NSString *)kCGWindowOwnerPID] intValue]!=app.processIdentifier || [info[(__bridge NSString *)kCGWindowLayer] intValue]!=0 || [info[(__bridge NSString *)kCGWindowAlpha] doubleValue]<=0 || ![info[(__bridge NSString *)kCGWindowIsOnscreen] boolValue])continue;
      NSNumber *wid=info[(__bridge NSString *)kCGWindowNumber];CGRect b=CGRectZero;
      CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)info[(__bridge NSString *)kCGWindowBounds],&b);
      if(b.size.width<60 || b.size.height<40)continue;
      BOOL known=NO;for(NSDictionary *existing in windows)if([existing[@"pid"] intValue]==app.processIdentifier && [existing[@"windowId"] isEqual:wid]){known=YES;break;}
      if(known)continue;
      NSString *title=info[(__bridge NSString *)kCGWindowName];if(![title isKindOfClass:NSString.class])title=@"";
      NSString *label=title.length?title:[NSString stringWithFormat:@"%@ · 窗口 %lu",app.localizedName?:@"应用",(unsigned long)(number+1)];
      [windows addObject:@{@"id":[NSString stringWithFormat:@"%d:%@",app.processIdentifier,wid],@"pid":@(app.processIdentifier),@"index":@(-1),@"appPath":pin,@"title":label,@"rawTitle":title,@"minimized":@NO,@"windowId":wid,@"bounds":@[@(b.origin.x),@(b.origin.y),@(b.size.width),@(b.size.height)]}];
      if(++number>=12)break;
    }
  }
  output(@{ @"accessibility":@(trusted),@"screen":@(CGPreflightScreenCaptureAccess()),@"running":running,@"windows":windows });
}
static bool sessionLocked() {
  NSDictionary *session=CFBridgingRelease(CGSessionCopyCurrentDictionary());
  return !session || [session[@"CGSSessionScreenIsLocked"] boolValue] || ![session[(__bridge NSString *)kCGSessionOnConsoleKey] boolValue] || ![session[(__bridge NSString *)kCGSessionLoginDoneKey] boolValue];
}
static void activateWindow(NSDictionary *selection) {
  if(sessionLocked()){output(@{@"ok":@NO,@"reason":@"locked"});return;}
  if(!AXIsProcessTrusted()){output(@{@"ok":@NO,@"reason":@"permission"});return;}
  pid_t pid=[selection[@"pid"] intValue];
  NSRunningApplication *app=[NSRunningApplication runningApplicationWithProcessIdentifier:pid];
  NSString *expected=selection[@"appPath"],*title=selection[@"rawTitle"];
  if(!app || ![expected isKindOfClass:NSString.class] || ![canonical(expected) isEqualToString:canonical(app.bundleURL.path)]){output(@{@"ok":@NO,@"reason":@"closed"});return;}
  NSArray *cgWindows=CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionAll,kCGNullWindowID));
  CGWindowID requested=[selection[@"windowId"] unsignedIntValue];
  NSDictionary *live=nil;
  if(requested){for(NSDictionary *info in cgWindows)if([info[(__bridge NSString *)kCGWindowNumber] unsignedIntValue]==requested && [info[(__bridge NSString *)kCGWindowOwnerPID] intValue]==pid){live=info;break;}
    if(!live){output(@{@"ok":@NO,@"reason":@"closed"});return;}}
  AXUIElementRef root=AXUIElementCreateApplication(pid);AXUIElementSetMessagingTimeout(root,0.2);
  NSArray *windows=attribute(root,kAXWindowsAttribute);AXUIElementRef target=nullptr;
  NSMutableArray *matches=[NSMutableArray array];
  CGRect desired=CGRectZero;
  if(live)CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)live[(__bridge NSString *)kCGWindowBounds],&desired);
  if([windows isKindOfClass:NSArray.class])for(id raw in windows){
    AXUIElementRef candidate=(__bridge AXUIElementRef)raw;
    if(![attribute(candidate,kAXRoleAttribute) isEqualToString:(__bridge NSString *)kAXWindowRole])continue;
    CGWindowID candidateId=exactWindowId(candidate);
    if(requested && candidateId){if(candidateId==requested){target=candidate;break;}continue;}
    CGRect b=boundsOf(candidate);
    if(requested){if(fabs(b.origin.x-desired.origin.x)<3 && fabs(b.origin.y-desired.origin.y)<3 && fabs(b.size.width-desired.size.width)<3 && fabs(b.size.height-desired.size.height)<3)[matches addObject:raw];}
    else if([attribute(candidate,kAXTitleAttribute)?:@"" isEqualToString:title?:@""])[matches addObject:raw];
  }
  if(!target && matches.count==1)target=(__bridge AXUIElementRef)matches[0];
  if(target){AXUIElementSetMessagingTimeout(target,0.2);AXUIElementSetAttributeValue(target,kAXMinimizedAttribute,kCFBooleanFalse);AXUIElementSetAttributeValue(target,kAXMainAttribute,kCFBooleanTrue);}
  BOOL focused=requested && focusServerWindow(pid,requested);
  if(target){if(!focused)[app activateWithOptions:NSApplicationActivateIgnoringOtherApps];BOOL raised=AXUIElementPerformAction(target,kAXRaiseAction)==kAXErrorSuccess;focused=focused||raised;}
  CFRelease(root);output(@{@"ok":@(focused)});
}

@interface FrameSink : NSObject<SCStreamOutput>
@property(nonatomic,strong) CIContext *context;
@end
@implementation FrameSink
- (void)stream:(SCStream *)stream didOutputSampleBuffer:(CMSampleBufferRef)sample ofType:(SCStreamOutputType)type {
  (void)stream;
  if(type!=SCStreamOutputTypeScreen || !CMSampleBufferIsValid(sample) || !CMSampleBufferDataIsReady(sample))return;
  @autoreleasepool {
    NSArray *attachments=(__bridge NSArray *)CMSampleBufferGetSampleAttachmentsArray(sample,false);
    if(attachments.count && [attachments[0][SCStreamFrameInfoStatus] integerValue]!=SCFrameStatusComplete)return;
    CVImageBufferRef buffer=CMSampleBufferGetImageBuffer(sample);if(!buffer)return;
    CIImage *image=[CIImage imageWithCVPixelBuffer:buffer];
    CGImageRef frame=[self.context createCGImage:image fromRect:image.extent];if(!frame)return;
    NSBitmapImageRep *rep=[[NSBitmapImageRep alloc] initWithCGImage:frame];
    NSData *png=[rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}];CGImageRelease(frame);
    if(png)output(@{@"ok":@YES,@"image":[@"data:image/png;base64," stringByAppendingString:[png base64EncodedStringWithOptions:0]]});
  }
}
@end
static void streamPreview(CGWindowID windowId, pid_t owner) {
  if(sessionLocked()){output(@{@"ok":@NO,@"reason":@"locked"});return;}
  if(!CGPreflightScreenCaptureAccess()){output(@{@"ok":@NO,@"reason":@"permission"});return;}
  if(@available(macOS 13.0,*)) {
    CFRunLoopRef loop=CFRunLoopGetCurrent();
    __block SCStream *stream=nil;__block FrameSink *sink=nil;
    auto stop=^{CFRunLoopPerformBlock(loop,kCFRunLoopCommonModes,^{CFRunLoopStop(loop);});CFRunLoopWakeUp(loop);};
    [SCShareableContent getShareableContentExcludingDesktopWindows:YES onScreenWindowsOnly:NO completionHandler:^(SCShareableContent *content,NSError *error){
      if(error){output(@{@"ok":@NO,@"reason":@"unavailable"});stop();return;}
      SCWindow *selected=nil;for(SCWindow *window in content.windows)if(window.windowID==windowId && window.owningApplication.processID==owner){selected=window;break;}
      if(!selected || selected.frame.size.width<1 || selected.frame.size.height<1){output(@{@"ok":@NO,@"reason":@"closed"});stop();return;}
      SCContentFilter *filter=[[SCContentFilter alloc] initWithDesktopIndependentWindow:selected];
      SCStreamConfiguration *config=[SCStreamConfiguration new];
      CGFloat scale=MIN(360.0/selected.frame.size.width,220.0/selected.frame.size.height);
      config.width=MAX(1,(NSUInteger)(selected.frame.size.width*scale));config.height=MAX(1,(NSUInteger)(selected.frame.size.height*scale));
      config.showsCursor=NO;config.minimumFrameInterval=CMTimeMake(1,6);config.queueDepth=3;
      sink=[FrameSink new];sink.context=[CIContext contextWithOptions:@{kCIContextUseSoftwareRenderer:@NO}];
      stream=[[SCStream alloc] initWithFilter:filter configuration:config delegate:nil];
      NSError *attachError=nil;
      if(![stream addStreamOutput:sink type:SCStreamOutputTypeScreen sampleHandlerQueue:dispatch_queue_create("app.vigo.preview",DISPATCH_QUEUE_SERIAL) error:&attachError]){output(@{@"ok":@NO,@"reason":@"unavailable"});stop();return;}
      [stream startCaptureWithCompletionHandler:^(NSError *startError){if(startError){output(@{@"ok":@NO,@"reason":@"unavailable"});stop();}}];
    }];
    std::thread reader([loop]{char byte;while(::read(STDIN_FILENO,&byte,1)>0){}CFRunLoopPerformBlock(loop,kCFRunLoopCommonModes,^{CFRunLoopStop(loop);});CFRunLoopWakeUp(loop);usleep(300000);_exit(0);});reader.detach();
    // The control pipe owns capture lifetime; EOF also enforces process exit if teardown stalls.
    CFRunLoopRun();
    if(stream)[stream stopCaptureWithCompletionHandler:^(NSError *){}];
  } else output(@{@"ok":@NO,@"reason":@"unsupported"});
}

int main(int argc,char **argv) { @autoreleasepool {
  if(argc<2){output(@{@"ok":@NO});return 2;}
  NSString *command=@(argv[1]);
  if([command isEqualToString:@"list"]){NSMutableArray *paths=[NSMutableArray array];for(int i=2;i<argc;i++)[paths addObject:@(argv[i])];listWindows(paths);}
  else if([command isEqualToString:@"activate"] && argc==3){NSData *data=[@(argv[2]) dataUsingEncoding:NSUTF8StringEncoding];id item=[NSJSONSerialization JSONObjectWithData:data options:0 error:nil];if([item isKindOfClass:NSDictionary.class])activateWindow(item);else output(@{@"ok":@NO});}
  else if([command isEqualToString:@"stream"] && argc==4)streamPreview((CGWindowID)strtoul(argv[2],nullptr,10),(pid_t)strtol(argv[3],nullptr,10));
  else if([command isEqualToString:@"request-screen"])output(@{@"granted":@(CGRequestScreenCaptureAccess())});
  else if([command isEqualToString:@"permissions"])output(@{@"accessibility":@(AXIsProcessTrusted()),@"screen":@(CGPreflightScreenCaptureAccess())});
  else{output(@{@"ok":@NO});return 2;}
 }return 0;}
