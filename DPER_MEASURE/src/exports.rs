//! Exports de données et de géométries pour l'interopérabilité locale.

use crate::{geometry::{self, Annotation, AnnotationTarget, Measurement, Point, Tool}, reference, storage::Session};
use base64::Engine as _;
use image::ImageEncoder;

pub fn measurements_csv(session: &Session) -> Result<Vec<u8>, String> {
    let mut csv = String::from(
        "id;libellé;type;résultat;mm_par_pixel;mm_par_pixel_x;mm_par_pixel_y;incertitude_élargie_mm;capture_id;calibration_id;protocole_id;protocole_version;origine;méthode;points_px_json\r\n",
    );
    for measurement in &session.measurements {
        let points = serde_json::to_string(&measurement.points)
            .map_err(|error| format!("Sérialisation des points impossible : {error}"))?;
        let fields = [
            measurement.id.to_string(),
            measurement.label.clone(),
            measurement.kind.label().to_owned(),
            geometry::result(measurement),
            measurement.mm_per_pixel.map(|value| format!("{value:.10}")).unwrap_or_default(),
            measurement.mm_per_pixel_x.map(|value| format!("{value:.10}")).unwrap_or_default(),
            measurement.mm_per_pixel_y.map(|value| format!("{value:.10}")).unwrap_or_default(),
            measurement.expanded_uncertainty_mm.map(|value| format!("{value:.8}")).unwrap_or_default(),
            measurement.capture_id.clone(),
            measurement.calibration_id.clone().unwrap_or_default(),
            measurement.protocol_id.clone().unwrap_or_default(),
            measurement.protocol_version.map(|version| version.to_string()).unwrap_or_default(),
            measurement.origin.clone(),
            measurement.method.clone().unwrap_or_default(),
            points,
        ];
        for (index, field) in fields.iter().enumerate() {
            if index > 0 { csv.push(';'); }
            csv.push_str(&csv_escape(field));
        }
        csv.push_str("\r\n");
    }
    Ok(csv.into_bytes())
}

/// Builds a minimal XLSX workbook with one typed measurement sheet and no external Office dependency.
pub fn measurements_xlsx(session: &Session) -> Result<Vec<u8>, String> {
    let notice = xlsx_conformity_notice_for(session, None);
    build_measurements_xlsx(&[(session.image_file.as_str(), session, notice)])
}

pub fn measurements_xlsx_for_image(
    session: &Session,
    rgb: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let assessment = crate::quality::assess_image_quality_with_correction(
        rgb, width, height, &session.quality_thresholds, session.display_correction,
    ).ok();
    let notice = xlsx_conformity_notice_for(session, assessment.as_ref());
    build_measurements_xlsx(&[(session.image_file.as_str(), session, notice)])
}

pub(crate) fn measurements_xlsx_batch(
    sessions: &[(&str, &Session, Option<String>)],
) -> Result<Vec<u8>, String> {
    if sessions.is_empty() {
        return Err("Aucune image mesurée n'est sélectionnée pour le classeur XLSX du lot.".into());
    }
    build_measurements_xlsx(sessions)
}

