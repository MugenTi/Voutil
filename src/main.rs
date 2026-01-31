#![windows_subsystem = "windows"]
//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use slint::{Image, SharedPixelBuffer, ComponentHandle, Weak, PhysicalPosition, PhysicalSize, Rgba8Pixel, Model, ModelRc, VecModel};
use std::rc::Rc;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use rfd::FileDialog;
use oculante::settings::{PersistentSettings, VolatileSettings};
use arboard::{Clipboard, ImageData};
use std::borrow::Cow;
use image::{imageops::FilterType, ImageBuffer, DynamicImage};
use std::env;
use std::thread;

slint::include_modules!();

#[derive(Default)]
struct AppState {
    last_window_size: PhysicalSize,
    last_window_position: PhysicalPosition,
    image_list: Vec<PathBuf>,
    current_image_index: Option<usize>,
}

fn load_image_to_slint(img: DynamicImage) -> Image {
    let img_data = img.to_rgba8();
    let image_width = img_data.width();
    let image_height = img_data.height();
    Image::from_rgba8(
        SharedPixelBuffer::clone_from_slice(
            img_data.as_raw(),
            image_width,
            image_height,
        ),
    )
}

fn buffer_to_slint_image(buffer: SharedPixelBuffer<Rgba8Pixel>) -> Image {
    Image::from_rgba8(buffer)
}


fn update_image_info(ui: &AppWindow) {
    if let Some(pixel_buffer) = ui.get_image_display().to_rgba8() {
        ui.invoke_update_image_info();
        let width = ui.get_image_w();
        let height = ui.get_image_h();
        let scale = ui.get_image_scale();
        ui.set_info_text(format!("{:>0.1}% | {} x {} | {} x {}", scale * 100.0, width, height, pixel_buffer.width(), pixel_buffer.height()).into());
    }
}

