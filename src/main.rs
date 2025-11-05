#![windows_subsystem = "windows"]

use clap::Arg;
use clap::Command;
#[allow(unused_imports)]
use image::{GenericImageView, ImageBuffer, Rgba};
use image_editing::LegacyEditState;
use log::debug;
use log::error;
use log::info;
use log::trace;
use log::warn;
use nalgebra::Vector2;
use notan::app::Event;
use notan::draw::*;
use notan::egui;
use notan::egui::Align;
use notan::egui::EguiConfig;
use notan::egui::EguiPluginSugar;
use notan::egui::FontData;
use notan::egui::FontDefinitions;
use notan::egui::FontFamily;
use notan::egui::FontTweak;
use notan::egui::Id;
use notan::egui::Key;
use notan::prelude::*;
use oculante::comparelist::CompareItem;
use std::io::{stdin, IsTerminal, Read};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(feature = "file_open")]
use filebrowser::browse_for_image_path;
use oculante::appstate::*;
use oculante::utils::*;
use oculante::*;
use shortcuts::key_pressed;
use ui::PANEL_WIDTH;
use ui::*;

#[cfg(feature = "turbo")]
use image_editing::lossless_tx;
use image_editing::EditState;
use scrubber::find_first_image_in_directory;
use shortcuts::InputEvent::*;

#[notan_main]
fn main() -> Result<(), String> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    // on debug builds, override log level
    #[cfg(debug_assertions)]
    {
        println!("Debug");
        std::env::set_var("RUST_LOG", "debug");
    }
    let _ = env_logger::try_init();

    let icon_data = include_bytes!("../icon.ico");

    // Determine window geometry and fullscreen state BEFORE creating the window config
    let mut start_in_fullscreen = false;
    let mut loaded_geometry: Option<((i32, i32), (u32, u32))> = None;
    match settings::VolatileSettings::load() {
        Ok(volatile_settings) => {
            if volatile_settings.is_fullscreen {
                start_in_fullscreen = true;
            } else if volatile_settings.window_geometry != Default::default() {
                loaded_geometry = Some(volatile_settings.window_geometry);
            }
        }
        Err(e) => error!("Could not load volatile settings: {e}"),
    }

    let mut window_config = WindowConfig::new()
        .set_title(&format!("Oculante | {}", env!("CARGO_PKG_VERSION")))
        .set_size(1026, 600) // window's size
        .set_resizable(true) // window can be resized
        .set_window_icon_data(Some(icon_data))
        .set_taskbar_icon_data(Some(icon_data))
        .set_multisampling(0)
        .set_fullscreen(start_in_fullscreen) // Use the flag here
        .set_app_id("oculante");

    // Apply geometry if it was loaded and we are not starting in fullscreen
    if let Some(geometry) = loaded_geometry {
        window_config = window_config.set_position(geometry.0.0, geometry.0.1);
        window_config.width = geometry.1.0 as u32;
        window_config.height = geometry.1.1 as u32;
    }

    #[cfg(target_os = "windows")]
    {
        window_config = window_config
            .set_lazy_loop(true) // don't redraw every frame on windows
            .set_vsync(true)
            .set_high_dpi(true);
    }

    #[cfg(target_os = "linux")]
    {
        window_config = window_config
            .set_lazy_loop(true)
            .set_vsync(true)
            .set_high_dpi(true);
    }

    #[cfg(any(target_os = "netbsd", target_os = "freebsd"))]
    {
        window_config = window_config.set_lazy_loop(true).set_vsync(true);
    }

    #[cfg(target_os = "macos")]
    {
        window_config = window_config
            .set_lazy_loop(true)
            .set_vsync(true)
            .set_high_dpi(true);
    }

    #[cfg(target_os = "macos")]
    {
        // MacOS needs an incredible dance performed just to open a file
        let _ = oculante::mac::launch();
    }

    // Unfortunately we need to load the persistent settings here, too - the window settings need
    // to be set before window creation
    match settings::PersistentSettings::load() {
        Ok(settings) => {
            window_config.vsync = settings.vsync;
            window_config.lazy_loop = !settings.force_redraw;
            window_config.decorations = !settings.borderless;

            trace!("Loaded settings.");
            if settings.zen_mode {
                let mut title_string = window_config.title.clone();
                title_string.push_str(&format!(
                    "          '{}' to disable zen mode",
                    shortcuts::lookup(&settings.shortcuts, &shortcuts::InputEvent::ZenMode)
                ));
                window_config = window_config.set_title(&title_string);
            }
            window_config.min_size = Some(settings.min_window_size);

            // LIBHEIF_SECURITY_LIMITS needs to be set before a libheif context is created
            #[cfg(feature = "heif")]
            settings.decoders.heif.maybe_limits();
        }
        Err(e) => {
            error!("Could not load persistent settings: {e}");
        }
    }
    window_config.always_on_top = true;
    window_config.max_size = None;

    debug!("Starting oculante.");
    notan::init_with(init)
        .add_config(window_config)
        .add_config(EguiConfig)
        .add_config(DrawConfig)
        .event(process_events)
        .update(update)
        .draw(drawe)
        .build()
}