fn build_measurements_xlsx(
    sessions: &[(&str, &Session, Option<String>)],
) -> Result<Vec<u8>, String> {
    let estimated_data_rows = sessions.iter().try_fold(0usize, |total, input| {
        let session = input.1;
        let reference_rows = session.reference_document.as_ref()
            .map(|document| document.dimensions.len()).unwrap_or_default();
        total.checked_add(session.measurements.len())?.checked_add(reference_rows)
    }).ok_or_else(|| "Le nombre de lignes XLSX dépasse la capacité mémoire.".to_owned())?;
    if estimated_data_rows > 1_048_575 {
        return Err("Le classeur dépasse la limite de lignes XLSX.".into());
    }
    let headers = [
        "Image", "Appareil", "Appareil ID", "Configuration optique", "ID mesure", "Libellé",
        "Type", "Résultat", "Échelle X (mm/px)", "Échelle Y (mm/px)", "U élargie (mm)",
        "Capture", "Calibration", "Protocole", "Version protocole", "Origine", "Méthode",
        "Points natifs JSON", "Cote de référence", "Texte source de la cote", "Page OCR",
        "Confiance OCR (%)", "Nominal (mm)", "Tolérance - (mm)", "Tolérance + (mm)",
        "Mesure référencée (mm)", "Écart (mm)", "Garde de conformité", "Statut",
    ];
    let mut rows = String::from("<row r=\"1\">");
    for (column, header) in headers.iter().enumerate() {
        rows.push_str(&xlsx_inline_cell(1, column + 1, header));
    }
    rows.push_str("</row>");

    let mut row_number = 2usize;
    for (image_name, session, conformity_notice) in sessions {
        let mut written_dimensions = vec![false; session.reference_document.as_ref()
            .map(|document| document.dimensions.len()).unwrap_or_default()];
        for measurement in &session.measurements {
            let associated = session.reference_document.as_ref().map(|document| document.dimensions.iter()
                .enumerate().filter(|(_, dimension)| dimension.measurement_id == Some(measurement.id))
                .collect::<Vec<_>>()).unwrap_or_default();
            if associated.is_empty() {
                append_xlsx_measurement_row(&mut rows, row_number, image_name, session,
                    Some(measurement), None, conformity_notice.as_deref())?;
                row_number += 1;
            } else {
                for (dimension_index, dimension) in associated {
                    written_dimensions[dimension_index] = true;
                    append_xlsx_measurement_row(&mut rows, row_number, image_name, session,
                        Some(measurement), Some(dimension), conformity_notice.as_deref())?;
                    row_number += 1;
                }
            }
        }
        if let Some(reference_document) = session.reference_document.as_ref() {
            for (dimension_index, dimension) in reference_document.dimensions.iter().enumerate() {
                if written_dimensions[dimension_index] { continue; }
                append_xlsx_measurement_row(&mut rows, row_number, image_name, session,
                    None, Some(dimension), conformity_notice.as_deref())?;
                row_number += 1;
            }
        }
    }

    let sheet = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{rows}</sheetData></worksheet>"
    );
    let entries = vec![
        ("[Content_Types].xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>".to_vec()),
        ("_rels/.rels", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>".to_vec()),
        ("xl/workbook.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"Mesures\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>".to_vec()),
        ("xl/_rels/workbook.xml.rels", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>".to_vec()),
        ("xl/worksheets/sheet1.xml", sheet.into_bytes()),
    ];
    zip_stored(entries)
}

fn append_xlsx_measurement_row(
    rows: &mut String,
    row: usize,
    image_name: &str,
    session: &Session,
    measurement: Option<&Measurement>,
    dimension: Option<&reference::ReferenceDimension>,
    conformity_notice: Option<&str>,
) -> Result<(), String> {
    rows.push_str(&format!("<row r=\"{row}\">"));
    let points = measurement.map(|measurement| serde_json::to_string(&measurement.points)
        .map_err(|error| format!("Sérialisation des points impossible : {error}")))
        .transpose()?.unwrap_or_default();
    if points.chars().count() > 32_000 {
        let measurement_id = measurement.map(|measurement| measurement.id.to_string()).unwrap_or_default();
        return Err(format!("Les points de M{measurement_id} dépassent la limite de texte d’une cellule XLSX."));
    }
    let comparison = dimension.map(|dimension| reference::compare_dimension(dimension, &session.measurements));
    let measured_mm = comparison.as_ref().and_then(|value| value.measured_mm)
        .or_else(|| measurement.and_then(reference::measurement_mm));
    let deviation = dimension.and_then(|dimension| measured_mm.map(|value| value - dimension.nominal_mm));
    let status = if conformity_notice.is_some() {
        "Verdict suspendu".to_owned()
    } else if let Some(comparison) = comparison.as_ref() {
        comparison.status.label().to_owned()
    } else if session.reference_document.is_some() {
        "Non associée".to_owned()
    } else {
        "Pas de cote de référence".to_owned()
    };
    let cells = vec![
        XlsxCell::Text(image_name.to_owned()),
        XlsxCell::Text(session.device.clone()),
        XlsxCell::Text(session.device_id.clone()),
        XlsxCell::Text(session.optical_setup.clone()),
        measurement.map(|value| XlsxCell::Text(value.id.to_string()))
            .unwrap_or_else(|| XlsxCell::Text(String::new())),
        XlsxCell::Text(measurement.map(|value| value.label.clone())
            .or_else(|| dimension.map(|value| value.label.clone())).unwrap_or_default()),
        XlsxCell::Text(measurement.map(|value| value.kind.label().to_owned()).unwrap_or_default()),
        XlsxCell::Text(measurement.map(geometry::result).unwrap_or_default()),
        optional_number(measurement.and_then(|value| value.mm_per_pixel_x.or(value.mm_per_pixel))),
        optional_number(measurement.and_then(|value| value.mm_per_pixel_y.or(value.mm_per_pixel))),
        optional_number(measurement.and_then(|value| value.expanded_uncertainty_mm)),
        XlsxCell::Text(measurement.map(|value| value.capture_id.clone()).unwrap_or_else(|| session.capture_id.clone())),
        XlsxCell::Text(measurement.and_then(|value| value.calibration_id.clone())
            .or_else(|| session.calibration.as_ref().map(|value| value.id.clone())).unwrap_or_default()),
        XlsxCell::Text(measurement.and_then(|value| value.protocol_id.clone())
            .or_else(|| session.protocol.as_ref().map(|value| value.id.clone())).unwrap_or_default()),
        measurement.and_then(|value| value.protocol_version).or_else(|| session.protocol.as_ref().map(|value| value.version))
            .map(|value| XlsxCell::Number(value.to_string())).unwrap_or_else(|| XlsxCell::Text(String::new())),
        XlsxCell::Text(measurement.map(|value| value.origin.clone()).unwrap_or_default()),
        XlsxCell::Text(measurement.and_then(|value| value.method.clone()).unwrap_or_default()),
        XlsxCell::Text(points),
        XlsxCell::Text(dimension.map(|value| value.label.clone()).unwrap_or_default()),
        XlsxCell::Text(dimension.map(|value| value.source_text.clone()).unwrap_or_default()),
        dimension.and_then(|value| value.ocr_page)
            .map(|value| XlsxCell::Number(value.to_string())).unwrap_or_else(|| XlsxCell::Text(String::new())),
        optional_number(dimension.and_then(|value| value.ocr_confidence)),
        dimension.map(|value| XlsxCell::Number(format!("{:.10}", value.nominal_mm)))
            .unwrap_or_else(|| XlsxCell::Text(String::new())),
        dimension.map(|value| XlsxCell::Number(format!("{:.10}", value.tolerance_minus_mm)))
            .unwrap_or_else(|| XlsxCell::Text(String::new())),
        dimension.map(|value| XlsxCell::Number(format!("{:.10}", value.tolerance_plus_mm)))
            .unwrap_or_else(|| XlsxCell::Text(String::new())),
        measured_mm.map(|value| XlsxCell::Number(format!("{value:.10}")))
            .unwrap_or_else(|| XlsxCell::Text(String::new())),
        deviation.map(|value| XlsxCell::Number(format!("{value:.10}")))
            .unwrap_or_else(|| XlsxCell::Text(String::new())),
        XlsxCell::Text(conformity_notice.unwrap_or_default().to_owned()),
        XlsxCell::Text(status),
    ];
    for (column, cell) in cells.into_iter().enumerate() {
        if let XlsxCell::Text(value) = &cell {
            if value.encode_utf16().count() > 32_767 {
                return Err(format!("Le texte de la cellule {}{row} dépasse la limite XLSX.", xlsx_column(column + 1)));
            }
        }
        rows.push_str(&match cell {
            XlsxCell::Text(value) => xlsx_inline_cell(row, column + 1, &value),
            XlsxCell::Number(value) => xlsx_number_cell(row, column + 1, &value),
        });
    }
    rows.push_str("</row>");
    Ok(())
}

fn xlsx_conformity_notice_for(
    session: &Session,
    assessment: Option<&crate::quality::ImageQualityAssessment>,
) -> Option<String> {
    let mut reasons = Vec::new();
    match assessment {
        Some(assessment) if assessment.is_exploitable
            && assessment.matches_context(&session.quality_thresholds, session.display_correction)
            && session.calibration.as_ref()
                .is_none_or(|profile| assessment.image_dimensions == profile.resolution) => {}
        Some(assessment) if !assessment.is_exploitable => {
            let issues = assessment.issues.iter().map(|issue| issue.label()).collect::<Vec<_>>();
            if issues.is_empty() {
                reasons.push("qualité image non exploitable selon les seuils provisoires".to_owned());
            } else {
                reasons.push(format!("qualité image non exploitable ({})", issues.join(", ")));
            }
        }
        Some(_) => reasons.push("évaluation de qualité périmée ou calculée avec d'autres paramètres".to_owned()),
        None => reasons.push("qualité image non évaluée".to_owned()),
    }
    if let Some(notice) = crate::folder_export::session_protocol_notice(session) {
        reasons.push(notice.trim_start_matches("Verdict suspendu : ").trim_end_matches('.').to_owned());
    }
    if reasons.is_empty() { None }
    else { Some(format!("Verdict suspendu : {}", reasons.join("; "))) }
}

enum XlsxCell { Text(String), Number(String) }

fn optional_number(value: Option<f64>) -> XlsxCell {
    value.filter(|number| number.is_finite())
        .map(|number| XlsxCell::Number(format!("{number:.10}")))
        .unwrap_or_else(|| XlsxCell::Text(String::new()))
}

fn xlsx_column(mut index: usize) -> String {
    let mut column = String::new();
    while index > 0 {
        index -= 1;
        column.insert(0, (b'A' + (index % 26) as u8) as char);
        index /= 26;
    }
    column
}

fn xlsx_inline_cell(row: usize, column: usize, value: &str) -> String {
    format!(
        "<c r=\"{}{row}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
        xlsx_column(column), xml_escape(value),
    )
}

fn xlsx_number_cell(row: usize, column: usize, value: &str) -> String {
    format!("<c r=\"{}{row}\"><v>{value}</v></c>", xlsx_column(column))
}

fn zip_stored(entries: Vec<(&str, Vec<u8>)>) -> Result<Vec<u8>, String> {
    let entry_count = u16::try_from(entries.len())
        .map_err(|_| "Trop d’éléments dans le classeur XLSX".to_owned())?;
    let mut output = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        if name.len() > u16::MAX as usize || data.len() > u32::MAX as usize {
            return Err("Classeur XLSX trop volumineux pour ZIP32".into());
        }
        let offset = u32::try_from(output.len())
            .map_err(|_| "Classeur XLSX trop volumineux pour ZIP32".to_owned())?;
        let name_bytes = name.as_bytes();
        let checksum = crc32(&data);
        put_zip_u32(&mut output, 0x0403_4b50);
        put_zip_u16(&mut output, 20); put_zip_u16(&mut output, 0x0800); put_zip_u16(&mut output, 0);
        put_zip_u16(&mut output, 0); put_zip_u16(&mut output, 0x21);
        put_zip_u32(&mut output, checksum); put_zip_u32(&mut output, data.len() as u32);
        put_zip_u32(&mut output, data.len() as u32); put_zip_u16(&mut output, name_bytes.len() as u16);
        put_zip_u16(&mut output, 0); output.extend_from_slice(name_bytes); output.extend_from_slice(&data);

        put_zip_u32(&mut central, 0x0201_4b50);
        put_zip_u16(&mut central, 20); put_zip_u16(&mut central, 20); put_zip_u16(&mut central, 0x0800);
        put_zip_u16(&mut central, 0); put_zip_u16(&mut central, 0); put_zip_u16(&mut central, 0x21);
        put_zip_u32(&mut central, checksum); put_zip_u32(&mut central, data.len() as u32);
        put_zip_u32(&mut central, data.len() as u32); put_zip_u16(&mut central, name_bytes.len() as u16);
        put_zip_u16(&mut central, 0); put_zip_u16(&mut central, 0); put_zip_u16(&mut central, 0);
        put_zip_u16(&mut central, 0); put_zip_u32(&mut central, 0); put_zip_u32(&mut central, offset);
        central.extend_from_slice(name_bytes);
    }
    let central_offset = u32::try_from(output.len())
        .map_err(|_| "Classeur XLSX trop volumineux pour ZIP32".to_owned())?;
    let central_size = u32::try_from(central.len())
        .map_err(|_| "Classeur XLSX trop volumineux pour ZIP32".to_owned())?;
    output.extend_from_slice(&central);
    put_zip_u32(&mut output, 0x0605_4b50); put_zip_u16(&mut output, 0); put_zip_u16(&mut output, 0);
    put_zip_u16(&mut output, entry_count); put_zip_u16(&mut output, entry_count);
    put_zip_u32(&mut output, central_size); put_zip_u32(&mut output, central_offset); put_zip_u16(&mut output, 0);
    Ok(output)
}

fn put_zip_u16(output: &mut Vec<u8>, value: u16) { output.extend_from_slice(&value.to_le_bytes()); }
fn put_zip_u32(output: &mut Vec<u8>, value: u32) { output.extend_from_slice(&value.to_le_bytes()); }

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 { crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 }; }
    }
    !crc
}

