use img_parts::{Bytes, DynImage, ImageEXIF};
use log::info;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;
use anyhow::Result;
use image::{imageops::colorops, Rgba, RgbaImage, DynamicImage};
use strum::Display;

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "bmp",
    "dds",
    "exr",
    "ff",
    "gif",
    "hdr",
    "ico",
    "jpeg",
    "jpg",
    "png",
    "pnm",
    "psd",
    "svg",
    "tga",
    "tif",
    "tiff",
    "webp",
    "nef",
    "cr2",
    "dng",
    "mos",
    "erf",
    "raf",
    "arw",
    "3fr",
    "ari",
    "srf",
    "sr2",
    "braw",
    "r3d",
    "icns",
    "nrw",
    "raw",
    "avif",
    "jxl",
    "ppm",
    "dcm",
    "ima",
    "qoi",
    "kra",
    // #[cfg(feature = "j2k")]
    // "jp2",
    #[cfg(feature = "heif")]
    "heif",
    #[cfg(feature = "heif")]
    "heic",
    #[cfg(feature = "heif")]
    "heifs",
    #[cfg(feature = "heif")]
    "heics",
    #[cfg(feature = "heif")]
    "avci",
    #[cfg(feature = "heif")]
    "avcs",
    #[cfg(feature = "heif")]
    "hif",
];

pub fn reveal_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("explorer")
            .arg("/select,")
            .arg(path)
            .spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = path.parent() {
            let _ = Command::new("xdg-open")
                .arg(parent)
                .spawn();
        }
    }
}

fn is_pixel_fully_transparent(p: &Rgba<u8>) -> bool {
    p.0 == [0, 0, 0, 0]
}

#[derive(Debug, Clone, Default)]
pub struct DicomData {
    pub physical_size: (f32, f32),
    pub dicom_data: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct ExtendedImageInfo {
    pub num_pixels: usize,
    pub num_transparent_pixels: usize,
    pub num_colors: usize,
    pub red_histogram: Vec<(i32, u64)>,
    pub green_histogram: Vec<(i32, u64)>,
    pub blue_histogram: Vec<(i32, u64)>,
    pub exif: HashMap<String, String>,
    pub dicom: Option<DicomData>,
    pub raw_exif: Option<Bytes>,
    pub name: String,
}

impl ExtendedImageInfo {
    pub fn with_exif(&mut self, image_path: &Path) -> Result<()> {
        self.name = image_path.to_string_lossy().to_string();
        if image_path.extension() == Some(OsStr::new("gif")) {
            return Ok(());
        }

        let input = std::fs::read(image_path)?;

        // Store original EXIF to write in in case of save event
        if let Some(d) = DynImage::from_bytes(input.clone().into())? {
            self.raw_exif = d.exif()
        }

        // User-friendly Exif in key/value form
        let mut c = Cursor::new(input);
        let exifreader = exif::Reader::new();
        let exif = exifreader.read_from_container(&mut c)?;
        // in case exif could not be set, for example for DNG or other "exotic" formats,
        // just bang in raw exif and let the writer deal with it later.
        // The good stuff is that this will be automagically preserved across formats.
        if self.raw_exif.is_none() {
            self.raw_exif = Some(exif.buf().to_vec().into());
        }
        for f in exif.fields() {
            self.exif.insert(
                f.tag.to_string(),
                f.display_value().with_unit(&exif).to_string(),
            );
        }
        Ok(())
    }

    pub fn with_dicom(&mut self, image_path: &Path) -> Result<()> {
        self.name = image_path.to_string_lossy().to_string();
        if image_path.extension() != Some(OsStr::new("dcm"))
            || image_path.extension() != Some(OsStr::new("ima"))
        {
            let obj = dicom_object::open_file(image_path)?;
            let mut dicom_data = HashMap::new();

            // WIP: Find out interesting items to display
            for name in &[
                "StudyDate",
                "ModalitiesInStudy",
                "Modality",
                "SourceType",
                "ImageType",
                "Manufacturer",
                "InstitutionName",
                "PrivateDataElement",
                "PrivateDataElementName",
                "OperatorsName",
                "ManufacturerModelName",
                "PatientName",
                "PatientBirthDate",
                "PatientAge",
                "PixelSpacing",
            ] {
                if let Ok(e) = obj.element_by_name(name) {
                    if let Ok(s) = e.to_str() {
                        info!("{name}: {s}");
                        dicom_data.insert(name.to_string(), s.to_string());
                    }
                }
            }
            self.dicom = Some(DicomData {
                physical_size: (0.0, 0.0),
                dicom_data,
            })
        }

        Ok(())
    }