fn init(_app: &mut App, gfx: &mut Graphics, plugins: &mut Plugins) -> OculanteState {
    debug!("Now matching arguments {:?}", std::env::args());
    // Filter out strange mac args
    let args: Vec<String> = std::env::args().filter(|a| !a.contains("psn_")).collect();

    let mut matches = Command::new("Oculante")
        .arg(
            Arg::new("INPUT")
                .help("Display this image")
                .multiple_values(true), // .index(1)
                                        // )
        )
        .arg(
            Arg::new("l")
                .short('l')
                .help("Listen on port")
                .takes_value(true),
        )
        .arg(
            Arg::new("stdin")
                .short('s')
                .id("stdin")
                .takes_value(false)
                .help("Load data from STDIN"),
        )
        .arg(
            Arg::new("chainload")
                .required(false)
                .takes_value(false)
                .short('c')
                .help("Chainload on Mac"),
        )
        .get_matches_from(args);

    debug!("Completed argument parsing.");

    let mut state = OculanteState::default();

    state.player = Player::new(
        state.texture_channel.0.clone(),
        state.persistent_settings.max_cache,
        state.message_channel.0.clone(),
        state.persistent_settings.decoders,
    );

    debug!("matches {:?}", matches);

    let paths_to_open = piped_paths(&matches)
        .map(|iter| iter.collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(
            matches
                .remove_many::<String>("INPUT")
                .unwrap_or_default()
                .map(PathBuf::from),
        )
        .collect::<Vec<_>>();

    debug!("Image is: {:?}", paths_to_open);

    if paths_to_open.len() == 1 {
        let location = paths_to_open
            .into_iter()
            .next()
            .expect("It should be tested already that exactly one argument was passed.");
        if location.is_dir() {
            // Folder - Pick first image from the folder...
            if let Ok(first_img_location) = find_first_image_in_directory(&location) {
                state.is_loaded = false;
                state.player.load(&first_img_location);
                state.current_path = Some(first_img_location);
            }
        } else {
            state.is_loaded = false;
            state.player.load(&location);
            state.current_path = Some(location);
        };
    } else if paths_to_open.len() > 1 {
        let location = paths_to_open
            .first()
            .expect("It should be verified already that exactly one argument was passed.");
        if location.is_dir() {
            // Folder - Pick first image from the folder...
            if let Ok(first_img_location) = find_first_image_in_directory(location) {
                state.is_loaded = false;
                state.current_path = Some(first_img_location.clone());
                state.player.load_advanced(
                    &first_img_location,
                    Some(Frame::ImageCollectionMember(Default::default())),
                );
            }
        } else {
            state.is_loaded = false;
            state.current_path = Some(location.clone());
            state.player.load_advanced(
                location,
                Some(Frame::ImageCollectionMember(Default::default())),
            );
        };

        // If launched with more than one path and none of those paths are directories, it's likely
        // that the user wants to view a fixed set of images rather than traverse into directories.
        // This handles the case where the app is launched with files from different dirs as well e.g.
        // a/1.png b/2.png c/3.png
        state.scrubber.fixed_paths = paths_to_open.iter().all(|path| path.is_file());
        state.scrubber.entries = paths_to_open;
    }

    // Clear selection after initial image load
    state.selection_rect = None;
    state.is_selecting = false;
    state.selection_drag = SelectionDrag::None;

    if matches.contains_id("stdin") {
        debug!("Trying to read from pipe");
        let mut input = vec![];
        if let Ok(bytes_read) = std::io::stdin().read_to_end(&mut input) {
            if bytes_read > 0 {
                debug!("There was stdin");

                match image::load_from_memory(input.as_ref()) {
                    Ok(i) => {
                        // println!("got image");
                        debug!("Sending image!");
                        let _ = state
                            .texture_channel
                            .0
                            .clone()
                            .send(utils::Frame::new_reset(i));
                    }
                    Err(e) => error!("ERR loading from stdin: {e} - for now, oculante only supports data that can be decoded by the image crate."),
                }
            }
        }
    }

    if let Some(port) = matches.value_of("l") {
        match port.parse::<i32>() {
            Ok(p) => {
                state.send_message_info(&format!("Listening on {p}"));
                net::recv(p, state.texture_channel.0.clone());
                state.current_path = Some(PathBuf::from(&format!("network port {p}")));
                state.network_mode = true;
            }
            Err(_) => error!("Port must be a number"),
        }
    }

    // Set up egui style / theme
    plugins.egui(|ctx| {
        // FIXME: Wait for https://github.com/Nazariglez/notan/issues/315 to close, then remove

        let mut fonts = FontDefinitions::default();
        egui_extras::install_image_loaders(ctx);

        ctx.options_mut(|o| o.zoom_with_keyboard = false);

        info!("This Display has DPI {:?}", gfx.dpi());
        let offset = if gfx.dpi() > 1.0 { 0.0 } else { -1.4 };

        fonts.font_data.insert(
            "inter".to_owned(),
            FontData::from_static(FONT).tweak(FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: offset,
                baseline_offset_factor: 0.0,
            }),
        );

        fonts.font_data.insert(
            "inter_bold".to_owned(),
            FontData::from_static(BOLD_FONT).tweak(FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: offset,
                baseline_offset_factor: 0.0,
            }),
        );
        fonts.families.insert(
            FontFamily::Name("bold".to_owned().into()),
            vec!["inter_bold".into()],
        );

        fonts.font_data.insert(
            "icons".to_owned(),
            FontData::from_static(include_bytes!("../res/fonts/icons.ttf")).tweak(FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: 1.0,
                baseline_offset_factor: 0.0,
            }),
        );

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "icons".to_owned());

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "inter".to_owned());

        let fonts = load_system_fonts(fonts);

        debug!("Theme {:?}", state.persistent_settings.theme);
        apply_theme(&mut state, ctx);
        ctx.set_fonts(fonts);
    });

    // load checker texture
    if let Ok(checker_image) = image::load_from_memory(include_bytes!("../res/checker.png")) {
        // state.checker_texture = checker_image.into_rgba8().to_texture(gfx);
        // No mipmaps for the checker pattern!
        let img = checker_image.into_rgba8();
        state.checker_texture = gfx
            .create_texture()
            .from_bytes(&img, img.width(), img.height())
            .with_mipmaps(false)
            .with_format(notan::prelude::TextureFormat::SRgba8)
            .build()
            .ok();
    }

    // force a frame to render so ctx() has a size (important for centering the image)
    gfx.render(&plugins.egui(|_| {}));

    state
}



fn image_rect_from_image_geometry(
    image_geometry: &ImageGeometry,
    _window_width: f32,
    _window_height: f32,
) -> egui::Rect {
    let img_w = image_geometry.dimensions.0 as f32 * image_geometry.scale;
    let img_h = image_geometry.dimensions.1 as f32 * image_geometry.scale;

    let x = image_geometry.offset.x;
    let y = image_geometry.offset.y;

    egui::Rect::from_min_max(
        egui::pos2(x, y),
        egui::pos2(x + img_w, y + img_h),
    )
}



