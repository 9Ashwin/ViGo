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
      if([info[(__bridge NSString *)kCGWindowOwnerPID] intValue]!=app.processIdentifier || [info[(__bridge NSString *)kCGWindowLayer] intValue]!=0 || [info[(__bridge NSString *)kCGWindowAlpha] doubleValue]<=0)continue;
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
  output(@{ @"accessibility":@(trusted),@"running":running,@"windows":windows });
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
  // Public activation informs Stage Manager before selecting an exact server window.
  (void)bringToFront(app);
  if(sessionLocked()){CFRelease(root);output(@{@"ok":@NO,@"reason":@"locked"});return;}
  BOOL focused=requested && focusServerWindow(pid,requested);
  if(target){if(!focused)[app activateWithOptions:NSApplicationActivateIgnoringOtherApps];BOOL raised=AXUIElementPerformAction(target,kAXRaiseAction)==kAXErrorSuccess;focused=focused||raised;}
  CFRelease(root);output(@{@"ok":@(focused && NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier==pid)});
}

static bool browserPath(NSString *path) {
  NSDictionary *info=[NSBundle bundleWithPath:path].infoDictionary;
  for(NSDictionary *type in info[@"CFBundleURLTypes"])for(NSString *scheme in type[@"CFBundleURLSchemes"])if([scheme isEqualToString:@"http"] || [scheme isEqualToString:@"https"])return true;
  return false;
}
// Chromium exposes its tab strip through AX; skip web content and menus entirely.
static void collectTabs(AXUIElementRef node,NSMutableArray *tabs,int depth,NSUInteger *visited,NSDate *deadline,bool inTabGroup=false) {
  if(depth>14 || ++*visited>1600 || deadline.timeIntervalSinceNow<=0)return;
  AXUIElementSetMessagingTimeout(node,.06);
  NSString *role=attribute(node,kAXRoleAttribute),*subrole=attribute(node,kAXSubroleAttribute);
  if([role isEqualToString:@"AXWebArea"] || [role isEqualToString:@"AXMenuBar"] || [role isEqualToString:@"AXMenu"])return;
  if([subrole isEqualToString:@"AXTabButton"] || (inTabGroup && [role isEqualToString:@"AXRadioButton"])) {
    NSString *title=attribute(node,kAXTitleAttribute);if(![title isKindOfClass:NSString.class] || !title.length)title=attribute(node,kAXDescriptionAttribute);
    if([title isKindOfClass:NSString.class] && title.length)[tabs addObject:@{@"node":(__bridge id)node,@"identifier":attribute(node,kAXIdentifierAttribute)?:@"",@"title":title,@"selected":@([attribute(node,kAXValueAttribute) boolValue])}];
    return;
  }
  NSArray *children=attribute(node,kAXChildrenAttribute);if([children isKindOfClass:NSArray.class])for(id child in children)collectTabs((__bridge AXUIElementRef)child,tabs,depth+1,visited,deadline,inTabGroup || [role isEqualToString:@"AXTabGroup"]);
}
static void browserTabs(NSArray<NSString *> *paths,NSDictionary *selection) {
  if(!AXIsProcessTrusted() || sessionLocked()){output(@{@"tabs":@[],@"ok":@NO});return;}
  NSMutableArray *result=[NSMutableArray array];NSDate *deadline=[NSDate dateWithTimeIntervalSinceNow:1.5];
  NSArray *cg=CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionAll,kCGNullWindowID));
  for(NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
    NSString *path=nil;for(NSString *candidate in paths)if([canonical(candidate) isEqualToString:canonical(app.bundleURL.path)]){path=candidate;break;}
    if(!path || !browserPath(path) || (selection && [selection[@"pid"] intValue]!=app.processIdentifier))continue;
    AXUIElementRef root=AXUIElementCreateApplication(app.processIdentifier);AXUIElementSetMessagingTimeout(root,.1);
    NSArray *windows=attribute(root,kAXWindowsAttribute);if(![windows isKindOfClass:NSArray.class])windows=@[];
    for(id raw in windows) {
      AXUIElementRef window=(__bridge AXUIElementRef)raw;CGWindowID wid=exactWindowId(window);
      if(!wid)wid=windowNumber(app.processIdentifier,attribute(window,kAXTitleAttribute),boundsOf(window),cg).unsignedIntValue;
      if(!wid || (selection && [selection[@"windowId"] unsignedIntValue]!=wid))continue;
      NSMutableArray *tabs=[NSMutableArray array];NSUInteger visited=0;collectTabs(window,tabs,0,&visited,deadline);
      for(NSUInteger i=0;i<tabs.count;i++) {
        NSDictionary *tab=tabs[i];
        if(selection){
          // Prefer the browser's stable identifier; absent one, reject ambiguous duplicate titles.
          NSUInteger titleCount=0;for(NSDictionary *candidate in tabs)if([candidate[@"title"] isEqual:selection[@"title"]])titleCount++;
          NSString *identifier=selection[@"identifier"];
          BOOL stable=[identifier isKindOfClass:NSString.class] && identifier.length;
          NSUInteger identifierCount=0;if(stable)for(NSDictionary *candidate in tabs)if([candidate[@"identifier"] isEqual:identifier])identifierCount++;
          BOOL identity=stable?identifierCount==1 && [identifier isEqual:tab[@"identifier"]]:titleCount==1 && [selection[@"index"] unsignedIntegerValue]==i;
          if(identity && [selection[@"title"] isEqual:tab[@"title"]]){
            AXUIElementRef node=(__bridge AXUIElementRef)tab[@"node"];
            BOOL pressed=AXUIElementPerformAction(node,kAXPressAction)==kAXErrorSuccess;
            if(pressed){AXUIElementSetAttributeValue(window,kAXMinimizedAttribute,kCFBooleanFalse);(void)bringToFront(app);if(sessionLocked()){CFRelease(root);output(@{@"ok":@NO,@"reason":@"locked"});return;}focusServerWindow(app.processIdentifier,wid);if(NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier!=app.processIdentifier)pressed=NO;AXUIElementPerformAction(window,kAXRaiseAction);}
            CFRelease(root);output(@{@"ok":@(pressed)});return;
          }
        } else [result addObject:@{@"id":[NSString stringWithFormat:@"tab:%d:%u:%lu",app.processIdentifier,wid,(unsigned long)i],@"kind":@"tab",@"appPath":path,@"pid":@(app.processIdentifier),@"windowId":@(wid),@"index":@(i),@"title":tab[@"title"],@"identifier":tab[@"identifier"],@"selected":tab[@"selected"]}];
      }
    }
    CFRelease(root);
  }
  output(selection?@{@"ok":@NO}:@{@"tabs":result,@"ok":@YES});
}

