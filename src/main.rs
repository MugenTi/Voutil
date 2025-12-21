use slint::{Image, SharedPixelBuffer, ComponentHandle, Weak, PhysicalPosition, PhysicalSize};
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;
use rfd::FileDialog;
use oculante::settings::{PersistentSettings, VolatileSettings};

slint::include_modules!();

#[derive(Default)]
struct AppState {
    last_window_size: PhysicalSize,
    last_window_position: PhysicalPosition,
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
    
    // Initialize AppState with current window geometry
    app_state.borrow_mut().last_window_position = initial_pos;
    app_state.borrow_mut().last_window_size = initial_size;

    // --- Initial state setup ---
    main_window.set_status_text("Ready. Open a file to begin.".into());
    settings_window.set_vsync_enabled(persistent_settings.borrow().vsync);
    settings_window.set_show_checker_background(persistent_settings.borrow().show_checker_background);

    // --- Main window callbacks ---
    let main_window_handle = main_window.as_weak();
    main_window.on_request_open_file(move || {
        let ui = main_window_handle.unwrap();
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            let path_str = path.to_string_lossy().to_string();
            if let Some(new_slint_image) = load_image_to_slint(path) {
                // The 'image_display-changed' callback in .slint will now handle the reset.
                ui.set_image_display(new_slint_image);
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
        // Just set the auto_fit property, Slint will handle the rest.
        ui.set_auto_fit(true); 
        ui.set_status_text("View reset.".into());
    });

    let settings_window_handle = settings_window.as_weak();
    main_window.on_show_settings_window(move || {
        if let Some(settings_ui) = settings_window_handle.upgrade() {
            settings_ui.show();
        }
    });

    let main_window_handle_zoom = main_window.as_weak();
    main_window.on_zoom_image(move |delta_y, mouse_x, mouse_y| {
        let ui = main_window_handle_zoom.unwrap();
        let zoom_amount = 0.1;
        let old_scale = ui.get_image_scale();
        
        let new_scale = if delta_y < 0.0 {
            old_scale * (1.0 + zoom_amount)
        } else {
            old_scale * (1.0 - zoom_amount)
        };
        let new_scale = new_scale.max(0.1).min(10.0);
        
        let old_image_x = ui.get_image_x();
        let old_image_y = ui.get_image_y();

        let mouse_img_x = (mouse_x - old_image_x) / old_scale;
        let mouse_img_y = (mouse_y - old_image_y) / old_scale;

        let new_image_x = mouse_x - mouse_img_x * new_scale;
        let new_image_y = mouse_y - mouse_img_y * new_scale;

        ui.set_image_scale(new_scale);
        ui.set_image_x(new_image_x);
        ui.set_image_y(new_image_y);
        
        ui.set_status_text(format!("Zoom: {:.0}%", new_scale * 100.0).into());
    });

    // --- Tick handler for dynamic resize/move ---
    let ui_handle_tick = main_window.as_weak();
    let app_state_clone_tick = Rc::clone(&app_state);
    let volatile_settings_clone_tick = Rc::clone(&volatile_settings);
    main_window.on_tick(move || {
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
            ui.set_status_text("Window resized!".into()); // For debugging
            // Trigger auto-fit when window size changes
            ui.set_auto_fit(true);
        }
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

fn load_image_to_slint(path: PathBuf) -> Option<Image> {
    image::open(&path).ok().map(|img| {
        let img_data = img.to_rgb8();
        let image_width = img_data.width();
        let image_height = img_data.height();
        Image::from_rgb8(
            SharedPixelBuffer::clone_from_slice(
                img_data.as_raw(),
                image_width,
                image_height,
            ),
        )
    })
}