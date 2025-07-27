use crate::ResourceLocation;
use crate::config::{is_no_hurt_cam_enabled, is_no_fog_enabled, is_particles_disabler_enabled, is_java_clouds_enabled};
use libc::{off64_t, off_t};
use materialbin::{CompiledMaterialDefinition, MinecraftVersion};
use ndk::asset::Asset;
use ndk_sys::{AAsset, AAssetManager};
use once_cell::sync::Lazy;
use scroll::Pread;
use std::{
    borrow::Cow,
    collections::HashMap,
    ffi::{CStr, CString, OsStr},
    io::{self, Cursor, Read, Seek, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

// This makes me feel wrong... but all we will do is compare the pointer
// and the struct will be used in a mutex so i guess this is safe??
#[derive(PartialEq, Eq, Hash)]
struct AAssetPtr(*const ndk_sys::AAsset);
unsafe impl Send for AAssetPtr {}

// The minecraft version we will use to port shaders to
static MC_VERSION: OnceLock<Option<MinecraftVersion>> = OnceLock::new();

// The assets we have registrered to remplace data about
static WANTED_ASSETS: Lazy<Mutex<HashMap<AAssetPtr, Cursor<Vec<u8>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Embedded no-fog material bin files
const LEGACY_CUBEMAP_MATERIAL_BIN: &[u8] = include_bytes!("no_fog_materials/LegacyCubemap.material.bin");
const ACTOR_MATERIAL_BIN: &[u8] = include_bytes!("no_fog_materials/Actor.material.bin");
const ACTOR_BANNER_MATERIAL_BIN: &[u8] = include_bytes!("no_fog_materials/ActorBanner.material.bin");
const ACTOR_GLINT_MATERIAL_BIN: &[u8] = include_bytes!("no_fog_materials/ActorGlint.material.bin");
const ITEM_IN_HAND_COLOR_MATERIAL_BIN: &[u8] = include_bytes!("no_fog_materials/ItemInHandColor.material.bin");
const ITEM_IN_HAND_COLOR_GLINT_MATERIAL_BIN: &[u8] = include_bytes!("no_fog_materials/ItemInHandColorGlint.material.bin");
const ITEM_IN_HAND_TEXTURED_MATERIAL_BIN: &[u8] = include_bytes!("no_fog_materials/ItemInHandTextured.material.bin");
const PARTICLE_MATERIAL_BIN: &[u8] = include_bytes!("no_fog_materials/Particle.material.bin");
const RENDER_CHUNK_MATERIAL_BIN: &[u8] = include_bytes!("no_fog_materials/RenderChunk.material.bin");

// Custom splash text JSON content
const CUSTOM_SPLASHES_JSON: &str = r#"{"splashes":["Origin Client","Origin > any other client","The Best Client!!","BlueCat","Origin is so much better","Origin Optimizes like no other client","Make Sure to star our repository:https://github.com/Origin-Client/Origin","Contributions open!","Made by the community, for the community","Yami is goated!!"]}"#;

const CUSTOM_FIRST_PERSON_JSON: &str = r#"{"format_version":"1.18.10","minecraft:camera_entity":{"description":{"identifier":"minecraft:first_person"},"components":{"minecraft:camera":{"field_of_view":66,"near_clipping_plane":0.025,"far_clipping_plane":2500},"minecraft:camera_first_person":{},"minecraft:camera_render_first_person_objects":{},"minecraft:camera_attach_to_player":{},"minecraft:camera_offset":{"view":[0,0],"entity":[0,0,0]},"minecraft:camera_direct_look":{"pitch_min":-89.9,"pitch_max":89.9},"minecraft:camera_perspective_option":{"view_mode":"first_person"},"minecraft:update_player_from_camera":{"look_mode":"along_camera"},"minecraft:extend_player_rendering":{},"minecraft:camera_player_sleep_vignette":{},"minecraft:vr_comfort_move":{},"minecraft:default_input_camera":{},"minecraft:gameplay_affects_fov":{},"minecraft:allow_inside_block":{}}}}"#;
const CUSTOM_THIRD_PERSON_JSON: &str = r#"{"format_version":"1.18.10","minecraft:camera_entity":{"description":{"identifier":"minecraft:third_person"},"components":{"minecraft:camera":{"field_of_view":66,"near_clipping_plane":0.025,"far_clipping_plane":2500},"minecraft:camera_third_person":{},"minecraft:camera_render_player_model":{},"minecraft:camera_attach_to_player":{},"minecraft:camera_offset":{"view":[0,0],"entity":[0,2,5]},"minecraft:camera_look_at_player":{},"minecraft:camera_orbit":{"azimuth_smoothing_spring":0,"polar_angle_smoothing_spring":0,"distance_smoothing_spring":0,"polar_angle_min":0.1,"polar_angle_max":179.9,"radius":4},"minecraft:camera_avoidance":{"relax_distance_smoothing_spring":0,"distance_constraint_min":0.25},"minecraft:camera_perspective_option":{"view_mode":"third_person"},"minecraft:update_player_from_camera":{"look_mode":"along_camera"},"minecraft:camera_player_sleep_vignette":{},"minecraft:gameplay_affects_fov":{},"minecraft:allow_inside_block":{},"minecraft:extend_player_rendering":{}}}}"#;
const CUSTOM_THIRD_PERSON_FRONT_JSON: &str = r#"{"format_version":"1.18.10","minecraft:camera_entity":{"description":{"identifier":"minecraft:third_person_front"},"components":{"minecraft:camera":{"field_of_view":66,"near_clipping_plane":0.025,"far_clipping_plane":2500},"minecraft:camera_third_person":{},"minecraft:camera_render_player_model":{},"minecraft:camera_attach_to_player":{},"minecraft:camera_offset":{"view":[0,0],"entity":[0,2,5]},"minecraft:camera_look_at_player":{},"minecraft:camera_orbit":{"azimuth_smoothing_spring":0,"polar_angle_smoothing_spring":0,"distance_smoothing_spring":0,"polar_angle_min":0.1,"polar_angle_max":179.9,"radius":4,"invert_x_input":true},"minecraft:camera_avoidance":{"relax_distance_smoothing_spring":0,"distance_constraint_min":0.25},"minecraft:camera_perspective_option":{"view_mode":"third_person_front"},"minecraft:update_player_from_camera":{"look_mode":"at_camera"},"minecraft:camera_player_sleep_vignette":{},"minecraft:gameplay_affects_fov":{},"minecraft:allow_inside_block":{},"minecraft:extend_player_rendering":{}}}}"#;

const CUSTOM_LOADING_MESSAGES_JSON: &str = r#"{"beginner_loading_messages":["Origin Client","Origin > any other client","The Best Client!!","BlueCat","Origin is so much better","Origin Optimizes like no other client","Make Sure to star our repository:https://github.com/Origin-Client/Origin","Contributions open!","Made by the community, for the community","Yami is goated!!"],"mid_game_loading_messages":["Origin Client","Origin > any other client","The Best Client!!","BlueCat","Origin is so much better","Origin Optimizes like no other client","Make Sure to star our repository:https://github.com/Origin-Client/Origin","Contributions open!","Made by the community, for the community","Yami is goated!!"],"late_game_loading_messages":["Origin Client","Origin > any other client","The Best Client!!","BlueCat","Origin is so much better","Origin Optimizes like no other client","Make Sure to star our repository:https://github.com/Origin-Client/Origin","Contributions open!","Made by the community, for the community","Yami is goated!!"],"creative_loading_messages":["Origin Client","Origin > any other client","The Best Client!!","BlueCat","Origin is so much better","Origin Optimizes like no other client","Make Sure to star our repository:https://github.com/Origin-Client/Origin","Contributions open!","Made by the community, for the community","Yami is goated!!"],"editor_loading_messages":["Origin Client","Origin > any other client","The Best Client!!","BlueCat","Origin is so much better","Origin Optimizes like no other client","Make Sure to star our repository:https://github.com/Origin-Client/Origin","Contributions open!","Made by the community, for the community","Yami is goated!!"],"realms_loading_messages":["Origin Client","Origin > any other client","The Best Client!!","BlueCat","Origin is so much better","Origin Optimizes like no other client","Make Sure to star our repository:https://github.com/Origin-Client/Origin","Contributions open!","Made by the community, for the community","Yami is goated!!"],"addons_loading_messages":["Origin Client","Origin > any other client","The Best Client!!","BlueCat","Origin is so much better","Origin Optimizes like no other client","Make Sure to star our repository:https://github.com/Origin-Client/Origin","Contributions open!","Made by the community, for the community","Yami is goated!!"],"store_progress_tooltips":["Origin Client","Origin > any other client","The Best Client!!","BlueCat","Origin is so much better","Origin Optimizes like no other client","Make Sure to star our repository:https://github.com/Origin-Client/Origin","Contributions open!","Made by the community, for the community","Yami is goated!!"]}"#;

// Java clouds texture
const JAVA_CLOUDS_TEXTURE: &[u8] = include_bytes!("Diskksks.png");

// Im very sorry but its just that AssetManager is so shitty to work with
// i cant handle how randomly it breaks
fn get_current_mcver(man: ndk::asset::AssetManager) -> Option<MinecraftVersion> {
    let mut file = match get_uitext(man) {
        Some(asset) => asset,
        None => {
            log::error!("Shader fixing is disabled as no mc version was found");
            return None;
        }
    };
    let mut buf = Vec::with_capacity(file.length());
    if let Err(e) = file.read_to_end(&mut buf) {
        log::error!("Something is wrong with AssetManager, mc detection failed: {e}");
        return None;
    };
    for version in materialbin::ALL_VERSIONS {
        if buf
            .pread_with::<CompiledMaterialDefinition>(0, version)
            .is_ok()
        {
            log::info!("Mc version is {version}");
            return Some(version);
        };
    }
    None
}

// Try to open UIText.material.bin to guess mc shader version
fn get_uitext(man: ndk::asset::AssetManager) -> Option<Asset> {
    const NEW: &CStr = c"assets/renderer/materials/UIText.material.bin";
    const OLD: &CStr = c"renderer/materials/UIText.material.bin";
    for path in [NEW, OLD] {
        if let Some(asset) = man.open(path) {
            return Some(asset);
        }
    }
    None
}

macro_rules! folder_list {
    ($( apk: $apk_folder:literal -> pack: $pack_folder:expr),
        *,
    ) => {
        [
            $(($apk_folder, $pack_folder)),*,
        ]
    }
}

fn get_no_fog_material_data(filename: &str) -> Option<&'static [u8]> {
    if !is_no_fog_enabled() {
        return None;
    }
    
    match filename {
        "LegacyCubemap.material.bin" => Some(LEGACY_CUBEMAP_MATERIAL_BIN),
        "Actor.material.bin" => Some(ACTOR_MATERIAL_BIN),
        "ActorBanner.material.bin" => Some(ACTOR_BANNER_MATERIAL_BIN),
        "ActorGlint.material.bin" => Some(ACTOR_GLINT_MATERIAL_BIN),
        "ItemInHandColor.material.bin" => Some(ITEM_IN_HAND_COLOR_MATERIAL_BIN),
        "ItemInHandColorGlint.material.bin" => Some(ITEM_IN_HAND_COLOR_GLINT_MATERIAL_BIN),
        "ItemInHandTextured.material.bin" => Some(ITEM_IN_HAND_TEXTURED_MATERIAL_BIN),
        "Particle.material.bin" => Some(PARTICLE_MATERIAL_BIN),
        "RenderChunk.material.bin" => Some(RENDER_CHUNK_MATERIAL_BIN),
        _ => None,
    }
}

fn is_particle_json_file(c_path: &Path) -> bool {
    if !is_particles_disabler_enabled() {
        return false;
    }
    
    let path_str = c_path.to_string_lossy();
    
    // Check if the path contains particles folder and ends with .json
    path_str.contains("particles/") && path_str.ends_with(".json")
}

fn process_particle_json(original_data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Convert bytes to string
    let json_str = std::str::from_utf8(original_data)?;
    
    // Parse JSON
    let mut json_value: serde_json::Value = serde_json::from_str(json_str)?;
    
    // Function to recursively process JSON and set particle counts to 0
    fn process_json_recursive(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                // Set num_particles and max_particles to 0 if they exist
                if let Some(num_particles) = map.get_mut("num_particles") {
                    *num_particles = serde_json::Value::Number(serde_json::Number::from(0));
                    log::info!("Set num_particles to 0");
                }
                if let Some(max_particles) = map.get_mut("max_particles") {
                    *max_particles = serde_json::Value::Number(serde_json::Number::from(0));
                    log::info!("Set max_particles to 0");
                }
                
                // Recursively process all values in the object
                for (_, v) in map.iter_mut() {
                    process_json_recursive(v);
                }
            }
            serde_json::Value::Array(arr) => {
                // Recursively process all values in the array
                for item in arr.iter_mut() {
                    process_json_recursive(item);
                }
            }
            _ => {} // Do nothing for primitive values
        }
    }
    
    // Process the JSON recursively
    process_json_recursive(&mut json_value);
    
    // Convert back to string and then to bytes
    let modified_json = serde_json::to_string(&json_value)?;
    Ok(modified_json.into_bytes())
}