fn process_events(app: &mut App, state: &mut OculanteState, evt: Event) {
    if state.key_grab {
        return;
    }
    match evt {
        Event::KeyUp { .. } => {
            // Fullscreen needs to be on key up on mac (bug)
            if key_pressed(app, state, Fullscreen) {
                toggle_fullscreen(app, state);
            }
        }
        Event::KeyDown { .. } => {
            debug!("key down");

            // return;
            // pan image with keyboard
            let delta = 40.;
            if key_pressed(app, state, PanRight) {
                state.image_geometry.offset.x -= delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanUp) {
                state.image_geometry.offset.y += delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanLeft) {
                state.image_geometry.offset.x += delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanDown) {
                state.image_geometry.offset.y -= delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, CompareNext) {
                compare_next(app, state);
            }
            if key_pressed(app, state, ResetView) {
                state.reset_image = true
            }
            if key_pressed(app, state, ZenMode) {
                toggle_zen_mode(state, app);
                state.reset_image = true;
            }
            if key_pressed(app, state, PerfectFullscreen) {
                let is_fullscreen = app.window().is_fullscreen();
                toggle_fullscreen(app, state);
                if !is_fullscreen {
                    set_zen_mode(state, app, true);
                } else {
                    set_zen_mode(state, app, state.persistent_settings.zen_mode_normal);
                }
                state.reset_image = true;
            }
            if key_pressed(app, state, ZoomActualSize) {
                set_zoom(1.0, None, state);
            }
            if key_pressed(app, state, ZoomDouble) {
                set_zoom(2.0, None, state);
            }
            if key_pressed(app, state, ZoomThree) {
                set_zoom(3.0, None, state);
            }
            if key_pressed(app, state, ZoomFour) {
                set_zoom(4.0, None, state);
            }
            if key_pressed(app, state, ZoomFive) {
                set_zoom(5.0, None, state);
            }
            if key_pressed(app, state, Copy) {
                if let Some(img) = &state.current_image {
                    clipboard_copy(img);
                    state.send_message_info("Image copied");
                }
            }
            if key_pressed(app, state, CopySelection) {
                if let Some(selection_rect) = state.selection_rect {
                    // Call a helper function to copy the selected region
                    copy_selected_region(state, selection_rect);
                }
            }
            if key_pressed(app, state, CropSelection) {
                if let Some(selection_rect) = state.selection_rect {
                    // Call a helper function to crop the image to the selected region
                    crop_to_selected_region(state, selection_rect);
                }
            }

            if key_pressed(app, state, SelectAll) {
                if let Some(image) = &state.current_image {
                    let (w, h) = image.dimensions();
                    state.selection_rect = Some(egui::Rect::from_min_max(
                        egui::pos2(0.0, 0.0),
                        egui::pos2(w as f32, h as f32),
                    ));
                }
            }

            if key_pressed(app, state, Deselect) {
                state.selection_rect = None;
            }

            if key_pressed(app, state, Paste) {
                match clipboard_to_image() {
                    Ok(img) => {
                        state.current_path = None;
                        // Stop in the even that an animation is running
                        state.player.stop();
                        _ = state
                            .player
                            .image_sender
                            .send(crate::utils::Frame::new_still(img));
                        // Since pasted data has no path, make sure it's not set
                        state.send_message_info("Image pasted");
                    }
                    Err(e) => state.send_message_err(&e.to_string()),
                }
            }
            if key_pressed(app, state, Quit) {
                app.exit();
            }

            if app.keyboard.was_pressed(KeyCode::Escape) {
                if state.file_browser_visible {
                    state.file_browser_visible = false;
                    state.file_browser_save = false;
                } else if app.window().is_fullscreen() {
                    toggle_fullscreen(app, state);
                    set_zen_mode(state, app, state.persistent_settings.zen_mode_normal);
                    state.reset_image = true;
                } else {
                    app.exit();
                }
            }
            #[cfg(feature = "turbo")]
            if key_pressed(app, state, LosslessRotateRight) {
                debug!("Lossless rotate right");

                if let Some(p) = &state.current_path {
                    if lossless_tx(p, turbojpeg::Transform::op(turbojpeg::TransformOp::Rot90))
                        .is_ok()
                    {
                        state.is_loaded = false;
                        // This needs "deep" reload
                        state.player.cache.clear();
                        state.player.load(p);
                    }
                }
            }
            #[cfg(feature = "turbo")]
            if key_pressed(app, state, LosslessRotateLeft) {
                debug!("Lossless rotate left");
                if let Some(p) = &state.current_path {
                    if lossless_tx(p, turbojpeg::Transform::op(turbojpeg::TransformOp::Rot270))
                        .is_ok()
                    {
                        state.is_loaded = false;
                        // This needs "deep" reload
                        state.player.cache.clear();
                        state.player.load(p);
                    } else {
                        warn!("rotate left failed")
                    }
                }
            }
            if key_pressed(app, state, Browse) {
                state.redraw = true;
                #[cfg(feature = "file_open")]
                browse_for_image_path(state);
                #[cfg(not(feature = "file_open"))]
                {
                    state.file_browser_visible = !state.file_browser_visible;
                }
            }

            if key_pressed(app, state, NextImage) {
                next_image(state)
            }
            if key_pressed(app, state, PreviousImage) {
                prev_image(state)
            }
            if key_pressed(app, state, FirstImage) {
                first_image(state)
            }
            if key_pressed(app, state, LastImage) {
                last_image(state)
            }
            if key_pressed(app, state, AlwaysOnTop) {
                state.always_on_top = !state.always_on_top;
                app.window().set_always_on_top(state.always_on_top);
            }
            if key_pressed(app, state, InfoMode) {
                state.persistent_settings.info_enabled = !state.persistent_settings.info_enabled;
            }
            if key_pressed(app, state, EditMode) {
                state.persistent_settings.edit_enabled = !state.persistent_settings.edit_enabled;
            }
            if key_pressed(app, state, DeleteFile) {
                // TODO: needs confirmation
                delete_file(state);
            }
            if key_pressed(app, state, ClearImage) {
                clear_image(state);
            }
            if key_pressed(app, state, ZoomIn) {
                let scale: f32 = state.image_geometry.scale;
                let new_scale: f32 = get_new_scale(
                    scale,
                    state.persistent_settings.zoom_multiplier,
                    false,
                );
                if new_scale > 0.05 && new_scale < 40. {
                    // We want to zoom towards the center
                    let center: Vector2<f32> = nalgebra::Vector2::new(
                        app.window().width() as f32 / 2.,
                        app.window().height() as f32 / 2.,
                    );
                    let scale_inc: f32 = scale - new_scale;
                    state.image_geometry.offset += scale_pt(
                        state.image_geometry.offset,
                        center,
                        scale,
                        scale_inc,
                    );
                    state.image_geometry.scale = new_scale;
                }
            }
            if key_pressed(app, state, ZoomOut) {
                let scale: f32 = state.image_geometry.scale;
                let new_scale: f32 = get_new_scale(
                    scale,
                    state.persistent_settings.zoom_multiplier,
                    true,
                );
                if new_scale > 0.05 && new_scale < 40. {
                    // We want to zoom towards the center
                    let center: Vector2<f32> = nalgebra::Vector2::new(
                        app.window().width() as f32 / 2.,
                        app.window().height() as f32 / 2.,
                    );
                    let scale_inc: f32 = scale - new_scale;
                    state.image_geometry.offset += scale_pt(
                        state.image_geometry.offset,
                        center,
                        scale,
                        scale_inc,
                    );
                    state.image_geometry.scale = new_scale;
                }
            }
        }
        Event::WindowResize { width, height } => {
            //TODO: remove this if save on exit works
            state.volatile_settings.window_geometry.1 = (width, height);
            let pos = app.window().position();
            state.volatile_settings.window_geometry.0 = (pos.0, pos.1);

            // Save the volatile settings whenever the window is resized.
            _ = state.volatile_settings.save_blocking();

            // By resetting the image, we make it fill the window on resize
            if state.persistent_settings.fit_image_on_window_resize {
                state.reset_image = true;
            }
        }
        _ => (),
    }

    match evt {
        Event::Exit => {
            info!("About to exit, saving window geometry.");
            // save position
            state.volatile_settings.window_geometry = (
                app.window().position(),
                app.window().size(),
            );
        }
        Event::MouseWheel { delta_y, .. } => {
            trace!("Mouse wheel event");
            if !state.pointer_over_ui {
                if app.keyboard.ctrl() {
                    // Change image to next/prev
                    // - map scroll-down == next, as that's the natural scrolling direction
                    if delta_y > 0.0 {
                        prev_image(state)
                    } else {
                        next_image(state)
                    }
                } else {
                    let zoom_out: bool = delta_y < 0.0;
                    let scale: f32 = state.image_geometry.scale;
                    let new_scale: f32 = get_new_scale(
                        scale,
                        state.persistent_settings.zoom_multiplier,
                        zoom_out,
                    );

                    // limit scale
                    if new_scale > 0.01 && new_scale < 40. {
                        let scale_inc: f32 = scale - new_scale;
                        state.image_geometry.offset += scale_pt(
                            state.image_geometry.offset,
                            state.cursor,
                            scale,
                            scale_inc,
                        );
                        state.image_geometry.scale = new_scale;
                    }
                }
            }
        }

        Event::Drop(file) => {
            trace!("File drop event");
            if let Some(p) = file.path {
                if let Some(ext) = p.extension() {
                    if SUPPORTED_EXTENSIONS
                        .contains(&ext.to_string_lossy().to_string().to_lowercase().as_str())
                    {
                        state.is_loaded = false;
                        state.current_image = None;
                        state.player.load(&p);
                        state.current_path = Some(p);
                    } else {
                        state.send_message_warn("Unsupported file!");
                    }
                }
            }
        }
        Event::MouseDown { button, .. } => match button {
            MouseButton::Left => {
                if state.selection_drag != SelectionDrag::None {
                    // Do nothing, resizing will be handled in update
                } else if !state.mouse_grab {
                    state.drag_enabled = true;
                }
            }
            MouseButton::Middle => {
                state.drag_enabled = true;
            }
            MouseButton::Right => {
                if state.selection_drag != SelectionDrag::None {
                    // Do nothing, resizing will be handled in update
                } else if !state.pointer_over_ui && !state.mouse_grab {
                    let image_rect = image_rect_from_image_geometry(
                        &state.image_geometry,
                        app.window().width() as f32,
                        app.window().height() as f32,
                    );
                    if image_rect.contains(egui::pos2(state.cursor.x, state.cursor.y)) {
                        state.is_selecting = true;
                        state.selection_start_mouse_pos =
                            Some(egui::pos2(state.cursor.x, state.cursor.y));
                        // New selection, so clear the old one
                        state.selection_rect = None;
                    }
                }
            }
            _ => {}
        },
        Event::MouseUp { button, .. } => match button {
            MouseButton::Left | MouseButton::Middle => {
                state.drag_enabled = false;
                state.selection_drag = SelectionDrag::None;
            }
            MouseButton::Right => {
                state.is_selecting = false;
                state.selection_drag = SelectionDrag::None; // Reset selection_drag on mouse up
                // Clear selection if it's 1 pixel or less
                if let Some(selection_rect) = state.selection_rect {
                    if selection_rect.width() <= 1.0 || selection_rect.height() <= 1.0 {
                        state.selection_rect = None;
                    }
                }
            }
            _ => {}
        },
        _ => {
            trace!("Event: {:?}", evt);
        }
    }
}