static void listDock(){
  NSMutableArray *items=[NSMutableArray array];NSMutableSet *seen=[NSMutableSet set];BOOL exact=NO;
  void(^add)(id,NSString*,NSString*)=^(id raw,NSString *name,NSString *kind){
    NSURL *url=[raw isKindOfClass:NSURL.class]?raw:[raw isKindOfClass:NSString.class]?[NSURL URLWithString:raw]:nil;
    NSString *path=url.isFileURL?url.path:nil;if(!path.length||[seen containsObject:path])return;
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
        if([sub isEqualToString:@"AXApplicationDockItem"]||[sub isEqualToString:@"AXFolderDockItem"]||[sub isEqualToString:@"AXFileDockItem"]){id url=attribute(e,CFSTR("AXURL"));if(!url){exact=NO;continue;}add(url,attribute(e,kAXTitleAttribute),[sub isEqualToString:@"AXApplicationDockItem"]?@"app":@"file");}
      }
    }
    CFRelease(root);
  }
  if(items.count==0)exact=NO;
  if(!exact){[items removeAllObjects];[seen removeAllObjects];for(NSString *key in @[@"persistent-apps",@"recent-apps",@"persistent-others"]){
    NSArray *entries=CFBridgingRelease(CFPreferencesCopyAppValue((__bridge CFStringRef)key,CFSTR("com.apple.dock")));if(![entries isKindOfClass:NSArray.class])continue;
    for(id entry in entries){if(![entry isKindOfClass:NSDictionary.class])continue;NSDictionary *tile=entry[@"tile-data"];if(![tile isKindOfClass:NSDictionary.class])continue;add(tile[@"file-data"][@"_CFURLString"],tile[@"file-label"],[key isEqualToString:@"persistent-others"]?@"file":@"app");}
  }}
  output(@{@"items":items,@"exact":@(exact)});
}

int main(int argc,char **argv) { @autoreleasepool {
  if(argc<2){output(@{@"ok":@NO});return 2;}
  NSString *command=@(argv[1]);
  if([command isEqualToString:@"dock"])listDock();
  else if([command isEqualToString:@"tabs"]){NSMutableArray *paths=[NSMutableArray array];for(int i=2;i<argc;i++)[paths addObject:@(argv[i])];browserTabs(paths,nil);}
  else if([command isEqualToString:@"activate-tab"] && argc==3){id item=[NSJSONSerialization JSONObjectWithData:[@(argv[2]) dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];if([item isKindOfClass:NSDictionary.class] && [item[@"appPath"] isKindOfClass:NSString.class])browserTabs(@[item[@"appPath"]],item);else output(@{@"ok":@NO});}
  else if([command isEqualToString:@"list"]){NSMutableArray *paths=[NSMutableArray array];for(int i=2;i<argc;i++)[paths addObject:@(argv[i])];listWindows(paths);}
  else if([command isEqualToString:@"activate"] && argc==3){NSData *data=[@(argv[2]) dataUsingEncoding:NSUTF8StringEncoding];id item=[NSJSONSerialization JSONObjectWithData:data options:0 error:nil];if([item isKindOfClass:NSDictionary.class])activateWindow(item);else output(@{@"ok":@NO});}
  else if([command isEqualToString:@"permissions"])output(@{@"accessibility":@(AXIsProcessTrusted())});
  else{output(@{@"ok":@NO});return 2;}
 }return 0;}
