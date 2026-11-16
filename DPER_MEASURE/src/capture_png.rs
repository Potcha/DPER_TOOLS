//! Camera and optics recorded with a native PNG capture.
use std::{
    fs::File,
    io::{BufReader, BufWriter, Seek, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::ImageData;

const KEYWORD: &str = "DPER_MEASURE capture v1";

#[derive(Serialize, Deserialize)]
pub(super) struct Provenance {
    pub device: String,
    pub device_id: String,
    pub optical_setup: String,
    pub resolution: [u32; 2],
}

pub(super) fn save(path: &Path, image: &ImageData, provenance: Option<&Provenance>) -> Result<(), String> {
    let mut file = BufWriter::new(File::create(path).map_err(|error| format!("Échec photo : {error}"))?);
    let mut encoder = png::Encoder::new(&mut file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    if let Some(provenance) = provenance {
        let text = serde_json::to_string(provenance).map_err(|error| format!("Échec photo : {error}"))?;
        encoder.add_itxt_chunk(KEYWORD.into(), text).map_err(|error| format!("Échec photo : {error}"))?;
    }
    let mut writer = encoder.write_header().map_err(|error| format!("Échec photo : {error}"))?;
    writer.write_image_data(&image.rgb).map_err(|error| format!("Échec photo : {error}"))?;
    writer.finish().map_err(|error| format!("Échec photo : {error}"))?;
    file.flush().map_err(|error| format!("Échec photo : {error}"))
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