/// Encode one RGB capture as a baseline TIFF containing an OME-XML plane map.
pub fn ome_tiff(rgb: &[u8], width: u32, height: u32, session: &Session) -> Result<Vec<u8>, String> {
    let pixel_len = (width as usize).checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or_else(|| "Dimensions de l'image trop grandes pour OME-TIFF".to_owned())?;
    if width == 0 || height == 0 || rgb.len() != pixel_len {
        return Err("Pixels source invalides pour OME-TIFF".into());
    }
    let pixel_count = u32::try_from(pixel_len)
        .map_err(|_| "Image trop volumineuse pour un TIFF classique de moins de 4 Gio".to_owned())?;
    let mut physical_sizes = String::new();
    if let Some(profile) = session.calibration.as_ref().filter(|profile| profile.validation_status != "failed") {
        let scale_x = profile.scale_x_mm_per_pixel.unwrap_or(profile.mm_per_pixel);
        let scale_y = profile.scale_y_mm_per_pixel.unwrap_or(profile.mm_per_pixel);
        if scale_x.is_finite() && scale_x > 0.0 && scale_y.is_finite() && scale_y > 0.0 {
            physical_sizes = format!(
                " PhysicalSizeX=\"{scale_x:.12}\" PhysicalSizeXUnit=\"mm\" PhysicalSizeY=\"{scale_y:.12}\" PhysicalSizeYUnit=\"mm\""
            );
        }
    }
    let image_name = std::path::Path::new(&session.image_file)
        .file_name().and_then(|name| name.to_str()).unwrap_or("capture");
    let description = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><OME xmlns=\"http://www.openmicroscopy.org/Schemas/OME/2016-06\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"http://www.openmicroscopy.org/Schemas/OME/2016-06 http://www.openmicroscopy.org/Schemas/OME/2016-06/ome.xsd\"><Image ID=\"Image:0\" Name=\"{}\"><Description>DPER_MEASURE capture {}</Description><Pixels ID=\"Pixels:0\" DimensionOrder=\"XYZCT\" Type=\"uint8\" SizeX=\"{width}\" SizeY=\"{height}\" SizeZ=\"1\" SizeC=\"3\" SizeT=\"1\" Interleaved=\"true\" BigEndian=\"false\"{physical_sizes}><Channel ID=\"Channel:0:0\" Name=\"RGB\" SamplesPerPixel=\"3\"/><TiffData IFD=\"0\" PlaneCount=\"1\"/></Pixels></Image></OME>",
        xml_escape(image_name), xml_escape(&session.capture_id),
    );
    let mut description = description.into_bytes();
    description.push(0);
    let mut software = format!("DPER_MEASURE {}", env!("CARGO_PKG_VERSION")).into_bytes();
    software.push(0);
    let ifd_entry_count = 13u16;
    let ifd_end = 8usize + 2 + ifd_entry_count as usize * 12 + 4;
    let mut external = Vec::new();
    let bits_offset = append_tiff_external(&mut external, ifd_end, &[8, 0, 8, 0, 8, 0])?;
    let description_offset = append_tiff_external(&mut external, ifd_end, &description)?;
    let software_offset = append_tiff_external(&mut external, ifd_end, &software)?;
    let pixel_offset = ifd_end.checked_add(external.len())
        .ok_or_else(|| "Offsets TIFF trop grands".to_owned())?;
    let pixel_offset = u32::try_from(pixel_offset)
        .map_err(|_| "Métadonnées OME-TIFF trop volumineuses pour un TIFF classique".to_owned())?;
    let output_len = (pixel_offset as usize).checked_add(pixel_len)
        .filter(|length| *length <= u32::MAX as usize)
        .ok_or_else(|| "Image trop volumineuse pour un TIFF classique de moins de 4 Gio".to_owned())?;

    let mut entries = Vec::with_capacity(ifd_entry_count as usize);
    entries.push((254, 4, 1, 0)); // NewSubfileType
    entries.push((256, 4, 1, width));
    entries.push((257, 4, 1, height));
    entries.push((258, 3, 3, bits_offset));
    entries.push((259, 3, 1, 1)); // No compression
    entries.push((262, 3, 1, 2)); // RGB
    entries.push((270, 2, description.len() as u32, description_offset));
    entries.push((273, 4, 1, pixel_offset));
    entries.push((277, 3, 1, 3)); // SamplesPerPixel
    entries.push((278, 4, 1, height));
    entries.push((279, 4, 1, pixel_count));
    entries.push((284, 3, 1, 1)); // Chunky/interleaved samples
    entries.push((305, 2, software.len() as u32, software_offset));

    let mut output = Vec::with_capacity(output_len);
    output.extend_from_slice(b"II");
    put_tiff_u16(&mut output, 42);
    put_tiff_u32(&mut output, 8);
    put_tiff_u16(&mut output, ifd_entry_count);
    for (tag, field_type, count, value) in entries {
        put_tiff_u16(&mut output, tag);
        put_tiff_u16(&mut output, field_type);
        put_tiff_u32(&mut output, count);
        put_tiff_u32(&mut output, value);
    }
    put_tiff_u32(&mut output, 0); // No next IFD: the OME dataset has one plane.
    output.extend_from_slice(&external);
    output.resize(pixel_offset as usize, 0);
    output.extend_from_slice(rgb);
    Ok(output)
}

