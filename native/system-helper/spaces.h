// Read-only Space inventory and accessibility acquisition. No window/Space membership is changed.
#include <chrono>

static void *spaceSymbol(const char *name) {
  static void *library=dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight",RTLD_LAZY);
  return library?dlsym(library,name):nullptr;
}
static int spaceConnection() {
  using Fn=int(*)();static auto fn=(Fn)spaceSymbol("CGSMainConnectionID");return fn?fn():0;
}
static NSArray *managedDisplays() {
  using Fn=CFArrayRef(*)(int);static auto fn=(Fn)spaceSymbol("CGSCopyManagedDisplaySpaces");
  return fn?CFBridgingRelease(fn(spaceConnection())):@[];
}
static NSArray *windowSpaces(CGWindowID wid) {
  using Fn=CFArrayRef(*)(int,int,CFArrayRef);static auto fn=(Fn)spaceSymbol("CGSCopySpacesForWindows");
  return fn?CFBridgingRelease(fn(spaceConnection(),7,(__bridge CFArrayRef)@[@(wid)])):@[];
}
static bool windowOnCurrentSpace(CGWindowID wid) {
  NSArray *ids=windowSpaces(wid);if(!ids.count)return true;
  for(NSDictionary *display in managedDisplays())if([ids containsObject:display[@"Current Space"][@"ManagedSpaceID"]])return true;
  return false;
}
static NSDictionary *spaceMetadata(NSArray *ids) {
  NSUInteger desktop=0;
  for(NSDictionary *display in managedDisplays()) {
    NSArray *spaces=display[@"Spaces"];
    for(NSUInteger i=0;i<spaces.count;i++) {
      NSDictionary *space=spaces[i];NSNumber *sid=space[@"ManagedSpaceID"]?:space[@"id64"];
      bool fullscreen=[space[@"type"]intValue]!=0;if(!fullscreen)desktop++;
      if([ids containsObject:sid])return @{@"space":ids.count>1?@"所有桌面":fullscreen?@"全屏桌面":[NSString stringWithFormat:@"桌面 %lu",(unsigned long)desktop],@"sid":sid,@"display":display[@"Display Identifier"]?:@"",@"index":@(i),@"spaceCount":@(spaces.count)};
    }
  }
  return @{};
}
static NSDictionary *globalWindowInventory(pid_t pid) {
  using Fn=CFArrayRef(*)(int,uint32_t,CFArrayRef,uint32_t,uint64_t*,uint64_t*);
  static auto copy=(Fn)spaceSymbol("CGSCopyWindowsWithOptionsAndTags");
  if(!copy)return @{};
  NSMutableArray *spaces=[NSMutableArray array];
  for(NSDictionary *display in managedDisplays())for(NSDictionary *space in display[@"Spaces"]){NSNumber *sid=space[@"ManagedSpaceID"]?:space[@"id64"];if(sid)[spaces addObject:sid];}
  uint64_t set=0,clear=0;
  NSArray *ids=CFBridgingRelease(copy(spaceConnection(),0,(__bridge CFArrayRef)spaces,7,&set,&clear));
  if(!ids.count)return @{};
  NSMutableDictionary *result=[NSMutableDictionary dictionary];
  using Query=CFTypeRef(*)(int,CFArrayRef,int);using Iter=CFTypeRef(*)(CFTypeRef);using Advance=bool(*)(CFTypeRef);using Int=int(*)(CFTypeRef);using Id=uint32_t(*)(CFTypeRef);using Bounds=CGRect(*)(CFTypeRef);using Title=CFStringRef(*)(CFTypeRef);
  static auto query=(Query)spaceSymbol("SLSWindowQueryWindows");static auto iterator=(Iter)spaceSymbol("SLSWindowQueryResultCopyWindows");
  static auto advance=(Advance)spaceSymbol("SLSWindowIteratorAdvance");static auto owner=(Int)spaceSymbol("SLSWindowIteratorGetPID");
  static auto level=(Int)spaceSymbol("SLSWindowIteratorGetLevel");static auto identifier=(Id)spaceSymbol("SLSWindowIteratorGetWindowID");
  static auto parent=(Id)spaceSymbol("SLSWindowIteratorGetParentID");static auto getBounds=(Bounds)spaceSymbol("SLSWindowIteratorGetBounds");
  static auto title=(Title)spaceSymbol("SLSWindowIteratorCopyTitle");
  if(!query||!iterator||!advance||!owner||!level||!identifier||!parent||!getBounds)return result;
  CFTypeRef snapshot=query(spaceConnection(),(__bridge CFArrayRef)ids,1);if(!snapshot)return result;
  CFTypeRef it=iterator(snapshot);if(!it){CFRelease(snapshot);return result;}
  NSUInteger guard=0;
  while(guard++<ids.count+16 && advance(it)) {
    if(owner(it)!=pid || level(it)!=0 || parent(it)!=0)continue;
    CGRect bounds=getBounds(it);if(bounds.size.width<70 || bounds.size.height<70)continue;
    NSNumber *wid=@(identifier(it));if(!wid.unsignedIntValue)continue;
    NSMutableDictionary *record=[spaceMetadata(windowSpaces(wid.unsignedIntValue)) mutableCopy];
    record[@"title"]=title?CFBridgingRelease(title(it))?:@"":@"";
    result[wid]=record;
  }
  CFRelease(it);CFRelease(snapshot);
  return result;
}
static AXUIElementRef remoteWindowElement(pid_t pid,uint64_t identifier) {
  using Fn=AXUIElementRef(*)(CFDataRef);static auto create=(Fn)dlsym(RTLD_DEFAULT,"_AXUIElementCreateWithRemoteToken");if(!create)return nullptr;
  uint8_t bytes[20]={};uint32_t magic=0x636f636f;
  memcpy(bytes,&pid,4);memcpy(bytes+8,&magic,4);memcpy(bytes+12,&identifier,8);
  CFDataRef data=CFDataCreate(nullptr,bytes,sizeof(bytes));AXUIElementRef element=create(data);CFRelease(data);return element;
}
static bool standardRoot(AXUIElementRef element) {
  if(![attribute(element,kAXRoleAttribute)isEqual:(__bridge NSString *)kAXWindowRole])return false;
  id subrole=attribute(element,kAXSubroleAttribute);
  return !subrole || [subrole isEqual:(__bridge NSString *)kAXStandardWindowSubrole];
}
static uint64_t verifiedElementHint(AXUIElementRef element,pid_t pid) {
  // A hint only: validate equality before using it, rather than reading an opaque CF object's memory.
  uint64_t hash=CFHash(element),base=(uint32_t)pid+uint64_t(0x636f636f);
  if(hash<base || (hash-base)%16)return 0;
  uint64_t hint=(hash-base)/16;AXUIElementRef candidate=remoteWindowElement(pid,hint);
  bool equal=candidate&&CFEqual(element,candidate);if(candidate)CFRelease(candidate);return equal?hint:0;
}
static NSMutableDictionary *publishedWindowRoots(pid_t pid,NSMutableSet *rejected=nil) {
  AXUIElementRef root=AXUIElementCreateApplication(pid);AXUIElementSetMessagingTimeout(root,.07);
  NSMutableArray *elements=[NSMutableArray array];id windows=attribute(root,kAXWindowsAttribute);
  if([windows isKindOfClass:NSArray.class])[elements addObjectsFromArray:windows];
  for(CFStringRef key:{kAXFocusedWindowAttribute,kAXMainWindowAttribute}){id e=attribute(root,key);if(e)[elements addObject:e];}
  NSMutableDictionary *result=[NSMutableDictionary dictionary];
  for(id value in elements) {
    AXUIElementRef element=(__bridge AXUIElementRef)value;AXUIElementSetMessagingTimeout(element,.04);
    CGWindowID wid=exactWindowId(element);if(wid&&standardRoot(element))result[@(wid)]=value;else if(wid&&[attribute(element,kAXRoleAttribute)isEqual:@"AXWindow"])[rejected addObject:@(wid)];
  }
  CFRelease(root);return result;
}
static void acquireMissingRoots(pid_t pid,NSDictionary *inventory,NSMutableDictionary *roots,NSMutableDictionary *hints,NSMutableSet *rejected=nil) {
  NSMutableSet *missing=[NSMutableSet setWithArray:inventory.allKeys];[missing minusSet:[NSSet setWithArray:roots.allKeys]];
  if(!missing.count)return;
  uint64_t seed=0;
  for(NSNumber *wid in roots) {
    uint64_t hint=verifiedElementHint((__bridge AXUIElementRef)roots[wid],pid);if(hint){hints[wid]=@(hint);seed=MAX(seed,hint);}
  }
  auto started=std::chrono::steady_clock::now();
  // Search a small low-ID prefix, then close to this process's verified live elements.
  for(uint64_t n=0;std::chrono::steady_clock::now()-started<std::chrono::milliseconds(350)&&missing.count;n++) {
    uint64_t identifier=n<512?n:(seed>128?seed-128:512)+n-512;
    AXUIElementRef element=remoteWindowElement(pid,identifier);if(!element)continue;
    AXUIElementSetMessagingTimeout(element,.006);
    CGWindowID wid=exactWindowId(element);
    if([missing containsObject:@(wid)] && [attribute(element,kAXRoleAttribute)isEqual:@"AXWindow"]) {
      if(standardRoot(element)){roots[@(wid)]=(__bridge id)element;hints[@(wid)]=@(identifier);}else{[rejected addObject:@(wid)];}
      [missing removeObject:@(wid)];
    }
    CFRelease(element);
  }
}