    pub fn from_image(img: &RgbaImage) -> Self {
        let mut hist_r: [u64; 256] = [0; 256];
        let mut hist_g: [u64; 256] = [0; 256];
        let mut hist_b: [u64; 256] = [0; 256];

        let num_pixels = img.width() as usize * img.height() as usize;
        let mut num_transparent_pixels = 0;

        //Colors counting
        const FIXED_RGB_SIZE: usize = 24;
        const SUB_INDEX_SIZE: usize = 5;
        const MAIN_INDEX_SIZE: usize = 1 << (FIXED_RGB_SIZE - SUB_INDEX_SIZE);
        let mut color_map = vec![0u32; MAIN_INDEX_SIZE];

        for p in img.pixels() {
            if is_pixel_fully_transparent(p) {
                num_transparent_pixels += 1;
            }

            hist_r[p.0[0] as usize] += 1;
            hist_g[p.0[1] as usize] += 1;
            hist_b[p.0[2] as usize] += 1;

            //Store every existing color combination in a bit
            //Therefore we use a 24 bit index, splitted into a main and a sub index.
            let pos = u32::from_le_bytes([p.0[0], p.0[1], p.0[2], 0]);
            let pos_main = pos >> SUB_INDEX_SIZE;
            let pos_sub = pos - (pos_main << SUB_INDEX_SIZE);
            color_map[pos_main as usize] |= 1 << pos_sub;
        }

        let mut full_colors = 0u32;
        for &intensity in color_map.iter() {
            full_colors += intensity.count_ones();
        }

        let green_histogram: Vec<(i32, u64)> = hist_g
            .iter()
            .enumerate()
            .map(|(k, v)| (k as i32, *v))
            .collect();

        let red_histogram: Vec<(i32, u64)> = hist_r
            .iter()
            .enumerate()
            .map(|(k, v)| (k as i32, *v))
            .collect();

        let blue_histogram: Vec<(i32, u64)> = hist_b
            .iter()
            .enumerate()
            .map(|(k, v)| (k as i32, *v))
            .collect();

        Self {
            num_pixels,
            num_transparent_pixels,
            num_colors: full_colors as usize,
            blue_histogram,
            green_histogram,
            red_histogram,
            raw_exif: Default::default(),
            name: Default::default(),
            exif: Default::default(),
            dicom: Default::default(),
        }
    }
}

pub fn apply_color_corrections(
    buffer: &mut RgbaImage,
    brightness: f32,
    contrast: f32,
    gamma: f32,
    r: f32,
    g: f32,
    b: f32,
    saturation: f32,
) {
    if brightness != 0.0 {
        colorops::brighten_in_place(buffer, brightness as i32);
    }

    if contrast != 0.0 {
        // image crate contrast is bugged, this is a workaround
        for p in buffer.pixels_mut() {
            let f = (1.0 + contrast / 100.0).max(0.0);
            *p = Rgba([
                (((p[0] as f32 - 128.0) * f) + 128.0).clamp(0.0, 255.0) as u8,
                (((p[1] as f32 - 128.0) * f) + 128.0).clamp(0.0, 255.0) as u8,
                (((p[2] as f32 - 128.0) * f) + 128.0).clamp(0.0, 255.0) as u8,
                p[3],
            ]);
        }
    }

    if gamma != 100.0 {
        let gamma_f = gamma / 100.0;
        let inv_gamma = 1.0 / gamma_f;
        for p in buffer.pixels_mut() {
            *p = Rgba([
                ((p[0] as f32 / 255.0).powf(inv_gamma) * 255.0) as u8,
                ((p[1] as f32 / 255.0).powf(inv_gamma) * 255.0) as u8,
                ((p[2] as f32 / 255.0).powf(inv_gamma) * 255.0) as u8,
                p[3],
            ]);
        }
    }

    if r != 0.0 || g != 0.0 || b != 0.0 {
        let r_f = r / 100.0;
        let g_f = g / 100.0;
        let b_f = b / 100.0;
        for p in buffer.pixels_mut() {
            *p = Rgba([
                (p[0] as f32 * (1.0 + r_f)).clamp(0.0, 255.0) as u8,
                (p[1] as f32 * (1.0 + g_f)).clamp(0.0, 255.0) as u8,
                (p[2] as f32 * (1.0 + b_f)).clamp(0.0, 255.0) as u8,
                p[3],
            ]);
        }
    }

    // Convert -100..100 input to 0.0..2.0 factor
    // -100 -> 0.0 (grayscale)
    //    0 -> 1.0 (no change)
    //  100 -> 2.0 (double saturation)
    let saturation_f = ((saturation as f32 / 100.0) + 1.0).clamp(0.0, 2.0);
    if saturation_f != 1.0 {
        for p in buffer.pixels_mut() {
            let [r, g, b, a] = p.0;

            // 1. Normalize to 0.0 ~ 1.0
            let (r_f, g_f, b_f) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);

            // 2. Calculate luminance (using Rec.709 coefficients)
            // This is the "gray" reference point when saturation is reduced
            let luminance = 0.2126 * r_f + 0.7152 * g_f + 0.0722 * b_f;

            // 3. Amplify the difference between each channel and luminance by the saturation factor
            let new_r = (luminance + (r_f - luminance) * saturation_f).clamp(0.0, 1.0);
            let new_g = (luminance + (g_f - luminance) * saturation_f).clamp(0.0, 1.0);
            let new_b = (luminance + (b_f - luminance) * saturation_f).clamp(0.0, 1.0);

            // 4. Convert back to 0 ~ 255 and apply
            *p = Rgba([
                (new_r * 255.0) as u8,
                (new_g * 255.0) as u8,
                (new_b * 255.0) as u8,
                a,
            ]);
        }
    }
}

