#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/runtime.h>
#include <node_api.h>
#include <cstring>
#include <cmath>

static char effectsKey;
// All calls originate in Electron's main process, on AppKit's main thread.
static NSView *host(napi_env env,napi_value value) {
  bool buffer=false;napi_is_buffer(env,value,&buffer);if(!buffer)return nil;
  void *bytes=nullptr;size_t size=0;napi_get_buffer_info(env,value,&bytes,&size);if(size!=sizeof(void *))return nil;
  void *pointer=nullptr;memcpy(&pointer,bytes,sizeof(pointer));if(!pointer)return nil;
  id object=(__bridge id)pointer;
  if([object isKindOfClass:NSWindow.class])return [object contentView];
  return [object isKindOfClass:NSView.class]?object:nil;
}
static double number(napi_env env,napi_value object,const char *key) {
  napi_value v;double n=0;if(napi_get_named_property(env,object,key,&v)==napi_ok)napi_get_value_double(env,v,&n);return std::isfinite(n)?n:0;
}
static napi_value update(napi_env env,napi_callback_info info) {
  size_t argc=3;napi_value args[3];napi_get_cb_info(env,info,&argc,args,nullptr,nullptr);
  if(argc<3||!NSThread.isMainThread){napi_throw_error(env,nullptr,"Glass requires a main-thread window handle");return nullptr;}
  NSView *view=host(env,args[0]);if(!view){napi_throw_error(env,nullptr,"Invalid native window handle");return nullptr;}
  NSRect parentBounds=view.bounds;
  if(!std::isfinite(parentBounds.origin.x)||!std::isfinite(parentBounds.origin.y)||!std::isfinite(parentBounds.size.width)||!std::isfinite(parentBounds.size.height)||parentBounds.size.width<1||parentBounds.size.height<1){napi_throw_error(env,nullptr,"Window geometry is not ready");return nullptr;}
  // A global non-activating overlay joins the active app's Stage Manager set.
  if(@available(macOS 13.0,*)){
    auto behavior=view.window.collectionBehavior;
    behavior &= ~(NSWindowCollectionBehaviorPrimary|NSWindowCollectionBehaviorAuxiliary);
    behavior |= NSWindowCollectionBehaviorCanJoinAllApplications;
    if(view.window.collectionBehavior!=behavior)view.window.collectionBehavior=behavior;
  }
  uint32_t length=0;napi_get_array_length(env,args[1],&length);length=MIN(length,16u);
  bool dark=false;napi_get_value_bool(env,args[2],&dark);
  NSMutableArray<NSView *> *effects=objc_getAssociatedObject(view,&effectsKey);
  if(!effects){effects=[NSMutableArray array];objc_setAssociatedObject(view,&effectsKey,effects,OBJC_ASSOCIATION_RETAIN_NONATOMIC);}
  while(effects.count>length){[effects.lastObject removeFromSuperview];[effects removeLastObject];}
  while(effects.count<length){
    NSView *effect;
    if(@available(macOS 26.0,*)){NSGlassEffectView *glass=[[NSGlassEffectView alloc] initWithFrame:NSMakeRect(0,0,16,16)];glass.style=NSGlassEffectViewStyleClear;effect=glass;}
    else {NSVisualEffectView *frost=[[NSVisualEffectView alloc] initWithFrame:NSMakeRect(0,0,16,16)];frost.blendingMode=NSVisualEffectBlendingModeBehindWindow;frost.material=NSVisualEffectMaterialPopover;frost.state=NSVisualEffectStateActive;effect=frost;}
    if(![effect isKindOfClass:NSClassFromString(@"NSGlassEffectView")]){effect.wantsLayer=YES;effect.layer.masksToBounds=YES;}
    effect.autoresizingMask=NSViewNotSizable;
    [view addSubview:effect positioned:NSWindowBelow relativeTo:view.subviews.firstObject];[effects addObject:effect];
  }
  for(uint32_t i=0;i<length;i++){
    napi_value rect;napi_get_element(env,args[1],i,&rect);double x=number(env,rect,"x"),y=number(env,rect,"y"),w=MAX(1,number(env,rect,"width")),h=MAX(1,number(env,rect,"height"));
    NSView *effect=effects[i];
    NSRect proposed=NSMakeRect(x,view.isFlipped?y:parentBounds.size.height-y-h,w,h);
    NSRect visible=NSIntersectionRect(proposed,parentBounds);
    if(NSIsEmptyRect(visible)||visible.size.width<4||visible.size.height<4){effect.hidden=YES;continue;}
    effect.hidden=NO;
    if(@available(macOS 26.0,*)){if([effect isKindOfClass:NSGlassEffectView.class])[(NSGlassEffectView *)effect setCornerRadius:0];}
    effect.frame=visible;w=visible.size.width;h=visible.size.height;
    double radius=MAX(0,MIN(number(env,rect,"radius"),MIN(w,h)/2-.25));
    if(@available(macOS 26.0,*)){if([effect isKindOfClass:NSGlassEffectView.class])[(NSGlassEffectView *)effect setCornerRadius:radius];}
    if(![effect isKindOfClass:NSClassFromString(@"NSGlassEffectView")])effect.layer.cornerRadius=radius;
    effect.appearance=[NSAppearance appearanceNamed:dark?NSAppearanceNameDarkAqua:NSAppearanceNameAqua];
  }
  napi_value result;napi_create_uint32(env,(uint32_t)effects.count,&result);return result;
}
static napi_value init(napi_env env,napi_value exports){napi_value fn;napi_create_function(env,"update",NAPI_AUTO_LENGTH,update,nullptr,&fn);napi_set_named_property(env,exports,"update",fn);return exports;}
NAPI_MODULE(NODE_GYP_MODULE_NAME,init)