fn append_tiff_external(output: &mut Vec<u8>, ifd_end: usize, value: &[u8]) -> Result<u32, String> {
    if output.len() % 2 != 0 { output.push(0); }
    let offset = ifd_end.checked_add(output.len())
        .ok_or_else(|| "Offset TIFF invalide".to_owned())?;
    let offset = u32::try_from(offset)
        .map_err(|_| "Métadonnées OME-TIFF trop volumineuses pour un TIFF classique".to_owned())?;
    output.extend_from_slice(value);
    if output.len() % 2 != 0 { output.push(0); }
    Ok(offset)
}

fn put_tiff_u16(output: &mut Vec<u8>, value: u16) { output.extend_from_slice(&value.to_le_bytes()); }
fn put_tiff_u32(output: &mut Vec<u8>, value: u32) { output.extend_from_slice(&value.to_le_bytes()); }

fn csv_escape(value: &str) -> String {
    if value.chars().any(|character| matches!(character, ';' | '"' | '\r' | '\n')) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

pub fn measurement_svg(session: &Session, rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let expected = (width as usize).checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or_else(|| "Dimensions de l'image trop grandes pour le SVG".to_owned())?;
    if width == 0 || height == 0 || rgb.len() != expected {
        return Err("Pixels source invalides pour le SVG".into());
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|error| format!("Encodage de la source PNG impossible : {error}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(png);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n<image x=\"0\" y=\"0\" width=\"{width}\" height=\"{height}\" href=\"data:image/png;base64,{encoded}\"/>\n<g fill=\"none\" stroke-width=\"2\" vector-effect=\"non-scaling-stroke\">\n"
    );
    for measurement in &session.measurements {
        if measurement.points.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()) { continue; }
        let color = color_hex(measurement.id);
        svg.push_str(&format!("<g stroke=\"{color}\"><title>{}</title>\n", xml_escape(
            &format!("M{} · {} · {}", measurement.id, measurement.label, geometry::result(measurement)),
        )));
        let points = measurement.points.iter().map(|point| format!("{:.3},{:.3}", point.x, point.y))
            .collect::<Vec<_>>().join(" ");
        match measurement.kind {
            Tool::CircleFit | Tool::Ellipse | Tool::Arc => {
                if let Some(curve) = geometry::sampled_curve(measurement.kind, &measurement.points, 128) {
                    let curve = curve.iter().map(|point| format!("{:.3},{:.3}", point.x, point.y)).collect::<Vec<_>>().join(" ");
                    svg.push_str(&format!("<polyline points=\"{curve}\"/>\n"));
                }
            }
            Tool::PointLineDistance if measurement.points.len() >= 3 => {
                let [point, first, second] = [measurement.points[0], measurement.points[1], measurement.points[2]];
                let axis = Point { x: second.x - first.x, y: second.y - first.y };
                let denominator = axis.x * axis.x + axis.y * axis.y;
                let t = ((point.x - first.x) * axis.x + (point.y - first.y) * axis.y) / denominator;
                let projection = Point { x: first.x + t * axis.x, y: first.y + t * axis.y };
                svg.push_str(&format!("<polyline points=\"{:.3},{:.3} {:.3},{:.3}\"/>\n<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\"/>\n",
                    first.x, first.y, second.x, second.y, point.x, point.y, projection.x, projection.y));
            }
            Tool::ParallelDistance if measurement.points.len() >= 4 => {
                let first_mid = Point { x: (measurement.points[0].x + measurement.points[1].x) * 0.5,
                    y: (measurement.points[0].y + measurement.points[1].y) * 0.5 };
                let second_mid = Point { x: (measurement.points[2].x + measurement.points[3].x) * 0.5,
                    y: (measurement.points[2].y + measurement.points[3].y) * 0.5 };
                let axis = Point { x: measurement.points[1].x - measurement.points[0].x,
                    y: measurement.points[1].y - measurement.points[0].y };
                let length = axis.x.hypot(axis.y);
                let normal = Point { x: -axis.y / length, y: axis.x / length };
                let gap = (second_mid.x - first_mid.x) * normal.x + (second_mid.y - first_mid.y) * normal.y;
                let end = Point { x: first_mid.x + normal.x * gap, y: first_mid.y + normal.y * gap };
                svg.push_str(&format!("<polyline points=\"{:.3},{:.3} {:.3},{:.3}\"/><polyline points=\"{:.3},{:.3} {:.3},{:.3}\"/><line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\"/>\n",
                    measurement.points[0].x, measurement.points[0].y, measurement.points[1].x, measurement.points[1].y,
                    measurement.points[2].x, measurement.points[2].y, measurement.points[3].x, measurement.points[3].y,
                    first_mid.x, first_mid.y, end.x, end.y));
            }
            Tool::Coordinate if !measurement.points.is_empty() => {
                let point = measurement.points[0];
                svg.push_str(&format!("<path d=\"M {:.3} {:.3} h 12 m -6 -6 v 12\"/>\n", point.x - 6.0, point.y));
            }
            Tool::Distance if measurement.points.len() >= 2 => {
                svg.push_str(&format!("<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\"/>\n",
                    measurement.points[0].x, measurement.points[0].y,
                    measurement.points[1].x, measurement.points[1].y));
            }
            Tool::Circle if measurement.points.len() >= 2 => {
                svg.push_str(&format!("<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\"/>\n<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\"/>\n",
                    measurement.points[0].x, measurement.points[0].y,
                    measurement.points[0].dist(measurement.points[1]),
                    measurement.points[0].x, measurement.points[0].y,
                    measurement.points[1].x, measurement.points[1].y));
            }
            Tool::Rectangle if measurement.points.len() >= 2 => {
                let a = measurement.points[0];
                let b = measurement.points[1];
                svg.push_str(&format!("<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\"/>\n",
                    a.x.min(b.x), a.y.min(b.y), (a.x - b.x).abs(), (a.y - b.y).abs()));
            }
            Tool::Polygon if measurement.points.len() >= 3 => {
                svg.push_str(&format!("<polygon points=\"{points}\"/>\n"));
            }
            Tool::Polyline | Tool::Angle if measurement.points.len() >= 2 => {
                svg.push_str(&format!("<polyline points=\"{points}\"/>\n"));
            }
            _ => {}
        }
        for point in &measurement.points {
            svg.push_str(&format!("<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"3\" fill=\"{color}\" stroke=\"white\" stroke-width=\"1\"/>\n",
                point.x, point.y));
        }
        let label_comment: String = measurement.label.trim().chars().take(30).collect();
        let label_comment = if measurement.label.trim().chars().count() > 30 {
            format!("{label_comment}…")
        } else { label_comment };
        let label = if label_comment.is_empty() {
            format!("M{}", measurement.id)
        } else {
            format!("M{} · {label_comment}", measurement.id)
        };
        let preferred = measurement.label_position_native_px
            .map(|position| (position.x + 5.0, position.y + 16.0))
            .unwrap_or_else(|| (
                measurement.points.first().map(|point| point.x + 5.0).unwrap_or(5.0),
                measurement.points.first().map(|point| point.y - 5.0).unwrap_or(14.0),
            ));
        let text_width = label.chars().count() as f64 * 8.0 + 4.0;
        let label_x = preferred.0.clamp(2.0, (width as f64 - text_width - 2.0).max(2.0));
        let label_y = (preferred.1).clamp(14.0, (height as f64 - 2.0).max(14.0));
        svg.push_str(&format!("<text x=\"{label_x:.3}\" y=\"{label_y:.3}\" fill=\"{color}\" stroke=\"white\" stroke-width=\"3\" paint-order=\"stroke\" font-family=\"sans-serif\" font-size=\"14\">{}</text>\n</g>\n",
            xml_escape(&label)));
    }
    for (index, annotation) in session.annotations.iter().enumerate() {
        let Some(point) = annotation_anchor(annotation, &session.measurements) else { continue; };
        let color = color_hex(index as u64 + 1);
        svg.push_str(&format!(
            "<g id=\"annotation-{}\" fill=\"none\" stroke=\"{color}\"><title>{}</title><circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"5\" stroke-width=\"2\"/><circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"1.5\" stroke-width=\"2\"/></g>\n",
            index + 1, xml_escape(&annotation.text), point.x, point.y, point.x, point.y,
        ));
    }
    svg.push_str("</g>\n</svg>\n");
    Ok(svg.into_bytes())
}

pub fn measurement_dxf(session: &Session, image_height: u32) -> Result<Vec<u8>, String> {
    for measurement in &session.measurements {
        let valid_scale = |scale: Option<f64>| scale.is_some_and(|scale| scale.is_finite() && scale > 0.0);
        if !valid_scale(measurement.mm_per_pixel_x.or(measurement.mm_per_pixel))
            || !valid_scale(measurement.mm_per_pixel_y.or(measurement.mm_per_pixel))
        {
            return Err(format!("DXF en millimètres impossible : la mesure M{} n'est pas étalonnée.", measurement.id));
        }
    }
    let mut dxf = String::from(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1024\n9\n$INSUNITS\n70\n4\n9\n$MEASUREMENT\n70\n1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n",
    );
    for measurement in &session.measurements {
        let fallback = measurement.mm_per_pixel.filter(|value| value.is_finite() && *value > 0.0);
        let Some(scale_x) = measurement.mm_per_pixel_x.filter(|value| value.is_finite() && *value > 0.0).or(fallback) else { continue; };
        let Some(scale_y) = measurement.mm_per_pixel_y.filter(|value| value.is_finite() && *value > 0.0).or(fallback) else { continue; };
        let points = measurement.points.iter().map(|point| Point {
            x: point.x * scale_x,
            y: (image_height as f64 - point.y) * scale_y,
        }).collect::<Vec<_>>();
        if points.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()) {
            return Err(format!("Coordonnées DXF invalides pour M{}.", measurement.id));
        }
        match measurement.kind {
            Tool::Circle if measurement.points.len() >= 2 => write_scaled_circle(
                &mut dxf, measurement.id,
                dxf_point(measurement.points[0], image_height, scale_x, scale_y),
                measurement.points[0].dist(measurement.points[1]), scale_x, scale_y,
            )?,
            Tool::CircleFit => {
                if let Some(circle) = geometry::fit_circle(&measurement.points) {
                    let center = dxf_point(circle.center, image_height, scale_x, scale_y);
                    write_scaled_circle(&mut dxf, measurement.id, center, circle.radius,
                        scale_x, scale_y)?;
                }
            }
            Tool::Rectangle if points.len() >= 2 => {
                let a = points[0];
                let b = points[1];
                let corners = [
                    Point { x: a.x.min(b.x), y: a.y.min(b.y) },
                    Point { x: a.x.max(b.x), y: a.y.min(b.y) },
                    Point { x: a.x.max(b.x), y: a.y.max(b.y) },
                    Point { x: a.x.min(b.x), y: a.y.max(b.y) },
                ];
                write_polyline(&mut dxf, measurement.id, &corners, true);
            }
            Tool::Polygon if points.len() >= 3 => write_polyline(&mut dxf, measurement.id, &points, true),
            Tool::Ellipse => {
                if let Some(ellipse) = geometry::ellipse_geometry(&measurement.points) {
                    let first_axis = Point {
                        x: ellipse.first_axis.x * scale_x,
                        y: -ellipse.first_axis.y * scale_y,
                    };
                    let second_axis = Point {
                        x: ellipse.second_axis.x * scale_x,
                        y: -ellipse.second_axis.y * scale_y,
                    };
                    write_transformed_ellipse(
                        &mut dxf, measurement.id,
                        dxf_point(ellipse.center, image_height, scale_x, scale_y),
                        first_axis, second_axis,
                    )?;
                }
            }
            Tool::Arc if measurement.points.len() >= 3 => {
                if let Some(arc) = geometry::arc_geometry(&measurement.points) {
                    if (scale_x - scale_y).abs() <= scale_x.max(scale_y) * 1e-9 {
                        if arc.sweep_radians.abs() >= std::f64::consts::TAU - 1e-10 {
                            write_scaled_circle(&mut dxf, measurement.id,
                                dxf_point(arc.center, image_height, scale_x, scale_y),
                                arc.radius, scale_x, scale_y)?;
                        } else {
                            write_dxf_arc(
                                &mut dxf, measurement.id,
                                dxf_point(arc.center, image_height, scale_x, scale_y),
                                arc.radius * scale_x, arc.start_angle, arc.sweep_radians,
                            )?;
                        }
                    } else {
                        write_scaled_circle_arc(
                            &mut dxf, measurement.id,
                            dxf_point(arc.center, image_height, scale_x, scale_y),
                            arc.radius, arc.start_angle, arc.sweep_radians,
                            scale_x, scale_y,
                        )?;
                    }
                }
            }
            Tool::PointLineDistance if points.len() >= 3 => {
                write_polyline(&mut dxf, measurement.id, &points[1..3], false);
                let point = points[0];
                let start = points[1];
                let end = points[2];
                let axis = Point { x: end.x - start.x, y: end.y - start.y };
                let t = ((point.x - start.x) * axis.x + (point.y - start.y) * axis.y)
                    / (axis.x * axis.x + axis.y * axis.y);
                write_polyline(&mut dxf, measurement.id, &[point, Point { x: start.x + axis.x * t, y: start.y + axis.y * t }], false);
            }
            Tool::ParallelDistance if points.len() >= 4 => {
                write_polyline(&mut dxf, measurement.id, &points[0..2], false);
                write_polyline(&mut dxf, measurement.id, &points[2..4], false);
            }
            Tool::Coordinate if !points.is_empty() => {
                dxf.push_str(&format!("0\nPOINT\n8\nDPER_M{}\n10\n{:.8}\n20\n{:.8}\n30\n0\n", measurement.id, points[0].x, points[0].y));
            }
            Tool::ObjectCount => {
                for point in &points {
                    dxf.push_str(&format!("0\nPOINT\n8\nDPER_M{}\n10\n{:.8}\n20\n{:.8}\n30\n0\n", measurement.id, point.x, point.y));
                }
            }
            _ if points.len() >= 2 => write_polyline(&mut dxf, measurement.id, &points, false),
            _ => {}
        }
    }
    dxf.push_str("0\nENDSEC\n0\nEOF\n");
    Ok(dxf.into_bytes())
}

