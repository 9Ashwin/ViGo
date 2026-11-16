//! Local mascot assets and the public Codex Pets package format. No executable package contents.
use crate::{model::Settings, platform};
use objc2::{msg_send, rc::Retained};
use objc2_foundation::{ns_string, NSDictionary, NSString, NSURL};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub type Result<T> = std::result::Result<T, String>;
const LIMIT: usize = 24 * 1024 * 1024;
pub const FRAMES: [usize; 11] = [7, 8, 8, 4, 5, 8, 6, 6, 6, 8, 8];
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    DefaultCat,
    CodexV1,
    CodexV2,
    Static,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub image: String,
    pub layout: Layout,
    pub width: u32,
    pub height: u32,
    pub bounds: Vec<Bounds>,
    /// Derived from image alpha when selected; old packages need no metadata migration.
    #[serde(skip)]
    pub visible_frames: Vec<Vec<bool>>,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub author: String,
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Pet {
    display_name: String,
    spritesheet_path: String,
    sprite_version_number: Option<u8>,
}
pub fn library() -> PathBuf {
    Settings::path().parent().unwrap().join("mascots")
}
fn requests() -> PathBuf {
    Settings::path().parent().unwrap().join("mascot-requests")
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectionRequest {
    pub request_id: String,
    pub asset_id: String,
}
#[derive(Serialize, Deserialize)]
struct SelectionResponse {
    request_id: String,
    applied: bool,
    error: Option<String>,
}
pub fn pending_requests() -> Vec<SelectionRequest> {
    let mut result: Vec<_> = fs::read_dir(requests())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".request.json") {
                return None;
            }
            let request: SelectionRequest =
                serde_json::from_slice(&read_file(&entry.path(), 8192).ok()?).ok()?;
            if !valid_id(&request.request_id)
                || !valid_id(&request.asset_id)
                || name != format!("{}.request.json", request.request_id)
            {
                return None;
            }
            Some(request)
        })
        .collect();
    result.sort_by_key(|r| r.request_id.clone());
    result
}
pub fn respond(request: &SelectionRequest, result: Result<()>) {
    let response = SelectionResponse {
        request_id: request.request_id.clone(),
        applied: result.is_ok(),
        error: result.err(),
    };
    let path = requests().join(format!("{}.response.json", request.request_id));
    let temp = path.with_extension("tmp");
    if fs::write(&temp, serde_json::to_vec(&response).unwrap()).is_ok() {
        let _ = fs::rename(temp, path);
        let _ = fs::remove_file(requests().join(format!("{}.request.json", request.request_id)));
    }
}
fn queue_selection(asset_id: &str) -> Result<String> {
    fs::create_dir_all(requests()).map_err(|e| e.to_string())?;
    let request_id = unique();
    let request = SelectionRequest {
        request_id: request_id.clone(),
        asset_id: asset_id.into(),
    };
    let target = requests().join(format!("{request_id}.request.json"));
    let temp = target.with_extension("tmp");
    fs::write(&temp, serde_json::to_vec(&request).unwrap()).map_err(|e| e.to_string())?;
    fs::rename(temp, target).map_err(|e| e.to_string())?;
    Ok(request_id)
}
fn app_running() -> bool {
    use std::os::fd::AsRawFd;
    let Ok(file) = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(Settings::path().with_extension("native-lock"))
    else {
        return false;
    };
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if locked == 0 {
        unsafe {
            libc::flock(file.as_raw_fd(), libc::LOCK_UN);
        }
        false
    } else {
        true
    }
}
pub fn command_line(args: &[String]) -> Option<std::process::ExitCode> {
    if args
        .get(1)
        .is_none_or(|a| a != "--import-companion" && a != "--import-mascot")
    {
        return None;
    }
    let result = objc2::rc::autoreleasepool(|_| -> Result<serde_json::Value> {
        let path = args
            .get(2)
            .filter(|_| args.len() == 3)
            .ok_or("用法：ViGo --import-companion <素材文件或文件夹绝对路径>")?;
        let asset = import_local(Path::new(path))?;
        let request_id = queue_selection(&asset.id)?;
        if !app_running() && Path::new("/Applications/ViGo.app").exists() {
            let _ = Command::new("/usr/bin/open")
                .args(["-n", "-g", "-a", "/Applications/ViGo.app"])
                .status();
        }
        let response_path = requests().join(format!("{request_id}.response.json"));
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            if let Ok(bytes) = read_file(&response_path, 8192) {
                if let Ok(response) = serde_json::from_slice::<SelectionResponse>(&bytes) {
                    if response.request_id == request_id {
                        let _ = fs::remove_file(&response_path);
                        if let Some(error) = response.error {
                            return Err(error);
                        }
                        return Ok(
                            serde_json::json!({"ok":true,"imported":true,"applied":response.applied,"id":asset.id,"name":asset.name}),
                        );
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(80));
        }
        Ok(
            serde_json::json!({"ok":true,"imported":true,"applied":false,"id":asset.id,"name":asset.name,"message":"素材已导入，打开 ViGo 后会自动使用"}),
        )
    });
    Some(match result {
        Ok(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", serde_json::json!({"ok":false,"error":error}));
            std::process::ExitCode::FAILURE
        }
    })
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
pub fn image_path(asset: &Asset) -> PathBuf {
    if asset.layout == Layout::DefaultCat {
        platform::resource("mascot-sheet.png")
    } else {
        library().join(&asset.id).join(&asset.image)
    }
}
pub fn default_asset() -> Asset {
    let boxes = [
        (25, 64, 141, 132),
        (40, 292, 112, 112),
        (33, 501, 125, 111),
        (41, 708, 109, 112),
        (23, 896, 145, 132),
        (22, 1132, 148, 104),
        (30, 1312, 132, 132),
        (8, 1503, 176, 149),
        (8, 1714, 176, 146),
        (51, 1936, 68, 132),
        (43, 2145, 68, 131),
    ];
    Asset {
        id: "default".into(),
        name: "默认小猫".into(),
        image: "mascot-sheet.png".into(),
        layout: Layout::DefaultCat,
        width: 1536,
        height: 2288,
        bounds: boxes
            .into_iter()
            .enumerate()
            .map(|(row, (x, y, w, h))| Bounds {
                x,
                y: y - row as u32 * 208,
                width: w,
                height: h,
            })
            .collect(),
        visible_frames: vec![],
        source_url: String::new(),
        author: String::new(),
    }
}
impl Asset {
    pub fn frame(&self, row: usize, frame: usize) -> Bounds {
        let mut row = if self.layout == Layout::Static || self.layout == Layout::CodexV1 && row >= 9
        {
            0
        } else {
            row.min(self.bounds.len().saturating_sub(1))
        };
        let count = if self.layout == Layout::Static {
            1
        } else if self.layout == Layout::CodexV1 && row == 0 {
            6
        } else {
            FRAMES[row]
        };
        let mut frame = frame % count;
        if let Some(visible) = self.visible_frames.get(row) {
            if !visible.get(frame).copied().unwrap_or(false) {
                if let Some(next) = (0..count)
                    .map(|offset| (frame + offset) % count)
                    .find(|&i| visible.get(i).copied().unwrap_or(false))
                {
                    frame = next;
                } else if let Some((fallback, index)) = self
                    .visible_frames
                    .iter()
                    .enumerate()
                    .find_map(|(r, frames)| frames.iter().position(|&v| v).map(|i| (r, i)))
                {
                    row = fallback;
                    frame = index;
                }
            }
        }
        let b = self.bounds[row];
        Bounds {
            x: b.x
                + frame as u32
                    * if self.layout == Layout::Static {
                        0
                    } else {
                        192
                    },
            y: b.y
                + row as u32
                    * if self.layout == Layout::Static {
                        0
                    } else {
                        208
                    },
            ..b
        }
    }
    fn validate(&self) -> Result<()> {
        if !valid_id(&self.id)
            || !safe_member(&self.image)
            || Path::new(&self.image).components().count() != 1
        {
            return Err("素材路径无效".into());
        }
        let rows = match self.layout {
            Layout::CodexV1 => 9,
            Layout::CodexV2 => 11,
            Layout::Static => 1,
            Layout::DefaultCat => return Err("导入包不能覆盖默认素材".into()),
        };
        if self.bounds.len() != rows
            || self.width == 0
            || self.height == 0
            || self.width > 4096
            || self.height > 4096
            || self.width as u64 * self.height as u64 > 4_194_304
        {
            return Err("素材尺寸或帧信息无效".into());
        }
        let (cw, ch) = if self.layout == Layout::Static {
            (self.width, self.height)
        } else {
            (192, 208)
        };
        if self.layout != Layout::Static && (self.width != 1536 || self.height != 208 * rows as u32)
        {
            return Err("图集尺寸与版本不匹配".into());
        }
        if self.bounds.iter().any(|b| {
            b.width == 0
                || b.height == 0
                || b.x.checked_add(b.width).is_none_or(|v| v > cw)
                || b.y.checked_add(b.height).is_none_or(|v| v > ch)
        }) {
            return Err("素材裁切区域无效".into());
        }
        Ok(())
    }
}
pub fn load(id: &str) -> Result<Asset> {
    if !valid_id(id) {
        return Err("素材标识无效".into());
    }
    let root = library().join(id);
    if fs::symlink_metadata(&root)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("素材目录不能是链接".into());
    }
    let asset: Asset =
        serde_json::from_slice(&read_file(&root.join("vigo-mascot.json"), 64 * 1024)?)
            .map_err(|_| "素材配置无法读取")?;
    asset.validate()?;
    if asset.id != id {
        return Err("素材标识不匹配".into());
    }
    let image = image_path(&asset);
    let meta = fs::symlink_metadata(&image).map_err(|_| "素材图片已被移走")?;
    if !meta.is_file() || meta.len() as usize > LIMIT {
        return Err("素材图片无效".into());
    }
    Ok(asset)
}
pub fn list() -> Vec<Asset> {
    let mut assets: Vec<_> = fs::read_dir(library())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| load(&entry.file_name().to_string_lossy()).ok())
        .collect();
    assets.sort_by_key(|a| (a.name.to_lowercase(), a.id.clone()));
    assets
}
fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() as usize > limit {
        return Err("素材文件过大，或不是普通文件".into());
    }
    let mut data = vec![];
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > limit {
        return Err("素材文件过大".into());
    }
    Ok(data)
}
fn output(mut cmd: Command, limit: usize, timeout: Duration) -> Result<Vec<u8>> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = vec![];
        stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("读取素材超时，请重试".into());
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| "读取素材失败")?
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("素材文件超过大小限制".into());
    }
    if !status.success() {
        return Err("无法读取素材，请检查文件或网络".into());
    }
    Ok(bytes)
}
pub fn safe_member(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.len() < 512
        && !name
            .chars()
            .any(|c| c.is_control() || "*?[]\\:".contains(c))
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
fn zip_bytes(path: &Path, member: &str, limit: usize) -> Result<Vec<u8>> {
    if !safe_member(member) {
        return Err("宠物包包含无效路径".into());
    }
    let mut cmd = Command::new("/usr/bin/unzip");
    cmd.arg("-p").arg(path).arg(member);
    output(cmd, limit, Duration::from_secs(12))
}
fn parse_pet(data: &[u8]) -> Result<Pet> {
    serde_json::from_slice(data).map_err(|_| "pet.json 格式不正确".into())
}
fn local_bytes(path: &Path) -> Result<(Vec<u8>, Option<Pet>)> {
    if path.is_dir() {
        let root = path.canonicalize().map_err(|e| e.to_string())?;
        let pet = parse_pet(&read_file(&root.join("pet.json"), 64 * 1024)?)?;
        if !safe_member(&pet.spritesheet_path) {
            return Err("pet.json 图片路径无效".into());
        }
        let image = root.join(&pet.spritesheet_path);
        let canonical = image.canonicalize().map_err(|_| "没有找到图集")?;
        if !canonical.starts_with(&root) {
            return Err("素材图片必须在所选文件夹内".into());
        }
        return Ok((read_file(&image, LIMIT)?, Some(pet)));
    }
    if path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("json"))
    {
        return local_bytes(path.parent().ok_or("文件位置无效")?);
    }
    if path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("zip"))
    {
        let mut cmd = Command::new("/usr/bin/unzip");
        cmd.args(["-Z", "-1"]).arg(path);
        let entries = String::from_utf8(output(cmd, 128 * 1024, Duration::from_secs(10))?)
            .map_err(|_| "宠物包目录无效")?;
        let manifests: Vec<_> = entries
            .lines()
            .filter(|p| {
                Path::new(p).file_name().is_some_and(|s| s == "pet.json")
                    && !p.starts_with("__MACOSX/")
            })
            .collect();
        if manifests.len() != 1 {
            return Err("宠物包需要一个 pet.json".into());
        }
        let manifest = manifests[0];
        let pet = parse_pet(&zip_bytes(path, manifest, 64 * 1024)?)?;
        if !safe_member(&pet.spritesheet_path) {
            return Err("pet.json 图片路径无效".into());
        }
        let member = Path::new(manifest)
            .parent()
            .unwrap()
            .join(&pet.spritesheet_path)
            .to_string_lossy()
            .into_owned();
        if entries.lines().filter(|p| *p == member).count() != 1 {
            return Err("宠物包的图集缺失或重复".into());
        }
        return Ok((zip_bytes(path, &member, LIMIT)?, Some(pet)));
    }
    Ok((read_file(path, LIMIT)?, None))
}
pub fn pet_id(value: &str) -> Result<String> {
    let value = value.trim();
    let rest = value
        .strip_prefix("https://codex-pets.net/")
        .ok_or("请粘贴 codex-pets.net 的宠物详情链接")?;
    let route = rest
        .strip_prefix("#/pets/")
        .or_else(|| rest.strip_prefix("pets/"))
        .ok_or("链接需要指向一个宠物详情页")?;
    let id = route.split(['?', '#', '/']).next().unwrap_or_default();
    if !valid_id(id) {
        return Err("宠物链接标识无效".into());
    }
    Ok(id.into())
}
fn download(url: &str, limit: usize) -> Result<Vec<u8>> {
    if !url.starts_with("https://codex-pets.net/") {
        return Err("素材下载地址必须来自 codex-pets.net".into());
    }
    let mut cmd = Command::new("/usr/bin/curl");
    cmd.args([
        "--silent",
        "--show-error",
        "--fail",
        "--proto",
        "=https",
        "--connect-timeout",
        "10",
        "--max-time",
        "40",
        url,
    ]);
    output(cmd, limit, Duration::from_secs(45))
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
fn unique() -> String {
    format!(
        "mascot-{}-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    )
}
pub fn import_local(path: &Path) -> Result<Asset> {
    let (data, pet) = local_bytes(path)?;
    let name = pet
        .as_ref()
        .map(|p| p.display_name.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    install(data, pet, name, String::new(), String::new())
}
pub fn import_site(url: &str) -> Result<Asset> {
    let id = pet_id(url)?;
    let doc: serde_json::Value = serde_json::from_slice(&download(
        &format!("https://codex-pets.net/api/pets/{id}"),
        128 * 1024,
    )?)
    .map_err(|_| "无法读取网站素材信息")?;
    let pet = &doc["pet"];
    let data = download(
        pet["spritesheetUrl"].as_str().ok_or("网站没有提供图集")?,
        LIMIT,
    )?;
    let manifest = serde_json::from_value::<Pet>(pet.clone()).map_err(|_| "网站素材配置无效")?;
    install(
        data,
        Some(manifest),
        pet["displayName"].as_str().unwrap_or(&id).into(),
        format!("https://codex-pets.net/#/pets/{id}"),
        pet["ownerName"].as_str().unwrap_or_default().into(),
    )
}
fn install(
    data: Vec<u8>,
    pet: Option<Pet>,
    name: String,
    source_url: String,
    author: String,
) -> Result<Asset> {
    fs::create_dir_all(library()).map_err(|e| e.to_string())?;
    let id = unique();
    let stage = library().join(format!(".{id}"));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let scratch = Scratch(stage);
    let temp = scratch.0.join("image");
    fs::write(&temp, data).map_err(|e| e.to_string())?;
    let Inspection {
        layout,
        width,
        height,
        bounds,
        ext,
        visible_frames,
    } = inspect(
        &temp,
        pet.as_ref()
            .map(|p| p.sprite_version_number)
            .unwrap_or(None),
    )?;
    let image = format!("image.{ext}");
    fs::rename(&temp, scratch.0.join(&image)).map_err(|e| e.to_string())?;
    let asset = Asset {
        id: id.clone(),
        name: name.chars().filter(|c| !c.is_control()).take(80).collect(),
        image,
        layout,
        width,
        height,
        bounds,
        visible_frames,
        source_url,
        author,
    };
    asset.validate()?;
    fs::write(
        scratch.0.join("vigo-mascot.json"),
        serde_json::to_vec_pretty(&asset).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&scratch.0, library().join(id)).map_err(|e| e.to_string())?;
    Ok(asset)
}

type Ref = *mut std::ffi::c_void;
#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    fn CGImageSourceCreateWithURL(url: Ref, options: Ref) -> Ref;
    fn CGImageSourceCopyPropertiesAtIndex(source: Ref, index: usize, options: Ref) -> Ref;
    fn CGImageSourceCreateImageAtIndex(source: Ref, index: usize, options: Ref) -> Ref;
    fn CGImageSourceGetType(source: Ref) -> Ref;
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGImageGetDataProvider(image: Ref) -> Ref;
    fn CGDataProviderCopyData(provider: Ref) -> Ref;
    fn CGImageGetBitsPerPixel(image: Ref) -> usize;
    fn CGImageGetBitsPerComponent(image: Ref) -> usize;
    fn CGImageGetBytesPerRow(image: Ref) -> usize;
    fn CGImageGetAlphaInfo(image: Ref) -> u32;
    fn CGImageGetBitmapInfo(image: Ref) -> u32;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFDataGetLength(data: Ref) -> isize;
    fn CFDataGetBytePtr(data: Ref) -> *const u8;
}
struct Owned(Ref);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            platform::CFRelease(self.0);
        }
    }
}
fn owned(value: Ref) -> Result<Owned> {
    if value.is_null() {
        Err("无法解码素材图片".into())
    } else {
        Ok(Owned(value))
    }
}
fn classify(width: u32, height: u32, version: Option<u8>) -> Result<Layout> {
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || width as u64 * height as u64 > 4_194_304
    {
        return Err("图片最多支持 4096 像素边长、约 400 万像素".into());
    }
    let layout = match (width, height) {
        (1536, 2288) => Layout::CodexV2,
        (1536, 1872) => Layout::CodexV1,
        _ => Layout::Static,
    };
    if let Some(version) = version {
        if !matches!(
            (version, layout),
            (2, Layout::CodexV2) | (1, Layout::CodexV1)
        ) {
            return Err("pet.json 的版本与图集尺寸不匹配".into());
        }
    }
    Ok(layout)
}
struct Inspection {
    layout: Layout,
    width: u32,
    height: u32,
    bounds: Vec<Bounds>,
    ext: &'static str,
    visible_frames: Vec<Vec<bool>>,
}
pub fn populate_visibility(asset: &mut Asset) -> Result<()> {
    if asset.layout == Layout::DefaultCat || !asset.visible_frames.is_empty() {
        return Ok(());
    }
    let info = inspect(
        &image_path(asset),
        match asset.layout {
            Layout::CodexV1 => Some(1),
            Layout::CodexV2 => Some(2),
            _ => None,
        },
    )?;
    if info.width != asset.width || info.height != asset.height || info.layout != asset.layout {
        return Err("素材图片尺寸已变化，请重新导入".into());
    }
    asset.bounds = info.bounds;
    asset.visible_frames = info.visible_frames;
    Ok(())
}
fn inspect(path: &Path, version: Option<u8>) -> Result<Inspection> {
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let src = owned(CGImageSourceCreateWithURL(
            Retained::as_ptr(&url) as Ref,
            std::ptr::null_mut(),
        ))?;
        let kind = CGImageSourceGetType(src.0);
        if kind.is_null() {
            return Err("素材图片格式无法识别".into());
        }
        let kind = (&*(kind as *const NSString)).to_string();
        let ext = match kind.as_str() {
            "public.png" => "png",
            "org.webmproject.webp" => "webp",
            _ => return Err("请选择 PNG 或 WebP 图片".into()),
        };
        let properties = owned(CGImageSourceCopyPropertiesAtIndex(
            src.0,
            0,
            std::ptr::null_mut(),
        ))?;
        let dict = &*(properties.0 as *const NSDictionary<NSString, objc2::runtime::AnyObject>);
        let number = |key: &NSString| -> Result<u32> {
            let v = dict.objectForKey(key).ok_or("无法读取图片尺寸")?;
            let n: u64 = msg_send![&*v, unsignedLongLongValue];
            u32::try_from(n).map_err(|_| "图片尺寸过大".into())
        };
        let width = number(ns_string!("PixelWidth"))?;
        let height = number(ns_string!("PixelHeight"))?;
        let layout = classify(width, height, version)?;
        let rows = match layout {
            Layout::CodexV1 => 9,
            Layout::CodexV2 => 11,
            _ => 1,
        };
        let (cw, ch) = if layout == Layout::Static {
            (width, height)
        } else {
            (192, 208)
        };
        let image = owned(CGImageSourceCreateImageAtIndex(
            src.0,
            0,
            std::ptr::null_mut(),
        ))?;
        let mut bounds = vec![
            Bounds {
                x: 0,
                y: 0,
                width: cw,
                height: ch
            };
            rows
        ];
        let mut visible_frames = (0..rows)
            .map(|row| {
                vec![
                    true;
                    if layout == Layout::Static {
                        1
                    } else if row == 0 && layout == Layout::CodexV1 {
                        6
                    } else {
                        FRAMES[row]
                    }
                ]
            })
            .collect();
        if CGImageGetBitsPerPixel(image.0) == 32 && CGImageGetBitsPerComponent(image.0) == 8 {
            let alpha = CGImageGetAlphaInfo(image.0);
            let mut index = match alpha {
                1 | 3 => Some(3),
                2 | 4 => Some(0),
                _ => None,
            };
            if CGImageGetBitmapInfo(image.0) & 0x7000 == 0x2000 {
                index = index.map(|v| 3 - v);
            }
            if let Some(alpha) = index {
                let bytes = owned(CGDataProviderCopyData(CGImageGetDataProvider(image.0)))?;
                let stride = CGImageGetBytesPerRow(image.0);
                let length = CFDataGetLength(bytes.0);
                if length < 0
                    || stride < width as usize * 4
                    || stride
                        .checked_mul(height as usize)
                        .is_none_or(|n| n > length as usize)
                    || length as usize > 64 * 1024 * 1024
                {
                    return Err("图片像素数据异常".into());
                }
                let data = std::slice::from_raw_parts(CFDataGetBytePtr(bytes.0), length as usize);
                let analysis = analyze_pixels(data, width, height, stride, alpha, layout)?;
                bounds = analysis.bounds;
                visible_frames = analysis.visible_frames;
            }
        }
        Ok(Inspection {
            layout,
            width,
            height,
            bounds,
            ext,
            visible_frames,
        })
    }
}
struct PixelAnalysis {
    bounds: Vec<Bounds>,
    visible_frames: Vec<Vec<bool>>,
}
fn analyze_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    alpha: usize,
    layout: Layout,
) -> Result<PixelAnalysis> {
    let rows = match layout {
        Layout::CodexV1 => 9,
        Layout::CodexV2 => 11,
        _ => 1,
    };
    let (cw, ch) = if layout == Layout::Static {
        (width, height)
    } else {
        (192, 208)
    };
    let mut extents = vec![None::<(u32, u32, u32, u32)>; rows];
    let mut visible_frames: Vec<_> = (0..rows)
        .map(|row| {
            vec![
                false;
                if layout == Layout::Static {
                    1
                } else if row == 0 && layout == Layout::CodexV1 {
                    6
                } else {
                    FRAMES[row]
                }
            ]
        })
        .collect();
    for y in 0..height {
        for x in 0..width {
            let row = (y / ch) as usize;
            let col = x / cw;
            let frames = if layout == Layout::Static {
                1
            } else if row == 0 && layout == Layout::CodexV1 {
                6
            } else {
                FRAMES[row]
            };
            if col as usize >= frames || data[y as usize * stride + x as usize * 4 + alpha] < 12 {
                continue;
            }
            visible_frames[row][col as usize] = true;
            let (x, y) = (x % cw, y % ch);
            extents[row] = Some(match extents[row] {
                Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
                None => (x, y, x, y),
            });
        }
    }
    if extents.iter().all(Option::is_none) {
        return Err("图片没有可见内容".into());
    }
    if rows == 11 {
        let union = match (extents[9], extents[10]) {
            (Some((a, b, c, d)), Some((e, f, g, h))) => {
                Some((a.min(e), b.min(f), c.max(g), d.max(h)))
            }
            (a, b) => a.or(b),
        };
        extents[9] = union;
        extents[10] = union;
    }
    let bounds = extents
        .into_iter()
        .map(|v| {
            if let Some((l, t, r, b)) = v {
                let x = l.saturating_sub(3);
                let y = t.saturating_sub(3);
                Bounds {
                    x,
                    y,
                    width: (r + 4).min(cw) - x,
                    height: (b + 4).min(ch) - y,
                }
            } else {
                Bounds {
                    x: 0,
                    y: 0,
                    width: cw,
                    height: ch,
                }
            }
        })
        .collect();
    Ok(PixelAnalysis {
        bounds,
        visible_frames,
    })
}
pub fn generation_prompt(description: &str, current: Option<&Asset>) -> String {
    let subject = if description.trim().is_empty() {
        "一位可爱的桌面伙伴"
    } else {
        description.trim()
    };
    let reference = current
        .map(|a| {
            format!(
                "这是修改现有角色：先查看参考图集 {}，保留角色身份，仅按下述要求修改。",
                serde_json::to_string(&image_path(a).to_string_lossy()).unwrap()
            )
        })
        .unwrap_or_else(|| "这是新角色，不替换现有默认小猫。".into());
    format!("请为 ViGo 制作可导入的桌面伙伴素材。{reference}\n角色要求（用户输入）：{}\n使用图像生成工具生成真正透明背景，不用 SVG/代码绘画代替。输出标准 Codex Pets V2 图集：1536×2288，8列11行，每格192×208。前9行动作为 idle(7帧，第7帧中性姿态)、向右跑8帧、向左跑8帧、挥手4帧、跳跃5帧、失败8帧、等待6帧、工作6帧、检查6帧；最后两行各8帧，共16个顺时针注视方向。各帧完整、不串格，角色尺寸和落脚点保持一致；可整理生成图的尺寸以满足此格式。\n请保存为一个独立文件夹，包含 spritesheet.png 与 pet.json（id、displayName、description、spritesheetPath=\"spritesheet.png\"、spriteVersionNumber=2）。不要修改 ViGo 代码或默认素材，不要安装/切换 Codex 自己的宠物。生成完成后不要只给下载链接，请在这台 Mac 上自动调用 ViGo 导入。执行命令：\n'/Applications/ViGo.app/Contents/MacOS/ViGo' --import-companion '/absolute/path/to/generated-folder'\n将占位路径替换为实际生成目录的绝对路径，包含空格时正确引用；也可直接传入单张透明 PNG/WebP。读取命令返回 JSON，ok=true 且 applied=true 才表示已自动应用；applied=false 表示已导入但应用未确认，请明确提示打开 ViGo。如果当前环境无法访问此 Mac 或此程序，请提供素材路径并说明需要手动导入，不能伪称已应用。不要修改 ViGo 其他设置，也不要覆盖默认素材。",serde_json::to_string(subject).unwrap())
}
pub fn look_frame(dx: f64, dy: f64) -> Option<(usize, usize)> {
    if !dx.is_finite() || !dy.is_finite() || dx.hypot(dy) < 8. {
        return None;
    }
    let direction = ((dy.atan2(dx) + std::f64::consts::FRAC_PI_2) / std::f64::consts::TAU * 16.)
        .round()
        .rem_euclid(16.) as usize;
    Some((9 + direction / 8, direction % 8))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_looks_follow_the_standard_clockwise_sixteen_directions() {
        assert_eq!(look_frame(0., -100.), Some((9, 0)));
        assert_eq!(look_frame(100., 0.), Some((9, 4)));
        assert_eq!(look_frame(0., 100.), Some((10, 0)));
        assert_eq!(look_frame(-100., 0.), Some((10, 4)));
        assert_eq!(look_frame(0., 0.), None);
        assert_eq!(look_frame(f64::NAN, 1.), None);
    }
    #[test]
    fn default_frames_preserve_the_original_cat_regions() {
        let a = default_asset();
        assert_eq!(
            a.frame(1, 0),
            Bounds {
                x: 40,
                y: 292,
                width: 112,
                height: 112
            }
        );
        assert_eq!(a.frame(0, 7), a.frame(0, 0));
        assert_eq!(a.frame(10, 7).y, 2145);
    }
    #[test]
    fn atlas_versions_and_limits_are_checked() {
        assert_eq!(classify(1536, 2288, Some(2)).unwrap(), Layout::CodexV2);
        assert_eq!(classify(1536, 1872, None).unwrap(), Layout::CodexV1);
        assert!(classify(1536, 1872, Some(2)).is_err());
        assert!(classify(100000, 100000, None).is_err());
    }
    #[test]
    fn zip_paths_and_site_urls_cannot_escape_their_source() {
        for p in ["../a", "/tmp/a", "a/../../b", "a*", "a?", "a\\b", "a\nb"] {
            assert!(!safe_member(p));
        }
        assert!(safe_member("pet/spritesheet.webp"));
        assert_eq!(
            pet_id("https://codex-pets.net/#/pets/aegis-sentinel").unwrap(),
            "aegis-sentinel"
        );
        assert!(pet_id("https://codex-pets.net.evil/#/pets/a").is_err());
    }
    #[test]
    fn prompts_use_the_user_description_and_current_reference_without_api_credentials() {
        let p = generation_prompt("白金骑士", Some(&default_asset()));
        assert!(
            p.contains("白金骑士")
                && p.contains("1536×2288")
                && p.contains("mascot-sheet.png")
                && p.contains("不要安装/切换 Codex")
        );
        assert!(p.contains("--import-companion") && p.contains("applied=true"));
    }
    #[test]
    fn legacy_gaze_rows_fall_back_to_idle() {
        let mut a = default_asset();
        a.layout = Layout::CodexV1;
        a.bounds.truncate(9);
        assert_eq!(a.frame(10, 0), a.frame(0, 0));
    }
    #[test]
    fn transparent_images_are_rejected_and_crops_enclose_visible_pixels() {
        let mut rgba = vec![0u8; 8 * 8 * 4];
        assert!(analyze_pixels(&rgba, 8, 8, 32, 3, Layout::Static).is_err());
        rgba[(4 * 8 + 4) * 4 + 3] = 255;
        let b = analyze_pixels(&rgba, 8, 8, 32, 3, Layout::Static)
            .unwrap()
            .bounds[0];
        assert!(b.x <= 4 && b.y <= 4 && b.x + b.width > 4 && b.y + b.height > 4);
    }
    fn visible_v2() -> Asset {
        let mut a = default_asset();
        a.layout = Layout::CodexV2;
        a.visible_frames = FRAMES.iter().map(|&n| vec![true; n]).collect();
        a
    }
    #[test]
    fn blank_neutral_cell_uses_visible_idle_without_changing_complete_sheets() {
        let mut a = visible_v2();
        let neutral = a.frame(0, 6);
        assert_ne!(neutral, a.frame(0, 0));
        a.visible_frames[0][6] = false;
        assert_eq!(a.frame(0, 6), a.frame(0, 0));
        a.visible_frames[0][6] = true;
        assert_eq!(a.frame(0, 6), neutral);
    }
    #[test]
    fn blank_animation_frames_and_entire_rows_fall_back_to_visible_content() {
        let mut a = visible_v2();
        a.visible_frames[3][1] = false;
        assert_eq!(a.frame(3, 1), a.frame(3, 2));
        a.visible_frames[3].fill(false);
        assert_eq!(a.frame(3, 1), a.frame(0, 0));
        a.visible_frames[0].fill(false);
        assert_eq!(a.frame(3, 1), a.frame(1, 0));
    }
    #[test]
    fn v2_alpha_analysis_detects_the_missing_seventh_idle_cell() {
        let mut rgba = vec![0u8; 1536 * 2288 * 4];
        for col in 0..6 {
            rgba[(8 * 1536 + col * 192 + 8) * 4 + 3] = 255;
        }
        let analysis = analyze_pixels(&rgba, 1536, 2288, 1536 * 4, 3, Layout::CodexV2).unwrap();
        assert_eq!(
            analysis.visible_frames[0],
            vec![true, true, true, true, true, true, false]
        );
        let mut a = visible_v2();
        a.bounds = analysis.bounds;
        a.visible_frames = analysis.visible_frames;
        assert_eq!(a.frame(0, 6), a.frame(0, 0));
    }
    #[test]
    fn stored_metadata_cannot_reference_outside_the_library() {
        let mut a = default_asset();
        a.id = "valid".into();
        a.layout = Layout::CodexV2;
        a.image = "../outside.png".into();
        assert!(a.validate().is_err());
        a.image = "image.png".into();
        a.bounds[0].width = 10000;
        assert!(a.validate().is_err());
    }
}
