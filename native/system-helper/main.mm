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
    CGPoint outside=CGPointMake(300000,300000);memcpy(bytes+0x10,&outside,sizeof(outside));memcpy(bytes+0x20,&outside,sizeof(outside));memcpy(bytes+0x3c,&wid,sizeof(wid));
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

#include "spaces.h"
static void listWindows(NSString *path) {
  NSMutableArray *items=[NSMutableArray array];
  if(sessionLocked() || !AXIsProcessTrusted()){output(@{@"items":items});return;}
  for(NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
    if(![canonical(app.bundleURL.path)isEqual:canonical(path)])continue;
    NSDictionary *inventory=globalWindowInventory(app.processIdentifier);
    NSMutableSet *rejected=[NSMutableSet set];
    NSMutableDictionary *roots=publishedWindowRoots(app.processIdentifier,rejected),*hints=[NSMutableDictionary dictionary];
    acquireMissingRoots(app.processIdentifier,inventory,roots,hints,rejected);
    NSMutableSet *ids=[NSMutableSet setWithArray:inventory.allKeys];[ids addObjectsFromArray:roots.allKeys];
    [ids minusSet:rejected];
    AXUIElementRef application=AXUIElementCreateApplication(app.processIdentifier);AXUIElementSetMessagingTimeout(application,.05);
    id focused=attribute(application,kAXFocusedWindowAttribute);CGWindowID focusedId=focused?exactWindowId((__bridge AXUIElementRef)focused):0;CFRelease(application);
    NSUInteger ordinal=0;
    for(NSNumber *wid in [ids.allObjects sortedArrayUsingSelector:@selector(compare:)]) {
      ordinal++;id value=roots[wid];AXUIElementRef window=value?(__bridge AXUIElementRef)value:nullptr;
      id title=window?attribute(window,kAXTitleAttribute):nil;
      if(![title isKindOfClass:NSString.class] || ![title length])title=inventory[wid][@"title"];
      bool titleKnown=[title isKindOfClass:NSString.class]&&[title length];
      if(!titleKnown)title=[NSString stringWithFormat:@"%@ · 窗口 %lu",app.localizedName?:@"软件",(unsigned long)ordinal];
      if([title length]>512)title=[title substringToIndex:512];
      uint64_t hint=[hints[wid]unsignedLongLongValue];if(!hint&&window)hint=verifiedElementHint(window,app.processIdentifier);
      NSDictionary *item=@{@"pid":@(app.processIdentifier),@"id":wid,@"title":title,@"titleKnown":@(titleKnown),@"element":@(hint),@"space":inventory[wid][@"space"]?:spaceMetadata(windowSpaces(wid.unsignedIntValue))[@"space"]?:@"",@"minimized":@(window&&[attribute(window,kAXMinimizedAttribute) boolValue])};
      if(wid.unsignedIntValue==focusedId)[items insertObject:item atIndex:0];else[items addObject:item];
    }
  }
  output(@{@"items":items});
}
static void activateWindow(NSString *path,pid_t pid,CGWindowID wid,uint64_t hint) {
  NSRunningApplication *app=nil;for(NSRunningApplication *candidate in NSWorkspace.sharedWorkspace.runningApplications)if(candidate.processIdentifier==pid&&[canonical(candidate.bundleURL.path)isEqual:canonical(path)]){app=candidate;break;}
  if(!app || !wid || sessionLocked() || !AXIsProcessTrusted()){output(@{@"ok":@NO});return;}
  NSDictionary *inventory=globalWindowInventory(pid);NSMutableDictionary *roots=publishedWindowRoots(pid);
  if(hint&&!roots[@(wid)]) {AXUIElementRef e=remoteWindowElement(pid,hint);if(e){AXUIElementSetMessagingTimeout(e,.03);if(exactWindowId(e)==wid&&standardRoot(e))roots[@(wid)]=(__bridge id)e;CFRelease(e);}}
  if(!roots[@(wid)]&&inventory[@(wid)]){NSMutableDictionary *hints=[NSMutableDictionary dictionary];acquireMissingRoots(pid,@{@(wid):inventory[@(wid)]},roots,hints);}
  if(!roots[@(wid)]&&inventory[@(wid)]&&showWindowSpaceThroughMissionControl(wid))roots=publishedWindowRoots(pid);
  id value=roots[@(wid)];if(!value){output(@{@"ok":@NO});return;}
  AXUIElementRef target=(__bridge AXUIElementRef)value;AXUIElementSetMessagingTimeout(target,.08);
  AXUIElementRef root=AXUIElementCreateApplication(pid);AXUIElementSetMessagingTimeout(root,.08);
  AXUIElementSetAttributeValue(target,kAXMinimizedAttribute,kCFBooleanFalse);
  AXUIElementSetAttributeValue(target,kAXMainAttribute,kCFBooleanTrue);
  (void)focusServerWindow(pid,wid);AXUIElementPerformAction(target,kAXRaiseAction);
  bool front=bringToFront(app);
  NSDate *spaceDeadline=[NSDate dateWithTimeIntervalSinceNow:.8];
  while(!windowOnCurrentSpace(wid)&&spaceDeadline.timeIntervalSinceNow>0&&!sessionLocked())[NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.03]];
  if(!windowOnCurrentSpace(wid))front=showWindowSpaceThroughMissionControl(wid)&&bringToFront(app);
  if(front&&!sessionLocked()){(void)focusServerWindow(pid,wid);AXUIElementPerformAction(target,kAXRaiseAction);}
  bool selected=false;NSDate *deadline=[NSDate dateWithTimeIntervalSinceNow:.6];
  do {
    id focused=attribute(root,kAXFocusedWindowAttribute)?:attribute(root,kAXMainWindowAttribute);
    selected=focused && exactWindowId((__bridge AXUIElementRef)focused)==wid;
    if((selected&&windowOnCurrentSpace(wid)) || !front || sessionLocked())break;
    [NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.03]];
  } while(deadline.timeIntervalSinceNow>0);
  CFRelease(root);output(@{@"ok":@(front&&selected&&windowOnCurrentSpace(wid)&&!sessionLocked())});
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
  else if([command isEqualToString:@"windows"]&&argc==3)listWindows(@(argv[2]));
  else if([command isEqualToString:@"activate-window"]&&(argc==5||argc==6))activateWindow(@(argv[2]),(pid_t)[@(argv[3]) intValue],(CGWindowID)[@(argv[4]) longLongValue],argc==6?[@(argv[5])longLongValue]:0);
  else if([command isEqualToString:@"permissions"])output(@{@"accessibility":@(AXIsProcessTrusted())});
  else{output(@{@"ok":@NO});return 2;}
 }return 0;}