fn update(app: &mut App, state: &mut OculanteState) {
    if state.new_image_loaded {
        if let Some(current_image) = &state.current_image {
            // If the edit state is still empty, populate it with the current image
            if state.edit_state.result_pixel_op.width() == 0 {
                state.edit_state.result_pixel_op = current_image.clone();
            }
            if state.edit_state.result_image_op.width() == 0 {
                state.edit_state.result_image_op = current_image.clone();
            }
        }
        state.new_image_loaded = false;
    }

    if state.first_start {
        app.window().set_always_on_top(false);
        state.last_window_pos = app.window().position();
        state.is_fullscreen = app.window().is_fullscreen();
    }

    // Check if window has moved and save if so
    let current_pos = app.window().position();
    if current_pos != state.last_window_pos {
        state.last_window_pos = current_pos;
        state.volatile_settings.window_geometry.0 = (current_pos.0, current_pos.1);
        _ = state.volatile_settings.save_blocking();
        trace!("Window moved, saved position.");
    }

    if let Some(p) = &state.current_path {
        let t = app.timer.elapsed_f32() % 0.8;
        if t <= 0.05 {
            trace!("chk mod {}", t);
            state.player.check_modified(p);
        }
    }

    let mouse_pos = app.mouse.position();

    state.mouse_delta = Vector2::new(mouse_pos.0, mouse_pos.1) - state.cursor;
    state.cursor = mouse_pos.size_vec();
    if state.drag_enabled && !state.mouse_grab || app.mouse.is_down(MouseButton::Middle) {
        state.image_geometry.offset += state.mouse_delta;
        limit_offset(app, state);
    }

    // Handle selection rectangle drawing and resizing
    if let Some(current_image) = &state.current_image {
        let image_rect = image_rect_from_image_geometry(
            &state.image_geometry,
            app.window().width() as f32,
            app.window().height() as f32,
        );

        if state.is_selecting {
            let start_pos = state.selection_start_mouse_pos.unwrap_or(egui::pos2(state.cursor.x, state.cursor.y));
            let end_pos = state.cursor;

            let start_x = (start_pos.x - image_rect.min.x) / state.image_geometry.scale;
            let start_y = (start_pos.y - image_rect.min.y) / state.image_geometry.scale;
            let mut end_x = (end_pos.x - image_rect.min.x) / state.image_geometry.scale;
            let mut end_y = (end_pos.y - image_rect.min.y) / state.image_geometry.scale;

            // Lock aspect ratio with CTRL or ALT
            if app.keyboard.ctrl() || app.keyboard.alt() {
                let width = (end_x - start_x).abs();
                let height = (end_y - start_y).abs();
                let size = width.max(height);
                end_x = start_x + size * (end_x - start_x).signum();
                end_y = start_y + size * (end_y - start_y).signum();

                // Clamp the selection to the image bounds while maintaining aspect ratio
                let dx = end_x - start_x;
                let dy = end_y - start_y;
                let mut scale: f32 = 1.0;

                if dx > 0.0 && end_x > current_image.width() as f32 {
                    scale = scale.min((current_image.width() as f32 - start_x) / dx);
                }
                if dx < 0.0 && end_x < 0.0 {
                    scale = scale.min(-start_x / dx);
                }
                if dy > 0.0 && end_y > current_image.height() as f32 {
                    scale = scale.min((current_image.height() as f32 - start_y) / dy);
                }
                if dy < 0.0 && end_y < 0.0 {
                    scale = scale.min(-start_y / dy);
                }

                end_x = start_x + dx * scale;
                end_y = start_y + dy * scale;
            }

            let min_x = start_x.min(end_x).max(0.0);
            let min_y = start_y.min(end_y).max(0.0);
            let max_x = start_x.max(end_x).min(current_image.width() as f32);
            let max_y = start_y.max(end_y).min(current_image.height() as f32);

            state.selection_rect = Some(egui::Rect::from_min_max(
                egui::pos2(min_x, min_y),
                egui::pos2(max_x, max_y),
            ));
        } else if (app.mouse.is_down(MouseButton::Left) || app.mouse.is_down(MouseButton::Right)) && state.selection_drag != SelectionDrag::None {
            // Resizing existing selection
            if let Some(mut selection_rect) = state.selection_rect {
                let original_aspect_ratio = if selection_rect.height() > 0.0 {
                    selection_rect.width() / selection_rect.height()
                } else {
                    1.0
                };

                let mouse_delta_x = state.mouse_delta.x / state.image_geometry.scale;
                let mouse_delta_y = state.mouse_delta.y / state.image_geometry.scale;

                // Store original state for reference
                let original_min = selection_rect.min;
                let original_max = selection_rect.max;

                match state.selection_drag {
                    SelectionDrag::Left => selection_rect.min.x += mouse_delta_x,
                    SelectionDrag::Right => selection_rect.max.x += mouse_delta_x,
                    SelectionDrag::Top => selection_rect.min.y += mouse_delta_y,
                    SelectionDrag::Bottom => selection_rect.max.y += mouse_delta_y,
                    SelectionDrag::TopLeft => {
                        selection_rect.min.x += mouse_delta_x;
                        selection_rect.min.y += mouse_delta_y;
                    }
                    SelectionDrag::TopRight => {
                        selection_rect.max.x += mouse_delta_x;
                        selection_rect.min.y += mouse_delta_y;
                    }
                    SelectionDrag::BottomLeft => {
                        selection_rect.min.x += mouse_delta_x;
                        selection_rect.max.y += mouse_delta_y;
                    }
                    SelectionDrag::BottomRight => {
                        selection_rect.max.x += mouse_delta_x;
                        selection_rect.max.y += mouse_delta_y;
                    }
                    _ => {}
                }

                // Aspect ratio correction
                if app.keyboard.alt() || app.keyboard.ctrl() {
                    let aspect_ratio = if app.keyboard.ctrl() { 1.0 } else { original_aspect_ratio };

                    // Determine which dimension's change is dominant based on the drag handle
                    let width_is_dominant = matches!(state.selection_drag, SelectionDrag::Left | SelectionDrag::Right);
                    let height_is_dominant = matches!(state.selection_drag, SelectionDrag::Top | SelectionDrag::Bottom);

                    let mut new_width = selection_rect.width();
                    let mut new_height = selection_rect.height();

                    if width_is_dominant {
                        new_height = new_width / aspect_ratio;
                    } else if height_is_dominant {
                        new_width = new_height * aspect_ratio;
                    } else { // Corner drag
                        // For corner drags, base the calculation on the distance from the anchor to the current mouse position
                        // to avoid cursor drift.
                        let anchor = match state.selection_drag {
                            SelectionDrag::TopLeft => original_max,
                            SelectionDrag::TopRight => egui::pos2(original_min.x, original_max.y),
                            SelectionDrag::BottomLeft => egui::pos2(original_max.x, original_min.y),
                            SelectionDrag::BottomRight => original_min,
                            _ => selection_rect.min, // Should not happen
                        };

                        let image_rect = image_rect_from_image_geometry(
                            &state.image_geometry,
                            app.window().width() as f32,
                            app.window().height() as f32,
                        );
                        let cursor_on_image_x = (state.cursor.x - image_rect.min.x) / state.image_geometry.scale;
                        let cursor_on_image_y = (state.cursor.y - image_rect.min.y) / state.image_geometry.scale;

                        new_width = (cursor_on_image_x - anchor.x).abs();
                        new_height = (cursor_on_image_y - anchor.y).abs();

                        if new_height > 0.0 && new_width / new_height > aspect_ratio {
                            new_height = new_width / aspect_ratio;
                        } else {
                            new_width = new_height * aspect_ratio;
                        }
                    }

                    let anchor = match state.selection_drag {
                        SelectionDrag::Right | SelectionDrag::Bottom | SelectionDrag::BottomRight => original_min,
                        SelectionDrag::Left | SelectionDrag::Top | SelectionDrag::TopLeft => original_max,
                        SelectionDrag::TopRight => egui::pos2(original_min.x, original_max.y),
                        SelectionDrag::BottomLeft => egui::pos2(original_max.x, original_min.y),
                        _ => original_min,
                    };

                    // Clamp new_width and new_height to image bounds while maintaining aspect ratio
                    let max_w = if anchor.x == original_min.x { current_image.width() as f32 - anchor.x } else { anchor.x };
                    let max_h = if anchor.y == original_min.y { current_image.height() as f32 - anchor.y } else { anchor.y };

                    if new_width > max_w {
                        let scale = max_w / new_width;
                        new_width *= scale;
                        new_height *= scale;
                    }
                    if new_height > max_h {
                        let scale = max_h / new_height;
                        new_width *= scale;
                        new_height *= scale;
                    }

                    match state.selection_drag {
                        // Edges and corners that expand from the top-left anchor
                        SelectionDrag::Right | SelectionDrag::Bottom | SelectionDrag::BottomRight => {
                            selection_rect.max.x = original_min.x + new_width;
                            selection_rect.max.y = original_min.y + new_height;
                        }
                        // Edges and corners that expand from the bottom-right anchor
                        SelectionDrag::Left | SelectionDrag::Top | SelectionDrag::TopLeft => {
                            selection_rect.min.x = original_max.x - new_width;
                            selection_rect.min.y = original_max.y - new_height;
                        }
                        // Remaining corners
                        SelectionDrag::TopRight => {
                            selection_rect.max.x = original_min.x + new_width;
                            selection_rect.min.y = original_max.y - new_height;
                        }
                        SelectionDrag::BottomLeft => {
                            selection_rect.min.x = original_max.x - new_width;
                            selection_rect.max.y = original_min.y + new_height;
                        }
                        _ => {}
                    }
                }

                // Ensure valid rectangle (min <= max)
                selection_rect.min.x = selection_rect.min.x.min(selection_rect.max.x);
                selection_rect.min.y = selection_rect.min.y.min(selection_rect.max.y);
                selection_rect.max.x = selection_rect.max.x.max(selection_rect.min.x);
                selection_rect.max.y = selection_rect.max.y.max(selection_rect.min.y);

                // Clamp to image bounds
                selection_rect.min.x = selection_rect.min.x.max(0.0);
                selection_rect.min.y = selection_rect.min.y.max(0.0);
                selection_rect.max.x = selection_rect.max.x.min(current_image.width() as f32);
                selection_rect.max.y = selection_rect.max.y.min(current_image.height() as f32);

                state.selection_rect = Some(selection_rect);
            }
        } else if let Some(selection_rect) = state.selection_rect {
            // Check for hover over edges when not actively selecting or dragging
            let screen_selection_rect = egui::Rect::from_min_max(
                egui::pos2(
                    image_rect.min.x + selection_rect.min.x * state.image_geometry.scale,
                    image_rect.min.y + selection_rect.min.y * state.image_geometry.scale,
                ),
                egui::pos2(
                    image_rect.min.x + selection_rect.max.x * state.image_geometry.scale,
                    image_rect.min.y + selection_rect.max.y * state.image_geometry.scale,
                ),
            );
            state.selection_drag = get_resize_handle(screen_selection_rect, egui::pos2(state.cursor.x, state.cursor.y), 5.0);
        }
    }

    // Since we can't access the window in the event loop, we store it in the state
    state.window_size = app.window().size().size_vec();

    if let Some(dimensions) = state.current_image.as_ref().map(|image| image.dimensions()) {
        state.image_geometry.dimensions = dimensions;
    }

    if state.persistent_settings.info_enabled || state.edit_state.painting {
        state.cursor_relative = pos_from_coord(
            state.image_geometry.offset,
            state.cursor,
            Vector2::new(
                state.image_geometry.dimensions.0 as f32,
                state.image_geometry.dimensions.1 as f32,
            ),
            state.image_geometry.scale,
        );
    }

    // redraw if extended info is missing so we make sure it's promply displayed
    if state.persistent_settings.info_enabled && state.image_metadata.is_none() {
        app.window().request_frame();
    }

    // check extended info has been sent
    if let Ok(info) = state.extended_info_channel.1.try_recv() {
        debug!("Received extended image info for {}", info.name);
        state.image_metadata = Some(info);
        app.window().request_frame();
    }

    // check if a new message has been sent
    if let Ok(msg) = state.message_channel.1.try_recv() {
        debug!("Received message: {:?}", msg);
        match msg {
            Message::LoadError(e) => {
                state.toasts.error(e);
                state.current_image = None;
                state.is_loaded = true;
                state.current_texture.clear();
            }
            Message::Info(m) => {
                state
                    .toasts
                    .info(m)
                    .set_duration(Some(Duration::from_secs(1)));
            }
            Message::Warning(m) => {
                state.toasts.warning(m);
            }
            Message::Error(m) => {
                state.toasts.error(m);
            }
            Message::Saved(_) => {
                state.toasts.info("Saved");
            }
        }
    }
    state.first_start = false;
}

