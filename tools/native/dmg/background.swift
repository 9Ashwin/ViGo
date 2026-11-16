import AppKit
let width:CGFloat=640,height:CGFloat=400
let image=NSImage(size:NSSize(width:width,height:height))
for scale:CGFloat in [1,2] {
 let rep=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:Int(width*scale),pixelsHigh:Int(height*scale),bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:0,bitsPerPixel:0)!
 rep.size=NSSize(width:width,height:height)
 let context=NSGraphicsContext(bitmapImageRep:rep)!
 NSGraphicsContext.saveGraphicsState();NSGraphicsContext.current=context
 let gradient=NSGradient(starting:NSColor(srgbRed:0.97,green:0.95,blue:0.91,alpha:1),ending:NSColor(srgbRed:0.90,green:0.93,blue:0.91,alpha:1))!
 gradient.draw(in:NSRect(x:0,y:0,width:width,height:height),angle:20)
 func label(_ text:String,_ frame:NSRect,_ size:CGFloat,_ bold:Bool=false) {
  let style=NSMutableParagraphStyle();style.alignment = .center
  (text as NSString).draw(in:frame,withAttributes:[.font:NSFont.systemFont(ofSize:size,weight:bold ? .semibold : .regular),.foregroundColor:NSColor(srgbRed:0.24,green:0.23,blue:0.21,alpha:1),.paragraphStyle:style])
 }
 label("把 ViGo 放进应用程序",NSRect(x:30,y:322,width:580,height:38),25,true)
 label("拖动左侧小猫到右侧文件夹，即可安装。",NSRect(x:30,y:286,width:580,height:26),13)
 let rope=NSBezierPath();rope.move(to:NSPoint(x:268,y:191));rope.curve(to:NSPoint(x:372,y:191),controlPoint1:NSPoint(x:297,y:202),controlPoint2:NSPoint(x:342,y:202));rope.lineWidth=2.3;rope.lineCapStyle = .round;NSColor(srgbRed:0.64,green:0.52,blue:0.36,alpha:0.8).setStroke();rope.stroke()
 let arrow=NSBezierPath();arrow.move(to:NSPoint(x:362,y:199));arrow.line(to:NSPoint(x:373,y:191));arrow.line(to:NSPoint(x:362,y:184));arrow.lineWidth=2.3;arrow.lineCapStyle = .round;arrow.stroke()
 label("更新前先退出旧版，再拖入并选择“替换”",NSRect(x:30,y:72,width:580,height:24),14,true)
 label("按住 V · 移动选择 · 松开打开",NSRect(x:30,y:43,width:580,height:24),12)
 NSGraphicsContext.restoreGraphicsState();image.addRepresentation(rep)
}
let out=CommandLine.arguments[1]
try image.tiffRepresentation!.write(to:URL(fileURLWithPath:out))