/// A single frame
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Display)]
pub enum Frame {
    /// A regular still frame (most common)
    Still(DynamicImage),
    /// Part of an animation. Delay in ms
    Animation(DynamicImage, u16),
    /// First frame of animation. This is necessary to reset the image and stop the player.
    AnimationStart(DynamicImage),
    /// Result of an edit operation with image
    EditResult(DynamicImage),
    /// Only update the current texture.
    UpdateTexture,
    /// A member of a custom image collection, for example when dropping many files or opening the app with more than one file as argument
    ImageCollectionMember(DynamicImage),
}

impl Frame {
    pub fn new(source: Frame) -> Frame {
        source
    }

    pub fn new_reset(buffer: DynamicImage) -> Frame {
        Frame::AnimationStart(buffer)
    }

    pub fn new_animation(buffer: DynamicImage, delay_ms: u16) -> Frame {
        Frame::Animation(buffer, delay_ms)
    }

    #[allow(dead_code)]
    pub fn new_edit(buffer: DynamicImage) -> Frame {
        Frame::EditResult(buffer)
    }

    #[allow(dead_code)]
    pub fn new_empty_edit() -> Frame {
        Frame::UpdateTexture
    }

    pub fn new_still(buffer: DynamicImage) -> Frame {
        Frame::Still(buffer)
    }

    // Convert one `Frame` variant to something else, replacing its buffer.
    // This is useful to force a certain frame type.
    pub fn transmute(self, forced_variant: Self) -> Frame {
        let mut forced_variant = forced_variant;
        match &self {
            Frame::Still(img)
            | Frame::Animation(img, _)
            | Frame::AnimationStart(img)
            | Frame::EditResult(img)
            | Frame::ImageCollectionMember(img) => match forced_variant {
                Frame::Still(ref mut image_buffer)
                | Frame::Animation(ref mut image_buffer, _)
                | Frame::AnimationStart(ref mut image_buffer)
                | Frame::EditResult(ref mut image_buffer)
                | Frame::ImageCollectionMember(ref mut image_buffer) => *image_buffer = img.clone(),
                Frame::UpdateTexture => (),
            },
            Frame::UpdateTexture => (),
        }
        forced_variant
    }

    /// Return the image buffor of a `Frame`.
    pub fn get_image(&self) -> Option<DynamicImage> {
        match self {
            Frame::AnimationStart(img)
            | Frame::Still(img)
            | Frame::EditResult(img)
            | Frame::Animation(img, _)
            | Frame::ImageCollectionMember(img) => Some(img.clone()),
            _ => None,
        }
    }
}

/// Determine if an enxtension is compatible with oculante
pub fn is_ext_compatible(fname: &Path) -> bool {
    SUPPORTED_EXTENSIONS.contains(
        &fname
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
    )
}

pub fn fit(oldvalue: f32, oldmin: f32, oldmax: f32, newmin: f32, newmax: f32) -> f32 {
    (((oldvalue - oldmin) * (newmax - newmin)) / (oldmax - oldmin)) + newmin
}