fn write_polyline(output: &mut String, id: u64, points: &[Point], closed: bool) {
    output.push_str(&format!(
        "0\nLWPOLYLINE\n100\nAcDbEntity\n8\nDPER_M{id}\n100\nAcDbPolyline\n90\n{}\n70\n{}\n",
        points.len(), if closed { 1 } else { 0 },
    ));
    for point in points {
        output.push_str(&format!("10\n{:.8}\n20\n{:.8}\n", point.x, point.y));
    }
}

fn dxf_point(point: Point, image_height: u32, scale_x: f64, scale_y: f64) -> Point {
    Point {
        x: point.x * scale_x,
        y: (image_height as f64 - point.y) * scale_y,
    }
}

fn write_scaled_circle(
    output: &mut String,
    id: u64,
    center: Point,
    radius_px: f64,
    scale_x: f64,
    scale_y: f64,
) -> Result<(), String> {
    if (scale_x - scale_y).abs() <= scale_x.max(scale_y) * 1e-9 {
        let radius = radius_px * scale_x;
        return write_dxf_circle(output, id, center, radius);
    }

    let (major_axis, ratio) = if scale_x >= scale_y {
        (Point { x: radius_px * scale_x, y: 0.0 }, scale_y / scale_x)
    } else {
        (Point { x: 0.0, y: radius_px * scale_y }, scale_x / scale_y)
    };
    write_dxf_ellipse(output, id, center, major_axis, ratio, 0.0, std::f64::consts::TAU)
}

