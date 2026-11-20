//! Camera and optics recorded with a native PNG capture.
use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Seek, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::ImageData;

const KEYWORD: &str = "DPER_MEASURE capture v1";
const PROCESSING_KEYWORD: &str = "DPER_MEASURE processing v1";

#[derive(Serialize, Deserialize)]
pub(super) struct Provenance {
    pub device: String,
    pub device_id: String,
    pub optical_setup: String,
    pub resolution: [u32; 2],
}

pub(super) fn save(path: &Path, image: &ImageData, provenance: Option<&Provenance>) -> Result<(), String> {
    let metadata = provenance.map(serde_json::to_string).transpose().map_err(|error| error.to_string())?;
    let file = File::create(path).map_err(|error| format!("Échec photo : {error}"))?;
    write_png(file, image.width, image.height, &image.rgb, metadata.map(|text| (KEYWORD, text)))
}

pub(super) fn save_processed(
    path: &Path, width: u32, height: u32, rgb: &[u8], manifest: &serde_json::Value, overwrite: bool,
) -> Result<(), String> {
    let text = serde_json::to_string(manifest).map_err(|error| error.to_string())?;
    let file = if overwrite { File::create(path) } else {
        OpenOptions::new().write(true).create_new(true).open(path)
    }.map_err(|error| error.to_string())?;
    let result = write_png(file, width, height, rgb, Some((PROCESSING_KEYWORD, text)));
    if result.is_err() && !overwrite { let _ = fs::remove_file(path); }
    result
}

fn write_png(file: File, width: u32, height: u32, rgb: &[u8], metadata: Option<(&str, String)>) -> Result<(), String> {
    let mut file = BufWriter::new(file);
    let mut encoder = png::Encoder::new(&mut file, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    if let Some((keyword, text)) = metadata {
        encoder.add_itxt_chunk(keyword.into(), text).map_err(|error| format!("Échec photo : {error}"))?;
    }
    let mut writer = encoder.write_header().map_err(|error| format!("Échec photo : {error}"))?;
    writer.write_image_data(rgb).map_err(|error| format!("Échec photo : {error}"))?;
    writer.finish().map_err(|error| format!("Échec photo : {error}"))?;
    file.flush().map_err(|error| format!("Échec photo : {error}"))
}

pub(super) fn processing_sidecar(path: &Path) -> std::path::PathBuf {
    let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or("image_derivee");
    path.with_file_name(format!("{stem}.dper_measure.json"))
}

pub(super) fn read_processing(path: &Path, dimensions: [u32; 2]) -> Option<serde_json::Value> {
    if image::ImageFormat::from_path(path).ok()? != image::ImageFormat::Png { return None; }
    let file = File::open(path).ok()?;
    let reader = png::Decoder::new(BufReader::new(file)).read_info().ok()?;
    let mut chunks = reader.info().utf8_text.iter().filter(|chunk| chunk.keyword == PROCESSING_KEYWORD);
    let value: serde_json::Value = if let Some(chunk) = chunks.next() {
        if chunks.next().is_some() { return None; }
        serde_json::from_str(&chunk.get_text().ok()?).ok()?
    } else {
        let sidecar = processing_sidecar(path);
        if fs::metadata(&sidecar).ok()?.len() > 1024 * 1024 { return None; }
        let legacy: serde_json::Value = serde_json::from_slice(&fs::read(sidecar).ok()?).ok()?;
        if legacy["output_file"].as_str()? != path.file_name()?.to_str()? { return None; }
        legacy
    };
    (value["format_version"].as_u64() == Some(1)
        && value["processing"].as_str().is_some_and(|kind| !kind.is_empty())
        && value["output_dimensions"] == serde_json::json!(dimensions)
        && (value["source_image"].is_string() || value["source_images"].is_array())
        && value["calibration_carried_over"].as_bool() == Some(false)).then_some(value)
}

pub(super) fn open_image(path: &Path) -> Result<(image::RgbImage, Option<Provenance>), String> {
    let mut input = BufReader::new(File::open(path).map_err(|error| format!("Erreur d'ouverture : {error}"))?);
    let format = image::ImageFormat::from_path(path).map_err(|error| format!("Erreur d'ouverture : {error}"))?;
    let provenance = if format == image::ImageFormat::Png {
        // Inspect only the PNG header and read the pixels from the same open file.
        let provenance = read_provenance(&mut input);
        input.rewind().map_err(|error| format!("Erreur d'ouverture : {error}"))?;
        provenance
    } else {
        None
    };
    let image = image::ImageReader::with_format(input, format).decode()
        .map_err(|error| format!("Erreur d'ouverture : {error}"))?.to_rgb8();
    let provenance = provenance.filter(|value| {
        value.resolution == [image.width(), image.height()]
            && !value.device_id.trim().is_empty() && value.device_id != "imported-image"
            && !value.optical_setup.trim().is_empty()
    });
    Ok((image, provenance))
}

fn read_provenance(input: &mut BufReader<File>) -> Option<Provenance> {
    let reader = png::Decoder::new(input).read_info().ok()?;
    let mut chunks = reader.info().utf8_text.iter().filter(|chunk| chunk.keyword == KEYWORD);
    let chunk = chunks.next()?;
    if chunks.next().is_some() { return None; }
    serde_json::from_str(&chunk.get_text().ok()?).ok()
}
