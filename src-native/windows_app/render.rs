use super::*;
pub struct Surface {
    pub pixels: *mut u8,
    pub dc: HDC,
    pub bitmap: HBITMAP,
    pub old: HGDIOBJ,
    pub width: i32,
    pub height: i32,
}
impl Surface {
    /// DIB lifetime follows the MIT PiPie native renderer pattern; the selected GDI object is restored.
    pub unsafe fn new(width: i32, height: i32) -> Option<Self> {
        let screen = GetDC(None);
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut pixels = ptr::null_mut();
        let bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut pixels, None, 0);
        let dc = CreateCompatibleDC(screen);
        let _ = ReleaseDC(None, screen);
        let Ok(bitmap) = bitmap else {
            let _ = DeleteDC(dc);
            return None;
        };
        if dc.is_invalid() || pixels.is_null() {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(dc);
            return None;
        }
        let old = SelectObject(dc, HGDIOBJ(bitmap.0));
        Some(Self {
            pixels: pixels as *mut u8,
            dc,
            bitmap,
            old,
            width,
            height,
        })
    }
    pub unsafe fn buffer(&mut self) -> &mut [u8] {
        std::slice::from_raw_parts_mut(self.pixels, (self.width * self.height * 4) as usize)
    }
    pub unsafe fn present(&self, hwnd: HWND, x: i32, y: i32, alpha: u8) -> bool {
        UpdateLayeredWindow(
            hwnd,
            HDC(ptr::null_mut()),
            Some(&POINT { x, y }),
            Some(&SIZE {
                cx: self.width,
                cy: self.height,
            }),
            self.dc,
            Some(&POINT::default()),
            COLORREF(0),
            Some(&BLENDFUNCTION {
                BlendOp: 0,
                BlendFlags: 0,
                SourceConstantAlpha: alpha,
                AlphaFormat: 1,
            }),
            ULW_ALPHA,
        )
        .is_ok()
    }
    pub unsafe fn text(&mut self, value: &str, x: i32, y: i32, w: i32, h: i32, color: [u8; 3]) {
        let Some(mask) = Self::new(w, h) else { return };
        std::ptr::write_bytes(mask.pixels, 0, (w * h * 4) as usize);
        let font = CreateFontW(
            -14,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            0,
            0,
            ANTIALIASED_QUALITY.0 as u32,
            0,
            w!("Segoe UI"),
        );
        let old = SelectObject(mask.dc, HGDIOBJ(font.0));
        SetTextColor(mask.dc, COLORREF(0xffffff));
        SetBkMode(mask.dc, TRANSPARENT);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        let mut s = wide(value);
        DrawTextW(
            mask.dc,
            &mut s,
            &mut rect,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );
        for yy in 0..h {
            for xx in 0..w {
                let p =
                    std::slice::from_raw_parts(mask.pixels.add(((yy * w + xx) * 4) as usize), 4);
                let a = p[0].max(p[1]).max(p[2]);
                if a > 0 {
                    self.pixel(x + xx, y + yy, [color[0], color[1], color[2], a]);
                }
            }
        }
        SelectObject(mask.dc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
    pub unsafe fn pixel(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let p = self.pixels.add(((y * self.width + x) * 4) as usize);
        let a = c[3] as u32;
        for (i, v) in [c[2], c[1], c[0]].into_iter().enumerate() {
            *p.add(i) = ((v as u32 * a + *p.add(i) as u32 * (255 - a)) / 255) as u8;
        }
        *p.add(3) = (a + *p.add(3) as u32 * (255 - a) / 255) as u8;
    }
    pub unsafe fn circle(&mut self, cx: f64, cy: f64, r: f64, c: [u8; 4]) {
        for y in (cy - r - 1.) as i32..=(cy + r + 1.) as i32 {
            for x in (cx - r - 1.) as i32..=(cx + r + 1.) as i32 {
                let coverage = (r - (x as f64 - cx).hypot(y as f64 - cy) + 0.5).clamp(0., 1.);
                if coverage > 0. {
                    let mut color = c;
                    color[3] = (color[3] as f64 * coverage) as u8;
                    self.pixel(x, y, color);
                }
            }
        }
    }
    pub unsafe fn sprite(
        &mut self,
        sheet: &Sheet,
        row: usize,
        frame: usize,
        cx: f64,
        cy: f64,
        size: f64,
    ) {
        let (n, x, y, w, h) = ROWS[row];
        let sx = x + frame % n * 192;
        let scale = size / (w.max(h) as f64);
        let dw = (w as f64 * scale) as i32;
        let dh = (h as f64 * scale) as i32;
        for yy in 0..dh {
            for xx in 0..dw {
                let px = sx + (xx as f64 / scale) as usize;
                let py = y + (yy as f64 / scale) as usize;
                let i = (py * sheet.width + px) * 4;
                if i + 3 < sheet.rgba.len() {
                    self.pixel(
                        cx as i32 - dw / 2 + xx,
                        cy as i32 - dh / 2 + yy,
                        [
                            sheet.rgba[i],
                            sheet.rgba[i + 1],
                            sheet.rgba[i + 2],
                            sheet.rgba[i + 3],
                        ],
                    );
                }
            }
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.dc);
        }
    }
}
pub struct Sheet {
    pub rgba: Vec<u8>,
    pub width: usize,
}
impl Sheet {
    pub fn load() -> Option<Self> {
        let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!(
            "../../assets/mascot-sheet.png"
        )));
        let mut reader = decoder.read_info().ok()?;
        let mut bytes = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut bytes).ok()?;
        bytes.truncate(info.buffer_size());
        if info.color_type != png::ColorType::Rgba {
            return None;
        }
        Some(Self {
            rgba: bytes,
            width: info.width as usize,
        })
    }
}
const ROWS: [(usize, usize, usize, usize, usize); 11] = [
    (7, 25, 64, 141, 132),
    (8, 40, 292, 112, 112),
    (8, 33, 501, 125, 111),
    (4, 41, 708, 109, 112),
    (5, 23, 896, 145, 132),
    (8, 22, 1132, 148, 104),
    (6, 30, 1312, 132, 132),
    (6, 8, 1503, 176, 149),
    (6, 8, 1714, 176, 146),
    (8, 51, 1936, 68, 131),
    (8, 43, 2145, 68, 131),
];

