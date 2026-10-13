use tauri::{Manager, State, Emitter};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use windows::core::Interface;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::Media::Audio::{
    eRender, DEVICE_STATE_ACTIVE, IAudioSessionManager2, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

struct MediaState {
    title: String,
    artist: String,
    status: String,
    thumbnail: Option<Vec<u8>>,
    position: i64,
    duration: i64,
    volume: i32,
    muted: bool,
    peak: f32,
    history: Vec<serde_json::Value>,
}
struct AppState(Arc<Mutex<MediaState>>);

fn trigger_media_key(action: &str) {
    #[cfg(windows)]
    {
        unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
        if let Ok(manager_op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            if let Ok(manager) = manager_op.get() {
                if let Ok(session) = manager.GetCurrentSession() {
                    match action {
                        "playpause" => { let _ = session.TryTogglePlayPauseAsync(); }
                        "next" => { let _ = session.TrySkipNextAsync(); }
                        "prev" => { let _ = session.TrySkipPreviousAsync(); }
                        _ => {}
                    }
                    return;
                }
            }
        }
        unsafe {
            let vkey = match action {
                "playpause" => 0xB3,
                "next" => 0xB0,
                "prev" => 0xB1,
                _ => 0,
            };
            if vkey != 0 {
                keybd_event(vkey, 0, KEYEVENTF_EXTENDEDKEY, 0);
                keybd_event(vkey, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP, 0);
            }
        }
    }
}

#[cfg(windows)]
fn apply_audio_and_get_peak(target_vol: Option<i32>, target_mute: Option<bool>) -> f32 {
    let mut max_peak = 0.0f32;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            if let Ok(collection) = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
                let count = collection.GetCount().unwrap_or(0);
                for i in 0..count {
                    if let Ok(device) = collection.Item(i) {
                        if let Ok(meter) = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) {
                            if let Ok(p) = meter.GetPeakValue() {
                                if p > max_peak {
                                    max_peak = p;
                                }
                            }
                        }

                        if let Ok(session_mgr) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                            if let Ok(session_enum) = session_mgr.GetSessionEnumerator() {
                                let s_count = session_enum.GetCount().unwrap_or(0);
                                for s in 0..s_count {
                                    if let Ok(ctrl) = session_enum.GetSession(s) {
                                        if let Ok(s_meter) = ctrl.cast::<IAudioMeterInformation>() {
                                            if let Ok(sp) = s_meter.GetPeakValue() {
                                                if sp > max_peak {
                                                    max_peak = sp;
                                                }
                                            }
                                        }

                                        if target_vol.is_some() || target_mute.is_some() {
                                            if let Ok(simple_vol) = ctrl.cast::<ISimpleAudioVolume>() {
                                                if let Some(v) = target_vol {
                                                    let scalar = (v as f32 / 100.0).clamp(0.0, 1.0);
                                                    let _ = simple_vol.SetMasterVolume(scalar, std::ptr::null());
                                                }
                                                if let Some(m) = target_mute {
                                                    let _ = simple_vol.SetMute(m, std::ptr::null());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    max_peak
}

// NEU: Ultraschneller Endpunkt nur für den Visualizer (überträgt keinen unnötigen Ballast)
#[tauri::command]
fn get_peak(state: State<'_, AppState>) -> f32 {
    let lock = state.0.lock().unwrap();
    if lock.muted || lock.status != "Playing" {
        0.0
    } else {
        lock.peak
    }
}

#[tauri::command]
fn get_media_info(state: State<'_, AppState>) -> serde_json::Value {
    let lock = state.0.lock().unwrap();
    serde_json::json!({
        "title": lock.title,
        "artist": lock.artist,
        "status": lock.status,
        "thumbnail": lock.thumbnail,
        "position": lock.position,
        "duration": lock.duration,
        "volume": lock.volume,
        "muted": lock.muted,
        "peak": lock.peak,
        "history": lock.history
    })
}

#[tauri::command]
fn media_control(action: String, state: State<'_, AppState>) -> serde_json::Value {
    match action.as_str() {
        "vol_up" | "vol_down" | "mute" => {
            let (new_vol, new_mute) = {
                let mut lock = state.0.lock().unwrap();
                match action.as_str() {
                    "vol_up" => {
                        lock.volume = (lock.volume + 5).clamp(0, 100);
                        lock.muted = false;
                    }
                    "vol_down" => {
                        lock.volume = (lock.volume - 5).clamp(0, 100);
                    }
                    "mute" => {
                        lock.muted = !lock.muted;
                    }
                    _ => {}
                }
                (lock.volume, lock.muted)
            };

            #[cfg(windows)]
            {
                let p = apply_audio_and_get_peak(Some(new_vol), Some(new_mute));
                if let Ok(mut lock) = state.0.lock() {
                    lock.peak = p;
                }
            }

            return serde_json::json!({ "volume": new_vol, "muted": new_mute });
        }
        _ => {
            trigger_media_key(action.as_str());
        }
    }
    serde_json::json!({})
}

#[tauri::command]
fn seek_media(seconds: i64) {
    #[cfg(windows)]
    {
        unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
        if let Ok(manager_op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            if let Ok(manager) = manager_op.get() {
                if let Ok(session) = manager.GetCurrentSession() {
                    let ticks = seconds * 10_000_000;
                    let _ = session.TryChangePlaybackPositionAsync(ticks);
                }
            }
        }
    }
}

#[tauri::command]
fn resize_window(window: tauri::Window, width: f64, height: f64) {
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
}

#[tauri::command]
fn start_drag(window: tauri::Window) {
    let _ = window.start_dragging();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let media_state = Arc::new(Mutex::new(MediaState {
        title: "Keine Wiedergabe".to_string(),
        artist: "Warte auf YouTube Music...".to_string(),
        status: "Paused".to_string(),
        thumbnail: None,
        position: 0,
        duration: 0,
        volume: 100,
        muted: false,
        peak: 0.0,
        history: Vec::new(),
    }));

    let state_clone = media_state.clone();

    thread::spawn(move || {
        #[cfg(windows)]
        unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
        
        let mut tick_count: u32 = 0;

        loop {
            // 1. High-Speed Audio-Sampling (alle 40ms = 25 FPS für knackige Beats!)
            #[cfg(windows)]
            {
                let p = apply_audio_and_get_peak(None, None);
                if let Ok(mut lock) = state_clone.lock() {
                    lock.peak = p;
                }
            }

            // 2. Metadaten-Tick (nur alle 20 Ticks = ca. 800ms)
            if tick_count % 20 == 0 {
                let mut new_title = "Keine Wiedergabe".to_string();
                let mut new_artist = "Warte auf YouTube Music...".to_string();
                let mut new_status = "Paused".to_string();
                let mut new_thumb: Option<Vec<u8>> = None;
                let mut new_pos: i64 = 0;
                let mut new_dur: i64 = 0;

                #[cfg(windows)]
                {
                    if let Ok(manager_op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
                        if let Ok(manager) = manager_op.get() {
                            if let Ok(session) = manager.GetCurrentSession() {
                                if let Ok(info) = session.GetPlaybackInfo() {
                                    if let Ok(stat) = info.PlaybackStatus() {
                                        if stat == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
                                            new_status = "Playing".to_string();
                                        }
                                    }
                                }

                                if let Ok(timeline) = session.GetTimelineProperties() {
                                    if let Ok(pos) = timeline.Position() {
                                        new_pos = pos.Duration / 10_000_000;
                                    }
                                    if let Ok(end) = timeline.EndTime() {
                                        new_dur = end.Duration / 10_000_000;
                                    }
                                }

                                if let Ok(props_op) = session.TryGetMediaPropertiesAsync() {
                                    if let Ok(props) = props_op.get() {
                                        if let Ok(t) = props.Title() { if !t.is_empty() { new_title = t.to_string(); } }
                                        if let Ok(a) = props.Artist() { if !a.is_empty() { new_artist = a.to_string(); } }
                                        if let Ok(thumb_ref) = props.Thumbnail() {
                                            if let Ok(stream_op) = thumb_ref.OpenReadAsync() {
                                                if let Ok(stream) = stream_op.get() {
                                                    if let Ok(size) = stream.Size() {
                                                        if let Ok(reader) = DataReader::CreateDataReader(&stream) {
                                                            if let Ok(load_op) = reader.LoadAsync(size as u32) {
                                                                if load_op.get().is_ok() {
                                                                    let mut buffer = vec![0u8; size as usize];
                                                                    if reader.ReadBytes(&mut buffer).is_ok() {
                                                                        new_thumb = Some(buffer);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                let mut lock = state_clone.lock().unwrap();
                if lock.title != new_title 
                    && lock.title != "Keine Wiedergabe" 
                    && !lock.title.is_empty() 
                    && new_title != "Keine Wiedergabe" 
                {
                    let prev_song = serde_json::json!({
                        "title": lock.title,
                        "artist": lock.artist
                    });
                    if lock.history.first() != Some(&prev_song) {
                        lock.history.insert(0, prev_song);
                        lock.history.truncate(3);
                    }
                }

                lock.title = new_title;
                lock.artist = new_artist;
                lock.status = new_status;
                lock.thumbnail = new_thumb;
                lock.position = new_pos;
                lock.duration = new_dur;
            }

            tick_count = tick_count.wrapping_add(1);
            thread::sleep(Duration::from_millis(40));
        }
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Moved(pos) = event {
                let _ = window.emit("window-moved", (pos.x, pos.y));
            }
        })
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_shadow(false);
                let _ = window.set_size(tauri::LogicalSize::new(360.0, 90.0));
            }

            if let Ok(sc_master) = "Ctrl+Alt+M".parse::<Shortcut>() {
                let _ = app.global_shortcut().on_shortcut(sc_master, move |app, _sc, event| {
                    if event.state == ShortcutState::Pressed {
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.emit("fade-out", ());
                                let w = window.clone();
                                thread::spawn(move || {
                                    thread::sleep(Duration::from_millis(300));
                                    let _ = w.hide();
                                });
                            } else {
                                let _ = window.emit("fade-in", ());
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                });
            }

            if let Ok(sc_next) = "Ctrl+Alt+N".parse::<Shortcut>() {
                let _ = app.global_shortcut().on_shortcut(sc_next, move |app, _sc, event| {
                    if event.state == ShortcutState::Pressed {
                        trigger_media_key("next");
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("hotkey-pulse", "next");
                        }
                    }
                });
            }

            if let Ok(sc_prev) = "Ctrl+Alt+B".parse::<Shortcut>() {
                let _ = app.global_shortcut().on_shortcut(sc_prev, move |app, _sc, event| {
                    if event.state == ShortcutState::Pressed {
                        trigger_media_key("prev");
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("hotkey-pulse", "prev");
                        }
                    }
                });
            }

            if let Ok(sc_play) = "Ctrl+Alt+P".parse::<Shortcut>() {
                let _ = app.global_shortcut().on_shortcut(sc_play, move |app, _sc, event| {
                    if event.state == ShortcutState::Pressed {
                        trigger_media_key("playpause");
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("hotkey-pulse", "playpause");
                        }
                    }
                });
            }

            Ok(())
        })
        .manage(AppState(media_state))
        .invoke_handler(tauri::generate_handler![
            get_media_info,
            get_peak,
            media_control, 
            seek_media, 
            resize_window, 
            start_drag
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}