fn write_transformed_ellipse(
    output: &mut String,
    id: u64,
    center: Point,
    first_axis: Point,
    second_axis: Point,
) -> Result<(), String> {
    let xx = first_axis.x * first_axis.x + second_axis.x * second_axis.x;
    let xy = first_axis.x * first_axis.y + second_axis.x * second_axis.y;
    let yy = first_axis.y * first_axis.y + second_axis.y * second_axis.y;
    let trace = xx + yy;
    let discriminant = (xx - yy).hypot(2.0 * xy);
    let major_squared = (trace + discriminant) * 0.5;
    let minor_squared = (trace - discriminant) * 0.5;
    if !major_squared.is_finite() || !minor_squared.is_finite() || minor_squared <= 0.0 {
        return Err(format!("Géométrie d’ellipse DXF invalide pour M{id}."));
    }
    let orientation = 0.5 * (2.0 * xy).atan2(xx - yy);
    let major_radius = major_squared.sqrt();
    let ratio = (minor_squared / major_squared).sqrt();
    if (1.0 - ratio).abs() <= 1e-12 {
        return write_dxf_circle(output, id, center, major_radius);
    }
    let major_axis = Point {
        x: major_radius * orientation.cos(),
        y: major_radius * orientation.sin(),
    };
    write_dxf_ellipse(output, id, center, major_axis, ratio, 0.0, std::f64::consts::TAU)
}

fn write_dxf_circle(
    output: &mut String,
    id: u64,
    center: Point,
    radius: f64,
) -> Result<(), String> {
    if !center.x.is_finite() || !center.y.is_finite() || !radius.is_finite() || radius <= 0.0 {
        return Err(format!("Géométrie de cercle DXF invalide pour M{id}."));
    }
    output.push_str(&format!(
        "0\nCIRCLE\n100\nAcDbEntity\n8\nDPER_M{id}\n100\nAcDbCircle\n10\n{:.8}\n20\n{:.8}\n30\n0\n40\n{radius:.8}\n",
        center.x, center.y,
    ));
    Ok(())
}

fn write_scaled_circle_arc(
    output: &mut String,
    id: u64,
    center: Point,
    radius_px: f64,
    image_start_angle: f64,
    image_sweep_radians: f64,
    scale_x: f64,
    scale_y: f64,
) -> Result<(), String> {
    if !image_start_angle.is_finite() || !image_sweep_radians.is_finite()
        || image_sweep_radians.abs() <= 1e-10
    {
        return Err(format!("Géométrie d’arc DXF invalide pour M{id}."));
    }
    if image_sweep_radians.abs() >= std::f64::consts::TAU - 1e-10 {
        return write_scaled_circle(output, id, center, radius_px, scale_x, scale_y);
    }
    let (major_axis, ratio, parameter_offset) = if scale_x >= scale_y {
        (Point { x: radius_px * scale_x, y: 0.0 }, scale_y / scale_x, 0.0)
    } else {
        (Point { x: 0.0, y: radius_px * scale_y },
            scale_x / scale_y, -std::f64::consts::FRAC_PI_2)
    };
    let parameter_start = -image_start_angle + parameter_offset;
    let parameter_sweep = -image_sweep_radians;
    let (forward_start, forward_sweep) = if parameter_sweep >= 0.0 {
        (parameter_start, parameter_sweep)
    } else {
        (parameter_start + parameter_sweep, -parameter_sweep)
    };
    let start_parameter = forward_start.rem_euclid(std::f64::consts::TAU);
    write_dxf_ellipse(
        output,
        id,
        center,
        major_axis,
        ratio,
        start_parameter,
        start_parameter + forward_sweep,
    )
}