static void findSpaceLists(AXUIElementRef element,NSMutableArray *lists,int depth,std::chrono::steady_clock::time_point started) {
  if(depth>12 || std::chrono::steady_clock::now()-started>std::chrono::milliseconds(700))return;
  AXUIElementSetMessagingTimeout(element,.025);
  if([attribute(element,CFSTR("AXIdentifier"))isEqual:@"mc.spaces.list"]){[lists addObject:(__bridge id)element];return;}
  for(id child in attribute(element,kAXChildrenAttribute))findSpaceLists((__bridge AXUIElementRef)child,lists,depth+1,started);
}
static bool showWindowSpaceThroughMissionControl(CGWindowID wid) {
  NSDictionary *target=spaceMetadata(windowSpaces(wid));if(!target[@"sid"])return false;
  if(windowOnCurrentSpace(wid))return true;
  NSURL *url=[NSURL fileURLWithPath:@"/System/Applications/Mission Control.app"];
  if(![NSWorkspace.sharedWorkspace openURL:url])return false;
  auto started=std::chrono::steady_clock::now();bool pressed=false;
  while(std::chrono::steady_clock::now()-started<std::chrono::milliseconds(1300)&&!sessionLocked()) {
    for(NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
      if(![app.bundleIdentifier isEqual:@"com.apple.WindowManager"]&&![app.bundleIdentifier isEqual:@"com.apple.dock"])continue;
      AXUIElementRef root=AXUIElementCreateApplication(app.processIdentifier);NSMutableArray *lists=[NSMutableArray array];findSpaceLists(root,lists,0,std::chrono::steady_clock::now());CFRelease(root);
      for(id value in lists) {
        AXUIElementRef list=(__bridge AXUIElementRef)value;NSMutableArray *buttons=[NSMutableArray array];
        for(id child in attribute(list,kAXChildrenAttribute))if([attribute((__bridge AXUIElementRef)child,kAXRoleAttribute)isEqual:@"AXButton"]&&![attribute((__bridge AXUIElementRef)child,CFSTR("AXIdentifier"))isEqual:@"mc.spaces.add"])[buttons addObject:child];
        if(buttons.count!=[target[@"spaceCount"]unsignedIntegerValue])continue;
        // With separate displays, require the list's screen to match the target Space's display.
        bool displayMatches=lists.count==1 && managedDisplays().count==1;
        if(!displayMatches) {
          id position=attribute(list,kAXPositionAttribute);CGPoint point={};
          if(position && AXValueGetType((__bridge AXValueRef)position)==kAXValueCGPointType && AXValueGetValue((__bridge AXValueRef)position,static_cast<AXValueType>(kAXValueCGPointType),&point)) {
            for(NSScreen *screen in NSScreen.screens){CGDirectDisplayID did=[screen.deviceDescription[@"NSScreenNumber"]unsignedIntValue];CFUUIDRef uuid=CGDisplayCreateUUIDFromDisplayID(did);NSString *name=uuid?CFBridgingRelease(CFUUIDCreateString(nullptr,uuid)):@"";if(uuid)CFRelease(uuid);if([name isEqual:target[@"display"]]&&CGRectContainsPoint(CGDisplayBounds(did),point))displayMatches=true;}
          }
        }
        NSUInteger index=[target[@"index"]unsignedIntegerValue];
        if(displayMatches && index<buttons.count && AXUIElementPerformAction((__bridge AXUIElementRef)buttons[index],kAXPressAction)==kAXErrorSuccess){pressed=true;break;}
      }
      if(pressed)break;
    }
    if(pressed)break;
    [NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.04]];
  }
  if(!pressed) {
    // Close the overview we opened; do not leave the user's desktop in Mission Control on failure.
    bool open=false;
    for(NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications)if([app.bundleIdentifier isEqual:@"com.apple.WindowManager"]||[app.bundleIdentifier isEqual:@"com.apple.dock"]){AXUIElementRef root=AXUIElementCreateApplication(app.processIdentifier);NSMutableArray *lists=[NSMutableArray array];findSpaceLists(root,lists,0,std::chrono::steady_clock::now());CFRelease(root);if(lists.count)open=true;}
    if(open)for(bool down:{true,false}){CGEventRef event=CGEventCreateKeyboardEvent(nullptr,53,down);CGEventPost(kCGSessionEventTap,event);CFRelease(event);}
    return false;
  }
  NSDate *deadline=[NSDate dateWithTimeIntervalSinceNow:.9];
  while(!windowOnCurrentSpace(wid)&&deadline.timeIntervalSinceNow>0&&!sessionLocked())[NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.03]];
  return windowOnCurrentSpace(wid);
}