fn set_image(ui: &AppWindow, thumbnail_window: &ThumbnailWindow, app_state: &mut AppState, path: PathBuf, volatile_settings: &Rc<RefCell<VolatileSettings>>) {
    if let Ok(img) = image::open(&path) {
        let new_slint_image = load_image_to_slint(img);

        let parent_dir = path.parent().unwrap_or(&path).to_path_buf();
        let thumb_ui_handle = thumbnail_window.as_weak();

        // Update current directory and save to volatile settings
        if let Ok(current_dir) = std::env::current_dir() {
            if current_dir != parent_dir {
                let _ = std::env::set_current_dir(&parent_dir);
                volatile_settings.borrow_mut().last_open_directory = parent_dir.clone();
                let _ = volatile_settings.borrow_mut().save_blocking();
            }
        } else {
            // Handle error case for current_dir, e.g., set to parent_dir and save
            let _ = std::env::set_current_dir(&parent_dir);
            volatile_settings.borrow_mut().last_open_directory = parent_dir.clone();
            let _ = volatile_settings.borrow_mut().save_blocking();
        }

        // Always re-scan and update thumbnail list if directory changes or is empty
        if app_state.image_list.is_empty() || app_state.image_list[0].parent().map_or(true, |p| p != parent_dir) {
            let mut image_list: Vec<PathBuf> = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&parent_dir) {
                image_list = entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|p| p.is_file() && (
                        p.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") || ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("gif") || ext.eq_ignore_ascii_case("bmp"))
                    ))
                    .collect();
                image_list.sort();
            }
            
            let image_list_clone = image_list.clone();
            if let Some(thumb_ui) = thumb_ui_handle.upgrade() {
                thumb_ui.set_thumbnails(Rc::new(VecModel::default()).into());
            }

            thread::spawn(move || {
                let mut thumb_data = Vec::new();
                for p in &image_list_clone {
                    if let Ok(img) = image::open(p) {
                        let thumb = img.thumbnail(180, 120);
                        let rgba_image = thumb.to_rgba8();
                        let buffer = SharedPixelBuffer::clone_from_slice(rgba_image.as_raw(), rgba_image.width(), rgba_image.height());
                        thumb_data.push((buffer, p.to_string_lossy().to_string()));
                    }
                }

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(thumb_ui) = thumb_ui_handle.upgrade() {
                        let thumbnails: Rc<VecModel<Thumbnail>> = Rc::new(VecModel::default());
                        for (buffer, path) in thumb_data {
                            thumbnails.push(Thumbnail {
                                source: buffer_to_slint_image(buffer),
                                path: path.into(),
                            });
                        }
                        thumb_ui.set_thumbnails(thumbnails.into());
                    }
                });
            });

            app_state.image_list = image_list;
        }

        app_state.current_image_index = app_state.image_list.iter().position(|p| p == &path);
        
        ui.set_auto_fit(true);
        ui.set_image_display(new_slint_image);
        update_image_info(&ui);
        ui.set_show_resize_dialog(false);
        ui.set_status_text(format!("Loaded: {}", path.to_string_lossy()).into());
    } else {
        ui.set_status_text(format!("Failed to load: {}", path.to_string_lossy()).into());
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let persistent_settings = Rc::new(RefCell::new(PersistentSettings::load().unwrap_or_default()));
    let volatile_settings = Rc::new(RefCell::new(VolatileSettings::load().unwrap_or_default()));

    let main_window = AppWindow::new()?;
    let settings_window = SettingsWindow::new()?;
    let thumbnail_window = ThumbnailWindow::new()?;
    let app_state = Rc::new(RefCell::new(AppState::default()));

    // --- Initial state setup from settings ---
    let initial_pos: PhysicalPosition = volatile_settings.borrow().window_position.into();
    let initial_size: PhysicalSize = volatile_settings.borrow().window_size.into();

    main_window.window().set_position(initial_pos);
    main_window.window().set_size(initial_size);

    // --- Initialize AppState with current window geometry ---
    app_state.borrow_mut().last_window_position = initial_pos;
    app_state.borrow_mut().last_window_size = initial_size;

    // --- Initial state setup ---
    main_window.set_status_text("Ready. Open a file to begin.".into());
    settings_window.set_vsync_enabled(persistent_settings.borrow().vsync);
    settings_window.set_show_checker_background(persistent_settings.borrow().show_checker_background);

    // Set initial current directory based on last opened directory
    let default_dir = PathBuf::from(".");
    let initial_dir_for_env = volatile_settings.borrow().last_open_directory.clone();
    let _ = std::env::set_current_dir(initial_dir_for_env);


    // --- Handle command line arguments ---
    let volatile_settings_clone_cl = volatile_settings.clone();
    if let Some(path_str) = env::args().nth(1) {
        set_image(&main_window, &thumbnail_window, &mut app_state.borrow_mut(), PathBuf::from(path_str), &volatile_settings_clone_cl);
    }

    // --- Main window callbacks ---
    let main_window_handle = main_window.as_weak();
    let thumbnail_window_handle = thumbnail_window.as_weak();
    let app_state_clone = app_state.clone();
    let volatile_settings_clone = volatile_settings.clone();
    main_window.on_request_open_file(move || {
        let ui = main_window_handle.unwrap();
        let thumb_ui = thumbnail_window_handle.unwrap();
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .pick_file() 
        {
            set_image(&ui, &thumb_ui, &mut app_state_clone.borrow_mut(), path, &volatile_settings_clone);
        } else {
            ui.set_status_text("File open cancelled.".into());
        }
    });

    main_window.on_exit(move || {
        slint::quit_event_loop().unwrap();
    });

    let main_window_handle_reset = main_window.as_weak();
    main_window.on_reset_view(move || {
        let ui = main_window_handle_reset.unwrap();
        ui.set_auto_fit(true);
        update_image_info(&ui);
        ui.set_status_text("View reset.".into());
    });

    let v_settings = volatile_settings.clone();
    let main_window_handle_1_1 = main_window.as_weak();
    main_window.on_view_one_to_one(move || {
        let ui = main_window_handle_1_1.unwrap();
        v_settings.borrow_mut().image_scale = 1.0;
        ui.set_auto_fit(false);
        ui.set_image_scale(1.0);
        update_image_info(&ui);
        ui.set_status_text("View 1:1".into());
    });

    let settings_window_handle = settings_window.as_weak();
    main_window.on_show_settings_window(move || {
        if let Some(settings_ui) = settings_window_handle.upgrade() {
            settings_ui.show();
        }
    });

    let main_window_handle_resize = main_window.as_weak();
    main_window.on_show_resize_window(move || {
        let ui = main_window_handle_resize.unwrap();
        if let Some(pixel_buffer) = ui.get_image_display().to_rgba8() {
            ui.set_resize_dialog_original_w(pixel_buffer.width() as i32);
            ui.set_resize_dialog_original_h(pixel_buffer.height() as i32);
            ui.set_resize_dialog_new_w(pixel_buffer.width() as i32);
            ui.set_resize_dialog_new_h(pixel_buffer.height() as i32);
            ui.set_resize_dialog_lock_aspect(true);
        }
    });

    let main_window_handle_resize_confirmed = main_window.as_weak();
    main_window.on_resize_confirmed(move || {
        // println!("[DEBUG] resize_confirmed callback triggered.");
        let ui = main_window_handle_resize_confirmed.unwrap();
        if let Some(pixel_buffer) = ui.get_image_display().to_rgba8() {

            let new_w = ui.get_resize_dialog_new_w() as u32;
            let new_h = ui.get_resize_dialog_new_h() as u32;

            let interpolation = ui.get_resize_dialog_interpolation();
            let interp = if interpolation == "Nearest" {
                FilterType::Nearest
            } else if interpolation == "Triangle" {
                FilterType::Triangle
            } else if interpolation == "CatmullRom" {
                FilterType::CatmullRom
            } else if interpolation == "Gaussian" {
                FilterType::Gaussian
            } else {
                FilterType::Lanczos3
            };
            // println!("[DEBUG] Resizing to: {}x{}", new_w, new_h);
            // println!("[DEBUG] Interpolation: {:?}", interp);
            // println!("[DEBUG] Original size: {}x{}", pixel_buffer.width(), pixel_buffer.height());

            let img_buffer: ImageBuffer<image::Rgba<u8>, _> = ImageBuffer::from_raw(
                pixel_buffer.width(),
                pixel_buffer.height(),
                pixel_buffer.as_bytes().to_vec(),
            ).unwrap();

            let resized = image::imageops::resize(&img_buffer, new_w, new_h, interp);
            let new_pixel_buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(resized.as_raw(), new_w, new_h);
            let new_image = Image::from_rgba8(new_pixel_buffer);
            // println!("[DEBUG] Resized image created. Setting display.");

            ui.set_image_display(new_image);
            update_image_info(&ui);
            ui.set_status_text("Image resized.".into());
        // } else {
            // println!("[DEBUG] Could not get pixel_buffer in resize_confirmed."); 
        }
    });

    let main_window_handle_c_c = main_window.as_weak();
    main_window.on_copy_to_clipboard(move || {
        let ui = main_window_handle_c_c.unwrap();
        if let Some(pixel_buffer) = ui.get_image_display().to_rgba8(){
            let image_data = ImageData {
                width: pixel_buffer.width() as usize,
                height: pixel_buffer.height() as usize,
                bytes: Cow::Owned(pixel_buffer.as_bytes().to_vec()),
            };
            let mut clipboard = Clipboard::new().unwrap();
            if clipboard.set_image(image_data).is_ok() {
                ui.set_status_text("Image (RGBA) copied to clipboard.".into());
            } else {
                ui.set_status_text("Failed to copy image to clipboard.".into());
            }
        }
    });

    let main_window_handle_p_c = main_window.as_weak();
    let app_state_p_c = app_state.clone();
    main_window.on_paste_from_clipboard(move || {
        let ui = main_window_handle_p_c.unwrap();
        let mut clipboard = Clipboard::new().unwrap();
        if let Ok(clipboard_image) = clipboard.get_image() {
            let pixel_buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                &clipboard_image.bytes,
                clipboard_image.width as u32,
                clipboard_image.height as u32,
            );
            let slint_image = Image::from_rgba8(pixel_buffer);

            // Pasted image doesn't have a path, so clear the list
            let mut app = app_state_p_c.borrow_mut();
            app.image_list.clear();
            app.current_image_index = None;

            ui.set_auto_fit(true);
            ui.set_image_display(slint_image);
            update_image_info(&ui);
            ui.set_status_text("Image pasted from clipboard.".into());
        } else {
            ui.set_status_text("No image found on clipboard.".into());
        }
    });

    let main_window_handle_next = main_window.as_weak();
    let thumbnail_window_handle_next = thumbnail_window.as_weak();
    let app_state_next = app_state.clone();
    let volatile_settings_clone_next = volatile_settings.clone();
    main_window.on_next_image(move || {
        let ui = main_window_handle_next.unwrap();
        let thumb_ui = thumbnail_window_handle_next.unwrap();
        let mut app = app_state_next.borrow_mut();
        if let Some(index) = app.current_image_index {
            if !app.image_list.is_empty() {
                let new_index = (index + 1) % app.image_list.len();
                let next_path = app.image_list[new_index].clone();
                set_image(&ui, &thumb_ui, &mut app, next_path, &volatile_settings_clone_next);
            }
        }
    });

    let main_window_handle_prev = main_window.as_weak();
    let thumbnail_window_handle_prev = thumbnail_window.as_weak();
    let app_state_prev = app_state.clone();
    let volatile_settings_clone_prev = volatile_settings.clone();
    main_window.on_previous_image(move || {
        let ui = main_window_handle_prev.unwrap();
        let thumb_ui = thumbnail_window_handle_prev.unwrap();
        let mut app = app_state_prev.borrow_mut();
        if let Some(index) = app.current_image_index {
            if !app.image_list.is_empty() {
                let new_index = (index + app.image_list.len() - 1) % app.image_list.len();
                let prev_path = app.image_list[new_index].clone();
                set_image(&ui, &thumb_ui, &mut app, prev_path, &volatile_settings_clone_prev);
            }
        }
    });

    let thumbnail_window_handle_toggle = thumbnail_window.as_weak();
    main_window.on_show_thumbnail_window(move || {
        if let Some(thumb_ui) = thumbnail_window_handle_toggle.upgrade() {
            if thumb_ui.window().is_visible() {
                thumb_ui.hide();
            } else {
                thumb_ui.show();
            }
        }
    });

    let main_window_handle_thumb_click = main_window.as_weak();
    let thumbnail_window_handle_thumb_click = thumbnail_window.as_weak();
    let app_state_thumb_click = app_state.clone();
    let volatile_settings_clone_thumb_click = volatile_settings.clone();
    thumbnail_window.on_thumbnail_clicked(move |path_str| {
        let ui = main_window_handle_thumb_click.unwrap();
        let thumb_ui = thumbnail_window_handle_thumb_click.unwrap();
        let path = PathBuf::from(path_str.to_string());
        set_image(&ui, &thumb_ui, &mut app_state_thumb_click.borrow_mut(), path, &volatile_settings_clone_thumb_click);
    });

    let v_settings_zoom = volatile_settings.clone();
    let main_window_handle_zoom = main_window.as_weak();
    main_window.on_zoom_image(move |delta_y: f32, mouse_x: f32, mouse_y: f32| {
        let ui = main_window_handle_zoom.unwrap();
        let mut volatile = v_settings_zoom.borrow_mut();
        let zoom_amount: f64 = 0.1;
        let old_scale: f64 = volatile.image_scale;

        let new_scale: f64 = if delta_y < 0.0 {
            old_scale * (1.0 + zoom_amount)
        } else {
            old_scale / (1.0 + zoom_amount)
        };
        let new_scale = new_scale.max(0.1).min(10.0);

        let old_image_x: f64 = ui.get_image_x() as f64;
        let old_image_y: f64 = ui.get_image_y() as f64;

        let mouse_img_x: f64 = (mouse_x as f64 - old_image_x) / old_scale;
        let mouse_img_y: f64 = (mouse_y as f64 - old_image_y) / old_scale;

        let new_image_x: f64 = mouse_x as f64 - mouse_img_x * new_scale;
        let new_image_y: f64 = mouse_y as f64 - mouse_img_y * new_scale;

        volatile.image_scale = new_scale;
        ui.set_image_scale(new_scale as f32);
        ui.set_image_x(new_image_x as i32);
        ui.set_image_y(new_image_y as i32);
        
        update_image_info(&ui);
    });

    let v_settings_scale = volatile_settings.clone();
    // let main_window_handle_scale_changed = main_window.as_weak();
    main_window.on_scale_changed(move |new_scale: f32| {
        // let ui = main_window_handle_scale_changed.unwrap();
        let mut volatile = v_settings_scale.borrow_mut();
        volatile.image_scale = new_scale as f64;
    });

    // --- Tick handler for dynamic resize/move ---
    let ui_handle_tick = main_window.as_weak();
    let app_state_clone_tick = Rc::clone(&app_state);
    let volatile_settings_clone_tick = Rc::clone(&volatile_settings);
    main_window.on_tick(move |auto_fit| {
        let ui = ui_handle_tick.unwrap();
        let mut app_state = app_state_clone_tick.borrow_mut();
        let mut volatile = volatile_settings_clone_tick.borrow_mut();

        let current_pos = ui.window().position();
        let current_size = ui.window().size();

        if app_state.last_window_position != current_pos {
            volatile.window_position = current_pos.into();
            let _ = volatile.save_blocking();
            app_state.last_window_position = current_pos;
        }

        if app_state.last_window_size != current_size {
            volatile.window_size = current_size.into();
            let _ = volatile.save_blocking();
            app_state.last_window_size = current_size;
            // Trigger auto-fit when window size changes
            ui.set_auto_fit(auto_fit);
        }
        update_image_info(&ui);
    });

    // --- Settings window callbacks ---
    let settings_clone1 = Rc::clone(&persistent_settings);
    settings_window.on_vsync_changed(move |enabled| {
        let mut settings = settings_clone1.borrow_mut();
        settings.vsync = enabled;
        let _ = settings.save_blocking();
    });

    let settings_clone2 = Rc::clone(&persistent_settings);
    settings_window.on_show_checker_background_changed(move |enabled| {
        let mut settings = settings_clone2.borrow_mut();
        settings.show_checker_background = enabled;
        let _ = settings.save_blocking();
    });

    main_window.run()
}