fn write_dxf_ellipse(
    output: &mut String,
    id: u64,
    center: Point,
    major_axis: Point,
    ratio: f64,
    start_parameter: f64,
    end_parameter: f64,
) -> Result<(), String> {
    if !center.x.is_finite() || !center.y.is_finite()
        || !major_axis.x.is_finite() || !major_axis.y.is_finite()
        || major_axis.x.hypot(major_axis.y) <= 0.0
        || !ratio.is_finite() || ratio <= 0.0 || ratio > 1.0
        || !start_parameter.is_finite() || !end_parameter.is_finite()
        || start_parameter < 0.0 || end_parameter <= start_parameter
    {
        return Err(format!("Géométrie d’ellipse DXF invalide pour M{id}."));
    }
    output.push_str(&format!(
        "0\nELLIPSE\n100\nAcDbEntity\n8\nDPER_M{id}\n100\nAcDbEllipse\n10\n{:.8}\n20\n{:.8}\n30\n0\n11\n{:.8}\n21\n{:.8}\n31\n0\n40\n{ratio:.12}\n41\n{start_parameter:.12}\n42\n{end_parameter:.12}\n",
        center.x, center.y, major_axis.x, major_axis.y,
    ));
    Ok(())
}

fn write_dxf_arc(
    output: &mut String,
    id: u64,
    center: Point,
    radius: f64,
    image_start_angle: f64,
    image_sweep_radians: f64,
) -> Result<(), String> {
    if !center.x.is_finite() || !center.y.is_finite()
        || !radius.is_finite() || radius <= 0.0
        || !image_start_angle.is_finite() || !image_sweep_radians.is_finite()
    {
        return Err(format!("Géométrie d’arc DXF invalide pour M{id}."));
    }
    let dxf_start = -image_start_angle;
    let dxf_sweep = -image_sweep_radians;
    let (start, end) = if dxf_sweep >= 0.0 {
        (dxf_start, dxf_start + dxf_sweep)
    } else {
        (dxf_start + dxf_sweep, dxf_start)
    };
    let start_degrees = start.to_degrees().rem_euclid(360.0);
    let end_degrees = end.to_degrees().rem_euclid(360.0);
    output.push_str(&format!(
        "0\nARC\n100\nAcDbEntity\n8\nDPER_M{id}\n100\nAcDbCircle\n10\n{:.8}\n20\n{:.8}\n30\n0\n40\n{radius:.8}\n100\nAcDbArc\n50\n{start_degrees:.8}\n51\n{end_degrees:.8}\n",
        center.x, center.y,
    ));
    Ok(())
}

pub fn annotated_png(rgb: &[u8], width: u32, height: u32, session: &Session) -> Result<Vec<u8>, String> {
    let raster = image::RgbImage::from_raw(width, height, rgb.to_vec())
        .ok_or_else(|| "Pixels source invalides pour l'image annotée".to_owned())?;
    let mut image = raster;
    for measurement in &session.measurements {
        let color = color_rgb(measurement.id);
        match measurement.kind {
            Tool::ObjectCount => {
                for point in &measurement.points {
                    draw_circle(&mut image, *point, 4.0, color);
                    draw_line(&mut image,
                        Point { x: point.x - 3.0, y: point.y },
                        Point { x: point.x + 3.0, y: point.y }, color);
                    draw_line(&mut image,
                        Point { x: point.x, y: point.y - 3.0 },
                        Point { x: point.x, y: point.y + 3.0 }, color);
                }
            }
            Tool::CircleFit | Tool::Ellipse | Tool::Arc => {
                if let Some(curve) = geometry::sampled_curve(measurement.kind, &measurement.points, 128) {
                    for pair in curve.windows(2) { draw_line(&mut image, pair[0], pair[1], color); }
                }
            }
            Tool::PointLineDistance if measurement.points.len() >= 3 => {
                let point = measurement.points[0];
                let first = measurement.points[1];
                let second = measurement.points[2];
                draw_line(&mut image, first, second, color);
                let axis = Point { x: second.x - first.x, y: second.y - first.y };
                let t = ((point.x - first.x) * axis.x + (point.y - first.y) * axis.y)
                    / (axis.x * axis.x + axis.y * axis.y);
                draw_line(&mut image, point, Point { x: first.x + t * axis.x, y: first.y + t * axis.y }, color);
            }
            Tool::ParallelDistance if measurement.points.len() >= 4 => {
                draw_line(&mut image, measurement.points[0], measurement.points[1], color);
                draw_line(&mut image, measurement.points[2], measurement.points[3], color);
            }
            Tool::Coordinate if !measurement.points.is_empty() => {
                let point = measurement.points[0];
                draw_line(&mut image, Point { x: point.x - 6.0, y: point.y }, Point { x: point.x + 6.0, y: point.y }, color);
                draw_line(&mut image, Point { x: point.x, y: point.y - 6.0 }, Point { x: point.x, y: point.y + 6.0 }, color);
            }
            Tool::Circle if measurement.points.len() >= 2 => {
                let center = measurement.points[0];
                let rim = measurement.points[1];
                draw_circle(&mut image, center, center.dist(rim), color);
                draw_line(&mut image, center, rim, color);
            }
            Tool::Rectangle if measurement.points.len() >= 2 => {
                let a = measurement.points[0];
                let b = measurement.points[1];
                let corners = [
                    Point { x: a.x.min(b.x), y: a.y.min(b.y) },
                    Point { x: a.x.max(b.x), y: a.y.min(b.y) },
                    Point { x: a.x.max(b.x), y: a.y.max(b.y) },
                    Point { x: a.x.min(b.x), y: a.y.max(b.y) },
                ];
                draw_closed_path(&mut image, &corners, color);
            }
            Tool::Polygon if measurement.points.len() >= 3 => draw_closed_path(&mut image, &measurement.points, color),
            _ if measurement.points.len() >= 2 => {
                for pair in measurement.points.windows(2) { draw_line(&mut image, pair[0], pair[1], color); }
            }
            _ => {}
        }
    }
    for (index, annotation) in session.annotations.iter().enumerate() {
        let Some(point) = annotation_anchor(annotation, &session.measurements) else { continue; };
        let color = color_rgb(index as u64 + 1);
        draw_circle(&mut image, point, 5.0, color);
        draw_line(&mut image, Point { x: point.x - 3.0, y: point.y },
            Point { x: point.x + 3.0, y: point.y }, color);
        draw_line(&mut image, Point { x: point.x, y: point.y - 3.0 },
            Point { x: point.x, y: point.y + 3.0 }, color);
    }
    let mut output = Vec::new();
    image::codecs::png::PngEncoder::new(&mut output)
        .write_image(image.as_raw(), width, height, image::ExtendedColorType::Rgb8)
        .map_err(|error| format!("Encodage PNG annoté impossible : {error}"))?;
    Ok(output)
}

fn draw_closed_path(image: &mut image::RgbImage, points: &[Point], color: [u8; 3]) {
    for pair in points.windows(2) { draw_line(image, pair[0], pair[1], color); }
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        draw_line(image, *last, *first, color);
    }
}