fn is_clouds_texture_file(c_path: &Path) -> bool {
    if !is_java_clouds_enabled() {
        return false;
    }
    
    let path_str = c_path.to_string_lossy();
    
    // Check if this is the clouds.png texture file
    path_str.contains("textures/environment/clouds.png") || 
    path_str.ends_with("textures/environment/clouds.png")
}

pub(crate) unsafe fn rem(aasset: *mut AAsset) -> off_t {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getRemainingLength(aasset),
    };
    (file.get_ref().len() - file.position() as usize) as off_t
}

pub(crate) unsafe fn rem64(aasset: *mut AAsset) -> off64_t {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getRemainingLength64(aasset),
    };
    (file.get_ref().len() - file.position() as usize) as off64_t
}

pub(crate) unsafe fn close(aasset: *mut AAsset) {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    if wanted_assets.remove(&AAssetPtr(aasset)).is_none() {
        ndk_sys::AAsset_close(aasset);
    }
}

pub(crate) unsafe fn get_buffer(aasset: *mut AAsset) -> *const libc::c_void {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getBuffer(aasset),
    };
    // Lets hope this does not go boom boom
    file.get_mut().as_mut_ptr().cast()
}

pub(crate) unsafe fn fd_dummy(
    aasset: *mut AAsset,
    out_start: *mut off_t,
    out_len: *mut off_t,
) -> libc::c_int {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(_) => {
            log::error!("WE GOT BUSTED NOOO");
            -1
        }
        None => ndk_sys::AAsset_openFileDescriptor(aasset, out_start, out_len),
    }
}