#[derive(Default)]
pub struct Icons(std::collections::HashMap<String, Option<Vec<u8>>>);
impl Icons {
    pub unsafe fn draw(
        &mut self,
        surface: &mut Surface,
        path: &str,
        name: &str,
        cx: f64,
        cy: f64,
        size: i32,
    ) {
        // Decode once per source, including failed Shell lookups. Cache premultiplied BGRA.
        const SIDE: i32 = 64;
        let pixels = self.0.entry(path.to_string()).or_insert_with(|| {
            let mut info = SHFILEINFOW::default();
            if !std::path::Path::new(path).is_file() {
                let shell_path = format!("shell:AppsFolder\\{path}");
                let factory = SHCreateItemFromParsingName::<_, _, IShellItemImageFactory>(
                    PCWSTR(wide(&shell_path).as_ptr()),
                    None,
                );
                if let Ok(bitmap) = factory.and_then(|f| {
                    f.GetImage(
                        SIZE { cx: SIDE, cy: SIDE },
                        SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
                    )
                }) {
                    let mut info = BITMAPINFO::default();
                    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                    info.bmiHeader.biWidth = SIDE;
                    info.bmiHeader.biHeight = -SIDE;
                    info.bmiHeader.biPlanes = 1;
                    info.bmiHeader.biBitCount = 32;
                    let dc = GetDC(None);
                    let mut bytes = vec![0u8; (SIDE * SIDE * 4) as usize];
                    let lines = GetDIBits(
                        dc,
                        bitmap,
                        0,
                        SIDE as u32,
                        Some(bytes.as_mut_ptr() as *mut c_void),
                        &mut info,
                        DIB_RGB_COLORS,
                    );
                    let _ = ReleaseDC(None, dc);
                    let _ = DeleteObject(HGDIOBJ(bitmap.0));
                    if lines == SIDE {
                        return Some(bytes);
                    }
                }
            }
            if SHGetFileInfoW(
                PCWSTR(wide(path).as_ptr()),
                FILE_ATTRIBUTE_NORMAL,
                Some(&mut info),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            ) == 0
            {
                return None;
            }
            let result = Surface::new(SIDE, SIDE).map(|mut icon| {
                icon.buffer().fill(0);
                let _ = DrawIconEx(icon.dc, 0, 0, info.hIcon, SIDE, SIDE, 0, None, DI_NORMAL);
                icon.buffer().to_vec()
            });
            let _ = DestroyIcon(info.hIcon);
            result
        });
        if let Some(bytes) = pixels {
            for y in 0..size {
                for x in 0..size {
                    let index = (((y * SIDE / size) * SIDE + x * SIDE / size) * 4) as usize;
                    let p = &bytes[index..index + 4];
                    let alpha = p[3] as u32;
                    if alpha > 0 {
                        let unpremul = |v: u8| (v as u32 * 255 / alpha).min(255) as u8;
                        surface.pixel(
                            cx as i32 - size / 2 + x,
                            cy as i32 - size / 2 + y,
                            [unpremul(p[2]), unpremul(p[1]), unpremul(p[0]), p[3]],
                        );
                    }
                }
            }
        } else {
            surface.circle(cx, cy, size as f64 * 0.45, [210, 220, 232, 245]);
            surface.text(
                &name.chars().take(2).collect::<String>(),
                cx as i32 - size / 2,
                cy as i32 - size / 2,
                size,
                size,
                [43, 61, 80],
            );
        }
    }
}