fn drawe(app: &mut App, gfx: &mut Graphics, plugins: &mut Plugins, state: &mut OculanteState) {
    // If the window is minimized, don't draw anything to prevent egui from losing its state.
    if app.window().width() == 0 {
        return;
    }
    let mut draw = gfx.create_draw();
    let mut zoom_image = gfx.create_draw();
    if let Ok(p) = state.load_channel.1.try_recv() {
        state.is_loaded = false;
        state.current_image = None;
        state.player.load(&p);
        if let Some(dir) = p.parent() {
            state.volatile_settings.last_open_directory = dir.to_path_buf();
        }
        state.current_path = Some(p);
        state.scrubber.fixed_paths = false;
        // Clear selection when a new image is loaded
        state.selection_rect = None;
        state.is_selecting = false;
        state.selection_drag = SelectionDrag::None;
        state.reset_image = true;
    }

    // check if a new loaded image has been sent
    if let Ok(frame) = state.texture_channel.1.try_recv() {
        state.is_loaded = true;

        debug!("Got frame: {}", frame);

        if matches!(
            &frame,
            Frame::AnimationStart(_) | Frame::Still(_) | Frame::ImageCollectionMember(_)
        ) {
            // Something new came in, update scrubber (index slider) and path
            if let Some(path) = &state.current_path {
                if state.scrubber.has_folder_changed(path) && !state.scrubber.fixed_paths {
                    debug!("Folder has changed, creating new scrubber");
                    state.scrubber = scrubber::Scrubber::new(path);
                    state.scrubber.wrap = state.persistent_settings.wrap_folder;
                } else {
                    let index = state
                        .scrubber
                        .entries
                        .iter()
                        .position(|p| p == path)
                        .unwrap_or_default();
                    if index < state.scrubber.entries.len() {
                        state.scrubber.index = index;
                    }
                }
            }

            if let Some(path) = &state.current_path {
                if !state.volatile_settings.recent_images.contains(path) {
                    state
                        .volatile_settings
                        .recent_images
                        .insert(0, path.clone());
                    state.volatile_settings.recent_images.truncate(12);
                }
            }
        }

        match &frame {
            Frame::Still(ref img) | Frame::ImageCollectionMember(ref img) => {
                if !state.persistent_settings.keep_view {
                    state.reset_image = true;

                    if let Some(p) = state.current_path.clone() {
                        if state.persistent_settings.max_cache != 0 {
                            state.player.cache.insert(&p, img.clone());
                        }
                    }
                }
                // always reset if first image
                if state.current_texture.get().is_none() {
                    state.reset_image = true;
                }

                if !state.persistent_settings.keep_edits {
                    state.edit_state = Default::default();
                    state.edit_state = Default::default();
                }

                // Load edit information if any
                if let Some(p) = &state.current_path {
                    if p.with_extension("oculante").is_file() {
                        if let Ok(f) = std::fs::File::open(p.with_extension("oculante")) {
                            match serde_json::from_reader::<_, EditState>(f) {
                                Ok(edit_state) => {
                                    state.send_message_info(
                                        "Edits have been loaded for this image.",
                                    );
                                    state.edit_state = edit_state;
                                    state.persistent_settings.edit_enabled = true;
                                    state.reset_image = true;
                                }
                                Err(e) => {
                                    // state.send_message_info("Edits have been loaded for this image.");
                                    warn!("{e}");

                                    if let Ok(f) = std::fs::File::open(p.with_extension("oculante"))
                                    {
                                        if let Ok(legacy_edit_state) =
                                            serde_json::from_reader::<_, LegacyEditState>(f)
                                        {
                                            warn!("Legacy edits found");
                                            state.send_message_info(
                                                "Edits have been loaded for this image.",
                                            );
                                            state.edit_state = legacy_edit_state.upgrade();
                                            state.persistent_settings.edit_enabled = true;
                                            state.reset_image = true;
                                            // Migrate config
                                            if let Ok(f) =
                                                std::fs::File::create(p.with_extension("oculante"))
                                            {
                                                _ = serde_json::to_writer_pretty(
                                                    &f,
                                                    &state.edit_state,
                                                );
                                            }
                                        }
                                    } else {
                                        state.send_message_err("Edits could not be loaded.");
                                    }
                                }
                            }
                        }
                    } else if let Some(parent) = p.parent() {
                        debug!("Looking for {}", parent.join(".oculante").display());
                        if parent.join(".oculante").is_file() {
                            debug!("is file {}", parent.join(".oculante").display());

                            if let Ok(f) = std::fs::File::open(parent.join(".oculante")) {
                                if let Ok(edit_state) = serde_json::from_reader::<_, EditState>(f) {
                                    state.send_message_info(
                                        "Directory edits have been loaded for this image.",
                                    );
                                    state.edit_state = edit_state;
                                    state.persistent_settings.edit_enabled = true;
                                    state.reset_image = true;
                                }
                            }
                        }
                    }
                }

                state.redraw = false;
                // state.image_info = None;
            }
            Frame::EditResult(_) => {
                state.redraw = false;
            }
            Frame::AnimationStart(_) => {
                state.redraw = true;
                state.reset_image = true
            }
            Frame::Animation(_, _) => {
                state.redraw = true;
            }
            Frame::CompareResult(_, geo) => {
                debug!("Received compare result");
                state.image_geometry = *geo;
                // always reset if first image
                if state.current_texture.get().is_none() {
                    state.reset_image = true;
                }

                state.redraw = false;
            }
            Frame::UpdateTexture => {}
        }

        if !matches!(frame, Frame::Animation(_, _)) {
            state.image_metadata = None;
        }

        // Deal with everything that sends an image
        match frame {
            Frame::AnimationStart(img)
            | Frame::Still(img)
            | Frame::EditResult(img)
            | Frame::CompareResult(img, _)
            | Frame::Animation(img, _)
            | Frame::ImageCollectionMember(img) => {
                debug!("Received image buffer: {:?}", img.dimensions(),);
                state.image_geometry.dimensions = img.dimensions();

                if let Err(error) =
                    state
                        .current_texture
                        .set_image(&img, gfx, &state.persistent_settings)
                {
                    state.send_message_warn(&format!("Error while displaying image: {error}"));
                }
                state.current_image = Some(img);
                state.new_image_loaded = true;


            }
            Frame::UpdateTexture => {
                // Only update the texture.

                // Prefer the edit result, if present
                if state.edit_state.result_pixel_op != Default::default() {
                    if let Err(error) = state.current_texture.set_image(
                        &state.edit_state.result_pixel_op,
                        gfx,
                        &state.persistent_settings,
                    ) {
                        state.send_message_warn(&format!("Error while displaying image: {error}"));
                    }
                } else {
                    // update from image
                    if let Some(img) = &state.current_image {
                        if let Err(error) =
                            state
                                .current_texture
                                .set_image(img, gfx, &state.persistent_settings)
                        {
                            state.send_message_warn(&format!(
                                "Error while displaying image: {error}"
                            ));
                        }
                    }
                }
            }
        }

        set_title(app, state);

        // Update the image buffer in all cases except incoming edits.
        // In those cases, we want the image to stay as it is.
        // TODO: PERF: This copies the image buffer. This should also maybe not run for animation frames
        // although it looks cool.
        send_extended_info(
            &state.current_image,
            &state.current_path,
            &state.extended_info_channel,
        );
    }

    if state.redraw {
        trace!("Force redraw");
        app.window().request_frame();
    }

    // TODO: Do we need/want a "global" checker?
    // if state.persistent_settings.show_checker_background {
    //     if let Some(checker) = &state.checker_texture {
    //         draw.pattern(checker)
    //             .blend_mode(BlendMode::ADD)
    //             .size(app.window().width() as f32, app.window().height() as f32);
    //     }
    // }
    let mut bbox_tl: egui::Pos2 = Default::default();
    let mut bbox_br: egui::Pos2 = Default::default();
    let mut info_panel_color = egui::Color32::from_gray(200);
    let egui_output = plugins.egui(|ctx| {
        state.toasts.show(ctx);

        if !state.pointer_over_ui
            && !state.mouse_grab
            && ctx.input(|r| {
                r.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            })
        {
            toggle_fullscreen(app, state);
        }

        // set info panel color dynamically
        info_panel_color = ctx.style().visuals.panel_fill;

        

        // the top menu bar
        if !state.persistent_settings.zen_mode {
            let menu_height = 36.0;
            egui::TopBottomPanel::top("menu")
                .exact_height(menu_height)
                .show_separator_line(false)
                .show(ctx, |ui| {
                    main_menu(ui, state, app, gfx);
                });

            if state.persistent_settings.show_status_bar {
                egui::TopBottomPanel::bottom("statusbar")
                    .min_height(25.0)
                    .show_separator_line(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(rect) = state.selection_rect {
                                let aspect_ratio = if rect.height() > 0.0 {
                                    rect.width() / rect.height()
                                } else {
                                    0.0
                                };
                                ui.label(format!(
                                    "Selection: {:.0}, {:.0}; {:.0} x {:.0}; {:.3}",
                                    rect.min.x,
                                    rect.min.y,
                                    rect.width(),
                                    rect.height(),
                                    aspect_ratio
                                ));
                            }

                            if state.current_image.is_some() {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(5.0);
                                    let dims = state.image_geometry.dimensions;
                                    if dims.0 > 0 {
                                        ui.label(format!("{} x {}", dims.0, dims.1));
                                    }

                                    ui.separator();

                                    let scale_percent = state.image_geometry.scale * 100.0;
                                    ui.label(format!("{:.1}%", scale_percent));
                                });
                            }
                        });
                    });
            }
        }
        if state.persistent_settings.zen_mode && state.persistent_settings.borderless {
            egui::TopBottomPanel::top("menu_zen")
                .min_height(40.)
                .default_height(40.)
                .show_separator_line(false)
                .frame(egui::containers::Frame::none())
                .show(ctx, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        drag_area(ui, state, app);
                        ui.add_space(15.);
                        draw_hamburger_menu(ui, state, app);
                    });
                });
        }

        if state.persistent_settings.show_scrub_bar {
            egui::TopBottomPanel::bottom("scrubber")
                .max_height(22.)
                .min_height(22.)
                .show(ctx, |ui| {
                    scrubber_ui(state, ui);
                });
        }

        if state.persistent_settings.edit_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
            && state.current_image.is_some()
        {
            edit_ui(app, ctx, state, gfx);
        }

        if state.persistent_settings.info_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
            && state.current_image.is_some()
        {
            (bbox_tl, bbox_br) = info_ui(ctx, state, gfx);
        }

        // if there is interaction on the ui (dragging etc)
        // we don't want zoom & pan to work, so we "grab" the pointer
        state.mouse_grab = ctx.is_using_pointer()
            || state.edit_state.painting
            || ctx.is_pointer_over_area()
            || state.edit_state.block_panning;

        state.key_grab = ctx.wants_keyboard_input();

        if state.reset_image {
            if let Some(current_image) = &state.current_image {
                let draw_area = ctx.available_rect();
                let window_size = nalgebra::Vector2::new(
                    draw_area.width().min(app.window().width() as f32),
                    draw_area.height().min(app.window().height() as f32),
                );
                let img_size = current_image.size_vec();
                state.image_geometry.scale = (window_size.x / img_size.x)
                    .min(window_size.y / img_size.y)
                    .min(1.0);
                state.image_geometry.offset =
                    window_size / 2.0 - (img_size * state.image_geometry.scale) / 2.0;
                // offset by left UI elements
                state.image_geometry.offset.x += draw_area.left();
                // offset by top UI elements
                state.image_geometry.offset.y += draw_area.top();
                debug!("Image has been reset.");
                state.reset_image = false;
                app.window().request_frame();
            }
        }

        #[cfg(not(feature = "file_open"))]
        {
            if state.file_browser_visible {
                let panel_response = egui::SidePanel::left("file_browser_panel")
                    .resizable(true)
                    .default_width(400.0)
                    .min_width(250.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(if state.file_browser_save { "Save" } else { "File Browser" });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("❌").clicked() {
                                    state.file_browser_visible = false;
                                    state.file_browser_save = false;
                                }
                            });
                        });
                        ui.separator();

                        let mut path = ui
                            .ctx()
                            .data(|r| r.get_temp::<PathBuf>(Id::new("FBPATH")))
                            .unwrap_or(filebrowser::load_recent_dir().unwrap_or_default());

                        if !state.file_browser_save {
                            filebrowser::browse(
                                &mut path,
                                SUPPORTED_EXTENSIONS,
                                &mut state.volatile_settings,
                                false, // save = false
                                |p| {
                                    let _ = state.load_channel.0.clone().send(p.to_path_buf());
                                    // Hide browser after selection
                                    state.file_browser_visible = false;
                                    state.file_browser_save = false;
                                },
                                ui,
                            );
                        } else {
                            let keys = &state.volatile_settings.encoding_options.iter().map(|e|e.ext()).collect::<Vec<_>>();
                            let key_slice = keys.iter().map(|k|k.as_str()).collect::<Vec<_>>();
                            let encoders = state.volatile_settings.encoding_options.clone();
                            filebrowser::browse(
                                &mut path,
                                key_slice.as_slice(),
                                &mut state.volatile_settings,
                                true, // save = true
                                |p| {
                                    let _ = save_with_encoding(&state.edit_state.result_pixel_op, p, &state.image_metadata, &encoders);
                                    // Hide browser after selection
                                    state.file_browser_visible = false;
                                    state.file_browser_save = false;
                                },
                                ui,
                            );
                        }

                        if ui.ctx().input(|r| r.key_pressed(Key::Escape)) {
                            state.file_browser_visible = false;
                            state.file_browser_save = false;
                        }
                        ui.ctx().data_mut(|w| w.insert_temp(Id::new("FBPATH"), path));
                    });

                if panel_response.response.rect.contains(egui::pos2(state.cursor.x, state.cursor.y)) {
                    state.mouse_grab = true;
                }
            }
        }

        // Settings come last, as they block keyboard grab (for hotkey assigment)
        settings_ui(app, ctx, state, gfx);

        state.pointer_over_ui = ctx.is_pointer_over_area();

        // Set cursor icon based on selection_drag state
        if state.selection_drag != SelectionDrag::None {
            ctx.set_cursor_icon(match state.selection_drag {
                SelectionDrag::Left | SelectionDrag::Right => egui::CursorIcon::ResizeHorizontal,
                SelectionDrag::Top | SelectionDrag::Bottom => egui::CursorIcon::ResizeVertical,
                SelectionDrag::TopLeft | SelectionDrag::BottomRight => egui::CursorIcon::ResizeNwSe,
                SelectionDrag::TopRight | SelectionDrag::BottomLeft => egui::CursorIcon::ResizeNeSw,
                _ => egui::CursorIcon::Default,
            });
        } else if state.is_selecting {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
        }
    });

    if let Some(texture) = &state.current_texture.get() {
        // align to pixel to prevent distortion
        let aligned_offset_x = state.image_geometry.offset.x.trunc();
        let aligned_offset_y = state.image_geometry.offset.y.trunc();

        if state.persistent_settings.show_checker_background {
            if let Some(checker) = &state.checker_texture {
                draw.pattern(checker)
                    .size(
                        texture.width() * state.image_geometry.scale * state.tiling as f32,
                        texture.height() * state.image_geometry.scale * state.tiling as f32,
                    )
                    .blend_mode(BlendMode::ADD)
                    .translate(aligned_offset_x, aligned_offset_y);
            }
        }
        if state.tiling < 2 {
            texture.draw_textures(
                &mut draw,
                aligned_offset_x,
                aligned_offset_y,
                state.image_geometry.scale,
            );
        } else {
            for yi in 0..state.tiling {
                for xi in 0..state.tiling {
                    //The "old" version used only a static offset, is this correct?
                    let translate_x = (xi as f32 * texture.width() * state.image_geometry.scale
                        + state.image_geometry.offset.x)
                        .trunc();
                    let translate_y = (yi as f32 * texture.height() * state.image_geometry.scale
                        + state.image_geometry.offset.y)
                        .trunc();

                    texture.draw_textures(
                        &mut draw,
                        translate_x,
                        translate_y,
                        state.image_geometry.scale,
                    );
                }
            }
        }

        if state.persistent_settings.show_frame {
            draw.rect((0.0, 0.0), texture.size())
                .stroke(1.0)
                .color(Color {
                    r: 0.5,
                    g: 0.5,
                    b: 0.5,
                    a: 0.5,
                })
                .blend_mode(BlendMode::ADD)
                .scale(state.image_geometry.scale, state.image_geometry.scale)
                .translate(aligned_offset_x, aligned_offset_y);
        }

        if state.persistent_settings.info_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
        {
            draw.rect((0., 0.), (PANEL_WIDTH + 24., state.window_size.y))
                .color(Color::from_rgb(
                    info_panel_color.r() as f32 / 255.,
                    info_panel_color.g() as f32 / 255.,
                    info_panel_color.b() as f32 / 255.,
                ));

            texture.draw_zoomed(
                &mut zoom_image,
                bbox_tl.x,
                bbox_tl.y,
                bbox_br.x - bbox_tl.x,
                (state.cursor_relative.x, state.cursor_relative.y),
                8.0,
            );
        }

        // Draw a brush preview when paint mode is on
        if state.edit_state.painting {
            if let Some(stroke) = state.edit_state.paint_strokes.last() {
                let dim = texture.width().min(texture.height()) / 50.;
                draw.circle(20.)
                    // .translate(state.cursor_relative.x, state.cursor_relative.y)
                    .alpha(0.5)
                    .stroke(1.5)
                    .scale(state.image_geometry.scale, state.image_geometry.scale)
                    .scale(stroke.width * dim, stroke.width * dim)
                    .translate(state.cursor.x, state.cursor.y);

                // For later: Maybe paint the actual brush? Maybe overkill.

                // if let Some(brush) = state.edit_state.brushes.get(stroke.brush_index) {
                //     if let Some(brush_tex) = brush.to_texture(gfx) {
                //         draw.image(&brush_tex)
                //             .blend_mode(BlendMode::NORMAL)
                //             .translate(state.cursor.x, state.cursor.y)
                //             .scale(state.scale, state.scale)
                //             .scale(stroke.width*dim, stroke.width*dim)
                //             // .translate(state.offset.x as f32, state.offset.y as f32)
                //             // .transform(state.cursor_relative)
                //             ;
                //     }
                // }
            }
        }

        // Draw selection rectangle
        if let Some(selection_rect) = state.selection_rect {
            if let Some(current_image) = &state.current_image {
                let image_rect = image_rect_from_image_geometry(
                    &state.image_geometry,
                    app.window().width() as f32,
                    app.window().height() as f32,
                );

                let screen_min_x = (image_rect.min.x + selection_rect.min.x * state.image_geometry.scale).round();
                let screen_min_y = (image_rect.min.y + selection_rect.min.y * state.image_geometry.scale).round();
                let screen_max_x = (image_rect.min.x + selection_rect.max.x * state.image_geometry.scale).round();
                let screen_max_y = (image_rect.min.y + selection_rect.max.y * state.image_geometry.scale).round();

                let border_thickness = 1.0; // Thickness of the border

                // Draw top border
                for x in (screen_min_x as i32)..(screen_max_x as i32) {
                    let img_x = ((x as f32 - image_rect.min.x) / state.image_geometry.scale) as u32;
                    let img_y = ((screen_min_y - image_rect.min.y) / state.image_geometry.scale) as u32;
                    let color = get_inverted_pixel_color(current_image, img_x, img_y);
                    draw.rect((x as f32, screen_min_y), (border_thickness, border_thickness)).color(color);
                }
                // Draw bottom border
                for x in (screen_min_x as i32)..(screen_max_x as i32) {
                    let img_x = ((x as f32 - image_rect.min.x) / state.image_geometry.scale) as u32;
                    let img_y = ((screen_max_y - border_thickness - image_rect.min.y) / state.image_geometry.scale) as u32;
                    let color = get_inverted_pixel_color(current_image, img_x, img_y);
                    draw.rect((x as f32, screen_max_y - border_thickness), (border_thickness, border_thickness)).color(color);
                }
                // Draw left border
                for y in (screen_min_y as i32)..(screen_max_y as i32) {
                    let img_x = ((screen_min_x - image_rect.min.x) / state.image_geometry.scale) as u32;
                    let img_y = ((y as f32 - image_rect.min.y) / state.image_geometry.scale) as u32;
                    let color = get_inverted_pixel_color(current_image, img_x, img_y);
                    draw.rect((screen_min_x, y as f32), (border_thickness, border_thickness)).color(color);
                }
                // Draw right border
                for y in (screen_min_y as i32)..(screen_max_y as i32) {
                    let img_x = ((screen_max_x - border_thickness - image_rect.min.x) / state.image_geometry.scale) as u32;
                    let img_y = ((y as f32 - image_rect.min.y) / state.image_geometry.scale) as u32;
                    let color = get_inverted_pixel_color(current_image, img_x, img_y);
                    draw.rect((screen_max_x - border_thickness, y as f32), (border_thickness, border_thickness)).color(color);
                }
            }
        }
    }

    if state.network_mode {
        app.window().request_frame();
    }
    // if state.edit_state.is_processing {
    //     app.window().request_frame();
    // }
    let c = state.persistent_settings.background_color;
    // draw.clear(Color:: from_bytes(c[0], c[1], c[2], 255));
    draw.clear(Color::from_rgb(
        c[0] as f32 / 255.,
        c[1] as f32 / 255.,
        c[2] as f32 / 255.,
    ));
    gfx.render(&draw);
    gfx.render(&zoom_image);
    gfx.render(&egui_output);
}



