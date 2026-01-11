#![windows_subsystem = "windows"]
//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use slint::{Image, SharedPixelBuffer, ComponentHandle, Weak, PhysicalPosition, PhysicalSize, Rgba8Pixel};
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;
use rfd::FileDialog;
use oculante::settings::{PersistentSettings, VolatileSettings};
use arboard::{Clipboard, ImageData};
use std::borrow::Cow;
use image::{imageops::FilterType, ImageBuffer};
use std::env;

slint::include_modules!();

#[derive(Default)]
struct AppState {
    last_window_size: PhysicalSize,
    last_window_position: PhysicalPosition,
}

fn load_image_to_slint(path: PathBuf) -> Option<Image> {
    image::open(&path).ok().map(|img| {
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
    })
}

fn update_info_text(ui: &AppWindow) {
    if let Some(pixel_buffer) = ui.get_image_display().to_rgba8() {
        ui.invoke_update_image_info();
        let width = ui.get_image_w();
        let height = ui.get_image_h();
        let scale = ui.get_image_scale();
        ui.set_info_text(format!("{:>0.1}% | {} x {} | {} x {}", scale * 100.0, width, height, pixel_buffer.width(), pixel_buffer.height()).into());
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let persistent_settings = Rc::new(RefCell::new(PersistentSettings::load().unwrap_or_default()));
    let volatile_settings = Rc::new(RefCell::new(VolatileSettings::load().unwrap_or_default()));

    let main_window = AppWindow::new()?;
    let settings_window = SettingsWindow::new()?;
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

    // --- Handle command line arguments ---
    if let Some(path_str) = env::args().nth(1) {
        let path = PathBuf::from(path_str);
        if let Some(new_slint_image) = load_image_to_slint(path.clone()) {
            main_window.set_auto_fit(true);
            main_window.set_image_display(new_slint_image);
            update_info_text(&main_window);
            main_window.set_status_text(format!("Loaded: {}", path.to_string_lossy()).into());
        }
    }

    // --- Main window callbacks ---
    let main_window_handle = main_window.as_weak();
    main_window.on_request_open_file(move || {
        let ui = main_window_handle.unwrap();
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            let path_str = path.to_string_lossy().to_string();
            if let Some(new_slint_image) = load_image_to_slint(path) {
                ui.set_auto_fit(true);
                ui.set_image_display(new_slint_image);
                update_info_text(&ui);
                ui.set_show_resize_dialog(false);
                ui.set_status_text(format!("Loaded: {}", path_str).into());
            } else {
                ui.set_status_text("Failed to load image.".into());
            }
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
        update_info_text(&ui);
        ui.set_status_text("View reset.".into());
    });

    let v_settings = volatile_settings.clone();
    let main_window_handle_1_1 = main_window.as_weak();
    main_window.on_view_one_to_one(move || {
        let ui = main_window_handle_1_1.unwrap();
        v_settings.borrow_mut().image_scale = 1.0;
        ui.set_auto_fit(false);
        ui.set_image_scale(1.0);
        update_info_text(&ui);
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
        println!("[DEBUG] resize_confirmed callback triggered.");
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
            println!("[DEBUG] Resizing to: {}x{}", new_w, new_h);
            println!("[DEBUG] Interpolation: {:?}", interp);
            println!("[DEBUG] Original size: {}x{}", pixel_buffer.width(), pixel_buffer.height());

            let img_buffer: ImageBuffer<image::Rgba<u8>, _> = ImageBuffer::from_raw(
                pixel_buffer.width(),
                pixel_buffer.height(),
                pixel_buffer.as_bytes().to_vec(),
            ).unwrap();

            let resized = image::imageops::resize(&img_buffer, new_w, new_h, interp);
            let new_pixel_buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(resized.as_raw(), new_w, new_h);
            let new_image = Image::from_rgba8(new_pixel_buffer);
            println!("[DEBUG] Resized image created. Setting display.");

            ui.set_image_display(new_image);
            update_info_text(&ui);
            ui.set_status_text("Image resized.".into());
        } else {
            println!("[DEBUG] Could not get pixel_buffer in resize_confirmed."); 
        }
    });

    let main_window_handle_c_c = main_window.as_weak();
    main_window.on_copy_to_clipboard(move || {
        let ui = main_window_handle_c_c.unwrap();
        let slint_image = ui.get_image_display();

        if let Some(pixel_buffer) = slint_image.to_rgba8(){
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
    main_window.on_paste_from_clipboard(move || {
        let ui = main_window_handle_p_c.unwrap();
        let mut clipboard = Clipboard::new().unwrap();
        if let Ok(clipboard_image) = clipboard.get_image() {
            let pixel_buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                &clipboard_image.bytes,
                clipboard_image.width as u32,
                clipboard_image.height as u32,
            );
            ui.set_auto_fit(true);
            let slint_image = Image::from_rgba8(pixel_buffer);
            ui.set_image_display(slint_image);
            update_info_text(&ui);
            ui.set_status_text("Image pasted from clipboard.".into());
        } else {
            ui.set_status_text("No image found on clipboard.".into());
        }
    });

    let v_settings = volatile_settings.clone();
    let main_window_handle_zoom = main_window.as_weak();
    main_window.on_zoom_image(move |delta_y: f32, mouse_x: f32, mouse_y: f32| {
        let ui = main_window_handle_zoom.unwrap();
        let mut volatile = v_settings.borrow_mut();
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
        
        update_info_text(&ui);
    });

    let v_settings = volatile_settings.clone();
    // let main_window_handle_scale_changed = main_window.as_weak();
    main_window.on_scale_changed(move |new_scale: f32| {
        // let ui = main_window_handle_scale_changed.unwrap();
        let mut volatile = v_settings.borrow_mut();
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
        update_info_text(&ui);
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
