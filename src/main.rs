use slint::{Image, SharedPixelBuffer, ComponentHandle, LogicalPosition, Weak};
use std::rc::Rc;
use std::path::PathBuf;
use rfd::FileDialog;
 
slint::include_modules!();

// AppState は、slint 側で状態を管理するため不要になるが、
// Rust 側で他の状態が必要になる可能性を考慮して残しておく
struct AppState {
    // last_mouse_pos: LogicalPosition, // .slint 側で管理
    // is_dragging: bool, // .slint 側で管理
    // image_x: f32, // .slint 側で管理
    // image_y: f32, // .slint 側で管理
    // image_scale: f32, // .slint 側で管理
}

impl Default for AppState {
    fn default() -> Self {
        AppState {}
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;

    ui.set_status_text("Ready. Open a file to begin.".into());
    let ui_handle = ui.as_weak();

    ui.on_request_open_file(move || {
        let ui = ui_handle.unwrap();
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            let path_str = path.to_string_lossy().to_string();
            if let Some(new_slint_image) = load_image_to_slint(path) {
                // Reset state on new image
                ui.set_image_x(0.0);
                ui.set_image_y(0.0);
                ui.set_image_scale(1.0);
                ui.set_image_display(new_slint_image);
                ui.set_status_text(format!("Loaded: {}", path_str).into());
            } else {
                ui.set_status_text("Failed to load image.".into());
            }
        } else {
            ui.set_status_text("File open cancelled.".into());
        }
    });

    ui.on_exit(move || {
        slint::quit_event_loop().unwrap();
    });
/*
    let ui_handle_zoom = ui.as_weak();
    ui.on_zoom_image(move |delta_y, mouse_x, mouse_y| {
        let ui = ui_handle_zoom.unwrap();

        let zoom_amount = 0.1;
        let old_scale = ui.get_image_scale();
        
        // Convert mouse_x, mouse_y from Slint's `length` type to `float` for calculation
        // Assuming 1 Slint unit == 1 pixel for now.
        let mouse_pos_x = mouse_x.to_pixels();
        let mouse_pos_y = mouse_y.to_pixels();

        let new_scale = if delta_y > 0.0 { // Inverted scroll for natural zoom (delta_y is negative for scroll down)
            old_scale * (1.0 + zoom_amount)
        } else {
            old_scale * (1.0 - zoom_amount)
        };
        let new_scale = new_scale.max(0.1).min(10.0);
        
        let old_image_x = ui.get_image_x().to_pixels();
        let old_image_y = ui.get_image_y().to_pixels();

        let mouse_img_x = (mouse_pos_x - old_image_x) / old_scale;
        let mouse_img_y = (mouse_pos_y - old_image_y) / old_scale;

        let new_image_x = mouse_pos_x - mouse_img_x * new_scale;
        let new_image_y = mouse_pos_y - mouse_img_y * new_scale;

        ui.set_image_scale(new_scale);
        ui.set_image_x(new_image_x.into()); // Convert back to Slint's length
        ui.set_image_y(new_image_y.into()); // Convert back to Slint's length
        
        ui.set_status_text(format!("Zoom: {:.0}%", new_scale * 100.0).into());
    });
*/
    ui.run()
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