// Make sure offset is restricted to window size so we don't offset to infinity
fn limit_offset(app: &mut App, state: &mut OculanteState) {
    let window_size = app.window().size();
    let scaled_image_size = (
        state.image_geometry.dimensions.0 as f32 * state.image_geometry.scale,
        state.image_geometry.dimensions.1 as f32 * state.image_geometry.scale,
    );
    state.image_geometry.offset.x = state
        .image_geometry
        .offset
        .x
        .min(window_size.0 as f32)
        .max(-scaled_image_size.0);
    state.image_geometry.offset.y = state
        .image_geometry
        .offset
        .y
        .min(window_size.1 as f32)
        .max(-scaled_image_size.1);
}

// Handle [`CompareNext`] events
fn compare_next(_app: &mut App, state: &mut OculanteState) {
    if let Some(CompareItem { path, geometry }) = state.compare_list.next() {
        state.is_loaded = false;
        state.current_image = None;
        state.player.load_advanced(
            path,
            Some(Frame::CompareResult(Default::default(), *geometry)),
        );
        state.current_path = Some(path.to_owned());
    }
}

// Parse piped file names from stdin.
fn piped_paths(args: &clap::ArgMatches) -> Option<impl Iterator<Item = PathBuf>> {
    // Don't yield paths if user is piping in raw image data
    (!args.contains_id("stdin") && !stdin().is_terminal()).then(|| {
        stdin().lines().flat_map(|line| {
            line.unwrap_or_default()
                .split_whitespace()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
    })
}

fn copy_selected_region(state: &mut OculanteState, selection_rect: egui::Rect) {
    if let Some(current_image) = &state.current_image {
        let (x, y, width, height) = (
            selection_rect.min.x.round() as u32,
            selection_rect.min.y.round() as u32,
            selection_rect.width().round() as u32,
            selection_rect.height().round() as u32,
        );

        if width > 0 && height > 0 {
            let cropped_image = current_image.crop_imm(x, y, width, height);
            clipboard_copy(&cropped_image);
            state.send_message_info(&format!("Copied {}x{} region to clipboard", width, height));
        } else {
            state.send_message_warn("Selection is too small to copy.");
        }
    } else {
        state.send_message_warn("No image to copy from.");
    }
}

fn crop_to_selected_region(state: &mut OculanteState, selection_rect: egui::Rect) {
    if let Some(current_image) = &mut state.current_image {
        let (x, y, width, height) = (
            selection_rect.min.x.round() as u32,
            selection_rect.min.y.round() as u32,
            selection_rect.width().round() as u32,
            selection_rect.height().round() as u32,
        );

        if width > 0 && height > 0 {
            *current_image = current_image.crop_imm(x, y, width, height);
            state.reset_image = true;
            let cloned_image = current_image.clone();
            state.send_frame(crate::utils::Frame::new_still(cloned_image));
            state.send_message_info(&format!("Cropped image to {}x{}", width, height));
            state.selection_rect = None; // Clear selection after cropping
        } else {
            state.send_message_warn("Selection is too small to crop.");
        }
    } else {
        state.send_message_warn("No image to crop.");
    }
}