pub(crate) unsafe fn fd_dummy64(
    aasset: *mut AAsset,
    out_start: *mut off64_t,
    out_len: *mut off64_t,
) -> libc::c_int {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(_) => {
            log::error!("WE GOT BUSTED NOOO");
            -1
        }
        None => ndk_sys::AAsset_openFileDescriptor64(aasset, out_start, out_len),
    }
}

pub(crate) unsafe fn is_alloc(aasset: *mut AAsset) -> libc::c_int {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(_) => false as libc::c_int,
        None => ndk_sys::AAsset_isAllocated(aasset),
    }
}

fn seek_facade(offset: i64, whence: libc::c_int, file: &mut Cursor<Vec<u8>>) -> i64 {
    let offset = match whence {
        libc::SEEK_SET => {
            //Lets check this so we dont mess up
            let u64_off = match u64::try_from(offset) {
                Ok(uoff) => uoff,
                Err(e) => {
                    log::error!("signed ({offset}) to unsigned failed: {e}");
                    return -1;
                }
            };
            io::SeekFrom::Start(u64_off)
        }
        libc::SEEK_CUR => io::SeekFrom::Current(offset),
        libc::SEEK_END => io::SeekFrom::End(offset),
        _ => {
            log::error!("Invalid seek whence");
            return -1;
        }
    };
    match file.seek(offset) {
        Ok(new_offset) => match new_offset.try_into() {
            Ok(int) => int,
            Err(err) => {
                log::error!("u64 ({new_offset}) to i64 failed: {err}");
                -1
            }
        },
        Err(err) => {
            log::error!("aasset seek failed: {err}");
            -1
        }
    }
}) unsafe fn open(
    man: *mut AAssetManager,
    fname: *const libc::c_char,
    mode: libc::c_int,
) -> *mut ndk_sys::AAsset {
    // This is where ub can happen, but we are merely a hook.
    let aasset = unsafe { ndk_sys::AAssetManager_open(man, fname, mode) };
    let c_str = unsafe { CStr::from_ptr(fname) };
    let raw_cstr = c_str.to_bytes();
    let os_str = OsStr::from_bytes(raw_cstr);
    let c_path: &Path = Path::new(os_str);
    
    // Extract filename
    let Some(os_filename) = c_path.file_name() else {
        log::warn!("Path had no filename: {c_path:?}");
        return aasset;
    };

    // Check if this is splashes.json and replace it with custom content
    if os_filename == "splashes.json" {
        log::info!("Intercepting splashes.json with custom content");
        let buffer = CUSTOM_SPLASHES_JSON.as_bytes().to_vec();
        let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
        wanted_lock.insert(AAssetPtr(aasset), Cursor::new(buffer));
        return aasset;
    }
    
    // Check if this is loading_messages.json and replace it with custom content
    if os_filename == "loading_messages.json" {
        log::info!("Intercepting loading_messages.json with custom content");
        let buffer = CUSTOM_LOADING_MESSAGES_JSON.as_bytes().to_vec();
        let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
        wanted_lock.insert(AAssetPtr(aasset), Cursor::new(buffer));
        return aasset;
    }

    // Check if this is a particle JSON file and particles disabler is enabled
    if is_particle_json_file(c_path) {
        log::info!("Intercepting particle JSON file: {}", c_path.display());
        
        // Read the original file data first
        if aasset.is_null() {
            log::warn!("Failed to open original particle file: {}", c_path.display());
            return aasset;
        }
        
        // Get the original file size and read its content
        let original_size = ndk_sys::AAsset_getLength(aasset) as usize;
        let mut original_data = vec![0u8; original_size];
        
        let bytes_read = ndk_sys::AAsset_read(aasset, original_data.as_mut_ptr() as *mut libc::c_void, original_size);
        if bytes_read < 0 {
            log::warn!("Failed to read original particle file: {}", c_path.display());
            return aasset;
        }
        
        // Reset the asset position
        ndk_sys::AAsset_seek(aasset, 0, libc::SEEK_SET);
        
        // Process the particle JSON
        let processed_data = match process_particle_json(&original_data) {
            Ok(data) => {
                log::info!("Successfully processed particle JSON: {}", c_path.display());
                data
            }
            Err(e) => {
                log::warn!("Failed to process particle JSON {}: {}, using empty JSON", c_path.display(), e);
                "{}".as_bytes().to_vec()
            }
        };
        
        let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
        wanted_lock.insert(AAssetPtr(aasset), Cursor::new(processed_data));
        return aasset;
    }

    // Check if this is the clouds texture and java clouds is enabled
    if is_clouds_texture_file(c_path) {
        log::info!("Intercepting clouds texture with Java clouds texture: {}", c_path.display());
        let buffer = JAVA_CLOUDS_TEXTURE.to_vec();
        let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
        wanted_lock.insert(AAssetPtr(aasset), Cursor::new(buffer));
        return aasset;
    }

    // Check if this is a camera JSON file in the cameras folder and nohurtcam is enabled
    if is_no_hurt_cam_enabled() {
        // Check if the path contains cameras folder and ends with the specific JSON files
        let path_str = c_path.to_string_lossy();
        
        if path_str.contains("cameras/") {
            if os_filename == "first_person.json" {
                log::info!("Intercepting cameras/first_person.json with custom content (nohurtcam enabled)");
                let buffer = CUSTOM_FIRST_PERSON_JSON.as_bytes().to_vec();
                let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
                wanted_lock.insert(AAssetPtr(aasset), Cursor::new(buffer));
                return aasset;
            }
            
            if os_filename == "third_person.json" {
                log::info!("Intercepting cameras/third_person.json with custom content (nohurtcam enabled)");
                let buffer = CUSTOM_THIRD_PERSON_JSON.as_bytes().to_vec();
                let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
                wanted_lock.insert(AAssetPtr(aasset), Cursor::new(buffer));
                return aasset;
            }
            
            if os_filename == "third_person_front.json" {
                log::info!("Intercepting cameras/third_person_front.json with custom content (nohurtcam enabled)");
                let buffer = CUSTOM_THIRD_PERSON_FRONT_JSON.as_bytes().to_vec();
                let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
                wanted_lock.insert(AAssetPtr(aasset), Cursor::new(buffer));
                return aasset;
            }
        }
    }

    // Check if this is a material.bin file that we want to replace for no-fog
    let filename_str = os_filename.to_string_lossy();
    if let Some(no_fog_data) = get_no_fog_material_data(&filename_str) {
        log::info!("Intercepting {} with no-fog material (no-fog enabled)", filename_str);
        let buffer = no_fog_data.to_vec();
        let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
        wanted_lock.insert(AAssetPtr(aasset), Cursor::new(buffer));
        return aasset;
    }

    // This is meant to strip the new "asset" folder path so we can be compatible with other versions
    let stripped = match c_path.strip_prefix("assets/") {
        Ok(yay) => yay,
        Err(_e) => c_path,
    };
    
    // Folder paths to replace and with what
    let replacement_list = folder_list! {
        apk: "gui/dist/hbui/" -> pack: "hbui/",
        apk: "skin_packs/persona/" -> pack: "persona/",
        apk: "renderer/" -> pack: "renderer/",
        apk: "resource_packs/vanilla/cameras/" -> pack: "vanilla_cameras/",
    };
    
    for replacement in replacement_list {
        // Remove the prefix we want to change
        if let Ok(file) = stripped.strip_prefix(replacement.0) {
            cxx::let_cxx_string!(cxx_out = "");
            let loadfn = match crate::RPM_LOAD.get() {
                Some(ptr) => ptr,
                None => {
                    log::warn!("ResourcePackManager fn is not ready yet?");
                    return aasset;
                }
            };
            let mut arraybuf = [0; 128];
            let file_path = opt_path_join(&mut arraybuf, &[Path::new(replacement.1), file]);
            let packm_ptr = crate::PACKM_OBJ.load(std::sync::atomic::Ordering::Acquire);
            let resource_loc = ResourceLocation::from_str(file_path.as_ref());
            log::info!("loading rpck file: {:#?}", &file_path);
            if packm_ptr.is_null() {
                log::error!("ResourcePackManager ptr is null");
                return aasset;
            }
            loadfn(packm_ptr, resource_loc, cxx_out.as_mut());
            // Free resource location
            if cxx_out.is_empty() {
                log::info!("File was not found");
                return aasset;
            }
            let buffer = if os_filename.as_encoded_bytes().ends_with(b".material.bin") {
                match process_material(man, cxx_out.as_bytes()) {
                    Some(updated) => updated,
                    None => cxx_out.as_bytes().to_vec(),
                }
            } else {
                cxx_out.as_bytes().to_vec()
            };
            let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
            wanted_lock.insert(AAssetPtr(aasset), Cursor::new(buffer));
            // we do not clean cxx string because cxx crate does that for us
            return aasset;
        }
    }
    return aasset;
}

