#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#include <cstdio>
#include <mutex>
#include <thread>
#include <unistd.h>
#include <dlfcn.h>
#include <cstring>
#import <Carbon/Carbon.h>

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
static NSString *canonical(NSString *path) { return path.stringByResolvingSymlinksInPath; }
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
static bool sessionLocked() {
  NSDictionary *session=CFBridgingRelease(CGSessionCopyCurrentDictionary());
  return !session || [session[@"CGSSessionScreenIsLocked"] boolValue] || ![session[(__bridge NSString *)kCGSessionOnConsoleKey] boolValue] || ![session[(__bridge NSString *)kCGSessionLoginDoneKey] boolValue];
}
static bool bringToFront(NSRunningApplication *app) {
  if(sessionLocked())return false;
  [NSApplication sharedApplication];[NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
  [app unhide];[app activateWithOptions:0];
  NSDate *deadline=[NSDate dateWithTimeIntervalSinceNow:.18];
  while(NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier!=app.processIdentifier && deadline.timeIntervalSinceNow>0 && !sessionLocked())
    [NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.015]];
  if(NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier!=app.processIdentifier && !sessionLocked()) {
    NSWorkspaceOpenConfiguration *config=[NSWorkspaceOpenConfiguration configuration];config.activates=YES;
    [NSWorkspace.sharedWorkspace openApplicationAtURL:app.bundleURL configuration:config completionHandler:nil];
    deadline=[NSDate dateWithTimeIntervalSinceNow:.3];
    while(NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier!=app.processIdentifier && deadline.timeIntervalSinceNow>0 && !sessionLocked())
      [NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.015]];
  }
  return !sessionLocked() && NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier==app.processIdentifier;
}
static void activateApplication(NSString *path) {
  if(sessionLocked()){output(@{@"ok":@NO});return;}
  if(![path.pathExtension.lowercaseString isEqualToString:@"app"]){output(@{@"ok":@NO});return;}
  NSRunningApplication *app=nil;
  for(NSRunningApplication *candidate in NSWorkspace.sharedWorkspace.runningApplications)
    if([canonical(candidate.bundleURL.path) isEqualToString:canonical(path)]){app=candidate;break;}
  if(app) {
    AXUIElementRef root=nullptr,target=nullptr;
    if(AXIsProcessTrusted()) {
      root=AXUIElementCreateApplication(app.processIdentifier);AXUIElementSetMessagingTimeout(root,.15);
      id window=attribute(root,kAXFocusedWindowAttribute)?:attribute(root,kAXMainWindowAttribute);
      if(!window){NSArray *windows=attribute(root,kAXWindowsAttribute);if([windows isKindOfClass:NSArray.class])window=windows.firstObject;}
      if(window){target=(AXUIElementRef)CFRetain((__bridge AXUIElementRef)window);AXUIElementSetMessagingTimeout(target,.15);}
    }
    if(target){AXUIElementSetAttributeValue(target,kAXMinimizedAttribute,kCFBooleanFalse);AXUIElementSetAttributeValue(target,kAXMainAttribute,kCFBooleanTrue);}
    (void)bringToFront(app);
    if(target&&!sessionLocked()) {
      CGWindowID wid=exactWindowId(target);if(wid)(void)focusServerWindow(app.processIdentifier,wid);
      AXUIElementPerformAction(target,kAXRaiseAction);
    }
    if(target)CFRelease(target);if(root)CFRelease(root);
    output(@{@"ok":@(!sessionLocked()&&NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier==app.processIdentifier)});
    return;
  }
  [NSApplication sharedApplication];[NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
  NSWorkspaceOpenConfiguration *config=[NSWorkspaceOpenConfiguration configuration];config.activates=YES;
  __block BOOL done=NO,opened=NO;
  [NSWorkspace.sharedWorkspace openApplicationAtURL:[NSURL fileURLWithPath:path] configuration:config completionHandler:^(NSRunningApplication *launched,NSError *error){opened=launched&&!error;done=YES;}];
  NSDate *deadline=[NSDate dateWithTimeIntervalSinceNow:2];
  while(!done&&deadline.timeIntervalSinceNow>0&&!sessionLocked())
    [NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.015]];
  output(@{@"ok":@(opened&&!sessionLocked())});
}

static void listDock(){
  NSMutableArray *items=[NSMutableArray array];NSMutableSet *seen=[NSMutableSet set];BOOL exact=NO;
  void(^add)(id,NSString*,NSString*)=^(id raw,NSString *name,NSString *kind){
    NSURL *url=[raw isKindOfClass:NSURL.class]?raw:[raw isKindOfClass:NSString.class]?[NSURL URLWithString:raw]:nil;
    NSString *path=url.isFileURL?url.path:nil;if(!path.length||![path.pathExtension.lowercaseString isEqualToString:@"app"]||[seen containsObject:path])return;
    [seen addObject:path];NSString *label=[name isKindOfClass:NSString.class]&&name.length?name:path.lastPathComponent.stringByDeletingPathExtension;
    [items addObject:@{@"path":path,@"name":label,@"kind":kind?:@"app"}];
  };
  if(AXIsProcessTrusted()&&!sessionLocked())for(NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications){
    if(![app.bundleIdentifier isEqualToString:@"com.apple.dock"])continue;
    auto root=AXUIElementCreateApplication(app.processIdentifier);AXUIElementSetMessagingTimeout(root,.1);
    NSArray *children=attribute(root,kAXChildrenAttribute);
    if([children isKindOfClass:NSArray.class])for(id child in children){
      auto list=(__bridge AXUIElementRef)child;if(![attribute(list,kAXRoleAttribute)isEqualToString:@"AXList"])continue;
      NSArray *entries=attribute(list,kAXChildrenAttribute);if(![entries isKindOfClass:NSArray.class])continue;exact=YES;
      for(id raw in entries){auto e=(__bridge AXUIElementRef)raw;AXUIElementSetMessagingTimeout(e,.05);NSString *sub=attribute(e,kAXSubroleAttribute);
        if(![sub isKindOfClass:NSString.class]){exact=NO;continue;}
        if([sub isEqualToString:@"AXApplicationDockItem"]){id url=attribute(e,CFSTR("AXURL"));if(!url){exact=NO;continue;}add(url,attribute(e,kAXTitleAttribute),@"app");}
      }
    }
    CFRelease(root);
  }
  if(items.count==0)exact=NO;
  if(!exact){[items removeAllObjects];[seen removeAllObjects];for(NSString *key in @[@"persistent-apps",@"recent-apps"]){
    NSArray *entries=CFBridgingRelease(CFPreferencesCopyAppValue((__bridge CFStringRef)key,CFSTR("com.apple.dock")));if(![entries isKindOfClass:NSArray.class])continue;
    for(id entry in entries){if(![entry isKindOfClass:NSDictionary.class])continue;NSDictionary *tile=entry[@"tile-data"];if(![tile isKindOfClass:NSDictionary.class])continue;add(tile[@"file-data"][@"_CFURLString"],tile[@"file-label"],@"app");}
  }}
  output(@{@"items":items,@"exact":@(exact)});
}

int main(int argc,char **argv) { @autoreleasepool {
  if(argc<2){output(@{@"ok":@NO});return 2;}
  NSString *command=@(argv[1]);
  if([command isEqualToString:@"dock"])listDock();
  else if([command isEqualToString:@"activate-app"]&&argc==3)activateApplication(@(argv[2]));
  else if([command isEqualToString:@"permissions"])output(@{@"accessibility":@(AXIsProcessTrusted())});
  else{output(@{@"ok":@NO});return 2;}
 }return 0;}