fn draw_line(image: &mut image::RgbImage, from: Point, to: Point, color: [u8; 3]) {
    if [from.x, from.y, to.x, to.y].iter().any(|value| !value.is_finite()) { return; }
    let (mut x0, mut y0) = (from.x.round() as i64, from.y.round() as i64);
    let (x1, y1) = (to.x.round() as i64, to.y.round() as i64);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        for oy in -1..=1 { for ox in -1..=1 {
            let px = x0 + ox;
            let py = y0 + oy;
            if px >= 0 && py >= 0 && px < image.width() as i64 && py < image.height() as i64 {
                image.put_pixel(px as u32, py as u32, image::Rgb(color));
            }
        }}
        if x0 == x1 && y0 == y1 { break; }
        let twice_error = 2 * error;
        if twice_error >= dy { error += dy; x0 += sx; }
        if twice_error <= dx { error += dx; y0 += sy; }
    }
}

fn draw_circle(image: &mut image::RgbImage, center: Point, radius: f64, color: [u8; 3]) {
    if !radius.is_finite() || radius <= 0.0 { return; }
    let steps = (std::f64::consts::TAU * radius).ceil().clamp(16.0, 100_000.0) as usize;
    let mut previous = None;
    for step in 0..=steps {
        let angle = std::f64::consts::TAU * step as f64 / steps as f64;
        let point = Point { x: center.x + radius * angle.cos(), y: center.y + radius * angle.sin() };
        if let Some(last) = previous { draw_line(image, last, point, color); }
        previous = Some(point);
    }
}

fn annotation_anchor(annotation: &Annotation, measurements: &[Measurement]) -> Option<Point> {
    match &annotation.target {
        AnnotationTarget::Measurement { measurement_id } => {
            let measurement = measurements.iter().find(|measurement| measurement.id == *measurement_id)?;
            if measurement.points.is_empty() { return None; }
            let count = measurement.points.len() as f64;
            Some(Point {
                x: measurement.points.iter().map(|point| point.x).sum::<f64>() / count,
                y: measurement.points.iter().map(|point| point.y).sum::<f64>() / count,
            })
        }
        AnnotationTarget::Point { point } => Some(*point),
        AnnotationTarget::Zone { first, second } => Some(Point {
            x: (first.x + second.x) * 0.5,
            y: (first.y + second.y) * 0.5,
        }),
    }
}

fn color_rgb(id: u64) -> [u8; 3] {
    const COLORS: [[u8; 3]; 8] = [
        [208, 91, 43], [115, 83, 186], [28, 130, 126], [193, 65, 111],
        [80, 116, 54], [183, 127, 31], [56, 101, 176], [124, 85, 65],
    ];
    COLORS[id.saturating_sub(1) as usize % COLORS.len()]
}

fn color_hex(id: u64) -> String {
    let color = color_rgb(id);
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

fn xml_escape(value: &str) -> String {
    let value = value.chars().filter(|character| matches!(*character,
        '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )).collect::<String>();
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Annotation, AnnotationTarget};

    fn test_session(calibrated: bool) -> Session {
        Session {
            schema_version: crate::storage::CURRENT_SCHEMA_VERSION,
            capture_id: "capture-export".into(), image_file: "pcb.png".into(),
            device: "Caméra test".into(), device_id: "camera-id".into(),
            optical_setup: "Objectif test".into(), calibration: None, calibrations: Vec::new(),
            measurements: vec![Measurement {
                id: 1, kind: Tool::Distance,
                points: vec![Point { x: 1.0, y: 1.0 }, Point { x: 11.0, y: 1.0 }],
                label: "Piste <1> & test".into(), label_position_native_px: None,
                capture_id: "capture-export".into(),
                calibration_id: calibrated.then(|| "cal-1".into()),
                mm_per_pixel: calibrated.then_some(0.1),
                mm_per_pixel_x: None, mm_per_pixel_y: None,
                expanded_uncertainty_mm: Some(0.02), origin: "test".into(),
                method: Some("manuel".into()), protocol_id: None, protocol_version: None,
            }],
            annotations: vec![Annotation {
                id: "annotation-1".into(), text: "Vérifier <le bord>".into(),
                target: AnnotationTarget::Measurement { measurement_id: 1 },
                created_at_unix_secs: 0,
            }],
            quality_thresholds: Default::default(), quality_assessment: None,
            display_correction: Default::default(), protocol: None,
            reference_document: None, measurement_audit_history: Vec::new(),
        }
    }

    #[test]
    fn csv_and_xlsx_exports_keep_measurement_provenance_and_escape_labels() {
        let mut session = test_session(true);
        session.measurements[0].label = "Piste; \"A\"".into();
        let csv = String::from_utf8(measurements_csv(&session).unwrap()).unwrap();
        assert!(csv.starts_with("id;libellé;type;résultat;"));
        assert!(csv.contains("\"Piste; \"\"A\"\"\""));
        assert!(csv.contains("capture-export"));
        assert!(csv.contains("0.1000000000"));

        let xlsx = measurements_xlsx(&session).unwrap();
        assert!(xlsx.starts_with(b"PK\x03\x04"));
        let stored_zip_text = String::from_utf8_lossy(&xlsx);
        assert!(stored_zip_text.contains("xl/worksheets/sheet1.xml"));
        assert!(stored_zip_text.contains("Cote de référence"));
        assert!(stored_zip_text.contains("Piste; &quot;A&quot;"));
    }

    #[test]
    fn vector_exports_encode_annotations_and_require_calibration_for_dxf() {
        let session = test_session(true);
        let source = vec![120_u8; 2 * 2 * 3];

        let svg = String::from_utf8(measurement_svg(&session, &source, 2, 2).unwrap()).unwrap();
        assert!(svg.contains("viewBox=\"0 0 2 2\""));
        assert!(svg.contains("Piste &lt;1&gt; &amp; test"));
        assert!(svg.contains("Vérifier &lt;le bord&gt;"));

        let dxf = String::from_utf8(measurement_dxf(&session, 2).unwrap()).unwrap();
        assert!(dxf.contains("1\nAC1024"));
        assert!(dxf.contains("$INSUNITS\n70\n4"));
        let uncalibrated = measurement_dxf(&test_session(false), 2).unwrap_err();
        assert!(uncalibrated.contains("n'est pas étalonnée"));
    }

    #[test]
    fn ome_tiff_and_annotated_png_round_trip_through_image_decoder() {
        let session = test_session(true);
        let source = vec![12_u8, 34, 56, 78, 90, 123];

        let ome = ome_tiff(&source, 2, 1, &session).unwrap();
        let decoded = image::load_from_memory(&ome).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.as_raw(), &source);
        assert!(String::from_utf8_lossy(&ome).contains("openmicroscopy.org"));

        let annotated = annotated_png(&source, 2, 1, &session).unwrap();
        let decoded = image::load_from_memory(&annotated).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (2, 1));
    }

    #[test]
    fn image_exports_reject_mismatched_source_buffers() {
        let session = test_session(true);
        assert!(ome_tiff(&[0; 3], 2, 1, &session).is_err());
        assert!(measurement_svg(&session, &[0; 3], 2, 1).is_err());
        assert!(annotated_png(&[0; 3], 2, 1, &session).is_err());
    }
}