/// Join paths without allocating if possible, or
/// if the joined path does not fit the buffer then just
/// allocate instead
fn opt_path_join<'a>(bytes: &'a mut [u8; 128], paths: &[&Path]) -> Cow<'a, CStr> {
    let total_len: usize = paths.iter().map(|p| p.as_os_str().len()).sum();
    if total_len + 1 > 128 {
        // panic!("fuck");
        let mut pathbuf = PathBuf::new();
        for path in paths {
            pathbuf.push(path);
        }
        let cpath = CString::new(pathbuf.into_os_string().as_encoded_bytes()).unwrap();
        return Cow::Owned(cpath);
    }

    let mut writer = bytes.as_mut_slice();
    for path in paths {
        let osstr = path.as_os_str().as_bytes();
        let _ = writer.write(osstr);
    }
    let _ = writer.write(&[0]);
    let guh = CStr::from_bytes_until_nul(bytes).unwrap();
    Cow::Borrowed(guh)
}

fn process_material(man: *mut AAssetManager, data: &[u8]) -> Option<Vec<u8>> {
    let mcver = MC_VERSION.get_or_init(|| {
        let pointer = match std::ptr::NonNull::new(man) {
            Some(yay) => yay,
            None => {
                log::warn!("AssetManager is null?, preposterous, mc detection failed");
                return None;
            }
        };
        let manager = unsafe { ndk::asset::AssetManager::from_ptr(pointer) };
        get_current_mcver(manager)
    });
    // just ignore if no mc version was found
    let mcver = (*mcver)?;
    for version in materialbin::ALL_VERSIONS {
        let material: CompiledMaterialDefinition = match data.pread_with(0, version) {
            Ok(data) => data,
            Err(e) => {
                log::trace!("[version] Parsing failed: {e}");
                continue;
            }
        };
        // Prevent some work
        if version == mcver {
            return None;
        }
        let mut output = Vec::with_capacity(data.len());
        if let Err(e) = material.write(&mut output, mcver) {
            log::trace!("[version] Write error: {e}");
            return None;
        }
        return Some(output);
    }

    None
}

pub(crate) unsafe fn seek64(aasset: *mut AAsset, off: off64_t, whence: libc::c_int) -> off64_t {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_seek64(aasset, off, whence),
    };
    seek_facade(off, whence, file) as off64_t
}

pub(crate) unsafe fn seek(aasset: *mut AAsset, off: off_t, whence: libc::c_int) -> off_t {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_seek(aasset, off, whence),
    };
    // This code can be very deadly on large files,
    // but since NO replacement should surpass u32 max we should be fine...
    // i dont even think a mcpack can exceed that
    seek_facade(off.into(), whence, file) as off_t
}

pub(crate) unsafe fn read(
    aasset: *mut AAsset,
    buf: *mut libc::c_void,
    count: libc::size_t,
) -> libc::c_int {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_read(aasset, buf, count),
    };
    // Reuse buffer given by caller
    let rs_buffer = core::slice::from_raw_parts_mut(buf as *mut u8, count);
    let read_total = match file.read(rs_buffer) {
        Ok(n) => n,
        Err(e) => {
            log::warn!("failed fake aaset read: {e}");
            return -1 as libc::c_int;
        }
    };
    read_total as libc::c_int
}

pub(crate) unsafe fn len(aasset: *mut AAsset) -> off_t {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getLength(aasset),
    };
    file.get_ref().len() as off_t
}

pub(crate) unsafe fn len64(aasset: *mut AAsset) -> off64_t {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getLength64(aasset),
    };
    file.get_ref().len() as off64_t
}